use super::*;
use crate::{
    backup::{self, BackupKind},
    database_state::DatabaseState,
};
use std::sync::Barrier;
use tempfile::TempDir;

pub(super) struct Fixture {
    pub(super) state: DatabaseState,
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) _dir: TempDir,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("data");
        let state = DatabaseState::default();
        let path = state.initialize_legacy_startup(&root).unwrap();
        let root = path.parent().unwrap().to_owned();
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES('entry','2026-10-18','Gammal',?1,1,'created','updated')",[r#"{"type":"doc","content":[{"type":"paragraph"}]}"#]).unwrap();
        Self {
            state,
            root,
            path,
            _dir: dir,
        }
    }
    fn candidate(&self) -> PreparedRestore {
        let archive = self
            .root
            .join(format!("source-{}.mno-backup", uuid::Uuid::new_v4()));
        backup::create_backup(
            &Connection::open(&self.path).unwrap(),
            &archive,
            BackupKind::Manual,
        )
        .unwrap();
        backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap()
    }
    fn run<T>(&self, action: impl for<'a, 's> FnOnce(RestoreContext<'a, 's>) -> Result<T>) -> T {
        with_active(
            &self.state,
            &self.path,
            self.candidate(),
            Arc::new(AtomicBool::new(false)),
            action,
        )
        .unwrap()
    }
}
fn valid_bytes() -> Vec<u8> {
    let f = Fixture::new();
    f.run(|mut ctx| {
        let auth = ctx.authorize()?;
        let marker = auth.publish_pending().unwrap();
        Ok(marker.bytes.clone())
    })
}
#[test]
fn strict_format_rejects_invalid_fields_paths_versions_and_identifiers() {
    let bytes = valid_bytes();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(parse(&bytes).is_ok());
    for (key, value) in [
        ("format", serde_json::json!("other")),
        ("version", serde_json::json!(2)),
        ("state", serde_json::json!("done")),
        ("restore_id", serde_json::json!("invalid")),
        (
            "restore_id",
            serde_json::json!("a29aa4d2-7298-4a75-07e3-6c824e64fbe8"),
        ),
        (
            "restore_id",
            serde_json::json!("00000000-0000-0000-0000-000000000000"),
        ),
        ("database_path", serde_json::json!("/mno.sqlite")),
        ("database_path", serde_json::json!("../mno.sqlite")),
        ("database_path", serde_json::json!("C:\\mno.sqlite")),
        ("database_path", serde_json::json!("other.sqlite")),
        (
            "protection_path",
            serde_json::json!("backups/pre-restore/../other"),
        ),
        ("protection_path", serde_json::json!("/external.mno-backup")),
        (
            "protection_path",
            serde_json::json!("backups/pre-restore/unexpected.mno-backup"),
        ),
        ("protection_sha256", serde_json::json!("a")),
        ("protection_sha256", serde_json::json!("A".repeat(64))),
        (
            "old_content",
            serde_json::json!({"version":2,"sha256":"a".repeat(64)}),
        ),
        (
            "new_content",
            serde_json::json!({"version":1,"sha256":"z".repeat(64)}),
        ),
        (
            "database_identity",
            serde_json::json!({"platform":"unix","volume":"12","file":"0000000000000001"}),
        ),
        (
            "directory_identity",
            serde_json::json!({"platform":"unknown","volume":"0000000000000001","file":"0000000000000001"}),
        ),
        ("schema_version", serde_json::json!(1)),
        ("created_at", serde_json::json!("yesterday")),
    ] {
        let mut changed = original.clone();
        changed[key] = value;
        assert!(
            parse(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{key}"
        );
    }
    for key in original.as_object().unwrap().keys() {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(
            parse(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "missing {key}"
        );
    }
    for field in [None, Some("database_identity"), Some("old_content")] {
        let mut changed = original.clone();
        let target = if let Some(field) = field {
            &mut changed[field]
        } else {
            &mut changed
        };
        target["unexpected"] = serde_json::json!(true);
        assert!(parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    for replaced in [
        String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"version\":1",
            "\"version\":1,\"version\":1",
            1,
        ),
        String::from_utf8(bytes).unwrap().replacen(
            "\"platform\":\"unix\"",
            "\"platform\":\"unix\",\"platform\":\"unix\"",
            1,
        ),
    ] {
        assert!(parse(replaced.as_bytes()).is_err());
    }
    assert!(matches!(
        parse(&vec![b' '; LIMIT as usize + 1]),
        Err(MarkerError::TooLarge)
    ));
}
#[test]
fn durable_publication_and_finish_preserve_journal_and_protection_and_keep_latched() {
    let f = Fixture::new();
    let original = fs::read(&f.path).unwrap();
    f.run(|mut ctx| {
        let auth = ctx.authorize()?;
        let protection = fs::read(auth.protection_path()).unwrap();
        let marker = auth.publish_pending().unwrap();
        let data = read(&f.root).unwrap();
        assert_eq!(
            data.restore_id,
            auth.context.maintenance.operation_id().to_string()
        );
        assert_eq!(data.old_content, data.new_content);
        assert_eq!(
            data.protection_sha256,
            digest_file(auth.protection_path()).unwrap()
        );
        assert!(publish_with(&auth, |_| Ok(())).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(f.root.join(DIRECTORY))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                marker.marker_file.metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        marker.finish().unwrap();
        assert!(f.root.join(DIRECTORY).is_dir());
        assert!(!f.root.join(DIRECTORY).join(NAME).exists());
        assert_eq!(fs::read(auth.protection_path()).unwrap(), protection);
        Ok(())
    });
    assert_eq!(fs::read(&f.path).unwrap(), original);
    assert!(f.state.operation(None).is_err());
}
#[test]
fn every_publication_barrier_error_fails_closed_and_preserves_published_marker() {
    for fault in [
        Point::FileSync,
        Point::Published,
        Point::DirectorySync,
        Point::FinalSync,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        f.run(|mut ctx| {
            let auth = ctx.authorize()?;
            let protection = fs::read(auth.protection_path()).unwrap();
            assert!(
                publish_with(&auth, |point| if point == fault {
                    Err(io::ErrorKind::StorageFull.into())
                } else {
                    Ok(())
                })
                .is_err()
            );
            let path = f.root.join(DIRECTORY).join(NAME);
            assert_eq!(path.exists(), fault != Point::FileSync);
            if path.exists() {
                assert!(read(&f.root).is_ok());
            }
            assert_eq!(fs::read(auth.protection_path()).unwrap(), protection);
            assert!(ctx.maintenance.ensure_usable().is_err());
            Ok(())
        });
        assert_eq!(fs::read(&f.path).unwrap(), before);
        drop(f.state);
        assert!(
            DatabaseState::default()
                .initialize_legacy_startup(&f.root)
                .is_err()
        );
    }
}
#[test]
fn publication_barriers_are_ordered_after_exact_content_before_acknowledgement() {
    let f = Fixture::new();
    f.run(|mut ctx| {
        let auth = ctx.authorize()?;
        let mut events = Vec::new();
        let marker = publish_with(&auth, |point| {
            events.push(point);
            let path = f.root.join(DIRECTORY).join(NAME);
            if point == Point::FileSync {
                assert!(!path.exists());
            }
            if point == Point::Published {
                assert!(read(&f.root).is_ok());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(events[0], Point::FileSync);
        assert_eq!(events[1], Point::Published);
        assert!(
            events[2..events.len() - 1]
                .iter()
                .all(|p| *p == Point::DirectorySync)
        );
        assert_eq!(
            events
                .iter()
                .filter(|p| **p == Point::DirectorySync)
                .count(),
            f.root.join(DIRECTORY).ancestors().count()
        );
        assert_eq!(events.last(), Some(&Point::FinalSync));
        drop(marker);
        assert!(read(&f.root).is_ok());
        Ok(())
    });
}
#[test]
fn finish_rejects_wrong_operation_or_changed_marker_and_never_deletes_unknown_files() {
    for fault in 0..4 {
        let f = Fixture::new();
        f.run(|mut ctx| {
            let auth = ctx.authorize()?;
            let mut marker = auth.publish_pending().unwrap();
            let path = f.root.join(DIRECTORY).join(NAME);
            match fault {
                0 => marker.operation = uuid::Uuid::new_v4(),
                1 => {
                    let mut value: serde_json::Value =
                        serde_json::from_slice(&marker.bytes).unwrap();
                    value["created_at"] = serde_json::json!("2026-10-18T00:00:00Z");
                    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
                }
                2 => {
                    let bytes = fs::read(&path).unwrap();
                    fs::rename(&path, path.with_extension("old")).unwrap();
                    fs::write(&path, bytes).unwrap();
                }
                _ => {
                    fs::write(f.root.join(DIRECTORY).join("unknown"), b"preserve").unwrap();
                }
            }
            let result = marker.finish();
            assert_eq!(result.is_err(), fault < 3);
            if fault < 3 {
                assert!(path.exists());
            } else {
                assert!(f.root.join(DIRECTORY).join("unknown").exists());
            }
            Ok(())
        });
        assert!(f.state.operation(None).is_err());
    }
}
#[test]
fn removal_errors_never_unlock_and_preserve_prior_protection() {
    for fault in [
        Point::BeforeRemove,
        Point::Removed,
        Point::RemovalSync,
        Point::ParentSync,
        Point::RemovalBarrier,
    ] {
        let f = Fixture::new();
        let original = fs::read(&f.path).unwrap();
        f.run(|mut ctx| {
            let auth = ctx.authorize()?;
            let prior = fs::read(auth.protection_path()).unwrap();
            let marker = auth.publish_pending().unwrap();
            assert!(
                marker
                    .finish_with(|point| if point == fault {
                        Err(io::ErrorKind::PermissionDenied.into())
                    } else {
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(fs::read(auth.protection_path()).unwrap(), prior);
            assert!(ctx.maintenance.ensure_usable().is_err());
            Ok(())
        });
        assert!(f.state.operation(None).is_err());
        assert_eq!(fs::read(&f.path).unwrap(), original);
    }
}
#[test]
fn existing_recovery_artifacts_are_never_overwritten_or_followed() {
    for kind in 0..3 {
        let f = Fixture::new();
        let external = TempDir::new().unwrap();
        fs::write(external.path().join(NAME), b"external").unwrap();
        let recovery = f.root.join(DIRECTORY);
        match kind {
            0 => fs::create_dir(&recovery).unwrap(),
            1 => fs::write(&recovery, b"existing").unwrap(),
            _ => std::os::unix::fs::symlink(external.path(), &recovery).unwrap(),
        }
        f.run(|mut ctx| {
            let auth = ctx.authorize()?;
            assert!(matches!(
                auth.publish_pending(),
                Err(MarkerError::AlreadyExists)
            ));
            Ok(())
        });
        assert_eq!(fs::read(external.path().join(NAME)).unwrap(), b"external");
        assert!(read(&f.root).is_err());
    }
}
#[test]
fn simultaneous_publishers_have_exactly_one_winner_without_overwrite() {
    let f = Fixture::new();
    // Two independent test-only coordinators deliberately bypass the real process
    // lock to challenge filesystem exclusivity. Production shares one coordinator.
    let barrier = Arc::new(Barrier::new(2));
    let path = f.path.clone();
    let mut workers = Vec::new();
    for _ in 0..2 {
        let candidate = f.candidate();
        let path = path.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let state = DatabaseState::ready_for_test();
            with_active(
                &state,
                &path,
                candidate,
                Arc::new(AtomicBool::new(false)),
                |mut ctx| {
                    let auth = ctx.authorize()?;
                    barrier.wait();
                    match auth.publish_pending() {
                        Ok(marker) => Ok(Some(marker.bytes.clone())),
                        Err(MarkerError::AlreadyExists) => Ok(None),
                        Err(e) => panic!("unexpected {e:?}"),
                    }
                },
            )
            .unwrap()
        }));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1);
    let winner = results.into_iter().flatten().next().unwrap();
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), winner);
    assert!(read(&f.root).is_ok());
}
#[test]
fn reader_is_bounded_and_rejects_symlinks_and_malformed_files() {
    let f = Fixture::new();
    fs::create_dir(f.root.join(DIRECTORY)).unwrap();
    let path = f.root.join(DIRECTORY).join(NAME);
    fs::write(&path, vec![b' '; LIMIT as usize + 1]).unwrap();
    assert!(matches!(read(&f.root), Err(MarkerError::TooLarge)));
    fs::remove_file(&path).unwrap();
    let external = f.root.join("external");
    fs::write(&external, valid_bytes()).unwrap();
    std::os::unix::fs::symlink(&external, &path).unwrap();
    assert!(read(&f.root).is_err());
}

#[test]
fn each_directory_barrier_failure_keeps_complete_marker_and_blocks_restart() {
    let count = Fixture::new().root.join(DIRECTORY).ancestors().count();
    for failing in 0..count {
        let f = Fixture::new();
        f.run(|mut ctx| {
            let auth = ctx.authorize()?;
            let mut seen = 0;
            let result = publish_with(&auth, |point| {
                if point == Point::DirectorySync {
                    let current = seen;
                    seen += 1;
                    if current == failing {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                }
                Ok(())
            });
            assert!(result.is_err());
            assert!(read(&f.root).is_ok());
            assert!(ctx.maintenance.ensure_usable().is_err());
            Ok(())
        });
        drop(f.state);
        assert!(
            DatabaseState::default()
                .initialize_legacy_startup(&f.root)
                .is_err()
        );
    }
}

#[test]
fn successful_finish_still_leaves_startup_blocked() {
    let f = Fixture::new();
    f.run(|mut ctx| {
        let auth = ctx.authorize()?;
        let marker = auth.publish_pending().unwrap();
        marker.finish().unwrap();
        assert!(f.root.join(DIRECTORY).is_dir());
        assert!(!f.root.join(DIRECTORY).join(NAME).exists());
        Ok(())
    });
    drop(f.state);
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    assert!(state.operation(None).is_err());
}

// The entire ownership experiment is unreachable in non-test builds.
#[cfg(target_os = "macos")]
#[path = "tests/pending_write_prototype.rs"]
mod pending_write_prototype;

// Frozen pre-separation wire fixture, independent of protocol constants and producers.
pub(super) const PENDING_V1_FIXTURE: &[u8] = br#"{"format":"mno-pending-restore","version":1,"restore_id":"00000000-0000-4000-8000-000000000001","state":"pending","database_path":"mno.sqlite","database_identity":{"platform":"unix","volume":"0000000000000001","file":"0000000000000002"},"directory_identity":{"platform":"unix","volume":"0000000000000001","file":"0000000000000003"},"protection_path":"backups/pre-restore/pre-restore-20261018-101112-00000000-0000-4000-8000-000000000002.mno-backup","protection_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","old_content":{"version":1,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"new_content":{"version":1,"sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},"schema_version":0,"created_at":"2026-10-18T10:11:12Z"}"#;

#[test]
fn versioning_pending_v1_fixture_roundtrips_and_only_accepts_original_combinations() {
    let data = parse(PENDING_V1_FIXTURE).unwrap();
    assert_eq!(serde_json::to_vec(&data).unwrap(), PENDING_V1_FIXTURE);
    let original: serde_json::Value = serde_json::from_slice(PENDING_V1_FIXTURE).unwrap();
    for version in [1, 2] {
        for schema in [0, 1] {
            for old in [1, 2] {
                for new in [1, 2] {
                    let mut changed = original.clone();
                    changed["version"] = version.into();
                    changed["schema_version"] = schema.into();
                    changed["old_content"]["version"] = old.into();
                    changed["new_content"]["version"] = new.into();
                    assert_eq!(
                        parse(&serde_json::to_vec(&changed).unwrap()).is_ok(),
                        version == 1 && schema == 0 && old == 1 && new == 1
                    );
                }
            }
        }
    }
    let produced: serde_json::Value = serde_json::from_slice(&valid_bytes()).unwrap();
    assert_eq!(produced["version"], 1);
    assert_eq!(produced["schema_version"], 0);
    assert_eq!(produced["old_content"]["version"], 1);
    assert_eq!(produced["new_content"]["version"], 1);
}
