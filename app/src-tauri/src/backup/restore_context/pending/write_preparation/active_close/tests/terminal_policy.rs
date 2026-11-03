//! Exercises the real production latch, outer scope and whole CloseOwner.
use super::*;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use crate::database_state::maintenance_retention::assert_exclusive;
use std::{
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};

// A hook capture witnesses forbidden B destructor execution after arming.
struct DestructionWitness(PathBuf);
impl Drop for DestructionWitness {
    fn drop(&mut self) {
        fs::write(&self.0, b"B hook destroyed").unwrap();
    }
}
#[test]
fn policy_child() {
    let Some(root) = std::env::var_os("MNO_4I_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_4I_MODE").unwrap();
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
    let witness = DestructionWitness(root.join("B-dropped"));
    ctx.incoming
        .source()
        .unwrap()
        .rollback_hook(Some(move || {
            let _ = &witness;
        }))
        .unwrap();
    let durable = ctx.publish_write_preparation().unwrap();
    let session = m.recovery_session().unwrap();
    let id = session.session_id();
    let pending = durable.prepare_write(session).unwrap();
    let marker = f.root.join(DIRECTORY).join(NAME);
    fs::write(root.join("expected-p"), fs::read(&marker).unwrap()).unwrap();
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    assert_exclusive(&f.state, &f.path);
    if mode.starts_with("retained-") {
        let mut inputs = pending.close_active().unwrap();
        let parts = inputs.owner.after.as_ref().unwrap();
        observe(parts, id, m, &f);
        assert!(std::ptr::eq(
            inputs.owner.terminal_policy.unwrap(),
            &scope.terminal_policy
        ));
        let live = parts.sources.live().unwrap();
        let b_identity = live
            .incoming
            .as_ref()
            .unwrap()
            .retained_test_identity()
            .unwrap();
        let c_identity = live
            .protection
            .as_ref()
            .unwrap()
            .candidate()
            .retained_test_identity()
            .unwrap();
        assert!(b_identity.4 && c_identity.4);
        let p_bytes = parts.pending.as_ref().unwrap().bytes.clone();
        let generation = parts.generation;
        assert_exclusive(&f.state, &f.path);
        let mut guard = BTransferGuard {
            terminal_policy: &scope.terminal_policy,
            incoming: None,
            outcome: None,
        };
        assert!(guard.outcome.is_none());
        let fixture_ref = &f;
        let inputs_ref = &mut inputs; // Borrow only: the real owner stays OUTSIDE catch.
        let unwind = mode == "retained-unwind";
        fs::write(
            root.join("events"),
            format!("real-chain\n{mode}\nguard-established\n"),
        )
        .unwrap();
        scope.terminal_policy.arm_for_test();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let parts = inputs_ref.owner.after.as_mut().unwrap();
            let SourcePhase::Live(live) = &mut parts.sources else {
                panic!("expected Live")
            };
            std::mem::swap(&mut guard.incoming, &mut live.incoming);
            assert!(guard.outcome.is_none());
            assert!(live.incoming.is_none());
            assert_eq!(
                guard
                    .incoming
                    .as_ref()
                    .unwrap()
                    .retained_test_identity()
                    .unwrap(),
                b_identity
            );
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
            assert_eq!(parts.session.as_ref().unwrap().session_id(), id);
            assert_eq!(parts.generation, generation);
            assert_eq!(parts.pending.as_ref().unwrap().bytes, p_bytes);
            assert_eq!(
                m.recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
            assert_exclusive(&fixture_ref.state, &fixture_ref.path);
            // Commit the observation only AFTER swap and all resource assertions.
            fs::write(
                root.join("events"),
                format!("real-chain\n{mode}\nguard-established\nswap-verified\nlocal-guard\n"),
            )
            .unwrap();
            if unwind {
                panic!("retained BTransferGuard unwind");
            }
            drop(guard);
        }));
        match result {
            Err(payload)
                if payload.downcast_ref::<&str>() == Some(&"retained BTransferGuard unwind") =>
            {
                std::process::exit(74)
            }
            Err(_) => std::process::exit(75),
            Ok(()) => std::process::exit(76),
        }
    } else if mode.starts_with("transfer-") {
        // The real B/C remain in inputs outside this LOCAL catch boundary.
        let inputs = pending.close_active().unwrap();
        observe(inputs.owner.after.as_ref().unwrap(), id, m, &f);
        assert!(std::ptr::eq(
            inputs.owner.terminal_policy.unwrap(),
            &scope.terminal_policy
        ));
        assert_exclusive(&f.state, &f.path);
        let negative = mode.contains("negative");
        let unwind = mode.ends_with("unwind");
        let wrong_policy = TerminalPolicy::new();
        let guard = BTransferGuard {
            terminal_policy: if negative {
                &wrong_policy
            } else {
                &scope.terminal_policy
            },
            incoming: None,
            outcome: None,
        };
        assert_eq!(
            std::ptr::eq(guard.terminal_policy, &scope.terminal_policy),
            !negative
        );
        assert!(guard.incoming.is_none());
        assert!(guard.outcome.is_none());
        fs::write(
            root.join("events"),
            format!("real-chain\n{mode}\nlocal-guard\n"),
        )
        .unwrap();
        scope.terminal_policy.arm_for_test();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let local_guard = guard;
            if unwind {
                panic!("empty BTransferGuard unwind");
            }
            drop(local_guard);
        }));
        // Exit BEFORE inputs/scope Drop: outer abort must never mask failure
        // of this guard, including in the permanently retained wrong-policy controls.
        match result {
            Err(payload)
                if payload.downcast_ref::<&str>() == Some(&"empty BTransferGuard unwind") =>
            {
                std::process::exit(74)
            }
            Err(_) => std::process::exit(75),
            Ok(()) => std::process::exit(76),
        }
    } else if mode == "outer" {
        // Never activated inner resources close safely first. Then test the
        // actual outer Maintenance owner's armed Drop in isolation.
        drop(pending.close_active().unwrap());
        assert!(root.join("B-dropped").exists());
        fs::remove_file(root.join("B-dropped")).unwrap();
        assert_exclusive(&f.state, &f.path);
        scope.terminal_policy.arm_for_test();
        fs::write(
            root.join("events"),
            b"real-chain
outer-armed
outer-drop
",
        )
        .unwrap();
        drop(scope);
    } else {
        scope.terminal_policy.arm_for_test();
        if mode == "failure" {
            let mut hit = false;
            let failure = pending
                .close_active_inner(&mut |point| {
                    if point == ClosePoint::AfterClose {
                        hit = true;
                        return Err(io::Error::other("controlled checkpoint"));
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(hit);
            assert!(matches!(failure.error(), ActiveCloseError::Checkpoint));
            let parts = failure.owner.after.as_ref().unwrap();
            observe(parts, id, m, &f);
            assert!(std::ptr::eq(
                failure.owner.terminal_policy.unwrap(),
                &scope.terminal_policy
            ));
            fs::write(
                root.join("events"),
                b"real-chain
inner-armed
expected-error-retained
inner-drop
",
            )
            .unwrap();
            drop(failure);
        } else {
            let inputs = pending.close_active().unwrap();
            observe(inputs.owner.after.as_ref().unwrap(), id, m, &f);
            assert!(std::ptr::eq(
                inputs.owner.terminal_policy.unwrap(),
                &scope.terminal_policy
            ));
            assert_exclusive(&f.state, &f.path);
            if mode == "unwind" {
                fs::write(
                    root.join("events"),
                    b"real-chain
inner-armed
intended-unwind
",
                )
                .unwrap();
                // The actual outer Maintenance scope stays outside the catch.
                // Only the inner owner unwinds here; its armed Drop must abort.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _inputs = inputs;
                    panic!("armed real CloseOwner unwind");
                }));
                // Never unwind/drop the armed outer scope on this failure path:
                // its abort would conceal a missing inner abort. Do not even
                // drop a potentially unrelated panic payload before exiting.
                match result {
                    Err(payload)
                        if payload.downcast_ref::<&str>()
                            == Some(&"armed real CloseOwner unwind") =>
                    {
                        std::process::exit(74);
                    }
                    Err(_) => std::process::exit(75),
                    Ok(()) => std::process::exit(76),
                }
            }
            assert_eq!(mode, "inner");
            fs::write(
                root.join("events"),
                b"real-chain
inner-armed
inner-drop
",
            )
            .unwrap();
            drop(inputs);
        }
    }
    fs::write(root.join("continued"), b"forbidden").unwrap();
    panic!("armed Drop returned");
}
#[test]
fn real_policy_aborts_before_inner_fields_and_outer_maintenance() {
    for (mode, expected) in [
        (
            "retained-drop",
            "real-chain\nretained-drop\nguard-established\nswap-verified\nlocal-guard\n",
        ),
        (
            "retained-unwind",
            "real-chain\nretained-unwind\nguard-established\nswap-verified\nlocal-guard\n",
        ),
        ("transfer-drop", "real-chain\ntransfer-drop\nlocal-guard\n"),
        (
            "transfer-unwind",
            "real-chain\ntransfer-unwind\nlocal-guard\n",
        ),
        (
            "transfer-negative-drop",
            "real-chain\ntransfer-negative-drop\nlocal-guard\n",
        ),
        (
            "transfer-negative-unwind",
            "real-chain\ntransfer-negative-unwind\nlocal-guard\n",
        ),
        (
            "inner",
            "real-chain
inner-armed
inner-drop
",
        ),
        (
            "failure",
            "real-chain
inner-armed
expected-error-retained
inner-drop
",
        ),
        (
            "unwind",
            "real-chain
inner-armed
intended-unwind
",
        ),
        (
            "outer",
            "real-chain
outer-armed
outer-drop
",
        ),
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["terminal_policy::policy_child", "--nocapture"])
            .env("MNO_4I_ROOT", root.path())
            .env("MNO_4I_MODE", mode)
            .env("TMPDIR", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("policy child timeout: {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if mode.starts_with("transfer-negative-") {
            // A wrong NeverArmed policy suppresses LOCAL abort while the real
            // outer policy remains Armed. The required-abort verdict rejects it.
            assert_ne!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
            assert_eq!(
                status.code(),
                Some(if mode.ends_with("unwind") { 74 } else { 76 }),
                "{mode}: {status}"
            );
        } else {
            assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
        }
        assert_eq!(
            fs::read_to_string(root.path().join("events")).unwrap(),
            expected
        );
        assert!(!root.path().join("continued").exists());
        assert!(!root.path().join("B-dropped").exists());
        let data = PathBuf::from(std::ffi::OsString::from(
            String::from_utf8(fs::read(root.path().join("data-root")).unwrap()).unwrap(),
        ));
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
            crate::startup::StartupOutcome::Ready(_) => panic!("P must block startup"),
        }
    }
}

