use super::*;
use rusqlite::{OpenFlags, params, types::Value};
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
};
use tempfile::TempDir;

const DOC: &str = r#"{ "type": "doc", "content": [{"type":"paragraph","content":[{"type":"text","text":"Snö 🌿"}]}] }"#;
const MANUAL: &str = r#"{"period":{"kind":"day","date":"2026-10-17"},"description":"  Snö 🌿\n  ","temperatureC":0,"precipitationMm":null}"#;
const FETCHED: &str = r#"{"period":{"kind":"day","date":"2026-10-16"},"location":{"coordinates":{"latitude":0,"longitude":0}},"timezone":"UTC","source":{"provider":"open-meteo","product":"historical-weather","model":"ERA5","dataKind":"reanalysis","attribution":"Open-Meteo","license":"CC-BY-4.0"},"retrievedAt":"2026-10-23T09:30:00Z","temperatureMeanC":0,"precipitationSumMm":0}"#;
fn legacy(db: &Connection, filled: bool) {
    crate::database::initialize_schema(db).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    if filled {
        for (id, title) in [("entry-1", "Blåmes 🌿"), ("entry-2", "雪 — Åäö")] {
            db.execute("INSERT INTO main.journal_entries VALUES (?1,'2026-10-17',?2,?3,1,'2026-10-18T01:02:03.456Z','2026-10-19T02:03:04Z')",params![id,title,DOC]).unwrap();
        }
    }
}
fn rows(db: &Connection) -> Vec<Vec<Value>> {
    db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id").unwrap()
        .query_map([], |r| (0..7).map(|i|r.get(i)).collect()).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap()
}
fn version(db: &Connection) -> i64 {
    db.pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}
fn assert_legacy(db: &Connection, original: &[Vec<Value>]) {
    assert!(db.is_autocommit());
    assert_eq!(version(db), 0);
    assert!(crate::database::validate_schema(db).unwrap());
    assert_eq!(rows(db), original);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM main.sqlite_schema WHERE name='journal_weather'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
fn migrated() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    legacy(&db, true);
    migrate(&mut db).unwrap();
    db
}
fn insert(db: &Connection, manual: Option<&str>, fetched: Option<&str>) -> rusqlite::Result<usize> {
    db.execute(
        "INSERT INTO main.journal_weather VALUES ('entry-1',1,?1,?2,1)",
        params![manual, fetched],
    )
}

#[test]
fn empty_and_populated_migrations_preserve_all_journal_values() {
    for filled in [false, true] {
        let mut db = Connection::open_in_memory().unwrap();
        legacy(&db, filled);
        let before = rows(&db);
        migrate(&mut db).unwrap();
        assert_eq!(version(&db), 1);
        assert_eq!(rows(&db), before);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM journal_weather", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        validate(&mut db).unwrap();
        assert!(matches!(migrate(&mut db), Err(Error::Schema)));
        assert_eq!(rows(&db), before);
        assert!(!crate::database::validate_schema(&db).unwrap());
    }
}

#[test]
fn isolated_definition_has_exact_columns_fk_and_constraints() {
    let mut db = Connection::open_in_memory().unwrap();
    define_schema(&db).unwrap();
    validate(&mut db).unwrap();
    let columns: Vec<(String, String, i64, i64)> = db
        .prepare("PRAGMA main.table_info(journal_weather)")
        .unwrap()
        .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(5)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        columns,
        vec![
            ("entry_id".into(), "TEXT".into(), 1, 1),
            ("format_version".into(), "INTEGER".into(), 1, 0),
            ("manual_json".into(), "TEXT".into(), 0, 0),
            ("fetched_json".into(), "TEXT".into(), 0, 0),
            ("revision".into(), "INTEGER".into(), 1, 0)
        ]
    );
    let fk:(String,String,String,String)=db.query_row("SELECT \"table\",\"from\",\"to\",on_delete FROM pragma_foreign_key_list('journal_weather')",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(
        fk,
        (
            "journal_entries".into(),
            "entry_id".into(),
            "id".into(),
            "RESTRICT".into()
        )
    );
}

