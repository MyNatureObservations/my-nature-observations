//! Test-only ownership experiment; no writer, SQL accessor, cleanup or admission.
use super::*;

struct OwnedPending {
    root: PathBuf,
    root_file: File,
    directory_file: File,
    marker_file: File,
    bytes: Vec<u8>,
    identities: Option<(Identity, Identity, Identity)>,
    operation: uuid::Uuid,
    generation: u32,
}
impl OwnedPending {
    // Infallible move of real returned publication handles. No callback runs
    // between the borrowed marker and the aggregate owner's pending field.
    fn consume(marker: PendingMarker<'_, '_>) -> Self {
        let PendingMarker {
            maintenance: _,
            active: _,
            root,
            root_file,
            directory_file,
            marker_file,
            bytes,
            operation,
            generation,
        } = marker;
        Self {
            root,
            root_file,
            directory_file,
            marker_file,
            bytes,
            identities: None,
            operation,
            generation,
        }
    }
    fn verify(&mut self, context: &RestoreContext<'_, '_>) -> MarkerResult<()> {
        use std::os::unix::fs::FileExt;
        if self.operation != context.maintenance.operation_id()
            || self.generation != context.maintenance.generation()
            || context.generation != self.generation
        {
            return Err(MarkerError::Binding);
        }
        context
            .active
            .ensure_current()
            .map_err(|_| MarkerError::Binding)?;
        let observed = (
            identity(&self.root_file)?,
            identity(&self.directory_file)?,
            identity(&self.marker_file)?,
        );
        if self
            .identities
            .as_ref()
            .is_some_and(|expected| expected != &observed)
            || observed.0 != identity(&open_plain(&self.root, true)?)?
            || observed.1 != identity(&open_plain(&self.root.join(DIRECTORY), true)?)?
            || observed.2 != identity(&open_plain(&self.root.join(DIRECTORY).join(NAME), false)?)?
            || self.marker_file.metadata()?.len() != self.bytes.len() as u64
        {
            return Err(MarkerError::Binding);
        }
        let mut bytes = vec![0; self.bytes.len()];
        self.marker_file.read_exact_at(&mut bytes, 0)?;
        if bytes != self.bytes {
            return Err(MarkerError::Binding);
        }
        let data = parse(&bytes)?;
        let (volume, file) = context.active.identity().components();
        if data.restore_id != self.operation.to_string()
            || data.directory_identity != observed.0
            || data.database_identity != Identity::from_parts(volume, file)
            || !context
                .protection
                .as_ref()
                .is_some_and(|p| p.is_bound_to(context.maintenance, &context.active))
        {
            return Err(MarkerError::Binding);
        }
        self.identities = Some(observed);
        Ok(())
    }
}
// Preserve the existing context destruction order, then release P's descriptors.
struct Retained<'m, 's> {
    context: RestoreContext<'m, 's>,
    pending: Option<OwnedPending>,
}
pub(crate) struct PendingWrite<'m, 's> {
    retained: Option<Retained<'m, 's>>,
}
impl std::fmt::Debug for PendingWrite<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingWrite").finish_non_exhaustive()
    }
}
impl Drop for PendingWrite<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Same conservative aggregate-retention policy as the recovery chain.
            // No callback or further access to leaked borrows is possible.
            std::mem::forget(self.retained.take());
        }
    }
}
struct Preparing<'m, 's>(Option<Retained<'m, 's>>);
impl Drop for Preparing<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::mem::forget(self.0.take());
        }
    }
}
#[derive(Debug)]
enum PrepareError {
    Context(ContextError),
    Marker(MarkerError),
    Checkpoint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Checkpoint {
    BeforeAuthorization,
    Publication(Point),
    DurablePending,
    BeforeMove,
    AfterMove,
}
pub(super) struct PrepareFailure<'m, 's> {
    retained: Retained<'m, 's>,
    error: PrepareError,
}
impl std::fmt::Debug for PrepareFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrepareFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
// The callback is borrowed: its destructor/panic payload is never owned by the
// prototype. No arbitrary callback destructor runs during an ownership move.
pub(super) fn prepare<'m, 's>(
    context: RestoreContext<'m, 's>,
    checkpoint: &mut impl FnMut(Checkpoint) -> io::Result<()>,
) -> std::result::Result<PendingWrite<'m, 's>, Box<PrepareFailure<'m, 's>>> {
    let mut owner = Preparing(Some(Retained {
        context,
        pending: None,
    }));
    let result = (|| {
        checkpoint(Checkpoint::BeforeAuthorization).map_err(|_| PrepareError::Checkpoint)?;
        let held = owner.0.as_mut().expect("preparation owner");
        let pending = {
            let authorization = held.context.authorize().map_err(PrepareError::Context)?;
            let marker = publish_with(&authorization, |p| checkpoint(Checkpoint::Publication(p)))
                .map_err(PrepareError::Marker)?;
            OwnedPending::consume(marker)
        }; // All authorization/ActiveDatabase loans end before context moves.
        held.pending = Some(pending);
        held.pending
            .as_mut()
            .expect("published handles")
            .verify(&held.context)
            .map_err(PrepareError::Marker)?;
        checkpoint(Checkpoint::DurablePending).map_err(|_| PrepareError::Checkpoint)?;
        checkpoint(Checkpoint::BeforeMove).map_err(|_| PrepareError::Checkpoint)
    })();
    if let Err(error) = result {
        return Err(Box::new(PrepareFailure {
            retained: owner.0.take().expect("preparation owner"),
            error,
        }));
    }
    // No field-by-field extraction, allocation or callback between the owners.
    let mut prepared = PendingWrite {
        retained: owner.0.take(),
    };
    if checkpoint(Checkpoint::AfterMove).is_err() {
        return Err(Box::new(PrepareFailure {
            retained: prepared.retained.take().expect("prepared owner"),
            error: PrepareError::Checkpoint,
        }));
    }
    Ok(prepared)
}

#[cfg(test)]
#[path = "pending_write_prototype/compiler_tests.rs"]
mod compiler_tests;
#[cfg(test)]
#[path = "pending_write_prototype/runtime_tests.rs"]
mod runtime_tests;
