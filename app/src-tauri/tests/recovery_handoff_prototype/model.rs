//! Test-only ownership model, not a recovery or disk-completion authorization.
//! The opaque lease stands for the one state-owned lease; no OS lock or SQL is opened.
use std::sync::{
    RwLock, RwLockWriteGuard,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub struct State {
    reserved: AtomicBool,
    next_session: AtomicU64,
    generation: RwLock<u32>,
    lease: Lease,
}
struct Lease;
impl State {
    pub fn new() -> Self {
        Self {
            reserved: AtomicBool::new(false),
            next_session: AtomicU64::new(1),
            generation: RwLock::new(1),
            lease: Lease,
        }
    }
    pub fn maintenance(&self) -> Maintenance<'_> {
        Maintenance {
            state: self,
            generation: self.generation.write().unwrap(),
        }
    }
    pub fn reserved(&self) -> bool {
        self.reserved.load(Ordering::Acquire)
    }
    pub fn ready(&self) -> bool {
        false
    } // This model contains no admission operation.
}
pub struct Maintenance<'state> {
    state: &'state State,
    generation: RwLockWriteGuard<'state, u32>,
}
impl<'state> Maintenance<'state> {
    pub fn session<'m>(&'m self) -> Result<RecoverySession<'m, 'state>, &'static str> {
        let reservation = Reservation::acquire(self.state)?;
        let id = self
            .state
            .next_session
            .try_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| "exhausted")?;
        Ok(RecoverySession {
            maintenance: self,
            reservation,
            id,
        })
    }
    pub fn generation(&self) -> u32 {
        *self.generation
    }
    // Requires the same exclusive borrow as the real advance_generation, but
    // deliberately changes neither the real nor the modeled generation.
    pub fn exclusive_probe(&mut self) {}
}
struct Reservation<'state> {
    state: &'state State,
}
impl<'state> Reservation<'state> {
    fn acquire(state: &'state State) -> Result<Self, &'static str> {
        state
            .reserved
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "busy")?;
        Ok(Self { state })
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.state.reserved.store(false, Ordering::Release);
    }
}
pub struct RecoverySession<'m, 'state> {
    maintenance: &'m Maintenance<'state>,
    reservation: Reservation<'state>,
    id: u64,
}
// Model every independent Maintenance borrow in the real closed chain,
// including the PhantomData borrow held by ProtectionRecord.
struct ContextSessionBinding<'m, 'state>(&'m Maintenance<'state>);
struct ProtectionRecord<'m, 'state>(std::marker::PhantomData<&'m Maintenance<'state>>);
struct ContextConnectionsClosed<'m, 'state> {
    maintenance: &'m Maintenance<'state>,
    binding: ContextSessionBinding<'m, 'state>,
    protection: ProtectionRecord<'m, 'state>,
}
struct ResourcesClosed<'m, 'state>(RecoverySession<'m, 'state>);
struct AllConnectionsClosed<'m, 'state> {
    context: ContextConnectionsClosed<'m, 'state>,
    resources: ResourcesClosed<'m, 'state>,
}
pub struct FinalFilesVerified<'m, 'state>(AllConnectionsClosed<'m, 'state>);
impl<'m, 'state> RecoverySession<'m, 'state> {
    // Ownership simulation only. It does not assert actual files were verified.
    pub fn model_closed_chain(self) -> FinalFilesVerified<'m, 'state> {
        let maintenance = self.maintenance;
        FinalFilesVerified(AllConnectionsClosed {
            context: ContextConnectionsClosed {
                maintenance,
                binding: ContextSessionBinding(maintenance),
                protection: ProtectionRecord(std::marker::PhantomData),
            },
            resources: ResourcesClosed(self),
        })
    }
    pub fn id(&self) -> u64 {
        self.id
    }
}
pub struct RunningHandoff<'state> {
    reservation: Reservation<'state>,
    id: u64,
    generation: u32,
    lease: &'state Lease,
}
impl<'state> FinalFilesVerified<'_, 'state> {
    pub fn into_handoff(self) -> RunningHandoff<'state> {
        let AllConnectionsClosed {
            context,
            resources: ResourcesClosed(session),
        } = self.0;
        // End ALL maintenance-dependent records, not just the session borrow.
        let ContextConnectionsClosed {
            maintenance,
            binding,
            protection,
        } = context;
        assert!(std::ptr::eq(maintenance, binding.0));
        let _ = protection;
        let generation = maintenance.generation();
        let RecoverySession {
            maintenance,
            reservation,
            id,
        } = session;
        let lease = &maintenance.state.lease;
        RunningHandoff {
            reservation,
            id,
            generation,
            lease,
        }
    }
}
impl RunningHandoff<'_> {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
    pub fn reserved(&self) -> bool {
        self.reservation.state.reserved()
    }
    pub fn retains_lease(&self, state: &State) -> bool {
        std::ptr::eq(self.lease, &state.lease)
    }
    pub fn consume(self) {} // Releases only the slot; never admission.
}
