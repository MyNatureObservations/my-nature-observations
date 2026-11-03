//! One process-local restore session. No active database writes or generation changes.
use super::{
    pre_restore::{self, ProtectionError, VerifiedProtection},
    restore::private::{self, FileIdentity},
    restore::{PrepareError, PreparedRestore},
};
use crate::{
    database,
    database_state::{DatabaseState, Maintenance},
};
use rusqlite::{Connection, OpenFlags};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
use tauri::Manager;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ContextError {
    Coordination(String),
    GenerationExhausted,
    ActiveDatabaseChanged,
    SourceUnavailable,
    UnsupportedJournalMode,
    Candidate(PrepareError),
    Protection(ProtectionError),
    InvalidProof,
    ProofAlreadyUsed,
}
type Result<T> = std::result::Result<T, ContextError>;

// Constructor private to this module. Path and connection are resolved once under maintenance.
pub(super) struct ActiveDatabase {
    connection: Option<Connection>,
    files: Option<ActiveFiles>,
}
struct ActiveFiles {
    file: File,
    identity: FileIdentity,
    path: PathBuf,
}
impl ActiveDatabase {
    fn open(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path).map_err(|_| ContextError::SourceUnavailable)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ContextError::SourceUnavailable);
        }
        let parent = path.parent().ok_or(ContextError::SourceUnavailable)?;
        if !private::is_plain_directory(
            &fs::symlink_metadata(parent).map_err(|_| ContextError::SourceUnavailable)?,
        ) {
            return Err(ContextError::SourceUnavailable);
        }
        let path = path
            .canonicalize()
            .map_err(|_| ContextError::SourceUnavailable)?;
        let file = File::open(&path).map_err(|_| ContextError::SourceUnavailable)?;
        let identity =
            private::file_identity(&file).map_err(|_| ContextError::SourceUnavailable)?;
        // Reject WAL before SQLite can create WAL/SHM side files on a read-only open.
        use std::io::Read;
        let mut header = [0; 20];
        (&file)
            .read_exact(&mut header)
            .map_err(|_| ContextError::SourceUnavailable)?;
        if &header[..16] != b"SQLite format 3\0" {
            return Err(ContextError::SourceUnavailable);
        }
        if header[18] != 1 || header[19] != 1 {
            return Err(ContextError::UnsupportedJournalMode);
        }

        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| ContextError::SourceUnavailable)?;
        let mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(|_| ContextError::SourceUnavailable)?;
        if !mode.eq_ignore_ascii_case("delete") {
            return Err(ContextError::UnsupportedJournalMode);
        }
        let active = Self {
            connection: Some(connection),
            files: Some(ActiveFiles {
                file,
                identity,
                path,
            }),
        };
        active.ensure_current()?;
        Ok(active)
    }
    pub(super) fn path(&self) -> &Path {
        &self.files.as_ref().expect("active files").path
    }
    pub(super) fn identity(&self) -> FileIdentity {
        self.files.as_ref().expect("active files").identity
    }
    pub(super) fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("active connection")
    }
    pub(super) fn ensure_current(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.files.as_ref().expect("active files").path)
            .map_err(|_| ContextError::ActiveDatabaseChanged)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || !private::same_file(
                &self.files.as_ref().expect("active files").file,
                &self.files.as_ref().expect("active files").path,
            )
            .unwrap_or(false)
        {
            return Err(ContextError::ActiveDatabaseChanged);
        }
        Ok(())
    }
}

pub(crate) struct RestoreContext<'operation, 's> {
    maintenance: &'operation Maintenance<'s>,
    terminal_policy: Option<&'operation TerminalPolicy>,
    active: ActiveDatabase,
    incoming: PreparedRestore,
    protection: Option<VerifiedProtection<'operation, 's>>,
    generation: u32,
    next_generation: u32,
    claimed: bool,
}
// A non-cloneable, borrowed authorization. A future engine must consume it in this callback.
pub(crate) struct AuthorizedRestore<'context, 'operation, 's> {
    context: &'context RestoreContext<'operation, 's>,
}
impl<'operation, 's> RestoreContext<'operation, 's> {
    fn attach(&mut self, proof: VerifiedProtection<'operation, 's>) -> Result<()> {
        if self.claimed || self.protection.is_some() {
            return Err(ContextError::ProofAlreadyUsed);
        }
        if !proof.is_bound_to(self.maintenance, &self.active) {
            return Err(ContextError::InvalidProof);
        }
        self.active.ensure_current()?;
        self.protection = Some(proof);
        Ok(())
    }
    pub fn authorize(&mut self) -> Result<AuthorizedRestore<'_, 'operation, 's>> {
        if self.claimed {
            return Err(ContextError::ProofAlreadyUsed);
        }
        self.maintenance
            .ensure_usable()
            .map_err(ContextError::Coordination)?;
        self.active.ensure_current()?;
        if self.maintenance.generation() != self.generation
            || !self
                .protection
                .as_ref()
                .is_some_and(|proof| proof.is_bound_to(self.maintenance, &self.active))
        {
            return Err(ContextError::InvalidProof);
        }
        self.incoming.source().map_err(ContextError::Candidate)?;
        self.claimed = true;
        Ok(AuthorizedRestore { context: self })
    }
}
impl AuthorizedRestore<'_, '_, '_> {
    pub fn generations(&self) -> (u32, u32) {
        (self.context.generation, self.context.next_generation)
    }
    pub fn incoming(&self) -> Result<&Connection> {
        self.context
            .incoming
            .source()
            .map_err(ContextError::Candidate)
    }
    pub fn protection_path(&self) -> &Path {
        self.context
            .protection
            .as_ref()
            .expect("authorized proof")
            .path()
    }
}

