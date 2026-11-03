//! Real B close, only in dedicated macOS test subprocesses.
use super::*;
use crate::backup::restore::close_results;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use crate::database_state::maintenance_retention::assert_exclusive;
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};

fn event(root: &Path, value: &str) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{value}"
    )
    .unwrap();
}
struct UnwindWitness(PathBuf);
impl Drop for UnwindWitness {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"forbidden unwind");
    }
}
struct PanicCapture(PathBuf);
impl Drop for PanicCapture {
    fn drop(&mut self) {
        event(&self.0, "internal-panic");
        let _witness = UnwindWitness(self.0.join("inner-unwind"));
        panic!("internal B rollback destructor");
    }
}
struct LaterCleanup(PathBuf);
impl Drop for LaterCleanup {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"later cleanup");
    }
}

// Separate process: its fd close cannot release the B owner's POSIX locks.
#[test]
fn lock_contender() {
    let Some(path) = std::env::var_os("MNO_B_LOCK_PATH") else {
        return;
    };
    let db = Connection::open_with_flags(PathBuf::from(path), OpenFlags::SQLITE_OPEN_READ_WRITE)
        .unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    let code = match db.execute_batch("BEGIN EXCLUSIVE") {
        Err(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            73
        }
        Ok(()) => {
            db.execute_batch("ROLLBACK").unwrap();
            74
        }
        other => panic!("unexpected contender result: {other:?}"),
    };
    db.close().unwrap();
    std::process::exit(code);
}
pub(super) fn check_b_lock(path: &Path, held: bool) {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["b_close_outcome::lock_contender", "--nocapture"])
        .env("MNO_B_LOCK_PATH", path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("lock contender timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        status.code(),
        Some(if held { 73 } else { 74 }),
        "B lock held={held}: {status}"
    );
}

#[test]
fn outcome_child() {
    let Some(root) = std::env::var_os("MNO_B_OUTCOME_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_B_OUTCOME_MODE").unwrap();
    let f = Fixture::new();
    assert!(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let scope = RestoreMaintenanceScope {
        maintenance: f.state.maintenance().unwrap(),
        terminal_policy: TerminalPolicy::new(),
    };
    let m = scope.maintenance();
    let mut ctx = context(&f, m);
    ctx.terminal_policy = Some(&scope.terminal_policy);
    let durable = ctx.publish_write_preparation().unwrap();
    let session = m.recovery_session().unwrap();
    let id = session.session_id();
    let mut inputs = durable
        .prepare_write(session)
        .unwrap()
        .close_active()
        .unwrap();
    let parts = inputs.owner.after.as_ref().unwrap();
    observe(parts, id, m, &f);
    let live = parts.sources.live().unwrap();
    let b = live.incoming.as_ref().unwrap();
    let b_identity = (b.retained_test_identity().unwrap(), b.sqlite_test_handle());
    let b_path = b.database_path();
    let c_identity = live
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .retained_test_identity()
        .unwrap();
    let marker = f.root.join(DIRECTORY).join(NAME);
    fs::write(root.join("expected-p"), fs::read(&marker).unwrap()).unwrap();
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    if mode.contains("busy") {
        // Safe existing resource, intentionally unfinished until process abort.
        // No raw statement lifetime crosses a Rust borrow; SQLite owns it after
        // forget. No finalize/retry is performed by the positive scenarios.
        std::mem::forget(b.source().unwrap().prepare("SELECT 42").unwrap());
    }
    if mode == "internal" {
        let capture = PanicCapture(root.clone());
        b.source()
            .unwrap()
            .rollback_hook(Some(move || {
                let _ = &capture;
            }))
            .unwrap();
        let later = LaterCleanup(root.join("later-cleanup"));
        b.source()
            .unwrap()
            .progress_handler(
                1,
                Some(move || {
                    let _ = &later;
                    false
                }),
            )
            .unwrap();
    }
    // Existing blocked-operation helper deliberately panics in a worker. Run
    // it BEFORE installing the all-panics-terminal policy, while B stays put.
    assert_exclusive(&f.state, &f.path);
    check_b_lock(&b_path, true);
    crate::panic_policy::install();
    let wrong_policy = TerminalPolicy::new();
    let mut guard = BTransferGuard {
        terminal_policy: if mode == "negative-busy-drop" {
            &wrong_policy
        } else {
            &scope.terminal_policy
        },
        incoming: None,
        outcome: None,
    };
    close_results::take();
    event(&root, "chain-verified");
    scope.terminal_policy.arm_for_test();
    let parts = inputs.owner.after.as_mut().unwrap();
    let SourcePhase::Live(live) = &mut parts.sources else {
        panic!("Live required")
    };
    std::mem::swap(&mut guard.incoming, &mut live.incoming);
    assert!(live.incoming.is_none());
    assert_eq!(
        (
            guard
                .incoming
                .as_ref()
                .unwrap()
                .retained_test_identity()
                .unwrap(),
            guard.incoming.as_ref().unwrap().sqlite_test_handle()
        ),
        b_identity
    );
    check_b_lock(&b_path, true);
    event(&root, "transfer-verified");
    event(&root, "before-close");
    // No callback, observation, match payload, allocation, or fallible work
    // between the consuming return and its place in the already-owned slot.
    // A Rust panic ANYWHERE in the consuming call aborts via the process hook,
    // before unwind can destroy the library's or caller's temporary resources.
    guard.outcome = Some(guard.incoming.take().unwrap().try_close());
    event(&root, "outcome-stored");
    assert!(guard.incoming.is_none());
    let attempts = close_results::take();
    match guard.outcome.as_ref().unwrap() {
        Ok(closed) => {
            assert!(!mode.contains("busy"));
            assert_eq!(attempts, vec![rusqlite::ffi::SQLITE_OK]);
            closed.check_retained_file().unwrap();
            check_b_lock(&b_path, false);
            event(&root, "ok-verified");
        }
        Err(failure) => {
            assert!(mode.contains("busy"));
            assert_eq!(attempts, vec![rusqlite::ffi::SQLITE_BUSY]);
            assert_eq!(failure.retained_test_identity().unwrap(), b_identity);
            assert!(
                matches!(&failure.error, rusqlite::Error::SqliteFailure(code, _)
                if code.extended_code == rusqlite::ffi::SQLITE_BUSY)
            );
            check_b_lock(&b_path, true);
            event(&root, "busy-verified");
        }
    }
    // Borrow only; no Live loan is valid while B is in the guard/result.
    let parts = inputs.owner.after.as_ref().unwrap();
    let SourcePhase::Live(live) = &parts.sources else {
        panic!("phase changed")
    };
    assert!(live.incoming.is_none());
    assert_eq!(
        live.protection
            .as_ref()
            .unwrap()
            .candidate()
            .retained_test_identity()
            .unwrap(),
        c_identity
    );
    assert!(std::ptr::eq(parts.maintenance, m));
    assert!(std::ptr::eq(
        inputs.owner.terminal_policy.unwrap(),
        &scope.terminal_policy
    ));
    assert_eq!(parts.session.as_ref().unwrap().session_id(), id);
    parts
        .binding
        .as_ref()
        .unwrap()
        .check_session(parts.session.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        parts.pending.as_ref().unwrap().bytes,
        fs::read(root.join("expected-p")).unwrap()
    );
    m.assert_running_recovery_generation_for_test(1);
    assert!(m.ensure_usable().is_err());
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    event(&root, "outer-retained");
    if mode == "negative-busy-drop" {
        // Deliberately wrong LOCAL policy, same armed outer chain. A dropped
        // BUSY payload retries via existing PreparedRestore::Drop; observe it
        // and exit BEFORE the outer guard could disguise the missing abort.
        drop(guard);
        assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_BUSY]);
        event(&root, "forbidden-result-drop-retry");
        std::process::exit(74);
    }
    event(&root, "terminal-boundary");
    if mode.ends_with("panic") {
        if mode.starts_with("negative-") {
            // Omit the intended panic ONLY in this subprocess control. The
            // armed guard stays alive; process exit must beat its later Drop.
            event(&root, "panic-suppressed");
        } else {
            event(&root, "intended-panic");
            panic!("after stored result before phase transition");
        }
        std::process::exit(77);
    } else {
        event(&root, "guard-drop");
        drop(guard); // Never a fallback for a missing intended panic.
        std::process::exit(76);
    }
}

