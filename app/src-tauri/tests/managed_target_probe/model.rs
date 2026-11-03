//! Test-only main-file/VFS boundary, NOT a registered writer VFS or SQL API.
use rusqlite::ffi;
use std::{
    ffi::{CString, c_int, c_void},
    fs::{File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
    ptr,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Identity,
    Sidecar,
    Io,
    Sqlite(c_int),
}
fn id(f: &File) -> io::Result<(u64, u64)> {
    let m = f.metadata()?;
    Ok((m.dev(), m.ino()))
}
fn plain(f: &File) -> bool {
    f.metadata()
        .is_ok_and(|m| m.is_file() && m.nlink() == 1 && m.uid() == unsafe { libc::geteuid() })
}
// Only the private fixture factory establishes this baseline. No production token.
pub(super) struct Fixture {
    pub(super) sandbox: tempfile::TempDir,
    root: File,
    identity: (u64, u64),
    directory: (u64, u64),
}
impl Fixture {
    pub(super) fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        let path = sandbox.path().join("mno.sqlite");
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE sample(value); INSERT INTO sample VALUES(7);")
            .unwrap();
        c.close().unwrap();
        let root = File::open(sandbox.path()).unwrap();
        let identity = id(&File::open(&path).unwrap()).unwrap();
        let directory = id(&root).unwrap();
        Self {
            sandbox,
            root,
            identity,
            directory,
        }
    }
    pub(super) fn path(&self) -> PathBuf {
        self.sandbox.path().join("mno.sqlite")
    }
    fn check(&self) -> Result<(), Error> {
        let named = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(self.sandbox.path())
            .map_err(|_| Error::Io)?;
        if id(&self.root).map_err(|_| Error::Io)? != self.directory
            || id(&named).map_err(|_| Error::Io)? != self.directory
        {
            return Err(Error::Identity);
        }
        // This probe reserves all SQLite suffixes, including ASCII case aliases.
        // No CREATE, delete, or xAccess claim about absent files is delegated.
        for entry in std::fs::read_dir(self.sandbox.path()).map_err(|_| Error::Io)? {
            let entry = entry.map_err(|_| Error::Io)?;
            use std::os::unix::ffi::OsStrExt;
            let name = entry.file_name();
            if name
                .as_bytes()
                .get(..11)
                .is_some_and(|p| p.eq_ignore_ascii_case(b"mno.sqlite-"))
            {
                return Err(Error::Sidecar);
            }
        }
        Ok(())
    }
}
// Original root and verified descriptor stay borrowed/owned until native close.
// No Clone/Copy, SQL accessor, path parameter, xWrite, xDelete, or raw handle API.
pub(super) struct Target<'a> {
    fixture: &'a Fixture,
    held: Option<File>,
    native: *mut ffi::sqlite3_file,
    filename: ffi::sqlite3_filename,
}
impl std::fmt::Debug for Target<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Target(private)")
    }
}
impl<'a> Target<'a> {
    pub(super) fn open(fixture: &'a Fixture) -> Result<Self, Error> {
        fixture.check()?;
        // Fixed leaf name and retained directory; neither CREATE nor TRUNC.
        let fd = unsafe {
            libc::openat(
                fixture.root.as_raw_fd(),
                c"mno.sqlite".as_ptr(),
                libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(Error::Io);
        }
        use std::os::fd::FromRawFd;
        let held = unsafe { File::from_raw_fd(fd) };
        if !plain(&held) || id(&held).map_err(|_| Error::Io)? != fixture.identity {
            return Err(Error::Identity);
        }
        let alias = CString::new(format!("/dev/fd/{fd}")).unwrap();
        // sqlite3_filename has SQLite-owned URI metadata, not a bare CString.
        let filename = unsafe {
            ffi::sqlite3_create_filename(
                alias.as_ptr(),
                c"".as_ptr(),
                c"".as_ptr(),
                0,
                ptr::null_mut(),
            )
        };
        if filename.is_null() {
            return Err(Error::Sqlite(ffi::SQLITE_NOMEM));
        }
        // Select documented POSIX native VFS, never unix-none/excl/dotfile.
        let vfs = unsafe { ffi::sqlite3_vfs_find(c"unix".as_ptr()) };
        if vfs.is_null() {
            unsafe { ffi::sqlite3_free_filename(filename) };
            return Err(Error::Sqlite(ffi::SQLITE_NOTFOUND));
        }
        let file =
            unsafe { ffi::sqlite3_malloc64((*vfs).szOsFile as u64) }.cast::<ffi::sqlite3_file>();
        if file.is_null() {
            unsafe { ffi::sqlite3_free_filename(filename) };
            return Err(Error::Sqlite(ffi::SQLITE_NOMEM));
        }
        unsafe { ptr::write_bytes(file.cast::<u8>(), 0, (*vfs).szOsFile as usize) };
        let mut target = Self {
            fixture,
            held: Some(held),
            native: file,
            filename,
        };
        let mut out = 0;
        let rc = unsafe {
            ((*vfs).xOpen.unwrap())(
                vfs,
                filename,
                file,
                ffi::SQLITE_OPEN_MAIN_DB | ffi::SQLITE_OPEN_READWRITE,
                &mut out,
            )
        };
        if rc != ffi::SQLITE_OK {
            return Err(Error::Sqlite(rc));
        }
        if out & ffi::SQLITE_OPEN_READWRITE == 0
            || out & (ffi::SQLITE_OPEN_CREATE | ffi::SQLITE_OPEN_READONLY) != 0
        {
            return Err(Error::Identity);
        }
        target.check()?;
        Ok(target)
    }
    pub(super) fn check(&mut self) -> Result<(), Error> {
        self.fixture.check()?;
        let held = self.held.as_ref().ok_or(Error::Identity)?;
        if !plain(held) || id(held).map_err(|_| Error::Io)? != self.fixture.identity {
            return Err(Error::Identity);
        }
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        let rc = unsafe {
            libc::fstatat(
                self.fixture.root.as_raw_fd(),
                c"mno.sqlite".as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc != 0 {
            return Err(Error::Io);
        }
        let st = unsafe { st.assume_init() };
        if (st.st_dev as u64, st.st_ino) != self.fixture.identity
            || st.st_mode & libc::S_IFMT != libc::S_IFREG
            || st.st_nlink != 1
        {
            return Err(Error::Identity);
        }
        // Native HAS_MOVED checks the SQLite-opened object against the retained
        // /dev/fd alias. Not evidence of the main pathname; checked above.
        let mut moved = -1;
        let rc = unsafe {
            ((*(*self.native).pMethods).xFileControl.unwrap())(
                self.native,
                ffi::SQLITE_FCNTL_HAS_MOVED,
                (&mut moved as *mut c_int).cast::<c_void>(),
            )
        };
        if rc != ffi::SQLITE_OK {
            return Err(Error::Sqlite(rc));
        }
        if moved != 0 {
            return Err(Error::Identity);
        }
        Ok(())
    }
    pub(super) fn lock(&mut self, level: c_int) -> Result<(), Error> {
        self.check()?;
        let rc = unsafe { ((*(*self.native).pMethods).xLock.unwrap())(self.native, level) };
        result(rc)
    }
    pub(super) fn unlock(&mut self, level: c_int) -> Result<(), Error> {
        let rc = unsafe { ((*(*self.native).pMethods).xUnlock.unwrap())(self.native, level) };
        result(rc)
    }
    pub(super) fn reserved(&mut self) -> Result<bool, Error> {
        let mut out = -1;
        let rc = unsafe {
            ((*(*self.native).pMethods).xCheckReservedLock.unwrap())(self.native, &mut out)
        };
        result(rc)?;
        Ok(out != 0)
    }
    pub(super) fn lock_state(&mut self) -> Result<c_int, Error> {
        let mut out = -1;
        let rc = unsafe {
            ((*(*self.native).pMethods).xFileControl.unwrap())(
                self.native,
                ffi::SQLITE_FCNTL_LOCKSTATE,
                (&mut out as *mut c_int).cast(),
            )
        };
        result(rc)?;
        Ok(out)
    }
    // A native xClose error does NOT mean the file can be closed again. Retain
    // storage and the descriptor conservatively; never retry a consumed fd.
    pub(super) fn close(mut self) -> Result<(), Error> {
        self.close_once()
    }
    fn close_once(&mut self) -> Result<(), Error> {
        if self.native.is_null() {
            return Ok(());
        }
        let file = self.native;
        self.native = ptr::null_mut();
        let methods = unsafe { (*file).pMethods };
        let rc = if methods.is_null() {
            ffi::SQLITE_OK
        } else {
            unsafe { ((*methods).xClose.unwrap())(file) }
        };
        if rc != ffi::SQLITE_OK {
            std::mem::forget(self.held.take());
            self.filename = ptr::null(); // Retain filename + allocation too.
            return Err(Error::Sqlite(rc));
        }
        unsafe {
            ffi::sqlite3_free(file.cast());
            ffi::sqlite3_free_filename(self.filename)
        };
        self.filename = ptr::null();
        self.held.take();
        Ok(())
    }
}
impl Drop for Target<'_> {
    fn drop(&mut self) {
        let _ = self.close_once();
    }
}
fn result(rc: c_int) -> Result<(), Error> {
    if rc == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(Error::Sqlite(rc))
    }
}
