//! Test-only actual Maintenance ownership. No writer or recovery integration.
use super::*;
use rusqlite::{Connection, OpenFlags};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

// Connection precedes the actual guard; incomplete owners NEVER field-drop this
// payload. Directory stays with them. No fd/connection extraction API exists.
struct Parts<'s> {
    connection: Option<Connection>,
    maintenance: Maintenance<'s>,
    directory: tempfile::TempDir,
}
pub(crate) struct Owner<'s>(Option<Box<Parts<'s>>>);
pub(crate) struct Failure<'s> {
    owner: Owner<'s>,
    error: rusqlite::Error,
}
pub(crate) struct Closed {
    directory: tempfile::TempDir,
}
impl<'s> Owner<'s> {
    pub(crate) fn new(
        maintenance: Maintenance<'s>,
        connection: Connection,
        directory: tempfile::TempDir,
    ) -> Self {
        Self(Some(Box::new(Parts {
            connection: Some(connection),
            maintenance,
            directory,
        })))
    }
    // Test-only short borrow for the production-chain integration prototype.
    // Never moves the guard or exposes Connection/File resources.
    pub(crate) fn maintenance(&self) -> &Maintenance<'s> {
        &self.0.as_ref().unwrap().maintenance
    }
    pub(crate) fn close(mut self) -> Result<Closed, Failure<'s>> {
        let parts = self.0.as_mut().unwrap();
        let connection = parts.connection.take().unwrap();
        if let Err((connection, error)) = connection.close() {
            parts.connection = Some(connection);
            return Err(Failure { owner: self, error });
        }
        // Confirmed SQLite close only. No hooks or fallible work inside transfer.
        let Parts {
            connection,
            maintenance,
            directory,
        } = *self.0.take().unwrap();
        assert!(connection.is_none());
        drop(maintenance);
        Ok(Closed { directory })
    }
}
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        if let Some(parts) = self.0.take() {
            // Deliberate, subprocess-bounded quarantine: actual Connection,
            // actual RwLockWriteGuard/Maintenance and TempDir together. No retry,
            // no destructor on live SQLite or M, no claim beyond process exit.
            let _ = Box::leak(parts);
        }
    }
}
fn fixture(parent: Option<&Path>) -> (tempfile::TempDir, DatabaseState, PathBuf) {
    let directory = match parent {
        Some(parent) => tempfile::TempDir::new_in(parent).unwrap(),
        None => tempfile::TempDir::new().unwrap(),
    };
    let state = DatabaseState::default();
    let database = state
        .initialize_startup(&directory.path().join("data"))
        .unwrap();
    (directory, state, database)
}
pub(crate) fn assert_exclusive(state: &DatabaseState, path: &Path) {
    assert!(state.maintenance.load(Ordering::Acquire));
    assert!(matches!(
        state.generation.try_read(),
        Err(std::sync::TryLockError::WouldBlock)
    ));
    // Hook runs ONLY after the real generation.try_read has failed. It aborts
    // this test request before blocking read(), so no permanently parked worker.
    let (tx, rx) = mpsc::channel();
    state.on_blocked(move || {
        tx.send(()).unwrap();
        panic!("real operation blocked");
    });
    std::thread::scope(|scope| {
        let attempt = scope.spawn(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _operation = state.operation(None).unwrap();
                // A real normal open would happen only after successful O.
                let _connection =
                    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
                panic!("unexpected normal database open");
            }))
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("real blocked checkpoint");
        let payload = attempt
            .join()
            .unwrap()
            .expect_err("request must hit blocked checkpoint");
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"real operation blocked")
        );
    });
}
#[test]
fn explicit_close_releases_real_maintenance_only_after_sqlite_close() {
    let (directory, state, path) = fixture(None);
    let maintenance = state.maintenance().unwrap();
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let owner = Owner::new(maintenance, connection, directory);
    assert_eq!(owner.0.as_ref().unwrap().maintenance.generation(), 1);
    assert_exclusive(&state, &path);
    let closed = match owner.close() {
        Ok(closed) => closed,
        Err(_) => panic!("explicit close must succeed"),
    };
    assert!(!state.maintenance.load(Ordering::Acquire));
    let operation = state.operation(Some(1)).unwrap();
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection.close().unwrap();
    drop(operation);
    assert!(closed.directory.path().exists());
}
#[test]
fn retained_child() {
    let Ok(mode) = std::env::var("MNO_5D3P_MODE") else {
        return;
    };
    let parent = PathBuf::from(std::env::var_os("MNO_5D3P_ROOT").unwrap());
    let (directory, state, path) = fixture(Some(&parent));
    let retained_root = directory.path().to_owned();
    let generation = state.operation(None).unwrap().snapshot(()).generation;
    let maintenance = state.maintenance().unwrap();
    let operation_id = maintenance.operation_id();
    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let owner = Owner::new(maintenance, connection, directory);
    assert_exclusive(&state, &path);
    match mode.as_str() {
        "drop" => drop(owner),
        "panic-before" => {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _owner = owner;
                panic!("before explicit close");
            }));
            let payload = caught.expect_err("intended panic");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"before explicit close")
            );
        }
        "busy" | "panic-after" => {
            let parts = owner.0.as_ref().unwrap();
            // Exactly one real unfinalized Statement is deliberately retained in
            // this child. It and the connection remain alive until process exit.
            let statement = parts
                .connection
                .as_ref()
                .unwrap()
                .prepare("SELECT * FROM journal_entries")
                .unwrap();
            let _ = Box::leak(Box::new(statement));
            let failure = match owner.close() {
                Err(failure) => failure,
                Ok(_) => panic!("real statement must cause BUSY"),
            };
            assert_eq!(
                failure.error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy)
            );
            let parts = failure.owner.0.as_ref().unwrap();
            assert_eq!(parts.maintenance.operation_id(), operation_id);
            assert_eq!(parts.maintenance.generation(), generation);
            assert!(!parts.connection.as_ref().unwrap().is_busy());
            assert_eq!(
                parts
                    .connection
                    .as_ref()
                    .unwrap()
                    .query_row("SELECT 42", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                42
            );
            assert_exclusive(&state, &path);
            if mode == "panic-after" {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _failure = failure;
                    panic!("after actual close BUSY");
                }));
                let payload = caught.expect_err("intended post-BUSY panic");
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"after actual close BUSY")
                );
            } else {
                drop(failure);
            }
        }
        _ => panic!("unknown child mode"),
    }
    // The OUTER owner/error owner has now really dropped or unwound. Its actual
    // Maintenance must still own the write guard, not merely a borrowed record.
    assert_exclusive(&state, &path);
    assert!(retained_root.is_dir() && path.is_file());
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Ready as u8);
    assert!(!state.recovery_required.load(Ordering::Acquire));
    println!("MAINTENANCE_RETAINED:{mode}");
    // Keep the real state backing the deliberately leaked guard alive until OS
    // exit; do not return and destruct state beneath the retained guard.
    std::process::exit(0);
}
fn child_case(mode: &str) {
    let directory = tempfile::TempDir::new().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "database_state::maintenance_retention::retained_child",
            "--nocapture",
        ])
        .env("MNO_5D3P_MODE", mode)
        .env("MNO_5D3P_ROOT", directory.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("child retention watchdog: {mode}");
        }
        // Watchdog only, not an ordering/correctness condition.
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!("MAINTENANCE_RETAINED:{mode}"))
    );
    // Parent owns only the enclosing fixture directory. Cleanup occurs AFTER
    // child exit, when OS locks/fds no longer exist. Never inspect main via a new
    // File while the child's SQLite handles are alive.
    let roots = std::fs::read_dir(directory.path())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(roots.len(), 1);
    let database = roots[0].path().join("data/mno.sqlite");
    let connection =
        Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection.close().unwrap();
}
#[test]
fn failed_close_drop_retains_real_guard() {
    child_case("busy");
}
#[test]
fn normal_drop_retains_real_guard() {
    child_case("drop");
}
#[test]
fn panic_before_close_retains_real_guard() {
    child_case("panic-before");
}
#[test]
fn panic_after_close_error_retains_real_guard() {
    child_case("panic-after");
}
