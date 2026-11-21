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
