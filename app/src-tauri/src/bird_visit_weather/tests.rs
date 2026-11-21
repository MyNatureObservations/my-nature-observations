use super::*;
use crate::{
    bird_observations as birds, database_state::DatabaseState, journal_search::SearchState,
};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};
const PLACE: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn date_only_change_preserves_november_18_weather_as_stale_after_november_19_restart() {
    let mut f = fixture();
    f.context.date = "2026-11-18".into();
    let (id, original) = create(&f, true);
    let saved = birds::reopen(&f.state, 1, &id).unwrap();
    let mut changed = f.context.clone();
    changed.date = "2026-11-19".into();
    // The UI sends no weather change when only the visit date was edited.
    update(&f, &saved, &changed, &[]).unwrap();
    let db = crate::database::open_existing(&f.path).unwrap();
    let stored = read_all(&db).unwrap();
    for record in &stored {
        assert!(record.stale);
        let before = original.iter().find(|r| r.kind == record.kind).unwrap();
        assert_eq!(record.context_json, before.context_json);
        assert_eq!(record.payload_json, before.payload_json);
    }
    db.close().unwrap();
    drop(f.state);
    let state = DatabaseState::default();
    state
        .initialize_schema12_startup(f.path.parent().unwrap())
        .unwrap();
    let reopened = birds::reopen(&state, 1, &id).unwrap();
    assert_eq!(reopened.visit_date, "2026-11-19");
    assert_eq!(reopened.weather, stored);
    assert_eq!(current(&reopened.weather).count(), 0);
}

