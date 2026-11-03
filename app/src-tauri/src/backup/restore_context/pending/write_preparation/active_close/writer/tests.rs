use super::*;
use crate::backup::restore_context::RestoreMaintenanceScope;
use crate::backup::restore_context::pending::tests::Fixture;
use crate::backup::restore_context::pending::write_preparation::tests::context;
use std::{
    io::Write,
    os::unix::{io::AsRawFd, process::ExitStatusExt},
    time::{Duration, Instant},
};

pub(super) fn event(root: &Path, name: &str) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{name}"
    )
    .unwrap();
}
struct CleanupWitness(PathBuf);
impl Drop for CleanupWitness {
    fn drop(&mut self) {
        fs::write(&self.0, b"unsafe unwind").unwrap();
    }
}
struct InternalPanic(PathBuf);
impl Drop for InternalPanic {
    fn drop(&mut self) {
        event(&self.0, "internal-close-panic");
        let _witness = CleanupWitness(self.0.join("inner-cleanup"));
        panic!("writer internal close panic");
    }
}
#[test]
fn lock_child() {
    let Some(path) = std::env::var_os("MNO_WRITER_LOCK_PATH") else {
        return;
    };
    let db =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    match db.execute_batch("BEGIN EXCLUSIVE") {
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            std::process::exit(23)
        }
        Ok(()) => std::process::exit(24),
        _ => std::process::exit(25),
    }
}
fn lock_status(path: &Path) -> i32 {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["writer::tests::lock_child", "--nocapture"])
        .env("MNO_WRITER_LOCK_PATH", path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap()
}
#[test]
fn writer_child() {
    let Some(root) = std::env::var_os("MNO_WRITER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_WRITER_MODE").unwrap();
    let f = Fixture::new();
    assert!(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let original = fs::read(&f.path).unwrap();
    fs::write(root.join("original"), &original).unwrap();
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    let scope = RestoreMaintenanceScope {
        maintenance: f.state.maintenance().unwrap(),
        terminal_policy: TerminalPolicy::new(),
    };
    let m = scope.maintenance();
    let mut ctx = context(&f, m);
    ctx.terminal_policy = Some(&scope.terminal_policy);
    let inputs = ctx
        .publish_write_preparation()
        .unwrap()
        .prepare_write(m.recovery_session().unwrap())
        .unwrap()
        .close_active()
        .unwrap();
    let episode = inputs.into_epoch_inputs().prepare_episode().unwrap();
    let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
    let lease = f.state.test_startup_owner();
    let id = parts.session.as_ref().unwrap().session_id();
    let reservation = parts.session.as_ref().unwrap().test_reservation_slot();
    let b_id = parts
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .retained_test_identity()
        .unwrap();
    let c_id = parts
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .retained_test_identity()
        .unwrap();
    let p = parts.pending.as_ref().unwrap().bytes.clone();
    fs::write(root.join("expected-p"), &p).unwrap();
    crate::panic_policy::install();
    let mut episode = if mode == "close-unopened" {
        let failure = episode.close_writer().err().expect("must reject");
        assert!(matches!(failure.error(), ActiveCloseError::Binding));
        assert!(scope.terminal_policy.is_never_armed());
        event(&root, "unopened-rejected");
        drop(failure);
        std::process::exit(0);
    } else {
        episode.begin_writer().unwrap()
    };
    assert!(scope.terminal_policy.is_armed());
    assert!(matches!(
        episode.inputs.inputs.owner.after.as_ref().unwrap().writer,
        WriterState::Opening
    ));
    if mode == "panic-opening" {
        event(&root, "opening-panic");
        panic!("writer opening phase");
    }
    if mode == "opening-order" {
        let failure = episode.close_b().err().expect("must reject");
        assert!(matches!(failure.error(), ActiveCloseError::Binding));
        assert!(failure.owner.after.as_ref().unwrap().sources.live().is_ok());
        event(&root, "opening-rejected");
        drop(failure);
        std::process::exit(76);
    }
    // TEST ONLY factory: all descriptors acquired AFTER arming, before any lock.
    // Production has no function capable of constructing Owned or opening D.
    let anchor = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&f.path)
        .unwrap();
    let anchor_fd = anchor.as_raw_fd();
    let connection = Connection::open_with_flags(
        &f.path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_PRIVATE_CACHE,
    )
    .unwrap();
    connection
        .execute_batch("BEGIN; SELECT * FROM journal_entries")
        .unwrap();
    if mode == "internal-panic" {
        let capture = InternalPanic(root.clone());
        connection
            .rollback_hook(Some(move || {
                let _ = &capture;
            }))
            .unwrap();
    }
    let handle = unsafe { connection.handle() } as usize;
    if mode == "busy" || mode == "busy-order" || mode == "busy-retry" {
        std::mem::forget(connection.prepare("SELECT 1").unwrap());
    }
    episode.inputs.inputs.owner.after.as_mut().unwrap().writer = WriterState::Owned {
        connection: Some(connection),
        anchor,
        outcome: None,
    };
    assert_eq!(lock_status(&f.path), 23);
    event(&root, "owned");
    if mode == "panic-live" {
        let _witness = CleanupWitness(root.join("inner-cleanup"));
        panic!("writer live phase");
    }
    if mode == "negative-lock" {
        // Deliberately unsafe ONLY in this child: proves the probe is sensitive
        // to POSIX process-lock loss while the original SQLite transaction lives.
        drop(File::open(&f.path).unwrap());
        assert_eq!(lock_status(&f.path), 24);
        event(&root, "lock-loss-detected");
        std::process::exit(78); // No later guard can mask the negative control.
    }
    if mode == "live-order" {
        let failure = episode.close_b().err().expect("must reject");
        assert!(matches!(failure.error(), ActiveCloseError::Binding));
        assert!(matches!(
            failure.owner.after.as_ref().unwrap().writer,
            WriterState::Owned {
                connection: Some(_),
                outcome: None,
                ..
            }
        ));
        event(&root, "live-rejected");
        drop(failure);
        std::process::exit(76);
    }
    if mode == "error" {
        probe::ERROR.with(|v| v.set(true));
    }
    if mode == "panic-outcome" || mode == "negative-panic" {
        probe::PANIC.with(|v| v.set(true));
    }
    event(&root, "before-close");
    if mode == "negative-panic" {
        std::panic::set_hook(Box::new(|_| {}));
        // Owner stays outside catch; exit before any later armed Drop can mask it.
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = episode.close_writer_inner();
        }));
        assert!(caught.is_err());
        event(&root, "panic-returned");
        std::process::exit(77);
    }
    let result = episode.close_writer();
    let owner = match result {
        Ok(episode) => {
            assert!(!matches!(
                mode.as_str(),
                "busy" | "busy-order" | "busy-retry" | "error"
            ));
            let WriterState::Closed { _anchor } =
                &episode.inputs.inputs.owner.after.as_ref().unwrap().writer
            else {
                panic!("checked close required")
            };
            assert_eq!(_anchor.as_raw_fd(), anchor_fd);
            assert_eq!(lock_status(&f.path), 24);
            if mode == "handoff" {
                let episode = episode.close_b().unwrap().close_c().unwrap();
                check_abc_connections_closed(episode.inputs.inputs.owner.after.as_ref().unwrap())
                    .unwrap();
                event(&root, "ABC-closed");
                episode.inputs.inputs.owner
            } else {
                episode.inputs.inputs.owner
            }
        }
        Err(failure) => {
            assert!(matches!(
                mode.as_str(),
                "busy" | "busy-order" | "busy-retry" | "error"
            ));
            assert!(matches!(failure.error(), ActiveCloseError::WriterClose));
            let WriterState::Owned {
                connection: None,
                anchor,
                outcome: Some(Err((connection, error))),
            } = &failure.owner.after.as_ref().unwrap().writer
            else {
                panic!("original failure owner")
            };
            assert_eq!(anchor.as_raw_fd(), anchor_fd);
            assert_eq!(unsafe { connection.handle() } as usize, handle);
            assert!(
                matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.extended_code == if mode == "busy" || mode == "busy-order" || mode == "busy-retry" { rusqlite::ffi::SQLITE_BUSY } else { rusqlite::ffi::SQLITE_IOERR })
            );
            assert_eq!(lock_status(&f.path), 23);
            if mode == "busy-order" || mode == "busy-retry" {
                let episode = PreparedWriteEpisode {
                    inputs: WriteEpochInputs {
                        inputs: WriteInputs {
                            owner: failure.owner,
                        },
                    },
                };
                let failure = if mode == "busy-order" {
                    episode.close_b()
                } else {
                    episode.close_writer()
                }
                .err()
                .expect("must reject retained failed writer");
                assert!(matches!(failure.error(), ActiveCloseError::Binding));
                assert_eq!(lock_status(&f.path), 23);
                failure.owner
            } else {
                failure.owner
            }
        }
    };
    let parts = owner.after.as_ref().unwrap();
    if mode != "handoff" {
        let live = parts.sources.live().unwrap();
        assert_eq!(
            live.incoming
                .as_ref()
                .unwrap()
                .retained_test_identity()
                .unwrap(),
            b_id
        );
        assert_eq!(
            live.protection
                .as_ref()
                .unwrap()
                .candidate()
                .retained_test_identity()
                .unwrap(),
            c_id
        );
    }
    assert_eq!(f.state.test_startup_owner(), lease);
    assert!(std::ptr::eq(parts.maintenance, m));
    assert_eq!(parts.session.as_ref().unwrap().session_id(), id);
    assert_eq!(
        parts.session.as_ref().unwrap().test_reservation_slot(),
        reservation
    );
    assert_eq!(parts.pending.as_ref().unwrap().bytes, p);
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    m.assert_running_recovery_generation_for_test(1);
    assert!(scope.terminal_policy.is_armed());
    event(&root, "retained-before-drop");
    drop(owner); // Abort BEFORE writer/anchor/A/B/C/P/reservation destruction.
    std::process::exit(76);
}

