//! Test-only terminal owner. Its SQLite connection is in-memory, never a writer.
use super::*;
use crate::database_state::maintenance_retention::assert_exclusive;
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    path::Path,
    time::{Duration, Instant},
};

enum State {
    Armed,
    Closed,
}
struct TerminalEpisode<'m, 's> {
    connection: Option<Connection>,
    prepared: Option<PreparedWriteEpisode<'m, 's>>,
    events: PathBuf,
    state: State,
}
fn event(path: &Path, text: &str) {
    // Test observation only. Failure prevents parent sequence acceptance.
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{text}");
    }
}
impl Drop for TerminalEpisode<'_, '_> {
    fn drop(&mut self) {
        if matches!(self.state, State::Armed) {
            event(&self.events, "guard-abort");
            std::process::abort(); // Before ANY resource field destruction.
        }
    }
}
impl<'m, 's> TerminalEpisode<'m, 's> {
    fn new(
        prepared: PreparedWriteEpisode<'m, 's>,
        connection: Connection,
        events: PathBuf,
    ) -> Self {
        Self {
            connection: Some(connection),
            prepared: Some(prepared),
            events,
            state: State::Armed,
        }
    }
    fn close(
        mut self,
        panic_after_ok: bool,
    ) -> std::result::Result<Option<PreparedWriteEpisode<'m, 's>>, (Self, rusqlite::Error)> {
        event(&self.events, "close-enter");
        // Guard already owns all caller resources before consuming Connection.
        let connection = self.connection.take().unwrap();
        match connection.close() {
            Err((connection, error)) => {
                self.connection = Some(connection); // Restore before observations.
                event(&self.events, "close-error-restored");
                Err((self, error))
            }
            Ok(()) => {
                event(&self.events, "close-ok");
                if panic_after_ok {
                    panic!("after actual close before disarm");
                }
                // Only this actual OK arm can change State. No public reset.
                self.state = State::Closed;
                let prepared = self.prepared.take();
                event(&self.events, "disarmed");
                Ok(prepared)
            }
        }
    }
}
#[test]
fn terminal_child() {
    let Some(root) = std::env::var_os("MNO_TERMINAL_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_TERMINAL_MODE").unwrap();
    let f = Fixture::new();
    assert!(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let outer = super::borrow_integration::outer(&f);
    let prepared = super::borrow_integration::prepared(&f, outer.maintenance());
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    fs::write(
        root.join("expected-p"),
        fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
    )
    .unwrap();
    let events = root.join("events");
    event(&events, "real-p-and-maintenance");
    let connection = Connection::open_in_memory().unwrap();
    if mode == "internal" {
        super::internal_close_panic::install_internal_panic(&connection, root.join("hook-drop"));
    }
    if mode == "busy" {
        // Exact existing 3p technique: retain one real unfinished Statement.
        // Child abort bounds both statement and connection retention.
        let statement = connection.prepare("SELECT 42").unwrap();
        let _ = Box::leak(Box::new(statement));
    }
    let episode = TerminalEpisode::new(prepared, connection, events.clone());
    assert_exclusive(&f.state, &f.path);
    event(&events, "operation-blocked");
    match mode.as_str() {
        "drop" => {
            event(&events, "drop-request");
            drop(episode);
        }
        "busy" => {
            let (failure, error) = match episode.close(false) {
                Err(f) => f,
                Ok(_) => panic!("expected BUSY"),
            };
            assert!(
                matches!(error, rusqlite::Error::SqliteFailure(ref e, _) if e.code == rusqlite::ErrorCode::DatabaseBusy)
            );
            assert_eq!(
                failure
                    .connection
                    .as_ref()
                    .unwrap()
                    .query_row("SELECT 42", [], |r| r.get::<_, i32>(0))
                    .unwrap(),
                42
            );
            assert!(matches!(failure.state, State::Armed));
            assert_exclusive(&f.state, &f.path);
            assert_eq!(
                outer.maintenance().recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
            event(&events, "busy-live-connection-and-maintenance");
            drop(failure);
        }
        "ok" => {
            let prepared = match episode.close(false) {
                Ok(p) => p.unwrap(),
                Err(_) => panic!("close success required"),
            };
            assert_exclusive(&f.state, &f.path);
            drop(prepared);
            let _closed = match outer.close() {
                Ok(c) => c,
                Err(_) => panic!("outer close"),
            };
            let next = f.state.maintenance().unwrap();
            blocked(&next, &f);
            event(&events, "safe-return");
            std::process::exit(0);
        }
        "internal" | "after-ok" => {
            event(&events, "panic-path-enter");
            let _ = episode.close(mode == "after-ok");
        }
        _ => panic!("unknown scenario"),
    }
    event(&events, "forbidden-continuation");
    std::process::exit(75);
}
fn run(mode: &str, expected: &str) {
    let directory = tempfile::TempDir::new().unwrap();
    let root = directory.path();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["terminal_guard::terminal_child", "--nocapture"])
        .env("MNO_TERMINAL_ROOT", root)
        .env("MNO_TERMINAL_MODE", mode)
        .env("TMPDIR", root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("terminal child timeout: {mode}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if mode == "ok" {
        assert_eq!(status.code(), Some(0));
    } else {
        assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
    }
    assert_eq!(
        fs::read_to_string(root.join("events")).unwrap(),
        expected,
        "{mode}"
    );
    assert_eq!(root.join("hook-drop").is_file(), mode == "internal");
    let data = PathBuf::from(String::from_utf8(fs::read(root.join("data-root")).unwrap()).unwrap());
    assert!(
        data.canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert_eq!(
        fs::read(data.join(DIRECTORY).join(NAME)).unwrap(),
        fs::read(root.join("expected-p")).unwrap()
    );
    match crate::startup::begin(&data).unwrap() {
        crate::startup::StartupOutcome::Recovery(g) => {
            assert_eq!(g.reason(), "database_recovery_artifact_present")
        }
        crate::startup::StartupOutcome::Ready(_) => panic!("P must block Ready"),
    }
}
#[test]
fn terminal_guard_real_close_and_busy() {
    run(
        "ok",
        "real-p-and-maintenance\noperation-blocked\nclose-enter\nclose-ok\ndisarmed\nsafe-return\n",
    );
    run(
        "busy",
        "real-p-and-maintenance\noperation-blocked\nclose-enter\nclose-error-restored\nbusy-live-connection-and-maintenance\nguard-abort\n",
    );
}
#[test]
fn terminal_guard_internal_panic_and_unfinished_drop() {
    run(
        "internal",
        "real-p-and-maintenance\noperation-blocked\npanic-path-enter\nclose-enter\nguard-abort\n",
    );
    run(
        "drop",
        "real-p-and-maintenance\noperation-blocked\ndrop-request\nguard-abort\n",
    );
    run(
        "after-ok",
        "real-p-and-maintenance\noperation-blocked\npanic-path-enter\nclose-enter\nclose-ok\nguard-abort\n",
    );
}
