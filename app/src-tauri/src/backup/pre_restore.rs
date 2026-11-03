//! Mandatory, strictly verified safety backup. No restore or active-database mutation.
use super::restore_context::ActiveDatabase;
use super::{
    restore::{self, PrepareError, PreparedRestore},
    *,
};
use crate::database_state::Maintenance;
use std::{
    marker::PhantomData,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub(crate) enum ProtectionError {
    Cancelled,
    DurabilityUnsupported,
    DurabilityFailed,
    BindingMismatch,
    InsufficientSpace,
    PermissionDenied,
    InvalidDirectory,
    AlreadyExists,
    SourceUnavailable,
    BackupFailed,
    Verification(PrepareError),
    Io,
}
type Result<T> = std::result::Result<T, ProtectionError>;

// The borrow prevents callers from releasing maintenance while using this proof.
// Drop this proof only after copying/restoring; advance generation before dropping maintenance.
pub(crate) struct VerifiedProtection<'a, 's> {
    path: PathBuf,
    candidate: PreparedRestore,
    binding: Option<ProtectionBinding>,
    _maintenance: PhantomData<&'a Maintenance<'s>>,
}
// Retained metadata is not a restore or cleanup authorization.
pub(super) struct ProtectionRecord<'a, 's> {
    _path: PathBuf,
    _binding: Option<ProtectionBinding>,
    _maintenance: PhantomData<&'a Maintenance<'s>>,
}
impl<'a, 's> VerifiedProtection<'a, 's> {
    #[cfg(test)]
    pub(super) fn test_set_operation(&mut self, operation: uuid::Uuid) {
        self.binding.as_mut().expect("bound protection").operation = operation;
    }
    pub(super) fn into_parts(self) -> (PreparedRestore, ProtectionRecord<'a, 's>) {
        (
            self.candidate,
            ProtectionRecord {
                _path: self.path,
                _binding: self.binding,
                _maintenance: self._maintenance,
            },
        )
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn candidate(&self) -> &PreparedRestore {
        &self.candidate
    }
}

struct ProtectionBinding {
    operation: uuid::Uuid,
    generation: u32,
    database: restore::private::FileIdentity,
    path: PathBuf,
}
impl VerifiedProtection<'_, '_> {
    pub(super) fn is_bound_to(
        &self,
        maintenance: &Maintenance<'_>,
        active: &ActiveDatabase,
    ) -> bool {
        self.binding.as_ref().is_some_and(|binding| {
            binding.operation == maintenance.operation_id()
                && binding.generation == maintenance.generation()
                && binding.database == active.identity()
                && binding.path == active.path()
        })
    }
}
/// Only the context can construct ActiveDatabase. Never resolve the app path again here.
pub(super) fn create_bound<'a, 's>(
    maintenance: &'a Maintenance<'s>,
    active: &ActiveDatabase,
    cancel: Arc<AtomicBool>,
) -> Result<VerifiedProtection<'a, 's>> {
    active
        .ensure_current()
        .map_err(|_| ProtectionError::BindingMismatch)?;
    let mut proof = create_with_source(
        maintenance,
        active.path(),
        cancel,
        &name()?,
        |_| Ok(()),
        durability::sync,
        Some(active.connection()),
    )?;
    active
        .ensure_current()
        .map_err(|_| ProtectionError::BindingMismatch)?;
    proof.binding = Some(ProtectionBinding {
        operation: maintenance.operation_id(),
        generation: maintenance.generation(),
        database: active.identity(),
        path: active.path().to_owned(),
    });
    Ok(proof)
}
pub(super) fn require_supported() -> Result<()> {
    durability::require_supported()
}
fn name() -> Result<String> {
    let format =
        time::format_description::parse_borrowed::<2>("[year][month][day]-[hour][minute][second]")
            .map_err(|_| ProtectionError::BackupFailed)?;
    let timestamp = OffsetDateTime::now_utc()
        .format(&format)
        .map_err(|_| ProtectionError::BackupFailed)?;
    Ok(format!(
        "pre-restore-{timestamp}-{}.mno-backup",
        uuid::Uuid::new_v4()
    ))
}
fn io_failure(error: io::Error) -> ProtectionError {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
            ProtectionError::InsufficientSpace
        }
        io::ErrorKind::PermissionDenied => ProtectionError::PermissionDenied,
        io::ErrorKind::AlreadyExists => ProtectionError::AlreadyExists,
        _ => ProtectionError::Io,
    }
}
fn backup_failure(error: Box<dyn std::error::Error + Send + Sync>) -> ProtectionError {
    if error.is::<durability::SyncFailure>() {
        return ProtectionError::DurabilityFailed;
    }
    if let Some(error) = error.downcast_ref::<PrepareError>() {
        return ProtectionError::Verification(*error);
    }
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error.as_ref());
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<io::Error>() {
            return io_failure(io::Error::from(error.kind()));
        }
        if let Some(rusqlite::Error::SqliteFailure(error, _)) =
            error.downcast_ref::<rusqlite::Error>()
        {
            if error.code == rusqlite::ErrorCode::DiskFull {
                return ProtectionError::InsufficientSpace;
            }
        }
        cause = error.source();
    }
    ProtectionError::BackupFailed
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(ProtectionError::Cancelled)
    } else {
        Ok(())
    }
}
fn create_at<'a, 's>(
    _maintenance: &'a Maintenance<'s>,
    source_path: &Path,
    cancel: Arc<AtomicBool>,
    filename: &str,
) -> Result<VerifiedProtection<'a, 's>> {
    create_with_check(_maintenance, source_path, cancel, filename, |_| Ok(()))
}
// Deterministic cancellation test point, optimized away for the production no-op.
fn create_with_check<'a, 's>(
    _maintenance: &'a Maintenance<'s>,
    source_path: &Path,
    cancel: Arc<AtomicBool>,
    filename: &str,
    before_verification: impl FnOnce(&Path) -> BackupResult<()>,
) -> Result<VerifiedProtection<'a, 's>> {
    create_with_sync(
        _maintenance,
        source_path,
        cancel,
        filename,
        before_verification,
        durability::sync,
    )
}
fn create_with_sync<'a, 's>(
    _maintenance: &'a Maintenance<'s>,
    source_path: &Path,
    cancel: Arc<AtomicBool>,
    filename: &str,
    before_verification: impl FnOnce(&Path) -> BackupResult<()>,
    sync: impl FnMut(durability::SyncPoint, &Path) -> io::Result<()>,
) -> Result<VerifiedProtection<'a, 's>> {
    create_with_source(
        _maintenance,
        source_path,
        cancel,
        filename,
        before_verification,
        sync,
        None,
    )
}
fn create_with_source<'a, 's>(
    _maintenance: &'a Maintenance<'s>,
    source_path: &Path,
    cancel: Arc<AtomicBool>,
    filename: &str,
    before_verification: impl FnOnce(&Path) -> BackupResult<()>,
    sync: impl FnMut(durability::SyncPoint, &Path) -> io::Result<()>,
    provided_source: Option<&Connection>,
) -> Result<VerifiedProtection<'a, 's>> {
    durability::require_supported()?;
    check_cancel(&cancel)?;
    let root = source_path
        .parent()
        .ok_or(ProtectionError::InvalidDirectory)?;
    let metadata = std::fs::symlink_metadata(root).map_err(io_failure)?;
    if !restore::private::is_plain_directory(&metadata) {
        return Err(ProtectionError::InvalidDirectory);
    }
    let opened = if provided_source.is_none() {
        Some(
            Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|_| ProtectionError::SourceUnavailable)?,
        )
    } else {
        None
    };
    let source = provided_source
        .or(opened.as_ref())
        .expect("source connection supplied or opened");
    let backups = root.join("backups");
    restore::private::ensure_directory(&backups).map_err(io_failure)?;
    let directory = backups.join("pre-restore");
    restore::private::ensure_directory(&directory).map_err(io_failure)?;
    let destination = directory.join(filename);
    if destination.symlink_metadata().is_ok() {
        return Err(ProtectionError::AlreadyExists);
    }
    let mut candidate = None;
    let sync = std::cell::RefCell::new(sync);
    create_with_publication(
        source,
        &destination,
        BackupKind::Manual,
        Duration::from_secs(30),
        |archive, _| {
            before_verification(archive)?;
            candidate = Some(restore::prepare_in(
                archive,
                &directory,
                cancel.clone(),
                |path| fs2::available_space(path),
            )?);
            check_cancel(&cancel).map_err(|_| PrepareError::Cancelled)?;
            (sync.borrow_mut())(durability::SyncPoint::VerifiedFile, archive)
                .map_err(durability::SyncFailure)?;
            Ok(())
        },
        |published| {
            // Sync the leaf first, then every MNO-created parent entry, even on retries.
            let root_parent = root
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            for path in [&directory, &backups, root, root_parent] {
                (sync.borrow_mut())(durability::SyncPoint::Directory, path)
                    .map_err(durability::SyncFailure)?;
            }
            // On macOS explicitly request F_FULLFSYNC after directory fsyncs.
            (sync.borrow_mut())(durability::SyncPoint::FinalFileBarrier, published)
                .map_err(durability::SyncFailure)?;
            Ok(())
        },
    )
    .map_err(backup_failure)?;
    // Publication AND every required durability barrier succeeded; no fallible work remains.
    Ok(VerifiedProtection {
        path: destination,
        candidate: candidate.expect("verified before publication"),
        binding: None,
        _maintenance: PhantomData,
    })
}

