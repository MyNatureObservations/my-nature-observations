//! Exercise ordinary macOS startup rather than the frozen schema-4 weather fixture.
use super::*;
use std::{fs, path::PathBuf};

fn upgraded(
    schema: u32,
) -> (
    tempfile::TempDir,
    PathBuf,
    DatabaseState,
    storage::StoredWeather,
) {
    let (dir, original, old_state, slots) = fixture();
    let p = preview(&old_state, &slots, &original);
    let expected = write(&old_state, &slots, &original, &p, false)
        .unwrap()
        .data;
    drop(old_state);
    let bytes = fs::read(&original).unwrap();
    let root = dir.path().join("journal");
    fs::create_dir(&root).unwrap();
    let path = root.join("mno.sqlite");
    fs::rename(original, &path).unwrap();
    let state = DatabaseState::default();
    if schema == 11 {
        state.initialize_startup(&root).unwrap();
    } else {
        state.initialize_schema10_startup(&root).unwrap();
    }
    let db = open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        schema
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT content_json FROM journal_entries WHERE id='one'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        r#"{"type":"doc","content":[{"type":"paragraph"}]}"#
    );
    db.close().unwrap();
    assert!(
        fs::read_dir(root.join("backups/bird-schema-upgrade"))
            .unwrap()
            .any(
                |entry| fs::read(entry.unwrap().path().join("schema4.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes)
            )
    );
    (dir, path, state, expected)
}
fn view(state: &DatabaseState, path: &Path) -> serde_json::Value {
    let before = fs::read(path).unwrap();
    let result =
        crate::weather::view::read_views(state, 1, || open(path), &["one".into()]).unwrap();
    assert_eq!(result.generation, 1);
    assert_eq!(fs::read(path).unwrap(), before);
    serde_json::to_value(result.data).unwrap()["one"].clone()
}
#[test]
fn ordinary_schema11_startup_reads_preserved_weather_and_manual_save_reopens() {
    let (_dir, path, state, expected) = upgraded(11);
    let first = view(&state, &path);
    assert_eq!(
        first["manual"],
        serde_json::from_str::<serde_json::Value>(MANUAL).unwrap()
    );
    assert_eq!(
        first["fetched"],
        serde_json::from_str::<serde_json::Value>(expected.fetched_json.as_ref().unwrap()).unwrap()
    );
    assert_eq!(first["revision"], expected.revision.to_string());
    let manual = r#"{"period":{"kind":"day","date":"2020-06-01"},"temperatureC":7,"description":"Synthetic edit"}"#;
    let saved = storage::update_manual(
        &state,
        1,
        || open(&path),
        "one",
        expected.revision,
        Some(manual),
    )
    .unwrap();
    assert_eq!(saved.fetched_json, expected.fetched_json);
    assert_eq!(saved.manual_json.as_deref(), Some(manual));
    drop(state);
    let restarted = DatabaseState::default();
    restarted
        .initialize_startup(path.parent().unwrap())
        .unwrap();
    assert_eq!(storage::read(&open(&path).unwrap(), "one").unwrap(), saved);
    assert_eq!(view(&restarted, &path)["manual"]["temperatureC"], 7);
}
#[test]
fn ordinary_schema11_startup_fetches_and_saves_exact_fixture_without_network() {
    let (_dir, path, state, expected) = upgraded(11);
    let slots = PreviewState::default();
    let before = fs::read(&path).unwrap();
    let p = preview(&state, &slots, &path);
    assert_eq!(fs::read(&path).unwrap(), before);
    let saved = write(&state, &slots, &path, &p, true).unwrap().data;
    assert_eq!(saved.manual_json, expected.manual_json);
    assert_eq!(
        saved.fetched_json,
        Some(serde_json::to_string(&p.fetched).unwrap())
    );
    assert_eq!(saved.revision, expected.revision + 1);
    drop(state);
    let restarted = DatabaseState::default();
    restarted
        .initialize_startup(path.parent().unwrap())
        .unwrap();
    assert_eq!(storage::read(&open(&path).unwrap(), "one").unwrap(), saved);
    assert_eq!(
        view(&restarted, &path)["fetched"],
        serde_json::to_value(p.fetched).unwrap()
    );
}
#[test]
fn supported_schema10_reads_use_its_exact_validator() {
    let (_dir, path, state, expected) = upgraded(10);
    assert_eq!(
        storage::read(&open(&path).unwrap(), "one").unwrap(),
        expected
    );
    assert_eq!(
        view(&state, &path)["revision"],
        expected.revision.to_string()
    );
}
#[test]
fn future_and_malformed_current_schemas_still_reject_weather_without_writes() {
    for schema in [10, 11] {
        for sql in [
            "PRAGMA user_version=12",
            "PRAGMA user_version=999",
            "CREATE TABLE unexpected(value TEXT)",
            "ALTER TABLE bird_row_photos ADD COLUMN unexpected TEXT",
        ] {
            let (_dir, path, state, _) = upgraded(schema);
            let db = open(&path).unwrap();
            db.execute_batch(sql).unwrap();
            db.close().unwrap();
            let before = fs::read(&path).unwrap();
            assert_eq!(
                storage::read(&open(&path).unwrap(), "one").unwrap_err(),
                "database_schema_incompatible"
            );
            assert!(
                crate::weather::view::read_views(&state, 1, || open(&path), &["one".into()])
                    .is_err()
            );
            assert!(
                storage::update_manual(&state, 1, || open(&path), "one", 2, Some(MANUAL)).is_err()
            );
            assert!(crate::database::initialize_database(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}
