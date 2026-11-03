//! Disk completion only. Never a generation, SQL or normal-admission capability.
use super::*;

pub(crate) struct DiskRecoveryCompleted<'a, 's> {
    unlinked: Option<QUnlinked<'a, 's>>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DiskCompletionProgress {
    Unlinked,
    DirectorySynced,
    FinalBarriersSynced,
    FilesVerified,
}
#[derive(Debug)]
pub(crate) enum DiskCompletionError {
    Unsupported,
    Files(FileCheckError),
    Barrier {
        name: &'static str,
        error: io::Error,
    },
    Checkpoint(io::Error),
}
pub(crate) struct DiskCompletionFailure<'a, 's> {
    unlinked: QUnlinked<'a, 's>,
    progress: DiskCompletionProgress,
    error: DiskCompletionError,
}
impl Drop for DiskRecoveryCompleted<'_, '_> {
    fn drop(&mut self) {}
}
impl Drop for DiskCompletionFailure<'_, '_> {
    fn drop(&mut self) {}
}
impl std::fmt::Debug for DiskCompletionFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskCompletionFailure")
            .field("progress", &self.progress)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl DiskCompletionFailure<'_, '_> {
    pub(crate) fn progress(&self) -> &DiskCompletionProgress {
        &self.progress
    }
    pub(crate) fn error(&self) -> &DiskCompletionError {
        &self.error
    }
}
// Same conservative unwind policy as DuringVerification, applied to the whole
// successful unlink owner. No dependent field is extracted or dropped early.
struct DuringDisk<'a, 's>(Option<QUnlinked<'a, 's>>);
impl Drop for DuringDisk<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::mem::forget(self.0.take());
        }
    }
}
impl<'a, 's> QUnlinked<'a, 's> {
    pub(crate) fn complete_disk(
        self,
    ) -> Result<DiskRecoveryCompleted<'a, 's>, DiskCompletionFailure<'a, 's>> {
        let mut owner = DuringDisk(Some(self));
        let mut progress = DiskCompletionProgress::Unlinked;
        let result = complete(owner.0.as_ref().expect("unlinked owner"), &mut progress);
        let unlinked = owner.0.take().expect("unlinked owner");
        match result {
            Ok(()) => Ok(DiskRecoveryCompleted {
                unlinked: Some(unlinked),
            }),
            Err(error) => Err(DiskCompletionFailure {
                unlinked,
                progress,
                error,
            }),
        }
    }
}
// Completion backend. Compiler probes retain the real owners and entry methods.
fn checkpoint(name: &str) -> Result<(), DiskCompletionError> {
    event(name).map_err(DiskCompletionError::Checkpoint)
}
fn verify(proof: &QUnlinked<'_, '_>) -> Result<(), DiskCompletionError> {
    file_checks::check_unlinked(proof).map_err(DiskCompletionError::Files)
}
fn complete(
    proof: &QUnlinked<'_, '_>,
    progress: &mut DiskCompletionProgress,
) -> Result<(), DiskCompletionError> {
    if !cfg!(target_os = "macos") {
        return Err(DiskCompletionError::Unsupported);
    }
    checkpoint("disk-before-verification")?;
    verify(proof)?;
    checkpoint("disk-before-directory-sync")?;
    verify(proof)?;
    let (all, marker) = proof.post_unlink_parts();
    let view = all.held().resources().closed().view();
    let barrier = |name, point, file| {
        // Cancellation is a failure, never permission to recreate Q or unlock.
        classification::cancelled(view.cancel)
            .map_err(|error| DiskCompletionError::Files(FileCheckError::Files(error)))?;
        super::super::super::super::barrier(name, point, file)
            .map_err(|error| DiskCompletionError::Barrier { name, error })
    };
    barrier(
        "disk-root-directory",
        SyncPoint::Directory,
        &view.files.root_file,
    )?;
    *progress = DiskCompletionProgress::DirectorySynced;
    checkpoint("disk-after-directory-sync")?;
    // Only still-linked files: the old Q fd is NEVER a removal barrier.
    barrier(
        "disk-journal-final",
        SyncPoint::FinalFileBarrier,
        &view.files.active_file,
    )?;
    barrier(
        "disk-protection-final",
        SyncPoint::FinalFileBarrier,
        &view.files.protection_file,
    )?;
    barrier(
        "disk-establishment-final",
        SyncPoint::FinalFileBarrier,
        marker.retained_file(),
    )?;
    *progress = DiskCompletionProgress::FinalBarriersSynced;
    checkpoint("disk-after-final-barriers")?;
    checkpoint("disk-before-final-verification")?;
    verify(proof)?;
    *progress = DiskCompletionProgress::FilesVerified;
    checkpoint("disk-after-final-verification")?;
    // Hooks can change files at the last boundary. Production also retains the
    // final read-only recheck; no earlier success or progress authorizes Ready.
    verify(proof)
}
#[cfg(all(test, target_os = "macos"))]
#[path = "disk_completion_tests.rs"]
mod tests;

#[path = "disk_completion/handoff.rs"]
pub(in crate::backup::restore_context::pending) mod handoff;
