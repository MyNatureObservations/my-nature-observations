//! Process-local coordination. Normal operations may run concurrently; maintenance
//! waits for all their permits. No database metadata or user data is modified.
use serde::Serialize;
use std::sync::{
    OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

pub(crate) const GENERATION_CONFLICT: &str = "journal_generation_conflict";
pub(crate) const RECOVERY_REQUIRED: &str = "database_recovery_required";
const MAINTENANCE: &str = "database_maintenance";

// Completing is deliberately absent until a verified completion protocol exists.
// There is no public phase setter, and destructors cannot grant Ready.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Starting,
    Recovery,
    Ready,
    Blocked,
}

pub(crate) struct DatabaseState {
    startup_started: AtomicBool,
    // One immutable ownership slot, irrespective of the startup verdict. Future
    // phase transitions must never move/drop its directory lease.
    directory: OnceLock<crate::startup::StartupOutcome>,
    phase: AtomicU8,
    startup_admitted: AtomicBool,
    session_active: AtomicBool,
    generation: RwLock<u32>,
    maintenance: AtomicBool,
    recovery_required: AtomicBool,
    #[cfg(test)]
    legacy_protocol_fixture: AtomicBool,
    #[cfg(test)]
    on_blocked: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
impl Default for DatabaseState {
    fn default() -> Self {
        Self {
            startup_started: AtomicBool::new(false),
            directory: OnceLock::new(),
            phase: AtomicU8::new(Phase::Starting as u8),
            startup_admitted: AtomicBool::new(false),
            session_active: AtomicBool::new(false),
            generation: RwLock::new(1),
            maintenance: AtomicBool::new(false),
            recovery_required: AtomicBool::new(false),
            #[cfg(test)]
            legacy_protocol_fixture: AtomicBool::new(false),
            #[cfg(test)]
            on_blocked: std::sync::Mutex::new(None),
        }
    }
}
#[derive(Serialize)]
pub(crate) struct Snapshot<T: Serialize> {
    pub generation: u32,
    pub data: T,
}
pub(crate) struct Operation<'a>(RwLockReadGuard<'a, u32>);
impl Operation<'_> {
    pub fn snapshot<T: Serialize>(&self, data: T) -> Snapshot<T> {
        Snapshot {
            generation: *self.0,
            data,
        }
    }
}
impl DatabaseState {
    pub(crate) fn permits_legacy_protocol_fixture(&self) -> bool {
        #[cfg(test)]
        {
            return self.legacy_protocol_fixture.load(Ordering::Acquire);
        }
        #[cfg(not(test))]
        false
    }

