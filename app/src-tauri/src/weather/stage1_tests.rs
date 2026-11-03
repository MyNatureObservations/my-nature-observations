use super::*;
use crate::{
    backup, database, database_state::DatabaseState, open_export, open_import, storage_content,
};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::json;
use std::{
    fs,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
const CHILD: &str = "weather::stage1_tests::recovery_child";
const LEGACY: &str = " {\"unknown\":{\"x\":1,\"x\":2},\"type\":\"legacy\"} \n";
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn manual(n: i32) -> String {
    json!({"period":{"kind":"day","date":"2026-10-25"},"location":{"name":"  Sjön  ","coordinates":{"latitude":59.33,"longitude":18.07}},"temperatureC":n,"windSpeedMs":0,"precipitationMm":0,"relativeHumidityPercent":0,"cloudCoverPercent":0,"snowDepthMm":0}).to_string()
}
fn fetched(n: i32) -> String {
    json!({"period":{"kind":"hour","start":"2026-10-25T02:00:00+02:00","endExclusive":"2026-10-25T02:00:00+01:00"},"location":{"name":"Sjön","coordinates":{"latitude":59.33,"longitude":18.07}},"timezone":"Europe/Stockholm","retrievedAt":"2026-10-26T08:30:00Z","source":{"provider":"open-meteo","product":"historical-weather","dataKind":"reanalysis","model":"ERA5","attribution":"Weather data by Open-Meteo","license":"CC-BY-4.0","licenseUrl":"https://creativecommons.org/licenses/by/4.0/"},"temperatureMeanC":n,"relativeHumidityPercent":100,"cloudCoverPercent":0,"snowDepthMm":0,"sunrise":"2026-10-25T07:45:00+01:00","sunset":"2026-10-25T16:30:00+01:00"}).to_string()
}
fn insert(path: &Path, n: i32) {
    let db = Connection::open(path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('odd / id','2026-02-30','  Original titel  ',?1,1,'old created text','')",[LEGACY]).unwrap();
    db.execute(
        "INSERT INTO journal_weather VALUES ('odd / id',1,?1,?2,7)",
        params![manual(n), fetched(n)],
    )
    .unwrap();
    db.close().unwrap();
}
fn proof(path: &Path) -> storage_content::Proof {
    let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let p = storage_content::verify(&mut db, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    p
}
fn backup_file(source: &Path, dest: &Path) {
    let db = Connection::open(source).unwrap();
    let m = backup::create_backup(&db, dest, backup::BackupKind::Manual).unwrap();
    assert_eq!((m.format_version, m.database_schema_version), (2, 1));
    db.close().unwrap();
}
fn export_file(source: &Path, dest: &Path) {
    let mut db = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let m = open_export::export(&mut db, dest, &AtomicBool::new(false)).unwrap();
    assert_eq!((m.format_version, m.database_schema_version), (2, 1));
    db.close().unwrap();
}
#[test]
fn weather_backup_and_open_roundtrip_preserve_exact_legacy_rows_and_all_weather() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source.sqlite");
    database::create_new_database(&source).unwrap();
    insert(&source, 0);
    let expected = proof(&source);
    let archive = d.path().join("weather.mno-backup");
    backup_file(&source, &archive);
    assert_eq!(backup::verify_backup(&archive).unwrap().format_version, 2);
    let candidate = backup::restore::prepare_candidate(&archive, flag()).unwrap();
    let restored = d.path().join("restored.sqlite");
    let mut db = Connection::open(&restored).unwrap();
    {
        let b = rusqlite::backup::Backup::new(candidate.source().unwrap(), &mut db).unwrap();
        b.run_to_completion(32, std::time::Duration::from_millis(1), None)
            .unwrap();
    }
    db.close().unwrap();
    drop(candidate);
    assert_eq!(proof(&restored), expected);
    let export = d.path().join("weather.mno-export");
    export_file(&source, &export);
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(File::open(&export).unwrap()).unwrap();
        let mut md = String::new();
        zip.by_name("journal.md")
            .unwrap()
            .read_to_string(&mut md)
            .unwrap();
        assert!(md.contains("Weather snapshots"));
        assert!(md.contains("sunrise"));
        assert!(md.contains("CC-BY-4.0"));
    }
    let imported = open_import::build(
        &mut File::open(&export).unwrap(),
        d.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(imported.proof().version, 2);
    assert_eq!(proof(imported.path()), expected);
    let db = Connection::open(imported.path()).unwrap();
    let row:(String,String,String,String,String,String,i64)=db.query_row("SELECT id,entry_date,title,content_json,created_at,updated_at,document_version FROM journal_entries",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).unwrap();
    assert_eq!(
        row,
        (
            "odd / id".into(),
            "2026-02-30".into(),
            "  Original titel  ".into(),
            LEGACY.into(),
            "old created text".into(),
            "".into(),
            1
        )
    );
    assert!(!backup::restore::document_is_editable(&row.3));
    let weather = storage::export_record(&db, "odd / id").unwrap().unwrap();
    assert_eq!(weather.manual_json, Some(manual(0)));
    assert_eq!(weather.fetched_json, Some(fetched(0)));
    assert_eq!(weather.revision, 7);
    db.close().unwrap();
}
#[test]
fn lossless_proof_covers_each_weather_source_revision_and_rejects_corruption() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source.sqlite");
    database::create_new_database(&source).unwrap();
    insert(&source, 0);
    let original = proof(&source);
    for sql in [
        "UPDATE journal_weather SET manual_json=NULL",
        "UPDATE journal_weather SET fetched_json=NULL",
        "UPDATE journal_weather SET revision=revision+1",
    ] {
        let db = Connection::open(&source).unwrap();
        db.execute_batch("BEGIN").unwrap();
        db.execute_batch(sql).unwrap();
        db.execute_batch("COMMIT").unwrap();
        db.close().unwrap();
        assert_ne!(proof(&source), original);
        let db = Connection::open(&source).unwrap();
        db.execute(
            "UPDATE journal_weather SET manual_json=?1,fetched_json=?2,revision=7",
            params![manual(0), fetched(0)],
        )
        .unwrap();
        db.close().unwrap();
    }
    for sql in [
        "UPDATE journal_weather SET manual_json='{}'",
        "UPDATE journal_weather SET entry_id='orphan'",
        "UPDATE journal_entries SET content_json='{broken'",
        "UPDATE journal_entries SET content_json=x'ff'",
        "UPDATE journal_entries SET document_version=2",
    ] {
        let copy = d
            .path()
            .join(format!("bad-{}.sqlite", uuid::Uuid::new_v4()));
        fs::copy(&source, &copy).unwrap();
        let mut db = Connection::open(&copy).unwrap();
        db.pragma_update(None, "foreign_keys", false).unwrap();
        db.execute_batch(sql).unwrap();
        assert!(
            storage_content::verify(&mut db, &AtomicBool::new(false)).is_err(),
            "{sql}"
        );
        db.close().unwrap();
    }
}
#[test]
fn independent_fetched_storage_and_invalid_optional_metrics() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("source.sqlite");
    database::create_new_database(&path).unwrap();
    insert(&path, 0);
    let state = DatabaseState::ready_for_test();
    let open = || Connection::open(&path).map_err(|e| e.to_string());
    let m = manual(0);
    let f = fetched(3);
    let r = storage::update(
        &state,
        1,
        open,
        "odd / id",
        7,
        storage::Source::Fetched,
        Some(&f),
    )
    .unwrap();
    assert_eq!(r.manual_json, Some(m.clone()));
    let db = Connection::open(&path).unwrap();
    db.close().unwrap();
    let r = storage::load(&state, 1, open, "odd / id").unwrap().data;
    assert_eq!(r.fetched_json, Some(f));
    for (field, value) in [
        ("relativeHumidityPercent", json!(101)),
        ("cloudCoverPercent", json!(-1)),
        ("snowDepthMm", json!(-1)),
        ("sunrise", json!("unqualified")),
    ] {
        let mut part: serde_json::Value = serde_json::from_str(&fetched(0)).unwrap();
        part[field] = value;
        assert!(
            storage::update(
                &state,
                1,
                open,
                "odd / id",
                8,
                storage::Source::Fetched,
                Some(&part.to_string())
            )
            .is_err()
        );
    }
    let mut part: serde_json::Value = serde_json::from_str(&m).unwrap();
    part["sunrise"] = json!("2026-10-25T07:45:00+01:00");
    assert!(
        storage::update(
            &state,
            1,
            open,
            "odd / id",
            8,
            storage::Source::Manual,
            Some(&part.to_string())
        )
        .is_err()
    );
    assert_eq!(
        storage::load(&state, 1, open, "odd / id")
            .unwrap()
            .data
            .revision,
        8
    );
}
fn setup(root: &Path, empty: bool) {
    let state = DatabaseState::default();
    let active = state
        .initialize_weather_stage1_startup(&root.join("data"))
        .unwrap();
    if !empty {
        insert(&active, 1);
    }
    drop(state);
    let source = root.join("incoming.sqlite");
    database::create_new_database(&source).unwrap();
    insert(&source, 2);
    backup_file(&source, &root.join("incoming.mno-backup"));
    export_file(&source, &root.join("incoming.mno-export"));
}
fn launch(root: &Path, task: &str, hook: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_STAGE1_TASK", task)
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_ACTIVATION_MODE", hook)
        .env("TMPDIR", root)
        .output()
        .unwrap()
}
fn success(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn recovery_child() {
    let Ok(task) = std::env::var("MNO_STAGE1_TASK") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    crate::panic_policy::install();
    let state = DatabaseState::default();
    let started = state.initialize_weather_stage1_startup(&root.join("data"));
    use crate::backup::restore_context::activation::{
        self,
        startup::{self, Action},
    };
    match task.as_str() {
        "restore" => {
            started.unwrap();
            let m = state.maintenance().unwrap();
            activation::activate(&m, &root.join("incoming.mno-backup"), flag()).unwrap();
        }
        "import" => {
            let active = started.unwrap();
            let old = activation::journal_proof(&active).unwrap();
            let candidate = open_import::build_current(
                &mut File::open(root.join("incoming.mno-export")).unwrap(),
                &root,
                &AtomicBool::new(false),
            )
            .unwrap();
            let new = candidate.proof().clone();
            drop(candidate);
            let m = state.maintenance().unwrap();
            activation::prepare_replacement(
                &m,
                &mut File::open(root.join("incoming.mno-export")).unwrap(),
                &old,
                &new,
                flag(),
            )
            .unwrap();
        }
        "blocked" => {
            assert!(started.is_err());
            assert!(startup::status(&state).map_or(true, |v| {
                !v.actions
                    .iter()
                    .any(|a| matches!(a, Action::Continue | Action::FinishNew))
            }));
            assert!(state.operation(None).is_err());
        }
        "new" | "old" => {
            assert!(started.is_err());
            let view = startup::status(&state).unwrap();
            let action = if task == "old" {
                Action::KeepPrevious
            } else {
                view.actions
                    .into_iter()
                    .find(|a| *a == Action::FinishNew || *a == Action::Continue)
                    .unwrap()
            };
            let confirm = view.replacement.and_then(|r| r.previous_confirmation);
            startup::run_confirmed(&state, action, confirm.as_deref()).unwrap();
            assert!(state.operation(None).is_ok());
            let active = root.join("data/mno.sqlite");
            if task == "new" {
                assert_eq!(proof(&active), proof(&root.join("incoming.sqlite")));
            } else {
                let db = Connection::open(&active).unwrap();
                let w = storage::export_record(&db, "odd / id").unwrap().unwrap();
                assert_eq!(w.manual_json, Some(manual(1)));
                assert_eq!(w.fetched_json, Some(fetched(1)));
                db.close().unwrap();
            }
        }
        _ => panic!("unknown child task"),
    }
    std::process::exit(0);
}
#[test]
fn restore_v2_restarts_at_publication_copy_close_and_replace_boundaries() {
    for point in [
        "marker-durable",
        "copy-done",
        "all-closed",
        "after-replace",
        "disk-verified",
    ] {
        let d = tempfile::tempdir().unwrap();
        setup(d.path(), false);
        let out = launch(d.path(), "restore", &format!("crash:{point}"));
        assert_eq!(
            out.status.code(),
            Some(88),
            "{point}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let m: serde_json::Value = serde_json::from_slice(
            &fs::read(d.path().join("data/recovery/activation-v1.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(m["version"], 2);
        assert_eq!(m["old_content"]["version"], 2);
        assert_eq!(m["new_content"]["version"], 2);
        // A crash before a closed candidate exists can safely retain the original.
        success(&launch(
            d.path(),
            if point == "marker-durable" || point == "copy-done" {
                "old"
            } else {
                "new"
            },
            "",
        ));
    }
}
#[test]
fn import_v3_restarts_after_publish_park_install_and_can_keep_previous_weather() {
    for empty in [false, true] {
        for point in [
            "replacement-after-publish",
            "v2-after-park",
            "v2-after-install",
        ] {
            let d = tempfile::tempdir().unwrap();
            setup(d.path(), empty);
            let out = launch(d.path(), "import", "crash:replacement-after-publish");
            assert_eq!(
                out.status.code(),
                Some(88),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let m: serde_json::Value = serde_json::from_slice(
                &fs::read(d.path().join("data/recovery/activation-v1.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(m["version"], 3);
            assert_eq!(m["content_contract"]["version"], 2);
            if point != "replacement-after-publish" {
                let out = launch(d.path(), "new", &format!("crash:{point}"));
                assert_eq!(
                    out.status.code(),
                    Some(88),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            success(&launch(d.path(), "new", ""));
        }
    }
    let d = tempfile::tempdir().unwrap();
    setup(d.path(), false);
    success(&launch(d.path(), "import", ""));
    success(&launch(d.path(), "old", ""));
}
#[test]
fn new_recovery_rejects_changed_weather_even_when_document_text_is_unchanged() {
    for operation in ["restore", "import"] {
        let d = tempfile::tempdir().unwrap();
        setup(d.path(), false);
        let point = if operation == "restore" {
            "all-closed"
        } else {
            "replacement-after-publish"
        };
        assert_eq!(
            launch(d.path(), operation, &format!("crash:{point}"))
                .status
                .code(),
            Some(88)
        );
        let p = d.path().join("data/recovery/activation-v1.json");
        let m: serde_json::Value = serde_json::from_slice(&fs::read(p).unwrap()).unwrap();
        let candidate = d
            .path()
            .join("data/recovery")
            .join(m["candidate_file"].as_str().unwrap());
        let db = Connection::open(candidate).unwrap();
        db.execute("UPDATE journal_weather SET fetched_json=?1", [fetched(9)])
            .unwrap();
        db.close().unwrap();
        success(&launch(d.path(), "blocked", ""));
    }
}
#[test]
fn legacy_open_format_upgrades_privately_then_backs_up_losslessly() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("legacy.sqlite");
    database::create_legacy_database(&source).unwrap();
    let db = Connection::open(&source).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('','invalid date','',?1,1,'legacy creation','legacy update')",[LEGACY]).unwrap();
    db.close().unwrap();
    let export = d.path().join("legacy.mno-export");
    let mut db = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        open_export::export(&mut db, &export, &AtomicBool::new(false))
            .unwrap()
            .format_version,
        1
    );
    db.close().unwrap();
    let legacy = open_import::build(
        &mut File::open(&export).unwrap(),
        d.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(legacy.proof().version, 1);
    assert!(database::validate_schema(&Connection::open(legacy.path()).unwrap()).unwrap());
    let current = open_import::build_current(
        &mut File::open(&export).unwrap(),
        d.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(current.proof().version, 2);
    assert_eq!(proof(&source), proof(current.path()));
    let archive = d.path().join("upgraded.mno-backup");
    backup_file(current.path(), &archive);
    let restored = backup::restore::prepare_candidate(&archive, flag()).unwrap();
    let exact: String = restored
        .source()
        .unwrap()
        .query_row("SELECT content_json FROM main.journal_entries", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(exact, LEGACY);
    assert!(!backup::restore::document_is_editable(&exact));
}
#[test]
fn legacy_backup_restores_to_current_schema_with_an_empty_weather_section() {
    let d = tempfile::tempdir().unwrap();
    setup(d.path(), false);
    let source = d.path().join("legacy.sqlite");
    database::create_legacy_database(&source).unwrap();
    let db = Connection::open(&source).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('legacy','2026-10-25','',?1,1,'','')",[r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Legacy"}]}]}"#]).unwrap();
    // Replace only our test's incoming archive and expected source.
    fs::remove_file(d.path().join("incoming.mno-backup")).unwrap();
    assert_eq!(
        backup::create_backup(
            &db,
            &d.path().join("incoming.mno-backup"),
            backup::BackupKind::Manual
        )
        .unwrap()
        .format_version,
        1
    );
    db.close().unwrap();
    fs::copy(&source, d.path().join("incoming.sqlite")).unwrap();
    assert_eq!(
        launch(d.path(), "restore", "crash:all-closed")
            .status
            .code(),
        Some(88)
    );
    success(&launch(d.path(), "new", ""));
    let db = Connection::open(d.path().join("data/mno.sqlite")).unwrap();
    assert!(database::validate_supported_schema(&db).unwrap());
    let count: i64 = db
        .query_row("SELECT count(*) FROM journal_weather", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    db.close().unwrap();
}
#[test]
fn current_open_format_rejects_rehashed_invalid_weather_without_publishing_candidate() {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source.sqlite");
    database::create_new_database(&source).unwrap();
    insert(&source, 0);
    let archive = d.path().join("source.mno-export");
    export_file(&source, &archive);
    let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        e.read_to_end(&mut bytes).unwrap();
        files.insert(e.name().to_owned(), bytes);
    }
    drop(zip);
    let original = files.clone();
    for mutation in ["revision", "version", "manual", "sunrise", "unknown"] {
        let mut files = original.clone();
        let mut r: serde_json::Value = serde_json::from_slice(&files["journal.jsonl"]).unwrap();
        match mutation {
            "revision" => r["weather"]["revision"] = json!(0),
            "version" => r["weather"]["format_version"] = json!(2),
            "manual" => r["weather"]["manual_json"] = json!("{}"),
            "sunrise" => {
                let mut f: serde_json::Value =
                    serde_json::from_str(r["weather"]["fetched_json"].as_str().unwrap()).unwrap();
                f["sunrise"] = json!("unqualified");
                r["weather"]["fetched_json"] = json!(f.to_string());
            }
            _ => r["weather"]["future"] = json!(true),
        }
        let mut line = serde_json::to_vec(&r).unwrap();
        line.push(b'\n');
        files.insert("journal.jsonl".into(), line);
        let mut m: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        m["files"]["journal.jsonl"] = json!({"size":files["journal.jsonl"].len(),"sha256":format!("{:x}",Sha256::digest(&files["journal.jsonl"]))});
        files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        let bad = d.path().join(format!("{mutation}.mno-export"));
        let mut zip = zip::ZipWriter::new(File::create(&bad).unwrap());
        for (name, b) in files {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&b).unwrap();
        }
        zip.finish().unwrap();
        assert!(
            open_import::build(
                &mut File::open(bad).unwrap(),
                d.path(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "{mutation}"
        );
        assert_eq!(proof(&source).entry_count, 1);
    }
}
#[test]
fn new_recovery_contracts_reject_mixed_versions_and_unknown_fields() {
    for operation in ["restore", "import"] {
        let d = tempfile::tempdir().unwrap();
        setup(d.path(), false);
        let point = if operation == "restore" {
            "all-closed"
        } else {
            "replacement-after-publish"
        };
        assert_eq!(
            launch(d.path(), operation, &format!("crash:{point}"))
                .status
                .code(),
            Some(88)
        );
        let marker = d.path().join("data/recovery/activation-v1.json");
        let bytes = fs::read(&marker).unwrap();
        let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let before = proof(&d.path().join("data/mno.sqlite"));
        for field in ["version", "schema", "proof", "unknown"] {
            let mut m = original.clone();
            match field {
                "version" => m["version"] = json!(if operation == "restore" { 3 } else { 2 }),
                "schema" => m["schema_version"] = json!(0),
                "proof" => {
                    if operation == "restore" {
                        m["new_content"]["version"] = json!(1)
                    } else {
                        m["content_contract"]["version"] = json!(1)
                    }
                }
                _ => m["unknown"] = json!(true),
            }
            fs::write(&marker, serde_json::to_vec(&m).unwrap()).unwrap();
            success(&launch(d.path(), "blocked", ""));
            assert_eq!(proof(&d.path().join("data/mno.sqlite")), before);
        }
        fs::write(&marker, bytes).unwrap();
        success(&launch(d.path(), "new", ""));
    }
}
#[test]
fn migration_crash_child() {
    let Some(root) = std::env::var_os("MNO_PRODUCTION_MIGRATION_ROOT") else {
        return;
    };
    let state = DatabaseState::default();
    state
        .initialize_weather_stage1_startup(Path::new(&root))
        .unwrap();
    panic!("crash point not reached");
}
#[test]
fn interrupted_candidate_migration_restarts_to_ready_with_exact_legacy_data() {
    for point in [
        "copied",
        "transaction",
        "migrated",
        "closed",
        "before-replace",
        "after-replace",
        "verified",
    ] {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("data");
        let state = DatabaseState::default();
        let source = state.initialize_legacy_startup(&root).unwrap();
        let db = Connection::open(&source).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('odd','invalid date','  title  ',?1,1,'old','')",
            [LEGACY],
        )
        .unwrap();
        db.close().unwrap();
        let expected = proof(&source);
        let original = fs::read(&source).unwrap();
        drop(state);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "weather::stage1_tests::migration_crash_child",
                "--nocapture",
            ])
            .env("MNO_PRODUCTION_MIGRATION_ROOT", &root)
            .env("MNO_MIGRATION_CRASH", point)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(89),
            "{point}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if [
            "copied",
            "transaction",
            "migrated",
            "closed",
            "before-replace",
        ]
        .contains(&point)
        {
            assert_eq!(fs::read(&source).unwrap(), original, "{point}");
        }
        assert!(
            !source.with_file_name("mno.sqlite-journal").exists(),
            "{point}"
        );
        // Interrupted private files are retained byte for byte and are not replayed.
        let abandoned: Vec<_> = fs::read_dir(root.join("backups/schema-migration"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let files: Vec<_> = abandoned
            .iter()
            .flat_map(|dir| {
                fs::read_dir(dir)
                    .unwrap()
                    .map(|e| {
                        let p = e.unwrap().path();
                        let b = fs::read(&p).unwrap();
                        (p, b)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let restarted = DatabaseState::default();
        let path = restarted.initialize_weather_stage1_startup(&root).unwrap();
        assert!(restarted.operation(None).is_ok(), "{point}");
        assert!(!restarted.startup_recovery_pending());
        assert_eq!(proof(&path), expected, "{point}");
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT entry_date,title,content_json,created_at,updated_at FROM journal_entries",
                [],
                |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?
                ))
            )
            .unwrap(),
            (
                "invalid date".into(),
                "  title  ".into(),
                LEGACY.into(),
                "old".into(),
                "".into()
            )
        );
        db.close().unwrap();
        for (p, b) in files {
            assert_eq!(fs::read(p).unwrap(), b, "{point}");
        }
        drop(restarted);
        // Repeated startup does not publish another migration for schema 1.
        let count = fs::read_dir(root.join("backups/schema-migration"))
            .unwrap()
            .count();
        let again = DatabaseState::default();
        again.initialize_weather_stage1_startup(&root).unwrap();
        assert_eq!(
            fs::read_dir(root.join("backups/schema-migration"))
                .unwrap()
                .count(),
            count
        );
    }
}
#[test]
fn unknown_active_sidecars_are_not_treated_as_owned_migration_work() {
    for suffix in ["-journal", "-wal", "-shm", "-JOURNAL"] {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("data");
        let state = DatabaseState::default();
        let source = state.initialize_legacy_startup(&root).unwrap();
        drop(state);
        let original = fs::read(&source).unwrap();
        let sidecar = source.with_file_name(format!("mno.sqlite{suffix}"));
        fs::write(&sidecar, b"untrusted migration data").unwrap();
        let state = DatabaseState::default();
        assert!(state.initialize_weather_stage1_startup(&root).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(&sidecar).unwrap(), b"untrusted migration data");
        assert!(!root.join("backups/schema-migration").exists());
    }
}
#[test]
fn physical_backup_restores_large_legacy_id_and_title_before_and_after_migration() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("legacy.sqlite");
    database::create_legacy_database(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    let id = "x".repeat(1024 * 1024 + 1);
    let title = "t".repeat(1024 * 1024 + 1);
    let doc = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Saved legacy entry"}]}]}"#;
    db.execute(
        "INSERT INTO journal_entries VALUES (?1,'2026-10-25',?2,?3,1,'','')",
        params![id, title, doc],
    )
    .unwrap();
    db.close().unwrap();
    let expected = proof(&path);
    for version in [1, 2] {
        if version == 2 {
            database::initialize_schema1_database_for_test(&path).unwrap();
        }
        let db = Connection::open(&path).unwrap();
        let archive = d.path().join(format!("v{version}.mno-backup"));
        assert_eq!(
            backup::create_backup(&db, &archive, backup::BackupKind::Manual)
                .unwrap()
                .format_version,
            version
        );
        db.close().unwrap();
        let restored = backup::restore::prepare_candidate(&archive, flag()).unwrap();
        let row: (String, String, String) = restored
            .source()
            .unwrap()
            .query_row(
                "SELECT id,title,content_json FROM journal_entries",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (id.clone(), title.clone(), doc.into()));
        let actual =
            storage_content::verified_snapshot(restored.source().unwrap(), &AtomicBool::new(false))
                .unwrap();
        assert_eq!(actual, expected);
        drop(restored);
    }
}

#[test]
fn candidate_migration_rejects_corrupt_legacy_values_without_changing_source() {
    for payload in ["'{bad json'", "x'7b7d'", "NULL"] {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("data");
        let state = DatabaseState::default();
        let source = state.initialize_legacy_startup(&root).unwrap();
        drop(state);
        let db = Connection::open(&source).unwrap();
        if payload == "NULL" {
            // Wrong SQL type in otherwise valid JSON's title.
            db.execute(
                "INSERT INTO journal_entries VALUES ('e','2026-10-25',x'00','{}',1,'','')",
                [],
            )
            .unwrap();
        } else {
            db.execute_batch(&format!(
                "INSERT INTO journal_entries VALUES ('e','2026-10-25','',{payload},1,'','')"
            ))
            .unwrap();
        }
        db.close().unwrap();
        let before = fs::read(&source).unwrap();
        let state = DatabaseState::default();
        assert!(state.initialize_weather_stage1_startup(&root).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(fs::read(&source).unwrap(), before);
        assert!(!root.join("backups/schema-migration").exists());
    }
}
