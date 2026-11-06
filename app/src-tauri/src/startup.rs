//! Fail-closed bootstrap. No restore parser or restore writes.
use crate::database;
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
const LOCK: &str = ".mno-data.lock";
fn failure(e: io::Error) -> String {
    format!("database_startup_io: {e}")
}
fn plain(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !metadata.file_type().is_symlink()
}
fn directory(path: &Path) -> io::Result<(PathBuf, bool)> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && plain(&m) => Ok((path.canonicalize()?, false)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe data directory",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let (parent, _) = directory(parent)?;
            let leaf = path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "missing directory name")
            })?;
            let path = parent.join(leaf);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            let created = match builder.create(&path) {
                Ok(()) => true,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => false,
                Err(e) => return Err(e),
            };
            let (canonical, _) = directory(&path)?;
            Ok((canonical, created))
        }
        Err(e) => Err(e),
    }
}
#[cfg(unix)]
fn identity(file: &File) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    Ok((m.dev(), m.ino()))
}
#[cfg(windows)]
fn identity(file: &File) -> io::Result<(u64, u64)> {
    use std::{mem::zeroed, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwVolumeSerialNumber as u64,
        ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
    ))
}
#[cfg(not(any(unix, windows)))]
fn identity(_: &File) -> io::Result<(u64, u64)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "data-directory locking unsupported",
    ))
}
fn open_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x02000000 | 0x00200000).share_mode(3);
    }
    options.open(path)
}
pub(crate) struct DirectoryLease {
    file: File,
    directory: File,
    root: PathBuf,
    new_lock: bool,
    new_directory: bool,
    #[cfg(windows)]
    boundary: established::windows::Boundary,
}
impl DirectoryLease {
    fn acquire(path: &Path) -> Result<Self, String> {
        Self::acquire_with(path, true)
    }
    // Classifier/diagnostics must never create a missing root or lock file.
    pub(crate) fn open_existing(path: &Path) -> Result<Self, String> {
        Self::acquire_with(path, false)
    }
    fn acquire_with(path: &Path, create: bool) -> Result<Self, String> {
        #[cfg(windows)]
        let mut boundary = established::windows::Boundary::capture(path).map_err(failure)?;
        let (root, new_directory) = if create {
            directory(path).map_err(failure)?
        } else {
            let metadata = fs::symlink_metadata(path).map_err(failure)?;
            if !metadata.is_dir() || !plain(&metadata) {
                return Err("database_directory_changed".into());
            }
            (path.canonicalize().map_err(failure)?, false)
        };
        #[cfg(windows)]
        boundary.attach(&root).map_err(failure)?;
        let directory = open_directory(&root).map_err(failure)?;
        let mut options = OpenOptions::new();
        options.read(true).write(create).create_new(create);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000).share_mode(3);
        }
        let (file, new_lock) = match options.open(root.join(LOCK)) {
            Ok(file) => (file, create),
            Err(e) if create && e.kind() == io::ErrorKind::AlreadyExists => {
                options.create_new(false);
                (options.open(root.join(LOCK)).map_err(failure)?, false)
            }
            Err(e) => return Err(failure(e)),
        };
        let m = file.metadata().map_err(failure)?;
        if !m.is_file() || !plain(&m) {
            return Err("database_startup_unsafe_lock".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if m.nlink() != 1 || m.uid() != unsafe { libc::geteuid() } {
                return Err("database_startup_unsafe_lock".into());
            }
        }
        fs2::FileExt::try_lock_exclusive(&file).map_err(|e| {
            if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
                "database_directory_in_use".into()
            } else {
                failure(e)
            }
        })?;
        let guard = Self {
            file,
            directory,
            root,
            new_lock,
            new_directory,
            #[cfg(windows)]
            boundary,
        };
        guard.ensure_current()?;
        Ok(guard)
    }
    pub(crate) fn ensure_current(&self) -> Result<(), String> {
        let root = fs::symlink_metadata(&self.root).map_err(failure)?;
        let lock = fs::symlink_metadata(self.root.join(LOCK)).map_err(failure)?;
        if !root.is_dir() || !plain(&root) || !lock.is_file() || !plain(&lock) {
            return Err("database_directory_changed".into());
        }
        let current_dir = open_directory(&self.root).map_err(failure)?;
        // On Unix, a replaced regular-file name may alias a locked SQLite
        // main inode. Never open it even after a metadata precheck: rejection
        // would close that fd and release this process's POSIX record locks.
        #[cfg(unix)]
        let current_metadata = self.file.metadata().map_err(failure)?;
        #[cfg(not(unix))]
        let current_lock = {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(0x00200000).share_mode(3);
            }
            options.open(self.root.join(LOCK)).map_err(failure)?
        };
        #[cfg(not(unix))]
        let current_metadata = current_lock.metadata().map_err(failure)?;
        if !current_metadata.is_file() || !plain(&current_metadata) {
            return Err("database_directory_changed".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // Preserve the classifier's single-link check, including changes
            // AFTER acquisition, and the existing lock-owner requirement.
            if current_metadata.nlink() != 1 || current_metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err("database_directory_changed".into());
            }
        }
        if identity(&current_dir).map_err(failure)? != identity(&self.directory).map_err(failure)? {
            return Err("database_directory_changed".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if lock.nlink() != 1
                || (lock.dev(), lock.ino()) != (current_metadata.dev(), current_metadata.ino())
            {
                return Err("database_directory_changed".into());
            }
            // Point-in-time name identity only; retained handles remain the
            // authority. No regular-file open/close occurs if names race.
        }
        #[cfg(not(unix))]
        if identity(&current_lock).map_err(failure)? != identity(&self.file).map_err(failure)? {
            return Err("database_directory_changed".into());
        }
        Ok(())
    }
}
impl DirectoryLease {
    pub(crate) fn directory_identity(&self) -> Result<(u64, u64), String> {
        self.ensure_current()?;
        identity(&self.directory).map_err(failure)
    }
    pub(crate) fn database_path(&self) -> PathBuf {
        database::expected_database_path(&self.root)
    }
}
impl Drop for DirectoryLease {
    fn drop(&mut self) {
        // Explicit release also avoids a transient inherited-fd lock during a
        // concurrent Unix spawn before its close-on-exec descriptors are closed.
        let _ = fs2::FileExt::unlock(&self.file);
    }
}
// The lockfile is never unlinked: file existence is not the lock. Closing the
// OS handle (also at process death) releases flock/LockFileEx automatically.
pub(crate) struct StartupReady {
    lock: Option<DirectoryLease>,
}
impl StartupReady {
    pub(crate) fn lease(&self) -> Result<&DirectoryLease, String> {
        let lease = self.lock.as_ref().ok_or("database_startup_blocked")?;
        lease.ensure_current()?;
        Ok(lease)
    }
    pub(crate) fn database_path(&self) -> PathBuf {
        database::expected_database_path(
            &self
                .lock
                .as_ref()
                .expect("production startup owns lock")
                .root,
        )
    }
    pub(crate) fn ensure_current(&self) -> Result<(), String> {
        match &self.lock {
            Some(lock) => lock.ensure_current(),
            #[cfg(test)]
            None => Ok(()),
            #[cfg(not(test))]
            None => Err("database_startup_blocked".into()),
        }
    }
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self { lock: None }
    }
}
// No constructor outside startup. Stored by DatabaseState even when startup is
// blocked, so dropping a borrowed recovery session cannot release the OS lock.
pub(crate) struct RecoveryGate {
    lease: DirectoryLease,
    reason: String,
}
impl RecoveryGate {
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
    pub(crate) fn lease(&self) -> Result<&DirectoryLease, String> {
        self.lease.ensure_current()?;
        Ok(&self.lease)
    }
}
pub(crate) enum StartupOutcome {
    Ready(StartupReady),
    Recovery(RecoveryGate),
}
fn checkpoint(
    root: &Path,
    mut metadata: impl FnMut(&Path) -> io::Result<fs::Metadata>,
) -> Result<bool, String> {
    let mut present = false;
    // Detect artifacts by existence/type only. Do not parse or classify here.
    // I/O failures are never absence, including when the other artifact exists.
    for name in [artifacts::RECOVERY, artifacts::PENDING, artifacts::RECEIPT] {
        match metadata(&root.join(name)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(failure(e)),
            Ok(_) => present = true,
        }
    }
    Ok(artifacts::unexpected_root(root, None).map_err(failure)? || present)
}
pub(crate) fn begin(root: &Path) -> Result<StartupOutcome, String> {
    begin_with(
        root,
        |path| fs::symlink_metadata(path),
        |path, new| {
            #[cfg(test)]
            if let Some(result) = migration_probe::initialize(path, new) {
                return result;
            }
            if new {
                database::create_current_database(path)
            } else {
                database::initialize_database(path)
            }
        },
    )
}
// Compatibility wrapper for existing isolated startup tests, not an app entry.
#[cfg(test)]
fn initialize(root: &Path) -> Result<StartupReady, String> {
    initialize_with(
        root,
        |path| fs::symlink_metadata(path),
        |path, new| {
            if new {
                database::create_current_database(path)
            } else {
                database::initialize_database(path)
            }
        },
    )
}
#[cfg(test)]
fn initialize_with(
    root: &Path,
    metadata: impl FnMut(&Path) -> io::Result<fs::Metadata>,
    initialize: impl FnOnce(&Path, bool) -> Result<(), String>,
) -> Result<StartupReady, String> {
    match begin_with(root, metadata, initialize)? {
        StartupOutcome::Ready(ready) => Ok(ready),
        StartupOutcome::Recovery(gate) => Err(gate.reason().to_owned()),
    }
}
fn begin_with(
    root: &Path,
    mut metadata: impl FnMut(&Path) -> io::Result<fs::Metadata>,
    initialize: impl FnOnce(&Path, bool) -> Result<(), String>,
) -> Result<StartupOutcome, String> {
    let lock = DirectoryLease::acquire(root)?;
    let recovery = match checkpoint(&lock.root, &mut metadata) {
        Ok(present) => present,
        Err(reason) => {
            return Ok(StartupOutcome::Recovery(RecoveryGate {
                lease: lock,
                reason,
            }));
        }
    };
    lock.ensure_current()?;
    if recovery {
        return Ok(StartupOutcome::Recovery(RecoveryGate {
            lease: lock,
            reason: "database_recovery_artifact_present".into(),
        }));
    }
    let path = database::expected_database_path(&lock.root);
    // No SQLite has been opened, nor establishment metadata published, yet.
    if let Err(reason) = sidecars::check(&path, &mut metadata) {
        return Ok(StartupOutcome::Recovery(RecoveryGate {
            lease: lock,
            reason,
        }));
    }
    let initialized = established::read(&lock.root)?;
    let exists = match fs::symlink_metadata(&path) {
        Ok(m) if !m.is_file() || !plain(&m) => {
            return Err("database_startup_unsafe_database".into());
        }
        Ok(_) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(failure(e)),
    };
    if !exists {
        if initialized || !lock.new_lock || !lock.new_directory {
            return Err("database_missing_established".into());
        }
        // Unknown legacy contents, backups or abandoned marker temporaries mean
        // absence is not a proven first install. The lock is the only allowed entry.
        for entry in fs::read_dir(&lock.root).map_err(failure)? {
            if entry.map_err(failure)?.file_name() != LOCK {
                return Err("database_missing_established".into());
            }
        }
    }
    if !initialized {
        #[cfg(not(windows))]
        established::publish(&lock.root)?;
        #[cfg(windows)]
        established::windows::publish(&lock.root, &lock.boundary)?;
    } else {
        // A previous publication may have failed AFTER the final name appeared.
        // Reconfirm mandatory barriers before trusting it on a later attempt.
        #[cfg(not(windows))]
        established::confirm(&lock.root)?;
        #[cfg(windows)]
        established::windows::confirm(&lock.root, &lock.boundary)?;
    }
    // Repeat immediately before BOTH startup open paths, after filesystem work.
    lock.ensure_current()?;
    let recovery = match checkpoint(&lock.root, &mut metadata) {
        Ok(present) => present,
        Err(reason) => {
            return Ok(StartupOutcome::Recovery(RecoveryGate {
                lease: lock,
                reason,
            }));
        }
    };
    if recovery {
        lock.ensure_current()?;
        return Ok(StartupOutcome::Recovery(RecoveryGate {
            lease: lock,
            reason: "database_recovery_artifact_present".into(),
        }));
    }
    if let Err(reason) = sidecars::check(&path, &mut metadata) {
        return Ok(StartupOutcome::Recovery(RecoveryGate {
            lease: lock,
            reason,
        }));
    }
    lock.ensure_current()?;
    initialize(&path, !exists)?;
    if !exists {
        #[cfg(not(windows))]
        established::sync_created_database(&path, &lock.root)?;
        #[cfg(windows)]
        {
            established::windows::sync_created_database(&path, &lock.boundary)?;
            database::initialize_database(&path)?;
        }
    }
    lock.ensure_current()?;
    Ok(StartupOutcome::Ready(StartupReady { lock: Some(lock) }))
}
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;