#[test]
fn real_b_close_results_remain_under_terminal_protection() {
    for mode in [
        "ok-drop",
        "busy-drop",
        "ok-panic",
        "busy-panic",
        "internal",
        "negative-busy-drop",
        "negative-ok-panic",
        "negative-busy-panic",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["b_close_outcome::outcome_child", "--nocapture"])
            .env("MNO_B_OUTCOME_ROOT", root.path())
            .env("MNO_B_OUTCOME_MODE", mode)
            .env("TMPDIR", root.path())
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
                child.wait().unwrap();
                panic!("timeout {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut expected = "chain-verified\ntransfer-verified\nbefore-close\n".to_owned();
        if mode == "internal" {
            expected.push_str("internal-panic\n");
        } else {
            expected.push_str("outcome-stored\n");
            expected.push_str(if mode.contains("busy") {
                "busy-verified\n"
            } else {
                "ok-verified\n"
            });
            expected.push_str("outer-retained\n");
            if mode == "negative-busy-drop" {
                expected.push_str("forbidden-result-drop-retry\n");
            } else {
                expected.push_str("terminal-boundary\n");
                expected.push_str(if mode.starts_with("negative-") {
                    "panic-suppressed\n"
                } else if mode.ends_with("panic") {
                    "intended-panic\n"
                } else {
                    "guard-drop\n"
                });
            }
        }
        if mode.starts_with("negative") {
            assert_ne!(
                status.signal(),
                Some(libc::SIGABRT),
                "positive abort verdict must reject this control"
            );
            assert_eq!(
                status.code(),
                Some(if mode.ends_with("panic") { 77 } else { 74 }),
                "{mode}: {status}"
            );
        } else {
            assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
        }
        assert_eq!(
            fs::read_to_string(root.path().join("events")).unwrap(),
            expected,
            "{mode}"
        );
        assert!(!root.path().join("inner-unwind").exists());
        assert!(!root.path().join("later-cleanup").exists());
        let data = PathBuf::from(
            String::from_utf8(fs::read(root.path().join("data-root")).unwrap()).unwrap(),
        );
        assert!(
            data.canonicalize()
                .unwrap()
                .starts_with(root.path().canonicalize().unwrap())
        );
        assert_eq!(
            fs::read(data.join(DIRECTORY).join(NAME)).unwrap(),
            fs::read(root.path().join("expected-p")).unwrap()
        );
        match crate::startup::begin(&data).unwrap() {
            crate::startup::StartupOutcome::Recovery(gate) => {
                assert_eq!(gate.reason(), "database_recovery_artifact_present")
            }
            crate::startup::StartupOutcome::Ready(_) => panic!("P must block Ready"),
        }
    }
}
