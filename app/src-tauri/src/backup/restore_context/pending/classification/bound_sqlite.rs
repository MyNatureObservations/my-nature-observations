//! Descriptor-bound, read-only SQLite opening. No private unixFile layout is used.
use super::*;
use rusqlite::ffi;
use std::{
    cell::UnsafeCell,
    ffi::{CStr, CString, c_int, c_void},
    mem::ManuallyDrop,
    ptr,
    sync::atomic::AtomicPtr,
};

#[derive(Clone, Copy)]
pub(super) enum Control {
    Real,
    #[cfg(test)]
    FilePointerError,
    #[cfg(test)]
    FilePointerUnsupported,
    #[cfg(test)]
    FilePointerNull,
    #[cfg(test)]
    FilePointerUnexpected,
    #[cfg(test)]
    RegisterError,
    #[cfg(test)]
    UnregisterError,
    #[cfg(test)]
    Unsupported,
    #[cfg(test)]
    Error,
    #[cfg(test)]
    FailAt {
        call: usize,
        code: c_int,
    },
}
// SQLite must close before unregistering the VFS or releasing its filename/fd.
pub(super) struct BoundSqlite {
    connection: Option<Connection>,
    registration: Option<Registration>,
    #[cfg(test)]
    close_error: Option<c_int>,
}
impl BoundSqlite {
    pub(super) fn open(
        path: &Path,
        file: Arc<File>,
        control: Control,
    ) -> std::result::Result<Self, Reason> {
        if !cfg!(target_os = "macos") {
            return Err(Reason::Unsupported);
        }
        let registration = Registration::new(path, file, control)?;
        let connection = Connection::open_with_flags_and_vfs(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            registration.resources.name.as_c_str(),
        )
        .map_err(|_| Reason::Identity)?;
        let bound = Self {
            connection: Some(connection),
            registration: Some(registration),
            #[cfg(test)]
            close_error: None,
        };
        bound.verify()?;
        Ok(bound)
    }
    pub(super) fn is_autocommit(&self) -> bool {
        self.connection
            .as_ref()
            .expect("live binding")
            .is_autocommit()
    }
    pub(super) fn connection_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("live binding")
    }
    pub(super) fn verify(&self) -> std::result::Result<(), Reason> {
        // SAFETY: the Connection and registered Context own both pointers for
        // this call. Only documented SQLite file controls are used.
        unsafe {
            let mut actual: *mut ffi::sqlite3_file = ptr::null_mut();
            let rc = ffi::sqlite3_file_control(
                self.connection.as_ref().expect("live binding").handle(),
                c"main".as_ptr(),
                ffi::SQLITE_FCNTL_FILE_POINTER,
                (&mut actual as *mut *mut ffi::sqlite3_file).cast(),
            );
            #[cfg(test)]
            let rc = match self
                .registration
                .as_ref()
                .expect("registered binding")
                .resources
                .context
                .control
            {
                Control::FilePointerError => ffi::SQLITE_IOERR,
                Control::FilePointerUnsupported => ffi::SQLITE_NOTFOUND,
                Control::FilePointerNull => {
                    actual = ptr::null_mut();
                    rc
                }
                Control::FilePointerUnexpected => {
                    // Never dereferenced: pointer equality must reject it first.
                    actual = std::ptr::dangling_mut::<ffi::sqlite3_file>();
                    rc
                }
                _ => rc,
            };
            if rc != ffi::SQLITE_OK
                || actual.is_null()
                || actual
                    != self
                        .registration
                        .as_ref()
                        .expect("registered binding")
                        .resources
                        .context
                        .opened
                        .load(Ordering::Acquire)
                || check(
                    actual,
                    &self
                        .registration
                        .as_ref()
                        .expect("registered binding")
                        .resources
                        .context,
                ) != ffi::SQLITE_OK
            {
                return Err(Reason::Identity);
            }
        }
        Ok(())
    }
}
// These types never expose Connection, VFS pointers, or a retry/authorization API.
#[derive(Debug)]
pub(super) enum CloseError {
    Sqlite(rusqlite::Error),
    Unregister,
}
pub(super) struct ClosedBinding {
    _private: (),
}
pub(super) struct BoundCloseFailure {
    pub(super) error: CloseError,
    pub(super) binding: Box<BoundSqlite>,
}
impl BoundSqlite {
    pub(super) fn try_close(mut self) -> std::result::Result<ClosedBinding, BoundCloseFailure> {
        #[cfg(test)]
        if let Some(code) = self.close_error {
            return Err(BoundCloseFailure {
                error: CloseError::Sqlite(rusqlite::Error::SqliteFailure(
                    ffi::Error::new(code),
                    None,
                )),
                binding: Box::new(self),
            });
        }
        let connection = self.connection.take().expect("live binding");
        // If Connection::close unwinds, retain the entire registered VFS. A
        // panic is not proof that SQLite released its callback references.
        let mut registration = DuringClose(self.registration.take());
        if let Err((connection, error)) = connection.close() {
            self.connection = Some(connection);
            self.registration = registration.0.take();
            return Err(BoundCloseFailure {
                error: CloseError::Sqlite(error),
                binding: Box::new(self),
            });
        }
        // No live SQLite object may reference this registration after close OK.
        let mut registration = registration.0.take().expect("registered binding");
        if !registration.unregister() {
            self.registration = Some(registration);
            return Err(BoundCloseFailure {
                error: CloseError::Unregister,
                binding: Box::new(self),
            });
        }
        registration.release_unregistered();
        Ok(ClosedBinding { _private: () })
    }
}
impl Drop for BoundSqlite {
    fn drop(&mut self) {
        // Drop cannot report failure. Never unregister or free a VFS after a
        // failed close: retain both connection and callback storage conservatively.
        if let Some(connection) = self.connection.take() {
            let mut registration = DuringClose(self.registration.take());
            if let Err((connection, _)) = connection.close() {
                std::mem::forget(connection);
                return; // DuringClose conservatively retains registration.
            }
            self.registration = registration.0.take();
        }
        // Only a successfully closed connection reaches Registration::drop.
    }
}
// Never frees registered memory during an unwinding or failed close attempt.
struct DuringClose(Option<Registration>);
impl Drop for DuringClose {
    fn drop(&mut self) {
        if let Some(registration) = self.0.take() {
            std::mem::forget(registration);
        }
    }
}
#[cfg(test)]
impl BoundSqlite {
    pub(super) fn test_unregister_failure(&mut self) {
        self.registration
            .as_mut()
            .unwrap()
            .resources
            .context
            .control = Control::UnregisterError;
    }
    pub(super) fn test_close_error(&mut self, code: c_int) {
        self.close_error = Some(code);
    }
    pub(super) fn test_name(&self) -> CString {
        self.registration.as_ref().unwrap().resources.name.clone()
    }
    pub(super) fn test_is_open(&self) -> bool {
        self.connection.is_some()
    }
    pub(super) fn test_statement(&self) -> *mut ffi::sqlite3_stmt {
        let mut statement = ptr::null_mut();
        // A deliberately outstanding real SQLite statement. Tests must finalize
        // it while the retained failed owner is still alive.
        assert_eq!(
            unsafe {
                ffi::sqlite3_prepare_v2(
                    self.connection.as_ref().unwrap().handle(),
                    c"SELECT 1".as_ptr(),
                    -1,
                    &mut statement,
                    ptr::null_mut(),
                )
            },
            ffi::SQLITE_OK
        );
        statement
    }
}
// SQLite owns mutation of pNext under its registry mutex. No Rust reference to
// sqlite3_vfs is formed while registered. Context is a separate allocation.
#[repr(C)]
struct RegisteredVfs {
    vfs: UnsafeCell<ffi::sqlite3_vfs>,
    context: *const Context,
}
impl RegisteredVfs {
    fn sqlite_ptr(&self) -> *mut ffi::sqlite3_vfs {
        // Derive from the WHOLE allocation, not from &self.vfs/get(). repr(C)
        // places vfs at offset zero and UnsafeCell<T> has T's layout. This cast
        // preserves provenance for both the SQLite prefix and the sibling
        // context pointer read by callbacks. The shared whole-object reference
        // permits SQLite mutation only inside UnsafeCell; context stays immutable.
        // Box ownership keeps the allocation stable until successful unregister.
        ptr::from_ref(self).cast::<ffi::sqlite3_vfs>().cast_mut()
    }
}
const _: () = assert!(std::mem::offset_of!(RegisteredVfs, vfs) == 0);
struct Context {
    native: *mut ffi::sqlite3_vfs,
    alias: ffi::sqlite3_filename,
    expected: CString,
    held: Arc<File>,
    opened: AtomicPtr<ffi::sqlite3_file>,
    control: Control,
    #[cfg(test)]
    checks: std::sync::atomic::AtomicUsize,
}
struct Filename(ffi::sqlite3_filename);
impl Drop for Filename {
    fn drop(&mut self) {
        // SAFETY: created by sqlite3_create_filename, freed once after SQLite closes.
        unsafe { ffi::sqlite3_free_filename(self.0) };
    }
}
struct Registration {
    // On an unregister error, retain every allocation SQLite may still reference.
    resources: ManuallyDrop<Resources>,
}
struct Resources {
    vfs: Box<RegisteredVfs>,
    context: Box<Context>,
    name: CString,
    _filename: Filename,
}
fn descriptor_alias(file: &File) -> std::result::Result<CString, Reason> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        CString::new(format!("/dev/fd/{}", file.as_raw_fd())).map_err(|_| Reason::Identity)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(Reason::Unsupported)
    }
}
impl Registration {
    fn new(path: &Path, file: Arc<File>, control: Control) -> std::result::Result<Self, Reason> {
        let alias = descriptor_alias(&file)?;
        let expected =
            CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| Reason::Identity)?;
        let journal = CString::new([expected.as_bytes(), b"-journal"].concat())
            .map_err(|_| Reason::Identity)?;
        let wal =
            CString::new([expected.as_bytes(), b"-wal"].concat()).map_err(|_| Reason::Identity)?;
        let name = CString::new(format!("mno-bound-{}", uuid::Uuid::new_v4()))
            .map_err(|_| Reason::Identity)?;
        // SAFETY: linked bundled SQLite owns the native VFS. A boxed, stable
        // public VFS prefix is registered non-default until Registration::drop.
        unsafe {
            let native = ffi::sqlite3_vfs_find(c"unix".as_ptr());
            if native.is_null() || (*native).xOpen.is_none() || (*native).iVersion > 3 {
                return Err(Reason::Unsupported);
            }
            // A real sqlite3_filename is required by native xOpen; a CString is
            // insufficient because SQLite may consult its hidden URI metadata.
            let filename = Filename(ffi::sqlite3_create_filename(
                alias.as_ptr(),
                journal.as_ptr(),
                wal.as_ptr(),
                0,
                ptr::null_mut(),
            ));
            if filename.0.is_null() {
                return Err(Reason::Identity);
            }
            // SQLite guarantees all native fields except pNext are immutable
            // while registered. Read them individually, never aggregate-copy
            // native or create &sqlite3_vfs (which would also cover pNext).
            let vfs = ffi::sqlite3_vfs {
                iVersion: (*native).iVersion,
                szOsFile: (*native).szOsFile,
                mxPathname: (*native).mxPathname,
                pNext: ptr::null_mut(),
                zName: name.as_ptr(),
                pAppData: (*native).pAppData,
                xOpen: Some(open),
                xDelete: Some(delete),
                xAccess: (*native).xAccess,
                xFullPathname: (*native).xFullPathname,
                xDlOpen: (*native).xDlOpen,
                xDlError: (*native).xDlError,
                xDlSym: (*native).xDlSym,
                xDlClose: (*native).xDlClose,
                xRandomness: (*native).xRandomness,
                xSleep: (*native).xSleep,
                xCurrentTime: (*native).xCurrentTime,
                xGetLastError: (*native).xGetLastError,
                xCurrentTimeInt64: (*native).xCurrentTimeInt64,
                xSetSystemCall: (*native).xSetSystemCall,
                xGetSystemCall: (*native).xGetSystemCall,
                xNextSystemCall: (*native).xNextSystemCall,
            };
            let context = Box::new(Context {
                native,
                alias: filename.0,
                expected,
                held: file,
                opened: AtomicPtr::new(ptr::null_mut()),
                control,
                #[cfg(test)]
                checks: std::sync::atomic::AtomicUsize::new(0),
            });
            let vfs = Box::new(RegisteredVfs {
                vfs: UnsafeCell::new(vfs),
                context: &*context,
            });
            #[cfg(test)]
            if matches!(control, Control::RegisterError) {
                return Err(Reason::Unsupported);
            }
            if ffi::sqlite3_vfs_register(vfs.sqlite_ptr(), 0) != ffi::SQLITE_OK {
                return Err(Reason::Unsupported);
            }
            Ok(Self {
                resources: ManuallyDrop::new(Resources {
                    vfs,
                    context,
                    name,
                    _filename: filename,
                }),
            })
        }
    }
}
impl Registration {
    fn unregister(&mut self) -> bool {
        #[cfg(test)]
        if matches!(self.resources.context.control, Control::UnregisterError) {
            return false; // Model failure before unlinking.
        }
        // SAFETY: the whole-allocation-derived pointer retains callback access
        // to context; SQLite only mutates the UnsafeCell prefix under its mutex.
        // The allocation and callback state remain alive throughout this call.
        unsafe { ffi::sqlite3_vfs_unregister(self.resources.vfs.sqlite_ptr()) == ffi::SQLITE_OK }
    }
}
impl Registration {
    fn release_unregistered(self) {
        // Caller has obtained SQLITE_OK from unregister. Suppress the fallback
        // Drop, then free every allocation exactly once, without another C call.
        let mut this = ManuallyDrop::new(self);
        unsafe { ManuallyDrop::drop(&mut this.resources) };
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        // BoundSqlite has already closed its Connection. On unexpected unlink
        // failure, keep all allocations alive rather than leave dangling registry
        // pointers. No handle is freed until SQLite confirms unlinking.
        if self.unregister() {
            unsafe { ManuallyDrop::drop(&mut self.resources) };
        }
    }
}
unsafe fn check(file: *mut ffi::sqlite3_file, context: &Context) -> c_int {
    #[cfg(test)]
    {
        let call = context.checks.fetch_add(1, Ordering::AcqRel);
        match context.control {
            Control::Unsupported => return ffi::SQLITE_NOTFOUND,
            Control::Error => return ffi::SQLITE_IOERR,
            Control::FailAt { call: at, code } if call == at => return code,
            _ => (),
        }
    }
    let _ = context;
    // SAFETY: caller supplies a live native SQLite file. No native private layout.
    unsafe {
        if file.is_null() || (*file).pMethods.is_null() {
            return ffi::SQLITE_IOERR;
        }
        let Some(check) = (*(*file).pMethods).xFileControl else {
            return ffi::SQLITE_NOTFOUND;
        };
        let mut moved: c_int = -1;
        let rc = check(
            file,
            ffi::SQLITE_FCNTL_HAS_MOVED,
            (&mut moved as *mut c_int).cast::<c_void>(),
        );
        if rc == ffi::SQLITE_OK && moved != 0 {
            ffi::SQLITE_IOERR
        } else {
            rc
        }
    }
}
unsafe extern "C" fn open(
    vfs: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    // SAFETY: SQLite invokes this callback only with the registered repr(C)
    // RegisteredVfs prefix and valid xOpen arguments. It performs no panicking operations.
    unsafe {
        (*file).pMethods = ptr::null();
        // vfs originated from the whole RegisteredVfs allocation (sqlite_ptr),
        // so this sibling-field read does not widen a field-derived provenance.
        // Read only the immutable pointer, then borrow the separate Context;
        // never create &sqlite3_vfs or a reference to the mutated prefix.
        let context = &*ptr::addr_of!((*vfs.cast::<RegisteredVfs>()).context).read();
        let Some(native_open) = (*context.native).xOpen else {
            return ffi::SQLITE_CANTOPEN;
        };
        if flags & ffi::SQLITE_OPEN_MAIN_DB == 0 {
            // Persistent journal/WAL writes are outside this read-only VFS.
            if flags & ffi::SQLITE_OPEN_WAL != 0
                || (flags & (ffi::SQLITE_OPEN_MAIN_JOURNAL | ffi::SQLITE_OPEN_SUPER_JOURNAL) != 0
                    && flags
                        & (ffi::SQLITE_OPEN_READWRITE
                            | ffi::SQLITE_OPEN_CREATE
                            | ffi::SQLITE_OPEN_DELETEONCLOSE)
                        != 0)
            {
                return ffi::SQLITE_READONLY;
            }
            return native_open(context.native, name, file, flags, out);
        }
        (*file).pMethods = ptr::null();
        if name.is_null()
            || CStr::from_ptr(name) != context.expected.as_c_str()
            || flags & ffi::SQLITE_OPEN_READONLY == 0
            || flags
                & (ffi::SQLITE_OPEN_READWRITE
                    | ffi::SQLITE_OPEN_CREATE
                    | ffi::SQLITE_OPEN_DELETEONCLOSE)
                != 0
            || !context.opened.load(Ordering::Acquire).is_null()
        {
            return ffi::SQLITE_CANTOPEN;
        }
        // On macOS /dev/fd/N refers to the retained open file description, not
        // a second resolution of mno.sqlite. Native unix VFS keeps its normal
        // POSIX locking and read-only I/O methods. SQLite's pager still uses the
        // original database name for rollback/WAL checks.
        let rc = native_open(context.native, context.alias, file, flags, out);
        if rc != ffi::SQLITE_OK {
            return rc;
        }
        // HAS_MOVED now compares the actual native inode to the descriptor alias
        // backed by held. It is not used as a standalone pathname comparison.
        if check(file, context) != ffi::SQLITE_OK {
            if !(*file).pMethods.is_null() {
                if let Some(close) = (*(*file).pMethods).xClose {
                    let _ = close(file);
                }
            }
            (*file).pMethods = ptr::null();
            return ffi::SQLITE_CANTOPEN;
        }
        context.opened.store(file, Ordering::Release);
        ffi::SQLITE_OK
    }
}