#[test]
fn valid_weather_parts_null_and_zero_are_preserved_without_reserialization() {
    for (manual, fetched) in [
        (None, None),
        (Some(MANUAL), None),
        (None, Some(FETCHED)),
        (Some(MANUAL), Some(FETCHED)),
    ] {
        let mut db = migrated();
        insert(&db, manual, fetched).unwrap();
        let before = rows(&db);
        for _ in 0..3 {
            validate(&mut db).unwrap();
        }
        let stored: (Option<String>, Option<String>, i64) = db
            .query_row(
                "SELECT manual_json,fetched_json,revision FROM journal_weather",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            stored,
            (manual.map(str::to_owned), fetched.map(str::to_owned), 1)
        );
        assert_eq!(rows(&db), before);
    }
}

#[test]
fn invalid_weather_uses_existing_contract_including_duplicate_fields() {
    let invalid = [
        "null",
        "[]",
        "{",
        r#"{"period":{"kind":"day","date":"2026-02-29"}}"#,
        r#"{"period":{"kind":"day","date":"2026-10-17"},"temperatureC":-274}"#,
        r#"{"period":{"kind":"day","date":"2026-10-17"},"temperatureC":1,"temperatureC":2}"#,
        r#"{"period":{"kind":"day","date":"2026-10-17"},"unknown":0}"#,
    ];
    for input in invalid {
        let mut db = migrated();
        insert(&db, Some(input), None).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::Content)), "{input}");
        assert_eq!(
            db.query_row("SELECT manual_json FROM journal_weather", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            input
        );
    }
    for input in [
        FETCHED.replace("\"latitude\":0", "\"latitude\":91"),
        FETCHED.replace("\"temperatureMeanC\":0", "\"temperatureMeanC\":1e999"),
        MANUAL.to_owned(),
        FETCHED.replace("\"reanalysis\"", "\"personalObservation\""),
    ] {
        let mut db = migrated();
        insert(&db, None, Some(&input)).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::Content)));
    }
}

