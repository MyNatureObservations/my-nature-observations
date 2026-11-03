use super::*;
use crate::database;
use rusqlite::params;
use std::{fs, path::PathBuf};
const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Blåmes 🌿"}]}]}"#;
fn fixture(dir: &Path, change: impl FnOnce(&Connection)) -> (PathBuf, PathBuf) {
    let source = dir.join("source.sqlite");
    let connection = Connection::open(&source).unwrap();
    database::initialize_schema(&connection).unwrap();
    connection.execute("INSERT INTO journal_entries VALUES ('entry','2026-10-17','Blåmes',?1,1,'created','updated')", [DOC]).unwrap();
    change(&connection);
    drop(connection);
    let bytes = fs::read(&source).unwrap();
    let manifest = BackupManifest {
        format: FORMAT.into(),
        format_version: 1,
        backup_id: uuid::Uuid::new_v4().to_string(),
        created_at: "2026-10-17T10:00:00Z".into(),
        app_version: "0.1.0".into(),
        database_schema_version: 0,
        book_id: None,
        kind: BackupKind::Manual,
        database_file: DATABASE.into(),
        database_size: bytes.len() as u64,
        database_sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    let archive = dir.join("selected.mno-backup");
    package(&archive, &manifest, &bytes, None);
    (archive, source)
}
fn package(path: &Path, manifest: &BackupManifest, bytes: &[u8], extra: Option<&str>) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(&serde_json::to_vec(manifest).unwrap())
        .unwrap();
    zip.start_file(DATABASE, options).unwrap();
    zip.write_all(bytes).unwrap();
    if let Some(name) = extra {
        zip.start_file(name, options).unwrap();
        zip.write_all(b"extra").unwrap();
    }
    zip.finish().unwrap();
}
fn rewrite(path: &Path, change: impl FnOnce(&mut BackupManifest, &mut Vec<u8>)) {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut manifest: BackupManifest =
        serde_json::from_reader(zip.by_name(MANIFEST).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name(DATABASE)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    drop(zip);
    change(&mut manifest, &mut bytes);
    package(path, &manifest, &bytes, None);
}
fn prepare(path: &Path, parent: &Path) -> Result<PreparedRestore> {
    prepare_in(path, parent, Arc::new(AtomicBool::new(false)), |p| {
        fs2::available_space(p)
    })
}
fn error(result: Result<PreparedRestore>) -> PrepareError {
    match result {
        Ok(_) => panic!("accepted incompatible candidate"),
        Err(e) => e,
    }
}
#[test]
fn valid_candidate_retains_verified_data_without_original_and_cleans_on_drop() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let (archive, source) = fixture(dir.path(), |_| {});
    let original = fs::read(&archive).unwrap();
    let source_before = fs::read(&source).unwrap();
    let candidate = prepare(&archive, staging.path()).unwrap();
    assert_eq!(candidate.manifest().format_version, 1);
    assert_eq!(fs::read(&archive).unwrap(), original);
    assert_eq!(fs::read(&source).unwrap(), source_before);
    fs::remove_file(&archive).unwrap();
    let value: String = candidate
        .source()
        .unwrap()
        .query_row("SELECT content_json FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(value, DOC);
    assert!(
        candidate
            .source()
            .unwrap()
            .execute("DELETE FROM journal_entries", [])
            .is_err()
    );
    let path = candidate.database_path();
    assert!(path.exists());
    drop(candidate);
    assert!(!path.exists());
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
}
#[test]
fn accepts_real_backup_engine_output() {
    let dir = TempDir::new().unwrap();
    let (_, source) = fixture(dir.path(), |_| {});
    let connection = Connection::open(source).unwrap();
    let archive = dir.path().join("real.mno-backup");
    create_backup(&connection, &archive, BackupKind::Manual).unwrap();
    prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
}
#[test]
fn archive_manifest_hash_and_size_failures_are_distinct_and_clean() {
    for case in 0..6 {
        let dir = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let (archive, source) = fixture(dir.path(), |_| {});
        let source_before = fs::read(&source).unwrap();
        let expected = match case {
            0 => {
                fs::write(&archive, b"broken ZIP").unwrap();
                PrepareError::DamagedArchive
            }
            1 => {
                rewrite(&archive, |m, _| m.database_sha256 = "0".repeat(64));
                PrepareError::DamagedArchive
            }
            2 => {
                rewrite(&archive, |m, _| m.format_version = 99);
                PrepareError::UnsupportedFormat
            }
            3 => {
                rewrite(&archive, |m, _| m.database_schema_version = 99);
                PrepareError::IncompatibleSchema
            }
            4 => {
                rewrite(&archive, |m, _| m.database_size += 1);
                PrepareError::DamagedArchive
            }
            _ => {
                rewrite(&archive, |m, b| {
                    b[0] = 0;
                    m.database_sha256 = format!("{:x}", Sha256::digest(&*b));
                });
                PrepareError::InvalidDatabase
            }
        };
        let before = fs::read(&archive).unwrap();
        assert_eq!(
            error(prepare(&archive, staging.path())),
            expected,
            "case {case}"
        );
        assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
        assert_eq!(fs::read(&archive).unwrap(), before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
    }
}
#[test]
fn schema_constraints_indexes_versions_and_extra_objects_are_rejected() {
    let changes = [
        "PRAGMA user_version=2",
        "DROP INDEX idx_journal_entries_date",
        "CREATE TRIGGER reject BEFORE UPDATE ON journal_entries BEGIN SELECT RAISE(ABORT,'no'); END",
        "CREATE VIEW extra AS SELECT * FROM journal_entries",
        "CREATE TABLE extra(x)",
        "ALTER TABLE journal_entries ADD COLUMN future TEXT",
        "CREATE UNIQUE INDEX extra_index ON journal_entries(title)",
        "ALTER TABLE journal_entries RENAME TO old; CREATE TABLE journal_entries AS SELECT * FROM old; DROP TABLE old",
    ];
    for sql in changes {
        let dir = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |db| db.execute_batch(sql).unwrap());
        assert_eq!(
            error(prepare(&archive, staging.path())),
            PrepareError::IncompatibleSchema,
            "{sql}"
        );
        assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
    }
}
#[test]
fn invalid_rows_and_document_versions_are_rejected() {
    for (sql, expected) in [
        (
            "UPDATE journal_entries SET document_version=2",
            PrepareError::IncompatibleDocument,
        ),
        (
            "UPDATE journal_entries SET entry_date='2026-02-30'",
            PrepareError::InvalidDatabase,
        ),
        (
            "UPDATE journal_entries SET id=NULL",
            PrepareError::InvalidDatabase,
        ),
        (
            "UPDATE journal_entries SET title=x'00'",
            PrepareError::InvalidDatabase,
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |db| db.execute_batch(sql).unwrap());
        assert_eq!(error(prepare(&archive, dir.path())), expected);
    }
}
#[test]
fn malformed_unknown_and_lossy_documents_are_rejected() {
    for doc in [
        "not json",
        "{}",
        "[]",
        r#"{"type":"doc","content":[{"type":"futureImage"}]}"#,
        r#"{"type":"doc","content":[{"type":"text","text":"invalid placement"}]}"#,
        r#"{"type":"doc","content":[{"type":"paragraph","future":1}]}"#,
        r#"{"type":"doc","type":"doc","content":[{"type":"paragraph"}]}"#,
        r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"code"},{"type":"bold"}]}]}]}"#,
    ] {
        let dir = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |db| {
            db.execute("UPDATE journal_entries SET content_json=?1", [doc])
                .unwrap();
        });
        assert_eq!(
            error(prepare(&archive, dir.path())),
            PrepareError::IncompatibleDocument,
            "{doc}"
        );
    }
}
#[test]
fn resource_limits_and_disk_space_are_deterministic() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    assert_eq!(
        error(prepare_in(
            &archive,
            staging.path(),
            Arc::new(AtomicBool::new(false)),
            |_| Ok(0)
        )),
        PrepareError::InsufficientSpace
    );
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        error(prepare_in(
            &archive,
            staging.path(),
            Arc::new(AtomicBool::new(false)),
            |_| {
                let c = calls.get();
                calls.set(c + 1);
                Ok(if c == 0 { u64::MAX } else { 0 })
            }
        )),
        PrepareError::InsufficientSpace
    );
    assert_eq!(
        error(prepare_in(
            &archive,
            staging.path(),
            Arc::new(AtomicBool::new(true)),
            |_| Ok(u64::MAX)
        )),
        PrepareError::Cancelled
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let next = cancel.clone();
    assert_eq!(
        error(prepare_in(&archive, staging.path(), cancel, move |_| {
            next.store(true, Ordering::Release);
            Ok(u64::MAX)
        })),
        PrepareError::Cancelled
    );
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
    let deep = format!("{}0{}", "[".repeat(70), "]".repeat(70));
    assert_eq!(document::validate(&deep), Err(PrepareError::ResourceLimit));
    let too_many = format!("[{}]", "0,".repeat(1_000_001) + "0");
    assert_eq!(
        document::validate(&too_many),
        Err(PrepareError::ResourceLimit)
    );
    rewrite(&archive, |m, _| {
        m.app_version = "x".repeat(MANIFEST_LIMIT as usize)
    });
    assert_eq!(
        error(prepare(&archive, staging.path())),
        PrepareError::ResourceLimit
    );
}
#[test]
fn unexpected_entries_and_oversized_archive_metadata_are_rejected() {
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let mut zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let manifest: BackupManifest = serde_json::from_reader(zip.by_name(MANIFEST).unwrap()).unwrap();
    let mut data = Vec::new();
    zip.by_name(DATABASE)
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    drop(zip);
    package(&archive, &manifest, &data, Some("../external"));
    assert_eq!(
        error(prepare(&archive, dir.path())),
        PrepareError::DamagedArchive
    );
    package(&archive, &manifest, &data, None);
    let bytes = fs::read(&archive).unwrap();
    let mut padded = vec![0; OVERHEAD_LIMIT as usize + 1];
    padded.extend(bytes);
    fs::write(&archive, padded).unwrap();
    assert!(prepare(&archive, dir.path()).is_err());
}
#[test]
fn supported_document_features_preserve_json() {
    let doc = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Title"}]},{"type":"blockquote","content":[{"type":"paragraph"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"bold"},{"type":"italic"}]},{"type":"hardBreak"}]}]}]},{"type":"orderedList","attrs":{"start":3,"type":null},"content":[{"type":"listItem","content":[{"type":"paragraph"}]}]},{"type":"codeBlock","attrs":{"language":null},"content":[{"type":"text","text":"code"}]},{"type":"horizontalRule"},{"type":"paragraph","content":[{"type":"text","text":"link","marks":[{"type":"link","attrs":{"href":"https://example.invalid"}},{"type":"underline"}]}]}]}"#;
    assert_eq!(document::validate(doc), Ok(()));
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |db| {
        db.execute("UPDATE journal_entries SET content_json=?1", params![doc])
            .unwrap();
    });
    let candidate = prepare(&archive, dir.path()).unwrap();
    assert_eq!(
        candidate
            .database
            .as_ref()
            .unwrap()
            .query_row("SELECT content_json FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        doc
    );
}

