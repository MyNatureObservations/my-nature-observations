//! Strict pending metadata and proof-owned mutations. No SQLite writes or unlock capability.
use super::*;
use crate::backup::{
    digest_file,
    journal_content::{self, VerifiedJournal},
    pre_restore::durability::{self, SyncPoint},
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

// Pending protocol v1 binds exactly schema 0 and content fingerprint v1.
// These accepted wire values do not follow current application versions.
const PENDING_FORMAT_VERSION: u32 = 1;
const PENDING_V1_SCHEMA_VERSION: u32 = 0;
const PENDING_V1_CONTENT_VERSION: u32 = 1;

const LIMIT: u64 = 16 * 1024;
const DIRECTORY: &str = "recovery";
const NAME: &str = "pending-restore.json";
#[derive(Debug)]
pub(crate) enum MarkerError {
    Invalid,
    TooLarge,
    AlreadyExists,
    Binding,
    Unsupported,
    Io(io::Error),
}
type MarkerResult<T> = std::result::Result<T, MarkerError>;
impl From<io::Error> for MarkerError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::AlreadyExists {
            Self::AlreadyExists
        } else {
            Self::Io(e)
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    platform: String,
    volume: String,
    file: String,
}
impl Identity {
    fn valid(&self) -> bool {
        matches!(self.platform.as_str(), "unix" | "windows")
            && hex(&self.volume, 16)
            && hex(&self.file, 16)
    }
    fn from_parts(volume: u64, file: u64) -> Self {
        Self {
            platform: if cfg!(unix) { "unix" } else { "windows" }.into(),
            volume: format!("{volume:016x}"),
            file: format!("{file:016x}"),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Fingerprint {
    version: u32,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingRestore {
    format: String,
    version: u32,
    restore_id: String,
    state: String,
    database_path: String,
    database_identity: Identity,
    directory_identity: Identity,
    protection_path: String,
    protection_sha256: String,
    old_content: Fingerprint,
    new_content: Fingerprint,
    schema_version: u32,
    created_at: String,
}
fn hex(s: &str, length: usize) -> bool {
    s.len() == length
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|id| {
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == s
    })
}
fn protection_name(s: &str) -> bool {
    let Some(name) = s.strip_prefix("backups/pre-restore/pre-restore-") else {
        return false;
    };
    let Some(name) = name.strip_suffix(".mno-backup") else {
        return false;
    };
    if name.len() != 52 || name.as_bytes().get(15) != Some(&b'-') {
        return false;
    }
    let (stamp, id) = name.split_at(15);
    let Ok(format) =
        time::format_description::parse_borrowed::<2>("[year][month][day]-[hour][minute][second]")
    else {
        return false;
    };
    time::PrimitiveDateTime::parse(stamp, &format).is_ok() && uuid(&id[1..])
}
impl PendingRestore {
    fn validate(&self) -> MarkerResult<()> {
        if self.format != "mno-pending-restore"
            || self.version != PENDING_FORMAT_VERSION
            || self.state != "pending"
            || !uuid(&self.restore_id)
            || self.database_path != "mno.sqlite"
            || !self.database_identity.valid()
            || !self.directory_identity.valid()
            || self.database_identity.platform != self.directory_identity.platform
            || !protection_name(&self.protection_path)
            || !hex(&self.protection_sha256, 64)
            || self.old_content.version != PENDING_V1_CONTENT_VERSION
            || !hex(&self.old_content.sha256, 64)
            || self.new_content.version != PENDING_V1_CONTENT_VERSION
            || !hex(&self.new_content.sha256, 64)
            || self.schema_version != PENDING_V1_SCHEMA_VERSION
            || time::OffsetDateTime::parse(
                &self.created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .is_err()
        {
            return Err(MarkerError::Invalid);
        }
        Ok(())
    }
}
fn parse(bytes: &[u8]) -> MarkerResult<PendingRestore> {
    if bytes.len() as u64 > LIMIT {
        return Err(MarkerError::TooLarge);
    }
    // Deriving structs (including all nested objects) rejects duplicate keys.
    let value: PendingRestore = serde_json::from_slice(bytes).map_err(|_| MarkerError::Invalid)?;
    value.validate()?;
    Ok(value)
}
fn open_plain(path: &Path, directory: bool) -> MarkerResult<File> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink()
        || (directory && !private::is_plain_directory(&m))
        || (!directory && !m.is_file())
    {
        return Err(MarkerError::Invalid);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err(MarkerError::Invalid);
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK
                | if directory { libc::O_DIRECTORY } else { 0 },
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000 | if directory { 0x02000000 } else { 0 });
        options.share_mode(1);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        if opened.dev() != m.dev() || opened.ino() != m.ino() || (!directory && opened.nlink() != 1)
        {
            return Err(MarkerError::Binding);
        }
    }
    Ok(file)
}
fn identity(file: &File) -> MarkerResult<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(Identity::from_parts(m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(MarkerError::Unsupported)
    }
}
// Borrow named metadata only: never acquire a regular fd through an untrusted P name.
// This observation does not freeze the namespace against replacement or ABA.
#[cfg(unix)]
fn retained_pending_name_matches(root: &Path, marker_file: &File) -> MarkerResult<bool> {
    retained_file_name_matches(&root.join(DIRECTORY).join(NAME), marker_file)
}
#[cfg(unix)]
fn retained_file_name_matches(path: &Path, marker_file: &File) -> MarkerResult<bool> {
    use std::os::unix::fs::MetadataExt;
    let named = fs::symlink_metadata(path)?;
    // Preserve early open_plain's Invalid category for an invalid file type,
    // without opening it. The later validator maps this to its own Binding.
    if !named.is_file() || named.file_type().is_symlink() {
        return Err(MarkerError::Invalid);
    }
    let held = marker_file.metadata()?;
    Ok(held.is_file()
        && named.nlink() == 1
        && held.nlink() == 1
        && (named.dev(), named.ino()) == (held.dev(), held.ino()))
}

fn read_file(file: &File) -> MarkerResult<Vec<u8>> {
    if file.metadata()?.len() > LIMIT {
        return Err(MarkerError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    parse(&bytes)?;
    Ok(bytes)
}
// Complete bounded observation of the original P, independent of its shared cursor.
#[cfg(unix)]
fn read_retained_pending(file: &File) -> MarkerResult<Vec<u8>> {
    use std::os::unix::fs::FileExt;
    if file.metadata()?.len() > LIMIT {
        return Err(MarkerError::TooLarge);
    }
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let mut offset = 0;
    while offset < LIMIT + 1 {
        #[cfg(all(test, target_os = "macos"))]
        retained_probe::read_checkpoint(file, offset);
        let capacity = buffer.len().min((LIMIT + 1 - offset) as usize);
        let count = match file.read_at(&mut buffer[..capacity], offset) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        bytes.extend_from_slice(&buffer[..count]);
        offset += count as u64;
    }
    // Parse BEFORE comparing expected bytes, as in the previous read_file path.
    parse(&bytes)?;
    Ok(bytes)
}

// Borrow only the genuine created/persisted P. Name metadata is an observation,
// never permission to reopen that name; a later substitution cannot redirect I/O.
fn check_retained_p(path: &Path, file: &File, expected: &[u8]) -> MarkerResult<()> {
    #[cfg(unix)]
    {
        if !retained_file_name_matches(path, file)? {
            return Err(MarkerError::Binding);
        }
        if read_retained_pending(file)? != expected {
            return Err(MarkerError::Binding);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, file, expected);
        Err(MarkerError::Unsupported)
    }
}

/// Read only; never treats malformed, unreadable, or absent data as permission to restore.
pub(crate) fn read(root: &Path) -> MarkerResult<PendingRestore> {
    let _root = open_plain(root, true)?;
    let _directory = open_plain(&root.join(DIRECTORY), true)?;
    let file = open_plain(&root.join(DIRECTORY).join(NAME), false)?;
    parse(&read_file(&file)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    FileSync,
    Published,
    DirectorySync,
    FinalSync,
    BeforeRemove,
    Removed,
    RemovalSync,
    ParentSync,
    RemovalBarrier,
}
fn barrier(
    point: Point,
    path: &Path,
    hook: &mut impl FnMut(Point) -> io::Result<()>,
) -> MarkerResult<()> {
    hook(point)?;
    let kind = match point {
        Point::FileSync => SyncPoint::VerifiedFile,
        Point::FinalSync | Point::RemovalBarrier => SyncPoint::FinalFileBarrier,
        _ => SyncPoint::Directory,
    };
    durability::sync(kind, path)?;
    Ok(())
}
// Publication's file barriers keep the original hook order but never acquire a
// regular fd by name. Even replacement AFTER the precheck cannot redirect sync.
fn retained_p_barrier(
    point: Point,
    path: &Path,
    file: &File,
    bytes: &[u8],
    hook: &mut impl FnMut(Point) -> io::Result<()>,
) -> MarkerResult<()> {
    hook(point)?;
    let kind = match point {
        Point::FileSync => SyncPoint::VerifiedFile,
        Point::FinalSync => SyncPoint::FinalFileBarrier,
        _ => return Err(MarkerError::Binding),
    };
    check_retained_p(path, file, bytes)?;
    #[cfg(all(test, target_os = "macos"))]
    retained_probe::file_checkpoint(
        retained_probe::FilePoint::BeforeSync(point),
        path,
        file,
        bytes,
    );
    durability::sync_opened(kind, file)?;
    #[cfg(all(test, target_os = "macos"))]
    retained_probe::file_checkpoint(
        retained_probe::FilePoint::AfterSync(point),
        path,
        file,
        bytes,
    );
    check_retained_p(path, file, bytes)
}
// The borrowed maintenance guard and active identity outlive this non-cloneable token.
// Drop NEVER removes a marker. Even successful finish does not unlock DatabaseState.
pub(crate) struct PendingMarker<'a, 's> {
    maintenance: &'a Maintenance<'s>,
    active: &'a ActiveDatabase,
    root: PathBuf,
    root_file: File,
    directory_file: File,
    marker_file: File,
    bytes: Vec<u8>,
    operation: uuid::Uuid,
    generation: u32,
}
impl<'s> AuthorizedRestore<'_, '_, 's> {
    pub(crate) fn publish_pending(&self) -> MarkerResult<PendingMarker<'_, 's>> {
        publish_with(self, |_| Ok(()))
    }
}
fn publish_with<'a, 's>(
    authorization: &'a AuthorizedRestore<'_, '_, 's>,
    mut hook: impl FnMut(Point) -> io::Result<()>,
) -> MarkerResult<PendingMarker<'a, 's>> {
    durability::require_supported().map_err(|_| MarkerError::Unsupported)?;
    let context = authorization.context;
    context
        .maintenance
        .ensure_usable()
        .map_err(|_| MarkerError::Binding)?;
    context
        .active
        .ensure_current()
        .map_err(|_| MarkerError::Binding)?;
    let proof = context.protection.as_ref().ok_or(MarkerError::Binding)?;
    if !proof.is_bound_to(context.maintenance, &context.active)
        || context.generation != context.maintenance.generation()
    {
        return Err(MarkerError::Binding);
    }
    let root = context
        .active
        .path()
        .parent()
        .ok_or(MarkerError::Binding)?
        .to_owned();
    if context.active.path().file_name() != Some(std::ffi::OsStr::new("mno.sqlite")) {
        return Err(MarkerError::Binding);
    }
    let root_file = open_plain(&root, true)?;
    let directory_identity = identity(&root_file)?;
    let (volume, file) = context.active.identity().components();
    let old = journal_content::fingerprint(
        &VerifiedJournal::from_candidate(proof.candidate(), Arc::new(AtomicBool::new(false)))
            .map_err(|_| MarkerError::Binding)?,
    )
    .map_err(|_| MarkerError::Binding)?;
    let new = journal_content::fingerprint(
        &VerifiedJournal::from_candidate(&context.incoming, Arc::new(AtomicBool::new(false)))
            .map_err(|_| MarkerError::Binding)?,
    )
    .map_err(|_| MarkerError::Binding)?;
    let relative = proof
        .path()
        .strip_prefix(&root)
        .map_err(|_| MarkerError::Binding)?
        .to_str()
        .ok_or(MarkerError::Binding)?
        .replace('\\', "/");
    let data = PendingRestore {
        format: "mno-pending-restore".into(),
        version: PENDING_FORMAT_VERSION,
        restore_id: context.maintenance.operation_id().to_string(),
        state: "pending".into(),
        database_path: "mno.sqlite".into(),
        database_identity: Identity::from_parts(volume, file),
        directory_identity,
        protection_path: relative,
        protection_sha256: digest_file(proof.path()).map_err(|_| MarkerError::Binding)?,
        old_content: Fingerprint {
            version: old.version,
            sha256: old.sha256,
        },
        new_content: Fingerprint {
            version: new.version,
            sha256: new.sha256,
        },
        schema_version: PENDING_V1_SCHEMA_VERSION,
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|_| MarkerError::Invalid)?,
    };
    data.validate()?;
    let bytes = serde_json::to_vec(&data).map_err(|_| MarkerError::Invalid)?;
    parse(&bytes)?;
    // Existing recovery artifacts are never reused or removed by publication.
    let directory = root.join(DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Ok(_) => {
            context.maintenance.require_recovery();
            return Err(MarkerError::AlreadyExists);
        }
        Err(e) => return Err(e.into()),
    }
    // No reset API in this step: any attempted persistent mutation latches closed.
    context.maintenance.require_recovery();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
    }
    #[cfg(not(unix))]
    {
        return Err(MarkerError::Unsupported);
    }
    let directory_file = open_plain(&directory, true)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".pending-")
        .tempfile_in(&directory)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    #[cfg(all(test, target_os = "macos"))]
    retained_probe::file_checkpoint(
        retained_probe::FilePoint::TemporaryValidation,
        temporary.path(),
        temporary.as_file(),
        &bytes,
    );
    check_retained_p(temporary.path(), temporary.as_file(), &bytes)?;
    retained_p_barrier(
        Point::FileSync,
        temporary.path(),
        temporary.as_file(),
        &bytes,
        &mut hook,
    )?;
    let marker_file = temporary
        .persist_noclobber(directory.join(NAME))
        .map_err(|e| MarkerError::from(e.error))?;
    hook(Point::Published)?;
    for parent in directory.ancestors() {
        barrier(Point::DirectorySync, parent, &mut hook)?;
    }
    retained_p_barrier(
        Point::FinalSync,
        &directory.join(NAME),
        &marker_file,
        &bytes,
        &mut hook,
    )?;
    let marker = PendingMarker {
        maintenance: context.maintenance,
        active: &context.active,
        root,
        root_file,
        directory_file,
        marker_file,
        bytes,
        operation: context.maintenance.operation_id(),
        generation: context.generation,
    };
    #[cfg(all(test, target_os = "macos"))]
    retained_probe::before_check(&marker);
    marker.check()?;
    Ok(marker)
}
impl PendingMarker<'_, '_> {
    fn check(&self) -> MarkerResult<()> {
        #[cfg(not(unix))]
        {
            Err(MarkerError::Unsupported)
        }
        #[cfg(unix)]
        {
            if self.maintenance.operation_id() != self.operation
                || self.maintenance.generation() != self.generation
            {
                return Err(MarkerError::Binding);
            }
            self.active
                .ensure_current()
                .map_err(|_| MarkerError::Binding)?;
            if identity(&self.root_file)? != identity(&open_plain(&self.root, true)?)?
                || identity(&self.directory_file)?
                    != identity(&open_plain(&self.root.join(DIRECTORY), true)?)?
                || !retained_pending_name_matches(&self.root, &self.marker_file)?
            {
                return Err(MarkerError::Binding);
            }
            if read_retained_pending(&self.marker_file)? != self.bytes {
                return Err(MarkerError::Binding);
            }
            let data = parse(&self.bytes)?;
            if data.restore_id != self.operation.to_string()
                || data.directory_identity != identity(&self.root_file)?
            {
                return Err(MarkerError::Binding);
            }
            Ok(())
        }
    }
    #[cfg(test)]
    pub(crate) fn finish(self) -> MarkerResult<()> {
        self.finish_with(|_| Ok(()))
    }
    #[cfg(test)]
    fn finish_with(self, mut hook: impl FnMut(Point) -> io::Result<()>) -> MarkerResult<()> {
        self.check()?;
        let directory = self.root.join(DIRECTORY);
        hook(Point::BeforeRemove)?;
        // Recheck after injectable boundary and immediately before removing only the fixed name.
        self.check()?;
        fs::remove_file(directory.join(NAME))?;
        hook(Point::Removed)?;
        barrier(Point::RemovalSync, &directory, &mut hook)?;
        // Keep recovery itself: startup remains fail-closed even after a late
        // cleanup error or restart. A future recovery protocol must own unlocking.
        for parent in self.root.ancestors() {
            barrier(Point::ParentSync, parent, &mut hook)?;
        }
        // Final hardware-cache barrier on a retained managed file, never the journal.
        barrier(
            Point::RemovalBarrier,
            &self.root.join(".mno-data.lock"),
            &mut hook,
        )?;
        Ok(())
    }
}
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;

pub(crate) mod classification;

// Strict metadata; production mutations require consuming verified capabilities.
pub(crate) mod completion;

// Internal ownership preparation only; no writer or admission entry point.
pub(crate) mod write_preparation;

// Diagnostics only. These callbacks do not exist in production; no reader/owner
// escapes. The publication callback runs AFTER FinalSync returns, not in barrier().
#[cfg(all(test, target_os = "macos"))]
mod retained_probe {
    use super::*;
    use std::cell::RefCell;
    type Before = Box<dyn for<'a, 's> FnOnce(&PendingMarker<'a, 's>)>;
    type Reader = Box<dyn FnMut(&File, u64)>;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum FilePoint {
        TemporaryValidation,
        BeforeSync(Point),
        AfterSync(Point),
    }
    type FileProbe = Box<dyn FnMut(FilePoint, &Path, &File, &[u8])>;
    thread_local! {
        static BEFORE: RefCell<Option<Before>> = const { RefCell::new(None) };
        static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
        static FILE: RefCell<Option<FileProbe>> = const { RefCell::new(None) };
    }
    pub(super) fn file_checkpoint(point: FilePoint, path: &Path, file: &File, bytes: &[u8]) {
        let callback = FILE.with(|slot| slot.borrow_mut().take());
        if let Some(mut callback) = callback {
            callback(point, path, file, bytes);
            FILE.with(|slot| *slot.borrow_mut() = Some(callback));
        }
    }
    pub(super) fn with_file<T>(
        callback: impl FnMut(FilePoint, &Path, &File, &[u8]) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                FILE.with(|slot| *slot.borrow_mut() = None);
            }
        }
        FILE.with(|slot| assert!(slot.borrow_mut().replace(Box::new(callback)).is_none()));
        let _reset = Reset;
        action()
    }
    pub(super) fn before_check(marker: &PendingMarker<'_, '_>) {
        let callback = BEFORE.with(|slot| slot.borrow_mut().take());
        if let Some(callback) = callback {
            callback(marker);
        }
    }
    pub(super) fn read_checkpoint(file: &File, offset: u64) {
        let callback = READER.with(|slot| slot.borrow_mut().take());
        if let Some(mut callback) = callback {
            callback(file, offset);
            READER.with(|slot| *slot.borrow_mut() = Some(callback));
        }
    }
    pub(super) fn with_before<T>(
        callback: impl for<'a, 's> FnOnce(&PendingMarker<'a, 's>) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                BEFORE.with(|slot| *slot.borrow_mut() = None);
            }
        }
        BEFORE.with(|slot| assert!(slot.borrow_mut().replace(Box::new(callback)).is_none()));
        let _reset = Reset;
        action()
    }
    pub(super) fn with_reader<T>(
        callback: impl FnMut(&File, u64) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                READER.with(|slot| *slot.borrow_mut() = None);
            }
        }
        READER.with(|slot| assert!(slot.borrow_mut().replace(Box::new(callback)).is_none()));
        let _reset = Reset;
        action()
    }
}