#[test]
fn weather_size_limit_is_inclusive_and_does_not_change_bytes() {
    let prefix = r#"{"period":{"kind":"day","date":"2026-10-17"},"description":""#;
    for length in [WEATHER_PART_LIMIT, WEATHER_PART_LIMIT + 1] {
        let json = format!("{prefix}{}\"}}", "a".repeat(length - prefix.len() - 2));
        assert_eq!(json.len(), length);
        let mut db = migrated();
        insert(&db, Some(&json), None).unwrap();
        match validate(&mut db) {
            Ok(()) => assert_eq!(length, WEATHER_PART_LIMIT),
            Err(Error::ResourceLimit) => assert_eq!(length, WEATHER_PART_LIMIT + 1),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            db.query_row("SELECT manual_json FROM journal_weather", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            json
        );
    }
}

#[test]
fn foreign_keys_uniqueness_and_restrict_are_enforced_and_orphans_detected() {
    let mut db = migrated();
    assert!(
        db.execute(
            "INSERT INTO journal_weather VALUES('missing',1,NULL,NULL,1)",
            []
        )
        .is_err()
    );
    insert(&db, Some(MANUAL), None).unwrap();
    assert!(insert(&db, None, None).is_err());
    assert!(
        db.execute("DELETE FROM journal_entries WHERE id='entry-1'", [])
            .is_err()
    );
    assert!(
        db.execute("INSERT INTO journal_weather VALUES(NULL,1,NULL,NULL,1)", [])
            .is_err()
    );
    db.pragma_update(None, "foreign_keys", false).unwrap();
    db.execute("UPDATE journal_weather SET entry_id='missing'", [])
        .unwrap();
    assert!(matches!(validate(&mut db), Err(Error::Content)));
}

#[test]
fn malformed_stored_types_versions_and_revisions_are_rejected() {
    for assignment in [
        "format_version=2",
        "format_version=1.5",
        "revision=0",
        "revision=1.5",
        "revision='bad'",
        "manual_json=X'7b7d'",
        "fetched_json=0",
    ] {
        let mut db = migrated();
        insert(&db, None, None).unwrap();
        db.pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        db.execute_batch(&format!("UPDATE journal_weather SET {assignment}"))
            .unwrap();
        assert!(validate(&mut db).is_err(), "{assignment}");
    }
    for assignment in [
        "id=NULL",
        "document_version=2",
        "document_version=1.5",
        "content_json='[]'",
        "title=X'ff'",
    ] {
        let mut db = migrated();
        db.execute_batch(&format!(
            "UPDATE journal_entries SET {assignment} WHERE id='entry-1'"
        ))
        .unwrap();
        assert!(
            matches!(validate(&mut db), Err(Error::Content)),
            "{assignment}"
        );
    }
}

#[test]
fn schema_objects_missing_extra_altered_fk_and_unknown_versions_are_rejected() {
    for sql in [
        "CREATE TABLE extra(x)",
        "CREATE INDEX extra ON journal_weather(revision)",
        "CREATE VIEW extra AS SELECT * FROM journal_weather",
        "CREATE TRIGGER extra AFTER INSERT ON journal_weather BEGIN SELECT 1; END;",
        "DROP TABLE journal_weather",
        "DROP INDEX idx_journal_entries_date",
        "PRAGMA user_version=0",
        "PRAGMA user_version=2",
    ] {
        let mut db = migrated();
        db.execute_batch(sql).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::Schema)), "{sql}");
    }
    for ddl in [
        WEATHER_DDL.replace("ON DELETE RESTRICT", "ON DELETE CASCADE"),
        WEATHER_DDL.replace(
            "REFERENCES journal_entries(id)",
            "REFERENCES journal_entries(title)",
        ),
        WEATHER_DDL.replace("CHECK (revision >= 1)", "CHECK (revision >= 0)"),
    ] {
        let mut db = migrated();
        db.execute_batch("DROP TABLE journal_weather").unwrap();
        db.execute_batch(&ddl).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::Schema)));
    }
}

