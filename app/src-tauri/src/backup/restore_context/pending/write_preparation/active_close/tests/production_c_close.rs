//! Actual private C producer, real B/C and outer owners, isolated terminal paths.
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
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"forbidden");
    }
}
struct PanicCapture(PathBuf);
impl Drop for PanicCapture {
    fn drop(&mut self) {
        event(&self.0, "internal-panic");
        let _witness = Cleanup(self.0.join("inner-unwind"));
        panic!("real C internal close panic");
    }
}
#[test]
fn production_c_child() {
    let Some(root) = std::env::var_os("MNO_PRODUCTION_C_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_PRODUCTION_C_MODE").unwrap();
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
    let session_id = session.session_id();
    let reservation = session.test_reservation_slot();
    let lease = f.state.test_startup_owner();
    let episode = durable
        .prepare_write(session)
        .unwrap()
        .close_active()
        .unwrap()
        .into_epoch_inputs()
        .prepare_episode()
        .unwrap();
    let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
    assert!(check_abc_connections_closed(parts).is_err());
    let live = parts.sources.live().unwrap();
    let b = live.incoming.as_ref().unwrap();
    let c = live.protection.as_ref().unwrap();
    let b_path = b.database_path();
    let c_path = c.candidate().database_path();
    let c_id = (
        c.candidate().retained_test_identity().unwrap(),
        c.candidate().sqlite_test_handle(),
    );
    let record_id = c.test_record_identity();
    if mode == "b-busy" {
        std::mem::forget(b.source().unwrap().prepare("SELECT 42").unwrap());
    }
    if mode == "busy" || mode == "retry-failed" {
        std::mem::forget(
            c.candidate()
                .source()
                .unwrap()
                .prepare("SELECT 42")
                .unwrap(),
        );
    }
    if mode == "internal" {
        let capture = PanicCapture(root.clone());
        c.candidate()
            .source()
            .unwrap()
            .rollback_hook(Some(move || {
                let _ = &capture;
            }))
            .unwrap();
        let later = Cleanup(root.join("later-cleanup"));
        c.candidate()
            .source()
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
    assert_exclusive(&f.state, &f.path); // Helper deliberately panics before installation.
    super::b_close_outcome::check_b_lock(&c_path, true);
    crate::panic_policy::install();
    close_results::take();
    let episode = if mode == "live" {
        episode
    } else {
        match episode.close_b() {
            Ok(episode) => episode,
            Err(failure) => {
                assert_eq!(mode, "b-busy");
                assert!(matches!(failure.error, ActiveCloseError::BSourceClose));
                let ActiveCloseFailure { owner, error: _ } = *failure;
                PreparedWriteEpisode {
                    inputs: WriteEpochInputs {
                        inputs: WriteInputs { owner },
                    },
                }
            }
        }
    };
    if mode != "live" && mode != "b-busy" {
        assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_OK]);
        let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
        assert!(check_abc_connections_closed(parts).is_err());
        let SourcePhase::BClosed {
            incoming: Some(b),
            protection: Some(c),
            record: None,
        } = &parts.sources
        else {
            panic!("complete BClosed required")
        };
        assert_eq!(b.retained_staging_path().join("journal.sqlite"), b_path);
        assert_eq!(
            (
                c.candidate().retained_test_identity().unwrap(),
                c.candidate().sqlite_test_handle()
            ),
            c_id
        );
        assert_eq!(c.test_record_identity(), record_id);
        super::b_close_outcome::check_b_lock(&b_path, false);
        super::b_close_outcome::check_b_lock(&c_path, true);
    } else {
        let results = close_results::take();
        assert_eq!(
            results,
            if mode == "live" {
                vec![]
            } else {
                vec![rusqlite::ffi::SQLITE_BUSY]
            }
        );
    }
    if mode.starts_with("alias-") {
        let lock_name = mode.contains("lock");
        let target = if lock_name {
            f.root.join(".mno-data.lock")
        } else {
            f.root.join(DIRECTORY).join(NAME)
        };
        let saved = target.with_file_name("p2-original");
        if mode.ends_with("race") {
            use std::sync::{
                Arc, Barrier,
                atomic::{AtomicBool, Ordering},
            };
            let stop = Arc::new(AtomicBool::new(false));
            let ready = Arc::new(Barrier::new(2));
            // All mutation is pathname-only. No contender or helper fd is
            // opened in the SQLite owner process during replacement.
            std::thread::scope(|threads| {
                let (worker_stop, worker_ready, target, saved, worker_c_path) = (
                    stop.clone(),
                    ready.clone(),
                    target.clone(),
                    saved.clone(),
                    c_path.clone(),
                );
                let worker = threads.spawn(move || {
                    let mut rounds = 0;
                    loop {
                        fs::rename(&target, &saved).unwrap();
                        fs::hard_link(&worker_c_path, &target).unwrap();
                        if rounds == 0 {
                            worker_ready.wait();
                        }
                        std::thread::yield_now();
                        fs::remove_file(&target).unwrap();
                        fs::rename(&saved, &target).unwrap();
                        rounds += 1;
                        if worker_stop.load(Ordering::Acquire) {
                            break;
                        }
                    }
                    rounds
                });
                ready.wait();
                for _ in 0..32 {
                    // A transient valid name may pass. Every result must be
                    // lock-safe; the stable bad-name case below MUST reject.
                    let _ = check_common_after(episode.inputs.inputs.owner.after.as_ref().unwrap());
                    super::b_close_outcome::check_b_lock(&c_path, true);
                }
                stop.store(true, Ordering::Release);
                assert!(worker.join().unwrap() > 0);
            });
            event(&root, "replacement-race-lock-held");
        }
        fs::rename(&target, &saved).unwrap();
        fs::hard_link(&c_path, &target).unwrap();
        event(&root, "alias-installed");
        if mode.ends_with("negative") {
            // Test-local reproduction of the OLD bug. This is intentionally
            // terminal: no further SQLite operation or normal owner Drop.
            drop(fs::File::open(&target).unwrap());
            super::b_close_outcome::check_b_lock(&c_path, false);
            fs::remove_file(&target).unwrap();
            fs::rename(&saved, &target).unwrap();
            event(&root, "negative-lock-loss");
            std::process::exit(78);
        }
        let failure = episode
            .close_c()
            .err()
            .expect("alias must reject before C close");
        if lock_name {
            assert!(matches!(failure.error, ActiveCloseError::Session(_)));
        } else {
            assert!(matches!(failure.error, ActiveCloseError::Binding));
        }
        assert!(
            close_results::take().is_empty(),
            "no C-close, including implicit retry"
        );
        super::b_close_outcome::check_b_lock(&c_path, true);
        event(&root, "rejected-lock-held");
        let parts = failure.owner.after.as_ref().unwrap();
        let SourcePhase::BClosed {
            incoming: Some(b),
            protection: Some(c),
            record: None,
        } = &parts.sources
        else {
            panic!("original complete BClosed must survive rejection")
        };
        b.check_retained_file().unwrap();
        assert_eq!(
            (
                c.candidate().retained_test_identity().unwrap(),
                c.candidate().sqlite_test_handle()
            ),
            c_id
        );
        assert_eq!(c.test_record_identity(), record_id);
        assert!(std::ptr::eq(parts.maintenance, m));
        assert_eq!(parts.session.as_ref().unwrap().session_id(), session_id);
        assert_eq!(
            parts.session.as_ref().unwrap().test_reservation_slot(),
            reservation
        );
        assert!(scope.terminal_policy.is_armed());
        fs::remove_file(&target).unwrap();
        fs::rename(&saved, &target).unwrap();
        assert_eq!(f.state.test_startup_owner(), lease);
        check_common_after(parts).unwrap();
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        m.assert_running_recovery_generation_for_test(1);
        event(&root, "original-owners-retained");
        event(&root, "whole-owner-drop");
        drop(failure);
        std::process::exit(76);
    }
    c_close_probe::configure(
        root.clone(),
        match mode.as_str() {
            "panic-before" => Some(BClosePoint::BeforeConsume),
            "panic-outcome" => Some(BClosePoint::AfterOutcome),
            "panic-phase" => Some(BClosePoint::AfterPhase),
            _ => None,
        },
        (mode == "file-error").then(|| (c_path.clone(), c_path.with_file_name("moved.sqlite"))),
        mode == "negative-handoff",
    );
    event(&root, "prepared-C-chain");
    let result = episode.close_c();
    let result = if mode == "double-close" {
        result.unwrap().close_c()
    } else if mode == "retry-failed" {
        let failure = result.err().unwrap();
        assert!(matches!(failure.error, ActiveCloseError::CSourceClose));
        let ActiveCloseFailure { owner, error: _ } = *failure;
        PreparedWriteEpisode {
            inputs: WriteEpochInputs {
                inputs: WriteInputs { owner },
            },
        }
        .close_c()
    } else {
        result
    };
    if mode.starts_with("panic-") || mode == "negative-panic" {
        event(&root, "panic-returned");
        std::process::exit(77);
    }
    event(&root, "method-returned");
    let owner = match &result {
        Ok(e) => {
            assert_eq!(mode, "ok");
            &e.inputs.inputs.owner
        }
        Err(f) => {
            assert!(matches!(
                (mode.as_str(), f.error()),
                ("busy", ActiveCloseError::CSourceClose)
                    | ("file-error", ActiveCloseError::CFileCheck)
                    | (
                        "double-close" | "retry-failed" | "b-busy" | "live",
                        ActiveCloseError::Binding
                    )
            ));
            &f.owner
        }
    };
    let parts = owner.after.as_ref().unwrap();
    match &parts.sources {
        SourcePhase::Closed {
            incoming,
            protection,
            record,
        } => {
            assert!(mode == "ok" || mode == "file-error" || mode == "double-close");
            assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_OK]);
            check_abc_connections_closed(parts).unwrap();
            assert_eq!(
                incoming.retained_staging_path().join("journal.sqlite"),
                b_path
            );
            incoming.check_retained_file().unwrap();
            assert_eq!(
                protection.retained_staging_path().join("journal.sqlite"),
                c_path
            );
            assert_eq!(record.test_record_identity(), record_id);
            if mode == "file-error" {
                assert!(protection.check_retained_file().is_err());
            } else {
                protection.check_retained_file().unwrap();
                super::b_close_outcome::check_b_lock(&c_path, false);
            }
            event(&root, "Closed");
        }
        SourcePhase::CFailed {
            incoming,
            protection,
            record,
        } => {
            assert!(mode == "busy" || mode == "retry-failed");
            assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_BUSY]);
            assert!(check_abc_connections_closed(parts).is_err());
            incoming.check_retained_file().unwrap();
            assert_eq!(
                incoming.retained_staging_path().join("journal.sqlite"),
                b_path
            );
            assert_eq!(protection.retained_test_identity().unwrap(), c_id);
            assert!(
                matches!(&protection.error,rusqlite::Error::SqliteFailure(e,_) if e.extended_code==rusqlite::ffi::SQLITE_BUSY)
            );
            assert_eq!(record.test_record_identity(), record_id);
            super::b_close_outcome::check_b_lock(&c_path, true);
            event(&root, "CFailed");
        }
        SourcePhase::BFailed { protection, .. } => {
            assert_eq!(mode, "b-busy");
            assert!(close_results::take().is_empty());
            assert!(check_abc_connections_closed(parts).is_err());
            assert_eq!(
                (
                    protection.candidate().retained_test_identity().unwrap(),
                    protection.candidate().sqlite_test_handle()
                ),
                c_id
            );
            super::b_close_outcome::check_b_lock(&b_path, true);
            super::b_close_outcome::check_b_lock(&c_path, true);
            event(&root, "BFailed");
        }
        SourcePhase::Live(_) => {
            assert_eq!(mode, "live");
            assert!(close_results::take().is_empty());
            assert!(check_abc_connections_closed(parts).is_err());
            assert!(!scope.terminal_policy.is_armed());
            event(&root, "Live");
        }
        _ => panic!("incomplete/incorrect phase"),
    }
    assert!(std::ptr::eq(parts.maintenance, m));
    let session = parts.session.as_ref().unwrap();
    assert_eq!(session.session_id(), session_id);
    assert_eq!(session.test_reservation_slot(), reservation);
    assert_eq!(f.state.test_startup_owner(), lease);
    parts
        .binding
        .as_ref()
        .unwrap()
        .check_session(session)
        .unwrap();
    assert_eq!(
        parts.pending.as_ref().unwrap().bytes,
        fs::read(root.join("expected-p")).unwrap()
    );
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    m.assert_running_recovery_generation_for_test(1);
    event(&root, "whole-owner-retained");
    if mode == "live" {
        drop(result);
        drop(scope);
        std::process::exit(0);
    }
    assert!(scope.terminal_policy.is_armed());
    event(&root, "whole-owner-drop");
    drop(result);
    std::process::exit(76);
}
#[test]
fn production_c_phases_keep_real_outer_resources() {
    for mode in [
        "ok",
        "busy",
        "b-busy",
        "internal",
        "panic-before",
        "panic-outcome",
        "panic-phase",
        "negative-handoff",
        "negative-panic",
        "file-error",
        "double-close",
        "retry-failed",
        "live",
        "alias-p",
        "alias-lock",
        "alias-p-race",
        "alias-lock-race",
        "alias-p-negative",
        "alias-lock-negative",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["production_c_close::production_c_child", "--nocapture"])
            .env("MNO_PRODUCTION_C_ROOT", root.path())
            .env("MNO_PRODUCTION_C_MODE", mode)
            .env("TMPDIR", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("timeout {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut expected = if mode.starts_with("alias-") {
            vec![]
        } else {
            vec!["prepared-C-chain"]
        };
        match mode {
            m if m.starts_with("alias-") => {
                if m.ends_with("race") {
                    expected.push("replacement-race-lock-held");
                }
                expected.push("alias-installed");
                if m.ends_with("negative") {
                    expected.push("negative-lock-loss");
                } else {
                    expected.extend([
                        "rejected-lock-held",
                        "original-owners-retained",
                        "whole-owner-drop",
                    ]);
                }
            }
            "live" | "b-busy" => {
                expected.extend([
                    "method-returned",
                    if mode == "live" { "Live" } else { "BFailed" },
                    "whole-owner-retained",
                ]);
                if mode != "live" {
                    expected.push("whole-owner-drop");
                }
            }
            "internal" => expected.extend(["before-consume", "internal-panic"]),
            "panic-before" => expected.push("before-consume"),
            "panic-outcome" => expected.extend(["before-consume", "after-outcome"]),
            "panic-phase" => expected.extend(["before-consume", "after-outcome", "after-phase"]),
            "negative-handoff" => {
                expected.extend(["before-consume", "after-outcome", "before-empty-check"])
            }
            _ => {
                expected.extend([
                    "before-consume",
                    "after-outcome",
                    "after-phase",
                    "before-empty-check",
                    "empty-guard-verified",
                ]);
                if mode == "negative-panic" {
                    expected.push("panic-returned");
                } else {
                    expected.extend([
                        "method-returned",
                        if mode == "busy" || mode == "retry-failed" {
                            "CFailed"
                        } else {
                            "Closed"
                        },
                        "whole-owner-retained",
                        "whole-owner-drop",
                    ]);
                }
            }
        }
        let events = fs::read_to_string(root.path().join("events")).unwrap_or_default();
        assert_eq!(events, expected.join("\n") + "\n", "{mode}: {status}");
        if mode.ends_with("negative") {
            assert_eq!(status.code(), Some(78));
            assert_ne!(status.signal(), Some(libc::SIGABRT));
        } else if mode == "negative-panic" {
            assert_eq!(status.code(), Some(77));
            assert_ne!(status.signal(), Some(libc::SIGABRT));
        } else if mode == "live" {
            assert!(status.success());
        } else {
            assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
        }
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
