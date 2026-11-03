use super::*;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
const PLACE: &str = "00000000-0000-4000-8000-000000000001";
const MANUAL: &str = r#"{ "period":{"kind":"day","date":"2020-06-01"},"temperatureC":0 }"#;
fn open(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.pragma_update(None, "foreign_keys", true).unwrap();
    Ok(db)
}
fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    DatabaseState,
    PreviewState,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    crate::database::create_current_database(&path).unwrap();
    let db = open(&path).unwrap();
    for entry in ["one", "two"] {
        db.execute("INSERT INTO journal_entries VALUES(?1,'2020-06-01','','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',1,'c','u')", [entry]).unwrap();
    }
    db.execute(
        "INSERT INTO places VALUES(?1,'Synthetic lake',59.3,18.1,1)",
        [PLACE],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_weather VALUES('one',1,?1,NULL,1)",
        [MANUAL],
    )
    .unwrap();
    (
        dir,
        path,
        DatabaseState::ready_for_test(),
        PreviewState::default(),
    )
}
fn preview(state: &DatabaseState, slots: &PreviewState, path: &Path) -> BoundPreview {
    preview_bound(
        state,
        slots,
        1,
        || open(path),
        "one",
        PlaceSource::Saved { id: PLACE.into() },
        super::super::tests::now(),
        |_| {
            Ok((
                200,
                serde_json::to_vec(&super::super::tests::response()).unwrap(),
            ))
        },
    )
    .unwrap()
    .data
}
fn write(
    state: &DatabaseState,
    slots: &PreviewState,
    path: &Path,
    p: &BoundPreview,
    replace: bool,
) -> Result<Snapshot<storage::StoredWeather>, String> {
    save(
        state,
        slots,
        1,
        || open(path),
        "one",
        &p.preview_id,
        p.weather_revision.parse().unwrap(),
        replace,
    )
}
#[test]
fn saves_exact_preview_preserves_manual_and_requires_replacement_confirmation() {
    let (_dir, path, state, slots) = fixture();
    let p = preview(&state, &slots, &path);
    let saved = write(&state, &slots, &path, &p, false).unwrap().data;
    assert_eq!(saved.manual_json.as_deref(), Some(MANUAL));
    assert_eq!(
        saved.fetched_json.as_deref(),
        Some(serde_json::to_string(&p.fetched).unwrap().as_str())
    );
    assert!(write(&state, &slots, &path, &p, true).is_err()); // receipt consumed
    let p = preview(&state, &slots, &path);
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        write(&state, &slots, &path, &p, false).err().unwrap(),
        "historical_weather_replace_confirmation_required"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        write(&state, &slots, &path, &p, true)
            .unwrap()
            .data
            .revision,
        3
    );
}
#[test]
fn changed_context_revision_wrong_entry_and_generation_cannot_write() {
    for sql in [
        "UPDATE journal_entries SET entry_date='2020-06-02' WHERE id='one'",
        "UPDATE places SET name='Changed',revision=2",
        "UPDATE places SET latitude=60,revision=2",
        "INSERT INTO journal_places VALUES('one',NULL,NULL,NULL,NULL,NULL,1)",
        "UPDATE journal_weather SET revision=2",
    ] {
        let (_dir, path, state, slots) = fixture();
        let p = preview(&state, &slots, &path);
        open(&path).unwrap().execute(sql, []).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(write(&state, &slots, &path, &p, true).is_err(), "{sql}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let (_dir, path, state, slots) = fixture();
    let p = preview(&state, &slots, &path);
    let before = std::fs::read(&path).unwrap();
    assert!(
        save(
            &state,
            &slots,
            1,
            || open(&path),
            "two",
            &p.preview_id,
            1,
            false
        )
        .is_err()
    );
    assert!(
        save(
            &state,
            &slots,
            1,
            || open(&path),
            "one",
            &p.preview_id,
            0,
            false
        )
        .is_err()
    );
    assert!(
        save(
            &state,
            &slots,
            2,
            || open(&path),
            "one",
            &p.preview_id,
            1,
            false
        )
        .is_err()
    );
    state.maintenance().unwrap().advance_generation().unwrap();
    assert!(write(&state, &slots, &path, &p, false).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn invalid_or_empty_preview_never_replaces_weather() {
    for invalid in ["{}".to_owned(), {
        let mut v = serde_json::to_value(super::super::tests::response()).unwrap();
        for (key, value) in v["daily"].as_object_mut().unwrap() {
            if key != "time" {
                *value = serde_json::json!([null]);
            }
        }
        assert_eq!(
            super::super::decode(
                &super::super::tests::request(),
                200,
                &serde_json::to_vec(&v).unwrap(),
                super::super::tests::now()
            )
            .err(),
            Some(Error::NoData)
        );
        let mut v = serde_json::to_value(
            super::super::decode(
                &super::super::tests::request(),
                200,
                &serde_json::to_vec(&super::super::tests::response()).unwrap(),
                super::super::tests::now(),
            )
            .unwrap()
            .fetched,
        )
        .unwrap();
        for key in [
            "temperatureMeanC",
            "temperatureMinC",
            "temperatureMaxC",
            "precipitationSumMm",
            "windSpeedMaxMs",
            "weatherCode",
        ] {
            v.as_object_mut().unwrap().remove(key);
        }
        v.to_string()
    }] {
        let (_dir, path, state, slots) = fixture();
        let p = preview(&state, &slots, &path);
        slots
            .0
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .1
            .as_mut()
            .unwrap()
            .fetched_json = invalid;
        let before = std::fs::read(&path).unwrap();
        assert!(write(&state, &slots, &path, &p, true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
#[test]
fn saved_snapshot_survives_physical_backup_restore_and_portable_roundtrip() {
    let (dir, path, state, slots) = fixture();
    let p = preview(&state, &slots, &path);
    let expected = write(&state, &slots, &path, &p, false).unwrap().data;
    let db = open(&path).unwrap();
    let backup = dir.path().join("snapshot.mno-backup");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    let restored =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    // The owned Restore source deliberately forbids PRAGMA; inspect the saved
    // values without bypassing its authorizer or reopening the candidate.
    let restored_weather = restored
        .source()
        .unwrap()
        .query_row(
            "SELECT manual_json,fetched_json,revision FROM journal_weather WHERE entry_id='one'",
            [],
            |r| {
                Ok(storage::StoredWeather {
                    manual_json: r.get(0)?,
                    fetched_json: r.get(1)?,
                    revision: r.get(2)?,
                })
            },
        )
        .unwrap();
    assert_eq!(restored_weather, expected);
    let archive = dir.path().join("export.mno-export");
    let mut readonly =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut readonly, &archive, &AtomicBool::new(false)).unwrap();
    let candidate = crate::open_import::build_v5(
        &mut std::fs::File::open(archive).unwrap(),
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        storage::read(&open(candidate.path()).unwrap(), "one").unwrap(),
        expected
    );
}

#[test]
fn journal_snapshot_and_default_selection_are_bound_without_changing_either() {
    for source in [PlaceSource::Journal, PlaceSource::Default] {
        let (_dir, path, state, slots) = fixture();
        let db = open(&path).unwrap();
        db.execute(
            "INSERT INTO journal_places VALUES('one',?1,'Historical lake',58.0,17.0,1,1)",
            [PLACE],
        )
        .unwrap();
        db.execute("INSERT INTO weather_default_place VALUES(1,?1,1)", [PLACE])
            .unwrap();
        let link = places::read_link(&db, "one").unwrap();
        let setting = places::read_weather_default_row(&db).unwrap();
        let p = preview_bound(
            &state,
            &slots,
            1,
            || open(&path),
            "one",
            source.clone(),
            super::super::tests::now(),
            |_| {
                Ok((
                    200,
                    serde_json::to_vec(&super::super::tests::response()).unwrap(),
                ))
            },
        )
        .unwrap()
        .data;
        let wire = serde_json::to_value(&p.fetched).unwrap();
        assert_eq!(
            wire["location"]["coordinates"]["latitude"],
            if source == PlaceSource::Journal {
                58.0
            } else {
                59.3
            }
        );
        write(&state, &slots, &path, &p, false).unwrap();
        assert_eq!(places::read_link(&db, "one").unwrap(), link);
        assert_eq!(places::read_weather_default_row(&db).unwrap(), setting);
        let p = preview_bound(
            &state,
            &slots,
            1,
            || open(&path),
            "one",
            source.clone(),
            super::super::tests::now(),
            |_| {
                Ok((
                    200,
                    serde_json::to_vec(&super::super::tests::response()).unwrap(),
                ))
            },
        )
        .unwrap()
        .data;
        if source == PlaceSource::Default {
            db.execute(
                "UPDATE weather_default_place SET place_id=NULL,revision=2",
                [],
            )
            .unwrap();
        } else {
            db.execute(
                "UPDATE journal_places SET place_name_snapshot='Changed',revision=2",
                [],
            )
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert!(write(&state, &slots, &path, &p, true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn late_network_results_and_superseded_previews_cannot_be_saved() {
    let (_dir, path, state, slots) = fixture();
    let p = preview(&state, &slots, &path);
    let latest = preview(&state, &slots, &path);
    assert!(write(&state, &slots, &path, &p, false).is_err());
    write(&state, &slots, &path, &latest, false).unwrap();
    let before = std::fs::read(&path).unwrap();
    let result = preview_bound(
        &state,
        &slots,
        1,
        || open(&path),
        "one",
        PlaceSource::Saved { id: PLACE.into() },
        super::super::tests::now(),
        |_| {
            open(&path)
                .unwrap()
                .execute(
                    "UPDATE journal_entries SET entry_date='2020-06-02' WHERE id='one'",
                    [],
                )
                .unwrap();
            Ok((
                200,
                serde_json::to_vec(&super::super::tests::response()).unwrap(),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(
        storage::read(&open(&path).unwrap(), "one")
            .unwrap()
            .revision,
        2
    );
    assert_ne!(std::fs::read(&path).unwrap(), before); // only the simulated journal edit
    let result = preview_bound(
        &state,
        &slots,
        1,
        || open(&path),
        "one",
        PlaceSource::Saved { id: PLACE.into() },
        super::super::tests::now(),
        |_| {
            state.maintenance().unwrap().advance_generation().unwrap();
            Ok((
                200,
                serde_json::to_vec(&super::super::tests::response()).unwrap(),
            ))
        },
    );
    assert!(result.is_err());
}

#[test]
fn bound_preview_is_readonly_and_a_late_response_cannot_replace_a_newer_preview() {
    let (_dir, path, state, slots) = fixture();
    let before = std::fs::read(&path).unwrap();
    let mut latest = None;
    let old = preview_bound(
        &state,
        &slots,
        1,
        || open(&path),
        "one",
        PlaceSource::Saved { id: PLACE.into() },
        super::super::tests::now(),
        |_| {
            latest = Some(preview(&state, &slots, &path));
            Ok((
                200,
                serde_json::to_vec(&super::super::tests::response()).unwrap(),
            ))
        },
    );
    assert!(matches!(old,Err(ref e) if e=="historical_weather_preview_expired"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    write(&state, &slots, &path, &latest.unwrap(), false).unwrap();
}

#[test]
fn current_snapshot_saves_without_changing_manual_and_survives_archives_and_date_changes() {
    let (dir, path, state, slots) = fixture();
    open(&path)
        .unwrap()
        .execute(
            "UPDATE journal_entries SET entry_date='2026-10-30' WHERE id='one'",
            [],
        )
        .unwrap();
    let response = serde_json::json!({"latitude":59.3,"longitude":18.1,"timezone":"Europe/Stockholm","utc_offset_seconds":7200,
        "current_units":{"time":"iso8601","interval":"seconds","temperature_2m":"°C","precipitation":"mm","wind_speed_10m":"m/s","weather_code":"wmo code"},
        "current":{"time":"2026-10-30T13:45","interval":900,"temperature_2m":16,"precipitation":0,"wind_speed_10m":0,"weather_code":0}});
    let p = preview_bound(
        &state,
        &slots,
        1,
        || open(&path),
        "one",
        PlaceSource::Saved { id: PLACE.into() },
        super::super::tests::now(),
        |_| Ok((200, serde_json::to_vec(&response).unwrap())),
    )
    .unwrap()
    .data;
    let saved = write(&state, &slots, &path, &p, false).unwrap().data;
    assert_eq!(saved.manual_json.as_deref(), Some(MANUAL));
    let snapshot: serde_json::Value =
        serde_json::from_str(saved.fetched_json.as_ref().unwrap()).unwrap();
    assert_eq!(snapshot["source"]["product"], "current-weather");
    assert!(snapshot["temperatureMeanC"].is_null());
    let db = open(&path).unwrap();
    let backup = dir.path().join("current.mno-backup");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    let restored =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    let restored_json: String = restored
        .source()
        .unwrap()
        .query_row(
            "SELECT fetched_json FROM journal_weather WHERE entry_id='one'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(Some(restored_json), saved.fetched_json);
    let archive = dir.path().join("current.mno-export");
    let mut readonly =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut readonly, &archive, &AtomicBool::new(false)).unwrap();
    let candidate = crate::open_import::build_v5(
        &mut std::fs::File::open(archive).unwrap(),
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        storage::read(&open(candidate.path()).unwrap(), "one").unwrap(),
        saved
    );
    db.execute(
        "UPDATE journal_entries SET entry_date='2026-10-31' WHERE id='one'",
        [],
    )
    .unwrap();
    assert_eq!(storage::read(&db, "one").unwrap(), saved);
}
