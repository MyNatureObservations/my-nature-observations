use super::*;
use crate::database_state::DatabaseState;
use rusqlite::{params, types::Value};
use sha2::{Digest, Sha256};
use std::{io::Write, process::Command};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const DOCS: [&str; 4] = [
    "{}",
    "{ \"text\":\"Original åäö 🌿\", \"text\":\"duplicate\" }",
    r#"{"type":"future-node","attrs":{"src":"/never/follow.jpg"}}"#,
    r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"known"}]}]}"#,
];
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn populate(path: &Path) {
    let db = Connection::open(path).unwrap();
    for (i, doc) in DOCS.iter().enumerate() {
        db.execute(
            "INSERT INTO journal_entries VALUES(?1,?2,?3,?4,1,?5,?6)",
            params![
                if i == 0 {
                    String::new()
                } else {
                    format!("id-{i}")
                },
                if i == 0 { "" } else { "legacy date" },
                "Titel\0åäö 🌿\n",
                doc,
                " raw creation\r\n",
                "updated\0exact"
            ],
        )
        .unwrap();
    }
    db.close().unwrap();
}
fn setup(root: &Path) -> (DatabaseState, ActiveDatabase) {
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root.join("data")).unwrap();
    populate(&path);
    let active = ActiveDatabase::open(&path).unwrap();
    (state, active)
}
fn rows(db: &Connection) -> Vec<Vec<Value>> {
    let mut stmt = db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
    stmt.query_map([], |r| (0..7).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}
fn filename(maintenance: &Maintenance<'_>, root: &Path) -> PathBuf {
    root.join("backups/pre-restore").join(format!(
        "pre-import-{}.mno-backup",
        maintenance.operation_id()
    ))
}
fn valid_archive(root: &Path) -> (PathBuf, ContentProof) {
    let (state, mut active) = setup(root);
    let maintenance = state.maintenance().unwrap();
    let protection = create(&maintenance, &mut active, flag()).unwrap();
    (protection.path.clone(), protection.evidence.content.clone())
}
fn package(path: &Path, entries: &[(String, Vec<u8>)]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut file = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (file.name().to_owned(), bytes)
        })
        .collect()
}
fn repack_database(archive: &Path, path: &Path, change: impl FnOnce(&Connection)) -> PathBuf {
    let mut parts = entries(archive);
    let db_path = path.join("changed.sqlite");
    fs::write(&db_path, &parts[1].1).unwrap();
    let db = Connection::open(&db_path).unwrap();
    change(&db);
    db.close().unwrap();
    let bytes = fs::read(&db_path).unwrap();
    let mut manifest: BackupManifest = serde_json::from_slice(&parts[0].1).unwrap();
    manifest.database_size = bytes.len() as u64;
    manifest.database_sha256 = backup::digest_file(&db_path).unwrap();
    parts[0].1 = serde_json::to_vec(&manifest).unwrap();
    parts[1].1 = bytes;
    let result = path.join("changed.mno-backup");
    package(&result, &parts);
    result
}