#[test]
fn duplicate_records_and_manipulated_central_sizes_are_rejected() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let mut zip = ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let manifest: BackupManifest = serde_json::from_reader(zip.by_name(MANIFEST).unwrap()).unwrap();
    let mut data = Vec::new();
    zip.by_name(DATABASE)
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    drop(zip);
    for name in [MANIFEST, DATABASE] {
        let alias = format!("{}X", &name[..name.len() - 1]);
        package(&archive, &manifest, &data, Some(&alias));
        let mut bytes = fs::read(&archive).unwrap();
        let positions: Vec<_> = bytes
            .windows(alias.len())
            .enumerate()
            .filter_map(|(i, v)| (v == alias.as_bytes()).then_some(i))
            .collect();
        assert_eq!(positions.len(), 2);
        for i in positions {
            bytes[i..i + name.len()].copy_from_slice(name.as_bytes());
        }
        fs::write(&archive, bytes).unwrap();
        assert_eq!(
            error(prepare(&archive, staging.path())),
            PrepareError::DamagedArchive
        );
    }
    package(&archive, &manifest, &data, None);
    let mut bytes = fs::read(&archive).unwrap();
    let central: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, v)| (v == b"PK\x01\x02").then_some(i))
        .collect();
    assert_eq!(central.len(), 2);
    for offset in [20, 24] {
        bytes[central[1] + offset..central[1] + offset + 4]
            .copy_from_slice(&(u32::MAX - 100).to_le_bytes());
    }
    fs::write(&archive, bytes).unwrap();
    assert!(prepare(&archive, staging.path()).is_err());
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
}
#[test]
fn header_and_integrity_corruption_are_rejected_even_with_matching_hash() {
    for case in 0..2 {
        let dir = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |_| {});
        rewrite(&archive, |m, b| {
            if case == 0 {
                b[18] = 2;
                b[19] = 2;
            } else {
                // Damage the table b-tree page, keeping the SQLite header and file size intact.
                let page = u16::from_be_bytes([b[16], b[17]]) as usize;
                b[page] = 0xff;
            }
            m.database_sha256 = format!("{:x}", Sha256::digest(&*b));
        });
        assert_eq!(
            error(prepare(&archive, dir.path())),
            PrepareError::InvalidDatabase
        );
    }
}

