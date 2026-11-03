//! Read-only classification and proof-owned completion stages under retained
//! process/session guards. Startup stays gated; a hot journal is never replayed.
use super::*;
use crate::backup::{journal_content, restore};
#[cfg(test)]
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;
mod bound_sqlite;
mod evidence;
pub(super) mod result;
use bound_sqlite::{BoundSqlite, Control};
use evidence::Evidence;
pub(crate) use result::{Assessment, Reason};
pub(crate) type CompletionProof<'a, 's> = result::CompletionProof<'a, 's>;
pub(crate) type AllConnectionsClosed<'a, 's> =
    result::receipt::all_closed::AllConnectionsClosed<'a, 's>;
pub(crate) type AllCloseFailure<'a, 's> = result::receipt::all_closed::AllCloseFailure<'a, 's>;
pub(crate) type AllCloseError = result::receipt::all_closed::AllCloseError;
pub(crate) type ProofRejection = result::ProofRejection;
pub(crate) type RejectedAssessment<'a, 's> = result::RejectedAssessment<'a, 's>;
pub(crate) type Status = result::Status;
use result::VerifiedStatus;

// Drop SQLite BEFORE closing any other descriptor for the same inode (POSIX
// locks can otherwise be released). All fields and connections remain private.
struct Observation {
    database: BoundSqlite,
    protection: PreparedRestore,
    binding: OperationBinding,
    files: Inputs,
    cancel: Arc<AtomicBool>,
}
// Only a real successful unlink owner can request the missing-Q observation.
// This is a borrowed verification mode, never a constructible completion proof.
enum ReceiptCheck<'p, 'a, 's> {
    Named(Option<&'p std::ffi::OsStr>),
    Startup(&'p result::receipt::all_closed::StartupHandoff<'s>),
    Running(&'p result::receipt::all_closed::RunningHandoff<'s>),
    Unlinked(&'p result::receipt::all_closed::QUnlinked<'a, 's>),
}
struct OperationBinding {
    restore_id: uuid::Uuid,
    directory: Identity,
    database: Identity,
    marker_sha256: String,
    database_sha256: String,
}
// Conversion does not establish a second snapshot or open the active database
// again. Recheck the retained VFS, exact artifacts, bindings and file bytes while
// the original reservation and session are still held.
impl Observation {
    fn recheck(&self, access: &Access<'_, '_>) -> std::result::Result<(), Reason> {
        self.recheck_temporary(access, None)
    }
    fn recheck_temporary(
        &self,
        access: &Access<'_, '_>,
        temporary: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), Reason> {
        cancelled(&self.cancel)?;
        access.check()?;
        self.database.verify()?;
        self.protection.source().map_err(|_| Reason::Protection)?;
        self.binding
            .check_files(&self.files, access, &self.cancel, temporary)?;
        if self.database.is_autocommit() {
            return Err(Reason::Identity);
        }
        let bytes = &self.files.evidence.bytes;
        let data = parse(bytes).map_err(|_| Reason::Marker)?;
        let cancel = &self.cancel;
        self.database.verify()?;
        access.check()?;
        self.files.check_temporary(&data, bytes, cancel, temporary)
    }
}
// Same retained bytes/identities and reference hashes before and after SQLite-close.
impl OperationBinding {
    fn check_files(
        &self,
        files: &Inputs,
        access: &Access<'_, '_>,
        cancel: &AtomicBool,
        temporary: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), Reason> {
        self.check_files_mode(files, access, cancel, &ReceiptCheck::Named(temporary))
    }
    fn check_files_mode(
        &self,
        files: &Inputs,
        access: &Access<'_, '_>,
        cancel: &AtomicBool,
        mode: &ReceiptCheck<'_, '_, '_>,
    ) -> std::result::Result<(), Reason> {
        cancelled(cancel)?;
        access.check()?;
        let data = parse(&files.evidence.bytes).map_err(|_| Reason::Marker)?;
        access.check_operation(&data)?;
        self.check_contents(files, cancel, mode)?;
        access.check()
    }
    fn check_contents(
        &self,
        files: &Inputs,
        cancel: &AtomicBool,
        mode: &ReceiptCheck<'_, '_, '_>,
    ) -> std::result::Result<(), Reason> {
        cancelled(cancel)?;
        let bytes = &files.evidence.bytes;
        let data = parse(bytes).map_err(|_| Reason::Marker)?;
        if data.restore_id != self.restore_id.to_string()
            || data.directory_identity != self.directory
            || data.database_identity != self.database
            || format!("{:x}", Sha256::digest(bytes)) != self.marker_sha256
        {
            return Err(Reason::Identity);
        }
        files.check_mode(&data, bytes, cancel, mode)?;
        if hash(&files.active_file, cancel)? != self.database_sha256 {
            return Err(Reason::Content);
        }
        if hash(&files.protection_file, cancel)? != data.protection_sha256 {
            return Err(Reason::Protection);
        }
        cancelled(cancel)?;
        files.check_mode(&data, bytes, cancel, mode)
    }
}
// The guard is held by Assessment, AFTER Observation in destruction order.
// Neither branch acquires a second lock during inspection.
enum Access<'a, 's> {
    Standalone(crate::startup::DirectoryLease),
    Session(crate::database_state::recovery_session::RecoverySession<'a, 's>),
}
impl Access<'_, '_> {
    fn is_session(&self) -> bool {
        matches!(self, Self::Session(_))
    }

    fn check_operation(&self, data: &PendingRestore) -> std::result::Result<(), Reason> {
        if let Self::Session(session) = self {
            if let Some((operation, _generation)) = session.maintenance_binding() {
                if operation.to_string() != data.restore_id {
                    return Err(Reason::Marker);
                }
            }
        }
        Ok(())
    }
    fn check(&self) -> std::result::Result<(), Reason> {
        match self {
            Self::Standalone(lease) => lease.ensure_current(),
            Self::Session(session) => session.ensure_current(),
        }
        .map_err(|_| Reason::Identity)
    }
}
struct Inputs {
    root: PathBuf,
    root_file: File,
    evidence: Evidence,
    active_file: Arc<File>,
    protection_file: File,
    backup_directory: File,
    protection_directory: File,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Locked,
    BeforeSqlite,
    AfterSqliteOpen,
    IdentityCheck,
    Snapshot,
    Final,
}
fn cancelled(cancel: &AtomicBool) -> std::result::Result<(), Reason> {
    if cancel.load(Ordering::Acquire) {
        Err(Reason::Cancelled)
    } else {
        Ok(())
    }
}
fn hash(file: &File, cancel: &AtomicBool) -> std::result::Result<String, Reason> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            cancelled(cancel)?;
            let n = file.read_at(&mut buffer, offset).map_err(|_| Reason::Io)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            offset += n as u64;
            #[cfg(test)]
            hash_blocks::processed(file, offset);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, cancel);
        Err(Reason::Unsupported)
    }
}
// Test-only, thread-local observer: no registry, callback or branch survives in
// release. Scoped reset also removes the observer if a test assertion panics.
#[cfg(test)]
mod hash_blocks {
    use std::{cell::RefCell, fs::File};
    type Observer = Box<dyn FnMut(&File, u64)>;
    thread_local! {
        static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
    }
    pub(super) fn processed(file: &File, bytes: u64) {
        OBSERVER.with(|slot| {
            if let Some(observer) = slot.borrow_mut().as_mut() {
                observer(file, bytes);
            }
        });
    }
    pub(super) fn with_observer<T>(
        observer: impl FnMut(&File, u64) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                OBSERVER.with(|slot| *slot.borrow_mut() = None);
            }
        }
        OBSERVER.with(|slot| {
            assert!(
                slot.borrow().is_none(),
                "nested hash observers are unsupported"
            );
            *slot.borrow_mut() = Some(Box::new(observer));
        });
        let _reset = Reset;
        action()
    }
}
fn current(file: &File, path: &Path, directory: bool) -> std::result::Result<(), Reason> {
    let meta = fs::symlink_metadata(path).map_err(|_| Reason::Identity)?;
    if meta.file_type().is_symlink()
        || (directory && !private::is_plain_directory(&meta))
        || (!directory && !meta.is_file())
    {
        return Err(Reason::Identity);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let retained = file.metadata().map_err(|_| Reason::Identity)?;
        if retained.dev() != meta.dev()
            || retained.ino() != meta.ino()
            || (!directory && meta.nlink() != 1)
        {
            return Err(Reason::Identity);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(Reason::Unsupported)
    }
}
fn sidecars(root: &Path) -> std::result::Result<(), Reason> {
    match crate::startup::artifacts::sidecars(
        &crate::database::expected_database_path(root),
        &mut |p| fs::symlink_metadata(p),
    ) {
        Ok(false) => Ok(()),
        _ => Err(Reason::SqliteState),
    }
}
impl Inputs {
    fn check(
        &self,
        data: &PendingRestore,
        bytes: &[u8],
        cancel: &AtomicBool,
    ) -> std::result::Result<(), Reason> {
        self.check_temporary(data, bytes, cancel, None)
    }
    fn check_temporary(
        &self,
        data: &PendingRestore,
        bytes: &[u8],
        cancel: &AtomicBool,
        temporary: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), Reason> {
        self.check_mode(data, bytes, cancel, &ReceiptCheck::Named(temporary))
    }
    fn check_mode(
        &self,
        data: &PendingRestore,
        bytes: &[u8],
        cancel: &AtomicBool,
        mode: &ReceiptCheck<'_, '_, '_>,
    ) -> std::result::Result<(), Reason> {
        cancelled(cancel)?;
        current(&self.root_file, &self.root, true)?;
        match mode {
            ReceiptCheck::Named(temporary) => self
                .evidence
                .check_temporary(&self.root, bytes, *temporary)?,
            ReceiptCheck::Startup(handoff) => {
                self.evidence.check_startup(&self.root, bytes, handoff)?
            }
            ReceiptCheck::Running(handoff) => {
                self.evidence.check_running(&self.root, bytes, handoff)?
            }
            ReceiptCheck::Unlinked(proof) => {
                self.evidence.check_unlinked(&self.root, bytes, proof)?
            }
        }
        current(&self.active_file, &self.root.join("mno.sqlite"), false)?;
        current(&self.backup_directory, &self.root.join("backups"), true)?;
        current(
            &self.protection_directory,
            &self.root.join("backups/pre-restore"),
            true,
        )?;
        current(
            &self.protection_file,
            &self.root.join(&data.protection_path),
            false,
        )?;
        if identity(&self.root_file).map_err(|_| Reason::Identity)? != data.directory_identity
            || identity(&self.active_file).map_err(|_| Reason::Identity)? != data.database_identity
        {
            return Err(Reason::Identity);
        }
        sidecars(&self.root)
    }
}
/// Consumes the session; its directory lease and maintenance borrow remain
/// held even for Uncertain/NotNeeded. Diagnostic status never authorizes cleanup.
pub(crate) fn classify_session<'a, 's>(
    session: crate::database_state::recovery_session::RecoverySession<'a, 's>,
    cancel: Arc<AtomicBool>,
) -> Assessment<'a, 's> {
    let root = match session.database_path() {
        Ok(path) => path.parent().expect("fixed database path").to_owned(),
        Err(_) => return Assessment::uncertain(Reason::Identity).retain(Access::Session(session)),
    };
    assess(
        &root,
        Access::Session(session),
        cancel,
        |_| {},
        Control::Real,
    )
}
pub(crate) fn classify(root: &Path, cancel: Arc<AtomicBool>) -> Assessment<'static, 'static> {
    classify_with(root, cancel, |_| {})
}
fn classify_with(
    root: &Path,
    cancel: Arc<AtomicBool>,
    hook: impl FnMut(Point),
) -> Assessment<'static, 'static> {
    classify_with_control(root, cancel, hook, Control::Real)
}
fn classify_with_control(
    root: &Path,
    cancel: Arc<AtomicBool>,
    hook: impl FnMut(Point),
    control: Control,
) -> Assessment<'static, 'static> {
    if !cfg!(target_os = "macos") {
        return Assessment::uncertain(Reason::Unsupported);
    }
    let lease = match crate::startup::DirectoryLease::open_existing(root) {
        Ok(lease) => lease,
        Err(_) => return Assessment::uncertain(Reason::Busy),
    };
    assess(root, Access::Standalone(lease), cancel, hook, control)
}
fn assess<'a, 's>(
    root: &Path,
    access: Access<'a, 's>,
    cancel: Arc<AtomicBool>,
    hook: impl FnMut(Point),
    control: Control,
) -> Assessment<'a, 's> {
    let assessment = match inspect(root, &access, cancel, hook, control) {
        Ok(Some((status, observation))) => Assessment::verified(status, observation),
        Ok(None) => Assessment::not_needed(),
        Err(reason) => Assessment::uncertain(reason),
    };
    assessment.retain(access)
}
fn inspect(
    root: &Path,
    access: &Access<'_, '_>,
    cancel: Arc<AtomicBool>,
    mut hook: impl FnMut(Point),
    control: Control,
) -> std::result::Result<Option<(VerifiedStatus, Observation)>, Reason> {
    if !cfg!(target_os = "macos") {
        return Err(Reason::Unsupported);
    }
    cancelled(&cancel)?;
    access.check()?;
    let root_file = open_plain(root, true).map_err(|_| Reason::Identity)?;
    let root = root.canonicalize().map_err(|_| Reason::Identity)?;
    current(&root_file, &root, true)?;
    hook(Point::Locked);
    let Some(evidence) = Evidence::load(&root)? else {
        access.check()?;
        current(&root_file, &root, true)?;
        cancelled(&cancel)?;
        return Ok(None);
    };
    let bytes = evidence.bytes.clone();
    let data = parse(&bytes).map_err(|_| Reason::Marker)?;
    access.check_operation(&data)?;
    let active_file =
        Arc::new(open_plain(&root.join("mno.sqlite"), false).map_err(|_| Reason::Identity)?);
    let backup_directory =
        open_plain(&root.join("backups"), true).map_err(|_| Reason::Protection)?;
    let protection_directory =
        open_plain(&root.join("backups/pre-restore"), true).map_err(|_| Reason::Protection)?;
    let protection_file =
        open_plain(&root.join(&data.protection_path), false).map_err(|_| Reason::Protection)?;
    let inputs = Inputs {
        root,
        root_file,
        evidence,
        active_file,
        protection_file,
        backup_directory,
        protection_directory,
    };
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    if hash(&inputs.protection_file, &cancel)? != data.protection_sha256 {
        return Err(Reason::Protection);
    }
    let protection = restore::prepare_open_file(
        inputs
            .protection_file
            .try_clone()
            .map_err(|_| Reason::Protection)?,
        cancel.clone(),
    )
    .map_err(|_| Reason::Protection)?;
    let old = journal_content::fingerprint(
        &VerifiedJournal::from_candidate(&protection, cancel.clone())
            .map_err(|_| Reason::Protection)?,
    )
    .map_err(|_| Reason::Protection)?;
    if old.version != data.old_content.version || old.sha256 != data.old_content.sha256 {
        return Err(Reason::Protection);
    }
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    let original = hash(&inputs.active_file, &cancel)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        let mut header = [0; 20];
        inputs
            .active_file
            .read_exact_at(&mut header, 0)
            .map_err(|_| Reason::Database)?;
        if &header[..16] != b"SQLite format 3\0" || header[18] != 1 || header[19] != 1 {
            return Err(Reason::SqliteState);
        }
    }
    hook(Point::BeforeSqlite);
    sidecars(&inputs.root)?;
    let mut bound = BoundSqlite::open(
        &inputs.root.join("mno.sqlite"),
        inputs.active_file.clone(),
        control,
    )?;
    hook(Point::AfterSqliteOpen);
    bound.verify()?;
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    let database = bound.connection_mut();
    database
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(|_| Reason::Database)?;
    database
        // Set before any database access. If an external client races the
        // DELETE-header check by switching to WAL, do not create a shared-memory
        // wal-index on this read-only connection. This never changes journal_mode.
        .execute_batch("PRAGMA query_only=ON; PRAGMA locking_mode=EXCLUSIVE; BEGIN DEFERRED")
        .map_err(|_| Reason::Database)?;
    let locking: String = database
        .pragma_query_value(None, "locking_mode", |r| r.get(0))
        .map_err(|_| Reason::SqliteState)?;
    if !locking.eq_ignore_ascii_case("exclusive") {
        return Err(Reason::SqliteState);
    }
    let snapshot =
        VerifiedJournal::verify(database, cancel.clone()).map_err(|_| Reason::Database)?;
    let active = journal_content::fingerprint(&snapshot).map_err(|_| Reason::Database)?;
    snapshot.finish().map_err(|_| Reason::Database)?;
    let autocommit = database.is_autocommit();
    hook(Point::Snapshot);
    bound.verify()?;
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    if autocommit
        || original != hash(&inputs.active_file, &cancel)?
        || hash(&inputs.protection_file, &cancel)? != data.protection_sha256
    {
        return Err(Reason::Identity);
    }
    let is_old =
        active.version == data.old_content.version && active.sha256 == data.old_content.sha256;
    let is_new =
        active.version == data.new_content.version && active.sha256 == data.new_content.sha256;
    let status = match inputs.evidence.receipt.as_ref().map(|q| q.outcome()) {
        Some(super::completion::Outcome::Old) if is_old => VerifiedStatus::Old,
        Some(super::completion::Outcome::New) if is_new => VerifiedStatus::New,
        None if is_old => VerifiedStatus::Old,
        None if is_new => VerifiedStatus::New,
        _ => return Err(Reason::Content),
    };
    hook(Point::Final);
    bound.verify()?;
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    if original != hash(&inputs.active_file, &cancel)?
        || hash(&inputs.protection_file, &cancel)? != data.protection_sha256
    {
        return Err(Reason::Identity);
    }
    cancelled(&cancel)?;
    hook(Point::IdentityCheck);
    bound.verify()?;
    access.check()?;
    inputs.check(&data, &bytes, &cancel)?;
    Ok(Some((
        status,
        Observation {
            database: bound,
            protection,
            binding: OperationBinding {
                restore_id: uuid::Uuid::parse_str(&data.restore_id).map_err(|_| Reason::Marker)?,
                directory: data.directory_identity,
                database: data.database_identity,
                marker_sha256: format!("{:x}", Sha256::digest(bytes)),
                database_sha256: original,
            },
            files: inputs,
            cancel,
        },
    )))
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

#[cfg(all(test, not(target_os = "macos")))]
mod unsupported_binding_tests {
    use super::*;
    #[test]
    fn unsupported_physical_binding_is_uncertain_without_creating_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("missing");
        let result = classify(&missing, Arc::new(AtomicBool::new(false)));
        assert_eq!(result.status(), Status::Uncertain);
        assert_eq!(result.reason(), Some(Reason::Unsupported));
        assert!(!missing.exists());
    }
}

