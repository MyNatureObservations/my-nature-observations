//! Proof-owned receipt publication and pending cleanup; no Ready transition.
use super::super::{Access, SyncPoint, completion, durability};
use super::{CompletionProof, Reason, Status};
use std::{ffi::OsStr, io, path::Path};

pub(crate) struct ReceiptDurable<'a, 's> {
    // Field order/drop-check of Assessment still owns SQLite before session.
    proof: Box<CompletionProof<'a, 's>>,
}
pub(crate) struct ReceiptFailure<'a, 's> {
    proof: Box<CompletionProof<'a, 's>>,
    error: PublicationError,
    may_have_receipt: bool,
}
#[derive(Debug)]
pub(crate) enum PublicationError {
    Verification(Reason),
    Receipt(completion::ReceiptError),
    Durability(io::Error),
}
impl std::fmt::Debug for ReceiptFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiptFailure")
            .field("error", &self.error)
            .field("may_have_receipt", &self.may_have_receipt)
            .finish_non_exhaustive()
    }
}
impl ReceiptFailure<'_, '_> {
    pub(crate) fn error(&self) -> &PublicationError {
        &self.error
    }
    pub(crate) fn may_have_receipt(&self) -> bool {
        self.may_have_receipt
    }
}
// Diagnostic progress, never authority to continue cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanupProgress {
    Unchanged,
    PendingUnlinkAttempted,
    PendingUnlinked,
    PendingDurable,
    RecoveryUnlinkAttempted,
    RecoveryUnlinked,
    RecoveryDurable,
}
#[derive(Debug)]
pub(crate) enum CleanupError {
    Verification(Reason),
    Io(io::Error),
    Unsupported,
}
pub(crate) struct RecoveryDirectoryRemoved<'a, 's> {
    receipt: ReceiptDurable<'a, 's>,
}
pub(crate) struct CleanupFailure<'a, 's> {
    receipt: ReceiptDurable<'a, 's>,
    error: CleanupError,
    progress: CleanupProgress,
}
impl std::fmt::Debug for CleanupFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanupFailure")
            .field("error", &self.error)
            .field("progress", &self.progress)
            .finish_non_exhaustive()
    }
}
impl CleanupFailure<'_, '_> {
    pub(crate) fn error(&self) -> &CleanupError {
        &self.error
    }
    pub(crate) fn progress(&self) -> CleanupProgress {
        self.progress
    }
}
impl<'a, 's> ReceiptDurable<'a, 's> {
    pub(crate) fn remove_pending(
        self,
    ) -> Result<RecoveryDirectoryRemoved<'a, 's>, CleanupFailure<'a, 's>> {
        remove_pending_impl(self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceCloseProgress {
    Unchanged,
    ActiveSqliteClosed,
    ActiveClosed,
    ProtectionClosed,
}
#[derive(Debug)]
pub(crate) enum ResourceCloseError {
    Verification(Reason),
    ActiveSqlite { code: Option<i32>, message: String },
    VfsUnregister,
    Io(io::Error),
    ProtectionSqlite { code: Option<i32>, message: String },
    Unsupported,
}
pub(crate) struct ResourcesClosed<'a, 's> {
    closed: Option<Box<resource_close::Closed<'a, 's>>>,
}
pub(crate) struct ResourceCloseFailure<'a, 's> {
    retained: Box<resource_close::Retained<'a, 's>>,
    error: ResourceCloseError,
    progress: ResourceCloseProgress,
}
// Explicit Drop-check keeps borrowed guards alive until the retained resources
// are actually destroyed, even if the caller never inspects the capability.
impl<'a, 's> ResourcesClosed<'a, 's> {
    fn closed(&self) -> &resource_close::Closed<'a, 's> {
        self.closed.as_deref().expect("resources closed owner")
    }
    #[cfg(test)]
    fn closed_mut(&mut self) -> &mut resource_close::Closed<'a, 's> {
        self.closed.as_deref_mut().expect("resources closed owner")
    }
}
impl Drop for ResourcesClosed<'_, '_> {
    fn drop(&mut self) {}
}
impl Drop for ResourceCloseFailure<'_, '_> {
    fn drop(&mut self) {}
}
impl std::fmt::Debug for ResourceCloseFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceCloseFailure")
            .field("error", &self.error)
            .field("progress", &self.progress)
            .finish_non_exhaustive()
    }
}
impl ResourceCloseFailure<'_, '_> {
    pub(crate) fn error(&self) -> &ResourceCloseError {
        &self.error
    }
    pub(crate) fn progress(&self) -> ResourceCloseProgress {
        self.progress
    }
}
impl<'a, 's> RecoveryDirectoryRemoved<'a, 's> {
    pub(crate) fn close_resources(
        self,
    ) -> Result<ResourcesClosed<'a, 's>, ResourceCloseFailure<'a, 's>> {
        resource_close::close(self)
    }
}
// Only this module constructs authorization, from an owned existing proof.
// Completion's engine can borrow it but cannot manufacture one from bytes/status.
pub(in crate::backup::restore_context::pending) struct Authorization<'p, 'a, 's> {
    proof: &'p mut CompletionProof<'a, 's>,
    bytes: Vec<u8>,
    pub(in crate::backup::restore_context::pending) may_have_receipt: bool,
}
impl Authorization<'_, '_, '_> {
    pub(in crate::backup::restore_context::pending) fn root(&self) -> &Path {
        &self
            .proof
            .assessment
            .observation
            .as_ref()
            .expect("proof observation")
            .files
            .root
    }
    pub(in crate::backup::restore_context::pending) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(in crate::backup::restore_context::pending) fn check(
        &self,
        temporary: Option<&OsStr>,
    ) -> Result<(), completion::ReceiptError> {
        let assessment = &self.proof.assessment;
        let access = assessment
            .access
            .as_ref()
            .ok_or(completion::ReceiptError::Binding)?;
        match access {
            Access::Session(session) => session
                .ensure_publication_phase()
                .map_err(|_| completion::ReceiptError::Binding)?,
            _ => return Err(completion::ReceiptError::Binding),
        }
        assessment
            .observation
            .as_ref()
            .ok_or(completion::ReceiptError::Binding)?
            .recheck_temporary(access, temporary)
            .map_err(|reason| match reason {
                Reason::Cancelled => completion::ReceiptError::Cancelled,
                _ => completion::ReceiptError::Binding,
            })
    }
    pub(in crate::backup::restore_context::pending) fn existing(
        &self,
    ) -> Option<&completion::LoadedReceipt> {
        self.proof
            .assessment
            .observation
            .as_ref()
            .unwrap()
            .files
            .evidence
            .receipt
            .as_ref()
    }
    pub(in crate::backup::restore_context::pending) fn install(
        &mut self,
        receipt: completion::LoadedReceipt,
    ) {
        self.proof
            .assessment
            .observation
            .as_mut()
            .unwrap()
            .files
            .evidence
            .receipt = Some(receipt);
    }
}
pub(super) fn publish<'a, 's>(
    mut proof: CompletionProof<'a, 's>,
) -> Result<ReceiptDurable<'a, 's>, ReceiptFailure<'a, 's>> {
    let mut may_have_receipt = proof
        .assessment
        .observation
        .as_ref()
        .is_some_and(|o| o.files.evidence.receipt.is_some());
    let result = (|| {
        let assessment = &proof.assessment;
        let observation = assessment
            .observation
            .as_ref()
            .ok_or(PublicationError::Verification(Reason::Content))?;
        let access = assessment
            .access
            .as_ref()
            .ok_or(PublicationError::Verification(Reason::Identity))?;
        observation
            .recheck(access)
            .map_err(PublicationError::Verification)?;
        let outcome = match assessment.status {
            Status::Old => completion::Outcome::Old,
            Status::New => completion::Outcome::New,
            _ => return Err(PublicationError::Verification(Reason::Content)),
        };
        let bytes = completion::receipt_bytes(&observation.files.evidence.bytes, outcome)
            .map_err(PublicationError::Receipt)?;
        // Retained descriptors only. A read snapshot proves content, not that
        // previous writes/name operations have reached stable storage.
        barrier(
            "journal-file",
            SyncPoint::VerifiedFile,
            &observation.files.active_file,
        )
        .map_err(PublicationError::Durability)?;
        barrier(
            "journal-directory",
            SyncPoint::Directory,
            &observation.files.root_file,
        )
        .map_err(PublicationError::Durability)?;
        barrier(
            "journal-final",
            SyncPoint::FinalFileBarrier,
            &observation.files.active_file,
        )
        .map_err(PublicationError::Durability)?;
        let mut authorization = Authorization {
            proof: &mut proof,
            bytes,
            may_have_receipt,
        };
        authorization
            .check(None)
            .map_err(PublicationError::Receipt)?;
        let result =
            completion::publish_authorized(&mut authorization).map_err(PublicationError::Receipt);
        may_have_receipt = authorization.may_have_receipt;
        result
    })();
    match result {
        Ok(()) => Ok(ReceiptDurable {
            proof: Box::new(proof),
        }),
        Err(error) => Err(ReceiptFailure {
            proof: Box::new(proof),
            error,
            may_have_receipt,
        }),
    }
}

