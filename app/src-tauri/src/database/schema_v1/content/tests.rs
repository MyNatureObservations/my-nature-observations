use super::*;
use rusqlite::{params, types::Value};
use std::panic::{AssertUnwindSafe, catch_unwind};
use tempfile::TempDir;

const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Snö 🌿"}]}]}"#;
const MANUAL: &str =
    r#"{"period":{"kind":"day","date":"2026-10-17"},"description":"Snö 🌿","temperatureC":0}"#;
const FETCHED: &str = r#"{"period":{"kind":"day","date":"2026-10-16"},"location":{"coordinates":{"latitude":0,"longitude":0}},"timezone":"UTC","source":{"provider":"open-meteo","product":"historical-weather","model":"ERA5","dataKind":"reanalysis","attribution":"Open-Meteo","license":"CC-BY-4.0"},"retrievedAt":"2026-10-23T09:30:00Z","temperatureMeanC":0,"precipitationSumMm":0}"#;
fn empty() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    super::super::define_schema(&db).unwrap();
    db
}
fn journal(db: &Connection, id: &str) {
    db.execute("INSERT INTO main.journal_entries VALUES (?1,'2026-10-17','Blåmes',?2,1,'created','updated')", params![id, DOC]).unwrap();
}
fn weather(db: &Connection, id: &str, manual: Option<&str>, fetched: Option<&str>) {
    db.execute(
        "INSERT INTO main.journal_weather VALUES (?1,1,?2,?3,1)",
        params![id, manual, fetched],
    )
    .unwrap();
}
fn digest(db: &mut Connection) -> [u8; 32] {
    let result = fingerprint(db, &AtomicBool::new(false)).unwrap();
    assert_eq!((result.content_version, result.schema_version), (2, 1));
    assert!(db.is_autocommit());
    result.digest
}
// Independent test oracle: collects a byte vector, uses owned SQLite values,
// explicit literal protocol descriptors, and never calls the streaming encoder.
fn reference_value(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => vec![0],
        Value::Integer(n) => [vec![1], n.to_be_bytes().to_vec()].concat(),
        Value::Real(n) => [vec![2], n.to_bits().to_be_bytes().to_vec()].concat(),
        Value::Text(s) => [
            vec![3],
            (s.len() as u64).to_be_bytes().to_vec(),
            s.as_bytes().to_vec(),
        ]
        .concat(),
        Value::Blob(b) => [vec![4], (b.len() as u64).to_be_bytes().to_vec(), b.clone()].concat(),
    }
}
fn reference(db: &Connection) -> [u8; 32] {
    let mut stream = b"MNO-database-content\0\0\0\0\x02\0\0\0\x01\0\0\0\x02".to_vec();
    for (table, fields, key) in [
        (
            "journal_entries",
            "id,entry_date,title,content_json,document_version,created_at,updated_at",
            "id",
        ),
        (
            "journal_weather",
            "entry_id,format_version,manual_json,fetched_json,revision",
            "entry_id",
        ),
    ] {
        stream.push(b'T');
        stream.extend((table.len() as u32).to_be_bytes());
        stream.extend(table.bytes());
        let names: Vec<_> = fields.split(',').collect();
        stream.extend((names.len() as u32).to_be_bytes());
        for field in &names {
            stream.extend((field.len() as u32).to_be_bytes());
            stream.extend(field.bytes());
        }
        let mut query = db
            .prepare(&format!(
                "SELECT {fields} FROM main.{table} ORDER BY {key} COLLATE BINARY ASC"
            ))
            .unwrap();
        let records: Vec<Vec<Value>> = query
            .query_map([], |row| (0..names.len()).map(|i| row.get(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for record in &records {
            stream.push(b'R');
            for value in record {
                stream.extend(reference_value(value));
            }
        }
        stream.push(b'E');
        stream.extend((records.len() as u64).to_be_bytes());
    }
    stream.push(b'Z');
    Sha256::digest(stream).into()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn independent_reference_vectors() {
    let mut db = empty();
    assert_eq!(digest(&mut db), reference(&db));
    assert_eq!(
        hex(&digest(&mut db)),
        "90b5c57d1705b91b9f7b6437d1e9624deeae0eed367542c4d2ecf2b33572d7e8"
    );
    journal(&db, "a");
    assert_eq!(digest(&mut db), reference(&db));
    assert_eq!(
        hex(&digest(&mut db)),
        "73bcddb1c4f362272b664a7985bcd9307750ba66da5f96dda3aa84900c640031"
    );
    weather(&db, "a", Some(MANUAL), Some(FETCHED));
    assert_eq!(digest(&mut db), reference(&db));
    assert_eq!(
        hex(&digest(&mut db)),
        "39cd91aff6bcca2d6823160c45aba9a55b306a53a84e3150cb5346a344ab1ad3"
    );
}
#[test]
fn manual_fetched_both_and_absent_weather_are_distinct() {
    let mut results = Vec::new();
    for parts in [
        None,
        Some((None, None)),
        Some((Some(MANUAL), None)),
        Some((None, Some(FETCHED))),
        Some((Some(MANUAL), Some(FETCHED))),
    ] {
        let mut db = empty();
        journal(&db, "a");
        if let Some((manual, fetched)) = parts {
            weather(&db, "a", manual, fetched);
        }
        let value = digest(&mut db);
        assert_eq!(value, reference(&db));
        assert!(!results.contains(&value));
        results.push(value);
    }
}
#[test]
fn ordering_reopening_and_temp_shadow_do_not_change_main_digest() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.sqlite");
    let mut db = Connection::open(&path).unwrap();
    super::super::define_schema(&db).unwrap();
    for id in ["z", "A", "a"] {
        journal(&db, id);
        weather(&db, id, None, None);
    }
    let expected = digest(&mut db);
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    assert_eq!(digest(&mut db), expected);
    let mut other = empty();
    for id in ["a", "z", "A"] {
        journal(&other, id);
        weather(&other, id, None, None);
    }
    other.execute_batch("CREATE TEMP TABLE journal_entries (id); INSERT INTO temp.journal_entries VALUES ('shadow');").unwrap();
    assert_eq!(digest(&mut other), expected);
}
#[test]
fn every_valid_field_mutation_changes_digest() {
    for sql in [
        "UPDATE journal_entries SET id='b'",
        "UPDATE journal_entries SET entry_date='2026-10-18'",
        "UPDATE journal_entries SET title='other'",
        "UPDATE journal_entries SET content_json=' {\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}'",
        "UPDATE journal_entries SET created_at='other'",
        "UPDATE journal_entries SET updated_at='other'",
    ] {
        let mut db = empty();
        journal(&db, "a");
        let before = digest(&mut db);
        db.execute_batch(sql).unwrap();
        assert_ne!(digest(&mut db), before, "{sql}");
        assert_eq!(digest(&mut db), reference(&db));
    }
    for sql in [
        "UPDATE journal_weather SET entry_id='b'",
        "UPDATE journal_weather SET manual_json=NULL",
        "UPDATE journal_weather SET fetched_json=NULL",
        "UPDATE journal_weather SET revision=2",
    ] {
        let mut db = empty();
        journal(&db, "a");
        journal(&db, "b");
        weather(&db, "a", Some(MANUAL), Some(FETCHED));
        let before = digest(&mut db);
        db.execute_batch(sql).unwrap();
        assert_ne!(digest(&mut db), before, "{sql}");
        assert_eq!(digest(&mut db), reference(&db));
    }
}
#[test]
fn fixed_version_fields_are_encoded_but_invalid_changes_are_rejected() {
    for (table, column) in [
        ("journal_entries", "document_version"),
        ("journal_weather", "format_version"),
    ] {
        let mut db = empty();
        journal(&db, "a");
        weather(&db, "a", None, None);
        let before = reference(&db);
        db.execute_batch(&format!(
            "PRAGMA ignore_check_constraints=ON; UPDATE {table} SET {column}=2"
        ))
        .unwrap();
        assert_ne!(reference(&db), before);
        assert!(fingerprint(&mut db, &AtomicBool::new(false)).is_err());
        assert!(db.is_autocommit());
    }
}
#[test]
fn scalar_codec_preserves_tags_lengths_and_float_bits() {
    let values = [
        Value::Null,
        Value::Text(String::new()),
        Value::Text("null".into()),
        Value::Integer(0),
        Value::Integer(-1),
        Value::Real(0.0),
        Value::Real(-0.0),
        Value::Blob(vec![]),
        Value::Blob(vec![0, 255]),
        Value::Text("é🌿".into()),
    ];
    let mut results = Vec::new();
    for value in values {
        let mut hash = Sha256::new();
        encode(&mut hash, ValueRef::from(&value), &AtomicBool::new(false)).unwrap();
        let actual: [u8; 32] = hash.finalize().into();
        let expected: [u8; 32] = Sha256::digest(reference_value(&value)).into();
        assert_eq!(actual, expected);
        assert!(!results.contains(&actual));
        results.push(actual);
    }
}
#[test]
fn raw_json_and_unicode_are_not_normalized() {
    let mut db = empty();
    journal(&db, "a");
    weather(&db, "a", Some(MANUAL), None);
    let before = digest(&mut db);
    db.execute(
        "UPDATE journal_weather SET manual_json=?1",
        [format!(" {MANUAL}\n")],
    )
    .unwrap();
    assert_ne!(digest(&mut db), before);
    assert_eq!(digest(&mut db), reference(&db));
    let before = digest(&mut db);
    db.execute(
        "UPDATE journal_entries SET content_json=?1",
        [DOC.replace("Snö", r"Sn\u00f6")],
    )
    .unwrap();
    assert_ne!(digest(&mut db), before);
    assert_eq!(digest(&mut db), reference(&db));
}
#[test]
fn invalid_types_and_json_null_never_produce_a_fingerprint() {
    for sql in [
        "UPDATE journal_weather SET manual_json=x'7b7d'",
        "UPDATE journal_weather SET manual_json='null'",
        "UPDATE journal_weather SET manual_json=''",
        "UPDATE journal_entries SET title=x'00'",
        "UPDATE journal_entries SET document_version=1.5",
    ] {
        let mut db = empty();
        journal(&db, "a");
        weather(&db, "a", None, None);
        db.execute_batch(sql).unwrap();
        assert!(
            fingerprint(&mut db, &AtomicBool::new(false)).is_err(),
            "{sql}"
        );
        assert!(db.is_autocommit());
    }
}
#[test]
fn large_valid_values_match_reference_and_sizes_are_not_global() {
    let mut db = empty();
    journal(&db, "a");
    let doc = DOC.replace("Snö 🌿", &"é".repeat(600_000));
    db.execute("UPDATE journal_entries SET content_json=?1", [doc])
        .unwrap();
    let prefix = r#"{"period":{"kind":"day","date":"2026-10-17"},"description":""#;
    let manual = format!(
        "{prefix}{}\"}}",
        "x".repeat(super::super::WEATHER_PART_LIMIT - prefix.len() - 2)
    );
    assert_eq!(manual.len(), 65536);
    weather(&db, "a", Some(&manual), None);
    assert_eq!(digest(&mut db), reference(&db));
    db.execute(
        "UPDATE journal_weather SET manual_json=?1",
        [format!("{manual} ")],
    )
    .unwrap();
    assert!(matches!(
        fingerprint(&mut db, &AtomicBool::new(false)),
        Err(Error::Validation(ValidationError::ResourceLimit))
    ));
    assert!(db.is_autocommit());
}
#[test]
fn wal_writes_between_tables_do_not_mix_snapshots() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wal.sqlite");
    let mut db = Connection::open(&path).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    super::super::define_schema(&db).unwrap();
    journal(&db, "a");
    weather(&db, "a", None, None);
    let before = digest(&mut db);
    let writer = Connection::open(&path).unwrap();
    let actual=fingerprint_with(&mut db,&AtomicBool::new(false),|point,_| {
        if point==Point::JournalHashed { writer.execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='new'; UPDATE journal_weather SET revision=2; COMMIT;")?; }
        Ok(())
    }).unwrap();
    assert_eq!(actual.digest, before);
    assert_ne!(digest(&mut db), before);
    assert_eq!(digest(&mut db), reference(&db));
}
#[test]
fn existing_transaction_is_preserved_and_rejected() {
    let mut db = empty();
    db.execute_batch("BEGIN").unwrap();
    assert!(matches!(
        fingerprint(&mut db, &AtomicBool::new(false)),
        Err(Error::ExistingTransaction)
    ));
    assert!(!db.is_autocommit());
    db.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn cancellation_at_each_checkpoint_and_before_start_leaves_no_transaction() {
    let mut db = empty();
    journal(&db, "a");
    assert!(matches!(
        fingerprint(&mut db, &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
    for at in [Point::Validated, Point::JournalHashed, Point::Hashed] {
        let cancel = AtomicBool::new(false);
        assert!(matches!(
            fingerprint_with(&mut db, &cancel, |point, _| {
                if point == at {
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            }),
            Err(Error::Cancelled)
        ));
        assert!(db.is_autocommit());
    }
}
#[test]
fn sql_error_unwind_and_failed_termination_return_no_result() {
    let mut db = empty();
    journal(&db, "a");
    let error = fingerprint_with(&mut db, &AtomicBool::new(false), |point, conn| {
        if point == Point::Validated {
            conn.execute_batch("SELECT nonexistent_function()")?;
        }
        Ok(())
    });
    let Err(Error::Sql(sql)) = error else {
        panic!("expected SQL failure");
    };
    assert!(sql.to_string().contains("nonexistent_function"));
    assert!(db.is_autocommit());
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        fingerprint_with(&mut db, &AtomicBool::new(false), |point, _| {
            if point == Point::Hashed {
                panic!("diagnostic unwind");
            }
            Ok(())
        })
    }));
    assert!(unwind.is_err());
    assert!(db.is_autocommit());
    let result = fingerprint_with(&mut db, &AtomicBool::new(false), |point, conn| {
        if point == Point::Hashed {
            conn.execute_batch("ROLLBACK")?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(Error::Sql(_))));
    assert!(db.is_autocommit());
    digest(&mut db);
}
#[test]
fn unsafe_sort_is_rejected_and_fixed_queries_are_index_scans() {
    let db = empty();
    for query in [JOURNAL_ROWS, WEATHER_ROWS] {
        require_streaming_order(&db, query).unwrap();
    }
    assert!(matches!(
        require_streaming_order(
            &db,
            "SELECT content_json FROM main.journal_entries ORDER BY title"
        ),
        Err(Error::UnsafeSort)
    ));
}

#[test]
fn sqlite_interrupt_during_hashing_returns_no_result_and_releases_transaction() {
    use std::sync::Arc;
    let mut db = empty();
    for id in ["a", "b", "c"] {
        journal(&db, id);
    }
    let fired = Arc::new(AtomicBool::new(false));
    let signal = fired.clone();
    let result = fingerprint_with(&mut db, &AtomicBool::new(false), |point, conn| {
        if point == Point::Validated {
            let signal = signal.clone();
            conn.progress_handler(1, Some(move || !signal.swap(true, Ordering::AcqRel)))?;
        }
        Ok(())
    });
    db.progress_handler(0, None::<fn() -> bool>).unwrap();
    assert!(fired.load(Ordering::Acquire));
    assert!(result.is_err());
    // Interrupting a read VM does not permit a partial fingerprint. Explicit
    // rollback and rusqlite's rollback-on-Drop must leave the connection usable.
    assert!(db.is_autocommit());
    digest(&mut db);
}

#[test]
fn schema_zero_and_unknown_versions_are_rejected_without_changing_them() {
    for version in [0, 2, 99] {
        let mut db = empty();
        db.pragma_update(None, "user_version", version).unwrap();
        assert!(matches!(
            fingerprint(&mut db, &AtomicBool::new(false)),
            Err(Error::Validation(ValidationError::Schema))
        ));
        assert!(db.is_autocommit());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
    }
}

#[test]
fn successful_read_does_not_change_database_bytes_or_connection_settings() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("readonly.sqlite");
    let mut db = Connection::open(&path).unwrap();
    super::super::define_schema(&db).unwrap();
    journal(&db, "a");
    weather(&db, "a", Some(MANUAL), Some(FETCHED));
    let before = std::fs::read(&path).unwrap();
    let changes = db.total_changes();
    let foreign_keys: bool = db
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    digest(&mut db);
    assert_eq!(db.total_changes(), changes);
    assert_eq!(
        db.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))
            .unwrap(),
        foreign_keys
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn persistent_sqlite_interrupt_blocks_cleanup_but_never_returns_a_result() {
    let mut db = empty();
    journal(&db, "a");
    let before = reference(&db);
    let result = fingerprint_with(&mut db, &AtomicBool::new(false), |point, conn| {
        if point == Point::Validated {
            conn.progress_handler(1, Some(|| true))?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(Error::Sql(_))));
    // Negative control: this deliberately hostile caller-owned callback also
    // interrupts ROLLBACK. No successful result or claim of cleanup is allowed.
    assert!(!db.is_autocommit());
    db.progress_handler(0, None::<fn() -> bool>).unwrap();
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(digest(&mut db), before);
}