#[test]
fn migration_rejects_unknown_partial_schemas_disabled_fk_and_nested_transactions() {
    for sql in [
        "PRAGMA user_version=1",
        "PRAGMA user_version=99",
        "CREATE TABLE extra(x)",
        "DROP INDEX idx_journal_entries_date",
        "PRAGMA foreign_keys=OFF",
    ] {
        let mut db = Connection::open_in_memory().unwrap();
        legacy(&db, true);
        let before = rows(&db);
        db.execute_batch(sql).unwrap();
        let initial_version = version(&db);
        assert!(migrate(&mut db).is_err(), "{sql}");
        assert_eq!(version(&db), initial_version);
        assert_eq!(rows(&db), before);
    }
    let mut db = Connection::open_in_memory().unwrap();
    legacy(&db, true);
    db.execute_batch("BEGIN").unwrap();
    assert!(matches!(migrate(&mut db), Err(Error::Schema)));
    assert!(!db.is_autocommit());
    db.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn failures_and_unwind_at_each_checkpoint_rollback_entire_migration() {
    for point in [
        Point::AfterBegin,
        Point::AfterTable,
        Point::AfterVersion,
        Point::BeforeCommit,
    ] {
        for panic in [false, true] {
            let mut db = Connection::open_in_memory().unwrap();
            legacy(&db, true);
            let before = rows(&db);
            let result = catch_unwind(AssertUnwindSafe(|| {
                migrate_with(&mut db, |here, _| {
                    if here == point {
                        if panic {
                            panic!("isolated migration fault");
                        }
                        return Err(Error::Content);
                    }
                    Ok(())
                })
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_legacy(&db, &before);
        }
    }
}

#[test]
fn transaction_recheck_and_final_validation_prevent_partial_or_invalid_commit() {
    for point in [Point::AfterBegin, Point::AfterVersion, Point::BeforeCommit] {
        let mut db = Connection::open_in_memory().unwrap();
        legacy(&db, true);
        let before = rows(&db);
        assert!(
            migrate_with(&mut db, |here, db| {
                if here == point {
                    db.execute_batch("CREATE TABLE unexpected(x)")?;
                }
                Ok(())
            })
            .is_err()
        );
        assert_legacy(&db, &before);
    }
    let mut db = Connection::open_in_memory().unwrap();
    legacy(&db, true);
    db.execute("UPDATE journal_entries SET content_json='[]'", [])
        .unwrap();
    let before = rows(&db);
    assert!(matches!(migrate(&mut db), Err(Error::Content)));
    assert_legacy(&db, &before);
}

#[test]
fn sqlite_interruption_rolls_back_and_does_not_leave_partial_schema() {
    let mut db = Connection::open_in_memory().unwrap();
    legacy(&db, true);
    let before = rows(&db);
    let result = migrate_with(&mut db, |point, db| {
        if point == Point::AfterTable {
            db.progress_handler(1, Some(|| true))?;
        }
        Ok(())
    });
    db.progress_handler(0, None::<fn() -> bool>).unwrap();
    match result {
        Err(Error::Sql(e)) => assert_eq!(
            e.sqlite_error_code(),
            Some(rusqlite::ErrorCode::OperationInterrupted)
        ),
        other => panic!("{other:?}"),
    }
    assert_legacy(&db, &before);
}

#[test]
fn busy_writer_cannot_partially_migrate() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    let before = rows(&db);
    let other = Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    match migrate(&mut db) {
        Err(Error::Sql(e)) => assert_eq!(
            e.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy)
        ),
        other => panic!("{other:?}"),
    }
    other.execute_batch("ROLLBACK").unwrap();
    assert_legacy(&db, &before);
}

#[test]
fn read_only_reopening_and_frozen_schema_one_validation_preserve_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    let before = rows(&db);
    migrate(&mut db).unwrap();
    insert(&db, Some(MANUAL), Some(FETCHED)).unwrap();
    drop(db);
    let physical = fs::read(&path).unwrap();
    for _ in 0..3 {
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        validate(&mut db).unwrap();
        assert_eq!(rows(&db), before);
        drop(db);
        assert_eq!(fs::read(&path).unwrap(), physical);
    }
    assert_eq!(crate::database::SCHEMA_VERSION, 4);
    crate::database::initialize_schema1_database_for_test(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), physical);
}

#[test]
fn interrupted_process_child() {
    let Some(root) = std::env::var_os("MNO_SCHEMA1_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let point = match std::env::var("MNO_SCHEMA1_TEST_POINT").unwrap().as_str() {
        "table" => Point::AfterTable,
        "version" => Point::AfterVersion,
        _ => panic!("invalid isolated checkpoint"),
    };
    let mut db = Connection::open(root.join("isolated.sqlite")).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    migrate_with(&mut db, |here, _| {
        if here == point {
            fs::write(root.join("checkpoint"), format!("{here:?}")).unwrap();
            // Deliberately no Drop/rollback: only this subprocess's temp fixture.
            std::process::exit(79);
        }
        Ok(())
    })
    .unwrap();
    panic!("checkpoint was not reached");
}

#[test]
fn process_exit_before_commit_recovers_original_schema_and_rows() {
    for point in ["table", "version"] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("isolated.sqlite");
        let db = Connection::open(&path).unwrap();
        legacy(&db, true);
        let before = rows(&db);
        drop(db);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "schema_v1::tests::interrupted_process_child",
                "--nocapture",
            ])
            .env("MNO_SCHEMA1_TEST_ROOT", dir.path())
            .env("MNO_SCHEMA1_TEST_POINT", point)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("migration subprocess timeout");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            status.code(),
            Some(79),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("checkpoint")).unwrap(),
            if point == "table" {
                "AfterTable"
            } else {
                "AfterVersion"
            }
        );
        // This historical schema prototype intentionally writes its disposable
        // fixture in place. SQLite recovery here is not production startup:
        // production rejects unknown active sidecars and migrates a private copy.
        let mut db = Connection::open(&path).unwrap();
        assert_legacy(&db, &before);
        crate::database::weather_schema::migrate(&mut db).unwrap();
        assert_eq!(rows(&db), before);
        drop(db);
    }
}