pub(in crate::backup::restore_context::pending) fn event(_name: &str) -> io::Result<()> {
    #[cfg(test)]
    {
        hooks::event(_name)?;
    }
    Ok(())
}
pub(in crate::backup::restore_context::pending) fn barrier(
    _name: &str,
    point: SyncPoint,
    file: &std::fs::File,
) -> io::Result<()> {
    #[cfg(test)]
    {
        if let Some(result) = hooks::sync(_name, point, file) {
            return result;
        }
    }
    durability::sync_opened(point, file)
}
#[cfg(all(test, target_os = "macos"))]
#[path = "receipt_tests.rs"]
mod tests;
#[cfg(test)]
mod hooks {
    use super::*;
    use std::cell::RefCell;
    type Event = Box<dyn FnMut(&str) -> io::Result<()>>;
    type Sync = Box<dyn FnMut(&str, SyncPoint, &std::fs::File) -> io::Result<()>>;
    thread_local! {
        static EVENT:RefCell<Option<Event>>=const { RefCell::new(None) };
        static SYNC:RefCell<Option<Sync>>=const { RefCell::new(None) };
    }
    pub(super) fn event(name: &str) -> io::Result<()> {
        EVENT.with(|s| match s.borrow_mut().as_mut() {
            Some(f) => f(name),
            None => Ok(()),
        })
    }
    pub(super) fn sync(name: &str, p: SyncPoint, f: &std::fs::File) -> Option<io::Result<()>> {
        SYNC.with(|s| s.borrow_mut().as_mut().map(|call| call(name, p, f)))
    }
    pub(super) fn with<T>(
        event: impl FnMut(&str) -> io::Result<()> + 'static,
        sync: impl FnMut(&str, SyncPoint, &std::fs::File) -> io::Result<()> + 'static,
        run: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                EVENT.with(|s| *s.borrow_mut() = None);
                SYNC.with(|s| *s.borrow_mut() = None);
            }
        }
        EVENT.with(|s| assert!(s.borrow_mut().replace(Box::new(event)).is_none()));
        SYNC.with(|s| assert!(s.borrow_mut().replace(Box::new(sync)).is_none()));
        let _reset = Reset;
        run()
    }
}

#[path = "cleanup.rs"]
mod cleanup;
fn remove_pending_impl<'a, 's>(
    receipt: ReceiptDurable<'a, 's>,
) -> Result<RecoveryDirectoryRemoved<'a, 's>, CleanupFailure<'a, 's>> {
    cleanup::remove(receipt)
}

#[path = "resource_close.rs"]
mod resource_close;

#[path = "all_closed.rs"]
pub(in crate::backup::restore_context::pending) mod all_closed;

// Scoped adapter for real cross-chain integration tests; production is unchanged.
#[cfg(all(test, target_os = "macos"))]
pub(in crate::backup::restore_context::pending) fn with_episode_probe<T>(
    event: impl FnMut(&str) -> io::Result<()> + 'static,
    sync: impl FnMut(&str, SyncPoint, &std::fs::File) -> io::Result<()> + 'static,
    run: impl FnOnce() -> T,
) -> T {
    hooks::with(event, sync, run)
}