#[test]
fn never_armed_real_scope_preserves_inner_then_maintenance_release() {
    let f = Fixture::new();
    let journal = fs::read(&f.path).unwrap();
    {
        let scope = RestoreMaintenanceScope {
            maintenance: f.state.maintenance().unwrap(),
            terminal_policy: TerminalPolicy::new(),
        };
        let m = scope.maintenance();
        let mut ctx = context(&f, m);
        ctx.terminal_policy = Some(&scope.terminal_policy);
        let durable = ctx.publish_write_preparation().unwrap();
        let id;
        let inputs = {
            let session = m.recovery_session().unwrap();
            id = session.session_id();
            durable
                .prepare_write(session)
                .unwrap()
                .close_active()
                .unwrap()
        };
        observe(inputs.owner.after.as_ref().unwrap(), id, m, &f);
        drop(BTransferGuard {
            terminal_policy: &scope.terminal_policy,
            incoming: None,
            outcome: None,
        });
        // NeverArmed empty guard did not release the real chain or Maintenance.
        observe(inputs.owner.after.as_ref().unwrap(), id, m, &f);
        drop(inputs);
        assert_exclusive(&f.state, &f.path);
        assert_eq!(fs::read(&f.path).unwrap(), journal);
        blocked(m, &f);
    }
    // Recovery still blocks normal use; Maintenance itself is released.
    let next = f.state.maintenance().unwrap();
    blocked(&next, &f);
    assert_eq!(fs::read(&f.path).unwrap(), journal);
}
