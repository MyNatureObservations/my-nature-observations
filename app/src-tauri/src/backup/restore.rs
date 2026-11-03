//! Strict, read-only preparation. Does not know the active journal path.
use super::*;
use rusqlite::{config::DbConfig, limits::Limit};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
mod document;

// Pure document check only: grants no restore or database admission authority.
pub(crate) fn document_is_editable(json: &str) -> bool {
    document::validate(json).is_ok()
}
pub(super) mod private;

const OVERHEAD_LIMIT: u64 = 1024 * 1024;
const DISK_RESERVE: u64 = 16 * 1024 * 1024;
const DOCUMENT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrepareError {
    DamagedArchive,
    UnsupportedFormat,
    InvalidDatabase,
    IncompatibleSchema,
    IncompatibleDocument,
    InsufficientSpace,
    ResourceLimit,
    Cancelled,
    Io,
    CandidateChanged,
}
impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PrepareError {}
type Result<T> = std::result::Result<T, PrepareError>;

// Every destruction path must confirm SQLite-close before staging cleanup.
// Options allow ownership transfer without manually destroying any Rust field.
pub(crate) struct PreparedRestore {
    database: Option<Connection>,
    retained: Option<ClosedPreparedRestore>,
}
impl PreparedRestore {
    pub fn manifest(&self) -> &BackupManifest {
        &self.retained.as_ref().expect("prepared files")._manifest
    }
    // Borrow this exact snapshot; never reopen it by path.
    pub fn source(&self) -> Result<&Connection> {
        let database = self
            .database
            .as_ref()
            .ok_or(PrepareError::CandidateChanged)?;
        let retained = self
            .retained
            .as_ref()
            .ok_or(PrepareError::CandidateChanged)?;
        if database.is_autocommit()
            || !private::same_file(&retained._file, &retained._staging.path().join(DATABASE))
                .unwrap_or(false)
            || handle_digest(&retained._file).map_err(|_| PrepareError::CandidateChanged)?
                != retained._manifest.database_sha256
        {
            return Err(PrepareError::CandidateChanged);
        }
        Ok(database)
    }
    // Inert observation of already owned resources; no source/hash/open/dup/close.
    #[cfg(all(test, target_os = "macos"))]
    pub(in crate::backup) fn retained_test_identity(
        &self,
    ) -> io::Result<(i32, u64, u64, u64, bool)> {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let retained = self
            .retained
            .as_ref()
            .ok_or_else(|| io::Error::other("missing retained file"))?;
        let metadata = retained._file.metadata()?;
        Ok((
            retained._file.as_raw_fd(),
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            self.database.is_some(),
        ))
    }
    // Test-only inert handle identity; no new connection, file or ownership.
    #[cfg(all(test, target_os = "macos"))]
    pub(in crate::backup) fn sqlite_test_handle(&self) -> usize {
        // SAFETY: only observe the pointer of this borrowed, still-owned handle.
        unsafe { self.database.as_ref().unwrap().handle() as usize }
    }
    #[cfg(test)]
    pub(in crate::backup) fn database_path(&self) -> std::path::PathBuf {
        self.retained
            .as_ref()
            .unwrap()
            ._staging
            .path()
            .join(DATABASE)
    }
}

// No SQL accessor. Normal Drop may remove staging only after confirmed close.
pub(in crate::backup) struct ClosedPreparedRestore {
    _file: File,
    _manifest: BackupManifest,
    _staging: private::PrivateDir,
}
impl ClosedPreparedRestore {
    /// File-only observation of retained snapshot resources, not a close proof.
    /// macOS only: no SQLite, open/dup/close, or shared file-offset changes.
    #[cfg(target_os = "macos")]
    pub(in crate::backup) fn check_retained_file(&self) -> Result<()> {
        use std::{
            fs,
            os::unix::{fs::FileExt, fs::MetadataExt},
        };
        let path = self._staging.path().join(DATABASE);
        let check_metadata = || -> Result<()> {
            let directory = fs::symlink_metadata(self._staging.path())
                .map_err(|_| PrepareError::CandidateChanged)?;
            let held = self
                ._file
                .metadata()
                .map_err(|_| PrepareError::CandidateChanged)?;
            let named = fs::symlink_metadata(&path).map_err(|_| PrepareError::CandidateChanged)?;
            if !directory.is_dir()
                || directory.file_type().is_symlink()
                || !held.is_file()
                || !named.is_file()
                || named.file_type().is_symlink()
                || held.nlink() != 1
                || named.nlink() != 1
                || held.len() != self._manifest.database_size
                || named.len() != self._manifest.database_size
                || !private::same_file(&self._file, &path)
                    .map_err(|_| PrepareError::CandidateChanged)?
            {
                return Err(PrepareError::CandidateChanged);
            }
            Ok(())
        };
        check_metadata()?;
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0; 64 * 1024];
        while offset < self._manifest.database_size {
            let count = (self._manifest.database_size - offset).min(buffer.len() as u64) as usize;
            self._file
                .read_exact_at(&mut buffer[..count], offset)
                .map_err(|_| PrepareError::CandidateChanged)?;
            hash.update(&buffer[..count]);
            offset += count as u64;
        }
        if format!("{:x}", hash.finalize()) != self._manifest.database_sha256 {
            return Err(PrepareError::CandidateChanged);
        }
        check_metadata()
    }
}