pub(super) mod durability;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;
#[cfg(all(test, not(any(target_os = "macos", target_os = "linux"))))]
mod unsupported_tests {
    use super::*;
    #[test]
    fn unsupported_durability_never_reports_success_or_creates_directories() {
        let root = TempDir::new().unwrap();
        let state = crate::database_state::DatabaseState::ready_for_test();
        let maintenance = state.maintenance().unwrap();
        let result = create_at(
            &maintenance,
            &root.path().join("missing.sqlite"),
            Arc::new(AtomicBool::new(false)),
            &name().unwrap(),
        );
        assert!(matches!(
            result,
            Err(ProtectionError::DurabilityUnsupported)
        ));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

// Inert ownership projection; no restore/publication authority.
pub(super) struct ClosedProtectionRecord {
    _path: PathBuf,
    _binding: Option<ProtectionBinding>,
}
impl ProtectionRecord<'_, '_> {
    pub(super) fn into_closed_record(self) -> ClosedProtectionRecord {
        let Self {
            _path,
            _binding,
            _maintenance: _,
        } = self;
        ClosedProtectionRecord { _path, _binding }
    }
}

#[cfg(test)]
impl ProtectionRecord<'_, '_> {
    pub(super) fn into_test_closed(self) -> ClosedProtectionRecord {
        self.into_closed_record()
    }
}

// Inert test observation of retained binding values; no I/O or authority.
#[cfg(all(test, target_os = "macos"))]
type ProtectionIdentity = (
    PathBuf,
    Option<(uuid::Uuid, u32, restore::private::FileIdentity, PathBuf)>,
);
#[cfg(all(test, target_os = "macos"))]
impl VerifiedProtection<'_, '_> {
    pub(in crate::backup) fn test_record_identity(&self) -> ProtectionIdentity {
        (
            self.path.clone(),
            self.binding
                .as_ref()
                .map(|b| (b.operation, b.generation, b.database, b.path.clone())),
        )
    }
}
#[cfg(all(test, target_os = "macos"))]
impl ProtectionRecord<'_, '_> {
    pub(in crate::backup) fn test_record_identity(&self) -> ProtectionIdentity {
        (
            self._path.clone(),
            self._binding
                .as_ref()
                .map(|b| (b.operation, b.generation, b.database, b.path.clone())),
        )
    }
}
