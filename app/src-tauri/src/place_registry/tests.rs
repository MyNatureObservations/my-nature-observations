use super::*;
use std::path::{Path, PathBuf};

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    crate::database_state::DatabaseState,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("places.sqlite");
    crate::database::create_current_database(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('entry-one','legacy date','title','{ \"unknown\":true }',1,'created','updated')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('entry-two','2026-10-10','','{}',1,'c2','u2')",
        [],
    )
    .unwrap();
    db.close().unwrap();
    (
        dir,
        path,
        crate::database_state::DatabaseState::ready_for_test(),
    )
}

fn open(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    Ok(db)
}

fn create(
    state: &crate::database_state::DatabaseState,
    path: &Path,
    name: &str,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Place {
    create_with(state, 1, || open(path), name.into(), latitude, longitude)
        .unwrap()
        .data
}

fn set(
    state: &crate::database_state::DatabaseState,
    path: &Path,
    entry: &str,
    revision: i64,
    place: Option<&str>,
) -> JournalPlace {
    set_journal_place_with(state, 1, || open(path), entry, revision, place)
        .unwrap()
        .data
}

fn read_default(state: &crate::database_state::DatabaseState, path: &Path) -> WeatherDefaultPlace {
    read_weather_default_place_with(state, 1, || open(path))
        .unwrap()
        .data
}

fn set_default(
    state: &crate::database_state::DatabaseState,
    path: &Path,
    revision: i64,
    place: Option<&str>,
) -> WeatherDefaultPlace {
    set_weather_default_place_with(state, 1, || open(path), revision, place)
        .unwrap()
        .data
}

#[test]
fn weather_default_select_swap_clear_and_stale_write_are_revision_protected() {
    let (_dir, path, state) = fixture();
    assert_eq!(
        read_default(&state, &path),
        WeatherDefaultPlace {
            place: None,
            revision: "0".into(),
        }
    );
    let first = create(&state, &path, "Första", Some(0.0), Some(0.0));
    let second = create(&state, &path, "Andra", Some(59.0), Some(18.0));
    let without_coordinates = create(&state, &path, "Utan koordinater", None, None);

    let selected = set_default(&state, &path, 0, Some(&first.id));
    assert_eq!(selected.revision, "1");
    assert_eq!(selected.place.as_ref().unwrap().id, first.id);
    assert_eq!(selected.place.as_ref().unwrap().latitude, 0.0);
    let swapped = set_default(&state, &path, 1, Some(&second.id));
    assert_eq!(swapped.revision, "2");
    assert_eq!(swapped.place.as_ref().unwrap().id, second.id);
    let cleared = set_default(&state, &path, 2, None);
    assert_eq!(cleared.revision, "3");
    assert!(cleared.place.is_none());
    let cleared_again = set_default(&state, &path, 3, None);
    assert_eq!(cleared_again.revision, "4");
    assert!(cleared_again.place.is_none());

    assert_eq!(
        set_weather_default_place_with(&state, 1, || open(&path), 2, Some(&first.id))
            .err()
            .unwrap(),
        "place_revision_conflict"
    );
    assert_eq!(
        set_weather_default_place_with(&state, 1, || open(&path), 4, Some(&without_coordinates.id))
            .err()
            .unwrap(),
        "weather_default_place_coordinates_required"
    );
    assert_eq!(
        set_weather_default_place_with(
            &state,
            1,
            || open(&path),
            4,
            Some("00000000-0000-4000-8000-000000000099")
        )
        .err()
        .unwrap(),
        "place_not_found"
    );
    assert_eq!(
        set_weather_default_place_with(&state, 1, || open(&path), 4, Some("not-a-uuid"))
            .err()
            .unwrap(),
        "place_id_invalid"
    );
    assert_eq!(
        set_weather_default_place_with(
            &state,
            2,
            || panic!("stale generation must not open the database"),
            4,
            Some(&first.id)
        )
        .err()
        .unwrap(),
        crate::database_state::GENERATION_CONFLICT
    );
    assert_eq!(read_default(&state, &path), cleared_again);
}

#[test]
fn weather_default_protects_registry_and_keeps_historical_snapshots_unchanged() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Historisk", Some(59.1), Some(18.2));
    let journal_snapshot = set(&state, &path, "entry-one", 0, Some(&place.id));
    let manual_weather =
        r#"{"period":{"kind":"day","date":"2026-10-10"},"temperatureC":7,"precipitationMm":0}"#;
    {
        let db = open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_weather VALUES ('entry-one',1,?1,NULL,1)",
            [manual_weather],
        )
        .unwrap();
    }
    let proof_before = {
        let mut db = open(&path).unwrap();
        crate::storage_content::verify_v5(&mut db, &std::sync::atomic::AtomicBool::new(false))
            .unwrap()
    };
    let selected = set_default(&state, &path, 0, Some(&place.id));
    let proof_selected = {
        let mut db = open(&path).unwrap();
        crate::storage_content::verify_v5(&mut db, &std::sync::atomic::AtomicBool::new(false))
            .unwrap()
    };
    assert_ne!(proof_selected, proof_before);
    assert_eq!(
        delete_with(&state, 1, || open(&path), &place.id)
            .err()
            .unwrap(),
        "place_in_use"
    );
    assert_eq!(
        update_with(
            &state,
            1,
            || open(&path),
            &place.id,
            1,
            "Utan koordinater".into(),
            None,
            None
        )
        .err()
        .unwrap(),
        "place_coordinates_required_by_weather_default"
    );
    let updated = update_with(
        &state,
        1,
        || open(&path),
        &place.id,
        1,
        "Aktuellt namn".into(),
        Some(60.0),
        Some(19.0),
    )
    .unwrap()
    .data;
    assert_eq!(updated.revision, "2");
    let current = read_default(&state, &path);
    assert_eq!(current.revision, selected.revision);
    assert_eq!(current.place.as_ref().unwrap().name, "Aktuellt namn");
    assert_eq!(current.place.as_ref().unwrap().latitude, 60.0);
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data,
        journal_snapshot
    );
    let stored_weather: (Option<String>, Option<String>, i64) = open(&path)
        .unwrap()
        .query_row(
            "SELECT manual_json,fetched_json,revision FROM journal_weather WHERE entry_id='entry-one'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        stored_weather,
        (Some(manual_weather.into()), None, 1),
        "default-place changes must not rewrite historical weather"
    );
    let proof_updated = {
        let mut db = open(&path).unwrap();
        crate::storage_content::verify_v5(&mut db, &std::sync::atomic::AtomicBool::new(false))
            .unwrap()
    };
    assert_ne!(proof_updated, proof_selected);
}