#[cfg(test)]
impl ClosedPreparedRestore {
    pub(in crate::backup) fn retained_staging_path(&self) -> &Path {
        self._staging.path()
    }
}
pub(in crate::backup) struct PreparedCloseFailure {
    pub(in crate::backup) error: rusqlite::Error,
    // Ordinary Drop of this owner uses the same safe close path. If an
    // outstanding statement has since been finalized, cleanup need not leak.
    _owner: Box<PreparedRestore>,
}
impl PreparedCloseFailure {
    #[cfg(all(test, target_os = "macos"))]
    pub(in crate::backup) fn retained_test_identity(
        &self,
    ) -> io::Result<((i32, u64, u64, u64, bool), usize)> {
        Ok((
            self._owner.retained_test_identity()?,
            self._owner.sqlite_test_handle(),
        ))
    }
    /// Recover the same whole live owner and original error, not a close proof.
    /// Pure moves: no retry, allocation, resource extraction or cleanup.
    pub(in crate::backup) fn into_parts(self) -> (Box<PreparedRestore>, rusqlite::Error) {
        (self._owner, self.error)
    }
}
// If close unwinds, retain files rather than run PrivateDir::drop under possibly
// live SQLite resources. Safe ownership operations only; no manual field drops.
struct StagingDuringClose(Option<ClosedPreparedRestore>);
impl Drop for StagingDuringClose {
    fn drop(&mut self) {
        if let Some(retained) = self.0.take() {
            std::mem::forget(retained);
        }
    }
}
impl PreparedRestore {
    fn close_inner(&mut self) -> rusqlite::Result<ClosedPreparedRestore> {
        let database = self.database.take().expect("prepared connection");
        let mut retained = StagingDuringClose(self.retained.take());
        #[cfg(test)]
        close_results::after_guard();
        match database.close() {
            Ok(()) => {
                #[cfg(test)]
                close_results::record(rusqlite::ffi::SQLITE_OK);
                Ok(retained.0.take().expect("prepared files"))
            }
            Err((database, error)) => {
                self.database = Some(database);
                self.retained = retained.0.take();
                #[cfg(test)]
                close_results::record(match &error {
                    rusqlite::Error::SqliteFailure(e, _) => e.extended_code,
                    _ => -1,
                });
                Err(error)
            }
        }
    }
    pub(in crate::backup) fn try_close(
        mut self,
    ) -> std::result::Result<ClosedPreparedRestore, PreparedCloseFailure> {
        match self.close_inner() {
            Ok(closed) => Ok(closed),
            Err(error) => Err(PreparedCloseFailure {
                error,
                _owner: Box::new(self),
            }),
        }
    }
}
impl Drop for PreparedRestore {
    fn drop(&mut self) {
        if self.database.is_some() {
            match self.close_inner() {
                Ok(closed) => drop(closed),
                Err(_) => {
                    // Drop cannot report success or prove that the outstanding
                    // SQLite resources are gone. Keep connection AND files.
                    std::mem::forget(self.database.take());
                    std::mem::forget(self.retained.take());
                }
            }
        } else {
            // Explicit close already transferred the files, or an unwinding
            // close attempt owns them in StagingDuringClose. Never guess here.
            std::mem::forget(self.retained.take());
        }
    }
}
#[cfg(test)]
pub(in crate::backup) mod close_results {
    use std::cell::{Cell, RefCell};
    thread_local! {
        static RESULTS: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
        static GUARD_SCOPE: Cell<bool> = const { Cell::new(false) };
        static PANIC_PENDING: Cell<bool> = const { Cell::new(false) };
        static GUARD_VISITS: Cell<usize> = const { Cell::new(0) };
    }
    pub(super) fn after_guard() {
        if GUARD_SCOPE.get() {
            GUARD_VISITS.set(GUARD_VISITS.get() + 1);
        }
        if PANIC_PENDING.replace(false) {
            panic!("injected panic after StagingDuringClose");
        }
    }
    pub(in crate::backup) fn with_guard_panic<T>(action: impl FnOnce() -> T) -> T {
        assert!(!GUARD_SCOPE.replace(true));
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                PANIC_PENDING.set(false);
                GUARD_SCOPE.set(false);
            }
        }
        let _reset = Reset;
        GUARD_VISITS.set(0);
        PANIC_PENDING.set(true);
        action()
    }
    pub(in crate::backup) fn guard_visits() -> usize {
        GUARD_VISITS.get()
    }
    pub(super) fn record(code: i32) {
        RESULTS.with(|v| v.borrow_mut().push(code));
    }
    pub(in crate::backup) fn take() -> Vec<i32> {
        RESULTS.with(|v| v.take())
    }
}