#[test]
fn busy_commit_rolls_back_instead_of_leaving_schema_one_pending() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let before = rows(&db);
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN DEFERRED").unwrap();
    assert_eq!(rows(&reader), before); // Real SHARED lock prevents COMMIT.
    let mut reached_commit = false;
    let result = migrate_with(&mut db, |point, _| {
        if point == Point::BeforeCommit {
            reached_commit = true;
        }
        Ok(())
    });
    assert!(reached_commit);
    match result {
        Err(Error::Sql(e)) => assert_eq!(
            e.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy)
        ),
        other => panic!("{other:?}"),
    }
    reader.execute_batch("ROLLBACK").unwrap();
    assert_legacy(&db, &before);
}

#[test]
fn rejected_read_only_weather_validation_preserves_stored_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    migrate(&mut db).unwrap();
    insert(&db, Some("null"), None).unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    for _ in 0..2 {
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::Content)));
        assert!(db.is_autocommit());
        drop(db);
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn strict_journal_failures_reject_validation_and_roll_back_migration() {
    for (column, bad) in [
        ("content_json", "{}"),
        ("content_json", r#"{"type":"unknown","content":[]}"#),
        (
            "content_json",
            r#"{"type":"doc","content":[{"type":"text","text":"wrong root child"}]}"#,
        ),
        (
            "content_json",
            r#"{"type":"doc","type":"doc","content":[{"type":"paragraph"}]}"#,
        ),
        (
            "content_json",
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a","text":"b"}]}]}"#,
        ),
        (
            "content_json",
            r#"{"type":"doc","content":[{"type":"image","attrs":{"src":"x"}}]}"#,
        ),
        ("id", ""),
        ("id", "with space"),
        ("id", "å"),
        ("id", "../entry"),
        ("entry_date", "2026-02-29"),
        ("entry_date", "2026-13-01"),
        ("entry_date", "2026-1-01"),
        ("entry_date", "not a date"),
    ] {
        let mut original = Connection::open_in_memory().unwrap();
        legacy(&original, true);
        original
            .execute(
                &format!("UPDATE journal_entries SET {column}=?1 WHERE id='entry-1'"),
                [bad],
            )
            .unwrap();
        let before = rows(&original);
        assert!(
            matches!(migrate(&mut original), Err(Error::Content)),
            "{column}: {bad}"
        );
        assert_legacy(&original, &before);
        let mut db = migrated();
        db.execute(
            &format!("UPDATE journal_entries SET {column}=?1 WHERE id='entry-1'"),
            [bad],
        )
        .unwrap();
        assert!(
            matches!(validate(&mut db), Err(Error::Content)),
            "{column}: {bad}"
        );
        assert_eq!(rows(&db), before);
    }
}

fn sized_weather(fetched: bool, size: usize, character: char) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(if fetched { FETCHED } else { MANUAL }).unwrap();
    let target = if fetched {
        "/source/attribution"
    } else {
        "/description"
    };
    *value.pointer_mut(target).unwrap() = "".into();
    let overhead = serde_json::to_string(&value).unwrap().len();
    let count = (size - overhead) / character.len_utf8();
    let remainder = (size - overhead) % character.len_utf8();
    *value.pointer_mut(target).unwrap() =
        (character.to_string().repeat(count) + &"a".repeat(remainder)).into();
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(json.len(), size);
    json
}