#[test]
fn weather_default_survives_real_restart_and_is_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    let state = crate::database_state::DatabaseState::default();
    let path = state.initialize_startup(&root).unwrap();
    let place = create_with(
        &state,
        1,
        || open(&path),
        "Omstart".into(),
        Some(57.0),
        Some(12.0),
    )
    .unwrap()
    .data;
    set_weather_default_place_with(&state, 1, || open(&path), 0, Some(&place.id)).unwrap();
    drop(state);

    let restarted = crate::database_state::DatabaseState::default();
    let reopened = restarted.initialize_startup(&root).unwrap();
    let value = read_weather_default_place_with(&restarted, 1, || open(&reopened))
        .unwrap()
        .data;
    assert_eq!(value.revision, "1");
    assert_eq!(value.place.unwrap().id, place.id);
}

#[test]
fn create_list_get_update_and_delete_preserve_unique_ids_and_allow_duplicate_names() {
    let (_dir, path, state) = fixture();
    let first = create(&state, &path, "Samma namn", Some(0.0), Some(0.0));
    let second = create(&state, &path, "Samma namn", None, None);
    assert_ne!(first.id, second.id);
    assert_eq!(first.revision, "1");
    assert_eq!(second.latitude, None);
    assert_eq!(second.longitude, None);

    let listed = list_with(&state, 1, || open(&path)).unwrap().data;
    assert_eq!(listed.len(), 2);
    assert_eq!(
        get_with(&state, 1, || open(&path), &first.id).unwrap().data,
        first
    );

    let updated = update_with(
        &state,
        1,
        || open(&path),
        &first.id,
        1,
        "Ängen".into(),
        Some(-90.0),
        Some(180.0),
    )
    .unwrap()
    .data;
    assert_eq!(updated.name, "Ängen");
    assert_eq!(updated.revision, "2");
    assert!(delete_with(&state, 1, || open(&path), &updated.id).is_ok());
    assert_eq!(list_with(&state, 1, || open(&path)).unwrap().data.len(), 1);
}

