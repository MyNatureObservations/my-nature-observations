//! Independent source updates. Revision 0 means no row; empty rows retain tombstones
//! so a stale client can never recreate deleted weather (ABA). No UI commands.
use crate::database_state::{DatabaseState, Snapshot};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
pub(crate) const PART_LIMIT: usize = 64 * 1024;
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct StoredWeather {
    pub manual_json: Option<String>,
    pub fetched_json: Option<String>,
    pub revision: i64,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum Source {
    Manual,
    Fetched,
}
pub(super) fn read(db: &Connection, entry: &str) -> Result<StoredWeather, String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.journal_entries WHERE id=?1)",
            [entry],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("weather_entry_missing".into());
    }
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version == 0 {
        if !crate::database::validate_schema(db).map_err(|e| e.to_string())? {
            return Err("database_schema_incompatible".into());
        }
        return Ok(StoredWeather {
            manual_json: None,
            fetched_json: None,
            revision: 0,
        });
    }
    match version {
        9 if crate::database::bird_requests::validate_schema(db).map_err(|e| e.to_string())? => {}
        8 if crate::database::bird_comments::validate_schema(db).map_err(|e| e.to_string())? => {}
        7 if crate::database::bird_visit_identity::validate_schema(db)
            .map_err(|e| e.to_string())? => {}
        1 => {}
        6 if crate::database::bird_visits::validate_schema(db).map_err(|e| e.to_string())? => {}
        5 if crate::database::bird_schema::validate_schema(db).map_err(|e| e.to_string())? => {}
        4 if crate::photographs::validate_schema(db).map_err(|e| e.to_string())? => {}
        3 if crate::database::weather_default_schema::validate_schema3(db)
            .map_err(|e| e.to_string())? => {}
        _ => return Err("database_schema_incompatible".into()),
    }
    validate_metadata(db, Some(entry))?;
    let data = db
        .query_row(
            "SELECT manual_json,fetched_json,revision FROM main.journal_weather WHERE entry_id=?1",
            [entry],
            |r| {
                Ok(StoredWeather {
                    manual_json: r.get(0)?,
                    fetched_json: r.get(1)?,
                    revision: r.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(StoredWeather {
            manual_json: None,
            fetched_json: None,
            revision: 0,
        });
    if data.revision == 0 {
        return Ok(data);
    }
    validate(
        entry,
        data.manual_json.as_deref(),
        data.fetched_json.as_deref(),
    )?;
    Ok(data)
}
pub(crate) fn validate(
    entry: &str,
    manual: Option<&str>,
    fetched: Option<&str>,
) -> Result<(), String> {
    if [manual, fetched]
        .into_iter()
        .flatten()
        .any(|p| p.len() > PART_LIMIT)
    {
        return Err("weather_resource_limit".into());
    }
    super::validate_stored_parts(entry, manual, fetched).map_err(|e| e.to_string())
}
pub(crate) fn load(
    state: &DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry: &str,
) -> Result<Snapshot<StoredWeather>, String> {
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let data = read(&tx, entry)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(operation.snapshot(data))
}
pub(crate) fn update(
    state: &DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry: &str,
    expected_revision: i64,
    source: Source,
    json: Option<&str>,
) -> Result<StoredWeather, String> {
    update_impl(
        state,
        generation,
        open,
        entry,
        expected_revision,
        source,
        json,
        false,
    )
}
pub(crate) fn update_manual(
    state: &DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry: &str,
    expected_revision: i64,
    json: Option<&str>,
) -> Result<StoredWeather, String> {
    update_impl(
        state,
        generation,
        open,
        entry,
        expected_revision,
        Source::Manual,
        json,
        true,
    )
}
fn update_impl(
    state: &DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry: &str,
    expected_revision: i64,
    source: Source,
    json: Option<&str>,
    enforce_editable: bool,
) -> Result<StoredWeather, String> {
    let _operation = state.operation(Some(generation))?;
    if entry.len() > crate::open_export::METADATA_LIMIT
        || json.is_some_and(|j| j.len() > PART_LIMIT)
    {
        return Err("weather_resource_limit".into());
    }
    let mut db = open()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let version: u32 = tx
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version == 0 {
        // Frozen legacy Recovery may admit schema 0. Upgrade on the next startup
        // through a private candidate, never as a side effect of a live edit.
        return Err("weather_schema_upgrade_requires_restart".into());
    }
    if enforce_editable {
        let original: Option<(String, i64)> = tx
            .query_row(
                "SELECT content_json, document_version FROM journal_entries WHERE id=?1",
                [entry],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let (document, version) = original.ok_or("weather_entry_missing")?;
        if !crate::photographs::document_editable(&tx, entry, version, &document) {
            return Err("journal_document_incompatible".into());
        }
    }
    let mut data = read(&tx, entry)?;
    if expected_revision < 0 || data.revision != expected_revision {
        return Err("weather_revision_conflict".into());
    }
    match source {
        Source::Manual => data.manual_json = json.map(str::to_owned),
        Source::Fetched => data.fetched_json = json.map(str::to_owned),
    }
    validate(
        entry,
        data.manual_json.as_deref(),
        data.fetched_json.as_deref(),
    )?;
    data.revision = data
        .revision
        .checked_add(1)
        .ok_or("weather_revision_overflow")?;
    tx.execute("INSERT INTO main.journal_weather (entry_id,format_version,manual_json,fetched_json,revision) VALUES (?1,1,?2,?3,?4) ON CONFLICT(entry_id) DO UPDATE SET manual_json=excluded.manual_json,fetched_json=excluded.fetched_json,revision=excluded.revision",params![entry,data.manual_json,data.fetched_json,data.revision]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_schema3_keeps_weather_contract1_operational() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        crate::database::create_current_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','2026-10-25','','{}',1,'','')",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let state = DatabaseState::ready_for_test();
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        let manual = r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0}"#;
        update(&state, 1, open, "e", 0, Source::Manual, Some(manual)).unwrap();
        assert_eq!(
            load(&state, 1, open, "e")
                .unwrap()
                .data
                .manual_json
                .as_deref(),
            Some(manual)
        );
    }

    #[test]
    fn independent_updates_conflicts_tombstones_and_generations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        crate::database::create_new_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','bad-date','', '{\"legacy\":true}',1,'','')",
            [],
        )
        .unwrap();
        drop(db);
        let state = DatabaseState::ready_for_test();
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        let manual = r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0,"precipitationMm":0,"cloudCoverPercent":0}"#;
        assert_eq!(load(&state, 1, open, "e").unwrap().data.revision, 0);
        update(&state, 1, open, "e", 0, Source::Manual, Some(manual)).unwrap();
        assert!(update(&state, 1, open, "e", 0, Source::Fetched, None).is_err());
        let saved = update(&state, 1, open, "e", 1, Source::Fetched, None).unwrap();
        assert_eq!(saved.manual_json.as_deref(), Some(manual));
        assert!(saved.fetched_json.is_none());
        update(&state, 1, open, "e", 2, Source::Manual, None).unwrap();
        assert_eq!(load(&state, 1, open, "e").unwrap().data.revision, 3);
        assert!(update(&state, 1, open, "e", 0, Source::Manual, Some(manual)).is_err());
        state.set_generation_for_test(2);
        assert_eq!(
            load(
                &state,
                1,
                || panic!("stale generation must not open database"),
                "e"
            )
            .err()
            .unwrap(),
            crate::database_state::GENERATION_CONFLICT
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WeatherRecord {
    pub format_version: u32,
    pub manual_json: Option<String>,
    pub fetched_json: Option<String>,
    pub revision: i64,
}
pub(crate) fn export_record(db: &Connection, entry: &str) -> Result<Option<WeatherRecord>, String> {
    db.query_row("SELECT format_version,manual_json,fetched_json,revision FROM main.journal_weather WHERE entry_id=?1",[entry],|r|Ok(WeatherRecord{format_version:r.get(0)?,manual_json:r.get(1)?,fetched_json:r.get(2)?,revision:r.get(3)?})).optional().map_err(|e|e.to_string())
}

// Direct-column typeof/octet_length checks reject oversized source parts before
// materializing their overflow pages. The bundled SQLite provides octet_length.
pub(crate) fn validate_metadata(db: &Connection, entry: Option<&str>) -> Result<(), String> {
    let sql = if entry.is_some() {
        "SELECT typeof(entry_id),octet_length(entry_id),typeof(format_version),CASE WHEN typeof(format_version)='integer' THEN format_version END,typeof(manual_json),octet_length(manual_json),typeof(fetched_json),octet_length(fetched_json),typeof(revision),CASE WHEN typeof(revision)='integer' THEN revision END FROM main.journal_weather WHERE entry_id=?1"
    } else {
        "SELECT typeof(entry_id),octet_length(entry_id),typeof(format_version),CASE WHEN typeof(format_version)='integer' THEN format_version END,typeof(manual_json),octet_length(manual_json),typeof(fetched_json),octet_length(fetched_json),typeof(revision),CASE WHEN typeof(revision)='integer' THEN revision END FROM main.journal_weather"
    };
    let mut statement = db.prepare(sql).map_err(|e| e.to_string())?;
    let mut rows = if let Some(entry) = entry {
        statement.query([entry])
    } else {
        statement.query([])
    }
    .map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let kind = |i| row.get::<_, String>(i).map_err(|e| e.to_string());
        let len = |i| row.get::<_, i64>(i).map_err(|e| e.to_string());
        if kind(0)? != "text"
            || kind(2)? != "integer"
            || len(3)? != 1
            || kind(8)? != "integer"
            || len(9)? < 1
        {
            return Err("weather_storage_invalid".into());
        }
        if len(1)? < 0 || len(1)? > crate::open_export::METADATA_LIMIT as i64 {
            return Err("weather_resource_limit".into());
        }
        for (kind_col, len_col) in [(4, 5), (6, 7)] {
            match kind(kind_col)?.as_str() {
                "null" => (),
                "text" => {
                    let bytes = len(len_col)?;
                    if bytes < 0 || bytes > PART_LIMIT as i64 {
                        return Err("weather_resource_limit".into());
                    }
                }
                _ => return Err("weather_storage_invalid".into()),
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    #[test]
    fn concurrent_writers_with_the_same_revision_cannot_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        crate::database::create_new_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','','','{}',1,'','')",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let state = Arc::new(DatabaseState::ready_for_test());
        let gate = Arc::new(Barrier::new(2));
        let threads:Vec<_>=[1,2].into_iter().map(|n|{let state=state.clone();let gate=gate.clone();let path=path.clone();std::thread::spawn(move||{
            let json=format!("{{\"period\":{{\"kind\":\"day\",\"date\":\"2026-10-25\"}},\"temperatureC\":{n}}}");
            update(&state,1,||{let db=Connection::open(&path).map_err(|e|e.to_string())?;gate.wait();Ok(db)},"e",0,Source::Manual,Some(&json))
        })}).collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| r
                    .as_ref()
                    .err()
                    .is_some_and(|e| e == "weather_revision_conflict"))
                .count(),
            1
        );
        let loaded = load(
            &state,
            1,
            || Connection::open(&path).map_err(|e| e.to_string()),
            "e",
        )
        .unwrap();
        assert_eq!(loaded.generation, 1);
        assert_eq!(loaded.data.revision, 1);
    }
    #[test]
    fn source_size_boundaries_and_revision_overflow_preserve_existing_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.sqlite");
        crate::database::create_new_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','','','{}',1,'','')",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let state = DatabaseState::ready_for_test();
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        let mut json =
            r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0}"#.to_owned();
        json.push_str(&" ".repeat(PART_LIMIT - json.len()));
        update(&state, 1, open, "e", 0, Source::Manual, Some(&json)).unwrap();
        let oversized = format!("{json} ");
        assert_eq!(
            update(
                &state,
                1,
                || panic!("oversized input must not open SQLite"),
                "e",
                1,
                Source::Manual,
                Some(&oversized)
            )
            .unwrap_err(),
            "weather_resource_limit"
        );
        let db = Connection::open(&path).unwrap();
        db.execute("UPDATE journal_weather SET revision=?1", [i64::MAX])
            .unwrap();
        db.close().unwrap();
        assert_eq!(
            update(&state, 1, open, "e", i64::MAX, Source::Manual, None).unwrap_err(),
            "weather_revision_overflow"
        );
        assert_eq!(
            load(&state, 1, open, "e").unwrap().data.manual_json,
            Some(json)
        );
    }
}
#[cfg(test)]
mod legacy_tests {
    use super::*;
    #[test]
    fn legacy_recovery_reads_absent_weather_and_requires_safe_startup_before_write() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("legacy.sqlite");
        crate::database::create_legacy_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','bad date','', '{\"unknown\":true}',1,'','')",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let state = DatabaseState::ready_for_test();
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        let before = std::fs::read(&path).unwrap();
        assert_eq!(load(&state, 1, open, "e").unwrap().data.revision, 0);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(update(&state, 1, open, "e", 0, Source::Manual, Some("{}")).is_err());
        assert!(crate::database::validate_schema(&Connection::open(&path).unwrap()).unwrap());
        let weather = r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0}"#;
        assert_eq!(
            update(&state, 1, open, "e", 0, Source::Manual, Some(weather)).unwrap_err(),
            "weather_schema_upgrade_requires_restart"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        #[cfg(not(windows))]
        crate::database::initialize_schema1_database_for_test(&path).unwrap();
        #[cfg(windows)]
        let (state, path) = {
            assert_eq!(
                crate::database::initialize_schema1_database_for_test(&path).unwrap_err(),
                "database_migration_platform_unsupported"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
            let current = DatabaseState::default();
            assert!(
                update(
                    &current,
                    1,
                    || panic!("unadmitted write must not open SQLite"),
                    "e",
                    0,
                    Source::Manual,
                    Some(weather)
                )
                .is_err()
            );
            let path = current
                .initialize_startup(&d.path().join("current"))
                .unwrap();
            let db = Connection::open(&path).unwrap();
            db.execute(
                "INSERT INTO journal_entries VALUES ('e','2026-10-25','',?1,1,'','')",
                [r#"{"type":"doc","content":[{"type":"paragraph"}]}"#],
            )
            .unwrap();
            db.close().unwrap();
            (current, path)
        };
        #[cfg(windows)]
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        update(&state, 1, open, "e", 0, Source::Manual, Some(weather)).unwrap();
        let db = Connection::open(&path).unwrap();
        #[cfg(not(windows))]
        assert!(crate::database::validate_supported_schema(&db).unwrap());
        #[cfg(windows)]
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            crate::database::SCHEMA_VERSION
        );
        let json: String = db
            .query_row("SELECT content_json FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        #[cfg(not(windows))]
        assert_eq!(json, r#"{"unknown":true}"#);
        #[cfg(windows)]
        assert_eq!(json, r#"{"type":"doc","content":[{"type":"paragraph"}]}"#);
        assert_eq!(
            load(&state, 1, open, "e")
                .unwrap()
                .data
                .manual_json
                .as_deref(),
            Some(weather)
        );
        db.close().unwrap();
    }
}

#[cfg(test)]
mod manual_ui_tests {
    use super::*;
    #[test]
    fn manual_write_protects_journal_and_preserves_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weather-ui.sqlite");
        crate::database::create_new_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        let doc = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"synthetic"}]}]}"#;
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','2026-10-25','',?1,1,'','')",
            [doc],
        )
        .unwrap();
        let provider = r#"{"period":{"kind":"day","date":"2026-10-25"},"location":{"coordinates":{"latitude":0,"longitude":0}},"timezone":"UTC","source":{"provider":"open-meteo","product":"test","dataKind":"reanalysis","attribution":"test","license":"test"},"retrievedAt":"2026-10-25T00:00:00Z","temperatureMeanC":10}"#;
        db.execute(
            "INSERT INTO journal_weather VALUES ('e',1,NULL,?1,1)",
            [provider],
        )
        .unwrap();
        drop(db);
        let state = DatabaseState::ready_for_test();
        let open = || Connection::open(&path).map_err(|e| e.to_string());
        let manual = r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0}"#;
        let result = update_manual(&state, 1, open, "e", 1, Some(manual)).unwrap();
        assert_eq!(result.fetched_json.as_deref(), Some(provider));
        assert_eq!(result.revision, 2);
        assert_eq!(
            update_manual(&state, 1, open, "e", 1, None).unwrap_err(),
            "weather_revision_conflict"
        );
        let result = update_manual(&state, 1, open, "e", 2, None).unwrap();
        assert_eq!(result.fetched_json.as_deref(), Some(provider));
        assert_eq!(result.revision, 3);
        let db = Connection::open(&path).unwrap();
        db.execute(
            "UPDATE journal_entries SET content_json='{}' WHERE id='e'",
            [],
        )
        .unwrap();
        drop(db);
        assert_eq!(
            update_manual(&state, 1, open, "e", 3, Some(manual)).unwrap_err(),
            "journal_document_incompatible"
        );
        assert_eq!(load(&state, 1, open, "e").unwrap().data.revision, 3);
        state.set_generation_for_test(2);
        assert!(
            update_manual(
                &state,
                1,
                || panic!("stale write must not open database"),
                "e",
                3,
                Some(manual)
            )
            .is_err()
        );
    }
}