    pub(crate) fn initialize_startup(
        &self,
        directory: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        if self.startup_started.swap(true, Ordering::AcqRel) {
            return Err("database_startup_already_attempted".into());
        }
        let outcome = match crate::startup::begin(directory) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.phase.store(Phase::Blocked as u8, Ordering::Release);
                return Err(error);
            }
        };
        let blocked_reason = match &outcome {
            crate::startup::StartupOutcome::Recovery(gate) => gate.reason().to_owned(),
            crate::startup::StartupOutcome::Ready(_) => String::new(),
        };
        let (phase, path) = match &outcome {
            crate::startup::StartupOutcome::Ready(ready) => {
                (Phase::Ready, Some(ready.database_path()))
            }
            crate::startup::StartupOutcome::Recovery(_) => (Phase::Recovery, None),
        };
        self.directory
            .set(outcome)
            .map_err(|_| "database_startup_already_attempted")?;
        self.phase.store(phase as u8, Ordering::Release);
        path.ok_or(blocked_reason)
    }
    // Shell status only; neither method changes phase or grants admission.
    pub(crate) fn startup_recovery_pending(&self) -> bool {
        self.phase.load(Ordering::Acquire) == Phase::Recovery as u8
            && !self.startup_admitted.load(Ordering::Acquire)
            && matches!(
                self.directory.get(),
                Some(crate::startup::StartupOutcome::Recovery(_))
            )
    }
    pub(crate) fn ready_database_path(&self) -> Result<std::path::PathBuf, String> {
        let _operation = self.operation(None)?;
        Ok(self.lease()?.database_path())
    }
    fn lease(&self) -> Result<&crate::startup::DirectoryLease, String> {
        match self.directory.get().ok_or("database_startup_blocked")? {
            crate::startup::StartupOutcome::Ready(ready) => ready.lease(),
            crate::startup::StartupOutcome::Recovery(gate) => gate.lease(),
        }
    }
    fn normal_startup(&self) -> Result<(), String> {
        // Publication of the owner precedes Ready; that intermediate window
        // must not admit even maintenance. A retained Recovery verdict requires
        // the separate verified Startup admission as well as Ready.
        let phase = self.phase.load(Ordering::Acquire);
        if phase != Phase::Ready as u8 && phase != Phase::Recovery as u8 {
            return Err("database_startup_blocked".into());
        }
        match self.directory.get().ok_or("database_startup_blocked")? {
            crate::startup::StartupOutcome::Ready(ready) => ready.ensure_current(),
            crate::startup::StartupOutcome::Recovery(gate)
                if self.startup_admitted.load(Ordering::Acquire) && phase == Phase::Ready as u8 =>
            {
                gate.lease().map(|_| ())
            }
            crate::startup::StartupOutcome::Recovery(_) => Err("database_startup_blocked".into()),
        }
    }
    pub(crate) fn recovery_session(
        &self,
    ) -> Result<recovery_session::RecoverySession<'_, '_>, String> {
        if self.phase.load(Ordering::Acquire) != Phase::Recovery as u8
            || self.startup_admitted.load(Ordering::Acquire)
            || !matches!(
                self.directory.get(),
                Some(crate::startup::StartupOutcome::Recovery(_))
            )
        {
            return Err("database_recovery_session_unavailable".into());
        }
        recovery_session::RecoverySession::startup(self)
    }
    #[cfg(test)]
    pub(crate) fn assert_startup_recovery_generation_for_test_poisoned(&self, value: u32) {
        assert_eq!(
            *self.generation.read().unwrap_or_else(|e| e.into_inner()),
            value
        );
        assert_eq!(self.phase.load(Ordering::Acquire), Phase::Recovery as u8);
        assert!(!self.startup_admitted.load(Ordering::Acquire));
    }
    #[cfg(test)]
    pub(crate) fn test_startup_reserved(&self, value: bool) {
        self.session_active.store(value, Ordering::Release);
    }
    #[cfg(test)]
    pub(crate) fn test_startup_owner(&self) -> (*const (), *const ()) {
        (
            std::ptr::from_ref(self.directory.get().unwrap()).cast(),
            std::ptr::from_ref(self.lease().unwrap()).cast(),
        )
    }
    // The immutable RecoveryGate/lease remains in its original OnceLock slot.
    // This internal entry point is deliberately not wired to startup or commands.
    #[allow(dead_code)] // Intentionally internal and not connected to production startup.
    pub(crate) fn admit_startup_verified<'s>(
        &'s self,
        handoff: crate::backup::restore_context::pending::classification::StartupHandoff<'s>,
    ) -> Result<
        (),
        crate::backup::restore_context::pending::classification::StartupAdmissionFailure<'s>,
    > {
        if !handoff.matches_startup_target(self) {
            return Err(handoff.reject_binding());
        }
        let generation = match self.generation.write() {
            Ok(guard) => guard,
            Err(_) => return Err(handoff.reject_coordination()),
        };
        let validated = handoff.validate_startup(self, *generation)?;
        // Commit point: no I/O, allocation, callbacks, fallible locks or cancellation.
        // No generation bump: this fresh process has never admitted an operation.
        self.startup_admitted.store(true, Ordering::Release);
        self.recovery_required.store(false, Ordering::Release);
        self.phase.store(Phase::Ready as u8, Ordering::Release);
        drop(validated);
        drop(generation);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn ready_for_test() -> Self {
        let state = Self::default();
        state.startup_started.store(true, Ordering::Release);
        assert!(
            state
                .directory
                .set(crate::startup::StartupOutcome::Ready(
                    crate::startup::StartupReady::for_test()
                ))
                .is_ok()
        );
        state.phase.store(Phase::Ready as u8, Ordering::Release);
        state
    }

    pub fn operation(&self, expected: Option<u32>) -> Result<Operation<'_>, String> {
        // Test hook fires only after an actual failed lock attempt. It is absent
        // from production builds and does not change their blocking read().
        #[cfg(test)]
        {
            let hook = self.on_blocked.lock().unwrap().take();
            if let Some(hook) = hook {
                match self.generation.try_read() {
                    Ok(generation) => return self.admit(generation, expected),
                    Err(std::sync::TryLockError::WouldBlock) => hook(),
                    Err(_) => return Err("database_coordination_failed".into()),
                }
            }
        }
        // Validate only AFTER acquiring the same lock used by maintenance.
        let generation = self
            .generation
            .read()
            .map_err(|_| "database_coordination_failed")?;
        self.admit(generation, expected)
    }

    fn admit<'a>(
        &self,
        generation: RwLockReadGuard<'a, u32>,
        expected: Option<u32>,
    ) -> Result<Operation<'a>, String> {
        self.normal_startup()?;
        if expected.is_some_and(|expected| expected != *generation) {
            return Err(GENERATION_CONFLICT.into());
        }
        if self.phase.load(Ordering::Acquire) != Phase::Ready as u8
            || self.recovery_required.load(Ordering::Acquire)
        {
            return Err(RECOVERY_REQUIRED.into());
        }
        if self.maintenance.load(Ordering::Acquire) {
            return Err(MAINTENANCE.into());
        }
        Ok(Operation(generation))
    }

    // Test observation only: the single-threaded normal-open fixture has no
    // Maintenance writer. A failed try_write therefore observes its live read guard.
    #[cfg(test)]
    pub(crate) fn assert_normal_operation_held_for_test(&self) {
        assert!(!self.maintenance.load(Ordering::Acquire));
        assert!(matches!(
            self.generation.try_write(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
    }

    #[cfg(test)]
    pub(crate) fn set_generation_for_test(&self, generation: u32) {
        *self.generation.write().unwrap() = generation;
    }

    #[cfg(test)]
    pub(crate) fn on_blocked(&self, hook: impl FnOnce() + Send + 'static) {
        *self.on_blocked.lock().unwrap() = Some(Box::new(hook));
    }

    // Internal API only: no maintenance or generation-changing Tauri command.
    // Reserve maintenance before waiting so new operations cannot overtake it.
    #[allow(dead_code)]
    pub fn maintenance(&self) -> Result<Maintenance<'_>, String> {
        self.normal_startup()?;
        self.maintenance
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| MAINTENANCE)?;
        match self.generation.write() {
            Ok(generation) => Ok(Maintenance {
                state: self,
                generation,
                operation_id: uuid::Uuid::new_v4(),
            }),
            Err(_) => {
                self.maintenance.store(false, Ordering::Release);
                Err("database_coordination_failed".into())
            }
        }
    }
}
pub(crate) struct Maintenance<'a> {
    state: &'a DatabaseState,
    generation: RwLockWriteGuard<'a, u32>,
    operation_id: uuid::Uuid,
}
#[allow(dead_code)]
impl<'s> Maintenance<'s> {
    #[cfg(test)]
    pub(crate) fn test_admission_state(&mut self, generation: Option<u32>, reserved: Option<bool>) {
        if let Some(value) = generation {
            *self.generation = value;
        }
        if let Some(value) = reserved {
            self.state.session_active.store(value, Ordering::Release);
        }
    }
    // Factory uses this guard's own DatabaseState; arbitrary lease/state pairs
    // cannot be supplied, and the existing write guard is borrowed, not reacquired.
    pub(crate) fn recovery_session(
        &self,
    ) -> Result<recovery_session::RecoverySession<'_, 's>, String> {
        if !cfg!(target_os = "macos") {
            return Err("database_recovery_unsupported".into());
        }
        let session = recovery_session::RecoverySession::running(self)?;
        self.require_recovery();
        Ok(session)
    }
    pub(crate) fn operation_id(&self) -> uuid::Uuid {
        self.operation_id
    }
    pub(crate) fn generation(&self) -> u32 {
        *self.generation
    }
    pub(crate) fn checked_next_generation(&self) -> Result<u32, String> {
        self.generation
            .checked_add(1)
            .ok_or_else(|| "database_generation_exhausted".into())
    }
    pub(crate) fn ensure_usable(&self) -> Result<(), String> {
        if self.state.recovery_required.load(Ordering::Acquire) {
            Err(RECOVERY_REQUIRED.into())
        } else {
            Ok(())
        }
    }
    // Future restore may leave this in-process latch set after dropping maintenance.
    // No reset API until verified recovery is implemented; no on-disk marker here.
    #[allow(dead_code)]
    pub(crate) fn require_recovery(&self) {
        self.state.recovery_required.store(true, Ordering::Release);
        self.state
            .phase
            .store(Phase::Recovery as u8, Ordering::Release);
    }

