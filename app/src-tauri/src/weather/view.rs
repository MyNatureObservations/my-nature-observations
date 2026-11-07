use super::storage;
use serde::Serialize;

// Read-only UI boundary. Existing domain validation is shared with preservation;
// revision is a decimal string so every SQLite i64 remains exact in JavaScript.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeatherView {
    manual: Option<serde_json::Value>,
    fetched: Option<serde_json::Value>,
    revision: String,
}
pub(crate) fn read_views(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<rusqlite::Connection, String>,
    ids: &[String],
) -> Result<crate::database_state::Snapshot<std::collections::BTreeMap<String, WeatherView>>, String>
{
    let operation = state.operation(Some(generation))?;
    if ids.len() > 100
        || ids
            .iter()
            .any(|id| id.len() > crate::open_export::METADATA_LIMIT)
    {
        return Err("weather_resource_limit".into());
    }
    let mut db = open()?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let mut views = std::collections::BTreeMap::new();
    for id in ids {
        let stored = storage::read(&tx, id)?;
        let parse = |json: Option<String>| {
            json.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
                .transpose()
        };
        views.insert(
            id.clone(),
            WeatherView {
                manual: parse(stored.manual_json)?,
                fetched: parse(stored.fetched_json)?,
                revision: stored.revision.to_string(),
            },
        );
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(operation.snapshot(views))
}
#[tauri::command]
pub(crate) fn read_journal_weather(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    ids: Vec<String>,
) -> Result<crate::database_state::Snapshot<std::collections::BTreeMap<String, WeatherView>>, String>
{
    read_views(
        &state,
        generation,
        || {
            let db = rusqlite::Connection::open_with_flags(
                crate::database::database_path(&app)?,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| e.to_string())?;
            db.busy_timeout(std::time::Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            Ok(db)
        },
        &ids,
    )
}

#[tauri::command]
pub(crate) fn update_manual_weather(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    entry_id: String,
    expected_revision: String,
    manual: Option<serde_json::Value>,
) -> Result<WeatherView, String> {
    let revision = parse_revision(&expected_revision)?;
    let json = manual
        .map(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
        .transpose()?;
    let saved = storage::update_manual(
        &state,
        generation,
        || crate::database::open_database(&app),
        &entry_id,
        revision,
        json.as_deref(),
    )?;
    Ok(WeatherView {
        manual: saved
            .manual_json
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| format!("{e}"))?,
        fetched: saved
            .fetched_json
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| format!("{e}"))?,
        revision: saved.revision.to_string(),
    })
}
pub(super) fn parse_revision(value: &str) -> Result<i64, String> {
    if value.is_empty()
        || value.len() > 19
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err("weather_revision_invalid".into());
    }
    value
        .parse::<i64>()
        .map_err(|_| "weather_revision_invalid".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_state::DatabaseState;
    use rusqlite::{Connection, OpenFlags, params};
    #[test]
    fn readonly_batch_preserves_both_sources_zero_values_and_full_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        crate::database::create_new_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        for id in ["e", "empty", "constructor"] {
            db.execute(
                "INSERT INTO journal_entries VALUES (?1,'2026-10-25','','{}',1,'','')",
                [id],
            )
            .unwrap();
        }
        let manual =
            r#"{"period":{"kind":"day","date":"2026-10-25"},"temperatureC":0,"precipitationMm":0}"#;
        let fetched = r#"{"period":{"kind":"day","date":"2026-10-25"},"location":{"name":"Synthetic","coordinates":{"latitude":0,"longitude":0}},"timezone":"Europe/Stockholm","source":{"provider":"open-meteo","product":"synthetic","dataKind":"reanalysis","attribution":"Test","license":"CC BY"},"retrievedAt":"2026-10-26T09:00:00Z","sunrise":"2026-10-25T08:00:00+01:00","temperatureMeanC":12}"#;
        storage::validate("e", Some(manual), Some(fetched)).unwrap();
        db.execute(
            "INSERT INTO journal_weather VALUES ('e',1,?1,?2,?3)",
            params![manual, fetched, i64::MAX],
        )
        .unwrap();
        db.close().unwrap();
        let before = std::fs::read(&path).unwrap();
        let state = DatabaseState::ready_for_test();
        let open = || {
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())
        };
        let snapshot = read_views(
            &state,
            1,
            open,
            &["e".into(), "empty".into(), "constructor".into()],
        )
        .unwrap();
        assert_eq!(snapshot.generation, 1);
        let result = &snapshot.data["e"];
        assert_eq!(result.revision, i64::MAX.to_string());
        assert_eq!(result.manual.as_ref().unwrap()["temperatureC"], 0);
        assert_eq!(result.manual.as_ref().unwrap()["precipitationMm"], 0);
        assert!(result.manual.as_ref().unwrap().get("windSpeedMs").is_none());
        assert_eq!(
            result.fetched.as_ref().unwrap(),
            &serde_json::from_str::<serde_json::Value>(fetched).unwrap()
        );
        assert!(snapshot.data["empty"].manual.is_none());
        assert_eq!(snapshot.data["constructor"].revision, "0");
        assert!(read_views(&state, 1, open, &["missing".into()]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        state.set_generation_for_test(2);
        assert!(read_views(&state, 1, || panic!("stale must not open"), &["e".into()]).is_err());
    }
    #[test]
    fn legacy_reads_do_not_migrate_and_corrupt_weather_is_not_presented() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        crate::database::create_legacy_database(&path).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('e','','','{}',1,'','')",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let before = std::fs::read(&path).unwrap();
        let state = DatabaseState::ready_for_test();
        let open = || {
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())
        };
        assert_eq!(
            read_views(&state, 1, open, &["e".into()]).unwrap().data["e"].revision,
            "0"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(
            read_views(
                &state,
                1,
                || panic!("oversized batch must not open"),
                &vec!["e".into(); 101]
            )
            .is_err()
        );
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
            let path = current
                .initialize_startup(&dir.path().join("current"))
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
        let open = || {
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())
        };
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_weather VALUES ('e',1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-10-25\"},\"precipitationMm\":-1}',NULL,1)",[]).unwrap();
        db.close().unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(read_views(&state, 1, open, &["e".into()]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[cfg(test)]
mod manual_revision_tests {
    use super::parse_revision;
    #[test]
    fn revision_is_exact_canonical_nonnegative_i64() {
        assert_eq!(parse_revision("0").unwrap(), 0);
        assert_eq!(parse_revision("9223372036854775807").unwrap(), i64::MAX);
        for bad in [
            "",
            "-1",
            "+1",
            "00",
            "01",
            "9223372036854775808",
            " 1",
            "1.0",
        ] {
            assert!(parse_revision(bad).is_err(), "{bad}");
        }
    }
}