// Compile actual capability definitions in small standalone probes. Only the
// filesystem I/O backends are stubbed, just like the probes' Observation. The
// runtime suite separately exercises the real publisher and system calls.
#[cfg(test)]
fn capability_probe(directory: &Path) -> PathBuf {
    let api = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/backup/restore_context/pending/classification/result.rs");
    let receipt_source = fs::read_to_string(api.with_file_name("receipt.rs")).unwrap();
    let definitions = receipt_source
        .split("// Only this module constructs")
        .next()
        .unwrap();
    let stub = directory.join("receipt.rs");
    fs::write(&stub, format!("{}\npub(super) fn publish<'a, 's>(proof:super::CompletionProof<'a, 's>)->Result<ReceiptDurable<'a, 's>,ReceiptFailure<'a, 's>>{{Ok(ReceiptDurable{{proof:Box::new(proof)}})}}\nfn remove_pending_impl<'a, 's>(receipt:ReceiptDurable<'a, 's>)->Result<RecoveryDirectoryRemoved<'a, 's>,CleanupFailure<'a, 's>>{{Ok(RecoveryDirectoryRemoved{{receipt}})}}",definitions)).unwrap();
    // The probe keeps real capability definitions; only the resource backend is stubbed.
    use std::io::Write as _;
    let mut stub_file = fs::OpenOptions::new().append(true).open(&stub).unwrap();
    writeln!(stub_file, "mod resource_close {{ pub(super) struct Closed<'a, 's>(pub &'a (), pub &'s ()); pub(super) struct Retained<'a, 's>(pub &'a (), pub &'s ()); pub(super) fn close<'a, 's>(_:super::RecoveryDirectoryRemoved<'a, 's>)->Result<super::ResourcesClosed<'a, 's>,super::ResourceCloseFailure<'a, 's>> {{ unimplemented!() }} }}").unwrap();
    let definitions = fs::read_to_string(&api)
        .unwrap()
        .replace("#[path = \"receipt.rs\"]", &format!("#[path = {:?}]", stub))
        .replace(
            "pub(in crate::backup::restore_context::pending) mod receipt",
            "pub(super) mod receipt",
        );
    let projected = directory.join("result.rs");
    fs::write(&projected, definitions).unwrap();
    projected
}

pub(crate) type RunningHandoff<'s> = result::receipt::all_closed::RunningHandoff<'s>;
pub(crate) type RunningAdmissionFailure<'s> =
    result::receipt::all_closed::RunningAdmissionFailure<'s>;

pub(crate) type StartupHandoff<'s> = result::receipt::all_closed::StartupHandoff<'s>;
pub(crate) type StartupAdmissionFailure<'s> =
    result::receipt::all_closed::StartupAdmissionFailure<'s>;
