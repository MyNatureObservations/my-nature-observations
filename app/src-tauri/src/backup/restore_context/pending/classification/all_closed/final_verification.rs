//! Consuming checkpoint only; never authority to remove Q or enter Ready.
use super::file_checks::{self, FileCheckError};
use super::*;
use crate::backup::{pre_restore::durability::SyncPoint, restore_context::pending::classification};
use crate::startup::established::RetainedMarker;

// All owners are already SQLite-closed. Marker precedes the complete guarded
// chain in destruction order; no self-reference or lifetime extension is used.
struct Retained<'a, 's> {
    marker: Option<RetainedMarker>,
    all: AllConnectionsClosed<'a, 's>,
}
pub(crate) struct FinalFilesVerified<'a, 's> {
    retained: Option<Retained<'a, 's>>,
}
pub(crate) struct FinalVerificationFailure<'a, 's> {
    retained: Retained<'a, 's>,
    error: FinalVerificationError,
}
#[cfg(test)]
impl<'a, 's> FinalFilesVerified<'a, 's> {
    fn retained(&self) -> &Retained<'a, 's> {
        self.retained.as_ref().expect("final verified owner")
    }
}
impl Drop for FinalFilesVerified<'_, '_> {
    fn drop(&mut self) {}
}
impl Drop for FinalVerificationFailure<'_, '_> {
    fn drop(&mut self) {}
}
#[derive(Debug)]
pub(crate) enum FinalVerificationError {
    Files(FileCheckError),
    Durability {
        barrier: &'static str,
        error: io::Error,
    },
    Checkpoint(io::Error),
}
impl std::fmt::Debug for FinalVerificationFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FinalVerificationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl FinalVerificationFailure<'_, '_> {
    pub(crate) fn error(&self) -> &FinalVerificationError {
        &self.error
    }
}
// If an unexpected panic unwinds the consuming operation, preserve the closed
// owner rather than letting staging destructors perform cleanup. No live SQL or
// callbacks remain, and leaked borrows are never accessed. Process exit releases
// descriptors. Ordinary success/error moves the owner out without leaking.
struct DuringVerification<'a, 's>(Option<Retained<'a, 's>>);
impl Drop for DuringVerification<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::mem::forget(self.0.take());
        }
    }
}
impl<'a, 's> AllConnectionsClosed<'a, 's> {
    pub(crate) fn verify_final(
        self,
    ) -> Result<FinalFilesVerified<'a, 's>, FinalVerificationFailure<'a, 's>> {
        let mut owner = DuringVerification(Some(Retained {
            marker: None,
            all: self,
        }));
        let result = owner.0.as_mut().expect("verification owner").run();
        let retained = owner.0.take().expect("verification owner");
        match result {
            Ok(()) => Ok(FinalFilesVerified {
                retained: Some(retained),
            }),
            Err(error) => Err(FinalVerificationFailure { retained, error }),
        }
    }
}
// Verification backend. Compiler probes keep actual ownership and entry methods.
impl Retained<'_, '_> {
    fn run(&mut self) -> Result<(), FinalVerificationError> {
        // file_checks checks platform, session, phase, generation, lease and all
        // retained identities before opening the fixed establishment name.
        if let Some(marker) = &self.marker {
            // Reuse the established descriptor when renewing an existing proof.
            file_checks::check_retained(&self.all, marker)
                .map_err(FinalVerificationError::Files)?;
        } else {
            self.marker = Some(
                self.all
                    .file_checks()
                    .map_err(FinalVerificationError::Files)?
                    .into_marker(),
            );
        }
        event("final-before-barriers").map_err(FinalVerificationError::Checkpoint)?;
        let view = self.all.held().resources().closed().view();
        let marker = self.marker.as_ref().expect("retained marker");
        let q = view
            .files
            .evidence
            .receipt
            .as_ref()
            .ok_or(FinalVerificationError::Files(FileCheckError::Artifact))?;
        let file_barrier = |name, point, file| {
            classification::cancelled(view.cancel)
                .map_err(|e| FinalVerificationError::Files(FileCheckError::Files(e)))?;
            super::super::barrier(name, point, file).map_err(|error| {
                FinalVerificationError::Durability {
                    barrier: name,
                    error,
                }
            })
        };
        let q_barrier = |name, point| {
            classification::cancelled(view.cancel)
                .map_err(|e| FinalVerificationError::Files(FileCheckError::Files(e)))?;
            q.sync_barrier(name, point)
                .map_err(|error| FinalVerificationError::Durability {
                    barrier: name,
                    error,
                })
        };
        file_barrier(
            "final-journal-file",
            SyncPoint::VerifiedFile,
            &view.files.active_file,
        )?;
        file_barrier(
            "final-protection-file",
            SyncPoint::VerifiedFile,
            &view.files.protection_file,
        )?;
        q_barrier("final-q-file", SyncPoint::VerifiedFile)?;
        file_barrier(
            "final-establishment-file",
            SyncPoint::VerifiedFile,
            marker.retained_file(),
        )?;
        file_barrier(
            "final-protection-directory",
            SyncPoint::Directory,
            &view.files.protection_directory,
        )?;
        file_barrier(
            "final-backup-directory",
            SyncPoint::Directory,
            &view.files.backup_directory,
        )?;
        file_barrier(
            "final-root-directory",
            SyncPoint::Directory,
            &view.files.root_file,
        )?;
        file_barrier(
            "final-journal-barrier",
            SyncPoint::FinalFileBarrier,
            &view.files.active_file,
        )?;
        file_barrier(
            "final-protection-barrier",
            SyncPoint::FinalFileBarrier,
            &view.files.protection_file,
        )?;
        q_barrier("final-q-barrier", SyncPoint::FinalFileBarrier)?;
        file_barrier(
            "final-establishment-barrier",
            SyncPoint::FinalFileBarrier,
            marker.retained_file(),
        )?;
        event("final-after-barriers").map_err(FinalVerificationError::Checkpoint)?;
        file_checks::check_retained(&self.all, marker).map_err(FinalVerificationError::Files)
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "final_verification_tests.rs"]
mod tests;

// This module and all actual ownership extraction entry points are absent in release.
#[cfg(all(test, target_os = "macos"))]
#[path = "final_verification/test_transfer.rs"]
mod test_transfer;

#[path = "final_verification/q_removal.rs"]
pub(super) mod q_removal;
