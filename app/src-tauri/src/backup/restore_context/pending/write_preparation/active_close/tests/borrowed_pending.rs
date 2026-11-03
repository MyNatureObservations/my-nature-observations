//! Borrowed PendingMarker checks, not the later OwnedPending validator.
use super::*;
use crate::backup::restore::close_results;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use std::{
    io::{Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileExt, MetadataExt, symlink},
            process::ExitStatusExt,
        },
    },
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn event(root: &Path, text: &str) {
    use std::io::Write;
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{text}"
    )
    .unwrap();
}
fn locks(paths: &[PathBuf; 2]) {
    for path in paths {
        super::b_close_outcome::check_b_lock(path, true);
    }
}
fn identities(ctx: &RestoreContext<'_, '_>) -> Vec<((i32, u64, u64, u64, bool), usize)> {
    [&ctx.incoming, ctx.protection.as_ref().unwrap().candidate()]
        .into_iter()
        .map(|b| (b.retained_test_identity().unwrap(), b.sqlite_test_handle()))
        .collect()
}
fn restore_p(file: &File, bytes: &[u8]) {
    file.write_all_at(bytes, 0).unwrap();
    file.set_len(bytes.len() as u64).unwrap();
}
fn binding(result: MarkerResult<()>) {
    assert!(matches!(result, Err(MarkerError::Binding)));
}

