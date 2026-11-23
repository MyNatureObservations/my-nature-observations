use super::*;
use serde_json::{Value, json};
#[test]
fn schema13_module_availability_and_ordinary_create_edit_are_strict() {
    let root = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    let path = state
        .initialize_startup(&root.path().join("journal"))
        .unwrap();
    let enabled = crate::bird_module::test_module(true);
    let disabled = crate::bird_module::test_module(false);
    assert!(available_with_module(&enabled, &state));
    assert!(!available_with_module(&disabled, &state));
    assert!(disabled.command().is_err());
    let mut wire: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/bird-create-command.json"
    ))
    .unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES(?1,'2026-11-12','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[wire["entryId"].as_str().unwrap()]).unwrap();
    db.execute(
        "INSERT INTO places VALUES(?1,'Lake',59,18,1)",
        [wire["placeId"].as_str().unwrap()],
    )
    .unwrap();
    db.close().unwrap();
    crate::bird_observations::reserve_id(
        &state,
        &SearchState::default(),
        1,
        wire["requestId"].as_str().unwrap(),
    )
    .unwrap();
    let _module = enabled.command().unwrap();
    let id = create_with_previews(
        &state,
        &SearchState::default(),
        None,
        serde_json::from_value(wire.clone()).unwrap(),
    )
    .unwrap();
    let view = read_view(&state, 1, None, &id).unwrap();
    assert_eq!(view.rows.len(), 2);
    wire.as_object_mut().unwrap().remove("requestId");
    wire["id"] = id.clone().into();
    wire["expectedToken"] = view.edit_token.into();
    wire["rows"][0]["count"] = "9".into();
    wire["weather"] = json!([]);
    let token = update_with_previews(
        &state,
        &SearchState::default(),
        None,
        serde_json::from_value(wire).unwrap(),
    )
    .unwrap();
    assert_eq!(read_view(&state, 1, None, &id).unwrap().edit_token, token);
    assert_eq!(
        crate::bird_observations::reopen(&state, 1, &id)
            .unwrap()
            .rows[0]
            .count,
        9
    );
    for mode in ["future", "structural"] {
        let db = crate::database::open_existing(&path).unwrap();
        if mode == "future" {
            db.pragma_update(None, "user_version", 14).unwrap();
        } else {
            db.pragma_update(None, "user_version", 13).unwrap();
            db.execute_batch("CREATE TABLE unexpected(x)").unwrap();
        }
        db.close().unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(!available_with_module(&enabled, &state));
        assert!(read_view(&state, 1, None, &id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
#[test]
fn imported_visits_project_once_lock_source_and_link_without_changing_snapshots() {
    let root = tempfile::TempDir::new().unwrap();
    let (state, path) =
        crate::ebird_persistence::tests::fixture_at_schema(&root.path().join("journal"), 13);
    let module = crate::bird_module::test_module(true);
    let search = SearchState::default();
    let mut fields: crate::ebird_persistence::Fields = [
        ("Submission ID", "S100"),
        ("Common Name", "Synthetic bird"),
        ("Scientific Name", "Aves sp."),
        ("Count", "X"),
        ("Location ID", "L100"),
        ("Location", "Lake"),
        ("Latitude", "59"),
        ("Longitude", "18"),
        ("Date", "2026-11-23"),
        ("Time", "8:15 AM"),
        ("ML Catalog Numbers", "10001 10002"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let visit = crate::ebird_persistence::InputVisit {
        submission_id: "S100".into(),
        rows: vec![fields.clone()],
    };
    crate::ebird_persistence::import(&state, &search, &module, 1, &[visit]).unwrap();
    let imported = all_views(&state, 1)
        .unwrap()
        .into_iter()
        .find(|v| v.source.is_some())
        .unwrap();
    let id = imported.id.clone();
    assert_eq!(imported.rows[0].count, "X");
    assert_eq!(imported.rows[0].scientific, "Aves sp.");
    assert!(imported.rows[0].species_id.is_empty());
    assert!(imported.place.id.starts_with("ebird:"));
    set_ebird_link(&state, &search, 1, &id, "entry", true).unwrap();
    set_ebird_link(&state, &search, 1, &id, "other", true).unwrap();
    set_ebird_link(&state, &search, 1, &id, "entry", true).unwrap();
    assert_eq!(
        all_views(&state, 1)
            .unwrap()
            .iter()
            .filter(|v| v.source.is_some())
            .count(),
        1
    );
    assert!(
        journal_targets(&state, 1, &id)
            .unwrap()
            .iter()
            .all(|e| e.linked)
    );
    set_ebird_link(&state, &search, 1, &id, "entry", false).unwrap();
    assert!(read_view(&state, 1, Some("entry"), &id).is_err());
    assert!(read_view(&state, 1, Some("other"), &id).is_ok());
    let db = crate::database::open_existing(&path).unwrap();
    let (row, source): (String, String) = db
        .query_row(
            "SELECT id,source_json FROM ebird_rows WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    db.execute(
        "INSERT INTO ebird_additions VALUES(?1,?2,?3,'Local comment')",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), id, row],
    )
    .unwrap();
    db.close().unwrap();
    assert!(crate::bird_observations::delete(&state, &search, 1, &id, "", true).is_err());
    assert_eq!(
        read_view(&state, 1, None, &id)
            .unwrap()
            .source
            .unwrap()
            .rows[0]
            .fields,
        fields
    );
    fields.insert("Scientific Name".into(), "Changed taxon".into());
    crate::ebird_persistence::import(
        &state,
        &search,
        &module,
        1,
        &[crate::ebird_persistence::InputVisit {
            submission_id: "S100".into(),
            rows: vec![fields],
        }],
    )
    .unwrap();
    let view = read_view(&state, 1, None, &id).unwrap();
    assert_eq!(
        view.source.unwrap().unresolved[0].additions,
        vec!["Local comment"]
    );
    let db = crate::database::open_existing(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT source_json FROM ebird_rows WHERE id=?1",
            [row],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        source
    );
    db.close().unwrap();
}