pub(crate) mod established;

#[cfg(all(test, not(any(target_os = "macos", target_os = "linux", windows))))]
mod unsupported_initialization_tests {
    use super::*;
    #[test]
    fn unsupported_durability_does_not_create_database() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("data");
        let state = crate::database_state::DatabaseState::default();
        assert!(
            matches!(state.initialize_startup(&root),Err(ref e) if e=="database_initialization_durability_unsupported")
        );
        assert!(!database::expected_database_path(&root).exists());
        assert!(state.operation(None).is_err());
    }
    #[test]
    fn sqlite_sidecars_are_not_ignored_on_unsupported_platform() {
        for suffix in ["-journal", "-wal", "-shm"] {
            let dir = tempfile::TempDir::new().unwrap();
            fs::write(dir.path().join(format!("mno.sqlite{suffix}")), b"").unwrap();
            let state = crate::database_state::DatabaseState::default();
            assert_eq!(
                state.initialize_startup(dir.path()).unwrap_err(),
                "database_sqlite_sidecar_present"
            );
            assert!(!database::expected_database_path(dir.path()).exists());
            assert!(state.operation(None).is_err());
            assert!(DirectoryLease::acquire(dir.path()).is_err());
        }
    }
    #[test]
    fn recovery_is_not_ignored_on_unsupported_platform() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir(dir.path().join("recovery")).unwrap();
        let state = crate::database_state::DatabaseState::default();
        assert!(
            matches!(state.initialize_startup(dir.path()),Err(ref e) if e=="database_recovery_artifact_present")
        );
        assert!(!database::expected_database_path(dir.path()).exists());
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod recovery_tests;

