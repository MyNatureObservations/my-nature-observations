//! A blocked borrow of the one directory lease. No proof or unlock capability.
use super::{DatabaseState, Maintenance};
use crate::startup::DirectoryLease;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

// 'a is the Maintenance borrow; 's is the state/lease and reservation lifetime.
// Keeping them distinct grants no authority to detach the retained session.
pub(crate) struct RecoverySession<'a, 's> {
    access: Access<'a, 's>,
    session_id: uuid::Uuid,
    _reservation: Reservation<'s>,
}
enum Access<'a, 's> {
    Startup(&'s DatabaseState),
    Running { maintenance: &'a Maintenance<'s> },
}
impl<'a, 's> RecoverySession<'a, 's> {
    // Only DatabaseState/Maintenance factories may choose the trusted owner.
    pub(super) fn startup(state: &'s DatabaseState) -> Result<Self, String> {
        if !cfg!(target_os = "macos") {
            return Err("database_recovery_unsupported".into());
        }
        state.lease()?;
        Ok(Self {
            access: Access::Startup(state),
            session_id: uuid::Uuid::new_v4(),
            _reservation: Reservation::acquire(&state.session_active)?,
        })
    }
    pub(super) fn running(maintenance: &'a Maintenance<'s>) -> Result<Self, String> {
        if !cfg!(target_os = "macos") {
            return Err("database_recovery_unsupported".into());
        }
        maintenance.state.lease()?;
        Ok(Self {
            access: Access::Running { maintenance },
            session_id: uuid::Uuid::new_v4(),
            _reservation: Reservation::acquire(&maintenance.state.session_active)?,
        })
    }
    // This is a checked startup-only observation, not an unlock capability.
    // Check the origin before taking generation's read lock: Running already
    // borrows its Maintenance write guard and must never reacquire that lock.
    pub(crate) fn startup_generation(&self) -> Result<u32, String> {
        self.ensure_publication_phase()?;
        let Access::Startup(state) = &self.access else {
            return Err("database_recovery_startup_required".into());
        };
        let generation = *state
            .generation
            .read()
            .map_err(|_| "database_coordination_failed")?;
        self.ensure_publication_phase()?;
        Ok(generation)
    }
    pub(crate) fn directory_identity(&self) -> Result<(u64, u64), String> {
        self.lease()?.directory_identity()
    }
    pub(crate) fn database_path(&self) -> Result<PathBuf, String> {
        Ok(self.lease()?.database_path())
    }
    pub(crate) fn ensure_publication_phase(&self) -> Result<(), String> {
        let state = match &self.access {
            Access::Startup(state) => *state,
            Access::Running { maintenance } => maintenance.state,
        };
        if state.phase.load(Ordering::Acquire) != super::Phase::Recovery as u8
            || !state.session_active.load(Ordering::Acquire)
        {
            return Err("database_recovery_session_invalid".into());
        }
        self.lease()?.ensure_current()
    }
    pub(crate) fn ensure_current(&self) -> Result<(), String> {
        self.lease()?.ensure_current()
    }
    fn lease(&self) -> Result<&DirectoryLease, String> {
        match &self.access {
            Access::Startup(state) => state.lease(),
            Access::Running { maintenance } => maintenance.state.lease(),
        }
    }
    pub(crate) fn session_id(&self) -> uuid::Uuid {
        self.session_id
    }
    pub(crate) fn maintenance_binding(&self) -> Option<(uuid::Uuid, u32)> {
        match &self.access {
            Access::Startup(_) => None,
            Access::Running { maintenance, .. } => {
                Some((maintenance.operation_id(), maintenance.generation()))
            }
        }
    }
}
// Drop releases only the session slot and borrow. The owner retains the OS lock; neither startup
// readiness nor the recovery latch is changed. No destructor unlocks recovery.

// Inert detached session ownership. No SQL, admission or generation authority.
// Raw Maintenance identity is equality-only and never dereferenced.
#[allow(dead_code)] // Retained for later admission; no admission API exists yet.
pub(crate) struct DetachedSession<'s> {
    state: &'s DatabaseState,
    id: uuid::Uuid,
    operation: Option<uuid::Uuid>,
    generation: u32,
    maintenance: Option<*const ()>,
    reservation: Reservation<'s>,
}
pub(crate) struct DetachedSessionBinding<'s> {
    state: &'s DatabaseState,
    id: uuid::Uuid,
    operation: Option<uuid::Uuid>,
    generation: u32,
    maintenance: Option<*const ()>,
    slot: *const (),
}
impl<'s> RecoverySession<'_, 's> {
    pub(crate) fn detach_binding(&self) -> Result<DetachedSessionBinding<'s>, String> {
        self.ensure_publication_phase()?;
        let (state, operation, generation, maintenance) = match &self.access {
            Access::Running { maintenance } => (
                maintenance.state,
                Some(maintenance.operation_id()),
                maintenance.generation(),
                Some(std::ptr::from_ref(*maintenance).cast()),
            ),
            Access::Startup(state) => (*state, None, self.startup_generation()?, None),
        };
        Ok(DetachedSessionBinding {
            state,
            id: self.session_id,
            operation,
            generation,
            maintenance,
            slot: std::ptr::from_ref(self._reservation.0).cast(),
        })
    }
    pub(crate) fn into_detached_session(
        self,
        binding: DetachedSessionBinding<'s>,
    ) -> DetachedSession<'s> {
        // Binding was produced from this actual session before any owner split.
        assert_eq!(self.session_id, binding.id);
        assert_eq!(std::ptr::from_ref(self._reservation.0).cast(), binding.slot);
        let Self {
            access: _,
            session_id: _,
            _reservation,
        } = self;
        let DetachedSessionBinding {
            state,
            id,
            operation,
            generation,
            maintenance,
            slot: _,
        } = binding;
        DetachedSession {
            state,
            id,
            operation,
            generation,
            maintenance,
            reservation: _reservation,
        }
    }
    #[cfg(test)]
    pub(crate) fn test_reservation_slot(&self) -> *const () {
        std::ptr::from_ref(self._reservation.0).cast()
    }
}
#[allow(dead_code)]
impl DetachedSession<'_> {
    pub(crate) fn session_id(&self) -> uuid::Uuid {
        self.id
    }
    pub(crate) fn slot(&self) -> *const () {
        std::ptr::from_ref(self.reservation.0).cast()
    }
    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }
    pub(crate) fn matches_running(&self, maintenance: &Maintenance<'_>) -> bool {
        self.maintenance == Some(std::ptr::from_ref(maintenance).cast())
            && std::ptr::eq(self.state, maintenance.state)
            && self.operation == Some(maintenance.operation_id())
            && self.generation == maintenance.generation()
            && self.slot() == std::ptr::from_ref(&maintenance.state.session_active).cast()
    }
    pub(crate) fn check_running(&self, m: &Maintenance<'_>) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("database_recovery_unsupported".into());
        }
        if !self.matches_running(m)
            || self.state.phase.load(Ordering::Acquire) != super::Phase::Recovery as u8
            || !self.state.recovery_required.load(Ordering::Acquire)
            || !self.state.maintenance.load(Ordering::Acquire)
            || !self.state.session_active.load(Ordering::Acquire)
        {
            return Err("database_recovery_admission_binding".into());
        }
        self.state.lease()?.ensure_current()
    }
    #[cfg(test)]
    pub(crate) fn test_startup_corrupt(&mut self, case: u8) {
        match case {
            0 => self.operation = Some(uuid::Uuid::new_v4()),
            1 => self.maintenance = Some(std::ptr::dangling::<()>()),
            2 => self.generation = self.generation.wrapping_add(1),
            _ => unreachable!(),
        }
    }
    // Nonblocking preflight rejects a foreign/Running owner before waiting for
    // its generation lock (which a caller's Maintenance may already hold).
    pub(crate) fn matches_startup_target(&self, state: &DatabaseState) -> bool {
        std::ptr::eq(self.state, state)
            && self.operation.is_none()
            && self.maintenance.is_none()
            && self.slot() == std::ptr::from_ref(&state.session_active).cast()
            && state.session_active.load(Ordering::Acquire)
            && !state.maintenance.load(Ordering::Acquire)
            && !state.startup_admitted.load(Ordering::Acquire)
            && state.phase.load(Ordering::Acquire) == super::Phase::Recovery as u8
            && matches!(
                state.directory.get(),
                Some(crate::startup::StartupOutcome::Recovery(_))
            )
    }
    // Caller holds generation's write guard; never reacquire it here.
    pub(crate) fn check_startup(
        &self,
        state: &DatabaseState,
        generation: u32,
    ) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("database_recovery_unsupported".into());
        }
        if !self.matches_startup_target(state) || self.generation != generation {
            return Err("database_recovery_admission_binding".into());
        }
        state.lease()?.ensure_current()
    }
    pub(crate) fn directory_identity(&self) -> Result<(u64, u64), String> {
        self.state.lease()?.directory_identity()
    }
    pub(crate) fn database_path(&self) -> Result<PathBuf, String> {
        Ok(self.state.lease()?.database_path())
    }
    pub(crate) fn running(&self) -> bool {
        self.operation.is_some()
    }
}
// A detached, inert copy of verified context binding fields, not a new identity.
pub(crate) struct ClosedContextBinding<'s> {
    _state: &'s DatabaseState,
    _session_id: uuid::Uuid,
    _operation_id: uuid::Uuid,
    _generation: u32,
    _directory: (u64, u64),
    _database_path: PathBuf,
    _maintenance: *const (),
}
impl<'s> ContextSessionBinding<'_, 's> {
    pub(crate) fn into_closed_record(self) -> ClosedContextBinding<'s> {
        let Self {
            session_id,
            operation_id,
            generation,
            directory,
            database_path,
            maintenance,
        } = self;
        ClosedContextBinding {
            _state: maintenance.state,
            _session_id: session_id,
            _operation_id: operation_id,
            _generation: generation,
            _directory: directory,
            _database_path: database_path,
            _maintenance: std::ptr::from_ref(maintenance).cast(),
        }
    }
}

