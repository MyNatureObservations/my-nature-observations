//! Verified construction of a closed previous-journal candidate. No swap or Ready.
//! Success is an observation, never a capability for a later replacement.
use super::*;
use crate::backup::{
    pre_restore::durability::{self, SyncPoint},
    restore::private,
};
use std::io::{self, Write};

#[derive(Debug, PartialEq, Eq)]
enum Point {
    Validated,
    Extracted,
    PartialWrite,
    Verified,
    BeforePublish,
    Published,
    SourceSync,
    DestinationSync,
    FinalSync,
    Durable,
}
#[derive(Debug)]
enum Error {
    Observation(Blocked),
    NotOld,
    Closed,
    Exists,
    Changed,
    Archive,
    Proof,
    Io(io::Error),
}
impl From<Blocked> for Error {
    fn from(e: Blocked) -> Self {
        Self::Observation(e)
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Debug)]
pub(super) struct Failure {
    published_or_uncertain: bool,
    cause: Error,
}
type BuildResult<T> = std::result::Result<T, Error>;

fn current(session: &RecoverySession<'_, '_>) -> BuildResult<(PathBuf, ImportV2, u32)> {
    let generation = session
        .startup_generation()
        .map_err(|_| Blocked::Ownership)?;
    let root = root(session)?;
    directory(&root.join("recovery"), false)?;
    let m = marker(&root)?;
    if m.target != Target::Old {
        return Err(Error::NotOld);
    }
    directory(&root.join("backups/activations"), true)?;
    if activation::present(&root.join("backups/activations").join(&m.operation_id))
        .map_err(|_| Blocked::Io)?
    {
        return Err(Error::Closed);
    }
    if m.old_content.0.sha256 == m.new_content.0.sha256
        && m.old_content.0.entry_count != m.new_content.0.entry_count
    {
        return Err(Blocked::InconsistentProof.into());
    }
    environment(&root, &m)?;
    Ok((root, m, generation))
}
fn read_marker_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(activation::LIMIT + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
// Same macOS no-clobber rename primitive used by pending completion. Never fall
// back to replacing an existing file or copying across filesystems.
pub(super) fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source = CString::new(source.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let destination =
            CString::new(destination.as_os_str().as_bytes()).map_err(io::Error::other)?;
        // SAFETY: both paths are live, NUL-terminated strings. RENAME_EXCL is atomic.
        if unsafe {
            libc::renameatx_np(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_EXCL,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, destination);
        Err(io::ErrorKind::Unsupported.into())
    }
}
pub(super) fn build(
    session: &mut RecoverySession<'_, '_>,
    flag: Arc<AtomicBool>,
) -> std::result::Result<(), Failure> {
    build_with(session, flag, |_, _| Ok(()))
}
fn build_with(
    session: &mut RecoverySession<'_, '_>,
    flag: Arc<AtomicBool>,
    mut hook: impl FnMut(Point, &Path) -> io::Result<()>,
) -> std::result::Result<(), Failure> {
    let mut published_or_uncertain = false;
    let result = (|| -> BuildResult<()> {
        cancelled(&flag)?;
        let (root, m, generation) = current(session)?;
        let recovery_path = root.join("recovery");
        let recovery = File::open(&recovery_path)?;
        let marker_path = recovery_path.join(activation::MARKER);
        let retained_marker = File::open(&marker_path)?;
        let marker_bytes = read_marker_bytes(&marker_path)?;
        let destination = recovery_path.join(&m.previous_candidate_file);
        if activation::present(&destination).map_err(|_| Blocked::Io)? {
            return Err(Error::Exists);
        }
        let archive_path = root.join("backups/pre-restore").join(&m.protection_file);
        if !regular(&archive_path)? {
            return Err(Error::Archive);
        }
        let retained_archive = File::open(&archive_path)?;
        let check = || -> BuildResult<()> {
            let (fresh_root, fresh, fresh_generation) = current(session)?;
            if fresh_root != root
                || fresh != m
                || fresh_generation != generation
                || !private::same_file(&recovery, &recovery_path)?
                || !private::same_file(&retained_marker, &marker_path)?
                || read_marker_bytes(&marker_path)? != marker_bytes
                || !regular(&archive_path)?
                || !private::same_file(&retained_archive, &archive_path)?
                || backup::digest_file(&archive_path).map_err(|_| Error::Archive)?
                    != m.protection_sha256
            {
                return Err(Error::Changed);
            }
            cancelled(&flag)?;
            Ok(())
        };
        check()?;
        // Reading target=old alone is not a durability proof. Establish the same
        // existing file/directory barriers before constructing any candidate.
        durability::sync_opened(SyncPoint::VerifiedFile, &retained_marker)?;
        durability::sync_opened(SyncPoint::Directory, &recovery)?;
        durability::sync_opened(SyncPoint::FinalFileBarrier, &retained_marker)?;
        hook(Point::Validated, &destination)?;
        check()?;
        let expected = proof_version(
            &m.old_content.0.sha256,
            m.old_content.0.entry_count,
            m.content_contract.version,
        );
        let backups = root.join("backups");
        directory(&backups, false)?;
        let extracted = import_protection::extract_bound_archive(
            &archive_path,
            &backups,
            &expected,
            &m.protection_sha256,
            flag.clone(),
        )
        .map_err(|_| Error::Archive)?;
        let source_path = extracted.path().join(backup::DATABASE);
        hook(Point::Extracted, &source_path)?;
        check()?;
        activation::no_sidecars(&source_path).map_err(|_| Blocked::Sidecar)?;
        let staged_path = extracted.path().join(&m.previous_candidate_file);
        // The source is an immutable, verified, positively closed SQLite backup
        // snapshot. Copying its bytes preserves SQL types and raw document text;
        // no SQLite writer, recovery or document reserialization is involved.
        let mut source = File::open(&source_path)?;
        let mut staged = private::create_file(&staged_path)?;
        let mut buffer = [0u8; 64 * 1024];
        let mut first = true;
        loop {
            cancelled(&flag)?;
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            staged.write_all(&buffer[..count])?;
            if first {
                first = false;
                hook(Point::PartialWrite, &staged_path)?;
            }
        }
        staged.flush()?;
        drop(source);
        drop(staged); // No file descriptor to this inode survives SQLite verification.
        if inspect_version(&staged_path, flag.clone(), m.content_contract.version)?
            != Some(Ok(expected.clone()))
        {
            return Err(Error::Proof);
        }
        hook(Point::Verified, &staged_path)?;
        hook(Point::BeforePublish, &staged_path)?;
        check()?;
        if activation::present(&destination).map_err(|_| Blocked::Io)? {
            return Err(Error::Exists);
        }
        // Verify again after the last fallible preparation checkpoint. inspect
        // checks sidecars first and positively closes its readonly connection.
        if inspect_version(&staged_path, flag.clone(), m.content_contract.version)?
            != Some(Ok(expected.clone()))
        {
            return Err(Error::Proof);
        }
        let file = File::open(&staged_path)?;
        durability::sync_opened(SyncPoint::VerifiedFile, &file)?;
        cancelled(&flag)?;
        published_or_uncertain = true;
        rename_exclusive(&staged_path, &destination)?;
        hook(Point::Published, &destination)?;
        hook(Point::SourceSync, extracted.path())?;
        durability::sync(SyncPoint::Directory, extracted.path())?;
        hook(Point::DestinationSync, &recovery_path)?;
        durability::sync_opened(SyncPoint::Directory, &recovery)?;
        hook(Point::FinalSync, &destination)?;
        durability::sync_opened(SyncPoint::FinalFileBarrier, &file)?;
        drop(file); // Re-open for final verification only after all retained handles close.
        check()?;
        if inspect_version(&destination, flag.clone(), m.content_contract.version)?
            != Some(Ok(expected))
        {
            return Err(Error::Proof);
        }
        hook(Point::Durable, &destination)?;
        Ok(())
    })();
    result.map_err(|cause| Failure {
        published_or_uncertain,
        cause,
    })
}
#[cfg(test)]
mod tests;
