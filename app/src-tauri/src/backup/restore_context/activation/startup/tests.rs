use super::*;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Command, Output},
};
const CHILD: &str = "backup::restore_context::activation::startup::tests::startup_child";
fn launch(root: &Path, mode: &str, action: &str, expected: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_ACTIVATION_MODE", mode)
        .env("MNO_STARTUP_ACTION", action)
        .env("MNO_STARTUP_EXPECTED", expected)
        .env("TMPDIR", root)
        .output()
        .unwrap()
}
fn ready(state: &DatabaseState, root: &Path, expected: &str) {
    let operation = state.operation(None).expect("real Ready admission");
    let db = Connection::open_with_flags(
        root.join("data/mno.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let id: String = db
        .query_row("SELECT id FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        id,
        if expected == "old" {
            "old-entry"
        } else {
            "new-entry"
        }
    );
    db.close().unwrap();
    drop(operation);
    let reference = if expected == "old" {
        root.join("old-bytes")
    } else {
        root.join("source.sqlite")
    };
    assert_eq!(
        verify_file(&root.join("data/mno.sqlite")).unwrap(),
        verify_file(&reference).unwrap()
    );
}
#[test]
fn startup_child() {
    let Some(root) = std::env::var_os("MNO_STARTUP_ACTION")
        .and_then(|_| std::env::var_os("MNO_ACTIVATION_ROOT"))
    else {
        return;
    };
    let root = PathBuf::from(root);
    let action = std::env::var("MNO_STARTUP_ACTION").unwrap();
    let expected = std::env::var("MNO_STARTUP_EXPECTED").unwrap();
    crate::panic_policy::install();
    let state = DatabaseState::default();
    let startup = state.initialize_legacy_startup(&root.join("data"));
    if startup.is_ok() {
        assert_ne!(action, "blocked", "unexpected initial Ready");
        ready(&state, &root, &expected);
        std::process::exit(0);
    }
    assert!(state.operation(None).is_err());
    let owner = state.test_startup_owner();
    if action == "blocked" {
        assert!(status(&state).is_err() || status(&state).unwrap().actions.is_empty());
        assert!(run(&state, Action::FinishNew).is_err());
        assert!(state.operation(None).is_err());
        std::process::exit(72);
    }
    let view = status(&state).expect("valid startup diagnosis");
    assert_eq!(view.operation, Operation::Restore);
    assert_eq!(serde_json::to_value(&view).unwrap()["operation"], "restore");
    if action == "wrong-owner" {
        let proof = complete(state.recovery_session().unwrap(), Action::FinishNew).unwrap();
        let other_dir = tempfile::TempDir::new().unwrap();
        fs::create_dir(other_dir.path().join("recovery")).unwrap();
        let other = DatabaseState::default();
        assert!(other.initialize_legacy_startup(other_dir.path()).is_err());
        assert!(other.admit_activation_verified(proof).is_err());
        assert!(state.operation(None).is_err());
        assert!(other.operation(None).is_err());
        std::process::exit(73);
    }
    if action == "wrong-action" {
        let forbidden = if view.phase == Phase::NewInstalled {
            Action::KeepPrevious
        } else {
            Action::FinishNew
        };
        let marker = root.join("data/recovery").join(MARKER);
        let before = fs::read(&marker).unwrap();
        assert!(run(&state, forbidden).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(fs::read(marker).unwrap(), before);
        std::process::exit(73);
    }

    let action = match action.as_str() {
        "new" => Action::FinishNew,
        "old" => Action::KeepPrevious,
        "continue" => Action::Continue,
        "auto" => {
            if view.actions.contains(&Action::FinishNew) {
                Action::FinishNew
            } else if view.actions.contains(&Action::Continue) {
                Action::Continue
            } else {
                Action::KeepPrevious
            }
        }
        _ => panic!("unknown action"),
    };
    assert!(view.actions.contains(&action), "{view:?}");
    match run(&state, action) {
        Ok(()) => {
            assert_eq!(state.test_startup_owner(), owner);
            ready(&state, &root, &expected);
            assert!(state.recovery_session().is_err());
            assert!(run(&state, action).is_err());
            std::process::exit(0);
        }
        Err(_) => {
            assert!(state.operation(None).is_err());
            std::process::exit(72);
        }
    }
}
fn seed(root: &Path, mode: &str) {
    let out = super::super::tests::command(root, mode).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(if mode == "success" { 0 } else { 88 }),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn check(out: Output, code: i32) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "status {}\n{}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.signal(), None);
}
fn backup(root: &Path) -> PathBuf {
    let marker = read_marker(&root.join("data")).unwrap();
    root.join("data/backups/pre-restore")
        .join(marker.protection_file)
}
fn assert_protection(root: &Path, protection: &Path, bytes: &[u8]) {
    assert_eq!(fs::read(protection).unwrap(), bytes);
    let candidate = super::super::super::super::restore::prepare_candidate(
        protection,
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let marker = fingerprint(&candidate, Arc::new(AtomicBool::new(false))).unwrap();
    match candidate.try_close() {
        Ok(files) => drop(files),
        Err(_) => panic!("protection close"),
    }
    assert_eq!(marker, verify_file(&root.join("old-bytes")).unwrap());
}
#[test]
fn startup_success_and_retry_matrix() {
    let points = [
        "startup-verified",
        "startup-sync-candidate",
        "startup-before-replace",
        "startup-after-replace",
        "startup-sync-replace-source",
        "startup-sync-replace-target",
        "startup-sync-active",
        "startup-sync-root",
        "startup-final-verified",
        "startup-sync-archive-parent",
        "startup-before-retire",
        "startup-after-retire",
        "startup-sync-archive",
        "startup-sync-retired-root",
        "startup-final-barrier",
        "startup-before-admit",
    ];
    for (seed_mode, action, expected) in [
        ("success", "new", "new"),
        ("crash:before-replace", "continue", "new"),
        ("crash:copy-step", "old", "old"),
        ("crash:marker-durable", "old", "old"),
    ] {
        let mut modes = vec!["success".to_owned()];
        for p in points {
            if action != "continue"
                && [
                    "startup-sync-candidate",
                    "startup-before-replace",
                    "startup-after-replace",
                    "startup-sync-replace-source",
                    "startup-sync-replace-target",
                ]
                .contains(&p)
            {
                continue;
            }
            modes.push(format!("crash:{p}"));
            modes.push(format!("fail:{p}"));
        }
        for mode in modes {
            let temp = tempfile::Builder::new()
                .prefix("MNO-startup-activation-")
                .tempdir_in("/private/tmp")
                .unwrap();
            let root = temp.path();
            seed(root, seed_mode);
            let protection = backup(root);
            let before = fs::read(&protection).unwrap();
            fs::write(root.join("events"), b"").unwrap();
            let code = if mode.starts_with("crash:") {
                88
            } else if mode.starts_with("fail:") {
                72
            } else {
                0
            };
            check(launch(root, &mode, action, expected), code);
            let events = fs::read_to_string(root.join("events")).unwrap();
            if let Some(point) = mode.strip_prefix("crash:") {
                assert_eq!(events.lines().last(), Some(point));
            }
            if let Some(point) = mode.strip_prefix("fail:") {
                assert!(events.lines().any(|v| v == point), "{mode}: {events}");
            }
            // Retry after interruption, including AFTER retirement but BEFORE admission.
            // Continue may already have replaced the target, so consult disk via next
            // startup diagnosis rather than assuming the previous phase persisted.
            let retry = "auto";
            check(launch(root, "success", retry, expected), 0);
            check(launch(root, "success", retry, expected), 0);
            assert_protection(root, &protection, &before);
            assert!(!root.join("data/recovery").exists());
            let archives: Vec<_> = fs::read_dir(root.join("data/backups/activations"))
                .unwrap()
                .collect();
            assert_eq!(archives.len(), 1);
            eprintln!("startup activation passed: {seed_mode} / {action} / {mode}");
        }
    }
}
#[test]
fn invalid_states_remain_blocked_without_repair() {
    for mode in [
        "missing-db",
        "damaged-db",
        "wal-header",
        "missing-marker",
        "invalid-marker",
        "unknown-version",
        "missing-protection",
        "damaged-protection",
        "p-conflict",
        "q-conflict",
        "active-journal",
        "active-wal",
        "active-shm",
        "unknown-entry",
        "missing-installation",
        "invalid-installation",
        "unknown-schema",
    ] {
        let temp = tempfile::Builder::new()
            .prefix("MNO-startup-blocked-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let root = temp.path();
        seed(root, "success");
        let protection = backup(root);
        let before = fs::read(&protection).unwrap();
        let data = root.join("data");
        match mode {
            "missing-db" => fs::remove_file(data.join("mno.sqlite")).unwrap(),
            "damaged-db" => fs::write(data.join("mno.sqlite"), b"damaged database").unwrap(),
            "wal-header" => {
                let mut b = fs::read(data.join("mno.sqlite")).unwrap();
                b[18] = 2;
                b[19] = 2;
                fs::write(data.join("mno.sqlite"), b).unwrap();
            }
            "missing-marker" => fs::remove_file(data.join("recovery").join(MARKER)).unwrap(),
            "invalid-marker" => fs::write(data.join("recovery").join(MARKER), b"invalid").unwrap(),
            "unknown-version" => {
                let p = data.join("recovery").join(MARKER);
                let mut v: serde_json::Value =
                    serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["version"] = 2.into();
                fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
            }
            "missing-protection" => {
                fs::rename(&protection, root.join("retained-protection.mno-backup")).unwrap()
            }
            "damaged-protection" => fs::write(&protection, b"broken archive").unwrap(),
            "p-conflict" => {
                fs::write(data.join("recovery/pending-restore.json"), b"old P").unwrap()
            }
            "q-conflict" => {
                fs::write(data.join(".mno-restore-completion-v1.json"), b"old Q").unwrap()
            }
            "active-journal" => {
                fs::write(data.join("mno.sqlite-journal"), b"journal blocker").unwrap()
            }
            "active-wal" => fs::write(data.join("mno.sqlite-wal"), b"wal blocker").unwrap(),
            "active-shm" => fs::write(data.join("mno.sqlite-shm"), b"shm blocker").unwrap(),
            "unknown-entry" => fs::write(data.join("recovery/foreign"), b"unknown").unwrap(),
            "missing-installation" => {
                fs::remove_file(data.join(".mno-initialized-v1.json")).unwrap()
            }
            "invalid-installation" => {
                fs::write(data.join(".mno-initialized-v1.json"), b"invalid").unwrap()
            }
            "unknown-schema" => {
                let db = Connection::open(data.join("mno.sqlite")).unwrap();
                db.execute_batch("PRAGMA user_version=99").unwrap();
                db.close().unwrap();
            }
            _ => unreachable!(),
        }
        let active = fs::read(data.join("mno.sqlite")).ok();
        for _ in 0..2 {
            check(launch(root, "success", "blocked", "new"), 72);
        }
        assert_eq!(fs::read(data.join("mno.sqlite")).ok(), active);
        if mode != "missing-protection" && mode != "damaged-protection" {
            assert_eq!(fs::read(protection).unwrap(), before);
        }
        if mode == "missing-protection" {
            assert_eq!(
                fs::read(root.join("retained-protection.mno-backup")).unwrap(),
                before
            );
        }
        assert!(data.join("recovery").exists());
        if mode == "p-conflict" {
            assert_eq!(
                fs::read(data.join("recovery/pending-restore.json")).unwrap(),
                b"old P"
            );
        }
        if mode == "q-conflict" {
            assert_eq!(
                fs::read(data.join(".mno-restore-completion-v1.json")).unwrap(),
                b"old Q"
            );
        }
        for (name, bytes) in [
            ("journal", b"journal blocker".as_slice()),
            ("wal", b"wal blocker".as_slice()),
            ("shm", b"shm blocker".as_slice()),
        ] {
            if mode == format!("active-{name}") {
                assert_eq!(
                    fs::read(data.join(format!("mno.sqlite-{name}"))).unwrap(),
                    bytes
                );
            }
        }
        eprintln!("blocked activation passed: {mode}");
    }
}

#[test]
fn repeated_interruptions_of_one_operation_preserve_data() {
    let dir = tempfile::Builder::new()
        .prefix("MNO-repeated-startup-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let root = dir.path();
    seed(root, "crash:before-replace");
    let protection = backup(root);
    let original = fs::read(&protection).unwrap();
    for point in [
        "startup-verified",
        "startup-after-replace",
        "startup-before-retire",
        "startup-after-retire",
    ] {
        fs::write(root.join("events"), b"").unwrap();
        check(launch(root, &format!("crash:{point}"), "auto", "new"), 88);
        assert_eq!(
            fs::read_to_string(root.join("events"))
                .unwrap()
                .lines()
                .last(),
            Some(point)
        );
        assert_eq!(fs::read(&protection).unwrap(), original);
    }
    check(launch(root, "success", "auto", "new"), 0);
    assert_protection(root, &protection, &original);
}

#[test]
fn foreign_owner_and_wrong_phase_cannot_admit() {
    for (seed_mode, action) in [
        ("success", "wrong-owner"),
        ("success", "wrong-action"),
        ("crash:before-replace", "wrong-action"),
    ] {
        let dir = tempfile::Builder::new()
            .prefix("MNO-activation-reject-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let root = dir.path();
        seed(root, seed_mode);
        let protection = backup(root);
        let bytes = fs::read(&protection).unwrap();
        check(launch(root, "success", action, "new"), 73);
        check(launch(root, "success", "auto", "new"), 0);
        assert_protection(root, &protection, &bytes);
    }
}