#[test]
fn both_weather_parts_use_utf8_bytes_at_and_across_the_limit() {
    for fetched in [false, true] {
        for ch in ['a', 'é', '🌿'] {
            for size in [
                WEATHER_PART_LIMIT - 1,
                WEATHER_PART_LIMIT,
                WEATHER_PART_LIMIT + 1,
            ] {
                let json = sized_weather(fetched, size, ch);
                let mut db = migrated();
                insert(
                    &db,
                    if fetched { None } else { Some(&json) },
                    if fetched { Some(&json) } else { None },
                )
                .unwrap();
                let column = if fetched {
                    "fetched_json"
                } else {
                    "manual_json"
                };
                let (bytes, characters): (i64, i64) = db
                    .query_row(
                        &format!(
                            "SELECT octet_length({column}),length({column}) FROM journal_weather"
                        ),
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(bytes, size as i64);
                if ch != 'a' {
                    assert!(characters < WEATHER_PART_LIMIT as i64);
                }
                if size <= WEATHER_PART_LIMIT {
                    validate(&mut db).unwrap();
                } else {
                    assert!(matches!(validate(&mut db), Err(Error::ResourceLimit)));
                }
                assert_eq!(
                    db.query_row(&format!("SELECT {column} FROM journal_weather"), [], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap(),
                    json
                );
            }
        }
    }
}

#[test]
fn oversized_weather_is_rejected_before_full_sqlite_column_access() {
    use rusqlite::limits::Limit;
    for fetched in [false, true] {
        let mut db = migrated();
        let json = sized_weather(fetched, 8 * 1024 * 1024, 'é');
        insert(
            &db,
            if fetched { None } else { Some(&json) },
            if fetched { Some(&json) } else { None },
        )
        .unwrap();
        // Isolated negative control: a full column read now fails SQLITE_TOOBIG,
        // while metadata-only rejection must still reach our ResourceLimit.
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)
            .unwrap();
        assert!(matches!(validate(&mut db), Err(Error::ResourceLimit)));
        let column = if fetched {
            "fetched_json"
        } else {
            "manual_json"
        };
        let err = db
            .query_row(&format!("SELECT {column} FROM journal_weather"), [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap_err();
        assert_eq!(err.sqlite_error_code(), Some(rusqlite::ErrorCode::TooBig));
        assert!(db.is_autocommit());
    }
}

#[test]
fn metadata_plan_guard_rejects_an_expression_that_loads_the_payload() {
    let db = migrated();
    require_metadata_plan(&db, JOURNAL_METADATA).unwrap();
    require_metadata_plan(&db, WEATHER_METADATA).unwrap();
    assert!(matches!(
        require_metadata_plan(
            &db,
            "SELECT typeof(manual_json),octet_length(manual_json || '') FROM main.journal_weather"
        ),
        Err(Error::UnsafeMetadataPlan)
    ));
    assert!(matches!(
        require_metadata_plan(&db, "SELECT manual_json FROM main.journal_weather"),
        Err(Error::UnsafeMetadataPlan)
    ));
    assert!(matches!(
        require_metadata_plan(&db, "SELECT length(manual_json) FROM main.journal_weather"),
        Err(Error::UnsafeMetadataPlan)
    ));
    assert_eq!(
        db.query_row("SELECT octet_length('é🌿'),length('é🌿')", [], |r| Ok(
            (r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)
        ))
        .unwrap(),
        (6, 2)
    );
}

#[test]
fn utf16_is_rejected_without_conversion_or_migration() {
    for encoding in ["UTF-16le", "UTF-16be"] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("isolated.sqlite");
        let mut db = Connection::open(&path).unwrap();
        db.pragma_update(None, "encoding", encoding).unwrap();
        legacy(&db, true);
        let before = rows(&db);
        assert!(crate::database::validate_schema(&db).unwrap());
        assert!(matches!(migrate(&mut db), Err(Error::UnsupportedEncoding)));
        assert_legacy(&db, &before);
        define_weather_for_encoding_test(&db);
        assert!(matches!(validate(&mut db), Err(Error::UnsupportedEncoding)));
        let stored_encoding: String = db
            .pragma_query_value(None, "encoding", |r| r.get(0))
            .unwrap();
        assert_eq!(stored_encoding.to_lowercase(), encoding.to_lowercase());
        drop(db);
        let bytes = fs::read(&path).unwrap();
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert!(matches!(validate(&mut db), Err(Error::UnsupportedEncoding)));
        drop(db);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
fn define_weather_for_encoding_test(db: &Connection) {
    // Fixture construction only; the migration itself has already refused UTF-16.
    db.execute_batch(WEATHER_DDL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
}

#[test]
fn large_complex_valid_journal_is_not_restricted_to_weather_size() {
    use rusqlite::limits::Limit;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    let long_text = "Å 🌿 ".repeat(150_000);
    let mut blocks =
        vec![serde_json::json!({"type":"paragraph","content":[{"type":"text","text":long_text}]})];
    for _ in 0..2_000 {
        blocks.push(serde_json::json!({"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Snö och sol","marks":[{"type":"bold"}]}]}));
    }
    let json = serde_json::to_string(&serde_json::json!({"type":"doc","content":blocks})).unwrap();
    assert!(json.len() > 10 * WEATHER_PART_LIMIT && json.len() < JOURNAL_JSON_LIMIT);
    db.execute(
        "UPDATE journal_entries SET content_json=?1 WHERE id='entry-1'",
        [&json],
    )
    .unwrap();
    let before = rows(&db);
    let old_limit = db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap();
    migrate(&mut db).unwrap();
    validate(&mut db).unwrap();
    assert_eq!(rows(&db), before);
    assert_eq!(db.limit(Limit::SQLITE_LIMIT_LENGTH).unwrap(), old_limit);
}

#[test]
fn original_document_resource_budget_is_retained() {
    let mut db = Connection::open_in_memory().unwrap();
    legacy(&db, true);
    let text = r#"{"type":"text","text":"x"}"#;
    let json = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{}]}}]}}"#,
        vec![text; 340_000].join(",")
    );
    assert!(json.len() < JOURNAL_JSON_LIMIT);
    db.execute(
        "UPDATE journal_entries SET content_json=?1 WHERE id='entry-1'",
        [&json],
    )
    .unwrap();
    let before = rows(&db);
    assert!(matches!(migrate(&mut db), Err(Error::ResourceLimit)));
    assert_legacy(&db, &before);
}

