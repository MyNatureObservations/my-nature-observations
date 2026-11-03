use super::*;
use crate::{database::initialize_schema, database_state::DatabaseState};
use std::{fs, sync::mpsc};
const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Blåmes 🌿"}]}]}"#;
fn fixture(root: &Path) -> PathBuf {
    let path = root.join("mno.sqlite");
    let db = Connection::open(&path).unwrap();
    initialize_schema(&db).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('id','2026-10-17','Title',?1,1,'created','updated')",
        [DOC],
    )
    .unwrap();
    path
}
fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn files(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join("backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect()
}
#[test]
fn successful_private_protection_is_restorable_and_survives_drop() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let first = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
    let path = first.path().to_owned();
    assert_eq!(
        path.parent().unwrap(),
        root.path().join("backups/pre-restore")
    );
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("pre-restore-")
    );
    assert_eq!(first.candidate().manifest().kind, BackupKind::Manual);
    assert_eq!(
        first
            .candidate()
            .source()
            .unwrap()
            .query_row("SELECT content_json FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        DOC
    );
    assert_eq!(verify_backup(&path).unwrap(), *first.candidate().manifest());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for directory in [
            root.path().join("backups"),
            root.path().join("backups/pre-restore"),
        ] {
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[cfg(windows)]
    {
        restore::private::verify_permissions(path.parent().unwrap(), true).unwrap();
        restore::private::verify_permissions(&path, false).unwrap();
    }
    drop(first);
    assert!(path.exists());
    assert_eq!(files(root.path()), vec![path.clone()]);
    let second = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
    assert_ne!(second.path(), path);
    let original_backup = fs::read(&path).unwrap();
    drop(second);
    assert_eq!(files(root.path()).len(), 2);
    let collision = create_at(
        &maintenance,
        &source,
        cancel(),
        path.file_name().unwrap().to_str().unwrap(),
    );
    assert!(matches!(collision, Err(ProtectionError::AlreadyExists)));
    assert_eq!(fs::read(&path).unwrap(), original_backup);
    assert_eq!(fs::read(&source).unwrap(), original);
}
#[test]
fn incompatible_current_data_never_publishes_a_protection() {
    for (sql, expected) in [
        (
            "UPDATE journal_entries SET document_version=99",
            PrepareError::IncompatibleDocument,
        ),
        (
            "CREATE TRIGGER unsafe AFTER UPDATE ON journal_entries BEGIN DELETE FROM journal_entries; END",
            PrepareError::IncompatibleSchema,
        ),
    ] {
        let root = TempDir::new().unwrap();
        let source = fixture(root.path());
        Connection::open(&source)
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        let original = fs::read(&source).unwrap();
        let state = DatabaseState::ready_for_test();
        let maintenance = state.maintenance().unwrap();
        assert!(
            matches!(create_at(&maintenance, &source, cancel(), &name().unwrap()), Err(ProtectionError::Verification(error)) if error == expected)
        );
        assert!(files(root.path()).is_empty());
        assert_eq!(fs::read(source).unwrap(), original);
    }
}
#[test]
fn cancellation_at_verification_cleans_only_new_temporary_files() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let first = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
    let path = first.path().to_owned();
    let original = fs::read(&source).unwrap();
    drop(first);
    let flag = cancel();
    let trigger = flag.clone();
    let result = create_with_check(&maintenance, &source, flag, &name().unwrap(), move |_| {
        trigger.store(true, Ordering::Release);
        Ok(())
    });
    assert!(matches!(
        result,
        Err(ProtectionError::Verification(PrepareError::Cancelled))
    ));
    assert_eq!(files(root.path()), vec![path.clone()]);
    verify_backup(&path).unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
    let flag = cancel();
    flag.store(true, Ordering::Release);
    assert!(matches!(
        create_at(&maintenance, &source, flag, &name().unwrap()),
        Err(ProtectionError::Cancelled)
    ));
}
#[test]
fn inaccessible_managed_path_has_no_fallback_or_source_change() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    fs::write(root.path().join("backups"), b"existing file").unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    assert!(matches!(
        create_at(&maintenance, &source, cancel(), &name().unwrap()),
        Err(ProtectionError::PermissionDenied)
    ));
    assert_eq!(
        fs::read(root.path().join("backups")).unwrap(),
        b"existing file"
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(matches!(
        create_at(
            &maintenance,
            &root.path().join("missing.sqlite"),
            cancel(),
            &name().unwrap()
        ),
        Err(ProtectionError::SourceUnavailable)
    ));
    assert!(!root.path().join("missing.sqlite").exists());
}
#[cfg(unix)]
#[test]
fn linked_directory_is_rejected_and_external_files_untouched() {
    let root = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    fs::write(external.path().join("original.jpg"), b"external original").unwrap();
    std::os::unix::fs::symlink(external.path(), root.path().join("backups")).unwrap();
    let source = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    assert!(matches!(
        create_at(&maintenance, &source, cancel(), &name().unwrap()),
        Err(ProtectionError::PermissionDenied)
    ));
    assert_eq!(
        fs::read(external.path().join("original.jpg")).unwrap(),
        b"external original"
    );
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 1);
}
#[test]
fn maintenance_covers_protection_and_future_generation_change_without_a_gap() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let state = Arc::new(DatabaseState::ready_for_test());
    // A normal save completed before maintenance is part of the protected state.
    {
        let _operation = state.operation(Some(1)).unwrap();
        Connection::open(&source)
            .unwrap()
            .execute("UPDATE journal_entries SET title='Latest saved'", [])
            .unwrap();
    }
    let mut maintenance = state.maintenance().unwrap();
    let (tx, rx) = mpsc::channel();
    state.on_blocked(move || tx.send(()).unwrap());
    let other = state.clone();
    let worker = std::thread::spawn(move || other.operation(Some(1)).err().unwrap());
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let proof = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
    assert_eq!(
        proof
            .candidate()
            .source()
            .unwrap()
            .query_row("SELECT title FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Latest saved"
    );
    assert!(!worker.is_finished());
    let saved = proof.path().to_owned();
    drop(proof);
    // Future 3A.3b replacement belongs HERE, still inside the same maintenance guard.
    maintenance.advance_generation().unwrap();
    drop(maintenance);
    assert_eq!(
        worker.join().unwrap(),
        crate::database_state::GENERATION_CONFLICT
    );
    assert!(saved.exists());
}
#[test]
fn io_failures_have_distinct_codes() {
    assert_eq!(
        io_failure(io::Error::from(io::ErrorKind::StorageFull)),
        ProtectionError::InsufficientSpace
    );
    assert_eq!(
        io_failure(io::Error::from(io::ErrorKind::QuotaExceeded)),
        ProtectionError::InsufficientSpace
    );
    assert_eq!(
        io_failure(io::Error::from(io::ErrorKind::PermissionDenied)),
        ProtectionError::PermissionDenied
    );
    assert_eq!(
        io_failure(io::Error::from(io::ErrorKind::AlreadyExists)),
        ProtectionError::AlreadyExists
    );
}

