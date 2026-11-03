//! Owned durable-P preparation only. No journal writer or admission authority.
use super::*;
use crate::database_state::recovery_session::{ContextSessionBinding, RecoverySession};

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
    // Metadata only: never open a P name that may alias a live SQLite inode.
    // This is a point-in-time observation; it does not freeze pathname identity.
    #[cfg(unix)]
    fn retained_name_matches(&self) -> MarkerResult<bool> {
        retained_pending_name_matches(&self.root, &self.marker_file)
    }
    fn verify(&mut self, context: &RestoreContext<'_, '_>) -> MarkerResult<()> {
        #[cfg(not(unix))]
        {
            let _ = context;
            return Err(MarkerError::Unsupported);
        }
        #[cfg(unix)]
        {
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
                || !self.retained_name_matches()?
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
}

// Dependency order: close context/SQLite safely, release P handles and binding,
// then release the SAME session reservation. DirectoryLease remains in state.
struct WriteParts<'m, 's> {
    context: RestoreContext<'m, 's>,
    pending: Option<OwnedPending>,
    binding: Option<ContextSessionBinding<'m, 's>>,
    session: Option<RecoverySession<'m, 's>>,
}
struct WriteOwner<'m, 's>(Option<WriteParts<'m, 's>>);
impl Drop for WriteOwner<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Retain the entire dependency chain, including the reservation.
            // These borrowed records are never recovered or dereferenced again.
            std::mem::forget(self.0.take());
        }
    }
}
/// Durable P and owned context; deliberately NOT a session-bound PendingWrite.
/// No SQL or journal mutation methods. May only bind an existing real session.
pub(crate) struct DurableWritePreparation<'m, 's> {
    owner: WriteOwner<'m, 's>,
}
/// Requires durable P AND ownership of the original, checked Running session.
/// No SQL, cleanup, handoff, generation or Ready authority is exposed.
pub(crate) struct PendingWrite<'m, 's> {
    owner: WriteOwner<'m, 's>,
}
#[derive(Debug)]
pub(crate) enum WritePreparationError {
    Unsupported,
    Context(ContextError),
    Marker(MarkerError),
    Session(String),
    #[cfg(test)]
    Checkpoint,
}
pub(crate) struct WritePreparationFailure<'m, 's> {
    owner: WriteOwner<'m, 's>,
    error: WritePreparationError,
}
impl WritePreparationFailure<'_, '_> {
    pub(crate) fn error(&self) -> &WritePreparationError {
        &self.error
    }
}
impl std::fmt::Debug for WritePreparationFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WritePreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for PendingWrite<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingWrite").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for DurableWritePreparation<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DurableWritePreparation")
            .finish_non_exhaustive()
    }
}
type WriteResult<'m, 's, T> = std::result::Result<T, Box<WritePreparationFailure<'m, 's>>>;
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparationPoint {
    BeforeAuthorization,
    Publication(Point),
    DurablePending,
    SessionOwned,
    Bound,
}
impl<'m, 's> RestoreContext<'m, 's> {
    /// Phase one must precede RecoverySession acquisition: existing authorize()
    /// rejects Recovery. It does not create a session or grant writer authority.
    pub(crate) fn publish_write_preparation(
        self,
    ) -> WriteResult<'m, 's, DurableWritePreparation<'m, 's>> {
        self.publish_write_preparation_inner(
            #[cfg(test)]
            &mut |_| Ok(()),
        )
    }
    fn publish_write_preparation_inner(
        self,
        #[cfg(test)] checkpoint: &mut impl FnMut(PreparationPoint) -> io::Result<()>,
    ) -> WriteResult<'m, 's, DurableWritePreparation<'m, 's>> {
        let mut owner = WriteOwner(Some(WriteParts {
            context: self,
            pending: None,
            binding: None,
            session: None,
        }));
        let result = (|| {
            if !cfg!(target_os = "macos") {
                return Err(WritePreparationError::Unsupported);
            }
            #[cfg(test)]
            checkpoint(PreparationPoint::BeforeAuthorization)
                .map_err(|_| WritePreparationError::Checkpoint)?;
            let held = owner.0.as_mut().expect("owned context");
            let pending = {
                let authorization = held
                    .context
                    .authorize()
                    .map_err(WritePreparationError::Context)?;
                let marker = publish_with(&authorization, |point| {
                    #[cfg(test)]
                    checkpoint(PreparationPoint::Publication(point))?;
                    #[cfg(not(test))]
                    let _ = point;
                    Ok(())
                })
                .map_err(WritePreparationError::Marker)?;
                OwnedPending::consume(marker)
            }; // No ActiveDatabase/authorization borrow crosses context ownership.
            held.pending = Some(pending);
            held.pending
                .as_mut()
                .expect("published P")
                .verify(&held.context)
                .map_err(WritePreparationError::Marker)?;
            #[cfg(test)]
            checkpoint(PreparationPoint::DurablePending)
                .map_err(|_| WritePreparationError::Checkpoint)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(DurableWritePreparation { owner }),
            Err(error) => Err(Box::new(WritePreparationFailure { owner, error })),
        }
    }
}
impl<'m, 's> DurableWritePreparation<'m, 's> {
    /// Phase two consumes an existing session; never acquires/replaces its slot.
    pub(crate) fn prepare_write(
        self,
        session: RecoverySession<'m, 's>,
    ) -> WriteResult<'m, 's, PendingWrite<'m, 's>> {
        self.prepare_write_inner(
            session,
            #[cfg(test)]
            &mut |_| Ok(()),
        )
    }
    fn prepare_write_inner(
        self,
        session: RecoverySession<'m, 's>,
        #[cfg(test)] checkpoint: &mut impl FnMut(PreparationPoint) -> io::Result<()>,
    ) -> WriteResult<'m, 's, PendingWrite<'m, 's>> {
        let mut owner = self.owner;
        // Infallible placement under the shared unwind guard before checks or
        // callbacks. No interval owns the context without its consumed session.
        owner.0.as_mut().expect("durable preparation").session = Some(session);
        let result = (|| {
            #[cfg(test)]
            checkpoint(PreparationPoint::SessionOwned)
                .map_err(|_| WritePreparationError::Checkpoint)?;
            let held = owner.0.as_mut().expect("session owner");
            let session = held.session.as_ref().expect("consumed session");
            let binding = session
                .bind_context(held.context.maintenance)
                .map_err(WritePreparationError::Session)?;
            binding
                .check_session(session)
                .map_err(WritePreparationError::Session)?;
            if binding.database_path() != held.context.active.path()
                || binding.operation_id() != held.context.maintenance.operation_id()
            {
                return Err(WritePreparationError::Marker(MarkerError::Binding));
            }
            let (volume, file) = session
                .directory_identity()
                .map_err(WritePreparationError::Session)?;
            let pending = held.pending.as_mut().expect("durable P");
            pending
                .verify(&held.context)
                .map_err(WritePreparationError::Marker)?;
            if pending.identities.as_ref().expect("verified identities").0
                != Identity::from_parts(volume, file)
            {
                return Err(WritePreparationError::Marker(MarkerError::Binding));
            }
            held.binding = Some(binding);
            #[cfg(test)]
            checkpoint(PreparationPoint::Bound).map_err(|_| WritePreparationError::Checkpoint)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(PendingWrite { owner }),
            Err(error) => Err(Box::new(WritePreparationFailure { owner, error })),
        }
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

// Separates only A; no writable target or broader completion authority.
pub(crate) mod active_close;