#[test]
fn lossless_protection_is_independently_verified_and_preserves_all_storage_values() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (state, mut active) = setup(&root);
    let original = fs::read(active.path()).unwrap();
    let expected = rows(active.connection());
    let maintenance = state.maintenance().unwrap();
    let protection = create(&maintenance, &mut active, flag()).unwrap();
    assert_eq!(protection.operation_id, maintenance.operation_id());
    assert_eq!(protection.generation, maintenance.generation());
    assert_eq!(protection.source_identity, active.identity());
    assert_eq!(
        protection.path,
        filename(&maintenance, active.path().parent().unwrap())
    );
    assert_eq!(protection.evidence.content.entry_count, 4);
    assert_eq!(
        protection.evidence.content,
        source_proof(&mut active, &AtomicBool::new(false)).unwrap()
    );
    let archive = protection.path.clone();
    let archive_before = fs::read(&archive).unwrap();
    for _ in 0..2 {
        let evidence =
            verify_archive(&archive, &root, &protection.evidence.content, flag()).unwrap();
        assert_eq!(evidence.content, protection.evidence.content);
        assert_eq!(evidence.manifest, protection.evidence.manifest);
        assert_eq!(evidence.archive_sha256, protection.evidence.archive_sha256);
    }
    let (directory, _) =
        backup::restore::extract_for_import(File::open(&archive).unwrap(), &root, flag(), |p| {
            fs2::available_space(p)
        })
        .unwrap();
    let db = Connection::open_with_flags(
        directory.path().join(backup::DATABASE),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(rows(&db), expected); // Value compares INTEGER/TEXT as well as exact bytes.
    db.close().unwrap();
    drop(directory);
    assert_eq!(fs::read(active.path()).unwrap(), original);
    assert_eq!(fs::read(&archive).unwrap(), archive_before);
    assert!(!active.path().parent().unwrap().join("recovery").exists());
    assert!(
        !active
            .path()
            .parent()
            .unwrap()
            .join("pending-restore.json")
            .exists()
    );
    assert!(
        !active
            .path()
            .parent()
            .unwrap()
            .join(".mno-restore-completion-v1.json")
            .exists()
    );
    drop(protection);
    assert!(archive.is_file());
    drop(active);
    drop(maintenance);
    assert!(state.operation(None).is_ok()); // No RecoverySession or new admission path.
}
#[test]
fn empty_journal_has_the_same_existing_import_proof() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let mut active = ActiveDatabase::open(&path).unwrap();
    let maintenance = state.maintenance().unwrap();
    let protection = create(&maintenance, &mut active, flag()).unwrap();
    assert_eq!(protection.evidence.content.entry_count, 0);
    assert_eq!(
        protection.evidence.content,
        source_proof(&mut active, &AtomicBool::new(false)).unwrap()
    );
}
#[test]
fn collision_and_racing_destination_never_overwrite_any_archive() {
    for race in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (state, mut active) = setup(&root);
        let maintenance = state.maintenance().unwrap();
        let path = filename(&maintenance, active.path().parent().unwrap());
        if !race {
            let first = create(&maintenance, &mut active, flag()).unwrap();
            let before = fs::read(&first.path).unwrap();
            assert!(create(&maintenance, &mut active, flag()).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        } else {
            let result = create_with(
                &maintenance,
                &mut active,
                flag(),
                |_| {
                    fs::write(&path, b"created after initial check")?;
                    Ok(())
                },
                durability::sync,
            );
            assert!(result.is_err());
            assert_eq!(fs::read(&path).unwrap(), b"created after initial check");
        }
    }
}
#[test]
fn actual_archive_corruption_manifest_and_part_errors_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (archive, proof) = valid_archive(&root);
    let original = entries(&archive);
    for mode in [
        "truncated",
        "checksum",
        "size",
        "version",
        "schema-version",
        "missing",
        "extra",
        "traversal",
        "manifest-limit",
        "overhead",
    ] {
        let bad = root.join(format!("{mode}.mno-backup"));
        let mut parts = original.clone();
        match mode {
            "truncated" => {
                fs::write(&bad, b"not a ZIP file").unwrap();
            }
            "checksum" | "size" | "version" | "schema-version" => {
                let mut m: serde_json::Value = serde_json::from_slice(&parts[0].1).unwrap();
                match mode {
                    "checksum" => m["database_sha256"] = serde_json::json!("0".repeat(64)),
                    "size" => m["database_size"] = serde_json::json!(1),
                    "version" => m["format_version"] = serde_json::json!(2),
                    _ => m["database_schema_version"] = serde_json::json!(1),
                }
                parts[0].1 = serde_json::to_vec(&m).unwrap();
                package(&bad, &parts);
            }
            "missing" => package(&bad, &parts[..1]),
            "extra" | "traversal" => {
                parts.push((
                    if mode == "extra" {
                        "extra"
                    } else {
                        "../external"
                    }
                    .into(),
                    b"ignored?".to_vec(),
                ));
                package(&bad, &parts);
            }
            "manifest-limit" => {
                parts[0].1 = vec![b' '; 64 * 1024 + 1];
                package(&bad, &parts);
            }
            "overhead" => {
                package(&bad, &parts);
                fs::OpenOptions::new()
                    .append(true)
                    .open(&bad)
                    .unwrap()
                    .write_all(&vec![0; 1024 * 1024 + 1])
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            verify_archive(&bad, &root, &proof, flag()).is_err(),
            "accepted {mode}"
        );
    }
    assert!(verify_archive(&archive, &root, &proof, flag()).is_ok());
    assert!(!root.join("external").exists());
}
#[test]
fn wrong_proofs_and_rehashed_invalid_database_contents_are_rejected() {
    for sql in [
        "CREATE TABLE extra(x)",
        "PRAGMA user_version=1",
        "UPDATE journal_entries SET content_json='not JSON'",
        "UPDATE journal_entries SET content_json='[]'",
        "UPDATE journal_entries SET document_version=2",
        "UPDATE journal_entries SET title=CAST('text' AS BLOB)",
        "UPDATE journal_entries SET title='changed valid text'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (archive, proof) = valid_archive(&root);
        let bad = repack_database(&archive, &root, |db| db.execute_batch(sql).unwrap());
        let error = verify_archive(&bad, &root, &proof, flag())
            .err()
            .expect("invalid database accepted")
            .to_string();
        assert!(
            error.contains(if sql.contains("changed valid text") {
                "content proof differs from source"
            } else {
                "content validation failed"
            }),
            "wrong failure for {sql}: {error}"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (archive, proof) = valid_archive(&root);
    for changed in [
        ContentProof {
            version: 2,
            ..proof.clone()
        },
        ContentProof {
            sha256: "0".repeat(64),
            ..proof.clone()
        },
        ContentProof {
            entry_count: 5,
            ..proof.clone()
        },
    ] {
        assert!(verify_archive(&archive, &root, &changed, flag()).is_err());
    }
}
// Test-only encoding of the frozen import proof, deliberately without JSON or
// size validation. This lets an over-limit fixture carry its CORRECT proof.
fn raw_fixture_proof(path: &Path) -> ContentProof {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let records = rows(&db);
    db.close().unwrap();
    let mut hash = Sha256::new();
    hash.update(b"MNO-open-import-content\0\x00\x00\x00\x01schema-0\0journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0");
    for row in &records {
        hash.update([0x52]);
        for (index, value) in row.iter().enumerate() {
            match (index, value) {
                (4, Value::Integer(version)) => {
                    hash.update([1]);
                    hash.update(version.to_be_bytes());
                }
                (_, Value::Text(text)) if index != 4 => {
                    hash.update([3]);
                    hash.update((text.len() as u64).to_be_bytes());
                    hash.update(text.as_bytes());
                }
                _ => panic!("unexpected SQL type in resource-limit fixture"),
            }
        }
    }
    hash.update([0x45]);
    hash.update((records.len() as u64).to_be_bytes());
    ContentProof {
        version: 1,
        sha256: format!("{:x}", hash.finalize()),
        entry_count: records.len() as u64,
    }
}
#[test]
fn disk_space_and_preservation_resource_limits_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (archive, proof) = valid_archive(&root);
    let error = verify_with_space(&archive, &root, &proof, flag(), |_| Ok(0))
        .err()
        .unwrap();
    assert_eq!(
        error.downcast_ref::<backup::restore::PrepareError>(),
        Some(&backup::restore::PrepareError::InsufficientSpace)
    );
    assert_eq!(raw_fixture_proof(&root.join("data/mno.sqlite")), proof);
    for (case, sql) in [
        (
            "oversized title",
            format!(
                "UPDATE journal_entries SET title='{}'",
                "a".repeat(crate::open_export::METADATA_LIMIT + 1)
            ),
        ),
        (
            "excessive JSON depth",
            format!(
                "UPDATE journal_entries SET content_json='{{\"nested\":{}0{}}}'",
                "[".repeat(70),
                "]".repeat(70)
            ),
        ),
    ] {
        let bad = repack_database(&archive, &root, |db| db.execute_batch(&sql).unwrap());
        let modified_proof = raw_fixture_proof(&root.join("changed.sqlite"));
        assert_ne!(modified_proof, proof);
        let error = verify_archive(&bad, &root, &modified_proof, flag())
            .err()
            .unwrap_or_else(|| panic!("{case} unexpectedly accepted"));
        assert_eq!(
            error.to_string(),
            "Import protection content validation failed: ResourceLimit",
            "wrong failure for {case}"
        );
    }
}
#[test]
fn cancellation_write_and_sync_errors_never_return_verified_protection() {
    for mode in [
        "cancel",
        "write-full",
        "verified-file",
        "directory",
        "final-file",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (state, mut active) = setup(&root);
        let before = fs::read(active.path()).unwrap();
        let maintenance = state.maintenance().unwrap();
        let destination = filename(&maintenance, active.path().parent().unwrap());
        let flag = flag();
        let cancel_flag = flag.clone();
        let result = create_with(
            &maintenance,
            &mut active,
            flag,
            |_| {
                if mode == "cancel" {
                    cancel_flag.store(true, Ordering::Release);
                }
                if mode == "write-full" {
                    return Err(std::io::Error::from(std::io::ErrorKind::StorageFull).into());
                }
                Ok(())
            },
            |point, path| {
                if matches!(
                    (mode, point),
                    ("verified-file", SyncPoint::VerifiedFile)
                        | ("directory", SyncPoint::Directory)
                        | ("final-file", SyncPoint::FinalFileBarrier)
                ) {
                    return Err(std::io::Error::from(std::io::ErrorKind::StorageFull));
                }
                durability::sync(point, path)
            },
        );
        assert!(result.is_err(), "reported success for {mode}");
        assert_eq!(fs::read(active.path()).unwrap(), before);
        if mode == "directory" || mode == "final-file" {
            assert!(destination.is_file());
            let expected = source_proof(&mut active, &AtomicBool::new(false)).unwrap();
            verify_archive(&destination, &root, &expected, self::flag()).unwrap();
        } else {
            assert!(!destination.exists());
        }
    }
}
#[test]
fn strict_restore_still_rejects_legacy_and_unknown_documents() {
    for doc in &DOCS[..3] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let state = DatabaseState::default();
        let path = state.initialize_legacy_startup(&root.join("data")).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES('valid-id','2026-10-24','Title',?1,1,'created','updated')",[doc]).unwrap();
        db.close().unwrap();
        let mut active = ActiveDatabase::open(&path).unwrap();
        let maintenance = state.maintenance().unwrap();
        let protection = create(&maintenance, &mut active, flag()).unwrap();
        let result = backup::restore::prepare_candidate(&protection.path, flag());
        assert!(matches!(
            result,
            Err(backup::restore::PrepareError::IncompatibleDocument)
        ));
    }
}
const CHILD: &str =
    "backup::restore_context::activation::import_protection::tests::interruption_child";