#[test]
fn writer_ownership_subprocesses() {
    for mode in [
        "ok",
        "busy",
        "busy-order",
        "busy-retry",
        "negative-lock",
        "error",
        "opening-order",
        "live-order",
        "handoff",
        "panic-outcome",
        "panic-opening",
        "panic-live",
        "internal-panic",
        "negative-panic",
        "close-unopened",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["writer::tests::writer_child", "--nocapture"])
            .env("MNO_WRITER_ROOT", root.path())
            .env("MNO_WRITER_MODE", mode)
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
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("timeout {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let events = fs::read_to_string(root.path().join("events")).unwrap_or_default();
        let expected = match mode {
            "close-unopened" => "unopened-rejected\n",
            "negative-lock" => "owned\nlock-loss-detected\n",
            "opening-order" => "opening-rejected\n",
            "live-order" => "owned\nlive-rejected\n",
            "panic-outcome" => "owned\nbefore-close\n",
            "panic-opening" => "opening-panic\n",
            "panic-live" => "owned\n",
            "internal-panic" => "owned\nbefore-close\ninternal-close-panic\n",
            "negative-panic" => "owned\nbefore-close\npanic-returned\n",
            "handoff" => "owned\nbefore-close\nABC-closed\nretained-before-drop\n",
            _ => "owned\nbefore-close\nretained-before-drop\n",
        };
        assert_eq!(events, expected, "{mode}: {status}");
        if mode == "close-unopened" {
            assert!(status.success());
        } else if mode == "negative-lock" {
            assert_eq!(status.code(), Some(78));
        } else if mode == "negative-panic" {
            assert_eq!(status.code(), Some(77));
            assert_ne!(status.signal(), Some(libc::SIGABRT));
        } else {
            assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}");
        }
        assert!(
            !root.path().join("inner-cleanup").exists(),
            "{mode}: no unwind cleanup"
        );
        let data = PathBuf::from(fs::read_to_string(root.path().join("data-root")).unwrap());
        assert_eq!(
            fs::read(data.join("mno.sqlite")).unwrap(),
            fs::read(root.path().join("original")).unwrap(),
            "{mode}"
        );
        assert_eq!(
            fs::read(data.join(DIRECTORY).join(NAME)).unwrap(),
            fs::read(root.path().join("expected-p")).unwrap()
        );
        let state = crate::database_state::DatabaseState::default();
        assert_eq!(
            state.initialize_legacy_startup(&data).unwrap_err(),
            "database_recovery_artifact_present",
            "{mode}: P must block startup"
        );
    }
}