pub(crate) fn prepare_candidate(path: &Path, cancel: Arc<AtomicBool>) -> Result<PreparedRestore> {
    prepare_in(path, &std::env::temp_dir(), cancel, |p| {
        fs2::available_space(p)
    })
}
fn io_error(e: io::Error) -> PrepareError {
    if matches!(
        e.kind(),
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
    ) {
        PrepareError::InsufficientSpace
    } else {
        PrepareError::Io
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(PrepareError::Cancelled)
    } else {
        Ok(())
    }
}
pub(super) fn prepare_in(
    path: &Path,
    parent: &Path,
    cancel: Arc<AtomicBool>,
    space: impl Fn(&Path) -> io::Result<u64>,
) -> Result<PreparedRestore> {
    check_cancel(&cancel)?;
    if !std::fs::metadata(path).map_err(io_error)?.is_file() {
        return Err(PrepareError::ResourceLimit);
    }
    let file = File::open(path).map_err(io_error)?;
    prepare_file_in(file, parent, cancel, space)
}
// The recovery inspector hashes and verifies this exact open archive, never a
// second path lookup. Existing path-based preparation delegates to the same code.
pub(in crate::backup) fn prepare_open_file(
    file: File,
    cancel: Arc<AtomicBool>,
) -> Result<PreparedRestore> {
    prepare_file_in(file, &std::env::temp_dir(), cancel, |p| {
        fs2::available_space(p)
    })
}
fn extract_archive_in(
    file: File,
    parent: &Path,
    cancel: Arc<AtomicBool>,
    space: impl Fn(&Path) -> io::Result<u64>,
) -> Result<(private::PrivateDir, BackupManifest)> {
    check_cancel(&cancel)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() {
        return Err(PrepareError::ResourceLimit);
    }
    // Streaming central-directory scan precedes the allocating ZIP name index.
    check_archive_entries(&file).map_err(|_| PrepareError::DamagedArchive)?;
    let mut zip = ZipArchive::new(file).map_err(|_| PrepareError::DamagedArchive)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| PrepareError::DamagedArchive)?;
        if entry.compression() != CompressionMethod::Stored
            || entry.encrypted()
            || entry.compressed_size() != entry.size()
            || entry
                .unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
        {
            return Err(PrepareError::ResourceLimit);
        }
        total = total
            .checked_add(entry.size())
            .ok_or(PrepareError::ResourceLimit)?;
        let end = entry
            .data_start()
            .checked_add(entry.size())
            .ok_or(PrepareError::ResourceLimit)?;
        if end > metadata.len() {
            return Err(PrepareError::DamagedArchive);
        }
    }
    if total > metadata.len() || metadata.len() - total > OVERHEAD_LIMIT {
        return Err(PrepareError::ResourceLimit);
    }
    let manifest: BackupManifest = {
        let mut entry = zip
            .by_name(MANIFEST)
            .map_err(|_| PrepareError::DamagedArchive)?;
        if entry.size() > MANIFEST_LIMIT {
            return Err(PrepareError::ResourceLimit);
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PrepareError::DamagedArchive)?;
        if bytes.len() as u64 > MANIFEST_LIMIT {
            return Err(PrepareError::ResourceLimit);
        }
        serde_json::from_slice(&bytes).map_err(|_| PrepareError::DamagedArchive)?
    };
    if manifest.format != FORMAT
        || !matches!(manifest.format_version, 1 | 2 | 3 | 4 | 5)
        || manifest.database_file != DATABASE
        || manifest.book_id.is_some()
    {
        return Err(PrepareError::UnsupportedFormat);
    }
    if !matches!(
        (manifest.format_version, manifest.database_schema_version),
        (1, 0) | (2, 1) | (3, 2) | (4, 3) | (5, 4)
    ) {
        return Err(PrepareError::IncompatibleSchema);
    }
    if manifest.app_version.is_empty()
        || uuid::Uuid::parse_str(&manifest.backup_id).is_err()
        || manifest.database_sha256.len() != 64
        || !manifest
            .database_sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
        || !OffsetDateTime::parse(&manifest.created_at, &Rfc3339)
            .is_ok_and(|t| t.offset() == time::UtcOffset::UTC)
    {
        return Err(PrepareError::DamagedArchive);
    }
    let mut entry = zip
        .by_name(DATABASE)
        .map_err(|_| PrepareError::DamagedArchive)?;
    let size = entry.size();
    if size != manifest.database_size {
        return Err(PrepareError::DamagedArchive);
    }
    let required = size
        .checked_add(DISK_RESERVE)
        .ok_or(PrepareError::ResourceLimit)?;
    if space(parent).map_err(io_error)? < required {
        return Err(PrepareError::InsufficientSpace);
    }
    let staging = private::PrivateDir::new(parent).map_err(io_error)?;
    let path = staging.path().join(DATABASE);
    let mut output = private::create_file(&path).map_err(io_error)?;
    let mut copied = 0u64;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancel(&cancel)?;
        let n = entry
            .read(&mut buffer)
            .map_err(|_| PrepareError::DamagedArchive)?;
        if n == 0 {
            break;
        }
        copied = copied
            .checked_add(n as u64)
            .ok_or(PrepareError::ResourceLimit)?;
        if copied > size {
            return Err(PrepareError::ResourceLimit);
        }
        if space(staging.path()).map_err(io_error)? < n as u64 + DISK_RESERVE {
            return Err(PrepareError::InsufficientSpace);
        }
        output.write_all(&buffer[..n]).map_err(io_error)?;
        digest.update(&buffer[..n]);
    }
    if copied != size || format!("{:x}", digest.finalize()) != manifest.database_sha256 {
        return Err(PrepareError::DamagedArchive);
    }
    output.flush().map_err(io_error)?;
    drop(output);
    drop(entry);
    drop(zip);
    check_cancel(&cancel)?;
    check_header(&path, size)?;
    Ok((staging, manifest))
}
// Import preservation validation reuses EXACTLY the same bounded package
// checks and private extraction. It cannot obtain a strict PreparedRestore.
pub(in crate::backup) fn extract_for_import(
    file: File,
    parent: &Path,
    cancel: Arc<AtomicBool>,
    space: impl Fn(&Path) -> io::Result<u64>,
) -> Result<(private::PrivateDir, BackupManifest)> {
    extract_archive_in(file, parent, cancel, space)
}
fn prepare_file_in(
    file: File,
    parent: &Path,
    cancel: Arc<AtomicBool>,
    space: impl Fn(&Path) -> io::Result<u64>,
) -> Result<PreparedRestore> {
    let (staging, manifest) = extract_archive_in(file, parent, cancel.clone(), space)?;
    let path = staging.path().join(DATABASE);
    let retained = private::open_retained(&path).map_err(io_error)?;
    let database = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| PrepareError::InvalidDatabase)?;
    harden(&database, cancel.clone())?;
    // BEGIN alone is deferred. integrity_check acquires the read snapshot, retained on return.
    database.execute_batch("BEGIN").map_err(sql_error)?;
    let actual: u32 = database
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_error)?;
    if actual != manifest.database_schema_version {
        return Err(PrepareError::IncompatibleSchema);
    }
    if manifest.format_version == 1 {
        validate_database(&database, &cancel)?;
    } else {
        database
            .set_limit(
                Limit::SQLITE_LIMIT_LENGTH,
                crate::storage_content::ROW_LIMIT as i32,
            )
            .map_err(sql_error)?;
        if manifest.format_version == 5 {
            crate::storage_content::snapshot_v5(&database, &cancel).map_err(preservation_error)?;
        } else if manifest.format_version == 4 {
            crate::storage_content::snapshot_v4(&database, &cancel).map_err(preservation_error)?;
        } else if manifest.format_version == 3 {
            crate::storage_content::snapshot_v3(&database, &cancel).map_err(preservation_error)?;
        } else {
            crate::storage_content::snapshot(&database, &cancel).map_err(preservation_error)?;
        }
    }
    database
        .progress_handler(0, None::<fn() -> bool>)
        .map_err(sql_error)?;
    check_cancel(&cancel)?;
    // Prevent accidental transaction release through the borrowed source connection.
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    database
        .authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::Transaction { .. }
            | AuthAction::Savepoint { .. }
            | AuthAction::Attach { .. }
            | AuthAction::Detach { .. }
            | AuthAction::Pragma { .. } => Authorization::Deny,
            _ => Authorization::Allow,
        }))
        .map_err(sql_error)?;
    let candidate = PreparedRestore {
        database: Some(database),
        retained: Some(ClosedPreparedRestore {
            _file: retained,
            _manifest: manifest,
            _staging: staging,
        }),
    };
    candidate.source()?;
    Ok(candidate)
}
fn handle_digest(file: &File) -> io::Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::FileExt;
        // source() may run while SQLite holds this inode's read lock. Closing
        // a duplicated fd here could release the process's POSIX locks.
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut offset = 0;
        loop {
            let n = file.read_at(&mut buffer, offset)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            offset += n as u64;
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        use std::io::{Seek, SeekFrom};
        let mut reader = file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn handle_digest_borrows_full_file_without_changing_position() {
    use std::io::{Seek, SeekFrom};
    use std::os::fd::AsRawFd;
    for length in [0, 17, 2 * 64 * 1024 + 31] {
        let bytes: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        file.seek(SeekFrom::Start(7)).unwrap();
        let fd = file.as_raw_fd();
        assert_eq!(
            handle_digest(&file).unwrap(),
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(file.stream_position().unwrap(), 7);
        assert_eq!(file.as_raw_fd(), fd);
    }
}

fn check_header(path: &Path, size: u64) -> Result<()> {
    let mut header = [0u8; 100];
    File::open(path)
        .map_err(io_error)?
        .read_exact(&mut header)
        .map_err(|_| PrepareError::InvalidDatabase)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(PrepareError::InvalidDatabase);
    }
    // Format v1 contains a standalone rollback-journal snapshot, never a WAL fragment.
    if header[18..20] != [1, 1] {
        return Err(PrepareError::InvalidDatabase);
    }
    let raw_page = u16::from_be_bytes([header[16], header[17]]);
    let page = if raw_page == 1 {
        65536
    } else {
        u64::from(raw_page)
    };
    let count = u64::from(u32::from_be_bytes(header[28..32].try_into().unwrap()));
    if !(512..=65536).contains(&page)
        || !page.is_power_of_two()
        || count.checked_mul(page) != Some(size)
    {
        return Err(PrepareError::InvalidDatabase);
    }
    Ok(())
}
pub(super) fn sql_error(e: rusqlite::Error) -> PrepareError {
    match e {
        rusqlite::Error::SqliteFailure(ref e, _) if e.code == rusqlite::ErrorCode::DiskFull => {
            PrepareError::InsufficientSpace
        }
        rusqlite::Error::SqliteFailure(ref e, _)
            if e.code == rusqlite::ErrorCode::TooBig
                || e.code == rusqlite::ErrorCode::OperationInterrupted
                || e.code == rusqlite::ErrorCode::OutOfMemory =>
        {
            PrepareError::ResourceLimit
        }
        rusqlite::Error::SqliteFailure(ref e, _)
            if matches!(
                e.code,
                rusqlite::ErrorCode::SystemIoFailure
                    | rusqlite::ErrorCode::CannotOpen
                    | rusqlite::ErrorCode::PermissionDenied
                    | rusqlite::ErrorCode::DatabaseBusy
                    | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            PrepareError::Io
        }
        _ => PrepareError::InvalidDatabase,
    }
}
pub(super) fn harden(db: &Connection, cancel: Arc<AtomicBool>) -> Result<()> {
    db.execute_batch("PRAGMA temp_store=FILE;")
        .map_err(sql_error)?;
    harden_open_connection(db, cancel)
}
pub(super) fn harden_open_connection(db: &Connection, cancel: Arc<AtomicBool>) -> Result<()> {
    // Never change temp_store on an existing connection: doing so can delete TEMP objects.
    let storage: i64 = db
        .pragma_query_value(None, "temp_store", |row| row.get(0))
        .map_err(sql_error)?;
    let file_default: bool = db
        .query_row(
            "SELECT sqlite_compileoption_used('TEMP_STORE=1')",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if storage != 1 && !(storage == 0 && file_default) {
        return Err(PrepareError::ResourceLimit);
    }

    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, DOCUMENT_LIMIT as i32),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 64 * 1024),
        (Limit::SQLITE_LIMIT_COLUMN, 32),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 32),
        (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 0),
    ] {
        db.set_limit(limit, value).map_err(sql_error)?;
    }
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(sql_error)?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA mmap_size=0; PRAGMA cache_size=-8192;")
        .map_err(sql_error)?;
    // Bounded work per database byte; no fixed book-size or wall-clock limit.
    let size = db
        .path()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0);
    let mut budget = size.saturating_mul(256).saturating_add(10_000_000) / 10_000;
    db.progress_handler(
        10_000,
        Some(move || {
            budget = budget.saturating_sub(1);
            cancel.load(Ordering::Acquire) || budget == 0
        }),
    )
    .map_err(sql_error)?;
    Ok(())
}
pub(super) fn validate_database(db: &Connection, cancel: &AtomicBool) -> Result<()> {
    let validation = (|| {
        let mut statement = db
            .prepare("PRAGMA main.integrity_check")
            .map_err(sql_error)?;
        let results = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut count = 0;
        for result in results {
            if result.map_err(sql_error)? != "ok" {
                return Err(PrepareError::InvalidDatabase);
            }
            count += 1;
        }
        if count != 1 {
            return Err(PrepareError::InvalidDatabase);
        }
        if !crate::database::validate_schema(db).map_err(|error| match sql_error(error) {
            PrepareError::ResourceLimit => PrepareError::ResourceLimit,
            PrepareError::InsufficientSpace => PrepareError::InsufficientSpace,
            PrepareError::Io => PrepareError::Io,
            _ => PrepareError::IncompatibleSchema,
        })? {
            return Err(PrepareError::IncompatibleSchema);
        }
        let mut rows = db.prepare("SELECT id, entry_date, title, content_json, document_version, created_at, updated_at FROM main.journal_entries").map_err(sql_error)?;
        let mut rows = rows.query([]).map_err(sql_error)?;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check_cancel(cancel)?;
            for index in [0, 1, 2, 3, 5, 6] {
                if row.get_ref(index).map_err(sql_error)?.data_type() != rusqlite::types::Type::Text
                {
                    return Err(PrepareError::InvalidDatabase);
                }
            }
            let id: &str = row
                .get_ref(0)
                .map_err(sql_error)?
                .as_str()
                .map_err(|_| PrepareError::InvalidDatabase)?;
            let date: &str = row
                .get_ref(1)
                .map_err(sql_error)?
                .as_str()
                .map_err(|_| PrepareError::InvalidDatabase)?;
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !valid_date(date)
            {
                return Err(PrepareError::InvalidDatabase);
            }
            if row
                .get::<_, i64>(4)
                .map_err(|_| PrepareError::IncompatibleDocument)?
                != 1
            {
                return Err(PrepareError::IncompatibleDocument);
            }
            let content = row
                .get_ref(3)
                .map_err(sql_error)?
                .as_str()
                .map_err(|_| PrepareError::IncompatibleDocument)?;
            document::validate(content)?;
        }
        Ok(())
    })();
    if cancel.load(Ordering::Acquire) {
        return Err(PrepareError::Cancelled);
    }
    validation
}
fn valid_date(s: &str) -> bool {
    let Ok(format) = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]") else {
        return false;
    };
    s.len() == 10 && time::Date::parse(s, &format).is_ok()
}
#[cfg(test)]
mod tests;

// Schema-evolution diagnostics only: no new production visibility or acceptance.
#[cfg(test)]
pub(crate) const SCHEMA_TEST_DOCUMENT_LIMIT: u64 = DOCUMENT_LIMIT;
#[cfg(test)]
pub(crate) fn validate_journal_for_schema_test(id: &str, date: &str, json: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !valid_date(date)
    {
        return Err(PrepareError::InvalidDatabase);
    }
    document::validate(json)
}

pub(crate) fn preservation_error(error: crate::storage_content::Error) -> PrepareError {
    match error {
        crate::storage_content::Error::Sql(e) => sql_error(e),
        crate::storage_content::Error::Limit => PrepareError::ResourceLimit,
        crate::storage_content::Error::Cancelled => PrepareError::Cancelled,
        _ => PrepareError::InvalidDatabase,
    }
}