#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_1B2_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_1B2_MODE").unwrap();
    let (state, mut active) = setup(&root);
    let maintenance = state.maintenance().unwrap();
    fs::write(root.join("before"), fs::read(active.path()).unwrap()).unwrap();
    fs::write(
        root.join("destination"),
        filename(&maintenance, active.path().parent().unwrap())
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let _result = create_with(
        &maintenance,
        &mut active,
        flag(),
        |_| Ok(()),
        |point, path| {
            if matches!(
                (mode.as_str(), point),
                ("before", SyncPoint::VerifiedFile) | ("after", SyncPoint::Directory)
            ) {
                eprintln!("INTERRUPTED:{mode}:{point:?}");
                std::process::exit(88);
            }
            durability::sync(point, path)
        },
    );
    panic!("intended interruption did not execute");
}
#[test]
fn process_interruptions_preserve_active_and_allow_normal_startup_without_markers() {
    for mode in ["before", "after"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env("MNO_1B2_ROOT", &root)
            .env("MNO_1B2_MODE", mode)
            .env("TMPDIR", &root)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(88),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains(&format!("INTERRUPTED:{mode}:")));
        let path = root.join("data/mno.sqlite");
        assert_eq!(
            fs::read(&path).unwrap(),
            fs::read(root.join("before")).unwrap()
        );
        let destination = PathBuf::from(fs::read_to_string(root.join("destination")).unwrap());
        assert_eq!(destination.exists(), mode == "after");
        let state = DatabaseState::default();
        state.initialize_legacy_startup(&root.join("data")).unwrap();
        assert!(state.operation(None).is_ok());
        assert!(!root.join("data/recovery").exists());
        if mode == "after" {
            let mut active = ActiveDatabase::open(&path).unwrap();
            let expected = source_proof(&mut active, &AtomicBool::new(false)).unwrap();
            verify_archive(&destination, &root, &expected, flag()).unwrap();
        }
        assert_eq!(
            fs::read(&path).unwrap(),
            fs::read(root.join("before")).unwrap()
        );
    }
}