    // The write guard predates recovery and is never reacquired. No Startup path.
    pub(crate) fn resume_verified(
        &mut self,
        handoff: crate::backup::restore_context::pending::classification::RunningHandoff<'s>,
    ) -> Result<
        (),
        crate::backup::restore_context::pending::classification::RunningAdmissionFailure<'s>,
    > {
        let next = match self.generation.checked_add(1) {
            Some(next) => next,
            None => return Err(handoff.reject_overflow()),
        };
        let validated = handoff.validate_running(self)?;
        // Commit point. Integer assignment and atomic stores only. All I/O,
        // allocation, hooks, cancellation and fallible locking precede this.
        *self.generation = next;
        self.state.recovery_required.store(false, Ordering::Release);
        self.state
            .phase
            .store(Phase::Ready as u8, Ordering::Release);
        // Closed files and reservation drop while this write guard still lives.
        drop(validated);
        Ok(())
    }

    // Future restore must invalidate old sessions before releasing exclusivity.
    #[allow(dead_code)]
    pub fn advance_generation(&mut self) -> Result<(), String> {
        *self.generation = self
            .generation
            .checked_add(1)
            .ok_or("database_generation_exhausted")?;
        Ok(())
    }
}
impl Drop for Maintenance<'_> {
    fn drop(&mut self) {
        self.state.maintenance.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub(crate) fn new_journal_session(
    state: tauri::State<'_, DatabaseState>,
) -> Result<Snapshot<()>, String> {
    Ok(state.operation(None)?.snapshot(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn recovery_outcome_cannot_be_admitted_by_phase_alone() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("book");
        let initial = DatabaseState::default();
        initial.initialize_startup(&root).unwrap();
        drop(initial);
        std::fs::write(root.join(".mno-completion-orphan"), b"block").unwrap();
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        state.phase.store(Phase::Ready as u8, Ordering::Release);
        assert!(state.operation(None).is_err());
        assert!(state.maintenance().is_err());
        assert!(!state.startup_admitted.load(Ordering::Acquire));
        assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
    }

    #[test]
    fn maintenance_waits_for_operations_and_reserves_exclusivity() {
        let state = Arc::new(DatabaseState::ready_for_test());
        let operation = state.operation(None).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("writer.sqlite");
        let mut connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE value(content TEXT); INSERT INTO value VALUES ('A');")
            .unwrap();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute("UPDATE value SET content = 'B'", [])
            .unwrap();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let other = state.clone();
        let worker = thread::spawn(move || {
            let mut maintenance = other.maintenance().unwrap();
            let connection = rusqlite::Connection::open(path).unwrap();
            let content: String = connection
                .query_row("SELECT content FROM value", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                content, "B",
                "the ongoing write must commit before maintenance"
            );
            acquired_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            maintenance.advance_generation().unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !state.maintenance.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "maintenance reservation timed out"
            );
            thread::yield_now();
        }
        assert!(acquired_rx.try_recv().is_err());
        assert_eq!(operation.snapshot(()).generation, 1);
        transaction.commit().unwrap();
        drop(connection);
        drop(operation);
        acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            state.generation.try_read(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(state.operation(None).unwrap().snapshot(()).generation, 2);
    }

    #[test]
    fn queued_save_checks_generation_after_maintenance_releases_lock() {
        let state = Arc::new(DatabaseState::ready_for_test());
        let mut maintenance = state.maintenance().unwrap();
        let (queued_tx, queued_rx) = mpsc::channel();
        state.on_blocked(move || queued_tx.send(()).unwrap());
        let other = state.clone();
        let worker = thread::spawn(move || other.operation(Some(1)).err().unwrap());
        queued_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        maintenance.advance_generation().unwrap();
        drop(maintenance);
        assert_eq!(worker.join().unwrap(), GENERATION_CONFLICT);
        assert!(state.operation(Some(2)).is_ok());
    }

    #[test]
    fn reservation_rejects_new_operations_and_cancel_preserves_generation() {
        let state = DatabaseState::ready_for_test();
        // This is the reservation state while maintenance waits for old operations.
        state.maintenance.store(true, Ordering::Release);
        assert_eq!(state.operation(Some(1)).err().unwrap(), MAINTENANCE);
        state.maintenance.store(false, Ordering::Release);
        drop(state.maintenance().unwrap());
        assert_eq!(state.operation(None).unwrap().snapshot(()).generation, 1);
        assert!(state.operation(Some(1)).is_ok());
    }
}

pub(crate) mod recovery_session;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod sidecar_startup_tests {
    use super::*;
    #[test]
    fn sidecars_keep_recovery_phase_generation_and_all_operation_gates() {
        for suffix in ["-journal", "-wal", "-shm"] {
            let dir = tempfile::TempDir::new().unwrap();
            let root = dir.path().join("book");
            let initial = DatabaseState::default();
            let path = initial.initialize_startup(&root).unwrap();
            drop(initial);
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            std::fs::write(std::path::Path::new(&name), b"preserve").unwrap();
            let state = DatabaseState::default();
            state.set_generation_for_test(37);
            assert_eq!(
                state.initialize_startup(&root).unwrap_err(),
                "database_sqlite_sidecar_present"
            );
            assert_eq!(*state.generation.read().unwrap(), 37);
            assert_eq!(state.phase.load(Ordering::Acquire), Phase::Recovery as u8);
            assert!(state.operation(None).is_err());
            assert!(state.operation(Some(37)).is_err());
            assert!(state.maintenance().is_err());
            assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
            if cfg!(target_os = "macos") {
                let session = state.recovery_session().unwrap();
                drop(session);
            }
            assert_eq!(*state.generation.read().unwrap(), 37);
            assert_eq!(state.phase.load(Ordering::Acquire), Phase::Recovery as u8);
            assert!(state.operation(None).is_err());
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) mod maintenance_retention;

// Admission accepts only the verified, one-use startup activation owner.
#[cfg(target_os = "macos")]
impl DatabaseState {
    pub(crate) fn admit_activation_verified(
        &self,
        proof: crate::backup::restore_context::activation::startup::VerifiedStartupActivation<'_>,
    ) -> Result<(), String> {
        if !proof.matches(self) {
            return Err("activation_owner_mismatch".into());
        }
        let generation = self
            .generation
            .write()
            .map_err(|_| "database_coordination_failed")?;
        proof
            .check(self, *generation)
            .map_err(|_| "activation_verification_failed")?;
        // Every fallible check precedes this exact startup-only commit point.
        self.startup_admitted.store(true, Ordering::Release);
        self.recovery_required.store(false, Ordering::Release);
        self.phase.store(Phase::Ready as u8, Ordering::Release);
        drop(proof);
        drop(generation);
        Ok(())
    }
}

#[cfg(test)]
impl DatabaseState {
    // Frozen pre-upgrade startup for old protocol fixtures. All directory locks,
    // artifact gates, sidecar checks and ownership paths remain the real ones.
    // New schema/recovery tests call initialize_startup, never this helper.
    pub(crate) fn initialize_legacy_startup(
        &self,
        root: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        self.legacy_protocol_fixture.store(true, Ordering::Release);
        if crate::startup::migration_probe::has_initializer() {
            return self.initialize_startup(root);
        }
        fn legacy(path: &std::path::Path, new: bool) -> Result<(), String> {
            if new {
                return crate::database::create_legacy_database(path);
            }
            let db = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .map_err(|e| e.to_string())?;
            match crate::database::validate_schema(&db) {
                Ok(true) => Ok(()),
                Ok(false) => Err("database_schema_incompatible".into()),
                Err(e) => Err(e.to_string()),
            }
        }
        crate::startup::migration_probe::with_initializer(legacy, || self.initialize_startup(root))
    }

    pub(crate) fn initialize_weather_stage1_startup(
        &self,
        root: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        self.legacy_protocol_fixture.store(true, Ordering::Release);
        fn schema1(path: &std::path::Path, new: bool) -> Result<(), String> {
            if new {
                crate::database::create_new_database(path)
            } else {
                crate::database::initialize_schema1_database_for_test(path)
            }
        }
        crate::startup::migration_probe::with_initializer(schema1, || self.initialize_startup(root))
    }
}