#[test]
fn weather_archives_verify_linked_unlinked_and_stale_records_without_touching_source() {
    let f = fixture();
    let linked = birds::reserve(&f.state, &SearchState::default(), 1).unwrap();
    let mut integer_weather = records(&linked, &f.context);
    for record in &mut integer_weather {
        let mut payload: serde_json::Value = serde_json::from_str(&record.payload_json).unwrap();
        payload["location"] = serde_json::json!({
            "coordinates": {"latitude": 59, "longitude": 18}
        });
        record.payload_json = payload.to_string();
    }
    create_request(&f, &linked, true, &integer_weather).unwrap();
    let (unlinked, _) = create(&f, false);
    birds::set_link(&f.state, &SearchState::default(), 1, &linked, "other", true).unwrap();
    let list = birds::reopen(&f.state, 1, &unlinked).unwrap();
    let mut c = f.context.clone();
    c.end = Some("19:37".into());
    update(&f, &list, &c, &[]).unwrap();
    let before = fs::read(&f.path).unwrap();
    let flag = AtomicBool::new(false);
    let mut db =
        Connection::open_with_flags(&f.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = crate::storage_content::verify_v13(&mut db, &flag).unwrap();
    let expected = read_all(&db).unwrap();
    let backup = f.temp.path().join("visits.mno-backup");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    let export = f.temp.path().join("visits.mno-export");
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    db.close().unwrap();
    let restored =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(read_all(restored.source().unwrap()).unwrap(), expected);
    assert_eq!(
        crate::storage_content::verified_snapshot_v13(restored.source().unwrap(), &flag).unwrap(),
        proof
    );
    drop(restored);
    let candidate = crate::open_import::build_for_schema12(
        &mut fs::File::open(&export).unwrap(),
        f.temp.path(),
        &flag,
        13,
    )
    .unwrap();
    assert_eq!(candidate.proof().sha256, proof.sha256);
    let db =
        Connection::open_with_flags(candidate.path(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(read_all(&db).unwrap(), expected);
    db.close().unwrap();
    drop(candidate);
    assert_eq!(fs::read(&f.path).unwrap(), before);
}
struct Fixture {
    temp: tempfile::TempDir,
    state: DatabaseState,
    path: std::path::PathBuf,
    context: Context,
    rows: Vec<birds::SpeciesCount>,
}
fn fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    let path = state
        .initialize_schema12_startup(&temp.path().join("journal"))
        .unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO places VALUES (?1,'Lake',59.0,18.0,1)", [PLACE])
        .unwrap();
    for entry in ["entry", "other"] {
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-19','Journal','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[entry]).unwrap();
    }
    db.execute("INSERT INTO journal_weather VALUES ('entry',1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-11-19\"},\"temperatureC\":42}',NULL,1)",[]).unwrap();
    db.close().unwrap();
    Fixture {
        temp,
        state,
        path,
        context: Context {
            date: "2026-11-19".into(),
            place_id: PLACE.into(),
            place_name: "Lake".into(),
            latitude: 59.0,
            longitude: 18.0,
            start: None,
            end: None,
        },
        rows: vec![birds::SpeciesCount {
            row_id: None,
            photo: None,
            avibase_id: "avibase-FBBBA943".into(),
            taxonomy_version: crate::species_registry::VERSION.into(),
            count: 4,
            comment: Some("Bird comment".into()),
        }],
    }
}
fn records(id: &str, c: &Context) -> Vec<Record> {
    let manual=serde_json::json!({"period":{"kind":"day","date":c.date},"temperatureC":7,"description":"Visit observation"}).to_string();
    let fetched=serde_json::json!({"period":{"kind":"day","date":c.date},"location":{"name":c.place_name,"coordinates":{"latitude":c.latitude,"longitude":c.longitude}},"timezone":"Europe/Stockholm","retrievedAt":"2026-11-19T12:00:00Z","temperatureMeanC":9,"source":{"provider":"open-meteo","product":"forecast-daily","model":"ecmwf_ifs","dataKind":"forecast","attribution":"Weather data by Open-Meteo","license":"CC BY 4.0","licenseUrl":"https://creativecommons.org/licenses/by/4.0/"}}).to_string();
    vec![
        Record::new(id, "manual", c, &manual).unwrap(),
        Record::new(id, "fetched", c, &fetched).unwrap(),
    ]
}
fn create(f: &Fixture, linked: bool) -> (String, Vec<Record>) {
    let id = birds::reserve(&f.state, &SearchState::default(), 1).unwrap();
    let weather = records(&id, &f.context);
    assert_eq!(create_request(f, &id, linked, &weather).unwrap(), id);
    (id, weather)
}
fn create_request(
    f: &Fixture,
    id: &str,
    linked: bool,
    weather: &[Record],
) -> Result<String, String> {
    birds::create_with_weather(
        &f.state,
        &SearchState::default(),
        1,
        &f.context.date,
        PLACE,
        &f.rows,
        linked.then_some("entry"),
        None,
        None,
        Some(id),
        weather,
    )
}
fn update(
    f: &Fixture,
    list: &birds::List,
    c: &Context,
    changes: &[Change],
) -> Result<String, String> {
    birds::update_with_weather(
        &f.state,
        &SearchState::default(),
        1,
        &list.id,
        None,
        &birds::edit_token(list),
        &c.date,
        &c.place_id,
        c.start.as_deref(),
        c.end.as_deref(),
        &list.rows,
        changes,
    )
}
#[test]
fn guarded_creation_preserves_independent_sources_restart_and_request_receipts() {
    let f = fixture();
    let (id, weather) = create(&f, true);
    assert_eq!(create_request(&f, &id, true, &weather).unwrap(), id);
    let list = birds::reopen(&f.state, 1, &id).unwrap();
    let mut ordered = weather.clone();
    ordered.sort_by(|a, b| a.kind.cmp(&b.kind));
    assert_eq!(list.weather, ordered);
    assert_eq!(current(&list.weather).count(), 2);
    assert!(weather[1].is_forecast());
    assert_eq!(weather[1].resolution(), "day");
    assert_eq!(weather[0].resolution(), "visit");
    let db = crate::database::open_existing(&f.path).unwrap();
    let journal: String = db
        .query_row(
            "SELECT manual_json FROM journal_weather WHERE entry_id='entry'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(journal.contains("42"));
    db.close().unwrap();
    let mut changed = weather.clone();
    changed[0].payload_json = changed[0].payload_json.replace("7", "8");
    assert!(create_request(&f, &id, true, &changed).is_err());
    let root = f.path.parent().unwrap().to_owned();
    drop(f.state);
    let state = DatabaseState::default();
    state.initialize_schema12_startup(&root).unwrap();
    assert_eq!(birds::reopen(&state, 1, &id).unwrap(), list);
}
#[test]
fn context_changes_are_sticky_and_manual_reuse_requires_confirmation() {
    let f = fixture();
    let (id, _) = create(&f, false);
    let original = birds::reopen(&f.state, 1, &id).unwrap();
    let mut c = f.context.clone();
    c.start = Some("08:07".into());
    update(&f, &original, &c, &[]).unwrap();
    let stale = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(current(&stale.weather).count(), 0);
    assert!(stale.weather.iter().all(|r| r.stale));
    assert_eq!(
        stale.weather[0].context_json,
        original.weather[0].context_json
    );
    let before = fs::read(&f.path).unwrap();
    assert!(update(&f, &stale, &c, &[Change::ReuseManual { confirmed: false }]).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let rebound = records(&id, &c)
        .into_iter()
        .find(|r| r.kind == "manual")
        .unwrap();
    assert!(
        update(
            &f,
            &stale,
            &c,
            &[Change::Set {
                record: rebound,
                confirmed: false
            }]
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    update(&f, &stale, &c, &[Change::ReuseManual { confirmed: true }]).unwrap();
    let reused = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(current(&reused.weather).count(), 1);
    assert!(
        reused
            .weather
            .iter()
            .find(|r| r.kind == "fetched")
            .unwrap()
            .stale
    );
    update(&f, &reused, &f.context, &[]).unwrap();
    assert_eq!(
        current(&birds::reopen(&f.state, 1, &id).unwrap().weather).count(),
        0
    );
}

#[test]
fn weather_transactions_rollback_and_replaced_generations_reject_late_updates() {
    let f = fixture();
    let (id, _) = create(&f, true);
    let list = birds::reopen(&f.state, 1, &id).unwrap();
    let before = fs::read(&f.path).unwrap();
    let mut invalid = list
        .weather
        .iter()
        .find(|r| r.kind == "manual")
        .unwrap()
        .clone();
    invalid.payload_json = invalid
        .payload_json
        .replace("\"temperatureC\":7", "\"temperatureC\":-500");
    assert!(
        update(
            &f,
            &list,
            &f.context,
            &[
                Change::Remove("fetched".into()),
                Change::Set {
                    record: invalid,
                    confirmed: false
                }
            ]
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert!(
        birds::delete_with_hook(
            &f.state,
            &SearchState::default(),
            1,
            &id,
            &birds::edit_token(&list),
            true,
            || Err("injected after cascade".into())
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert_eq!(birds::reopen(&f.state, 1, &id).unwrap(), list);
    f.state.set_generation_for_test(2);
    assert!(update(&f, &list, &f.context, &[Change::Remove("manual".into())]).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
}

#[test]
fn coordinate_comparison_is_numeric_exact_and_keeps_strict_validation() {
    let f = fixture();
    for template in records("synthetic-list", &f.context) {
        let base: serde_json::Value = serde_json::from_str(&template.payload_json).unwrap();
        for coordinates in [
            serde_json::json!({"latitude":59,"longitude":18}),
            serde_json::json!({"latitude":59.0,"longitude":18.0}),
            serde_json::json!({"latitude":59,"longitude":18.0}),
            serde_json::json!({"latitude":59.0,"longitude":18}),
        ] {
            let mut payload = base.clone();
            payload["location"] = serde_json::json!({"coordinates":coordinates});
            Record::new(
                "synthetic-list",
                &template.kind,
                &f.context,
                &payload.to_string(),
            )
            .unwrap();
        }
        for coordinates in [
            serde_json::json!({"latitude":59.00000000000001,"longitude":18}),
            serde_json::json!({"latitude":59,"longitude":18.000000000000004}),
            serde_json::json!({"latitude":91,"longitude":18}),
            serde_json::json!({"latitude":59,"longitude":181}),
            serde_json::json!({"latitude":"59","longitude":18}),
            serde_json::json!({"latitude":null,"longitude":18}),
            serde_json::json!({"latitude":59}),
            serde_json::json!({"longitude":18}),
        ] {
            let mut payload = base.clone();
            payload["location"] = serde_json::json!({"coordinates":coordinates});
            assert!(
                Record::new(
                    "synthetic-list",
                    &template.kind,
                    &f.context,
                    &payload.to_string()
                )
                .is_err(),
                "accepted invalid or different coordinates: {coordinates}"
            );
        }
        let mut payload = base.clone();
        payload["location"] = serde_json::json!({"coordinates":{"latitude":59,"longitude":18}});
        let overflow = payload
            .to_string()
            .replace("\"latitude\":59", "\"latitude\":1e400");
        assert!(Record::new("synthetic-list", &template.kind, &f.context, &overflow).is_err());
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 91.0] {
            let mut context = f.context.clone();
            context.latitude = invalid;
            assert!(
                Record::new(
                    "synthetic-list",
                    &template.kind,
                    &context,
                    &template.payload_json
                )
                .is_err()
            );
        }
    }
}

#[test]
fn daily_source_and_context_validation_reject_inconsistent_payloads() {
    let f = fixture();
    let records = records("synthetic-list", &f.context);
    for payload in [
        records[1]
            .payload_json
            .replace("forecast-daily", "current-weather"),
        records[1]
            .payload_json
            .replace("forecast-daily", "historical-weather")
            .replace("\"forecast\"", "\"reanalysis\""),
        records[1].payload_json.replace("59.0", "60.0"),
        records[1]
            .payload_json
            .replace("\"temperatureMeanC\":9", "\"temperatureMeanC\":-500"),
    ] {
        assert!(Record::new("synthetic-list", "fetched", &f.context, &payload).is_err());
    }
    let mut context = f.context.clone();
    context.start = Some("25:00".into());
    assert!(
        Record::new(
            "synthetic-list",
            "manual",
            &context,
            &records[0].payload_json
        )
        .is_err()
    );
}

#[test]
fn rehashed_invalid_visit_weather_exports_are_rejected_without_active_changes() {
    use sha2::Digest;
    use std::io::{Read, Write};
    let f = fixture();
    create(&f, true);
    let flag = AtomicBool::new(false);
    let export = f.temp.path().join("valid.mno-export");
    let mut db =
        Connection::open_with_flags(&f.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    db.close().unwrap();
    let before = fs::read(&f.path).unwrap();
    for mutation in 0..6 {
        let mut zip = zip::ZipArchive::new(fs::File::open(&export).unwrap()).unwrap();
        let mut payloads = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            payloads.push((name, bytes));
        }
        let data = payloads
            .iter_mut()
            .find(|p| p.0 == "bird_visit_weather.jsonl")
            .unwrap();
        let mut rows: Vec<serde_json::Value> = String::from_utf8(data.1.clone())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        match mutation {
            0 => rows[0]["kind"] = "unknown".into(),
            1 => rows[0]["list_id"] = "missing-list".into(),
            2 => rows.push(rows[0].clone()),
            3 => {
                let mut c: serde_json::Value =
                    serde_json::from_str(rows[0]["context_json"].as_str().unwrap()).unwrap();
                let mut p: serde_json::Value =
                    serde_json::from_str(rows[0]["payload_json"].as_str().unwrap()).unwrap();
                c["start"] = "09:13".into();
                rows[0]["context_json"] = serde_json::to_string(&c).unwrap().into();
                p["period"]["date"] = c["date"].clone();
                rows[0]["payload_json"] = serde_json::to_string(&p).unwrap().into();
                rows[0]["stale"] = false.into();
            }
            _ => {
                let mut p: serde_json::Value =
                    serde_json::from_str(rows[0]["payload_json"].as_str().unwrap()).unwrap();
                p["location"]["coordinates"]["latitude"] = if mutation == 4 {
                    serde_json::json!(59.00000000000001)
                } else {
                    serde_json::json!("59")
                };
                rows[0]["payload_json"] = p.to_string().into();
            }
        }
        data.1 = rows
            .iter()
            .map(|r| serde_json::to_string(r).unwrap() + "\n")
            .collect::<String>()
            .into_bytes();
        let size = data.1.len();
        let digest = format!("{:x}", sha2::Sha256::digest(&data.1));
        let manifest = payloads
            .iter_mut()
            .find(|p| p.0 == "manifest.json")
            .unwrap();
        let mut m: serde_json::Value = serde_json::from_slice(&manifest.1).unwrap();
        m["files"]["bird_visit_weather.jsonl"]["size"] = (size as u64).into();
        m["files"]["bird_visit_weather.jsonl"]["sha256"] = digest.into();
        m["files"]["bird_visit_weather.jsonl"]["row_count"] = (rows.len() as u64).into();
        manifest.1 = serde_json::to_vec(&m).unwrap();
        let bad = f.temp.path().join(format!("invalid-{mutation}.mno-export"));
        let mut zip = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
        for (name, bytes) in payloads {
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        let rejected = crate::open_import::build_for_schema12(
            &mut fs::File::open(&bad).unwrap(),
            f.temp.path(),
            &flag,
            13,
        )
        .err()
        .expect("invalid weather must be rejected");
        assert!(
            matches!(
                rejected,
                crate::open_import::Error::Record | crate::open_import::Error::Verification
            ),
            "semantic rejection, not a manifest/checksum failure: {rejected:?}"
        );
        assert_eq!(fs::read(&f.path).unwrap(), before);
    }
}
#[test]
fn invalid_creation_rolls_back_and_deleted_or_stale_lists_reject_late_weather() {
    let f = fixture();
    let id = birds::reserve(&f.state, &SearchState::default(), 1).unwrap();
    let mut bad = records(&id, &f.context);
    bad[1].payload_json = bad[1]
        .payload_json
        .replace("forecast-daily", "current-weather");
    let before = fs::read(&f.path).unwrap();
    assert!(create_request(&f, &id, true, &bad).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let (id, weather) = create(&f, true);
    let list = birds::reopen(&f.state, 1, &id).unwrap();
    let mut c = f.context.clone();
    c.date = "2026-11-18".into();
    update(&f, &list, &c, &[]).unwrap();
    let before = fs::read(&f.path).unwrap();
    assert!(
        update(
            &f,
            &list,
            &f.context,
            &[Change::Set {
                record: list.weather[0].clone(),
                confirmed: false
            }]
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let live = birds::reopen(&f.state, 1, &id).unwrap();
    birds::delete(
        &f.state,
        &SearchState::default(),
        1,
        &id,
        &birds::edit_token(&live),
        true,
    )
    .unwrap();
    let before = fs::read(&f.path).unwrap();
    assert!(update(&f, &live, &c, &[]).is_err());
    assert_eq!(
        create_request(&f, &id, true, &weather).unwrap_err(),
        "bird_request_retired"
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let db = crate::database::open_existing(&f.path).unwrap();
    assert_eq!(read_all(&db).unwrap().len(), 0);
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn referenced_place_coordinate_changes_invalidate_in_same_transaction() {
    let f = fixture();
    let (id, _) = create(&f, false);
    crate::place_registry::update_with(
        &f.state,
        1,
        || crate::database::open_existing(&f.path),
        PLACE,
        1,
        "Lake".into(),
        Some(60.0),
        Some(18.0),
    )
    .unwrap();
    let list = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(current(&list.weather).count(), 0);
    crate::place_registry::update_with(
        &f.state,
        1,
        || crate::database::open_existing(&f.path),
        PLACE,
        2,
        "Lake".into(),
        Some(59.0),
        Some(18.0),
    )
    .unwrap();
    assert_eq!(
        current(&birds::reopen(&f.state, 1, &id).unwrap().weather).count(),
        0
    );
    // A different place identity also invalidates weather, even at identical coordinates.
    let stale = birds::reopen(&f.state, 1, &id).unwrap();
    update(
        &f,
        &stale,
        &f.context,
        &[Change::ReuseManual { confirmed: true }],
    )
    .unwrap();
    let live = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(current(&live.weather).count(), 1);
    let other = "22222222-2222-4222-8222-222222222222";
    let db = crate::database::open_existing(&f.path).unwrap();
    db.execute("INSERT INTO places VALUES (?1,'Lake',59,18,1)", [other])
        .unwrap();
    db.close().unwrap();
    let mut c = f.context.clone();
    c.place_id = other.into();
    update(&f, &live, &c, &[]).unwrap();
    let changed = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(current(&changed.weather).count(), 0);
    assert!(
        changed
            .weather
            .iter()
            .all(|r| r.context_json.contains(PLACE))
    );
}

#[test]
fn complete_visit_weather_restore_and_import_activation_reopen() {
    let module = crate::bird_module::test_module(false);
    assert!(module.operation().is_err());
    // Archive verification/activation uses persistence contracts, never UI activation.
    const CHILD: &str = "MNO_VISIT_WEATHER_ACTIVATION";
    if std::env::var_os(CHILD).is_none() {
        for mode in ["restore", "import", "restore-old", "import-old"] {
            let root = tempfile::Builder::new()
                .prefix("mno-visit-weather-activation-")
                .tempdir_in(std::env::temp_dir())
                .unwrap();
            let token = uuid::Uuid::new_v4().to_string();
            fs::write(root.path().join("synthetic-test-token"), &token).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["bird_visit_weather::tests::complete_visit_weather_restore_and_import_activation_reopen", "--exact", "--nocapture"])
                .env(CHILD, mode).env("MNO_VISIT_WEATHER_CHILD_TOKEN", &token)
                .env("MNO_ACTIVATION_ROOT", root.path()).env("MNO_ACTIVATION_MODE", "")
                .output().unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        return;
    }
    // Never trust a pre-existing application path supplied through the environment.
    // Only a private, token-matched temporary fixture created by this parent test.
    let root = std::path::PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    assert!(
        !fs::symlink_metadata(&root)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let root = fs::canonicalize(root).unwrap();
    assert_eq!(
        root.parent().unwrap(),
        fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mno-visit-weather-activation-")
    );
    assert_eq!(
        fs::read_to_string(root.join("synthetic-test-token")).unwrap(),
        std::env::var("MNO_VISIT_WEATHER_CHILD_TOKEN").unwrap()
    );
    assert!(!root.join("data").exists());
    let mut f = fixture();
    let (linked, _) = create(&f, true);
    let (unlinked, _) = create(&f, false);
    birds::set_link(&f.state, &SearchState::default(), 1, &linked, "other", true).unwrap();
    let unlinked_list = birds::reopen(&f.state, 1, &unlinked).unwrap();
    let mut changed_context = f.context.clone();
    changed_context.end = Some("19:37".into());
    update(&f, &unlinked_list, &changed_context, &[]).unwrap();
    let expected = [
        birds::reopen(&f.state, 1, &linked).unwrap(),
        birds::reopen(&f.state, 1, &unlinked).unwrap(),
    ];
    drop(f.state);
    fs::rename(f.path.parent().unwrap(), root.join("data")).unwrap();
    f.path = root.join("data/mno.sqlite");
    f.state = DatabaseState::default();
    f.state
        .initialize_schema12_startup(&root.join("data"))
        .unwrap();
    let flag = AtomicBool::new(false);
    let mut db =
        Connection::open_with_flags(&f.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let backup = f.temp.path().join("activation.mno-backup");
    let export = f.temp.path().join("activation.mno-export");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    db.close().unwrap();
    let mode = std::env::var(CHILD).unwrap();
    let older = mode.ends_with("-old");
    let mut incoming = crate::backup::restore_context::activation::journal_proof(&f.path).unwrap();
    let (backup, export, source_format) = if older {
        let old = f.temp.path().join("old.sqlite");
        crate::database::create_current_database(&old).unwrap();
        let db = crate::database::open_existing(&old).unwrap();
        db.execute("INSERT INTO journal_entries VALUES ('legacy','2020-06-01','Older journal','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[]).unwrap();
        db.close().unwrap();
        if mode.starts_with("import") {
            crate::backup::restore_context::bird_upgrade::initialize_schema11(&old).unwrap();
        }
        let mut db =
            Connection::open_with_flags(&old, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let backup = f.temp.path().join("old.mno-backup");
        let export = f.temp.path().join("old.mno-export");
        crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
        crate::open_export::export(&mut db, &export, &flag).unwrap();
        db.close().unwrap();
        if mode.starts_with("import") {
            let candidate = crate::open_import::build_for_schema12(
                &mut fs::File::open(&export).unwrap(),
                f.temp.path(),
                &flag,
                12,
            )
            .unwrap();
            incoming = candidate.proof().clone();
            drop(candidate);
        }
        (backup, export, 12)
    } else {
        (backup, export, 13)
    };
    let live = birds::reopen(&f.state, 1, &linked).unwrap();
    let mut changed = live
        .weather
        .iter()
        .find(|r| r.kind == "manual")
        .unwrap()
        .clone();
    changed.payload_json = changed
        .payload_json
        .replace("\"temperatureC\":7", "\"temperatureC\":8");
    update(
        &f,
        &live,
        &f.context,
        &[Change::Set {
            record: changed,
            confirmed: false,
        }],
    )
    .unwrap();
    let before = crate::backup::restore_context::activation::journal_proof(&f.path).unwrap();
    assert_ne!(incoming, before);
    let maintenance = f.state.maintenance().unwrap();
    if mode.starts_with("import") {
        drop(
            crate::backup::restore_context::activation::importing::activate_schema12(
                &maintenance,
                &mut fs::File::open(&export).unwrap(),
                &before,
                &incoming,
                Arc::new(AtomicBool::new(false)),
                source_format,
            )
            .unwrap(),
        );
    } else {
        drop(
            crate::backup::restore_context::activation::activate(
                &maintenance,
                &backup,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
    }
    drop(maintenance);
    drop(f.state);
    let recovery = DatabaseState::default();
    assert!(
        recovery
            .initialize_schema12_startup(&root.join("data"))
            .is_err()
    );
    assert!(
        crate::backup::restore_context::activation::startup::status(&recovery)
            .unwrap()
            .actions
            .contains(&crate::backup::restore_context::activation::startup::Action::FinishNew)
    );
    crate::backup::restore_context::activation::startup::run(
        &recovery,
        crate::backup::restore_context::activation::startup::Action::FinishNew,
    )
    .unwrap();
    drop(recovery);
    let ready = DatabaseState::default();
    ready
        .initialize_schema12_startup(&root.join("data"))
        .unwrap();
    if older {
        let db = crate::database::open_existing(&f.path).unwrap();
        assert!(read_all(&db).unwrap().is_empty());
        assert_eq!(
            db.query_row(
                "SELECT title FROM journal_entries WHERE id='legacy'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "Older journal"
        );
        assert_eq!(
            db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            12
        );
        db.close().unwrap();
    } else {
        assert_eq!(birds::reopen(&ready, 1, &linked).unwrap(), expected[0]);
        assert_eq!(birds::reopen(&ready, 1, &unlinked).unwrap(), expected[1]);
    }
    let protection = fs::read_dir(root.join("data/backups/pre-restore"))
        .unwrap()
        .find_map(|p| {
            let path = p.unwrap().path();
            path.extension()
                .is_some_and(|e| e == "mno-backup")
                .then_some(path)
        })
        .unwrap();
    let protected =
        crate::backup::restore::prepare_candidate(&protection, Arc::new(AtomicBool::new(false)))
            .unwrap();
    let preserved = read_all(protected.source().unwrap()).unwrap();
    assert!(preserved.iter().any(|r| r.list_id == linked
        && r.kind == "manual"
        && r.payload_json.contains("\"temperatureC\":8")));
    drop(protected);
    assert_eq!(
        fs::read_dir(root.join("data/backups/pre-restore"))
            .unwrap()
            .filter(|p| p
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|e| e == "mno-backup"))
            .count(),
        1
    );
}

#[test]
fn every_supported_older_export_normalizes_with_no_visit_weather() {
    for schema in 4..=11 {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("old.sqlite");
        crate::database::create_current_database(&path).unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES ('entry','2026-11-19','Original text','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[]).unwrap();
        db.execute("INSERT INTO places VALUES (?1,'Lake',59,18,1)", [PLACE])
            .unwrap();
        db.execute("INSERT INTO journal_weather VALUES ('entry',1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-11-19\"},\"temperatureC\":42}',NULL,1)",[]).unwrap();
        db.close().unwrap();
        use crate::backup::restore_context::bird_upgrade as upgrade;
        match schema {
            4 => {}
            5 => upgrade::initialize(&path).unwrap(),
            6 => upgrade::initialize_schema6(&path).unwrap(),
            7 => upgrade::initialize_schema7(&path).unwrap(),
            8 => upgrade::initialize_schema8(&path).unwrap(),
            9 => upgrade::initialize_schema9(&path).unwrap(),
            10 => upgrade::initialize_schema10(&path).unwrap(),
            11 => upgrade::initialize_schema11(&path).unwrap(),
            _ => unreachable!(),
        }
        let mut db = crate::database::open_existing(&path).unwrap();
        if schema >= 5 {
            let id = uuid::Uuid::new_v4().to_string();
            let row = uuid::Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO bird_lists(id,visit_date,place_id) VALUES (?1,'2026-11-19',?2)",
                params![id, PLACE],
            )
            .unwrap();
            db.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count) VALUES (?1,?2,0,'avibase-FBBBA943','AviList v2026',4)",params![row,id]).unwrap();
            db.execute("INSERT INTO journal_bird_lists VALUES ('entry',?1)", [&id])
                .unwrap();
            if schema >= 9 {
                db.execute(
                    "INSERT INTO bird_creation_requests VALUES (?1,NULL,1)",
                    [&id],
                )
                .unwrap();
            }
            if schema >= 10 {
                let original = temp.path().join("original.png");
                image::RgbImage::from_pixel(3, 2, image::Rgb([20, 40, 60]))
                    .save(&original)
                    .unwrap();
                let mut photo = crate::bird_photos::Photo::from_asset(
                    crate::photographs::prepare(&original, row).unwrap(),
                );
                if schema >= 11 {
                    photo.caption = Some("Old caption".into());
                }
                photo.insert(&db).unwrap();
            }
        }
        let flag = AtomicBool::new(false);
        let old = crate::open_import::verify_database_version(&mut db, &flag, schema + 1).unwrap();
        db.close().unwrap();
        let mut db =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let export = temp.path().join("old.mno-export");
        crate::open_export::export(&mut db, &export, &flag).unwrap();
        db.close().unwrap();
        let candidate = crate::open_import::build_for_schema12(
            &mut fs::File::open(&export).unwrap(),
            temp.path(),
            &flag,
            schema + 1,
        )
        .unwrap();
        let db = Connection::open_with_flags(
            candidate.path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert!(read_all(&db).unwrap().is_empty());
        db.execute_batch("BEGIN").unwrap();
        let preserved = match schema {
            4 => crate::storage_content::verified_snapshot_v5(&db, &flag),
            5 => crate::storage_content::verified_snapshot_v6(&db, &flag),
            6 => crate::storage_content::verified_snapshot_v7(&db, &flag),
            7 => crate::storage_content::verified_snapshot_v8(&db, &flag),
            8 => crate::storage_content::verified_snapshot_v9(&db, &flag),
            9 => crate::storage_content::verified_snapshot_v10(&db, &flag),
            10 => crate::storage_content::verified_snapshot_v11(&db, &flag),
            11 => crate::storage_content::verified_snapshot_v12(&db, &flag),
            _ => unreachable!(),
        }
        .unwrap();
        assert_eq!(preserved.sha256, old.sha256);
        db.execute_batch("ROLLBACK").unwrap();
        db.close().unwrap();
        drop(candidate);
    }
}