#[test]
fn borrowed_pending_child() {
    let Some(root) = std::env::var_os("MNO_BORROWED_P_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_BORROWED_P_MODE").unwrap();
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
    let paths = staging(&ctx);
    let before = identities(&ctx);
    let lease = f.state.test_startup_owner();
    let named = f.root.join(DIRECTORY).join(NAME);
    let saved = named.with_extension("saved");
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    close_results::take();
    locks(&paths);
    event(&root, "B-C-live");

    if mode.contains("publish-") {
        let probe_root = root.clone();
        let probe_paths = paths.clone();
        let probe_named = named.clone();
        let probe_saved = saved.clone();
        let target = usize::from(mode.ends_with("-c"));
        let negative = mode.starts_with("negative-");
        let failure = retained_probe::with_before(
            move |marker| {
                // This is reached only AFTER the real final barrier returned. The
                // older FinalSync hook below records its BEFORE-barrier boundary.
                event(&probe_root, "after-final-sync");
                locks(&probe_paths);
                let mut bytes = vec![0; marker.bytes.len()];
                marker.marker_file.read_exact_at(&mut bytes, 0).unwrap();
                assert_eq!(bytes, marker.bytes);
                fs::write(probe_root.join("expected-p"), &bytes).unwrap();
                fs::rename(&probe_named, &probe_saved).unwrap();
                fs::hard_link(&probe_paths[target], &probe_named).unwrap();
                event(&probe_root, "alias-at-final-check");
                if negative {
                    // Reproduce the old primitive only in this disposable child.
                    drop(File::open(&probe_named).unwrap());
                }
            },
            || {
                ctx.publish_write_preparation_inner(&mut |point| {
                    if point == PreparationPoint::Publication(Point::FinalSync) {
                        event(&root, "before-final-sync");
                    }
                    Ok(())
                })
            },
        )
        .unwrap_err();
        assert!(matches!(
            failure.error(),
            WritePreparationError::Marker(MarkerError::Binding)
        ));
        let parts = failure.owner.0.as_ref().unwrap();
        assert!(parts.pending.is_none() && parts.binding.is_none() && parts.session.is_none());
        assert_eq!(identities(&parts.context), before);
        assert!(std::ptr::eq(parts.context.maintenance, m));
        assert!(std::ptr::eq(
            parts.context.terminal_policy.unwrap(),
            &scope.terminal_policy
        ));
        assert!(scope.terminal_policy.is_never_armed());
        assert_eq!(f.state.test_startup_owner(), lease);
        assert!(m.ensure_usable().is_err());
        assert!(close_results::take().is_empty());
        event(&root, "caller-failure-retains-context");
        if negative {
            super::b_close_outcome::check_b_lock(&paths[target], false);
            super::b_close_outcome::check_b_lock(&paths[1 - target], true);
            event(&root, "negative-lock-loss");
        } else {
            locks(&paths);
            event(&root, "rejected-with-locks");
        }
        // Marker local was dropped on Err; whole original context is still in
        // failure. No false claim that its P fd survived the failed publication.
        fs::remove_file(&named).unwrap();
        fs::rename(&saved, &named).unwrap();
        event(&root, "exit-before-context-drop");
        std::process::exit(if negative { 78 } else { 0 });
    }

    let auth = ctx.authorize().unwrap();
    let marker = publish_with(&auth, |_| Ok(())).unwrap();
    let session = m.recovery_session().unwrap(); // AFTER actual publication/authorization.
    let session_id = session.session_id();
    let reservation = session.test_reservation_slot();
    let file = &marker.marker_file;
    let held = (
        file.as_raw_fd(),
        file.metadata().unwrap().dev(),
        file.metadata().unwrap().ino(),
    );
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(7)).unwrap();
    fs::write(root.join("expected-p"), &marker.bytes).unwrap();
    event(&root, "borrowed-marker");
    let alias = mode.ends_with("-b") || mode.ends_with("-c");
    let target = usize::from(mode.ends_with("-c"));
    if alias {
        fs::rename(&named, &saved).unwrap();
        fs::hard_link(&paths[target], &named).unwrap();
    }
    match mode.as_str() {
        "valid" => marker.check().unwrap(),
        "direct-b" | "direct-c" => binding(marker.check()),
        "invalid-names" => {
            fs::rename(&named, &saved).unwrap();
            fs::write(&named, &marker.bytes).unwrap();
            binding(marker.check());
            fs::remove_file(&named).unwrap();
            fs::create_dir(&named).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::Invalid)));
            fs::remove_dir(&named).unwrap();
            symlink(&saved, &named).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::Invalid)));
            fs::remove_file(&named).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::Io(_))));
            fs::rename(&saved, &named).unwrap();
            fs::hard_link(&named, &saved).unwrap();
            binding(marker.check());
            fs::remove_file(&saved).unwrap();
        }
        "content" => {
            file.write_all_at(b"!", 0).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::Invalid)));
            restore_p(file, &marker.bytes);
            file.write_all_at(b" ", marker.bytes.len() as u64).unwrap();
            binding(marker.check()); // Valid JSON but different bytes, including the suffix.
            restore_p(file, &marker.bytes);
            file.set_len(0).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::Invalid)));
            restore_p(file, &marker.bytes);
            let mut padded = marker.bytes.clone();
            padded.resize(LIMIT as usize, b' ');
            restore_p(file, &padded);
            binding(marker.check()); // Exactly LIMIT, complete valid JSON.
            file.write_all_at(b" ", LIMIT).unwrap();
            assert!(matches!(marker.check(), Err(MarkerError::TooLarge)));
            restore_p(file, &marker.bytes);
        }
        "grow-metadata" | "grow-short" | "oversize-metadata" | "truncate-metadata"
        | "truncate-mid" => {
            if mode == "truncate-mid" {
                let mut padded = marker.bytes.clone();
                padded.resize(8192, b' ');
                restore_p(file, &padded);
            }
            let fired = Arc::new(AtomicBool::new(false));
            let fired_probe = fired.clone();
            let length = marker.bytes.len() as u64;
            if mode == "grow-short" {
                assert!(
                    length > 0 && length < 4096,
                    "fixture must produce an initial short read"
                );
            }
            let scenario = mode.clone();
            let result = retained_probe::with_reader(
                move |file, offset| {
                    let boundary = match scenario.as_str() {
                        "grow-short" => length,
                        "truncate-mid" => 4096,
                        _ => 0,
                    };
                    if offset == boundary && !fired_probe.swap(true, Ordering::SeqCst) {
                        match scenario.as_str() {
                            "grow-metadata" | "grow-short" => {
                                file.write_all_at(b" ", length).unwrap()
                            }
                            "oversize-metadata" => file.set_len(LIMIT + 1).unwrap(),
                            _ => file.set_len(0).unwrap(),
                        }
                    }
                },
                || marker.check(),
            );
            assert!(
                fired.load(Ordering::SeqCst),
                "intended read boundary must be reached"
            );
            match mode.as_str() {
                "oversize-metadata" => assert!(matches!(result, Err(MarkerError::TooLarge))),
                "truncate-metadata" => assert!(matches!(result, Err(MarkerError::Invalid))),
                _ => binding(result),
            }
            restore_p(file, &marker.bytes);
        }
        "race-b" | "race-c" => {
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = stop.clone();
            let (n, s, main) = (named.clone(), saved.clone(), paths[target].clone());
            let worker = std::thread::spawn(move || {
                let mut cycles = 0;
                while !worker_stop.load(Ordering::Acquire) {
                    fs::remove_file(&n).unwrap();
                    fs::rename(&s, &n).unwrap();
                    std::thread::yield_now();
                    fs::rename(&n, &s).unwrap();
                    fs::hard_link(&main, &n).unwrap();
                    cycles += 1;
                }
                cycles
            });
            for _ in 0..32 {
                assert!(matches!(
                    marker.check(),
                    Ok(()) | Err(MarkerError::Binding) | Err(MarkerError::Io(_))
                ));
                locks(&paths);
            }
            stop.store(true, Ordering::Release);
            assert!(worker.join().unwrap() > 0);
        }
        "content-race" => {
            let stop = AtomicBool::new(false);
            let bytes = &marker.bytes;
            std::thread::scope(|threads| {
                let worker = threads.spawn(|| {
                    let mut cycles = 0;
                    while !stop.load(Ordering::Acquire) {
                        file.set_len(0).unwrap();
                        file.write_all_at(bytes, 0).unwrap();
                        file.write_all_at(b" ", bytes.len() as u64).unwrap();
                        cycles += 1;
                    }
                    cycles
                });
                for _ in 0..32 {
                    assert!(matches!(
                        marker.check(),
                        Ok(()) | Err(MarkerError::Invalid) | Err(MarkerError::Binding)
                    ));
                    locks(&paths);
                }
                stop.store(true, Ordering::Release);
                assert!(worker.join().unwrap() > 0);
            });
            restore_p(file, bytes);
        }
        other => panic!("unknown mode {other}"),
    }
    locks(&paths); // Still under original owners, before name restoration or any Drop.
    assert_eq!(identities(auth.context), before);
    assert!(std::ptr::eq(marker.maintenance, m));
    assert!(scope.terminal_policy.is_never_armed());
    assert_eq!(session.session_id(), session_id);
    assert_eq!(session.test_reservation_slot(), reservation);
    assert_eq!(f.state.test_startup_owner(), lease);
    assert!(m.ensure_usable().is_err());
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(close_results::take().is_empty());
    assert_eq!(
        (
            file.as_raw_fd(),
            file.metadata().unwrap().dev(),
            file.metadata().unwrap().ino()
        ),
        held
    );
    assert_eq!(cursor.stream_position().unwrap(), 7);
    let mut bytes = vec![0; marker.bytes.len()];
    file.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(bytes, marker.bytes);
    if alias {
        fs::remove_file(&named).unwrap();
        fs::rename(&saved, &named).unwrap();
    }
    event(&root, "retained-identities-and-locks");
    event(&root, "exit-before-context-drop");
    std::process::exit(0);
}

