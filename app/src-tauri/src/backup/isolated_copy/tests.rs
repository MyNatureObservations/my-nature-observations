use super::*;
use crate::{
    backup::{self, BackupKind},
    database::initialize_schema,
};
use std::fs;
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn source() -> (TempDir, Connection) {
    let dir = TempDir::new().unwrap();
    let db = Connection::open(dir.path().join("source.sqlite")).unwrap();
    initialize_schema(&db).unwrap();
    let doc = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"å".repeat(16000)}]}]}).to_string();
    for i in 0..12 {
        db.execute(
            "INSERT INTO journal_entries VALUES (?1,'2026-10-17','title',?2,1,'created','updated')",
            rusqlite::params![format!("id{i}"), doc],
        )
        .unwrap();
    }
    (dir, db)
}
fn old(target: &TestDestination) {
    target.database.execute("INSERT INTO journal_entries VALUES ('old','2026-10-17','old','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',1,'created','updated')", []).unwrap();
}
#[test]
fn exact_copy_reopens_and_removes_old_rows_without_changing_source() {
    let (dir, mut db) = source();
    let before = fs::read(dir.path().join("source.sqlite")).unwrap();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let hash = journal_content::fingerprint(&snapshot).unwrap();
    let mut target = TestDestination::new();
    old(&target);
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Completed {
            cancel_requested: false
        }
    );
    assert_eq!(
        target
            .database
            .pragma_query_value(Some("main"), "synchronous", |r| r.get::<_, i32>(0))
            .unwrap(),
        3
    );
    #[cfg(target_os = "macos")]
    assert!(
        target
            .database
            .pragma_query_value(None, "fullfsync", |r| r.get::<_, bool>(0))
            .unwrap()
    );
    let TestDestination {
        directory,
        database,
    } = target;
    database.close().unwrap();
    let mut reopened = Connection::open_with_flags(
        directory.path().join("destination.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let result = VerifiedJournal::verify(&mut reopened, flag()).unwrap();
    assert_eq!(
        journal_content::compare(&snapshot, &result).unwrap(),
        Comparison::Identical
    );
    assert_eq!(journal_content::fingerprint(&result).unwrap(), hash);
    result.finish().unwrap();
    snapshot.finish().unwrap();
    assert_eq!(fs::read(dir.path().join("source.sqlite")).unwrap(), before);
}
#[test]
fn cancellation_before_middle_last_page_and_after_commit_are_distinct() {
    for when in 0..5 {
        let (_dir, mut db) = source();
        let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
        let mut target = TestDestination::new();
        old(&target);
        let path = target.directory.path().join("destination.sqlite");
        let before = fs::read(&path).unwrap();
        let cancel = flag();
        if when == 0 {
            cancel.store(true, Ordering::Release);
        }
        let mut steps = 0;
        let outcome = copy(
            &snapshot,
            &mut target,
            cancel.clone(),
            |point| match point {
                Point::BeforeStep if when == 4 => cancel.store(true, Ordering::Release),
                Point::AfterStep { remaining } => {
                    steps += 1;
                    if (when == 1 && steps == 2) || (when == 2 && remaining == 1) {
                        cancel.store(true, Ordering::Release);
                    }
                }
                Point::Committed if when == 3 => cancel.store(true, Ordering::Release),
                _ => {}
            },
        );
        if when == 3 {
            assert_eq!(
                outcome,
                Outcome::Completed {
                    cancel_requested: true
                }
            );
            let result = VerifiedJournal::verify(&mut target.database, flag()).unwrap();
            assert_eq!(
                journal_content::compare(&snapshot, &result).unwrap(),
                Comparison::Identical
            );
        } else {
            assert_eq!(outcome, Outcome::Unchanged(Failure::Cancelled));
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}
#[test]
fn busy_step_rolls_back_and_preserves_destination() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    old(&target);
    let lock = Connection::open(target.directory.path().join("destination.sqlite")).unwrap();
    let mut held = false;
    let result = copy(&snapshot, &mut target, flag(), |point| {
        if point == Point::BeforeStep && !held {
            lock.execute_batch("BEGIN IMMEDIATE").unwrap();
            held = true;
        }
    });
    assert!(matches!(
        result,
        Outcome::Unchanged(Failure::Busy) | Outcome::Uncertain(Failure::Busy)
    ));
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        target
            .database
            .query_row("SELECT id FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "old"
    );
    assert!(target.database.is_autocommit());
}
#[test]
fn rollback_source_snapshot_blocks_an_external_commit_until_released() {
    let (dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let writer = Connection::open(dir.path().join("source.sqlite")).unwrap();
    writer.busy_timeout(Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='new';")
        .unwrap();
    let error = writer.execute_batch("COMMIT").unwrap_err();
    assert_eq!(super::error(error), Failure::Busy);
    let mut target = TestDestination::new();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Completed {
            cancel_requested: false
        }
    );
    snapshot.finish().unwrap();
    writer.execute_batch("COMMIT").unwrap();
}
#[test]
fn wal_source_and_destination_are_rejected_without_conversion() {
    let (_dir, mut db) = source();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Unchanged(Failure::Source)
    );
    snapshot.finish().unwrap();
    db.pragma_update(None, "journal_mode", "DELETE").unwrap();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    target
        .database
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Unchanged(Failure::Mode)
    );
    assert_eq!(
        target
            .database
            .pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
}
#[test]
fn missing_target_transaction_and_settings_fail_before_copy() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    target.database.execute_batch("BEGIN").unwrap();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Unchanged(Failure::Transaction)
    );
    target.database.execute_batch("ROLLBACK").unwrap();
    target
        .database
        .authorizer(Some(|_: rusqlite::hooks::AuthContext<'_>| {
            rusqlite::hooks::Authorization::Deny
        }))
        .unwrap();
    assert!(matches!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Unchanged(_)
    ));
    target
        .database
        .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>)
        .unwrap();
    let disk = std::mem::replace(&mut target.database, Connection::open_in_memory().unwrap());
    drop(disk);
    fs::remove_file(target.directory.path().join("destination.sqlite")).unwrap();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Unchanged(Failure::MissingTarget)
    );
    assert!(!target.directory.path().join("destination.sqlite").exists());
}
#[test]
fn real_sqlite_page_limit_failure_never_claims_unchanged_or_success() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    target
        .database
        .pragma_update(None, "max_page_count", 3)
        .unwrap();
    let result = copy(&snapshot, &mut target, flag(), |_| {});
    assert!(
        matches!(
            result,
            Outcome::Uncertain(Failure::Full)
                | Outcome::Uncertain(Failure::Finish(ffi::SQLITE_FULL))
        ),
        "{result:?}"
    );
    assert!(target.database.is_autocommit());
    target
        .database
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map(|v| assert_eq!(v, "ok"))
        .unwrap();
}
#[test]
fn late_finish_error_is_uncertain_even_when_complete_data_exists() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    let result = copy_inner(
        &snapshot,
        &mut target,
        flag(),
        |_| {},
        |real| {
            assert_eq!(real, ffi::SQLITE_OK);
            ffi::SQLITE_IOERR
        },
    );
    assert_eq!(
        result,
        Outcome::Uncertain(Failure::Finish(ffi::SQLITE_IOERR))
    );
    let verified = VerifiedJournal::verify(&mut target.database, flag()).unwrap();
    assert_eq!(
        journal_content::compare(&snapshot, &verified).unwrap(),
        Comparison::Identical
    );
    assert_eq!(classify(ffi::SQLITE_LOCKED), Failure::Locked);
    assert_eq!(classify(ffi::SQLITE_IOERR_WRITE), Failure::Io);
}
#[test]
fn prepared_candidate_is_used_without_reopening_archive() {
    let (dir, db) = source();
    let path = dir.path().join("selected.mno-backup");
    backup::create_backup(&db, &path, BackupKind::Manual).unwrap();
    let candidate = backup::restore::prepare_candidate(&path, flag()).unwrap();
    fs::remove_file(path).unwrap();
    let snapshot = VerifiedJournal::from_candidate(&candidate, flag()).unwrap();
    let mut target = TestDestination::new();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Completed {
            cancel_requested: false
        }
    );
}
#[test]
fn invalid_source_cannot_become_a_copy_token() {
    let (_dir, mut db) = source();
    db.execute("UPDATE journal_entries SET content_json='broken'", [])
        .unwrap();
    assert!(VerifiedJournal::verify(&mut db, flag()).is_err());
}
#[test]
fn post_commit_verification_error_never_reports_unchanged() {
    let (_dir, mut db) = source();
    let source_cancel = flag();
    let snapshot = VerifiedJournal::verify(&mut db, source_cancel.clone()).unwrap();
    let mut target = TestDestination::new();
    let result = copy(&snapshot, &mut target, flag(), |point| {
        if point == Point::Committed {
            source_cancel.store(true, Ordering::Release);
        }
    });
    assert_eq!(result, Outcome::Uncertain(Failure::Verification));
    assert_eq!(
        target
            .database
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        12
    );
}

