//! Consuming P/directory cleanup only. Q and every snapshot/guard stay owned.
use super::super::super::{Access, Observation};
#[cfg(target_os = "macos")]
use super::super::super::{current, identity};
use super::*;
#[cfg(target_os = "macos")]
use std::fs::File;

fn observation<'p>(receipt: &'p ReceiptDurable<'_, '_>) -> &'p Observation {
    receipt
        .proof
        .assessment
        .observation
        .as_ref()
        .expect("durable proof observation")
}
fn check(receipt: &ReceiptDurable<'_, '_>) -> Result<(), CleanupError> {
    let access = receipt
        .proof
        .assessment
        .access
        .as_ref()
        .ok_or(CleanupError::Verification(Reason::Identity))?;
    match access {
        Access::Session(session) => session
            .ensure_publication_phase()
            .map_err(|_| CleanupError::Verification(Reason::Identity))?,
        _ => return Err(CleanupError::Verification(Reason::Identity)),
    }
    let observation = observation(receipt);
    if observation.files.evidence.receipt.is_none() {
        return Err(CleanupError::Verification(Reason::Marker));
    }
    observation
        .recheck(access)
        .map_err(CleanupError::Verification)
}
fn q_sync(
    receipt: &ReceiptDurable<'_, '_>,
    name: &str,
    point: SyncPoint,
) -> Result<(), CleanupError> {
    observation(receipt)
        .files
        .evidence
        .receipt
        .as_ref()
        .ok_or(CleanupError::Verification(Reason::Marker))?
        .sync_barrier(name, point)
        .map_err(CleanupError::Io)
}
fn checkpoint(name: &str) -> Result<(), CleanupError> {
    event(name).map_err(CleanupError::Io)
}
#[cfg(target_os = "macos")]
fn named(
    directory: &File,
    name: &std::ffi::CStr,
    file: &File,
    is_directory: bool,
) -> Result<(), CleanupError> {
    use std::os::fd::AsRawFd;
    let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: retained directory fd and static single-component name remain live.
    // fstatat never follows symlinks; no SQLite handle is opened or closed here.
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            info.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(CleanupError::Io(io::Error::last_os_error()));
    }
    let info = unsafe { info.assume_init() };
    let expected = if is_directory {
        libc::S_IFDIR
    } else {
        libc::S_IFREG
    };
    if info.st_mode & libc::S_IFMT != expected || (!is_directory && info.st_nlink != 1) {
        return Err(CleanupError::Verification(Reason::Identity));
    }
    let original = identity(file).map_err(|_| CleanupError::Verification(Reason::Identity))?;
    if original != super::super::super::Identity::from_parts(info.st_dev as u64, info.st_ino) {
        return Err(CleanupError::Verification(Reason::Identity));
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn unlink(
    directory: &File,
    name: &std::ffi::CStr,
    expected: &File,
    flags: i32,
) -> Result<(), CleanupError> {
    use std::os::fd::AsRawFd;
    #[cfg(test)]
    checkpoint(if flags == 0 {
        "cleanup-p-unlink-syscall"
    } else {
        "cleanup-recovery-rmdir-syscall"
    })?;
    // Recheck after the final test boundary as well as the full proof check.
    // The name is relative to a retained fd, not a freshly resolved parent path.
    named(directory, name, expected, flags == libc::AT_REMOVEDIR)?;
    // SAFETY: live retained directory, fixed relative name and supported flag.
    // AT_REMOVEDIR atomically refuses nonempty directories; never recurse.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) } != 0 {
        return Err(CleanupError::Io(io::Error::last_os_error()));
    }
    Ok(())
}
fn run(
    receipt: &mut ReceiptDurable<'_, '_>,
    progress: &mut CleanupProgress,
) -> Result<(), CleanupError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (receipt, progress);
        return Err(CleanupError::Unsupported);
    }
    #[cfg(target_os = "macos")]
    {
        check(receipt)?;
        // Reconfirm the unchanged retained Q before any destructive operation.
        q_sync(receipt, "cleanup-q-file", SyncPoint::VerifiedFile)?;
        q_sync(receipt, "cleanup-q-directory", SyncPoint::Directory)?;
        q_sync(receipt, "cleanup-q-final", SyncPoint::FinalFileBarrier)?;
        check(receipt)?;
        if observation(receipt)
            .files
            .evidence
            .marker_handle()
            .is_some()
        {
            checkpoint("cleanup-before-p-unlink")?;
            check(receipt)?;
            let files = &observation(receipt).files;
            let directory = files
                .evidence
                .recovery_handle()
                .ok_or(CleanupError::Verification(Reason::Marker))?;
            let marker = files
                .evidence
                .marker_handle()
                .ok_or(CleanupError::Verification(Reason::Marker))?;
            named(directory, c"pending-restore.json", marker, false)?;
            *progress = CleanupProgress::PendingUnlinkAttempted;
            unlink(directory, c"pending-restore.json", marker, 0)?;
            receipt
                .proof
                .assessment
                .observation
                .as_mut()
                .unwrap()
                .files
                .evidence
                .did_remove_marker();
            *progress = CleanupProgress::PendingUnlinked;
            checkpoint("cleanup-p-unlinked")?;
        }
        if let Some(directory) = observation(receipt).files.evidence.recovery_handle() {
            checkpoint("cleanup-before-p-directory-sync")?;
            check(receipt)?;
            barrier("cleanup-p-directory", SyncPoint::Directory, directory)
                .map_err(CleanupError::Io)?;
            checkpoint("cleanup-after-p-directory-sync")?;
            q_sync(receipt, "cleanup-p-final", SyncPoint::FinalFileBarrier)?;
            check(receipt)?;
            *progress = CleanupProgress::PendingDurable;
            checkpoint("cleanup-before-recovery-rmdir")?;
            check(receipt)?;
            let files = &observation(receipt).files;
            // Existing layout verification requires exactly zero children now.
            // Recheck the retained directory and relative named entry last.
            let directory = files.evidence.recovery_handle().unwrap();
            current(directory, &files.root.join("recovery"), true)
                .map_err(CleanupError::Verification)?;
            named(&files.root_file, c"recovery", directory, true)?;
            *progress = CleanupProgress::RecoveryUnlinkAttempted;
            unlink(&files.root_file, c"recovery", directory, libc::AT_REMOVEDIR)?;
            receipt
                .proof
                .assessment
                .observation
                .as_mut()
                .unwrap()
                .files
                .evidence
                .did_remove_recovery();
            *progress = CleanupProgress::RecoveryUnlinked;
            checkpoint("cleanup-recovery-unlinked")?;
        } else {
            // Q-only resumption: its retained-root fsync above already confirmed
            // observed absence. No missing directory/database is ever created.
            *progress = CleanupProgress::PendingDurable;
        }
        checkpoint("cleanup-before-root-sync")?;
        check(receipt)?;
        barrier(
            "cleanup-root",
            SyncPoint::Directory,
            &observation(receipt).files.root_file,
        )
        .map_err(CleanupError::Io)?;
        checkpoint("cleanup-after-root-sync")?;
        q_sync(receipt, "cleanup-root-final", SyncPoint::FinalFileBarrier)?;
        check(receipt)?;
        *progress = CleanupProgress::RecoveryDurable;
        checkpoint("cleanup-final-check")?;
        check(receipt)
    }
}
pub(super) fn remove<'a, 's>(
    mut receipt: ReceiptDurable<'a, 's>,
) -> Result<RecoveryDirectoryRemoved<'a, 's>, CleanupFailure<'a, 's>> {
    let mut progress = CleanupProgress::Unchanged;
    match run(&mut receipt, &mut progress) {
        Ok(()) => Ok(RecoveryDirectoryRemoved { receipt }),
        Err(error) => Err(CleanupFailure {
            receipt,
            error,
            progress,
        }),
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "cleanup_tests.rs"]
mod tests;