#[test]
fn copying_cancellation_and_disk_query_errors_clean_partial_staging() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let cancel = Arc::new(AtomicBool::new(false));
    let next = cancel.clone();
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        error(prepare_in(&archive, staging.path(), cancel, move |_| {
            let n = calls.get();
            calls.set(n + 1);
            if n == 1 {
                next.store(true, Ordering::Release);
            }
            Ok(u64::MAX)
        })),
        PrepareError::Cancelled
    );
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
    for kind in [io::ErrorKind::StorageFull, io::ErrorKind::QuotaExceeded] {
        assert_eq!(
            error(prepare_in(
                &archive,
                staging.path(),
                Arc::new(AtomicBool::new(false)),
                |_| Err(io::Error::from(kind))
            )),
            PrepareError::InsufficientSpace
        );
    }
}
#[test]
fn unknown_attributes_marks_and_broken_node_grammars_are_rejected() {
    for content in [
        r#"{"type":"heading","attrs":{"level":7}}"#,
        r#"{"type":"paragraph","attrs":{"future":true}}"#,
        r#"{"type":"paragraph","content":[{"type":"text","text":""}]}"#,
        r#"{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"future"}]}]}"#,
        r#"{"type":"bulletList","content":[{"type":"paragraph"}]}"#,
        r#"{"type":"orderedList","content":[{"type":"listItem","content":[{"type":"heading"}]}]}"#,
        r#"{"type":"codeBlock","content":[{"type":"hardBreak"}]}"#,
        r#"{"type":"paragraph","content":[{"type":"text","text":"x","marks":[{"type":"link","attrs":{"href":42}}]}]}"#,
    ] {
        let doc = format!("{{\"type\":\"doc\",\"content\":[{content}]}}");
        assert_eq!(
            document::validate(&doc),
            Err(PrepareError::IncompatibleDocument),
            "{content}"
        );
    }
}