#[test]
fn full_size_valid_document_is_not_subject_to_strict_restores_smaller_row_limit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let db = Connection::open(&path).unwrap();
    let mut doc = String::from("{\"text\":\"");
    doc.extend(std::iter::repeat_n(
        'a',
        crate::open_export::CONTENT_LIMIT - 11,
    ));
    doc.push_str("\"}");
    assert_eq!(doc.len(), crate::open_export::CONTENT_LIMIT);
    // The whole row exceeds 64 MiB even though each field meets import limits.
    db.execute(
        "INSERT INTO journal_entries VALUES('large','legacy date',?1,?2,1,'created','updated')",
        params!["t".repeat(1024), doc],
    )
    .unwrap();
    drop(doc);
    db.close().unwrap();
    let mut active = ActiveDatabase::open(&path).unwrap();
    let maintenance = state.maintenance().unwrap();
    let protection = create(&maintenance, &mut active, flag()).unwrap();
    let actual = verify_archive(
        &protection.path,
        &root,
        &protection.evidence.content,
        flag(),
    )
    .unwrap();
    assert_eq!(actual.content, protection.evidence.content);
    assert_eq!(actual.content.entry_count, 1);
}

#[test]
fn source_sidecars_are_rejected_without_sqlite_recovery_or_cleanup() {
    for suffix in ["-journal", "-wal", "-shm"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (state, mut active) = setup(&root);
        let before = fs::read(active.path()).unwrap();
        let sidecar = PathBuf::from(format!("{}{suffix}", active.path().display()));
        fs::write(&sidecar, b"reserved blocking evidence").unwrap();
        let maintenance = state.maintenance().unwrap();
        let error = create(&maintenance, &mut active, flag())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("Unsafe sidecar"), "{error}");
        assert_eq!(fs::read(&sidecar).unwrap(), b"reserved blocking evidence");
        assert_eq!(fs::read(active.path()).unwrap(), before);
        assert!(!filename(&maintenance, active.path().parent().unwrap()).exists());
    }
}
