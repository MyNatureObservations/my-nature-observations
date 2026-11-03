//! Reusable observations only: no final verification/cleanup capability or barriers.
use super::*;
use crate::{
    backup::restore_context::pending::classification::{self, Reason},
    startup::established::RetainedMarker,
};

#[derive(Debug)]
pub(crate) enum FileCheckError {
    Binding(AllCloseError),
    Files(Reason),
    Establishment(String),
    Artifact,
    Io(io::Error),
}
// Borrowing the entire owner retains session, reservation, lease and all opened
// objects. The marker is retained too. No success token authorizes a mutation.
pub(crate) struct FileChecks<'p, 'a, 's> {
    owner: &'p AllConnectionsClosed<'a, 's>,
    established: RetainedMarker,
}
impl<'a, 's> AllConnectionsClosed<'a, 's> {
    pub(crate) fn file_checks(&self) -> Result<FileChecks<'_, 'a, 's>, FileCheckError> {
        self.held().check().map_err(FileCheckError::Binding)?;
        let files = self.held().resources().closed().view().files;
        let established =
            RetainedMarker::open_at(&files.root_file).map_err(FileCheckError::Establishment)?;
        let checks = FileChecks {
            owner: self,
            established,
        };
        checks.check()?;
        Ok(checks)
    }
}
impl FileChecks<'_, '_, '_> {
    pub(crate) fn check(&self) -> Result<(), FileCheckError> {
        check_retained(self.owner, &self.established)
    }
    pub(super) fn into_marker(self) -> RetainedMarker {
        self.established
    }
}
pub(super) fn check_retained(
    owner: &AllConnectionsClosed<'_, '_>,
    established: &RetainedMarker,
) -> Result<(), FileCheckError> {
    check_retained_mode(
        owner,
        established,
        &classification::ReceiptCheck::Named(None),
    )
}
pub(super) fn check_unlinked(proof: &super::QUnlinked<'_, '_>) -> Result<(), FileCheckError> {
    let (owner, established) = proof.post_unlink_parts();
    check_retained_mode(
        owner,
        established,
        &classification::ReceiptCheck::Unlinked(proof),
    )
}
fn check_retained_mode(
    owner: &AllConnectionsClosed<'_, '_>,
    established: &RetainedMarker,
    mode: &classification::ReceiptCheck<'_, '_, '_>,
) -> Result<(), FileCheckError> {
    if owner.held().check().map_err(FileCheckError::Binding)? != owner.generation {
        return Err(FileCheckError::Binding(AllCloseError::Binding));
    }
    let view = owner.held().resources().closed().view();
    let artifacts = || {
        // Evidence already requires the exact Q object and pending bytes;
        // after cleanup no recovery entry may remain, including an empty one.
        absent(&view.files.root_file, c"recovery")?;
        absent(&view.files.root_file, c"pending-restore.json")?;
        if matches!(mode, classification::ReceiptCheck::Unlinked(_)) {
            absent(&view.files.root_file, c".mno-restore-completion-v1.json")?;
        }
        // Shared layout logic rejects orphan completion temporaries. This
        // additional reserved namespace policy does not reject unrelated files.
        if crate::startup::artifacts::unexpected_root(&view.files.root, None)
            .map_err(FileCheckError::Io)?
        {
            return Err(FileCheckError::Artifact);
        }
        established
            .check(&view.files.root_file)
            .map_err(FileCheckError::Establishment)
    };
    artifacts()?;
    view.binding
        .check_files_mode(view.files, view.access, view.cancel, mode)
        .map_err(FileCheckError::Files)?;
    artifacts()?;
    if owner.held().check().map_err(FileCheckError::Binding)? != owner.generation {
        return Err(FileCheckError::Binding(AllCloseError::Binding));
    }
    classification::cancelled(view.cancel).map_err(FileCheckError::Files)
}
fn absent(directory: &std::fs::File, name: &std::ffi::CStr) -> Result<(), FileCheckError> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: retained directory, fixed single-component name, valid output.
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0
        {
            return Err(FileCheckError::Artifact);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            Ok(())
        } else {
            Err(FileCheckError::Io(error))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (directory, name);
        Err(FileCheckError::Binding(AllCloseError::Unsupported))
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "file_checks_tests.rs"]
mod tests;