#[test]
fn panic_during_partial_copy_closes_handle_and_rolls_back() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let mut target = TestDestination::new();
    old(&target);
    let before = fs::read(target.directory.path().join("destination.sqlite")).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        copy(&snapshot, &mut target, flag(), |point| {
            if matches!(point, Point::AfterStep { .. }) {
                panic!("controlled test panic");
            }
        })
    }));
    assert!(result.is_err());
    assert!(target.database.is_autocommit());
    assert_eq!(
        fs::read(target.directory.path().join("destination.sqlite")).unwrap(),
        before
    );
}

#[test]
fn attempted_external_snapshot_replacement_cannot_change_copy_or_comparison() {
    let (dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    let original = journal_content::fingerprint(&snapshot).unwrap();
    let writer = Connection::open(dir.path().join("source.sqlite")).unwrap();
    writer.busy_timeout(Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='replacement';")
        .unwrap();
    assert_eq!(
        super::error(writer.execute_batch("COMMIT").unwrap_err()),
        Failure::Busy
    );
    let mut target = TestDestination::new();
    assert_eq!(
        copy(&snapshot, &mut target, flag(), |_| {}),
        Outcome::Completed {
            cancel_requested: false
        }
    );
    let result = VerifiedJournal::verify(&mut target.database, flag()).unwrap();
    assert_eq!(journal_content::fingerprint(&snapshot).unwrap(), original);
    assert_eq!(journal_content::fingerprint(&result).unwrap(), original);
    assert_eq!(
        journal_content::compare(&snapshot, &result).unwrap(),
        Comparison::Identical
    );
    result.finish().unwrap();
    snapshot.finish().unwrap();
    writer.execute_batch("COMMIT").unwrap();
    let changed = VerifiedJournal::verify(&mut db, flag()).unwrap();
    assert_ne!(journal_content::fingerprint(&changed).unwrap(), original);
}

#[test]
fn destination_cleanup_closes_sqlite_before_removing_directory_on_success_and_error() {
    let (_dir, mut db) = source();
    let snapshot = VerifiedJournal::verify(&mut db, flag()).unwrap();
    for cancel in [false, true] {
        let mut target = TestDestination::new();
        let path = target.directory.path().to_owned();
        let cancellation = flag();
        cancellation.store(cancel, Ordering::Release);
        let result = copy(&snapshot, &mut target, cancellation, |_| {});
        assert!(matches!(
            result,
            Outcome::Completed { .. } | Outcome::Unchanged(Failure::Cancelled)
        ));
        drop(target);
        assert!(!path.exists());
    }
}