#[test]
fn borrowed_pending_validation_preserves_real_locks() {
    for mode in [
        "valid",
        "direct-b",
        "direct-c",
        "publish-b",
        "publish-c",
        "negative-publish-b",
        "negative-publish-c",
        "invalid-names",
        "content",
        "grow-metadata",
        "grow-short",
        "oversize-metadata",
        "truncate-metadata",
        "truncate-mid",
        "race-b",
        "race-c",
        "content-race",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["borrowed_pending::borrowed_pending_child", "--nocapture"])
            .env("MNO_BORROWED_P_ROOT", root.path())
            .env("MNO_BORROWED_P_MODE", mode)
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
        let output = child.wait_with_output().unwrap();
        assert_eq!(status.signal(), None, "unexpected abort {mode}");
        assert_eq!(
            status.code(),
            Some(if mode.starts_with("negative-") { 78 } else { 0 }),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut expected = "B-C-live\n".to_owned();
        if mode.contains("publish-") {
            expected.push_str("before-final-sync\nafter-final-sync\nalias-at-final-check\ncaller-failure-retains-context\n");
            expected.push_str(if mode.starts_with("negative-") {
                "negative-lock-loss\n"
            } else {
                "rejected-with-locks\n"
            });
        } else {
            expected.push_str("borrowed-marker\nretained-identities-and-locks\n");
        }
        expected.push_str("exit-before-context-drop\n");
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
            crate::startup::StartupOutcome::Ready(_) => panic!("P must block Ready"),
        }
    }
}
