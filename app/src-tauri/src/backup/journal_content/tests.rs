use super::*;
use crate::{
    backup::{self, BackupKind},
    database::initialize_schema,
};
use rusqlite::params;
use std::sync::atomic::AtomicUsize;
use std::{fs, path::Path};
use tempfile::TempDir;
const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Blåmes 🌿"}]}]}"#;
fn database(path: &Path) -> Connection {
    let db = Connection::open(path).unwrap();
    initialize_schema(&db).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('one','2026-10-17','Title',?1,1,'created','updated')",
        [DOC],
    )
    .unwrap();
    db
}
fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn fingerprints(
    a: &mut Connection,
    b: &mut Connection,
) -> (Comparison, ContentFingerprint, ContentFingerprint) {
    let a = VerifiedJournal::verify(a, cancel()).unwrap();
    let b = VerifiedJournal::verify(b, cancel()).unwrap();
    let result = compare(&a, &b).unwrap();
    let x = fingerprint(&a).unwrap();
    let y = fingerprint(&b).unwrap();
    a.finish().unwrap();
    b.finish().unwrap();
    (result, x, y)
}
#[test]
fn identical_data_is_layout_independent_and_connections_and_files_are_not_written() {
    let dir = TempDir::new().unwrap();
    let a_path = dir.path().join("a.sqlite");
    let b_path = dir.path().join("b.sqlite");
    let mut a = database(&a_path);
    let mut b = Connection::open(&b_path).unwrap();
    b.execute_batch("PRAGMA page_size=8192").unwrap();
    initialize_schema(&b).unwrap();
    b.execute(
        "INSERT INTO journal_entries VALUES ('one','2026-10-17','Title',?1,1,'created','updated')",
        [DOC],
    )
    .unwrap();
    let before = (fs::read(&a_path).unwrap(), fs::read(&b_path).unwrap());
    assert_ne!(before.0, before.1);
    let (result, x, y) = fingerprints(&mut a, &mut b);
    assert_eq!(result, Comparison::Identical);
    assert_eq!(x, y);
    assert_eq!(x.version, 1);
    assert!(a.is_autocommit());
    assert!(b.is_autocommit());
    assert_eq!(
        (fs::read(a_path).unwrap(), fs::read(b_path).unwrap()),
        before
    );
}
#[test]
fn every_column_and_row_count_affects_exact_comparison_and_fingerprint() {
    let dir = TempDir::new().unwrap();
    for sql in [
        "UPDATE journal_entries SET id='other'",
        "UPDATE journal_entries SET entry_date='2026-10-18'",
        "UPDATE journal_entries SET title='Different'",
        "UPDATE journal_entries SET content_json=' {\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"Blåmes 🌿\"}]}]}'",
        "UPDATE journal_entries SET created_at='different'",
        "UPDATE journal_entries SET updated_at='different'",
        "DELETE FROM journal_entries",
        "INSERT INTO journal_entries SELECT 'extra',entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries",
    ] {
        let a_path = dir.path().join(format!("{}a.sqlite", uuid::Uuid::new_v4()));
        let b_path = dir.path().join(format!("{}b.sqlite", uuid::Uuid::new_v4()));
        let mut a = database(&a_path);
        let mut b = database(&b_path);
        b.execute_batch(sql).unwrap();
        let (result, x, y) = fingerprints(&mut a, &mut b);
        assert_eq!(result, Comparison::Different, "{sql}");
        assert_ne!(x, y, "{sql}");
    }
}
#[test]
fn row_insertion_order_does_not_change_content() {
    let mut a = Connection::open_in_memory().unwrap();
    let mut b = Connection::open_in_memory().unwrap();
    initialize_schema(&a).unwrap();
    initialize_schema(&b).unwrap();
    for (db, ids) in [(&a, vec!["a", "b", "c"]), (&b, vec!["c", "b", "a"])] {
        for id in ids {
            db.execute(
                "INSERT INTO journal_entries VALUES (?1,'2026-10-17',?1,?2,1,'created','updated')",
                params![id, DOC],
            )
            .unwrap();
        }
    }
    let (result, x, y) = fingerprints(&mut a, &mut b);
    assert_eq!(result, Comparison::Identical);
    assert_eq!(x, y);
}
#[test]
fn invalid_document_version_null_and_blob_never_produce_verified_equality() {
    for (sql, expected) in [
        (
            "UPDATE journal_entries SET document_version=2",
            PrepareError::IncompatibleDocument,
        ),
        (
            "UPDATE journal_entries SET id=NULL",
            PrepareError::InvalidDatabase,
        ),
        (
            "UPDATE journal_entries SET title=CAST(title AS BLOB)",
            PrepareError::InvalidDatabase,
        ),
        (
            "UPDATE journal_entries SET content_json='not json'",
            PrepareError::IncompatibleDocument,
        ),
        (
            "CREATE TRIGGER unexpected AFTER UPDATE ON journal_entries BEGIN DELETE FROM journal_entries; END",
            PrepareError::IncompatibleSchema,
        ),
        ("PRAGMA user_version=99", PrepareError::IncompatibleSchema),
    ] {
        let dir = TempDir::new().unwrap();
        let a = database(&dir.path().join("a.sqlite"));
        let mut b = database(&dir.path().join("b.sqlite"));
        b.execute_batch(sql).unwrap();
        if !sql.starts_with("CREATE") && !sql.starts_with("PRAGMA") {
            assert_eq!(
                compare_rows(&a, &b, &AtomicBool::new(false), &AtomicBool::new(false)).unwrap(),
                Comparison::Different
            );
            assert_ne!(
                fingerprint_rows(&a, &AtomicBool::new(false)).unwrap(),
                fingerprint_rows(&b, &AtomicBool::new(false)).unwrap()
            );
        }
        let result = VerifiedJournal::verify(&mut b, cancel());
        assert!(
            matches!(result,Err(ContentError::Database(e)) if e==expected),
            "{sql}"
        );
    }
}
#[test]
fn null_empty_text_blob_and_numeric_types_have_distinct_encoding() {
    let values = [
        ValueRef::Null,
        ValueRef::Text(b""),
        ValueRef::Blob(b""),
        ValueRef::Integer(1),
        ValueRef::Real(1.0),
        ValueRef::Text(b"1"),
        ValueRef::Blob(b"1"),
        ValueRef::Real(-0.0),
        ValueRef::Real(0.0),
    ];
    for (i, a) in values.iter().enumerate() {
        for (j, b) in values.iter().enumerate() {
            let mut x = Sha256::new();
            encode(&mut x, *a);
            let mut y = Sha256::new();
            encode(&mut y, *b);
            assert_eq!(same_value(*a, *b), i == j);
            assert_eq!(x.finalize() == y.finalize(), i == j);
        }
    }
    let mut x = Sha256::new();
    encode(&mut x, ValueRef::Text(b"ab"));
    encode(&mut x, ValueRef::Text(b"c"));
    let mut y = Sha256::new();
    encode(&mut y, ValueRef::Text(b"a"));
    encode(&mut y, ValueRef::Text(b"bc"));
    assert_ne!(x.finalize(), y.finalize());
}
#[test]
fn caller_read_transaction_is_preserved_and_write_transaction_is_rejected() {
    let dir = TempDir::new().unwrap();
    let mut db = database(&dir.path().join("a.sqlite"));
    db.execute_batch("BEGIN; SELECT count(*) FROM journal_entries;")
        .unwrap();
    let snapshot = VerifiedJournal::verify(&mut db, cancel()).unwrap();
    fingerprint(&snapshot).unwrap();
    snapshot.finish().unwrap();
    assert!(!db.is_autocommit());
    db.execute_batch("ROLLBACK").unwrap();
    db.execute_batch("BEGIN; UPDATE journal_entries SET title='uncommitted';")
        .unwrap();
    assert!(matches!(
        VerifiedJournal::verify(&mut db, cancel()),
        Err(ContentError::WriteTransaction)
    ));
    assert!(!db.is_autocommit());
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Title"
    );
}
#[test]
fn candidate_can_be_compared_without_reopening_or_releasing_its_transaction() {
    let dir = TempDir::new().unwrap();
    let mut db = database(&dir.path().join("source.sqlite"));
    let archive = dir.path().join("selected.mno-backup");
    backup::create_backup(&db, &archive, BackupKind::Manual).unwrap();
    let original = fs::read(&archive).unwrap();
    let candidate = backup::restore::prepare_candidate(&archive, cancel()).unwrap();
    fs::remove_file(&archive).unwrap();
    let a = VerifiedJournal::from_candidate(&candidate, cancel()).unwrap();
    let b = VerifiedJournal::verify(&mut db, cancel()).unwrap();
    assert_eq!(compare(&a, &b).unwrap(), Comparison::Identical);
    assert_eq!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
    a.finish().unwrap();
    b.finish().unwrap();
    assert!(!candidate.source().unwrap().is_autocommit());
    assert!(candidate.source().unwrap().execute_batch("COMMIT").is_err());
    assert!(!original.is_empty());
}
#[test]
fn wal_snapshot_stays_stable_when_another_connection_commits() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wal.sqlite");
    let mut reader = database(&path);
    reader
        .query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))
        .unwrap();
    let snapshot = VerifiedJournal::verify(&mut reader, cancel()).unwrap();
    let before = fingerprint(&snapshot).unwrap();
    std::thread::spawn(move || {
        let writer = Connection::open(path).unwrap();
        writer
            .execute("UPDATE journal_entries SET title='new committed value'", [])
            .unwrap();
    })
    .join()
    .unwrap();
    assert_eq!(fingerprint(&snapshot).unwrap(), before);
    snapshot.finish().unwrap();
    let new = VerifiedJournal::verify(&mut reader, cancel()).unwrap();
    assert_ne!(fingerprint(&new).unwrap(), before);
}
#[test]
fn cancelled_verification_and_fingerprint_are_errors_and_release_own_transactions() {
    let dir = TempDir::new().unwrap();
    let mut db = database(&dir.path().join("source.sqlite"));
    let flag = cancel();
    flag.store(true, Ordering::Release);
    assert!(matches!(
        VerifiedJournal::verify(&mut db, flag),
        Err(ContentError::Database(PrepareError::Cancelled))
    ));
    assert!(db.is_autocommit());
    let flag = cancel();
    let snapshot = VerifiedJournal::verify(&mut db, flag.clone()).unwrap();
    flag.store(true, Ordering::Release);
    assert!(matches!(
        fingerprint(&snapshot),
        Err(ContentError::Database(PrepareError::Cancelled))
    ));
    snapshot.finish().unwrap();
    assert!(db.is_autocommit());
}
#[test]
fn corruption_is_not_an_empty_or_matching_journal() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("broken.sqlite");
    let db = database(&path);
    drop(db);
    let mut bytes = fs::read(&path).unwrap();
    bytes[100] = 0xff;
    fs::write(&path, bytes).unwrap();
    let mut db = Connection::open(path).unwrap();
    assert!(matches!(
        VerifiedJournal::verify(&mut db, cancel()),
        Err(ContentError::Database(PrepareError::InvalidDatabase))
    ));
}
#[test]
fn oversized_valid_json_has_resource_classification_without_file_changes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("large.sqlite");
    let mut db = database(&path);
    let doc = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"{}"}}]}}]}}"#,
        "a".repeat(64 * 1024 * 1024)
    );
    db.execute("UPDATE journal_entries SET content_json=?1", [&doc])
        .unwrap();
    drop(doc);
    let before = backup::digest_file(&path).unwrap();
    assert!(matches!(
        VerifiedJournal::verify(&mut db, cancel()),
        Err(ContentError::Database(PrepareError::ResourceLimit))
    ));
    assert!(db.is_autocommit());
    assert_eq!(backup::digest_file(&path).unwrap(), before);
}
#[test]
fn io_errors_are_distinct_from_corruption_and_resources() {
    let e = rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
        None,
    );
    assert_eq!(sql(e), ContentError::Database(PrepareError::Io));
}

