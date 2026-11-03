//! Secure creation precedes writing any journal bytes. Fail closed on unsupported ACLs.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

pub(in crate::backup) struct PrivateDir(PathBuf);
impl PrivateDir {
    pub fn new(parent: &Path) -> io::Result<Self> {
        for _ in 0..8 {
            let path = parent.join(format!("mno-restore-{}", uuid::Uuid::new_v4()));
            match create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "temporary name collision",
        ))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[cfg(unix)]
fn create_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new().mode(0o700).create(path)?;
    // Creation cannot expose more than 0700; restore owner permissions if umask removed them.
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o700)) {
        let _ = fs::remove_dir(path);
        return Err(e);
    }
    Ok(())
}
#[cfg(windows)]
fn create_dir(path: &Path) -> io::Result<()> {
    windows::create_dir(path)
}

pub(in crate::backup) fn create_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    windows::verify_private(path, false)?;
    Ok(file)
}
pub(in crate::backup) fn open_retained(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        // Pin the file against write/delete handles on Windows, in addition to SQLite locking.
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(path)
}
pub(in crate::backup) fn same_file(file: &File, path: &Path) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let a = file.metadata()?;
        let b = fs::metadata(path)?;
        Ok(a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(windows)]
    {
        Ok(windows::identity(file)? == windows::identity(&File::open(path)?)?)
    }
}
#[cfg(windows)]
mod windows;

#[cfg(all(windows, test))]
pub(in crate::backup) fn verify_permissions(path: &Path, directory: bool) -> io::Result<()> {
    windows::verify_private(path, directory)
}

// Fixed MNO-owned directories may already exist. Never follow a directory symlink.
pub(in crate::backup) fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !is_plain_directory(&metadata) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "managed directory must not be a link or file",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
            #[cfg(windows)]
            windows::verify_private(path, true)?;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => match create_dir(path) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => ensure_directory(path),
            result => result,
        },
        Err(error) => Err(error),
    }
}

pub(in crate::backup) fn is_plain_directory(metadata: &fs::Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reject junctions and other reparse points as well as symbolic links.
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::backup) struct FileIdentity {
    volume: u64,
    file: u64,
}
impl FileIdentity {
    pub(in crate::backup) fn components(self) -> (u64, u64) {
        (self.volume, self.file)
    }
}
pub(in crate::backup) fn file_identity(file: &File) -> io::Result<FileIdentity> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(FileIdentity {
            volume: metadata.dev(),
            file: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let (volume, index) = windows::identity(file)?;
        Ok(FileIdentity {
            volume: u64::from(volume),
            file: index,
        })
    }
}