mod sidecars;

pub(crate) mod artifacts;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "startup/artifacts_tests.rs"]
mod artifacts_tests;

// Scoped initializer substitution at the real startup boundary, for isolated
// migration crash probes only. No alternate ordering, admission or production API.
#[cfg(test)]
pub(crate) mod migration_probe {
    use std::{cell::Cell, path::Path};
    type Initializer = fn(&Path, bool) -> Result<(), String>;
    thread_local! {
        static INITIALIZER: Cell<Option<Initializer>> = const { Cell::new(None) };
    }
    pub(crate) fn has_initializer() -> bool {
        INITIALIZER.with(|slot| slot.get().is_some())
    }
    pub(super) fn initialize(path: &Path, new: bool) -> Option<Result<(), String>> {
        INITIALIZER.with(|slot| slot.get().map(|init| init(path, new)))
    }
    pub(crate) fn with_initializer<T>(initializer: Initializer, action: impl FnOnce() -> T) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                INITIALIZER.with(|slot| slot.set(None));
            }
        }
        INITIALIZER.with(|slot| {
            assert!(slot.get().is_none(), "no nested startup initializer");
            slot.set(Some(initializer));
        });
        let _reset = Reset;
        action()
    }
}

#[cfg(all(test, windows))]
mod windows_tests;

#[cfg(test)]
mod generation_selection;
