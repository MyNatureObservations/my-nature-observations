//! Explicit new -> old marker publication. Never a database transition.
//! The exclusive session borrow serializes publication without another lock.
use super::*;
use crate::backup::{
    pre_restore::durability::{self, SyncPoint},
    restore::private,
};
use std::io::{self, Read, Seek, SeekFrom, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Point {
    Prepare,
    Write,
    PartialWrite,
    StagedSync,
    BeforeRename,
    AfterRename,
    SourceSync,
    DestinationSync,
    FinalSync,
    Durable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Progress {
    BeforeRename,
    MayHaveChanged,
}
#[derive(Debug)]
enum Cause {
    Decision(Error),
    Io(io::Error),
    Changed,
}
impl From<Error> for Cause {
    fn from(e: Error) -> Self {
        Self::Decision(e)
    }
}
impl From<Blocked> for Cause {
    fn from(e: Blocked) -> Self {
        Self::Decision(e.into())
    }
}
impl From<io::Error> for Cause {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Debug)]
pub(super) struct Failure {
    point: Point,
    progress: Progress,
    cause: Cause,
}

fn read_limited(file: &mut File) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    (&mut *file)
        .take(activation::LIMIT + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn unchanged(
    session: &RecoverySession<'_, '_>,
    root: &Path,
    recovery: &File,
    original: &mut File,
    bytes: &[u8],
) -> std::result::Result<(), Cause> {
    session
        .ensure_publication_phase()
        .map_err(|_| Blocked::Ownership)?;
    directory(&root.join("recovery"), false)?;
    let path = root.join("recovery").join(activation::MARKER);
    if !regular(&path)?
        || !private::same_file(recovery, &root.join("recovery"))?
        || !private::same_file(original, &path)?
        || read_limited(original)? != bytes
    {
        return Err(Cause::Changed);
    }
    Ok(())
}
pub(super) fn publish(
    session: &mut RecoverySession<'_, '_>,
    decision: Option<&Confirmation>,
    flag: Arc<AtomicBool>,
) -> std::result::Result<(), Failure> {
    publish_with(session, decision, flag, |_, _| Ok(()))
}
// One local checkpoint closure supports I/O-error and process-exit tests. It runs
// before the named I/O operation, except PartialWrite/AfterRename/Durable.
fn publish_with(
    session: &mut RecoverySession<'_, '_>,
    decision: Option<&Confirmation>,
    flag: Arc<AtomicBool>,
    hook: impl FnMut(Point, &Path) -> io::Result<()>,
) -> std::result::Result<(), Failure> {
    publish_with_barrier(session, decision, flag, hook, sync_barrier)
}
fn sync_barrier(point: Point, path: &Path, file: Option<&File>) -> io::Result<()> {
    match (point, file) {
        (Point::StagedSync, Some(file)) => durability::sync_opened(SyncPoint::VerifiedFile, file),
        (Point::FinalSync, Some(file)) => {
            durability::sync_opened(SyncPoint::FinalFileBarrier, file)
        }
        (Point::SourceSync | Point::DestinationSync, None) => {
            durability::sync(SyncPoint::Directory, path)
        }
        _ => Err(io::Error::from(io::ErrorKind::InvalidInput)),
    }
}
// The injected barrier replaces the actual I/O call, so deleting a mandatory
// barrier cannot be hidden by a passing checkpoint-only fault test.
fn publish_with_barrier(
    session: &mut RecoverySession<'_, '_>,
    decision: Option<&Confirmation>,
    flag: Arc<AtomicBool>,
    mut hook: impl FnMut(Point, &Path) -> io::Result<()>,
    mut barrier: impl FnMut(Point, &Path, Option<&File>) -> io::Result<()>,
) -> std::result::Result<(), Failure> {
    let mut point = Point::Prepare;
    let mut progress = Progress::BeforeRename;
    let mut checkpoint = |p, path: &Path| {
        point = p;
        hook(p, path)
    };
    let result = (|| -> std::result::Result<(), Cause> {
        let verified = validate(session, decision, flag.clone())?;
        let expected = &verified.0.binding.marker;
        let root = root(session)?;
        let recovery_path = root.join("recovery");
        let marker_path = recovery_path.join(activation::MARKER);
        directory(&recovery_path, false)?;
        if !regular(&marker_path)? {
            return Err(Cause::Changed);
        }
        let recovery = File::open(&recovery_path)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        let mut original = options.open(&marker_path)?;
        let original_bytes = read_limited(&mut original)?;
        let mut replacement = marker(&root)?;
        if &replacement != expected {
            return Err(Cause::Changed);
        }
        unchanged(session, &root, &recovery, &mut original, &original_bytes)?;
        replacement.target = Target::Old;
        let bytes = serde_json::to_vec(&replacement).map_err(|_| Cause::Changed)?;
        if bytes.len() as u64 > activation::LIMIT {
            return Err(Cause::Changed);
        }
        let backups = root.join("backups");
        directory(&backups, false)?;
        checkpoint(Point::Prepare, &backups)?;
        let staging = private::PrivateDir::new(&backups)?;
        let path = staging.path().join(activation::MARKER);
        checkpoint(Point::Write, &path)?;
        let mut file = private::create_file(&path)?;
        let half = bytes.len() / 2;
        file.write_all(&bytes[..half])?;
        checkpoint(Point::PartialWrite, &path)?;
        file.write_all(&bytes[half..])?;
        file.flush()?;
        checkpoint(Point::StagedSync, &path)?;
        barrier(Point::StagedSync, &path, Some(&file))?;
        checkpoint(Point::BeforeRename, &path)?;
        // No cached ValidatedDecision is sufficient here. Re-open and verify
        // current content under the same retained startup session, then close it.
        validate(session, decision, flag.clone())?;
        unchanged(session, &root, &recovery, &mut original, &original_bytes)?;
        if !regular(&path)?
            || !private::same_file(&file, &path)?
            || read_limited(&mut File::open(&path)?)? != bytes
        {
            return Err(Cause::Changed);
        }
        cancelled(&flag)?;
        // Cooperative MNO writers are excluded by the retained session/lease.
        // These checks are not a pathname CAS against an active same-user attacker.
        progress = Progress::MayHaveChanged;
        fs::rename(&path, &marker_path)?;
        checkpoint(Point::AfterRename, &marker_path)?;
        checkpoint(Point::SourceSync, staging.path())?;
        barrier(Point::SourceSync, staging.path(), None)?;
        checkpoint(Point::DestinationSync, &recovery_path)?;
        barrier(Point::DestinationSync, &recovery_path, None)?;
        checkpoint(Point::FinalSync, &marker_path)?;
        barrier(Point::FinalSync, &marker_path, Some(&file))?;
        session
            .ensure_publication_phase()
            .map_err(|_| Blocked::Ownership)?;
        directory(&recovery_path, false)?;
        if !private::same_file(&recovery, &recovery_path)?
            || !regular(&marker_path)?
            || !private::same_file(&file, &marker_path)?
            || marker(&root)? != replacement
        {
            return Err(Cause::Changed);
        }
        cancelled(&flag)?;
        checkpoint(Point::Durable, &marker_path)?;
        Ok(()) // Marker durability only. No file-swap/completion/Ready capability.
    })();
    result.map_err(|cause| Failure {
        point,
        progress,
        cause,
    })
}
#[cfg(test)]
mod tests;
