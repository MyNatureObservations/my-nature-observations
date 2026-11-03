//! Early retained-P validation with real B/C snapshots, isolated from app data.
use super::*;
use crate::backup::restore::close_results;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use std::{
    io::{Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileExt, symlink},
            process::ExitStatusExt,
        },
    },
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn event(root: &Path, value: &str) {
    use std::io::Write;
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

// Borrowed observations only: no source(), extra main descriptor or connection.
fn identities(parts: &WriteParts<'_, '_>) -> Vec<(i32, u64, u64, u64, bool, usize)> {
    [
        &parts.context.incoming,
        parts.context.protection.as_ref().unwrap().candidate(),
    ]
    .into_iter()
    .map(|candidate| {
        let (fd, dev, ino, len, live) = candidate.retained_test_identity().unwrap();
        (fd, dev, ino, len, live, candidate.sqlite_test_handle())
    })
    .collect()
}
fn locks(paths: &[PathBuf; 2]) {
    for path in paths {
        super::b_close_outcome::check_b_lock(path, true);
    }
}
fn verify(parts: &mut WriteParts<'_, '_>) -> MarkerResult<()> {
    parts.pending.as_mut().unwrap().verify(&parts.context)
}
fn reject(result: MarkerResult<()>, missing: bool) {
    if missing {
        assert!(matches!(result, Err(MarkerError::Io(_))));
    } else {
        assert!(matches!(result, Err(MarkerError::Binding)));
    }
}

#[test]
fn early_pending_child() {
    let Some(root) = std::env::var_os("MNO_EARLY_P_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_EARLY_P_MODE").unwrap();
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
    let mut session = Some(m.recovery_session().unwrap());
    let session_id = session.as_ref().unwrap().session_id();
    let reservation = session.as_ref().unwrap().test_reservation_slot();
    let lease = f.state.test_startup_owner();
    // Keep prepare_write's actual failure path available for the alias cases.
    let mut owner = if mode.starts_with("prepare-") {
        durable.owner
    } else {
        durable
            .prepare_write(session.take().unwrap())
            .unwrap()
            .owner
    };
    let parts = owner.0.as_mut().unwrap();
    let paths = staging(&parts.context);
    let before = identities(parts);
    let p = parts.pending.as_mut().unwrap();
    let marker = p.root.join(DIRECTORY).join(NAME);
    let saved = marker.with_extension("saved");
    let bytes = p.bytes.clone();
    let p_fd = p.marker_file.as_raw_fd();
    p.marker_file.seek(SeekFrom::Start(3)).unwrap();
    fs::write(root.join("expected-p"), &bytes).unwrap();
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    close_results::take();
    locks(&paths);
    event(&root, "live-chain-locked");

    let alias = mode.ends_with("-b") || mode.ends_with("-c");
    let target = usize::from(mode.ends_with("-c"));
    if alias {
        fs::rename(&marker, &saved).unwrap();
        fs::hard_link(&paths[target], &marker).unwrap();
    }
    if mode.starts_with("negative-") {
        // Intentionally reproduce the old hazard ONLY in this isolated child.
        // Its status must be observed BEFORE any owner can Drop/retry SQLite.
        drop(File::open(&marker).unwrap());
        super::b_close_outcome::check_b_lock(&paths[target], false);
        super::b_close_outcome::check_b_lock(&paths[1 - target], true);
        fs::remove_file(&marker).unwrap();
        fs::rename(&saved, &marker).unwrap();
        event(&root, "negative-lock-loss-detected");
        std::process::exit(78);
    }
    if mode.starts_with("race-") {
        // The worker only changes names/links. It never opens either main inode.
        // Success during a valid observation is allowed: metadata is not a lease.
        // Restore the initial alias before the common alias cleanup below.
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let (marker_worker, saved_worker, main) =
            (marker.clone(), saved.clone(), paths[target].clone());
        let worker = std::thread::spawn(move || {
            let mut cycles = 0;
            while !stop_worker.load(Ordering::Acquire) {
                fs::remove_file(&marker_worker).unwrap();
                fs::rename(&saved_worker, &marker_worker).unwrap();
                std::thread::yield_now();
                fs::rename(&marker_worker, &saved_worker).unwrap();
                fs::hard_link(&main, &marker_worker).unwrap();
                cycles += 1;
            }
            cycles
        });
        for _ in 0..32 {
            match verify(owner.0.as_mut().unwrap()) {
                Ok(()) | Err(MarkerError::Binding) | Err(MarkerError::Io(_)) => (),
                other => panic!("unexpected race result: {other:?}"),
            }
            locks(&paths);
        }
        stop.store(true, Ordering::Release);
        assert!(worker.join().unwrap() > 0);
        event(&root, "race-locks-held");
    } else if mode.starts_with("prepare-") {
        let failure = DurableWritePreparation { owner }
            .prepare_write(session.take().unwrap())
            .unwrap_err();
        assert!(matches!(
            failure.error,
            WritePreparationError::Marker(MarkerError::Binding)
        ));
        owner = failure.owner;
        // Binding is installed only after successful verification.
        assert!(owner.0.as_ref().unwrap().binding.is_none());
        event(&root, "prepare-rejected");
    } else if mode.starts_with("close-") {
        let mut visits = 0;
        let mut failure = PendingWrite { owner }
            .close_active_inner(&mut |_| {
                visits += 1;
                Ok(())
            })
            .unwrap_err();
        assert_eq!(visits, 0, "reject before A-close checkpoints");
        assert!(matches!(
            failure.error(),
            ActiveCloseError::Marker(MarkerError::Binding)
        ));
        assert!(failure.owner.after.is_none());
        owner = failure.owner.before.take().unwrap();
        event(&root, "close-precheck-rejected");
    } else if alias {
        reject(verify(owner.0.as_mut().unwrap()), false);
        event(&root, "alias-rejected");
    } else if mode == "invalid-names" {
        fs::rename(&marker, &saved).unwrap();
        // Distinct regular inode, directory, symlink, and missing name.
        fs::write(&marker, &bytes).unwrap();
        reject(verify(owner.0.as_mut().unwrap()), false);
        fs::remove_file(&marker).unwrap();
        fs::create_dir(&marker).unwrap();
        assert!(matches!(
            verify(owner.0.as_mut().unwrap()),
            Err(MarkerError::Invalid)
        ));
        fs::remove_dir(&marker).unwrap();
        symlink(&saved, &marker).unwrap();
        assert!(matches!(
            verify(owner.0.as_mut().unwrap()),
            Err(MarkerError::Invalid)
        ));
        fs::remove_file(&marker).unwrap();
        reject(verify(owner.0.as_mut().unwrap()), true);
        // Held-P nlink mismatch, even with the correct P pathname.
        fs::rename(&saved, &marker).unwrap();
        fs::hard_link(&marker, &saved).unwrap();
        reject(verify(owner.0.as_mut().unwrap()), false);
        fs::remove_file(&saved).unwrap();
        event(&root, "invalid-names-rejected");
    } else if mode == "content" {
        let p = owner.0.as_ref().unwrap().pending.as_ref().unwrap();
        p.marker_file.write_all_at(b"!", 0).unwrap();
        reject(verify(owner.0.as_mut().unwrap()), false);
        let p = owner.0.as_ref().unwrap().pending.as_ref().unwrap();
        p.marker_file.write_all_at(&bytes, 0).unwrap();
        p.marker_file.set_len(bytes.len() as u64 + 1).unwrap();
        reject(verify(owner.0.as_mut().unwrap()), false);
        owner
            .0
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .marker_file
            .set_len(bytes.len() as u64)
            .unwrap();
        event(&root, "content-rejected");
    } else {
        assert_eq!(mode, "valid");
        verify(owner.0.as_mut().unwrap()).unwrap();
        event(&root, "valid");
    }
    // Before restoring substituted names or dropping ANY resource: locks and
    // original owners still exist, and verification did not perform B/C-close.
    locks(&paths);
    let parts = owner.0.as_mut().unwrap();
    assert_eq!(identities(parts), before);
    assert!(std::ptr::eq(parts.context.maintenance, m));
    assert!(std::ptr::eq(
        parts.context.terminal_policy.unwrap(),
        &scope.terminal_policy
    ));
    assert!(scope.terminal_policy.is_never_armed());
    let s = parts.session.as_ref().unwrap();
    assert_eq!(s.session_id(), session_id);
    assert_eq!(s.test_reservation_slot(), reservation);
    assert_eq!(f.state.test_startup_owner(), lease);
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(m.ensure_usable().is_err());
    assert!(close_results::take().is_empty());
    let p = parts.pending.as_mut().unwrap();
    assert_eq!(p.marker_file.as_raw_fd(), p_fd);
    assert_eq!(p.marker_file.stream_position().unwrap(), 3);
    let mut observed = vec![0; bytes.len()];
    p.marker_file.read_exact_at(&mut observed, 0).unwrap();
    assert_eq!(observed, bytes);
    event(&root, "original-owners-and-locks-retained");
    if alias {
        fs::remove_file(&marker).unwrap();
        fs::rename(&saved, &marker).unwrap();
    }
    // Exit with the real chain still alive. The parent checks durable P and
    // fresh startup Recovery, rather than mistaking cleanup for retention.
    event(&root, "exit-before-owner-drop");
    std::process::exit(0);
}

#[test]
fn early_pending_validation_keeps_live_b_c_locks() {
    for mode in [
        "valid",
        "direct-b",
        "direct-c",
        "prepare-b",
        "prepare-c",
        "close-b",
        "close-c",
        "invalid-names",
        "content",
        "race-b",
        "race-c",
        "negative-b",
        "negative-c",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["early_pending::early_pending_child", "--nocapture"])
            .env("MNO_EARLY_P_ROOT", root.path())
            .env("MNO_EARLY_P_MODE", mode)
            .env("TMPDIR", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
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
        assert_eq!(status.signal(), None, "unexpected abort: {mode}");
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            status.code(),
            Some(if mode.starts_with("negative-") { 78 } else { 0 }),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let middle = if mode.starts_with("negative-") {
            "negative-lock-loss-detected"
        } else if mode.starts_with("race-") {
            "race-locks-held"
        } else if mode.starts_with("prepare-") {
            "prepare-rejected"
        } else if mode.starts_with("close-") {
            "close-precheck-rejected"
        } else if mode.starts_with("direct-") {
            "alias-rejected"
        } else if mode == "invalid-names" {
            "invalid-names-rejected"
        } else if mode == "content" {
            "content-rejected"
        } else {
            "valid"
        };
        let mut expected = format!("live-chain-locked\n{middle}\n");
        if !mode.starts_with("negative-") {
            expected.push_str("original-owners-and-locks-retained\nexit-before-owner-drop\n");
        }
        assert_eq!(
            fs::read_to_string(root.path().join("events")).unwrap(),
            expected,
            "{mode}"
        );
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
            crate::startup::StartupOutcome::Ready(_) => panic!("durable P must block Ready"),
        }
    }
}
