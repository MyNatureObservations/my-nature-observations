//! Only Q name removal. No directory sync, disk-completion or drift admission.
use super::*;

pub(crate) struct QRemovalPrepared<'a, 's> {
    retained: Option<Retained<'a, 's>>,
}
pub(crate) struct QUnlinked<'a, 's> {
    retained: Option<Retained<'a, 's>>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum QRemovalProgress {
    QRequired,
    UnlinkAttempted,
    Unlinked,
}
#[derive(Debug)]
pub(crate) enum QRemovalError {
    Unsupported,
    Verification(FinalVerificationError),
    Checkpoint(io::Error),
    Target(String),
    Unlink(io::Error),
    Cancelled,
}
pub(crate) struct QRemovalFailure<'a, 's> {
    retained: Retained<'a, 's>,
    progress: QRemovalProgress,
    error: QRemovalError,
}
impl Drop for QRemovalPrepared<'_, '_> {
    fn drop(&mut self) {}
}
impl Drop for QUnlinked<'_, '_> {
    fn drop(&mut self) {}
}
impl Drop for QRemovalFailure<'_, '_> {
    fn drop(&mut self) {}
}
impl std::fmt::Debug for QRemovalFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QRemovalFailure")
            .field("progress", &self.progress)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl QRemovalFailure<'_, '_> {
    pub(crate) fn progress(&self) -> &QRemovalProgress {
        &self.progress
    }
    pub(crate) fn error(&self) -> &QRemovalError {
        &self.error
    }
}
impl<'a, 's> FinalFilesVerified<'a, 's> {
    pub(crate) fn prepare_q_removal(
        mut self,
    ) -> Result<QRemovalPrepared<'a, 's>, QRemovalFailure<'a, 's>> {
        // No fallible work or callbacks between take and the aggregate guard.
        // Reuse the existing conservative unwind retention contract.
        let mut owner = DuringVerification(self.retained.take());
        let result = prepare_retained(owner.0.as_mut().expect("final owner"));
        let retained = owner.0.take().expect("final owner");
        match result {
            Ok(()) => Ok(QRemovalPrepared {
                retained: Some(retained),
            }),
            Err(error) => Err(QRemovalFailure {
                retained,
                error,
                progress: QRemovalProgress::QRequired,
            }),
        }
    }
}
impl<'a, 's> QRemovalPrepared<'a, 's> {
    pub(crate) fn unlink_q(mut self) -> Result<QUnlinked<'a, 's>, QRemovalFailure<'a, 's>> {
        let mut owner = DuringVerification(self.retained.take());
        let mut progress = QRemovalProgress::QRequired;
        let result = unlink_retained(owner.0.as_mut().expect("prepared owner"), &mut progress);
        let retained = owner.0.take().expect("prepared owner");
        match result {
            Ok(()) => Ok(QUnlinked {
                retained: Some(retained),
            }),
            Err(error) => Err(QRemovalFailure {
                retained,
                error,
                progress,
            }),
        }
    }
}
// Removal backend. Compiler probes keep real owners and consuming entry methods.
fn supported() -> Result<(), QRemovalError> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err(QRemovalError::Unsupported)
    }
}
fn point(name: &str) -> Result<(), QRemovalError> {
    event(name).map_err(QRemovalError::Checkpoint)
}
fn check(retained: &Retained<'_, '_>) -> Result<(), QRemovalError> {
    supported()?;
    let marker = retained
        .marker
        .as_ref()
        .ok_or(QRemovalError::Target("missing establishment owner".into()))?;
    file_checks::check_retained(&retained.all, marker)
        .map_err(|error| QRemovalError::Verification(FinalVerificationError::Files(error)))
}
fn prepare_retained(retained: &mut Retained<'_, '_>) -> Result<(), QRemovalError> {
    supported()?;
    point("q-prepare-before-verification")?;
    check(retained)?;
    // All eleven existing required file/directory barriers and post-verification.
    // With a retained marker run() neither reopens nor replaces that descriptor.
    retained.run().map_err(QRemovalError::Verification)?;
    point("q-prepare-after-barriers")?;
    check(retained)
}
fn unlink_retained(
    retained: &mut Retained<'_, '_>,
    progress: &mut QRemovalProgress,
) -> Result<(), QRemovalError> {
    supported()?;
    point("q-before-unlink-verification")?;
    check(retained)?;
    // Prepared values can be held: renew durability and verification immediately
    // within the consuming mutation call, not merely at preparation time.
    retained.run().map_err(QRemovalError::Verification)?;
    point("q-before-unlink")?;
    check(retained)?;
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let view = retained.all.held().resources().closed().view();
        let receipt = view
            .files
            .evidence
            .receipt
            .as_ref()
            .ok_or(QRemovalError::Target("missing Q owner".into()))?;
        #[cfg(test)]
        point("q-unlink-syscall")?;
        classification::cancelled(view.cancel).map_err(|_| QRemovalError::Cancelled)?;
        // Recheck all files after the last injectable boundary as well. The
        // final fixed-name fstatat still follows this complete verification.
        check(retained)?;
        classification::cancelled(view.cancel).map_err(|_| QRemovalError::Cancelled)?;
        receipt
            .check_unlink_target(&view.files.root_file)
            .map_err(|e| QRemovalError::Target(format!("{e:?}")))?;
        *progress = QRemovalProgress::UnlinkAttempted;
        // SAFETY: retained root fd and fixed component; no symlink traversal,
        // arbitrary path, recursive deletion or other target is possible here.
        if unsafe {
            libc::unlinkat(
                view.files.root_file.as_raw_fd(),
                c".mno-restore-completion-v1.json".as_ptr(),
                0,
            )
        } != 0
        {
            return Err(QRemovalError::Unlink(io::Error::last_os_error()));
        }
        // First operation after confirmed success: no allocation, hook or check
        // may fail before progress records the actual name mutation.
        *progress = QRemovalProgress::Unlinked;
        point("q-after-unlink")?;
        classification::cancelled(view.cancel).map_err(|_| QRemovalError::Cancelled)?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = progress;
        Err(QRemovalError::Unsupported)
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "q_removal_tests.rs"]
mod tests;

// Read-only projections tied to this actual successful unlink owner. Comparing
// current references creates no stored/self-referential pointer or new identity.
impl<'a, 's> QUnlinked<'a, 's> {
    pub(in crate::backup::restore_context::pending) fn post_unlink_parts(
        &self,
    ) -> (&AllConnectionsClosed<'a, 's>, &RetainedMarker) {
        let retained = self.retained.as_ref().expect("unlinked owner");
        (
            &retained.all,
            retained.marker.as_ref().expect("establishment owner"),
        )
    }
    pub(in crate::backup::restore_context::pending) fn require_receipt(
        &self,
        receipt: &crate::backup::restore_context::pending::completion::LoadedReceipt,
    ) -> Result<(), crate::backup::restore_context::pending::completion::ReceiptError> {
        let files = self
            .post_unlink_parts()
            .0
            .held()
            .resources()
            .closed()
            .view()
            .files;
        match files.evidence.receipt.as_ref() {
            Some(original) if std::ptr::eq(original, receipt) => Ok(()),
            _ => Err(crate::backup::restore_context::pending::completion::ReceiptError::Binding),
        }
    }
}
#[path = "disk_completion.rs"]
pub(in crate::backup::restore_context::pending) mod disk_completion;
