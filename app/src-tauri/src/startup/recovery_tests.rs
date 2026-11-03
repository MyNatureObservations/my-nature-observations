use super::*;
use crate::database_state::{DatabaseState, GENERATION_CONFLICT, RECOVERY_REQUIRED};
use std::{
    cell::Cell,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};
use tempfile::TempDir;

#[test]
fn every_recovery_combination_blocks_without_database_or_initialization() {
    for (directory, receipt) in [
        (true, None),
        (false, Some(b"".as_slice())),
        (false, Some(b"{corrupt".as_slice())),
        (false, Some(b"{}".as_slice())),
        (true, Some(b"unknown receipt".as_slice())),
    ] {
        let dir = TempDir::new().unwrap();
        if directory {
            fs::create_dir(dir.path().join("recovery")).unwrap();
        }
        if let Some(bytes) = receipt {
            fs::write(dir.path().join(".mno-restore-completion-v1.json"), bytes).unwrap();
        }
        let called = Cell::new(false);
        let outcome = begin_with(
            dir.path(),
            |p| fs::symlink_metadata(p),
            |_, _| {
                called.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(!called.get());
        assert!(matches!(outcome, StartupOutcome::Recovery(_)));
        assert!(DirectoryLease::open_existing(dir.path()).is_err());
        drop(outcome);
        let state = DatabaseState::default();
        assert_eq!(
            state.initialize_startup(dir.path()).unwrap_err(),
            "database_recovery_artifact_present"
        );
        assert!(!database::expected_database_path(dir.path()).exists());
        assert!(!dir.path().join(".mno-initialized-v1.json").exists());
        assert_eq!(
            state.operation(None).err().unwrap(),
            "database_startup_blocked"
        );
        assert!(state.maintenance().is_err());
        if cfg!(target_os = "macos") {
            let session = state.recovery_session().unwrap();
            session.ensure_current().unwrap();
            assert_eq!(
                session.database_path().unwrap(),
                dir.path().canonicalize().unwrap().join("mno.sqlite")
            );
            assert_eq!(session.maintenance_binding(), None);
            assert!(DirectoryLease::open_existing(dir.path()).is_err());
            drop(session);
        } else {
            assert!(state.recovery_session().is_err());
        }
        assert!(DirectoryLease::open_existing(dir.path()).is_err());
        assert!(state.operation(None).is_err());
        drop(state);
        assert!(DirectoryLease::open_existing(dir.path()).is_ok());
    }
}
#[test]
fn receipt_filesystem_errors_and_unsafe_artifacts_never_mean_absence() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let called = Cell::new(false);
    let result = begin_with(
        dir.path(),
        |p| {
            if p.file_name().unwrap() == ".mno-restore-completion-v1.json" {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                fs::symlink_metadata(p)
            }
        },
        |_, _| {
            called.set(true);
            Ok(())
        },
    );
    let StartupOutcome::Recovery(gate) = result.unwrap() else {
        panic!("I/O must remain gated")
    };
    assert!(DirectoryLease::open_existing(dir.path()).is_err());
    drop(gate);
    assert!(!called.get());
    assert!(!database::expected_database_path(dir.path()).exists());
    for kind in 0..3 {
        let dir = TempDir::new().unwrap();
        let artifact = dir.path().join(".mno-restore-completion-v1.json");
        match kind {
            0 => symlink(dir.path().join("absent"), &artifact).unwrap(),
            1 => fs::create_dir(&artifact).unwrap(),
            _ => fs::write(&artifact, b"not a valid receipt").unwrap(),
        }
        let state = DatabaseState::default();
        assert_eq!(
            state.initialize_startup(dir.path()).unwrap_err(),
            "database_recovery_artifact_present"
        );
        assert!(fs::symlink_metadata(artifact).is_ok());
        assert!(!database::expected_database_path(dir.path()).exists());
    }
}
#[test]
fn recovery_startup_preserves_existing_journal_and_prevents_second_directory_attempt() {
    let parent = TempDir::new().unwrap();
    let state = DatabaseState::default();
    let path = state
        .initialize_startup(&parent.path().join("book"))
        .unwrap();
    let root = path.parent().unwrap().to_owned();
    drop(state);
    let before = fs::read(&path).unwrap();
    fs::write(root.join(".mno-restore-completion-v1.json"), b"incomplete").unwrap();
    let blocked = DatabaseState::default();
    assert!(blocked.initialize_startup(&root).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        blocked
            .initialize_startup(&parent.path().join("other"))
            .is_err()
    );
    assert!(!parent.path().join("other").exists());
    assert!(DirectoryLease::open_existing(&root).is_err());
}
#[cfg(target_os = "macos")]
#[test]
fn runtime_session_borrows_existing_maintenance_and_never_unblocks_on_drop() {
    let dir = TempDir::new().unwrap();
    let state = DatabaseState::default();
    let path = state.initialize_startup(&dir.path().join("book")).unwrap();
    assert!(state.recovery_session().is_err());
    let maintenance = state.maintenance().unwrap();
    let before = fs::read(&path).unwrap();
    let session = maintenance.recovery_session().unwrap();
    assert_eq!(
        maintenance.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert_eq!(session.database_path().unwrap(), path);
    assert_eq!(
        session.maintenance_binding(),
        Some((maintenance.operation_id(), 1))
    );
    session.ensure_current().unwrap(); // No lock acquisition: already held.
    let id = session.session_id();
    assert!(DirectoryLease::open_existing(path.parent().unwrap()).is_err());
    drop(session);
    assert_eq!(maintenance.generation(), 1);
    assert!(maintenance.ensure_usable().is_err());
    let another = maintenance.recovery_session().unwrap();
    assert_ne!(another.session_id(), id);
    drop(another);
    drop(maintenance);
    assert_eq!(state.operation(Some(1)).err().unwrap(), RECOVERY_REQUIRED);
    assert_eq!(state.operation(Some(2)).err().unwrap(), GENERATION_CONFLICT);
    assert_eq!(fs::read(path).unwrap(), before);
}
#[cfg(target_os = "macos")]
#[test]
fn session_rejects_directory_and_lockfile_substitution() {
    for replacement in 0..3 {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("book");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("recovery")).unwrap();
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        let session = state.recovery_session().unwrap();
        if replacement == 1 {
            fs::rename(root.join(LOCK), root.join("old-lock")).unwrap();
            fs::write(root.join(LOCK), b"").unwrap();
        } else if replacement == 2 {
            fs::hard_link(root.join(LOCK), root.join("alias-lock")).unwrap();
        } else {
            fs::rename(&root, parent.path().join("moved")).unwrap();
            fs::create_dir(&root).unwrap();
        }
        assert!(session.ensure_current().is_err());
        assert!(session.database_path().is_err());
        assert!(state.recovery_session().is_err());
        assert!(state.operation(None).is_err());
        assert!(!root.join("mno.sqlite").exists());
    }
}
#[test]
fn root_replacement_during_checkpoint_is_detected_before_initializer() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("book");
    fs::create_dir(&root).unwrap();
    let called = Cell::new(false);
    let result = begin_with(
        &root,
        |p| {
            let metadata = fs::symlink_metadata(p);
            if p.file_name().unwrap() == ".mno-restore-completion-v1.json" {
                fs::rename(&root, parent.path().join("moved")).unwrap();
                fs::create_dir(&root).unwrap();
            }
            metadata
        },
        |_, _| {
            called.set(true);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(!called.get());
    assert!(!root.join("mno.sqlite").exists());
    assert!(!parent.path().join("moved/mno.sqlite").exists());
}
#[test]
fn existing_only_lease_never_creates_root_lock_or_database() {
    let dir = TempDir::new().unwrap();
    assert!(DirectoryLease::open_existing(&dir.path().join("absent")).is_err());
    assert!(!dir.path().join("absent").exists());
    assert!(DirectoryLease::open_existing(dir.path()).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn recovery_lock_child() {
    let Some(root) = std::env::var_os("MNO_RECOVERY_LEASE_CHILD") else {
        return;
    };
    let root = Path::new(&root);
    if std::env::var_os("MNO_RECOVERY_LEASE_HOLD").is_some() {
        let state = DatabaseState::default();
        assert_eq!(
            state.initialize_startup(root).unwrap_err(),
            "database_recovery_artifact_present"
        );
        println!("MNO_RECOVERY_HELD");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    } else {
        println!(
            "{}",
            if DirectoryLease::open_existing(root).is_ok() {
                "MNO_ACQUIRED"
            } else {
                "MNO_BUSY"
            }
        );
    }
}
#[test]
fn blocked_startup_lock_survives_session_drop_and_is_released_after_process_crash() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("recovery")).unwrap();
    let state = DatabaseState::default();
    assert!(state.initialize_startup(dir.path()).is_err());
    let probe = || {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::recovery_tests::recovery_lock_child",
                "--nocapture",
            ])
            .env("MNO_RECOVERY_LEASE_CHILD", dir.path())
            .output()
            .unwrap()
    };
    if cfg!(target_os = "macos") {
        drop(state.recovery_session().unwrap());
    }
    let output = probe();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("MNO_BUSY"));
    drop(state);
    let output = probe();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("MNO_ACQUIRED"));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup::recovery_tests::recovery_lock_child",
            "--nocapture",
        ])
        .env("MNO_RECOVERY_LEASE_CHILD", dir.path())
        .env("MNO_RECOVERY_LEASE_HOLD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        if line.contains("MNO_RECOVERY_HELD") {
            break;
        }
    }
    assert!(DirectoryLease::open_existing(dir.path()).is_err());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(DirectoryLease::open_existing(dir.path()).is_ok());
    assert!(dir.path().join("recovery").is_dir());
    assert!(!dir.path().join("mno.sqlite").exists());
}