#[test]
fn precheck_and_payload_reads_share_the_same_snapshot_during_external_change() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("isolated.sqlite");
    let mut db = Connection::open(&path).unwrap();
    legacy(&db, true);
    migrate(&mut db).unwrap();
    insert(&db, Some(MANUAL), None).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    let writer = Connection::open(&path).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .unwrap();
    precheck_payloads(&tx).unwrap(); // Establishes the original read snapshot.
    let oversized = sized_weather(false, 2 * WEATHER_PART_LIMIT, '🌿');
    writer
        .execute("UPDATE journal_weather SET manual_json=?1", [&oversized])
        .unwrap();
    validate_snapshot(&tx).unwrap();
    assert_eq!(
        tx.query_row("SELECT manual_json FROM journal_weather", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        MANUAL
    );
    tx.rollback().unwrap();
    assert!(matches!(validate(&mut db), Err(Error::ResourceLimit)));
}

#[test]
fn huge_non_text_weather_is_rejected_before_full_value_access() {
    use rusqlite::limits::Limit;
    let mut db = migrated();
    insert(&db, None, None).unwrap();
    db.execute_batch("UPDATE journal_weather SET fetched_json=zeroblob(8388608)")
        .unwrap();
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)
        .unwrap();
    assert!(matches!(validate(&mut db), Err(Error::Content)));
    let error = db
        .query_row("SELECT fetched_json FROM journal_weather", [], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .unwrap_err();
    assert_eq!(error.sqlite_error_code(), Some(rusqlite::ErrorCode::TooBig));
}

// Cross-check migration crash states against the actual startup coordinator.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "tests/startup.rs"]
mod startup;
