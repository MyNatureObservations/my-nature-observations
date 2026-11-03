use super::{Access, Observation};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Old,
    New,
    Uncertain,
    NotNeeded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Unsupported,
    Cancelled,
    Busy,
    Marker,
    Identity,
    Protection,
    SqliteState,
    Database,
    Content,
    Io,
}
// These variants are private to the classifier's implementation, not an authority
// that callers can construct from the public diagnostic Status.
pub(super) enum VerifiedStatus {
    Old,
    New,
}
pub(crate) struct Assessment<'a, 's> {
    status: Status,
    reason: Option<Reason>,
    observation: Option<Observation>,
    access: Option<Access<'a, 's>>,
}
/// Owned one-use capability. Only into_completion_proof can construct it.
/// Keeping the complete Assessment preserves session/operation IDs, exact P/Q
/// bytes, physical identities, snapshot and their destruction/drop-check order.
pub(crate) struct CompletionProof<'a, 's> {
    assessment: Assessment<'a, 's>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProofRejection {
    Uncertain(Option<Reason>),
    NotNeeded,
    SessionRequired,
    Invalidated(Reason),
}
/// Failure owns the original assessment, including its guards. Caller may
/// inspect the diagnostic reason, recover the assessment, or drop it; none of
/// these paths clears the owning recovery gate/latch.
pub(crate) struct RejectedAssessment<'a, 's> {
    reason: ProofRejection,
    assessment: Box<Assessment<'a, 's>>,
}
impl std::fmt::Debug for RejectedAssessment<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedAssessment")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl<'a, 's> RejectedAssessment<'a, 's> {
    pub(crate) fn reason(&self) -> ProofRejection {
        self.reason
    }
    pub(crate) fn into_assessment(self) -> Assessment<'a, 's> {
        *self.assessment
    }
}
impl<'a, 's> Assessment<'a, 's> {
    /// Completion stages consume owned capabilities and repeat mandatory checks
    /// before mutation. No terminal Ready transition is provided here.
    pub(crate) fn into_completion_proof(
        self,
    ) -> Result<CompletionProof<'a, 's>, RejectedAssessment<'a, 's>> {
        let rejection = match self.status {
            Status::Uncertain => Some(ProofRejection::Uncertain(self.reason)),
            Status::NotNeeded => Some(ProofRejection::NotNeeded),
            Status::Old | Status::New => match (&self.observation, &self.access) {
                (Some(observation), Some(access)) if access.is_session() => observation
                    .recheck(access)
                    .err()
                    .map(ProofRejection::Invalidated),
                _ => Some(ProofRejection::SessionRequired),
            },
        };
        if let Some(reason) = rejection {
            Err(RejectedAssessment {
                reason,
                assessment: Box::new(self),
            })
        } else {
            Ok(CompletionProof { assessment: self })
        }
    }

    pub(super) fn retain(mut self, access: Access<'a, 's>) -> Self {
        self.access = Some(access);
        self
    }
    pub(super) fn not_needed() -> Self {
        Self {
            status: Status::NotNeeded,
            reason: None,
            observation: None,
            access: None,
        }
    }
    pub(crate) fn status(&self) -> Status {
        self.status
    }
    pub(crate) fn reason(&self) -> Option<Reason> {
        self.reason
    }
    pub(super) fn uncertain(reason: Reason) -> Self {
        Self {
            status: Status::Uncertain,
            reason: Some(reason),
            observation: None,
            access: None,
        }
    }
    pub(super) fn verified(status: VerifiedStatus, observation: Observation) -> Self {
        Self {
            status: match status {
                VerifiedStatus::Old => Status::Old,
                VerifiedStatus::New => Status::New,
            },
            reason: None,
            observation: Some(observation),
            access: None,
        }
    }
}

// Drop-check must keep the borrowed session owner/maintenance alive until the
// snapshot is actually destroyed, not merely until the last status() call.
// Rust then drops fields in declaration order: observation first, access last.
impl Drop for Assessment<'_, '_> {
    fn drop(&mut self) {}
}

#[cfg(all(test, target_os = "macos"))]
#[path = "proof_tests.rs"]
mod proof_tests;

#[path = "receipt.rs"]
pub(in crate::backup::restore_context::pending) mod receipt;
pub(crate) use receipt::{ReceiptDurable, ReceiptFailure};
pub(crate) type RecoveryDirectoryRemoved<'a, 's> = receipt::RecoveryDirectoryRemoved<'a, 's>;
pub(crate) type CleanupFailure<'a, 's> = receipt::CleanupFailure<'a, 's>;
pub(crate) type CleanupProgress = receipt::CleanupProgress;
impl<'a, 's> CompletionProof<'a, 's> {
    pub(crate) fn publish_receipt(self) -> Result<ReceiptDurable<'a, 's>, ReceiptFailure<'a, 's>> {
        receipt::publish(self)
    }
}

pub(crate) type ResourcesClosed<'a, 's> = receipt::ResourcesClosed<'a, 's>;
pub(crate) type ResourceCloseFailure<'a, 's> = receipt::ResourceCloseFailure<'a, 's>;
pub(crate) type ResourceCloseError = receipt::ResourceCloseError;
pub(crate) type ResourceCloseProgress = receipt::ResourceCloseProgress;