#[cfg(test)]
pub(crate) type TestSession<'s> = DetachedSession<'s>;
#[cfg(test)]
pub(crate) type TestSessionBinding<'s> = DetachedSessionBinding<'s>;
#[cfg(test)]
impl<'s> RecoverySession<'_, 's> {
    pub(crate) fn test_transfer_binding(&self) -> Result<TestSessionBinding<'s>, String> {
        self.detach_binding()
    }
    pub(crate) fn into_test_session(self, binding: TestSessionBinding<'s>) -> TestSession<'s> {
        self.into_detached_session(binding)
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

// This slot belongs to the same DatabaseState as the retained lease. Construction
// is private; moving the session moves its reservation, including into proof/error
// wrappers. Releasing a slot never changes phase, generation, or the OS lock.
struct Reservation<'s>(&'s AtomicBool);
impl<'s> Reservation<'s> {
    fn acquire(slot: &'s AtomicBool) -> Result<Self, String> {
        slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "database_recovery_busy")?;
        Ok(Self(slot))
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        #[cfg(test)]
        reservation_drop_probe::run();
        self.0.store(false, Ordering::Release);
    }
}

// An inert record issued only from the real, reserved Running session. It borrows
// Maintenance, not RecoverySession: moving the same session into Assessment must
// remain possible. A future consumer must revalidate against that retained session.
pub(crate) struct ContextSessionBinding<'a, 's> {
    session_id: uuid::Uuid,
    operation_id: uuid::Uuid,
    generation: u32,
    directory: (u64, u64),
    database_path: PathBuf,
    maintenance: &'a Maintenance<'s>,
}
impl RecoverySession<'_, '_> {
    pub(crate) fn bind_context<'a, 's>(
        &self,
        maintenance: &'a Maintenance<'s>,
    ) -> Result<ContextSessionBinding<'a, 's>, String> {
        self.ensure_publication_phase()?;
        let Access::Running {
            maintenance: actual,
        } = &self.access
        else {
            return Err("database_context_session_origin".into());
        };
        // UUID equality alone is insufficient: require the identical live guard.
        if !std::ptr::eq(*actual, maintenance)
            || !std::ptr::eq(self._reservation.0, &maintenance.state.session_active)
        {
            return Err("database_context_session_maintenance".into());
        }
        let lease = self.lease()?;
        let binding = ContextSessionBinding {
            session_id: self.session_id,
            operation_id: maintenance.operation_id(),
            generation: maintenance.generation(),
            directory: lease.directory_identity()?,
            database_path: lease.database_path(),
            maintenance,
        };
        self.ensure_publication_phase()?;
        Ok(binding)
    }
}
impl ContextSessionBinding<'_, '_> {
    #[cfg(test)]
    pub(crate) fn test_set_session_id(&mut self, id: uuid::Uuid) {
        self.session_id = id;
    }
    pub(crate) fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }
    pub(crate) fn operation_id(&self) -> uuid::Uuid {
        self.operation_id
    }
    // No authority is returned. In particular an old record cannot be relabelled
    // with a successive session's UUID, even under the same Maintenance guard.
    pub(crate) fn check_session(&self, session: &RecoverySession<'_, '_>) -> Result<(), String> {
        let current = session.bind_context(self.maintenance)?;
        if self.session_id != current.session_id
            || self.operation_id != current.operation_id
            || self.generation != current.generation
            || self.directory != current.directory
            || self.database_path != current.database_path
        {
            return Err("database_context_session_changed".into());
        }
        Ok(())
    }
}

// Test-only observation at the final field's Drop boundary. No release API.
#[cfg(test)]
pub(crate) mod reservation_drop_probe {
    use std::cell::RefCell;
    thread_local! { static HOOK: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None); }
    pub(super) fn run() {
        let hook = HOOK.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook();
        }
    }
    pub(crate) fn with<T>(hook: impl FnOnce() + 'static, action: impl FnOnce() -> T) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                HOOK.with(|slot| {
                    slot.borrow_mut().take();
                });
            }
        }
        HOOK.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Box::new(hook));
        });
        let _reset = Reset;
        action()
    }
}