#[test]
fn identifiers_that_cannot_be_used_as_route_segments_are_rejected() {
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |db| {
        db.execute("UPDATE journal_entries SET id='../other'", [])
            .unwrap();
    });
    assert_eq!(
        error(prepare(&archive, dir.path())),
        PrepareError::InvalidDatabase
    );
}

#[test]
fn sqlite_work_budget_and_cancellation_are_enforced() {
    let db = Connection::open_in_memory().unwrap();
    harden(&db, Arc::new(AtomicBool::new(false))).unwrap();
    let query = "WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n";
    let e = db.query_row(query, [], |r| r.get::<_, i64>(0)).unwrap_err();
    assert_eq!(sql_error(e), PrepareError::ResourceLimit);
    let cancelled = Arc::new(AtomicBool::new(true));
    harden(&db, cancelled.clone()).unwrap();
    assert_eq!(
        validate_database(&db, &cancelled),
        Err(PrepareError::Cancelled)
    );
}

#[cfg(unix)]
#[test]
fn private_permissions_do_not_depend_on_umask() {
    let exe = std::env::current_exe().unwrap();
    for mask in ["000", "022"] {
        let status = std::process::Command::new("sh")
            .args([
                "-c",
                &format!("umask {mask}; exec \"$@\""),
                "permissions-test",
            ])
            .arg(&exe)
            .args([
                "--exact",
                "backup::restore::tests::private_permissions_child",
            ])
            .env("MNO_PRIVATE_PERMISSIONS_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
#[cfg(unix)]
#[test]
fn private_permissions_child() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("MNO_PRIVATE_PERMISSIONS_CHILD").is_none() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let candidate = prepare(&archive, dir.path()).unwrap();
    let path = candidate.database_path();
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
#[test]
fn verified_snapshot_blocks_sql_changes_and_copies_the_verified_content() {
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let original = fs::read(&archive).unwrap();
    let candidate = prepare(&archive, dir.path()).unwrap();
    assert!(!candidate.source().unwrap().is_autocommit());
    if let Ok(writer) = Connection::open(candidate.database_path()) {
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        assert!(
            writer
                .execute("UPDATE journal_entries SET title='changed'", [])
                .is_err()
        );
        assert!(writer.execute_batch("CREATE TRIGGER injected AFTER UPDATE ON journal_entries BEGIN DELETE FROM journal_entries; END;").is_err());
        assert!(writer.execute_batch("PRAGMA journal_mode=WAL").is_err());
    } // Windows may reject opening a write handle before SQLite locking is reached.
    for sql in ["COMMIT", "ROLLBACK", "SAVEPOINT release_snapshot"] {
        assert!(candidate.source().unwrap().execute_batch(sql).is_err());
    }
    let mut copied = Connection::open_in_memory().unwrap();
    let backup = rusqlite::backup::Backup::new(candidate.source().unwrap(), &mut copied).unwrap();
    backup
        .run_to_completion(100, std::time::Duration::ZERO, None)
        .unwrap();
    drop(backup);
    assert_eq!(
        copied
            .query_row("SELECT content_json FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        DOC
    );
    assert_eq!(
        copied
            .query_row("SELECT title FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Blåmes"
    );
    assert_eq!(
        copied
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='trigger'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(fs::read(&archive).unwrap(), original);
}
#[cfg(unix)]
#[test]
fn replacing_or_raw_editing_candidate_is_detected_and_cleaned() {
    use std::io::{Seek, SeekFrom, Write};
    for replace in [true, false] {
        let dir = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |_| {});
        let original = fs::read(&archive).unwrap();
        let candidate = prepare(&archive, dir.path()).unwrap();
        let path = candidate.database_path();
        let parent = path.parent().unwrap().to_owned();
        if replace {
            fs::rename(&path, path.with_extension("old")).unwrap();
            fs::copy(path.with_extension("old"), &path).unwrap();
        } else {
            let mut writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
            writer.seek(SeekFrom::Start(100)).unwrap();
            writer.write_all(&[0xff]).unwrap();
            writer.sync_all().unwrap();
        }
        assert!(matches!(
            candidate.source(),
            Err(PrepareError::CandidateChanged)
        ));
        drop(candidate);
        assert!(!parent.exists());
        assert_eq!(fs::read(&archive).unwrap(), original);
    }
}
#[test]
fn released_snapshot_is_rejected() {
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let candidate = prepare(&archive, dir.path()).unwrap();
    candidate
        .database
        .as_ref()
        .unwrap()
        .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>)
        .unwrap();
    candidate
        .database
        .as_ref()
        .unwrap()
        .execute_batch("ROLLBACK")
        .unwrap();
    assert!(matches!(
        candidate.source(),
        Err(PrepareError::CandidateChanged)
    ));
}
#[test]
fn accepts_exact_large_list_starts_but_rejects_lossy_integer_literals() {
    for value in [
        "9007199254740992",
        "-9007199254740992",
        "1000000000000000100",
        "1e30",
    ] {
        let document = format!(
            r#"{{"type":"doc","content":[{{"type":"orderedList","attrs":{{"start":{value}}},"content":[{{"type":"listItem","content":[{{"type":"paragraph"}}]}}]}}]}}"#
        );
        assert_eq!(document::validate(&document), Ok(()), "{value}");
    }
    for value in ["9007199254740993", "1.5", "1e400", "null", "\"2\""] {
        let document = format!(
            r#"{{"type":"doc","content":[{{"type":"orderedList","attrs":{{"start":{value}}},"content":[{{"type":"listItem","content":[{{"type":"paragraph"}}]}}]}}]}}"#
        );
        assert!(document::validate(&document).is_err(), "{value}");
    }
}

#[cfg(windows)]
#[test]
fn windows_candidate_has_private_acl_and_denies_write_and_delete_handles() {
    let dir = TempDir::new().unwrap();
    let (archive, _) = fixture(dir.path(), |_| {});
    let candidate = prepare(&archive, dir.path()).unwrap();
    let path = candidate.database_path();
    private::verify_permissions(path.parent().unwrap(), true).unwrap();
    private::verify_permissions(&path, false).unwrap();
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::rename(&path, path.with_extension("moved")).is_err());
    assert!(candidate.source().is_ok());
    drop(candidate);
    assert!(!path.exists());
}
#[test]
fn wal_source_backup_becomes_a_standalone_pinned_candidate() {
    let dir = TempDir::new().unwrap();
    let (_, source) = fixture(dir.path(), |_| {});
    let connection = Connection::open(&source).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL; UPDATE journal_entries SET title='WAL version';")
        .unwrap();
    let archive = dir.path().join("wal-source.mno-backup");
    create_backup(&connection, &archive, BackupKind::Manual).unwrap();
    let candidate = prepare(&archive, dir.path()).unwrap();
    assert_eq!(
        candidate
            .source()
            .unwrap()
            .query_row("SELECT title FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "WAL version"
    );
    assert!(
        !candidate
            .database_path()
            .with_extension("sqlite-wal")
            .exists()
    );
}

#[test]
fn valid_document_over_64_mib_is_a_resource_limit_not_corruption() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let large = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"{}"}}]}}]}}"#,
        "a".repeat(DOCUMENT_LIMIT as usize)
    );
    assert_eq!(document::validate(&large), Err(PrepareError::ResourceLimit));
    let (archive, source) = fixture(dir.path(), |connection| {
        connection
            .execute("UPDATE journal_entries SET content_json=?1", [&large])
            .unwrap();
    });
    let original = digest_file(&archive).unwrap();
    let source_hash = digest_file(&source).unwrap();
    assert_eq!(
        error(prepare(&archive, staging.path())),
        PrepareError::ResourceLimit
    );
    assert_eq!(digest_file(&archive).unwrap(), original);
    assert_eq!(digest_file(&source).unwrap(), source_hash);
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[test]
fn retained_file_check_preserves_offset_owner_and_staging_until_drop() {
    use std::io::{Seek, SeekFrom};
    let directory = tempfile::TempDir::new().unwrap();
    let (archive, _) = fixture(directory.path(), |_| {});
    let mut closed = prepare(&archive, directory.path())
        .unwrap()
        .try_close()
        .unwrap_or_else(|failure| panic!("explicit close failed: {}", failure.error));
    let staging = closed._staging.path().to_owned();
    let identity = private::file_identity(&closed._file).unwrap();
    closed._file.seek(SeekFrom::Start(17)).unwrap();
    closed.check_retained_file().unwrap();
    closed.check_retained_file().unwrap();
    assert_eq!(closed._file.stream_position().unwrap(), 17);
    assert_eq!(private::file_identity(&closed._file).unwrap(), identity);
    assert!(staging.is_dir());
    drop(closed);
    assert!(!staging.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn retained_file_check_rejects_changed_name_type_and_identity_without_losing_owner() {
    use std::os::unix::fs::symlink;
    for case in 0..4 {
        let directory = tempfile::TempDir::new().unwrap();
        let (archive, _) = fixture(directory.path(), |_| {});
        let closed = prepare(&archive, directory.path())
            .unwrap()
            .try_close()
            .unwrap_or_else(|failure| panic!("explicit close failed: {}", failure.error));
        let path = closed._staging.path().join(DATABASE);
        let moved = closed._staging.path().join("original");
        let identity = private::file_identity(&closed._file).unwrap();
        fs::rename(&path, &moved).unwrap();
        match case {
            0 => {
                fs::copy(&moved, &path).unwrap();
            }
            1 => symlink(&moved, &path).unwrap(),
            2 => fs::create_dir(&path).unwrap(),
            3 => {} // Missing name / metadata error.
            _ => unreachable!(),
        }
        assert_eq!(
            closed.check_retained_file(),
            Err(PrepareError::CandidateChanged)
        );
        assert_eq!(private::file_identity(&closed._file).unwrap(), identity);
        assert!(closed._staging.path().is_dir());
        match case {
            2 => fs::remove_dir(&path).unwrap(),
            3 => {}
            _ => fs::remove_file(&path).unwrap(),
        }
        fs::rename(&moved, &path).unwrap();
        closed.check_retained_file().unwrap();
        let staging = closed._staging.path().to_owned();
        drop(closed);
        assert!(!staging.exists());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn retained_file_check_rejects_size_hash_and_link_count_changes() {
    use std::os::unix::fs::FileExt;
    for case in 0..3 {
        let directory = tempfile::TempDir::new().unwrap();
        let (archive, _) = fixture(directory.path(), |_| {});
        let closed = prepare(&archive, directory.path())
            .unwrap()
            .try_close()
            .unwrap_or_else(|failure| panic!("explicit close failed: {}", failure.error));
        let path = closed._staging.path().join(DATABASE);
        let original = fs::read(&path).unwrap();
        let identity = private::file_identity(&closed._file).unwrap();
        let alias = closed._staging.path().join("alias");
        match case {
            0 => fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(1)
                .unwrap(),
            1 => {
                let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
                file.write_all_at(&[original[0] ^ 1], 0).unwrap();
            }
            2 => fs::hard_link(&path, &alias).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(
            closed.check_retained_file(),
            Err(PrepareError::CandidateChanged)
        );
        assert_eq!(private::file_identity(&closed._file).unwrap(), identity);
        if case == 2 {
            fs::remove_file(&alias).unwrap();
        } else {
            fs::write(&path, &original).unwrap();
        }
        closed.check_retained_file().unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn prepared_close_failure_into_parts_preserves_busy_owner_and_error() {
    use rusqlite::ffi;
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    for explicit_close in [true, false] {
        let dir = TempDir::new().unwrap();
        let (archive, _) = fixture(dir.path(), |_| {});
        let prepared = prepare(&archive, dir.path()).unwrap();
        let retained = prepared.retained.as_ref().unwrap();
        retained.check_retained_file().unwrap();
        let path = retained._staging.path().to_owned();
        let fd = retained._file.as_raw_fd();
        let metadata = retained._file.metadata().unwrap();
        let identity = (metadata.dev(), metadata.ino());
        let manifest = serde_json::to_vec(&retained._manifest).unwrap();
        let connection = prepared.database.as_ref().unwrap();
        // Same existing FFI test technique as context-close: a real unfinished
        // statement without a Rust borrow across consuming Connection::close.
        // SAFETY: connection remains owned on BUSY; finalize exactly once below.
        let handle = unsafe { connection.handle() };
        let mut statement = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                ffi::sqlite3_prepare_v2(
                    handle,
                    c"SELECT 42".as_ptr(),
                    -1,
                    &mut statement,
                    std::ptr::null_mut()
                ),
                ffi::SQLITE_OK
            );
            assert_eq!(ffi::sqlite3_step(statement), ffi::SQLITE_ROW);
        }
        close_results::take();
        let failure = match prepared.try_close() {
            Err(failure) => failure,
            Ok(_) => panic!("unfinished statement must cause actual close BUSY"),
        };
        assert_eq!(close_results::take(), vec![ffi::SQLITE_BUSY]);
        let expected_error = failure.error.to_string();
        let message_pointer = match &failure.error {
            rusqlite::Error::SqliteFailure(code, message) => {
                assert_eq!(code.extended_code, ffi::SQLITE_BUSY);
                message.as_ref().map(|value| value.as_ptr())
            }
            other => panic!("wrong close failure: {other}"),
        };
        let owner_pointer = &*failure._owner as *const PreparedRestore;
        let (owner, error) = failure.into_parts();
        assert!(
            close_results::take().is_empty(),
            "recovery must not retry close"
        );
        assert_eq!(&*owner as *const PreparedRestore, owner_pointer);
        assert_eq!(error.to_string(), expected_error);
        match &error {
            rusqlite::Error::SqliteFailure(code, message) => {
                assert_eq!(code.extended_code, ffi::SQLITE_BUSY);
                assert_eq!(
                    message.as_ref().map(|value| value.as_ptr()),
                    message_pointer
                );
            }
            other => panic!("original error replaced: {other}"),
        }
        let retained = owner.retained.as_ref().unwrap();
        assert_eq!(retained._file.as_raw_fd(), fd);
        let metadata = retained._file.metadata().unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), identity);
        assert_eq!(retained._staging.path(), path);
        assert_eq!(serde_json::to_vec(&retained._manifest).unwrap(), manifest);
        retained.check_retained_file().unwrap();
        assert!(path.is_dir());
        let connection = owner.database.as_ref().unwrap();
        assert_eq!(unsafe { connection.handle() }, handle);
        assert_eq!(
            connection
                .query_row("SELECT 7", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            7
        );
        // No extra main-file opens, dup or source() hashing while SQLite lives.
        assert_eq!(unsafe { ffi::sqlite3_finalize(statement) }, ffi::SQLITE_OK);
        assert!(close_results::take().is_empty());
        if explicit_close {
            let closed = (*owner)
                .try_close()
                .unwrap_or_else(|_| panic!("close after finalize"));
            assert_eq!(close_results::take(), vec![ffi::SQLITE_OK]);
            assert!(path.is_dir());
            closed.check_retained_file().unwrap();
            drop(closed);
        } else {
            drop(owner); // unchanged never-active Drop performs checked cleanup
            assert_eq!(close_results::take(), vec![ffi::SQLITE_OK]);
        }
        assert!(!path.exists(), "cleanup only after safe close");
    }
}

#[test]
fn prepared_close_failure_into_parts_compiler_checks_consumption_and_no_closed_proof() {
    fn declaration(source: &str, prefix: &str) -> String {
        let start = source.find(prefix).unwrap();
        let mut depth = 0;
        for (offset, ch) in source[start..].char_indices() {
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return source[start..start + offset + 1].to_owned();
                }
            }
        }
        panic!("unterminated declaration");
    }
    let source = include_str!("../restore.rs");
    let common = format!(
        r#"
        mod backup {{
            pub mod restore {{
                mod rusqlite {{ pub struct Connection; pub struct Error; }}
                use rusqlite::Connection;
                pub(in crate::backup) struct ClosedPreparedRestore;
                {}
                {}
                {}
            }}
            {}
        }}
    "#,
        declaration(source, "pub(crate) struct PreparedRestore"),
        declaration(source, "pub(in crate::backup) struct PreparedCloseFailure"),
        declaration(source, "impl PreparedCloseFailure"),
        "PROBE"
    );
    // Actual owner/factory declarations, with only adjacent SQLite/file backend
    // types stubbed. Static checks make no claim about runtime close behavior.
    let dir = TempDir::new().unwrap();
    let compile = |probe: &str| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, common.replace("PROBE", probe)).unwrap();
        std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg(&path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive =
        compile("fn valid(f:restore::PreparedCloseFailure){let (_owner,_error)=f.into_parts();}");
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, diagnostic) in [
        (
            "fn bad(f:restore::PreparedCloseFailure){let _=f.into_parts();let _=f.into_parts();}",
            "E0382",
        ),
        (
            "fn bad(f:restore::PreparedCloseFailure){let (owner,_)=f.into_parts();let _:restore::ClosedPreparedRestore=*owner;}",
            "E0308",
        ),
    ] {
        let result = compile(probe);
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn versioning_restore_keeps_schema_one_rejected_with_the_same_error_and_cleanup() {
    for change_manifest in [false, true] {
        let dir = TempDir::new().unwrap();
        let staging = TempDir::new().unwrap();
        let (archive, source) = fixture(dir.path(), |_| {});
        rewrite(&archive, |manifest, bytes| {
            if change_manifest {
                manifest.database_schema_version = 1;
            } else {
                // SQLite user_version is the big-endian u32 at file header offset 60.
                bytes[60..64].copy_from_slice(&1u32.to_be_bytes());
                manifest.database_sha256 = format!("{:x}", Sha256::digest(&*bytes));
            }
        });
        let source_before = fs::read(&source).unwrap();
        let archive_before = fs::read(&archive).unwrap();
        assert_eq!(
            error(prepare(&archive, staging.path())),
            PrepareError::IncompatibleSchema
        );
        assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        assert_eq!(fs::read(&archive).unwrap(), archive_before);
    }
}
