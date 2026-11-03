use super::*;
use crate::database;
use rusqlite::{OptionalExtension, params};
use std::{fs, sync::mpsc};

const ORIGINAL: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Blåmes 🌿"}]}]}"#;

fn source_at(path: &Path) -> Connection {
    let connection = Connection::open(path).unwrap();
    database::initialize_schema(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO journal_entries VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "entry-1",
                "2026-10-17",
                "Blåmes",
                ORIGINAL,
                1,
                "2026-10-17T10:00:00Z",
                "2026-10-17T11:00:00Z"
            ],
        )
        .unwrap();
    connection
}

fn unpack(path: &Path, destination: &Path) {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut entry = zip.by_name(DATABASE).unwrap();
    io::copy(&mut entry, &mut File::create(destination).unwrap()).unwrap();
}

fn rows(connection: &Connection) -> Vec<(String, String, String, String, i64, String, String)> {
    connection.prepare("SELECT id, entry_date, title, content_json, document_version, created_at, updated_at FROM journal_entries ORDER BY id").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))).unwrap()
        .collect::<Result<Vec<_>, _>>().unwrap()
}

fn package(path: &Path, manifest: &BackupManifest, bytes: &[u8]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(&serde_json::to_vec(manifest).unwrap())
        .unwrap();
    zip.start_file(DATABASE, options).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}

