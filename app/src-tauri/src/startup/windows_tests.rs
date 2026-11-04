//! Real local-NTFS startup regressions. No installed app or personal data.
use super::*;
use crate::database_state::DatabaseState;
use std::{
    io::Write,
    os::windows::fs::OpenOptionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
fn created() -> (TempDir, PathBuf) {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("app");
    let state = DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    drop(state);
    (parent, root)
}
fn bytes(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = fs::read_dir(root)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = if p.file_name().unwrap() == LOCK {
                assert_eq!(fs::metadata(&p).unwrap().len(), 0);
                Vec::new()
            } else {
                fs::read(&p).unwrap()
            };
            (p, b)
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
#[test]
fn first_launch_nested_boundary_and_repeated_startup_validate_current_schema() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("a/b/app");
    let mut boundary = established::windows::Boundary::capture(&root).unwrap();
    fs::create_dir_all(&root).unwrap();
    boundary.attach(&root.canonicalize().unwrap()).unwrap();
    let expected = parent.path().canonicalize().unwrap();
    let paths = boundary.paths();
    assert_eq!(paths.len(), 4);
    assert_eq!(paths.last().unwrap(), &expected);
    assert!(paths.iter().all(|p| p.starts_with(&expected)));
    drop(boundary);
    let root = parent.path().join("journal");
    let state = DatabaseState::default();
    let path = state.initialize_startup(&root).unwrap();
    database::initialize_database(&path).unwrap();
    assert!(state.operation(None).is_ok());
    assert!(DirectoryLease::acquire(&root).is_err());
    drop(state);
    let before = fs::read(&path).unwrap();
    for _ in 0..3 {
        let state = DatabaseState::default();
        assert_eq!(state.initialize_startup(&root).unwrap(), path);
        assert!(state.operation(None).is_ok());
    }
    assert_eq!(fs::read(path).unwrap(), before);
}
#[test]
fn existing_and_late_database_destinations_are_never_overwritten() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("app");
    let result = initialize_with(
        &root,
        |p| fs::symlink_metadata(p),
        |path, new| {
            assert!(new);
            fs::write(path, b"concurrent creator").unwrap();
            database::create_current_database(path)
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(root.join("mno.sqlite")).unwrap(),
        b"concurrent creator"
    );
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&root).is_err());
    assert!(state.operation(None).is_err());
    assert_eq!(
        fs::read(root.join("mno.sqlite")).unwrap(),
        b"concurrent creator"
    );
}
#[test]
fn concurrent_startup_owner_is_excluded_and_data_is_preserved() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("app");
    let owner = DatabaseState::default();
    let path = owner.initialize_startup(&root).unwrap();
    let before = fs::read(&path).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let other = DatabaseState::default();
                assert_eq!(
                    other.initialize_startup(&root).unwrap_err(),
                    "database_directory_in_use"
                );
                assert!(other.operation(None).is_err());
            })
            .join()
            .unwrap();
    });
    assert_eq!(fs::read(path).unwrap(), before);
    drop(owner);
    DatabaseState::default().initialize_startup(&root).unwrap();
}
#[test]
fn corrupt_marker_database_and_missing_established_database_fail_without_changes() {
    for fault in ["marker", "database", "missing"] {
        let (_parent, root) = created();
        let path = root.join("mno.sqlite");
        match fault {
            "marker" => fs::write(root.join(established::NAME), b"partial marker").unwrap(),
            "database" => fs::write(&path, b"invalid SQLite").unwrap(),
            _ => fs::remove_file(&path).unwrap(),
        };
        let before = bytes(&root);
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(bytes(&root), before);
        if fault == "missing" {
            assert!(!path.exists());
        }
    }
}
#[test]
fn sidecars_and_recovery_artifacts_still_block_without_cleanup() {
    for artifact in [
        "mno.sqlite-wal",
        "mno.sqlite-shm",
        "mno.sqlite-journal",
        "pending-restore.json",
    ] {
        let (_parent, root) = created();
        fs::write(root.join(artifact), b"preserved evidence").unwrap();
        fs::write(root.join("backup-sentinel"), b"backup").unwrap();
        let before = bytes(&root);
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        assert!(state.operation(None).is_err());
        assert!(state.maintenance().is_err());
        assert_eq!(bytes(&root), before);
    }
}
#[test]
fn write_and_sync_failures_never_create_a_ready_or_replacement_journal() {
    use established::windows::{Fault, with_fault};
    for point in [Fault::MarkerWrite, Fault::FileFlush, Fault::DirectoryFlush] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("app");
        let state = DatabaseState::default();
        assert!(with_fault(point, || state.initialize_startup(&root)).is_err());
        assert!(state.operation(None).is_err());
        assert!(!root.join("mno.sqlite").exists());
        let before = bytes(&root);
        let next = DatabaseState::default();
        assert!(next.initialize_startup(&root).is_err());
        assert_eq!(bytes(&root), before);
    }
}
#[test]
fn marker_publication_is_no_clobber_and_errors_preserve_published_evidence() {
    for after in [false, true] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("app");
        let lease = DirectoryLease::acquire(&root).unwrap();
        assert!(
            established::windows::publish_with(&lease.root, &lease.boundary, |at| {
                if at == established::Point::BeforePublish && !after {
                    fs::write(root.join(established::NAME), b"foreign marker")?;
                }
                if at == established::Point::AfterPublish && after {
                    return Err(io::Error::from(io::ErrorKind::StorageFull));
                }
                Ok(())
            })
            .is_err()
        );
        let marker = fs::read(root.join(established::NAME)).unwrap();
        if after {
            assert!(established::read(&root).unwrap());
        } else {
            assert_eq!(marker, b"foreign marker");
        }
        drop(lease);
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        assert!(!root.join("mno.sqlite").exists());
        assert_eq!(fs::read(root.join(established::NAME)).unwrap(), marker);
    }
}
#[test]
fn marker_sharing_denial_is_reported_without_changing_existing_journal() {
    let (_parent, root) = created();
    let _reader = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(root.join(established::NAME))
        .unwrap();
    let before = bytes(&root);
    let state = DatabaseState::default();
    let error = state.initialize_startup(&root).unwrap_err();
    assert!(error.contains("database_startup_io"), "{error}");
    assert!(state.operation(None).is_err());
    assert_eq!(bytes(&root), before);
}
#[test]
fn reparse_roots_and_nonlocal_paths_are_rejected_before_journal_creation() {
    let parent = TempDir::new().unwrap();
    let target = parent.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("preserve"), b"not touched").unwrap();
    let link = parent.path().join("junction");
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(DatabaseState::default().initialize_startup(&link).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    assert_eq!(fs::read(target.join("preserve")).unwrap(), b"not touched");
    fs::remove_dir(link).unwrap();
    assert_eq!(
        established::windows::Boundary::capture(Path::new(r"\\unsupported-server\share\app"))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Unsupported
    );
}
#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_C5C_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let stage = std::env::var("MNO_C5C_STOP").unwrap();
    let lease = DirectoryLease::acquire(&root).unwrap();
    established::windows::publish_with(&lease.root, &lease.boundary, |point| {
        if (stage == "before-marker" && point == established::Point::BeforePublish)
            || (stage == "after-marker" && point == established::Point::AfterPublish)
        {
            std::process::exit(86);
        }
        Ok(())
    })
    .unwrap();
    let path = root.join("mno.sqlite");
    if stage == "partial" {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        f.write_all(b"partial DB").unwrap();
        std::process::exit(86);
    }
    database::create_current_database(&path).unwrap();
    if stage == "created" {
        std::process::exit(86);
    }
    panic!("unknown stage");
}
#[test]
fn interrupted_initialization_blocks_ambiguity_and_can_validate_a_complete_database() {
    for stage in ["before-marker", "after-marker", "partial", "created"] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("app");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::windows_tests::interruption_child",
                "--nocapture",
            ])
            .env("MNO_C5C_ROOT", &root)
            .env("MNO_C5C_STOP", stage)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child timed out");
            }
            std::thread::yield_now();
        };
        assert_eq!(status.code(), Some(86));
        let before = bytes(&root);
        for _ in 0..2 {
            let state = DatabaseState::default();
            let result = state.initialize_startup(&root);
            if stage == "created" {
                let path = result.unwrap();
                database::initialize_database(&path).unwrap();
            } else {
                assert!(result.is_err());
                assert!(state.operation(None).is_err());
            }
        }
        assert_eq!(bytes(&root), before);
    }
}

#[test]
fn final_database_flush_sharing_failure_does_not_return_ready() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("app");
    let mut held = None;
    let result = initialize_with(
        &root,
        |p| fs::symlink_metadata(p),
        |path, new| {
            assert!(new);
            database::create_current_database(path)?;
            held = Some(
                OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(path)
                    .unwrap(),
            );
            Ok(())
        },
    );
    assert!(result.is_err());
    let before = bytes(&root);
    drop(held);
    let state = DatabaseState::default();
    let path = state.initialize_startup(&root).unwrap();
    database::initialize_database(&path).unwrap();
    assert_eq!(bytes(&root), before);
}

#[test]
fn independently_reopened_invalid_new_content_is_not_admitted() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("app");
    let result = initialize_with(
        &root,
        |p| fs::symlink_metadata(p),
        |path, new| {
            assert!(new);
            database::create_current_database(path)?;
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute("INSERT INTO journal_entries VALUES ('entry','2026-11-04','bad','invalid JSON',2,'created','updated')", []).unwrap();
            db.close().unwrap();
            Ok(())
        },
    );
    assert!(result.is_err());
    let before = bytes(&root);
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&root).is_err());
    assert!(state.operation(None).is_err());
    assert_eq!(bytes(&root), before);
}