#[test]
fn invalid_names_ids_and_coordinate_pairs_are_rejected() {
    let (_dir, path, state) = fixture();
    for (lat, lon) in [
        (Some(91.0), Some(0.0)),
        (Some(-90.0001), Some(0.0)),
        (Some(0.0), Some(180.0001)),
        (Some(0.0), None),
        (None, Some(0.0)),
        (Some(f64::NAN), Some(0.0)),
        (Some(0.0), Some(f64::INFINITY)),
    ] {
        assert_eq!(
            create_with(&state, 1, || open(&path), "Valid".into(), lat, lon)
                .err()
                .unwrap(),
            "place_coordinates_invalid"
        );
    }
    assert_eq!(
        create_with(&state, 1, || open(&path), " \n ".into(), None, None)
            .err()
            .unwrap(),
        "place_name_invalid"
    );
    assert!(validate_id("not-a-uuid").is_err());
    assert!(get_with(&state, 1, || open(&path), "not-a-uuid").is_err());
}

#[test]
fn registry_edits_do_not_rewrite_journal_snapshot_and_referenced_delete_is_blocked() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Skogsbrynet", Some(59.1), Some(18.2));
    let linked = set(&state, &path, "entry-one", 0, Some(&place.id));
    assert_eq!(linked.revision, "1");
    assert_eq!(linked.place.as_ref().unwrap().name, "Skogsbrynet");
    assert_eq!(linked.place.as_ref().unwrap().place_revision, "1");

    let changed = update_with(
        &state,
        1,
        || open(&path),
        &place.id,
        1,
        "Nya namnet".into(),
        Some(60.0),
        Some(19.0),
    )
    .unwrap()
    .data;
    assert_eq!(changed.revision, "2");
    assert_eq!(
        update_with(
            &state,
            1,
            || open(&path),
            &place.id,
            1,
            "Föråldrat namn".into(),
            Some(1.0),
            Some(2.0),
        )
        .err()
        .unwrap(),
        "place_revision_conflict"
    );
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data
            .place
            .unwrap(),
        PlaceSnapshot {
            id: place.id.clone(),
            name: "Skogsbrynet".into(),
            latitude: Some(59.1),
            longitude: Some(18.2),
            place_revision: "1".into(),
        }
    );
    assert_eq!(
        delete_with(&state, 1, || open(&path), &place.id)
            .err()
            .unwrap(),
        "place_in_use"
    );
}

#[test]
fn link_swap_tombstone_and_stale_writes_are_revision_protected() {
    let (_dir, path, state) = fixture();
    let first = create(&state, &path, "Första", None, None);
    let second = create(&state, &path, "Andra", Some(0.0), Some(0.0));

    let linked = set(&state, &path, "entry-one", 0, Some(&first.id));
    assert_eq!(linked.revision, "1");
    let swapped = set(&state, &path, "entry-one", 1, Some(&second.id));
    assert_eq!(swapped.revision, "2");
    assert_eq!(swapped.place.as_ref().unwrap().id, second.id);
    assert_eq!(
        set_journal_place_with(&state, 1, || open(&path), "entry-one", 1, Some(&first.id))
            .err()
            .unwrap(),
        "place_revision_conflict"
    );

    let removed = set(&state, &path, "entry-one", 2, None);
    assert!(removed.place.is_none());
    assert_eq!(removed.revision, "3");
    assert_eq!(
        set_journal_place_with(&state, 1, || open(&path), "entry-one", 2, Some(&first.id))
            .err()
            .unwrap(),
        "place_revision_conflict"
    );
    assert!(delete_with(&state, 1, || open(&path), &second.id).is_ok());
    assert!(delete_with(&state, 1, || open(&path), &first.id).is_ok());
}