/// Sole production entry: app path and DatabaseState originate from the same AppHandle.
/// The callback owns the context so it can consume it after authorization borrows end.
/// Both lifetimes stay higher-ranked here: neither a local Maintenance borrow
/// nor the AppHandle state borrow can escape through T.
pub(crate) fn with_restore_context<T>(
    app: &tauri::AppHandle,
    candidate: PreparedRestore,
    cancel: Arc<AtomicBool>,
    action: impl for<'operation, 's> FnOnce(RestoreContext<'operation, 's>) -> Result<T>,
) -> Result<T> {
    pre_restore::require_supported().map_err(ContextError::Protection)?;
    let state = app.state::<DatabaseState>();
    let path = database::database_path(app).map_err(|_| ContextError::SourceUnavailable)?;
    with_active(&state, &path, candidate, cancel, action)
}
// Policy only, not authority or close proof. Private writer preparation or B-close arms it;
// only the private completed writer handoff can produce Finished.
struct TerminalPolicy {
    phase: std::cell::Cell<u8>,
}
#[repr(u8)]
#[allow(dead_code)]
enum TerminalPhase {
    NeverArmed,
    Armed,
    Finished,
}
impl TerminalPolicy {
    fn new() -> Self {
        Self {
            phase: std::cell::Cell::new(TerminalPhase::NeverArmed as u8),
        }
    }
    fn abort_if_armed(&self) {
        if self.phase.get() == TerminalPhase::Armed as u8 {
            std::process::abort();
        }
    }
    #[cfg(all(target_os = "macos", any(test, panic = "abort")))]
    fn is_never_armed(&self) -> bool {
        self.phase.get() == TerminalPhase::NeverArmed as u8
    }
    #[cfg(all(target_os = "macos", any(test, panic = "abort")))]
    fn is_armed(&self) -> bool {
        self.phase.get() == TerminalPhase::Armed as u8
    }
    // Only the private B-close producer calls this in production-defined code.
    // No reset; the whole outer scope remains terminal until verified completion.
    #[cfg(all(target_os = "macos", any(test, panic = "abort")))]
    fn arm_for_b_close(&self) {
        assert!(self.is_never_armed());
        self.phase.set(TerminalPhase::Armed as u8);
    }
    // No resource may be acquired before the whole episode is terminally guarded.
    // This is not writer-open authority and has no reset/Finished transition.
    #[cfg(all(target_os = "macos", any(test, panic = "abort")))]
    fn arm_for_writer(&self) {
        assert!(self.is_never_armed());
        self.phase.set(TerminalPhase::Armed as u8);
    }
    #[cfg(all(test, target_os = "macos"))]
    fn arm_for_test(&self) {
        assert_eq!(self.phase.get(), TerminalPhase::NeverArmed as u8);
        self.phase.set(TerminalPhase::Armed as u8);
    }
}
// Actual Maintenance remains outer-owned. Armed Drop stops before its fields.
struct RestoreMaintenanceScope<'s> {
    maintenance: Maintenance<'s>,
    terminal_policy: TerminalPolicy,
}
impl<'s> RestoreMaintenanceScope<'s> {
    fn maintenance(&self) -> &Maintenance<'s> {
        &self.maintenance
    }
}
impl Drop for RestoreMaintenanceScope<'_> {
    fn drop(&mut self) {
        self.terminal_policy.abort_if_armed();
    }
}
// Internally state is fixed while the callback quantifies only the guard borrow.
// T cannot carry that guard borrow; no handoff factory is exposed.
fn with_active<'s, T>(
    state: &'s DatabaseState,
    path: &Path,
    candidate: PreparedRestore,
    cancel: Arc<AtomicBool>,
    action: impl for<'operation> FnOnce(RestoreContext<'operation, 's>) -> Result<T>,
) -> Result<T> {
    pre_restore::require_supported().map_err(ContextError::Protection)?;
    let scope = RestoreMaintenanceScope {
        maintenance: state.maintenance().map_err(ContextError::Coordination)?,
        terminal_policy: TerminalPolicy::new(),
    };
    let maintenance = scope.maintenance();
    maintenance
        .ensure_usable()
        .map_err(ContextError::Coordination)?;
    let generation = maintenance.generation();
    let next_generation = maintenance
        .checked_next_generation()
        .map_err(|_| ContextError::GenerationExhausted)?;
    candidate.source().map_err(ContextError::Candidate)?;
    let active = ActiveDatabase::open(path)?;
    let proof = pre_restore::create_bound(maintenance, &active, cancel)
        .map_err(ContextError::Protection)?;
    let mut context = RestoreContext {
        terminal_policy: Some(&scope.terminal_policy),
        maintenance,
        active,
        incoming: candidate,
        protection: None,
        generation,
        next_generation,
        claimed: false,
    };
    context.attach(proof)?;
    action(context)
}
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;

pub(crate) mod pending;

mod close;

// macOS staged activation. Ready requires the verified startup completion.
#[cfg(target_os = "macos")]
pub(crate) mod activation;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod migration;
