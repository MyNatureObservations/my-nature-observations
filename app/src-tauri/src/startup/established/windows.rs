//! Local-NTFS establishment only. No migration, restore or recovery authority.
use super::*;
use crate::startup::{identity, open_directory};
use std::{
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, PathBuf, Prefix},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetDriveTypeW,
    GetVolumeInformationByHandleW, GetVolumePathNameW, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    SetFileAttributesW,
};
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "database establishment path changed",
    )
}
fn plain_directory(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || !plain(&m) {
        return Err(invalid());
    }
    Ok(())
}
fn directory_handle(path: &Path) -> io::Result<File> {
    plain_directory(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_dir() || !plain(&m) {
        return Err(invalid());
    }
    Ok(file)
}
fn require_ntfs(path: &Path, file: &File) -> io::Result<()> {
    let mut name = [0u16; 32];
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let end = name.iter().position(|c| *c == 0).ok_or_else(invalid)?;
    if String::from_utf16(&name[..end]).map_err(|_| invalid())? != "NTFS" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "database initialization requires local NTFS",
        ));
    }
    let mut volume = vec![0u16; 32768];
    if unsafe {
        GetVolumePathNameW(
            wide(path).as_ptr(),
            volume.as_mut_ptr(),
            volume.len() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if !matches!(unsafe { GetDriveTypeW(volume.as_ptr()) }, 2 | 3) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "database initialization requires a local drive",
        ));
    }
    Ok(())
}
// Held without DELETE sharing. The first pre-existing ancestor is a boundary,
// not a request to open every system-owned ancestor up to the drive root.
pub(in crate::startup) struct Boundary {
    ancestor: PathBuf,
    anchor: File,
    directories: Vec<(PathBuf, File)>,
}
impl Boundary {
    pub(in crate::startup) fn capture(path: &Path) -> io::Result<Self> {
        if !path.is_absolute()
            || !matches!(path.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "database initialization requires an absolute local NTFS path",
            ));
        }
        let mut existing = path;
        match fs::symlink_metadata(path) {
            Ok(m) => {
                if !m.is_dir() || !plain(&m) {
                    return Err(invalid());
                }
                existing = path.parent().ok_or_else(invalid)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        loop {
            match fs::symlink_metadata(existing) {
                Ok(m) => {
                    if !m.is_dir() || !plain(&m) {
                        return Err(invalid());
                    }
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or_else(invalid)?
                }
                Err(e) => return Err(e),
            }
        }
        let ancestor = existing.canonicalize()?;
        let anchor = directory_handle(&ancestor)?;
        require_ntfs(&ancestor, &anchor)?;
        Ok(Self {
            ancestor,
            anchor,
            directories: Vec::new(),
        })
    }
    pub(in crate::startup) fn attach(&mut self, root: &Path) -> io::Result<()> {
        if !root.starts_with(&self.ancestor) || root == self.ancestor {
            return Err(invalid());
        }
        let mut path = root;
        while path != self.ancestor {
            let file = directory_handle(path)?;
            require_ntfs(path, &file)?;
            self.directories.push((path.into(), file));
            path = path.parent().ok_or_else(invalid)?;
        }
        self.check()
    }
    fn check(&self) -> io::Result<()> {
        for (path, file) in self
            .directories
            .iter()
            .map(|(p, f)| (p.as_path(), f))
            .chain(std::iter::once((self.ancestor.as_path(), &self.anchor)))
        {
            plain_directory(path)?;
            if identity(file)? != identity(&open_directory(path)?)? {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn sync(&self) -> io::Result<()> {
        self.check()?;
        for (_, file) in &self.directories {
            fault(Fault::DirectoryFlush)?;
            file.sync_all()?;
        }
        fault(Fault::DirectoryFlush)?;
        self.anchor.sync_all()?;
        self.check()
    }
    #[cfg(test)]
    pub(in crate::startup) fn paths(&self) -> Vec<PathBuf> {
        self.directories
            .iter()
            .map(|(p, _)| p.clone())
            .chain(std::iter::once(self.ancestor.clone()))
            .collect()
    }
}
fn opened_file(path: &Path) -> io::Result<File> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || !plain(&m) {
        return Err(invalid());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file() || !plain(&m) {
        return Err(invalid());
    }
    Ok(file)
}
fn flush(file: &File) -> io::Result<()> {
    fault(Fault::FileFlush)?;
    file.sync_all()
}
pub(in crate::startup) fn publish(root: &Path, boundary: &Boundary) -> Result<(), String> {
    publish_with(root, boundary, |_| Ok(()))
}
pub(in crate::startup) fn publish_with(
    root: &Path,
    boundary: &Boundary,
    mut hook: impl FnMut(Point) -> io::Result<()>,
) -> Result<(), String> {
    // Establish all newly created directory edges before any marker/database work.
    boundary.sync().map_err(failure)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".mno-init-")
        .tempfile_in(root)
        .map_err(failure)?;
    fault(Fault::MarkerWrite).map_err(failure)?;
    temporary.write_all(CONTENT).map_err(failure)?;
    temporary.flush().map_err(failure)?;
    flush(temporary.as_file()).map_err(failure)?;
    hook(Point::BeforePublish).map_err(failure)?;
    let source = temporary.into_temp_path();
    let from = wide(&source);
    let to = wide(&root.join(NAME));
    if unsafe { SetFileAttributesW(from.as_ptr(), FILE_ATTRIBUTE_NORMAL) } == 0 {
        return Err(failure(io::Error::last_os_error()));
    }
    // No REPLACE_EXISTING or COPY_ALLOWED. TempPath cleans only its owned source.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(failure(io::Error::last_os_error()));
    }
    hook(Point::AfterPublish).map_err(failure)?;
    boundary.sync().map_err(failure)?;
    flush(&opened_file(&root.join(NAME)).map_err(failure)?).map_err(failure)
}
pub(in crate::startup) fn confirm(root: &Path, boundary: &Boundary) -> Result<(), String> {
    let mut file = opened_file(&root.join(NAME)).map_err(failure)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(CONTENT.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if !valid_bytes(&bytes) {
        return Err("database_initialization_marker_invalid".into());
    }
    flush(&file).map_err(failure)?;
    boundary.sync().map_err(failure)?;
    flush(&file).map_err(failure)
}
pub(in crate::startup) fn sync_created_database(
    path: &Path,
    boundary: &Boundary,
) -> Result<(), String> {
    let file = opened_file(path).map_err(failure)?;
    flush(&file).map_err(failure)?;
    boundary.sync().map_err(failure)?;
    flush(&file).map_err(failure)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::startup) enum Fault {
    MarkerWrite,
    FileFlush,
    DirectoryFlush,
}
#[cfg(test)]
thread_local! {static FAILURE:std::cell::Cell<Option<Fault>>=const{std::cell::Cell::new(None)};}
fn fault(point: Fault) -> io::Result<()> {
    #[cfg(test)]
    if FAILURE.get() == Some(point) {
        return Err(io::Error::from(io::ErrorKind::StorageFull));
    }
    let _ = point;
    Ok(())
}
#[cfg(test)]
pub(in crate::startup) fn with_fault<T>(point: Fault, action: impl FnOnce() -> T) -> T {
    struct Reset(Option<Fault>);
    impl Drop for Reset {
        fn drop(&mut self) {
            FAILURE.set(self.0);
        }
    }
    let _reset = Reset(FAILURE.replace(Some(point)));
    action()
}

#[cfg(test)]
#[path = "selection_probe.rs"]
mod selection_probe;

#[path = "managed_layout.rs"]
pub(in crate::startup) mod managed_layout;