#[test]
fn late_creation_and_corrupt_archive_errors_do_not_publish_or_remove_prior_backups() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let first = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
    let saved = first.path().to_owned();
    drop(first);
    for disk_full in [true, false] {
        let result = create_with_check(
            &maintenance,
            &source,
            cancel(),
            &name().unwrap(),
            |archive| {
                if disk_full {
                    return Err(io::Error::from(io::ErrorKind::StorageFull).into());
                }
                fs::write(archive, b"broken archive")?;
                Ok(())
            },
        );
        if disk_full {
            assert!(matches!(result, Err(ProtectionError::InsufficientSpace)));
        } else {
            assert!(matches!(
                result,
                Err(ProtectionError::Verification(PrepareError::DamagedArchive))
            ));
        }
        assert_eq!(files(root.path()), vec![saved.clone()]);
        assert_eq!(fs::read(&source).unwrap(), original);
        verify_backup(&saved).unwrap();
    }
}
#[test]
fn collision_created_at_publication_boundary_is_never_overwritten() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let filename = name().unwrap();
    let target = root.path().join("backups/pre-restore").join(&filename);
    let result = create_with_check(&maintenance, &source, cancel(), &filename, |_| {
        fs::write(&target, b"other creator won")?;
        Ok(())
    });
    assert!(matches!(result, Err(ProtectionError::AlreadyExists)));
    assert_eq!(fs::read(&target).unwrap(), b"other creator won");
    assert_eq!(files(root.path()), vec![target]);
}
#[test]
fn oversized_current_document_is_resource_limited_and_not_published() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let large = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"{}"}}]}}]}}"#,
        "a".repeat(64 * 1024 * 1024)
    );
    Connection::open(&source)
        .unwrap()
        .execute("UPDATE journal_entries SET content_json=?1", [&large])
        .unwrap();
    drop(large);
    let original = digest_file(&source).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    assert!(matches!(
        create_at(&maintenance, &source, cancel(), &name().unwrap()),
        Err(ProtectionError::Verification(PrepareError::ResourceLimit))
    ));
    assert!(files(root.path()).is_empty());
    assert_eq!(digest_file(&source).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn intermediate_snapshot_and_archive_are_private_before_publication() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let directory = root.path().join("backups/pre-restore");
    fs::create_dir_all(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let proof = create_with_check(
        &maintenance,
        &source,
        cancel(),
        &name().unwrap(),
        |archive| {
            assert_eq!(
                fs::metadata(&directory)?.permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(fs::metadata(archive)?.permissions().mode() & 0o777, 0o600);
            let stages: Vec<_> = fs::read_dir(&directory)?
                .map(|e| e.unwrap().path())
                .filter(|p| p.is_dir())
                .collect();
            assert_eq!(stages.len(), 1);
            assert_eq!(
                fs::metadata(&stages[0])?.permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(stages[0].join(DATABASE))?.permissions().mode() & 0o777,
                0o600
            );
            Ok(())
        },
    )
    .unwrap();
    let path = proof.path().to_owned();
    drop(proof);
    assert_eq!(files(root.path()), vec![path]);
}

#[test]
fn verified_bytes_are_synced_before_publication_and_directory_chain_before_success() {
    use durability::SyncPoint::*;
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let filename = name().unwrap();
    let backups = root.path().join("backups");
    let directory = backups.join("pre-restore");
    let target = directory.join(&filename);
    assert!(!backups.exists());
    let mut events = Vec::new();
    let proof = create_with_sync(
        &maintenance,
        &source,
        cancel(),
        &filename,
        |_| Ok(()),
        |point, path| {
            if point == VerifiedFile {
                assert!(!target.exists());
                // Check real archived content, not just an event label.
                let candidate = restore::prepare_candidate(path, cancel()).unwrap();
                assert_eq!(
                    candidate
                        .source()
                        .unwrap()
                        .query_row("SELECT content_json FROM journal_entries", [], |row| row
                            .get::<_, String>(
                            0
                        ))
                        .unwrap(),
                    DOC
                );
            } else {
                assert!(target.is_file());
                verify_backup(&target).unwrap();
            }
            durability::sync(point, path)?;
            events.push((point, path.to_owned()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(events.len(), 6);
    assert_eq!(events[0].0, VerifiedFile);
    assert_eq!(
        &events[1..],
        &[
            (Directory, directory),
            (Directory, backups),
            (Directory, root.path().to_owned()),
            (Directory, root.path().parent().unwrap().to_owned()),
            (FinalFileBarrier, target.clone()),
        ]
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    drop(proof);
    assert_eq!(files(root.path()), vec![target]);
}
#[test]
fn each_required_sync_failure_rejects_success_and_preserves_published_and_older_backups() {
    for fail_at in 0..6 {
        let root = TempDir::new().unwrap();
        let source = fixture(root.path());
        let original = fs::read(&source).unwrap();
        let state = DatabaseState::ready_for_test();
        let maintenance = state.maintenance().unwrap();
        let older = create_at(&maintenance, &source, cancel(), &name().unwrap()).unwrap();
        let old_path = older.path().to_owned();
        let old_bytes = fs::read(&old_path).unwrap();
        drop(older);
        let filename = name().unwrap();
        let target = root.path().join("backups/pre-restore").join(&filename);
        let mut index = 0;
        let result = create_with_sync(
            &maintenance,
            &source,
            cancel(),
            &filename,
            |_| Ok(()),
            |point, path| {
                let current = index;
                index += 1;
                if current == fail_at {
                    return Err(io::Error::other("controlled required sync failure"));
                }
                durability::sync(point, path)
            },
        );
        assert!(
            matches!(result, Err(ProtectionError::DurabilityFailed)),
            "sync {fail_at}"
        );
        assert_eq!(index, fail_at + 1);
        assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
        assert_eq!(fs::read(&source).unwrap(), original);
        if fail_at == 0 {
            assert!(!target.exists());
            assert_eq!(files(root.path()), vec![old_path]);
        } else {
            // A late error must not delete the complete, restorable published protection.
            assert!(target.is_file());
            let candidate = restore::prepare_candidate(&target, cancel()).unwrap();
            assert_eq!(
                candidate
                    .source()
                    .unwrap()
                    .query_row("SELECT content_json FROM journal_entries", [], |row| row
                        .get::<_, String>(
                        0
                    ))
                    .unwrap(),
                DOC
            );
            assert_eq!(files(root.path()).len(), 2);
            assert!(files(root.path()).iter().all(|p| p.is_file()));
        }
    }
}
#[test]
fn engine_early_collision_is_typed_and_maps_to_already_exists() {
    let root = TempDir::new().unwrap();
    let source_path = fixture(root.path());
    let original = fs::read(&source_path).unwrap();
    let source =
        Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let target = root.path().join("occupied.mno-backup");
    create_backup(&source, &target, BackupKind::Manual).unwrap();
    let previous = fs::read(&target).unwrap();
    let error = create_backup(&source, &target, BackupKind::Manual).unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(backup_failure(error), ProtectionError::AlreadyExists);
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert_eq!(fs::read(&source_path).unwrap(), original);
}

#[test]
fn retry_syncs_existing_directories_after_an_earlier_failed_publication_barrier() {
    let root = TempDir::new().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    let state = DatabaseState::ready_for_test();
    let maintenance = state.maintenance().unwrap();
    let failed_name = name().unwrap();
    let result = create_with_sync(
        &maintenance,
        &source,
        cancel(),
        &failed_name,
        |_| Ok(()),
        |point, path| {
            if point == durability::SyncPoint::Directory {
                return Err(io::Error::other("directory flush failed"));
            }
            durability::sync(point, path)
        },
    );
    assert!(matches!(result, Err(ProtectionError::DurabilityFailed)));
    let previous = root.path().join("backups/pre-restore").join(failed_name);
    let previous_bytes = fs::read(&previous).unwrap();
    let mut directories = Vec::new();
    let proof = create_with_sync(
        &maintenance,
        &source,
        cancel(),
        &name().unwrap(),
        |_| Ok(()),
        |point, path| {
            durability::sync(point, path)?;
            if point == durability::SyncPoint::Directory {
                directories.push(path.to_owned());
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        directories,
        vec![
            root.path().join("backups/pre-restore"),
            root.path().join("backups"),
            root.path().to_owned(),
            root.path().parent().unwrap().to_owned()
        ]
    );
    assert_eq!(fs::read(previous).unwrap(), previous_bytes);
    assert_eq!(fs::read(source).unwrap(), original);
    drop(proof);
    assert_eq!(files(root.path()).len(), 2);
}