#[test]
fn schema2_backup_v3_preserves_places_and_tombstones() {
    let d = TempDir::new().unwrap();
    let mut source = source_at(&d.path().join("source.sqlite"));
    database::weather_schema::migrate(&mut source).unwrap();
    database::place_schema::migrate_candidate(&mut source).unwrap();
    let id = "00000000-0000-4000-8000-000000000001";
    source
        .execute(
            "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Field',0.0,0.0,2)",
            [id],
        )
        .unwrap();
    source.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES('entry-1',?1,'Old Field',0.0,0.0,1,3)",[id]).unwrap();
    source.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000002','Unused',1)",[]).unwrap();
    source
        .execute(
            "INSERT INTO journal_entries VALUES('entry-2','2026-10-17','Second',?1,1,'c','u')",
            [ORIGINAL],
        )
        .unwrap();
    source.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES('entry-2',NULL,NULL,NULL,NULL,NULL,4)",[]).unwrap();
    let archive = d.path().join("v3.mno-backup");
    let manifest = create_backup(&source, &archive, BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (3, 2)
    );
    assert_eq!(verify_backup(&archive).unwrap(), manifest);
    let extracted = d.path().join("extracted.sqlite");
    unpack(&archive, &extracted);
    let mut copy = Connection::open(&extracted).unwrap();
    assert_eq!(
        crate::storage_content::verify_v3(&mut source, &std::sync::atomic::AtomicBool::new(false))
            .unwrap(),
        crate::storage_content::verify_v3(&mut copy, &std::sync::atomic::AtomicBool::new(false))
            .unwrap()
    );
    let snapshot: String = copy
        .query_row("SELECT place_name_snapshot FROM journal_places", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(snapshot, "Old Field");
    let tombstone: (Option<String>, i64) = copy
        .query_row(
            "SELECT place_id,revision FROM journal_places WHERE entry_id='entry-2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(tombstone, (None, 4));
    let count: i64 = copy
        .query_row("SELECT COUNT(*) FROM places", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    let mut damaged = fs::read(&extracted).unwrap();
    damaged[100] ^= 1;
    let corrupt = d.path().join("corrupt.mno-backup");
    package(&corrupt, &manifest, &damaged);
    assert!(verify_backup(&corrupt).is_err());
}

#[test]
fn schema2_place_only_backup_is_preserved_and_verifiable() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    database::create_new_database(&source_path).unwrap();
    let mut source = Connection::open(&source_path).unwrap();
    database::place_schema::migrate_candidate(&mut source).unwrap();
    source.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Only place',1)", []).unwrap();
    let before =
        crate::storage_content::verify_v3(&mut source, &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
    assert_eq!(before.entry_count, 0);
    let archive = dir.path().join("place-only.mno-backup");
    let manifest = create_backup(&source, &archive, BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (3, 2)
    );
    let candidate = crate::backup::restore::prepare_candidate(
        &archive,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .unwrap();
    let after = crate::storage_content::verified_snapshot_v3(
        candidate.source().unwrap(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(after, before);
}

#[test]
fn schema4_backup_v5_preserves_weather_default_states_and_proof() {
    for state in ["absent", "active", "tombstone"] {
        let dir = TempDir::new().unwrap();
        let source_path = dir.path().join("source.sqlite");
        database::create_current_database(&source_path).unwrap();
        let mut source = Connection::open(&source_path).unwrap();
        if state != "absent" {
            let id = "00000000-0000-4000-8000-000000000001";
            source.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Weather place',59.3,18.1,1)",[id]).unwrap();
            source
                .execute(
                    "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,?1,7)",
                    [if state == "active" { Some(id) } else { None }],
                )
                .unwrap();
        }
        let before = crate::storage_content::verify_v5(
            &mut source,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let destination = dir.path().join(format!("{state}.mno-backup"));
        let manifest = create_backup(&source, &destination, BackupKind::Manual).unwrap();
        assert_eq!(
            (manifest.format_version, manifest.database_schema_version),
            (5, 4)
        );
        assert_eq!(verify_backup(&destination).unwrap(), manifest);
        let candidate = crate::backup::restore::prepare_candidate(
            &destination,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        let after = crate::storage_content::verified_snapshot_v5(
            candidate.source().unwrap(),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(after, before);
        let setting: Option<(Option<String>, i64)> = candidate
            .source()
            .unwrap()
            .query_row(
                "SELECT place_id,revision FROM weather_default_place WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap();
        assert_eq!(setting.is_some(), state != "absent");
        if state == "active" {
            assert_eq!(setting.unwrap().1, 7);
        } else if state == "tombstone" {
            assert_eq!(setting, Some((None, 7)));
        }
    }
}

#[test]
fn format5_rejects_manipulated_default_reference_and_manifest_without_publication_authority() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    database::create_current_database(&source_path).unwrap();
    let source = Connection::open(&source_path).unwrap();
    source.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES('00000000-0000-4000-8000-000000000001','Weather place',59.3,18.1,1)",[]).unwrap();
    source.execute("INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,'00000000-0000-4000-8000-000000000001',1)",[]).unwrap();
    let valid = dir.path().join("valid.mno-backup");
    let manifest = create_backup(&source, &valid, BackupKind::Manual).unwrap();

    let damaged_db = dir.path().join("damaged.sqlite");
    unpack(&valid, &damaged_db);
    let damaged = Connection::open(&damaged_db).unwrap();
    damaged.pragma_update(None, "foreign_keys", false).unwrap();
    damaged
        .execute(
            "UPDATE weather_default_place SET place_id='00000000-0000-4000-8000-000000000099'",
            [],
        )
        .unwrap();
    damaged.close().unwrap();
    let mut changed = manifest;
    changed.database_size = damaged_db.metadata().unwrap().len();
    changed.database_sha256 = digest_file(&damaged_db).unwrap();
    let manipulated = dir.path().join("manipulated-reference.mno-backup");
    package(&manipulated, &changed, &fs::read(&damaged_db).unwrap());
    assert!(verify_backup(&manipulated).is_err());
    assert!(
        crate::backup::restore::prepare_candidate(
            &manipulated,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
        )
        .is_err()
    );

    changed.format_version = 3;
    let contradictory = dir.path().join("contradictory-manifest.mno-backup");
    package(&contradictory, &changed, &fs::read(&damaged_db).unwrap());
    assert!(verify_backup(&contradictory).is_err());
}

#[test]
fn backup_contains_all_journal_fields_and_does_not_change_source() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    let before = fs::read(&source_path).unwrap();
    let destination = dir.path().join("manual.mno-backup");
    let manifest = create_backup(&source, &destination, BackupKind::Manual).unwrap();
    assert_eq!(manifest, verify_backup(&destination).unwrap());
    assert_eq!(manifest.kind, BackupKind::Manual);
    assert_eq!(manifest.book_id, None);
    assert_eq!(manifest.database_schema_version, 0);
    assert_eq!(manifest.database_sha256.len(), 64);
    assert_eq!(fs::read(&source_path).unwrap(), before);
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let copy = Connection::open(&extracted).unwrap();
    assert_eq!(rows(&copy), rows(&source));
    assert_eq!(rows(&copy)[0].3, ORIGINAL);
    let integrity: String = copy
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn snapshot_is_independent_of_later_source_changes() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let destination = dir.path().join("auto.mno-backup");
    create_backup(&source, &destination, BackupKind::Automatic).unwrap();
    source.execute("DELETE FROM journal_entries", []).unwrap();
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let copy = Connection::open(extracted).unwrap();
    assert_eq!(rows(&copy).len(), 1);
    assert_eq!(
        verify_backup(&destination).unwrap().kind,
        BackupKind::Automatic
    );
}

#[test]
fn unsupported_schema_fails_after_snapshot_without_publishing_or_leaving_temporary_files() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    source.pragma_update(None, "user_version", 99).unwrap();
    let before = fs::read(&source_path).unwrap();
    let destination = dir.path().join("failed.mno-backup");
    assert!(create_backup(&source, &destination, BackupKind::Manual).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read(&source_path).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn existing_destination_and_original_database_cannot_be_overwritten() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.mno-backup"));
    let destination = dir.path().join("existing.mno-backup");
    fs::write(&destination, "keep this backup").unwrap();
    assert!(create_backup(&source, &destination, BackupKind::Manual).is_err());
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "keep this backup"
    );
    assert!(
        create_backup(
            &source,
            &dir.path().join("source.mno-backup"),
            BackupKind::Manual
        )
        .is_err()
    );
    assert_eq!(rows(&source)[0].3, ORIGINAL);
}

#[test]
fn timeout_never_publishes_a_partial_backup() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let destination = dir.path().join("timeout.mno-backup");
    assert!(
        create_with_timeout(&source, &destination, BackupKind::Manual, Duration::ZERO).is_err()
    );
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(rows(&source)[0].3, ORIGINAL);
}

#[test]
fn missing_output_directory_does_not_change_source() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    let before = fs::read(&source_path).unwrap();
    let destination = dir.path().join("missing/output.mno-backup");
    assert!(create_backup(&source, &destination, BackupKind::Manual).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read(&source_path).unwrap(), before);
}

#[test]
fn backup_works_with_an_active_writer_but_excludes_uncommitted_data() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    let writer = Connection::open(&source_path).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='Uncommitted';")
        .unwrap();
    let destination = dir.path().join("active.mno-backup");
    create_backup(&source, &destination, BackupKind::Manual).unwrap();
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let copy = Connection::open(extracted).unwrap();
    assert_eq!(rows(&copy)[0].2, "Blåmes");
    writer.execute_batch("ROLLBACK").unwrap();
    assert_eq!(rows(&copy), rows(&source));
}

#[test]
fn wal_backup_survives_concurrent_commits_and_needs_no_sidecar_files() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    source.pragma_update(None, "journal_mode", "WAL").unwrap();
    source.execute("INSERT INTO journal_entries SELECT 'bulk-' || value, '2026-10-17', '', ?1, 1, 'created', 'updated' FROM json_each(?2)",
        params!["x".repeat(4096), serde_json::to_string(&(0..1000).collect::<Vec<_>>()).unwrap()]).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let writer = thread::spawn(move || {
        let connection = Connection::open(source_path).unwrap();
        connection.busy_timeout(Duration::from_secs(2)).unwrap();
        for i in 0..20 {
            connection
                .execute(
                    "UPDATE journal_entries SET title=?1, content_json=?2 WHERE id='entry-1'",
                    params![format!("v{i}"), format!(r#"{{"version":{i}}}"#)],
                )
                .unwrap();
            if i == 0 {
                ready_tx.send(()).unwrap();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let destination = dir.path().join("wal.mno-backup");
    let result = create_backup(&source, &destination, BackupKind::Manual);
    writer.join().unwrap();
    result.unwrap();
    verify_backup(&destination).unwrap();
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let copy = Connection::open_with_flags(&extracted, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(rows(&copy).len(), 1001);
    let entry = rows(&copy)
        .into_iter()
        .find(|row| row.0 == "entry-1")
        .unwrap();
    let version = entry.2.strip_prefix('v').unwrap();
    assert_eq!(entry.3, format!(r#"{{"version":{version}}}"#));
    let mode: String = copy
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
    assert!(!dir.path().join("extracted.sqlite-wal").exists());
}

#[test]
fn verifier_rejects_checksum_changes_unknown_versions_and_invalid_sqlite() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let destination = dir.path().join("good.mno-backup");
    let mut manifest = create_backup(&source, &destination, BackupKind::Manual).unwrap();
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let bytes = fs::read(&extracted).unwrap();
    let bad = dir.path().join("bad.mno-backup");
    manifest.database_sha256 = "0".repeat(64);
    package(&bad, &manifest, &bytes);
    assert!(verify_backup(&bad).is_err());
    manifest.database_sha256 = digest_file(&extracted).unwrap();
    manifest.format_version = 999;
    package(&bad, &manifest, &bytes);
    assert!(verify_backup(&bad).is_err());
    manifest.format_version = 1;
    let invalid = b"not a SQLite database";
    manifest.database_size = invalid.len() as u64;
    manifest.database_sha256 = format!("{:x}", Sha256::digest(invalid));
    package(&bad, &manifest, invalid);
    assert!(verify_backup(&bad).is_err());
    fs::write(&bad, b"PK truncated ZIP").unwrap();
    assert!(verify_backup(&bad).is_err());
}

#[test]
fn source_transaction_and_non_mno_database_are_rejected() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let destination = dir.path().join("failed.mno-backup");
    source.execute_batch("BEGIN").unwrap();
    assert!(create_backup(&source, &destination, BackupKind::Manual).is_err());
    source.execute_batch("ROLLBACK").unwrap();
    let empty = Connection::open_in_memory().unwrap();
    assert!(create_backup(&empty, &destination, BackupKind::Manual).is_err());
    assert!(!destination.exists());
}

#[test]
fn verifier_rejects_unexpected_paths_without_extracting_them() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("unexpected.mno-backup");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(b"{}").unwrap();
    zip.start_file("../outside.sqlite", options).unwrap();
    zip.write_all(b"should never be extracted").unwrap();
    zip.finish().unwrap();
    assert!(verify_backup(&path).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

// Construct a real ZIP with three central-directory records. The regular ZIP
// writer forbids duplicate names, so write a same-length alias and rename it in
// both headers. No lengths, offsets, payloads or CRCs change.
fn package_with_duplicate(path: &Path, manifest: &BackupManifest, database: &[u8], name: &str) {
    let alias = format!("{}X", &name[..name.len() - 1]);
    let manifest_bytes = serde_json::to_vec(manifest).unwrap();
    {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (entry_name, bytes) in [
            (MANIFEST, manifest_bytes.as_slice()),
            (DATABASE, database),
            (
                alias.as_str(),
                if name == MANIFEST {
                    manifest_bytes.as_slice()
                } else {
                    database
                },
            ),
        ] {
            zip.start_file(entry_name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    let mut bytes = fs::read(path).unwrap();
    let positions: Vec<_> = bytes
        .windows(alias.len())
        .enumerate()
        .filter_map(|(index, part)| (part == alias.as_bytes()).then_some(index))
        .collect();
    assert_eq!(positions.len(), 2);
    for index in positions {
        bytes[index..index + name.len()].copy_from_slice(name.as_bytes());
    }
    fs::write(path, bytes).unwrap();
}

#[test]
fn verifier_rejects_actual_duplicate_manifests_and_databases_even_when_zip_deduplicates() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let good = dir.path().join("good.mno-backup");
    let manifest = create_backup(&source, &good, BackupKind::Manual).unwrap();
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&good, &extracted);
    let bytes = fs::read(extracted).unwrap();
    for name in [MANIFEST, DATABASE] {
        let path = dir.path().join(format!("duplicate-{name}.mno-backup"));
        package_with_duplicate(&path, &manifest, &bytes, name);
        // The old verifier would see two names and accept this otherwise valid package.
        assert_eq!(
            ZipArchive::new(File::open(&path).unwrap()).unwrap().len(),
            2
        );
        let error = verify_backup(&path).unwrap_err();
        assert!(error.to_string().contains("duplicate central-directory"));
    }
}

#[test]
fn verifier_rejects_extra_archive_entries() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("extra.mno-backup");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for name in [MANIFEST, DATABASE, "unexpected.txt"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(b"unused").unwrap();
    }
    zip.finish().unwrap();
    assert!(
        verify_backup(&path)
            .unwrap_err()
            .to_string()
            .contains("Unexpected")
    );
}

#[test]
fn concurrent_publication_has_exactly_one_verified_winner() {
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("shared.mno-backup");
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut releases = Vec::new();
    let mut workers = Vec::new();
    for index in 0..2 {
        let source_path = dir.path().join(format!("source-{index}.sqlite"));
        let source = source_at(&source_path);
        source
            .execute(
                "UPDATE journal_entries SET title=?1",
                [format!("attempt-{index}")],
            )
            .unwrap();
        drop(source);
        let destination = destination.clone();
        let ready_tx = ready_tx.clone();
        let (release_tx, release_rx) = mpsc::channel();
        releases.push(release_tx);
        workers.push(thread::spawn(move || {
            let source = Connection::open(source_path).unwrap();
            create_before_publish(
                &source,
                &destination,
                BackupKind::Manual,
                Duration::from_secs(30),
                |path, _| {
                    // Both contenders have passed the initial exists check and verified
                    // their complete package before either may attempt publication.
                    let verified = verify_backup(path)?;
                    ready_tx.send((index, verified.backup_id.clone()))?;
                    release_rx.recv_timeout(Duration::from_secs(30))?;
                    Ok(())
                },
            )
        }));
    }
    drop(ready_tx);
    let readiness: Vec<_> = (0..2)
        .map(|_| ready_rx.recv_timeout(Duration::from_secs(30)))
        .collect();
    // Release everyone even on test failure, so no worker is left blocked.
    for release in releases {
        let _ = release.send(());
    }
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    let ready: Vec<_> = readiness.into_iter().map(Result::unwrap).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    let (winner_index, winner) = results
        .iter()
        .enumerate()
        .find_map(|(index, result)| result.as_ref().ok().map(|manifest| (index, manifest)))
        .unwrap();
    assert_eq!(&verify_backup(&destination).unwrap(), winner);
    assert_eq!(
        ready
            .iter()
            .find(|(index, _)| *index == winner_index)
            .unwrap()
            .1,
        winner.backup_id
    );
    let extracted = dir.path().join("extracted.sqlite");
    unpack(&destination, &extracted);
    let copy = Connection::open(extracted).unwrap();
    assert_eq!(rows(&copy)[0].2, format!("attempt-{winner_index}"));
    // Both attempts have finished: the loser did not replace the winner.
    assert_eq!(&verify_backup(&destination).unwrap(), winner);
}

#[test]
fn controlled_error_after_complete_verification_never_publishes() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.sqlite");
    let source = source_at(&source_path);
    let before = fs::read(&source_path).unwrap();
    let destination = dir.path().join("failed.mno-backup");
    let error = create_before_publish(
        &source,
        &destination,
        BackupKind::Manual,
        Duration::from_secs(30),
        |path, manifest| {
            assert_eq!(&verify_backup(path).unwrap(), manifest);
            assert!(!destination.exists());
            Err(io::Error::other("controlled late failure").into())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("controlled late failure"));
    assert!(!destination.exists());
    assert_eq!(fs::read(&source_path).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn real_publication_error_preserves_a_backup_created_after_initial_check() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let destination = dir.path().join("occupied.mno-backup");
    let mut published_manifest = None;
    let mut published_bytes = Vec::new();
    let result = create_before_publish(
        &source,
        &destination,
        BackupKind::Manual,
        Duration::from_secs(30),
        |_, _| {
            // Reliably occupy the destination only after the losing package was
            // fully written and verified. This exercises persist_noclobber itself.
            published_manifest = Some(create_backup(&source, &destination, BackupKind::Manual)?);
            published_bytes = fs::read(&destination)?;
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&destination).unwrap(), published_bytes);
    assert_eq!(
        verify_backup(&destination).unwrap(),
        published_manifest.unwrap()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn versioning_backup_v1_keeps_schema_zero_and_rejects_other_combinations() {
    let dir = TempDir::new().unwrap();
    let source = source_at(&dir.path().join("source.sqlite"));
    let archive = dir.path().join("original.mno-backup");
    let manifest = create_backup(&source, &archive, BackupKind::Manual).unwrap();
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.database_schema_version, 0);
    let extracted = dir.path().join("snapshot.sqlite");
    unpack(&archive, &extracted);
    let bytes = fs::read(extracted).unwrap();
    let changed = dir.path().join("changed.mno-backup");
    for format in [0, 1, 2] {
        for schema in [0, 1, 99] {
            let mut candidate: BackupManifest =
                serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
            candidate.format_version = format;
            candidate.database_schema_version = schema;
            package(&changed, &candidate, &bytes);
            let before = fs::read(&changed).unwrap();
            assert_eq!(
                verify_backup(&changed).is_ok(),
                format == 1 && schema == 0,
                "format={format}, schema={schema}"
            );
            assert_eq!(fs::read(&changed).unwrap(), before);
        }
    }
    source.pragma_update(None, "user_version", 1).unwrap();
    let rejected = dir.path().join("rejected.mno-backup");
    assert!(create_backup(&source, &rejected, BackupKind::Manual).is_err());
    assert!(!rejected.exists());
}
