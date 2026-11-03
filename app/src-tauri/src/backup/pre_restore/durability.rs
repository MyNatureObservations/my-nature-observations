//! Required barriers for mandatory pre-restore protection, never best-effort success.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backup) enum SyncPoint {
    VerifiedFile,
    Directory,
    FinalFileBarrier,
}

#[derive(Debug)]
pub(in crate::backup) struct SyncFailure(pub io::Error);
impl std::fmt::Display for SyncFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "required protection sync failed: {}", self.0)
    }
}
impl std::error::Error for SyncFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
pub(in crate::backup) fn require_supported() -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(ProtectionError::DurabilityUnsupported)
    }
}
pub(in crate::backup) fn sync(point: SyncPoint, path: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if point == SyncPoint::Directory {
            use std::os::unix::fs::OpenOptionsExt;
            let directory = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(path)?;
            return sync_opened(point, &directory);
        }
        sync_opened(point, &File::open(path)?)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (point, path);
        unsupported()
    }
}
// Same mandatory barriers, now also usable without reopening a retained object by name.
pub(in crate::backup) fn sync_opened(point: SyncPoint, file: &File) -> io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if point == SyncPoint::Directory {
            use std::os::fd::AsRawFd;
            loop {
                // Directories use fsync, not macOS F_FULLFSYNC.
                if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
                    return Ok(());
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            loop {
                if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
                    return Ok(());
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            file.sync_all()
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (point, file);
        unsupported()
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn unsupported() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mandatory protection durability is not supported",
    ))
}