unsafe extern "C" fn delete(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const std::ffi::c_char,
    sync: c_int,
) -> c_int {
    // SAFETY: registered RegisteredVfs prefix and nul-terminated name are supplied by SQLite.
    unsafe {
        // vfs originated from the whole RegisteredVfs allocation (sqlite_ptr),
        // so this sibling-field read does not widen a field-derived provenance.
        // Read only the immutable pointer, then borrow the separate Context;
        // never create &sqlite3_vfs or a reference to the mutated prefix.
        let context = &*ptr::addr_of!((*vfs.cast::<RegisteredVfs>()).context).read();
        if name.is_null()
            || CStr::from_ptr(name)
                .to_bytes()
                .starts_with(context.expected.as_bytes())
        {
            return ffi::SQLITE_READONLY;
        }
        match (*context.native).xDelete {
            Some(delete) => delete(context.native, name, sync),
            None => ffi::SQLITE_IOERR_DELETE,
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::sync::Barrier;

    fn source() -> (tempfile::TempDir, std::path::PathBuf, Arc<File>) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE sample(value TEXT); INSERT INTO sample VALUES ('retained')")
            .unwrap();
        drop(db);
        let path = fs::canonicalize(path).unwrap();
        let file = Arc::new(File::open(&path).unwrap());
        (dir, path, file)
    }
    fn read(bound: &mut BoundSqlite) {
        bound.verify().unwrap();
        let value: String = bound
            .connection_mut()
            .query_row("SELECT value FROM sample", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "retained");
    }

    #[test]
    fn concurrent_registrations_and_overlapping_unregistrations_keep_connections_valid() {
        const COUNT: usize = 8;
        let barrier = Barrier::new(COUNT);
        let names = std::sync::Mutex::new(std::collections::BTreeSet::new());
        // SQLite itself serializes its registry. Each thread owns its adapter;
        // no Send/Sync implementation or application-wide lock is needed.
        std::thread::scope(|scope| {
            for index in 0..COUNT {
                let barrier = &barrier;
                let names = &names;
                scope.spawn(move || {
                    let (_dir, path, file) = source();
                    barrier.wait();
                    let mut first =
                        Some(BoundSqlite::open(&path, file.clone(), Control::Real).unwrap());
                    let name = first
                        .as_ref()
                        .unwrap()
                        .registration
                        .as_ref()
                        .unwrap()
                        .resources
                        .name
                        .clone();
                    assert!(names.lock().unwrap().insert(name.clone()));
                    read(first.as_mut().unwrap());
                    // All registrations are simultaneously live before any drop.
                    barrier.wait();
                    if index % 2 == 0 {
                        drop(first.take());
                    }
                    barrier.wait();
                    if let Some(bound) = &mut first {
                        read(bound);
                    } else {
                        unsafe {
                            assert!(ffi::sqlite3_vfs_find(name.as_ptr()).is_null());
                        }
                    }
                    // A second registration wave overlaps the surviving first wave.
                    let mut second = BoundSqlite::open(&path, file, Control::Real).unwrap();
                    assert!(
                        names
                            .lock()
                            .unwrap()
                            .insert(second.registration.as_ref().unwrap().resources.name.clone())
                    );
                    barrier.wait();
                    read(&mut second);
                    drop(first);
                    read(&mut second);
                    let name = second.registration.as_ref().unwrap().resources.name.clone();
                    drop(second);
                    unsafe {
                        assert!(ffi::sqlite3_vfs_find(name.as_ptr()).is_null());
                    }
                });
            }
        });
        assert_eq!(names.into_inner().unwrap().len(), COUNT * 2);
    }

    #[test]
    fn dropping_busy_close_failure_never_frees_live_sqlite_callback_storage() {
        let (_dir, path, file) = source();
        let bound = BoundSqlite::open(&path, file.clone(), Control::Real).unwrap();
        let name = bound.test_name();
        let statement = bound.test_statement();
        let failed = bound.try_close().err().unwrap();
        assert!(
            matches!(&failed.error, CloseError::Sqlite(rusqlite::Error::SqliteFailure(e, _)) if e.extended_code == ffi::SQLITE_BUSY)
        );
        drop(failed); // outstanding statement still exists: Drop must leak safely.
        unsafe {
            assert!(!ffi::sqlite3_vfs_find(name.as_ptr()).is_null());
            assert_eq!(ffi::sqlite3_step(statement), ffi::SQLITE_ROW);
            assert_eq!(ffi::sqlite3_finalize(statement), ffi::SQLITE_OK);
        }
        assert!(Arc::strong_count(&file) > 1);
    }

    #[test]
    fn registration_failure_releases_descriptor_ownership() {
        let (_dir, path, file) = source();
        assert!(BoundSqlite::open(&path, file.clone(), Control::RegisterError).is_err());
        assert_eq!(Arc::strong_count(&file), 1);
        read(&mut BoundSqlite::open(&path, file, Control::Real).unwrap());
    }

    #[test]
    fn unregistration_failure_retains_callback_storage_until_successful_unlink() {
        let (_dir, path, file) = source();
        let mut registration =
            Registration::new(&path, file.clone(), Control::UnregisterError).unwrap();
        let name = registration.resources.name.clone();
        let pointer = registration.resources.vfs.sqlite_ptr();
        // Check the layout used by both C registration and callback recovery.
        assert_eq!(
            pointer.cast::<RegisteredVfs>().cast_const(),
            ptr::from_ref(&*registration.resources.vfs)
        );
        unsafe {
            let context = ptr::addr_of!((*pointer.cast::<RegisteredVfs>()).context).read();
            assert_eq!(context, ptr::from_ref(&*registration.resources.context));
        }
        // Exercise the same failing unlink path as Drop, while retaining an
        // owner for a later successful cleanup (no access to a dropped object).
        assert!(!registration.unregister());
        unsafe {
            assert_eq!(ffi::sqlite3_vfs_find(name.as_ptr()), pointer);
        }
        let db = Connection::open_with_flags_and_vfs(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
            name.as_c_str(),
        )
        .unwrap();
        let value: String = db
            .query_row("SELECT value FROM sample", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "retained");
        drop(db);
        assert!(Arc::strong_count(&file) > 1);
        registration.resources.context.control = Control::Real;
        drop(registration);
        unsafe {
            assert!(ffi::sqlite3_vfs_find(name.as_ptr()).is_null());
        }
        assert_eq!(Arc::strong_count(&file), 1);
    }
}
