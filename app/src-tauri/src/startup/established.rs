//! Immutable installation intent, not a restore marker or authorization.
use super::{failure, plain};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};
pub(super) const NAME: &str = ".mno-initialized-v1.json";
const CONTENT: &[u8] = b"{\"format\":\"mno-initialized\",\"version\":1}\n";
pub(super) fn read(root: &Path) -> Result<bool, String> {
    read_with(root, |path| {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        options.open(path)
    })
}
fn read_with(root: &Path, open: impl FnOnce(&Path) -> io::Result<File>) -> Result<bool, String> {
    let path = root.join(NAME);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(failure(e)),
        Ok(m) if !m.is_file() || !plain(&m) => {
            return Err("database_initialization_marker_invalid".into());
        }
        Ok(_) => {}
    }
    let file = open(&path).map_err(failure)?;
    let metadata = file.metadata().map_err(failure)?;
    if !metadata.is_file() || !plain(&metadata) {
        return Err("database_initialization_marker_invalid".into());
    }
    let mut bytes = Vec::new();
    file.take(CONTENT.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if !valid_bytes(&bytes) {
        return Err("database_initialization_marker_invalid".into());
    }
    Ok(true)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    BeforePublish,
    AfterPublish,
}
pub(super) fn publish(root: &Path) -> Result<(), String> {
    publish_with(root, |_| Ok(()))
}
pub(super) fn publish_with(
    root: &Path,
    mut hook: impl FnMut(Point) -> io::Result<()>,
) -> Result<(), String> {
    require_supported()?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".mno-init-")
        .tempfile_in(root)
        .map_err(failure)?;
    temporary.write_all(CONTENT).map_err(failure)?;
    temporary.flush().map_err(failure)?;
    sync_file(temporary.as_file()).map_err(failure)?;
    hook(Point::BeforePublish).map_err(failure)?;
    // persist_noclobber never replaces an existing marker. A late error must
    // never remove the final marker, even if a required barrier failed.
    let published = temporary
        .persist_noclobber(root.join(NAME))
        .map_err(|e| failure(e.error))?;
    hook(Point::AfterPublish).map_err(failure)?;
    for directory in root.canonicalize().map_err(failure)?.ancestors() {
        sync_directory(directory).map_err(failure)?;
    }
    sync_file(&published).map_err(failure)?;
    Ok(())
}
fn require_supported() -> Result<(), String> {
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        Ok(())
    } else {
        Err("database_initialization_durability_unsupported".into())
    }
}
fn sync_file(file: &File) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        loop {
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        file.sync_all()
    }
}
fn sync_directory(root: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(root)?;
        loop {
            if unsafe { libc::fsync(directory.as_raw_fd()) } == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
pub(super) fn confirm(root: &Path) -> Result<(), String> {
    require_supported()?;
    let file = File::open(root.join(NAME)).map_err(failure)?;
    sync_file(&file).map_err(failure)?;
    for directory in root.canonicalize().map_err(failure)?.ancestors() {
        sync_directory(directory).map_err(failure)?;
    }
    sync_file(&file).map_err(failure)
}
pub(super) fn sync_created_database(path: &Path, root: &Path) -> Result<(), String> {
    require_supported()?;
    let file = File::open(path).map_err(failure)?;
    sync_file(&file).map_err(failure)?;
    // Includes root's own entry and all existing ancestors, so freshly created
    // nested data directories are not an unflushed gap in the creation protocol.
    for directory in root.canonicalize().map_err(failure)?.ancestors() {
        sync_directory(directory).map_err(failure)?;
    }
    sync_file(&file).map_err(failure)
}
// Shared exact format validation; recovery never publishes or repairs this file.
fn valid_bytes(bytes: &[u8]) -> bool {
    bytes == CONTENT
}

// Metadata only. No startup approval, recovery proof or mutation capability.
pub(crate) struct RetainedMarker {
    file: File,
}
impl RetainedMarker {
    // Read-only opened descriptor; used for mandatory barriers without reopening.
    pub(crate) fn retained_file(&self) -> &File {
        &self.file
    }

    pub(crate) fn open_at(directory: &File) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            // Fixed single-component name, retained directory, no symlink following.
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    c".mno-initialized-v1.json".as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(failure(io::Error::last_os_error()));
            }
            // SAFETY: successful openat returns a new owned descriptor.
            let marker = Self {
                file: unsafe { File::from_raw_fd(fd) },
            };
            marker.check(directory)?;
            Ok(marker)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = directory;
            Err("database_recovery_unsupported".into())
        }
    }
    pub(crate) fn check(&self, directory: &File) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            use std::os::{
                fd::AsRawFd,
                unix::fs::{FileExt, MetadataExt},
            };
            let named = || -> Result<(), String> {
                let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: valid fd, fixed NUL-terminated name and writable stat.
                if unsafe {
                    libc::fstatat(
                        directory.as_raw_fd(),
                        c".mno-initialized-v1.json".as_ptr(),
                        stat.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                } != 0
                {
                    return Err(failure(io::Error::last_os_error()));
                }
                let stat = unsafe { stat.assume_init() };
                let meta = self.file.metadata().map_err(failure)?;
                if !meta.is_file()
                    || meta.nlink() != 1
                    || stat.st_mode & libc::S_IFMT != libc::S_IFREG
                    || stat.st_nlink != 1
                    || meta.dev() != stat.st_dev as u64
                    || meta.ino() != stat.st_ino
                {
                    return Err("database_initialization_marker_changed".into());
                }
                Ok(())
            };
            named()?;
            let mut bytes = vec![0; CONTENT.len() + 1];
            let mut offset = 0;
            while offset < bytes.len() {
                let n = self
                    .file
                    .read_at(&mut bytes[offset..], offset as u64)
                    .map_err(failure)?;
                if n == 0 {
                    break;
                }
                offset += n;
            }
            if !valid_bytes(&bytes[..offset]) {
                return Err("database_initialization_marker_invalid".into());
            }
            named()
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = directory;
            Err("database_recovery_unsupported".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreadable_marker_is_an_error_not_absence() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join(NAME), CONTENT).unwrap();
        assert!(
            read_with(dir.path(), |_| Err(io::Error::from(
                io::ErrorKind::PermissionDenied
            )))
            .is_err()
        );
    }
}
