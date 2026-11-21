use super::*;
use crate::{bird_app, database_state::DatabaseState, journal_search::SearchState};
use serde_json::{Value, json};
fn now() -> time::OffsetDateTime {
    time::OffsetDateTime::parse(
        "2026-11-19T12:00:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap()
}
fn response(date: &str) -> (u16, Vec<u8>) {
    (200,serde_json::to_vec(&json!({"latitude":59.01,"longitude":18.02,"timezone":"Europe/Stockholm","utc_offset_seconds":3600,"daily":{"time":[date],"temperature_2m_mean":[8],"weather_code":[2]},"daily_units":{"time":"iso8601","temperature_2m_mean":"°C","weather_code":"wmo code"}})).unwrap())
}
fn fixture() -> (
    tempfile::TempDir,
    DatabaseState,
    std::path::PathBuf,
    Request,
    Value,
) {
    fixture_schema(12)
}
fn fixture_schema(
    schema: u32,
) -> (
    tempfile::TempDir,
    DatabaseState,
    std::path::PathBuf,
    Request,
    Value,
) {
    let root = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    let path = if schema == 13 {
        state.initialize_schema13_startup(&root.path().join("journal"))
    } else {
        state.initialize_startup(&root.path().join("journal"))
    }
    .unwrap();
    let mut wire: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/bird-create-command.json"
    ))
    .unwrap();
    wire["visitDate"] = "2026-11-19".into();
    wire["visitStart"] = Value::Null;
    wire["visitEnd"] = Value::Null;
    let db = crate::database::open_existing(&path).unwrap();
    db.execute(
        "INSERT INTO places VALUES (?1,'Lake',59.0,18.0,1)",
        [wire["placeId"].as_str().unwrap()],
    )
    .unwrap();
    db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-12','Journal','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[wire["entryId"].as_str().unwrap()]).unwrap();
    db.execute("INSERT INTO journal_weather VALUES (?1,1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-11-12\"},\"temperatureC\":42}',NULL,1)",[wire["entryId"].as_str().unwrap()]).unwrap();
    db.close().unwrap();
    crate::bird_observations::reserve_id(
        &state,
        &SearchState::default(),
        1,
        wire["requestId"].as_str().unwrap(),
    )
    .unwrap();
    let request = Request {
        owner_id: wire["requestId"].as_str().unwrap().into(),
        expected_token: None,
        context: Context {
            date: "2026-11-19".into(),
            place_id: wire["placeId"].as_str().unwrap().into(),
            place_name: "Lake".into(),
            latitude: 59.0,
            longitude: 18.0,
            start: None,
            end: None,
        },
    };
    (root, state, path, request, wire)
}
fn fetch(state: &DatabaseState, slots: &PreviewState, request: &Request) -> Snapshot<Preview> {
    preview_with(
        state,
        slots,
        1,
        uuid::Uuid::new_v4().to_string(),
        request.clone(),
        now(),
        |url| {
            assert!(!url.query_pairs().any(|(k, _)| k == "current"));
            Ok(response(&request.context.date))
        },
    )
    .unwrap()
}
#[test]
fn preview_review_is_read_only_and_protected_save_keeps_both_sources_and_archives() {
    let (root, state, path, request, mut wire) = fixture();
    let slots = PreviewState::default();
    let bytes = std::fs::read(&path).unwrap();
    let p = fetch(&state, &slots, &request).data;
    assert!(p.today_forecast);
    review(&state, &slots, 1, &p.preview_id, &request).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut wrong = request.clone();
    wrong.context.date = "2026-11-18".into();
    assert!(review(&state, &slots, 1, &p.preview_id, &wrong).is_err());
    wire["weather"] = json!([{"action":"fetched","previewId":p.preview_id},{"action":"set","context":request.context,"payload":{"period":{"kind":"day","date":"2026-11-19"},"temperatureC":7},"confirmed":false}]);
    let id = bird_app::create_with_previews(
        &state,
        &SearchState::default(),
        Some(&slots),
        serde_json::from_value(wire.clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        bird_app::create_with_previews(
            &state,
            &SearchState::default(),
            Some(&slots),
            serde_json::from_value(wire).unwrap()
        )
        .unwrap(),
        id
    );
    let list = crate::bird_observations::reopen(&state, 1, &id).unwrap();
    assert_eq!(list.weather.len(), 2);
    let db = crate::database::open_existing(&path).unwrap();
    assert!(
        db.query_row("SELECT manual_json FROM journal_weather", [], |r| r
            .get::<_, String>(0))
            .unwrap()
            .contains("42")
    );
    let backup = root.path().join("weather.mno-backup");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    db.close().unwrap();
    let mut db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let export = root.path().join("weather.mno-export");
    let flag = std::sync::atomic::AtomicBool::new(false);
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    db.close().unwrap();
    let restored = crate::backup::restore::prepare_candidate(
        &backup,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(
        crate::bird_visit_weather::read_all(restored.source().unwrap()).unwrap(),
        list.weather
    );
    let imported = crate::open_import::build_for_schema12(
        &mut std::fs::File::open(&export).unwrap(),
        root.path(),
        &flag,
        13,
    )
    .unwrap();
    let db = crate::database::open_existing(imported.path()).unwrap();
    assert_eq!(
        crate::bird_visit_weather::read_all(&db).unwrap(),
        list.weather
    );
}
#[test]
fn cancellation_expiry_context_and_generation_changes_reject_late_results() {
    let (_root, state, path, request, _) = fixture();
    let slots = PreviewState::default();
    let cancelled = uuid::Uuid::new_v4().to_string();
    slots.cancel(&cancelled).unwrap();
    assert_eq!(
        preview_with(
            &state,
            &slots,
            1,
            cancelled,
            request.clone(),
            now(),
            |_| panic!("cancelled request must never start HTTP")
        )
        .err()
        .unwrap(),
        EXPIRED
    );
    let p = fetch(&state, &slots, &request).data;
    slots.cancel(&p.preview_id).unwrap();
    assert!(review(&state, &slots, 1, &p.preview_id, &request).is_err());
    let p = fetch(&state, &slots, &request).data;
    slots
        .0
        .lock()
        .unwrap()
        .ready
        .get_mut(&p.preview_id)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    assert_eq!(
        review(&state, &slots, 1, &p.preview_id, &request)
            .err()
            .unwrap(),
        EXPIRED
    );
    let token = uuid::Uuid::new_v4().to_string();
    assert!(
        preview_with(
            &state,
            &slots,
            1,
            token.clone(),
            request.clone(),
            now(),
            |_| {
                slots.cancel(&token).unwrap();
                Ok(response(&request.context.date))
            }
        )
        .is_err()
    );
    assert!(
        preview_with(
            &state,
            &slots,
            1,
            uuid::Uuid::new_v4().to_string(),
            request.clone(),
            now(),
            |_| {
                let db = crate::database::open_existing(&path).unwrap();
                db.execute("UPDATE places SET latitude=60,revision=revision+1", [])
                    .unwrap();
                db.close().unwrap();
                Ok(response(&request.context.date))
            }
        )
        .is_err()
    );
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("UPDATE places SET latitude=59,revision=revision+1", [])
        .unwrap();
    db.close().unwrap();
    assert!(
        preview_with(
            &state,
            &slots,
            1,
            uuid::Uuid::new_v4().to_string(),
            request.clone(),
            now(),
            |_| {
                state.maintenance().unwrap().advance_generation().unwrap();
                Ok(response(&request.context.date))
            }
        )
        .is_err()
    );
}
#[test]
fn failed_fetch_and_expired_transaction_leave_observation_uncreated() {
    let (_root, state, path, request, mut wire) = fixture();
    let slots = PreviewState::default();
    let before = std::fs::read(&path).unwrap();
    assert!(
        preview_with(
            &state,
            &slots,
            1,
            uuid::Uuid::new_v4().to_string(),
            request.clone(),
            now(),
            |_| Err(crate::weather::historical::Error::Network)
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let p = fetch(&state, &slots, &request).data;
    slots
        .0
        .lock()
        .unwrap()
        .ready
        .get_mut(&p.preview_id)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    wire["weather"] = json!([{"action":"fetched","previewId":p.preview_id}]);
    assert_eq!(
        bird_app::create_with_previews(
            &state,
            &SearchState::default(),
            Some(&slots),
            serde_json::from_value(wire).unwrap()
        )
        .unwrap_err(),
        EXPIRED
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn existing_visit_preview_rechecks_owner_expiry_and_context_inside_edit_transaction() {
    for schema in [12, 13] {
        for mode in ["valid", "expired", "place", "deleted", "generation"] {
            let (_root, state, path, mut request, wire) = fixture_schema(schema);
            let slots = PreviewState::default();
            let id = bird_app::create_with_previews(
                &state,
                &SearchState::default(),
                None,
                serde_json::from_value(wire.clone()).unwrap(),
            )
            .unwrap();
            let saved = crate::bird_observations::reopen(&state, 1, &id).unwrap();
            request.expected_token = Some(crate::bird_observations::edit_token(&saved));
            let p = fetch(&state, &slots, &request).data;
            let mut edit = wire.clone();
            edit.as_object_mut().unwrap().remove("requestId");
            edit["id"] = id.clone().into();
            edit["expectedToken"] = request.expected_token.clone().unwrap().into();
            edit["weather"] = json!([{"action":"fetched","previewId":p.preview_id}]);
            match mode {
                "expired" => {
                    slots
                        .0
                        .lock()
                        .unwrap()
                        .ready
                        .get_mut(&p.preview_id)
                        .unwrap()
                        .expires = Instant::now() - Duration::from_secs(1)
                }
                "place" => {
                    let db = crate::database::open_existing(&path).unwrap();
                    db.execute("UPDATE places SET longitude=19,revision=revision+1", [])
                        .unwrap();
                }
                "deleted" => crate::bird_observations::delete(
                    &state,
                    &SearchState::default(),
                    1,
                    &id,
                    request.expected_token.as_deref().unwrap(),
                    true,
                )
                .unwrap(),
                "generation" => {
                    state.maintenance().unwrap().advance_generation().unwrap();
                }
                _ => {}
            }
            let before = std::fs::read(&path).unwrap();
            let result = bird_app::update_with_previews(
                &state,
                &SearchState::default(),
                Some(&slots),
                serde_json::from_value(edit).unwrap(),
            );
            if mode == "valid" {
                result.unwrap();
                assert_eq!(
                    crate::bird_observations::reopen(&state, 1, &id)
                        .unwrap()
                        .weather
                        .len(),
                    1
                );
            } else {
                assert!(result.is_err(), "{mode}");
                assert_eq!(std::fs::read(&path).unwrap(), before);
            }
        }
    }
    assert!(
        serde_json::from_value::<bird_app::WeatherInput>(
            json!({"action":"fetched","previewId":"forged","payload":{"temperatureC":100}})
        )
        .is_err()
    );
}

#[test]
fn module_invalidation_retires_preview_receipts_before_reenable() {
    let (_root, state, _path, request, _wire) = fixture();
    let slots = PreviewState::default();
    let id = uuid::Uuid::new_v4().to_string();
    let preview = preview_with(
        &state,
        &slots,
        1,
        id.clone(),
        request.clone(),
        now(),
        |_| Ok(response("2026-11-19")),
    )
    .unwrap();
    slots.invalidate().unwrap();
    assert!(review(&state, &slots, 1, &preview.data.preview_id, &request).is_err());
    assert!(slots.0.lock().unwrap().ready.is_empty());
    assert!(slots.0.lock().unwrap().pending.is_empty());
}

#[test]
fn schema13_preview_creation_receipt_and_strict_schema_rejection() {
    for mode in ["valid", "future", "structural"] {
        let (_root, state, path, request, mut wire) = fixture_schema(13);
        let slots = PreviewState::default();
        let p = fetch(&state, &slots, &request).data;
        review(&state, &slots, 1, &p.preview_id, &request).unwrap();
        let receipt = slots
            .resolve(
                &p.preview_id,
                1,
                &request.owner_id,
                None,
                &request.context.date,
                &request.context.place_id,
                None,
                None,
            )
            .unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        if mode == "future" {
            db.pragma_update(None, "user_version", 14).unwrap();
        }
        if mode == "structural" {
            db.execute_batch("CREATE TABLE unexpected(x)").unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        if mode != "valid" {
            assert!(receipt.check(&db).is_err());
            db.close().unwrap();
            assert!(
                preview_with(
                    &state,
                    &slots,
                    1,
                    uuid::Uuid::new_v4().to_string(),
                    request.clone(),
                    now(),
                    |_| panic!("invalid schema must reject before HTTP")
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
            continue;
        }
        receipt.check(&db).unwrap();
        db.close().unwrap();
        wire["weather"] = json!([{"action":"fetched","previewId":p.preview_id}]);
        let id = bird_app::create_with_previews(
            &state,
            &SearchState::default(),
            Some(&slots),
            serde_json::from_value(wire).unwrap(),
        )
        .unwrap();
        let list = crate::bird_observations::reopen(&state, 1, &id).unwrap();
        assert_eq!(list.weather.len(), 1);
        assert_eq!(list.weather[0].kind, "fetched");
        assert!(!list.weather[0].stale);
    }
}
#[test]
fn schema13_shared_place_coordinate_change_preserves_weather_as_stale() {
    let (_root, state, path, request, mut wire) = fixture_schema(13);
    wire["weather"] = json!([{"action":"set","context":request.context,"payload":{"period":{"kind":"day","date":"2026-11-19"},"temperatureC":7},"confirmed":false}]);
    let id = bird_app::create_with_previews(
        &state,
        &SearchState::default(),
        None,
        serde_json::from_value(wire).unwrap(),
    )
    .unwrap();
    let before = crate::bird_observations::reopen(&state, 1, &id)
        .unwrap()
        .weather;
    crate::place_registry::update_with(
        &state,
        1,
        || crate::database::open_existing(&path),
        &request.context.place_id,
        1,
        "Lake".into(),
        Some(60.),
        Some(18.),
    )
    .unwrap();
    let after = crate::bird_observations::reopen(&state, 1, &id)
        .unwrap()
        .weather;
    assert_eq!(after.len(), 1);
    assert!(after[0].stale);
    assert_eq!(after[0].payload_json, before[0].payload_json);
    assert_eq!(after[0].context_json, before[0].context_json);
    assert_eq!(crate::bird_visit_weather::current(&after).count(), 0);
    let mut db = crate::database::open_existing(&path).unwrap();
    crate::storage_content::verify_v14(&mut db, &std::sync::atomic::AtomicBool::new(false))
        .unwrap();
    db.close().unwrap();
}
