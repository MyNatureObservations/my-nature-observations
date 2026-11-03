use super::*;
use rusqlite::{Connection, params, types::Value};

const MANUAL: &str =
    r#"{"period":{"kind":"day","date":"2026-10-10"},"temperatureC":0,"precipitationMm":null}"#;
const FETCHED: &str = r#"{"period":{"kind":"day","date":"2026-10-10"},"location":{"name":"Synthetic","coordinates":{"latitude":0,"longitude":0}},"timezone":"UTC","source":{"provider":"open-meteo","product":"historical-weather","model":"ERA5","dataKind":"reanalysis","attribution":"Open-Meteo","license":"CC-BY-4.0"},"retrievedAt":"2026-10-11T09:00:00Z","temperatureMeanC":0,"precipitationSumMm":0}"#;

fn schema1() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema1.sqlite");
    crate::database::create_new_database(&path).unwrap();
    let db = Connection::open(path).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('entry-a','legacy date',' title ','{ \"old\":true }',1,'created','updated')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('entry-b','2026-10-10','','{}',1,'c2','u2')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_weather VALUES ('entry-a',1,?1,?2,7)",
        params![MANUAL, FETCHED],
    )
    .unwrap();
    (dir, db)
}

fn journal_rows(db: &Connection) -> Vec<Vec<Value>> {
    let mut statement = db
        .prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id")
        .unwrap();
    statement
        .query_map([], |r| (0..7).map(|i| r.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn weather_rows(db: &Connection) -> Vec<Vec<Value>> {
    let mut statement = db
        .prepare("SELECT entry_id,format_version,manual_json,fetched_json,revision FROM journal_weather ORDER BY entry_id")
        .unwrap();
    statement
        .query_map([], |r| (0..5).map(|i| r.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn schema1_migration_is_additive_preserves_journal_and_both_weather_sources() {
    let (_dir, mut db) = schema1();
    let journal_before = journal_rows(&db);
    let weather_before = weather_rows(&db);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );

    migrate_candidate(&mut db).unwrap();

    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(validate_schema2(&db).unwrap());
    assert_eq!(journal_rows(&db), journal_before);
    assert_eq!(weather_rows(&db), weather_before);
    assert_eq!(
        db.query_row("SELECT count(*) FROM places", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_places", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let violations: Vec<(String, i64, String, i64)> = db
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(violations.is_empty());
}

#[test]
fn repeated_schema2_migration_is_a_validated_noop() {
    let (_dir, mut db) = schema1();
    migrate_candidate(&mut db).unwrap();
    let before = journal_rows(&db);
    let weather = weather_rows(&db);
    migrate_candidate(&mut db).unwrap();
    assert_eq!(journal_rows(&db), before);
    assert_eq!(weather_rows(&db), weather);
    assert!(validate_schema2(&db).unwrap());
}

#[test]
fn schema2_migration_rolls_back_all_ddl_and_version_on_interruption() {
    let (_dir, mut db) = schema1();
    let journal_before = journal_rows(&db);
    let weather_before = weather_rows(&db);
    let result = before_commit_for_test(&mut db, |tx| {
        assert!(
            tx.query_row("SELECT count(*) FROM places", [], |r| r.get::<_, i64>(0))
                .is_ok()
        );
        Err("injected interruption".into())
    });
    assert!(result.is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(crate::database::validate_supported_schema(&db).unwrap());
    assert_eq!(journal_rows(&db), journal_before);
    assert_eq!(weather_rows(&db), weather_before);
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('places','journal_places','idx_journal_places_place_id')", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn normal_database_creation_and_supported_schema_remain_schema1() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("normal.sqlite");
    crate::database::create_new_database(&path).unwrap();
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(crate::database::validate_supported_schema(&db).unwrap());
    assert!(!validate_schema2(&db).unwrap());
}