#[test]
fn link_rejects_missing_journal_and_place_without_partial_rows() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Finns", None, None);
    assert_eq!(
        set_journal_place_with(&state, 1, || open(&path), "missing", 0, Some(&place.id))
            .err()
            .unwrap(),
        "journal_entry_not_found"
    );
    assert_eq!(
        set_journal_place_with(
            &state,
            1,
            || open(&path),
            "entry-one",
            0,
            Some("00000000-0000-4000-8000-000000000099"),
        )
        .err()
        .unwrap(),
        "place_not_found"
    );
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data
            .revision,
        "0"
    );
}

#[test]
fn journal_delete_cascades_link_and_foreign_keys_reject_invalid_references() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Refererad", None, None);
    set(&state, &path, "entry-one", 0, Some(&place.id));
    {
        let db = open(&path).unwrap();
        assert!(
            db.execute("DELETE FROM journal_entries WHERE id='entry-one'", [])
                .is_ok()
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM journal_places WHERE entry_id='entry-one'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(db.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES ('absent','bad-id','X',NULL,NULL,1,1)", []).is_err());
    }
    assert!(delete_with(&state, 1, || open(&path), &place.id).is_ok());
}

#[test]
fn operations_reject_schema1_instead_of_activating_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema1.sqlite");
    crate::database::create_new_database(&path).unwrap();
    let state = crate::database_state::DatabaseState::ready_for_test();
    assert_eq!(
        list_with(&state, 1, || open(&path)).err().unwrap(),
        "place_schema_unavailable"
    );
    let db = open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn generation_and_revision_overflow_fail_without_changes() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Revision", None, None);
    assert_eq!(
        create_with(
            &state,
            2,
            || open(&path),
            "Wrong generation".into(),
            None,
            None
        )
        .err()
        .unwrap(),
        crate::database_state::GENERATION_CONFLICT
    );
    {
        let db = open(&path).unwrap();
        db.execute(
            "UPDATE places SET revision=?1 WHERE id=?2",
            params![i64::MAX, place.id],
        )
        .unwrap();
    }
    assert_eq!(
        update_with(
            &state,
            1,
            || open(&path),
            &place.id,
            i64::MAX,
            "Overflow".into(),
            None,
            None
        )
        .err()
        .unwrap(),
        "place_revision_overflow"
    );
    assert_eq!(
        get_with(&state, 1, || open(&path), &place.id)
            .unwrap()
            .data
            .name,
        "Revision"
    );
}

#[test]
fn removing_and_restoring_registry_coordinates_preserves_journal_history() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Lake", Some(59.0), Some(18.0));
    let historical = set(&state, &path, "entry-one", 0, Some(&place.id));
    let cleared = update_with(
        &state,
        1,
        || open(&path),
        &place.id,
        1,
        "Forest".into(),
        None,
        None,
    )
    .unwrap()
    .data;
    assert_eq!(cleared.revision, "2");
    assert_eq!((cleared.latitude, cleared.longitude), (None, None));
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data,
        historical
    );
    let restored = update_with(
        &state,
        1,
        || open(&path),
        &place.id,
        2,
        "Forest".into(),
        Some(0.0),
        Some(0.0),
    )
    .unwrap()
    .data;
    assert_eq!(restored.revision, "3");
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data,
        historical
    );
}

