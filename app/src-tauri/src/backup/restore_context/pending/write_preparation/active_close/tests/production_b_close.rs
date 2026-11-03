//! Calls the actual private production-defined producer through PreparedWriteEpisode.
use super::*;
use crate::backup::restore::close_results;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use crate::database_state::maintenance_retention::assert_exclusive;
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};
fn event(root: &Path, name: &str) {
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
struct PanicCapture(PathBuf);
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"forbidden");
    }
}
impl Drop for PanicCapture {
    fn drop(&mut self) {
        event(&self.0, "internal-panic");
        let _witness = Cleanup(self.0.join("inner-unwind"));
        panic!("real B internal panic");
    }
}
#[test]
fn production_b_child() {
    let Some(root) = std::env::var_os("MNO_PRODUCTION_B_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_PRODUCTION_B_MODE").unwrap();
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
    let reservation = session.test_reservation_slot();
    let lease = f.state.test_startup_owner();
    let inputs = durable
        .prepare_write(session)
        .unwrap()
        .close_active()
        .unwrap();
    let mut episode = inputs.into_epoch_inputs().prepare_episode().unwrap();
    let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
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
    if mode == "busy" {
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
        let later = Cleanup(root.join("later-cleanup"));
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
    assert_exclusive(&f.state, &f.path); // Deliberate helper panic BEFORE hook.
    super::b_close_outcome::check_b_lock(&b_path, true);
    if mode == "pre-error" {
        episode.inputs.inputs.owner.terminal_policy = None;
    }
    let panic_at = match mode.as_str() {
        "panic-before" => Some(BClosePoint::BeforeConsume),
        "panic-outcome" => Some(BClosePoint::AfterOutcome),
        "panic-phase" => Some(BClosePoint::AfterPhase),
        _ => None,
    };
    b_close_probe::configure(
        root.clone(),
        panic_at,
        (mode == "file-error").then(|| (b_path.clone(), b_path.with_file_name("moved.sqlite"))),
        mode == "negative-handoff",
    );
    close_results::take();
    event(&root, "prepared-real-chain");
    crate::panic_policy::install();
    let result = episode.close_b();
    let result = if mode == "double-close" {
        result.unwrap().close_b()
    } else {
        result
    };
    // An omitted checkpoint panic must not fall through to outer owner Drop.
    if mode.starts_with("panic-") || mode == "negative-panic" {
        event(&root, "panic-returned");
        std::process::exit(77);
    }
    event(&root, "method-returned");
    let owner = match &result {
        Ok(episode) => {
            assert_eq!(mode, "ok");
            &episode.inputs.inputs.owner
        }
        Err(failure) => {
            assert!(matches!(
                (mode.as_str(), failure.error()),
                ("busy", ActiveCloseError::BSourceClose)
                    | ("file-error", ActiveCloseError::BFileCheck)
                    | ("pre-error", ActiveCloseError::Binding)
                    | ("double-close", ActiveCloseError::Binding)
            ));
            &failure.owner
        }
    };
    let parts = owner.after.as_ref().unwrap();
    let c = match &parts.sources {
        SourcePhase::BClosed {
            incoming,
            protection,
            record,
        } => {
            assert!(record.is_none());
            let incoming = incoming.as_ref().unwrap();
            assert!(mode == "ok" || mode == "file-error" || mode == "double-close");
            assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_OK]);
            if mode != "file-error" {
                incoming.check_retained_file().unwrap();
                super::b_close_outcome::check_b_lock(&b_path, false);
            } else {
                assert!(incoming.check_retained_file().is_err());
            }
            event(&root, "BClosed");
            protection.as_ref().unwrap()
        }
        SourcePhase::BFailed {
            incoming,
            protection,
        } => {
            assert_eq!(mode, "busy");
            assert_eq!(close_results::take(), vec![rusqlite::ffi::SQLITE_BUSY]);
            assert_eq!(incoming.retained_test_identity().unwrap(), b_identity);
            assert!(
                matches!(&incoming.error, rusqlite::Error::SqliteFailure(code, _) if code.extended_code == rusqlite::ffi::SQLITE_BUSY)
            );
            super::b_close_outcome::check_b_lock(&b_path, true);
            event(&root, "BFailed");
            protection
        }
        SourcePhase::Live(live) => {
            assert_eq!(mode, "pre-error");
            assert!(scope.terminal_policy.is_never_armed());
            assert!(close_results::take().is_empty());
            assert_eq!(
                (
                    live.incoming
                        .as_ref()
                        .unwrap()
                        .retained_test_identity()
                        .unwrap(),
                    live.incoming.as_ref().unwrap().sqlite_test_handle()
                ),
                b_identity
            );
            live.protection.as_ref().unwrap()
        }
        _ => panic!("unexpected phase"),
    };
    assert_eq!(c.candidate().retained_test_identity().unwrap(), c_identity);
    assert!(std::ptr::eq(parts.maintenance, m));
    let session = parts.session.as_ref().unwrap();
    assert_eq!(session.session_id(), id);
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
    if mode == "pre-error" {
        drop(result);
        assert!(scope.terminal_policy.is_never_armed());
        drop(scope);
        let next = f.state.maintenance().unwrap();
        assert!(next.ensure_usable().is_err());
        event(&root, "never-armed-released");
        std::process::exit(0);
    }
    assert!(!scope.terminal_policy.is_never_armed());
    event(&root, "whole-owner-drop");
    drop(result); // Must abort before C, P, session or reservation are dropped.
    std::process::exit(76);
}
#[test]
fn production_b_phases_keep_real_outer_resources() {
    for mode in [
        "ok",
        "busy",
        "file-error",
        "internal",
        "panic-before",
        "panic-outcome",
        "panic-phase",
        "negative-handoff",
        "negative-panic",
        "pre-error",
        "double-close",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["production_b_close::production_b_child", "--nocapture"])
            .env("MNO_PRODUCTION_B_ROOT", root.path())
            .env("MNO_PRODUCTION_B_MODE", mode)
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
        let expected = match mode {
            "internal" => "prepared-real-chain\nbefore-consume\ninternal-panic\n",
            "panic-before" => "prepared-real-chain\nbefore-consume\n",
            "panic-outcome" => "prepared-real-chain\nbefore-consume\nafter-outcome\n",
            "panic-phase" => "prepared-real-chain\nbefore-consume\nafter-outcome\nafter-phase\n",
            "negative-handoff" => {
                "prepared-real-chain\nbefore-consume\nafter-outcome\nbefore-empty-check\n"
            }
            "negative-panic" => {
                "prepared-real-chain\nbefore-consume\nafter-outcome\nafter-phase\nbefore-empty-check\nempty-guard-verified\npanic-returned\n"
            }
            "pre-error" => {
                "prepared-real-chain\nmethod-returned\nwhole-owner-retained\nnever-armed-released\n"
            }
            "busy" => {
                "prepared-real-chain\nbefore-consume\nafter-outcome\nafter-phase\nbefore-empty-check\nempty-guard-verified\nmethod-returned\nBFailed\nwhole-owner-retained\nwhole-owner-drop\n"
            }
            _ => {
                "prepared-real-chain\nbefore-consume\nafter-outcome\nafter-phase\nbefore-empty-check\nempty-guard-verified\nmethod-returned\nBClosed\nwhole-owner-retained\nwhole-owner-drop\n"
            }
        };
        if mode == "negative-panic" {
            assert_eq!(status.code(), Some(77));
            assert_ne!(status.signal(), Some(libc::SIGABRT));
        } else if mode == "pre-error" {
            assert!(status.success());
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