#[test]
fn fingerprint_v1_has_a_fixed_independently_encoded_vector_and_empty_journals_match() {
    let dir = TempDir::new().unwrap();
    let mut db = database(&dir.path().join("known.sqlite"));
    let snapshot = VerifiedJournal::verify(&mut db, cancel()).unwrap();
    assert_eq!(
        fingerprint(&snapshot).unwrap().sha256,
        "50009532a89517b1e3c5ad52caa40fee6eea9b87a15f1b38549ca7a4a34a67b0"
    );
    snapshot.finish().unwrap();
    let mut a = Connection::open_in_memory().unwrap();
    let mut b = Connection::open_in_memory().unwrap();
    initialize_schema(&a).unwrap();
    initialize_schema(&b).unwrap();
    let (result, x, y) = fingerprints(&mut a, &mut b);
    assert_eq!(result, Comparison::Identical);
    assert_eq!(x, y);
}
#[test]
fn temp_shadowing_does_not_change_main_content_and_temp_objects_are_preserved() {
    let dir = TempDir::new().unwrap();
    let mut a = database(&dir.path().join("a.sqlite"));
    let mut b = database(&dir.path().join("b.sqlite"));
    a.execute_batch("CREATE TEMP TABLE journal_entries AS SELECT * FROM main.journal_entries; UPDATE temp.journal_entries SET title='TEMP ONLY';").unwrap();
    let (result, x, y) = fingerprints(&mut a, &mut b);
    assert_eq!(result, Comparison::Identical);
    assert_eq!(x, y);
    assert_eq!(
        a.query_row("SELECT title FROM temp.journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "TEMP ONLY"
    );
}
#[test]
fn memory_temp_store_is_rejected_without_changing_the_caller_transaction_or_data() {
    let mut db = Connection::open_in_memory().unwrap();
    initialize_schema(&db).unwrap();
    db.execute_batch("PRAGMA temp_store=MEMORY; CREATE TEMP TABLE retained(value); INSERT INTO retained VALUES ('keep'); BEGIN;").unwrap();
    assert!(matches!(
        VerifiedJournal::verify(&mut db, cancel()),
        Err(ContentError::Database(PrepareError::ResourceLimit))
    ));
    assert!(!db.is_autocommit());
    assert_eq!(
        db.query_row("SELECT value FROM retained", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    db.execute_batch("ROLLBACK").unwrap();
}

// These tests alone access private connection state to inject a deterministic interrupt.
fn streaming_source() -> (TempDir, Connection) {
    let dir = TempDir::new().unwrap();
    let db = database(&dir.path().join("stream.sqlite"));
    for i in 0..20 {
        db.execute("INSERT INTO journal_entries SELECT ?1,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries WHERE id='one'", [format!("extra{i}")]).unwrap();
    }
    (dir, db)
}
#[test]
fn progress_handler_cancellation_during_streaming_is_cancelled_not_resource_limit() {
    let (_dir, mut db) = streaming_source();
    let cancellation = cancel();
    let snapshot = VerifiedJournal::verify(&mut db, cancellation.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let token = cancellation.clone();
    // Test-only access to the pinned connection, no timing or worker scheduling.
    // SQLITE_INTERRUPT is emitted by SQLite while stepping the content SELECT.
    snapshot
        .database
        .progress_handler(
            1,
            Some(move || {
                if called.fetch_add(1, Ordering::AcqRel) == 100 {
                    token.store(true, Ordering::Release);
                    true
                } else {
                    false
                }
            }),
        )
        .unwrap();
    assert_eq!(
        fingerprint(&snapshot),
        Err(ContentError::Database(PrepareError::Cancelled))
    );
    assert!(calls.load(Ordering::Acquire) > 100);
    snapshot.finish().unwrap();
    assert!(db.is_autocommit());
}

#[test]
fn comparison_classifies_midstream_sqlite_interrupt_as_cancellation() {
    let (_dir, mut db) = streaming_source();
    let cancellation = cancel();
    let snapshot = VerifiedJournal::verify(&mut db, cancellation.clone()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let token = cancellation.clone();
    snapshot
        .database
        .progress_handler(
            1,
            Some(move || {
                if called.fetch_add(1, Ordering::AcqRel) == 100 {
                    token.store(true, Ordering::Release);
                    true
                } else {
                    false
                }
            }),
        )
        .unwrap();
    assert_eq!(
        compare(&snapshot, &snapshot),
        Err(ContentError::Database(PrepareError::Cancelled))
    );
    assert!(calls.load(Ordering::Acquire) > 100);
    snapshot.finish().unwrap();
    assert!(db.is_autocommit());
}

#[test]
fn compiler_rejects_sql_access_and_snapshot_replacement() {
    // Compile the actual API files, not a hand-written substitute for VerifiedJournal.
    // Archive preparation and the destination-capability implementation are stubbed;
    // no SQL is executed by this compiler probe.
    let dir = TempDir::new().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("src/backup/journal_content.rs")).unwrap();
    let declaration = "#[cfg(test)]\nmod tests;";
    let fixture = |source: &str| {
        let source = source.replace("\r\n", "\n");
        assert_eq!(source.matches(declaration).count(), 1);
        source.replace(declaration, "")
    };
    let lf = source.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    let api = fixture(&lf);
    assert_eq!(fixture(&crlf), api);
    assert!(!api.lines().any(|line| line.trim() == "mod tests;"));
    std::fs::write(dir.path().join("journal_content.rs"), api).unwrap();
    std::fs::create_dir(dir.path().join("journal_content")).unwrap();
    std::fs::copy(
        root.join("src/backup/journal_content/copy_handle.rs"),
        dir.path().join("journal_content/copy_handle.rs"),
    )
    .unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut externs = Vec::new();
    for name in ["rusqlite", "sha2", "serde"] {
        let prefix = format!("lib{name}-");
        let file = std::fs::read_dir(&deps)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&prefix)
                    && p.extension().is_some_and(|e| e == "rlib")
            })
            .max_by_key(|p| std::fs::metadata(p).unwrap().modified().unwrap())
            .unwrap();
        externs.push(format!("{name}={}", file.display()));
    }
    let common = r#"
mod restore {
    use rusqlite::Connection;
    use std::sync::{Arc,atomic::AtomicBool};
    #[derive(Debug,Clone,Copy,PartialEq,Eq)]
    pub enum PrepareError { Cancelled, ResourceLimit, IncompatibleSchema }
    pub struct PreparedRestore;
    impl PreparedRestore { pub fn source(&self)->Result<&Connection,PrepareError> { unimplemented!() } }
    pub fn sql_error(_:rusqlite::Error)->PrepareError { unimplemented!() }
    pub fn harden_open_connection(_: &Connection,_:Arc<AtomicBool>)->Result<(),PrepareError> { unimplemented!() }
    pub fn validate_database(_: &Connection,_:&AtomicBool)->Result<(),PrepareError> { unimplemented!() }
}
mod isolated_copy {
    pub struct CopyDestination<'a> { connection: &'a mut rusqlite::Connection }
    impl<'a> CopyDestination<'a> { pub fn into_connection(self)-> &'a mut rusqlite::Connection { self.connection } }
}
mod journal_content;
"#;
    let compile = |probe: &str| {
        let source = dir.path().join("probe.rs");
        std::fs::write(&source, format!("{common}\n{probe}")).unwrap();
        let mut compiler =
            std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        compiler
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--cfg",
                "test",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg("-L")
            .arg(format!("dependency={}", deps.display()));
        for value in &externs {
            compiler.arg("--extern").arg(value);
        }
        compiler
            .arg(&source)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    // A positive control prevents missing dependencies or fixture errors from
    // masquerading as a successful compile-fail test.
    let baseline = compile(
        "fn allowed(s: &journal_content::VerifiedJournal<'_>, d: isolated_copy::CopyDestination<'_>) { let handle = s.start_copy(d).unwrap(); let _ = handle.step(); let _ = handle.finish(); }",
    );
    assert!(
        baseline.status.success(),
        "{}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    for (probe, diagnostic) in [
        (
            "fn forbidden(s: &journal_content::VerifiedJournal<'_>) { s.copy_source().unwrap().execute_batch(\"COMMIT; BEGIN\").unwrap(); }",
            "E0599",
        ),
        (
            "fn forbidden(s: &journal_content::VerifiedJournal<'_>) { s.database.execute_batch(\"COMMIT; BEGIN\").unwrap(); }",
            "E0616",
        ),
        (
            "fn forbidden(s: &journal_content::VerifiedJournal<'_>, d: isolated_copy::CopyDestination<'_>) { let h=s.start_copy(d).unwrap(); h.execute_batch(\"COMMIT; BEGIN\").unwrap(); }",
            "E0599",
        ),
        (
            "fn forbidden(d: &mut rusqlite::Connection) { let s=journal_content::VerifiedJournal::verify(d,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))).unwrap(); d.execute_batch(\"COMMIT; BEGIN\").unwrap(); let _=journal_content::fingerprint(&s); }",
            "E0502",
        ),
    ] {
        let result = compile(probe);
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(diagnostic), "{stderr}");
    }
}