#[test]
fn explicit_reselection_refreshes_only_the_requested_journal_snapshot() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Old name", Some(59.0), Some(18.0));
    let first = set(&state, &path, "entry-one", 0, Some(&place.id));
    let second = set(&state, &path, "entry-two", 0, Some(&place.id));
    let changed = update_with(
        &state,
        1,
        || open(&path),
        &place.id,
        1,
        "New name".into(),
        Some(0.0),
        Some(0.0),
    )
    .unwrap()
    .data;
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data,
        first
    );
    let refreshed = set(&state, &path, "entry-one", 1, Some(&place.id));
    let snapshot = refreshed.place.unwrap();
    assert_eq!(snapshot.id, changed.id);
    assert_eq!(snapshot.name, changed.name);
    assert_eq!(
        (snapshot.latitude, snapshot.longitude),
        (changed.latitude, changed.longitude)
    );
    assert_eq!(snapshot.place_revision, changed.revision);
    assert_eq!(refreshed.revision, "2");
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-two")
            .unwrap()
            .data,
        second
    );
    assert_eq!(
        set_journal_place_with(&state, 1, || open(&path), "entry-one", 1, Some(&place.id))
            .err()
            .unwrap(),
        "place_revision_conflict"
    );
}

#[test]
fn place_usage_lists_all_journals_and_default_without_mutating_data() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Lake", Some(0.0), Some(0.0));
    let empty = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert_eq!(empty.total_count, 0);
    assert!(!empty.is_weather_default);
    let first = set(&state, &path, "entry-one", 0, Some(&place.id));
    let second = set(&state, &path, "entry-two", 0, Some(&place.id));
    let before = std::fs::read(&path).unwrap();
    let usage = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(usage.place_id, place.id);
    assert_eq!(usage.total_count, 2);
    assert_eq!(usage.entries.len(), 2);
    assert!(
        usage
            .entries
            .iter()
            .any(|entry| entry.id == "entry-two" && entry.title.is_empty())
    );
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-one")
            .unwrap()
            .data,
        first
    );
    assert_eq!(
        read_journal_place_with(&state, 1, || open(&path), "entry-two")
            .unwrap()
            .data,
        second
    );
    set_default(&state, &path, 0, Some(&place.id));
    let both = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert!(both.is_weather_default);
    assert_eq!(both.total_count, 2);
    assert_eq!(
        delete_with(&state, 1, || open(&path), &place.id)
            .err()
            .unwrap(),
        "place_in_use"
    );
    set(&state, &path, "entry-one", 1, None);
    set(&state, &path, "entry-two", 1, None);
    let only_default = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert!(only_default.is_weather_default);
    assert_eq!(only_default.total_count, 0);
    assert!(only_default.entries.is_empty());
    assert_eq!(
        delete_with(&state, 1, || open(&path), &place.id)
            .err()
            .unwrap(),
        "place_in_use"
    );
    set_default(&state, &path, 1, None);
    let unused = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert_eq!(unused.total_count, 0);
    assert!(!unused.is_weather_default);
}

#[test]
fn place_usage_preserves_long_titles_and_all_references_and_checks_identity() {
    let (_dir, path, state) = fixture();
    let place = create(&state, &path, "Forest", None, None);
    let long_title = "Long title ".repeat(1000);
    {
        let mut db = open(&path).unwrap();
        let tx = db.transaction().unwrap();
        for i in 0..125 {
            let id = format!("usage-{i:03}");
            tx.execute(
                "INSERT INTO journal_entries VALUES(?1,'2026-10-30',?2,'{}',1,'c','u')",
                params![id, long_title],
            )
            .unwrap();
            tx.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES(?1,?2,'Forest',1,1)", params![id, place.id]).unwrap();
        }
        tx.commit().unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let usage = read_place_usage_with(&state, 1, || open(&path), &place.id)
        .unwrap()
        .data;
    assert_eq!(usage.total_count, 125);
    assert_eq!(usage.entries.len(), 125);
    assert!(usage.entries.iter().all(|entry| entry.title == long_title));
    assert_eq!(usage.entries[0].id, "usage-124");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        read_place_usage_with(&state, 2, || open(&path), &place.id)
            .err()
            .unwrap(),
        "journal_generation_conflict"
    );
    assert!(read_place_usage_with(&state, 1, || open(&path), "not-a-uuid").is_err());
    assert_eq!(
        read_place_usage_with(&state, 1, || open(&path), &uuid::Uuid::new_v4().to_string())
            .err()
            .unwrap(),
        "place_not_found"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
