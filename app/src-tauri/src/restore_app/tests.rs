use super::*;
use crate::backup::{self, BackupKind, restore_context::activation::startup};
use rusqlite::{Connection, OptionalExtension};
use std::{fs, path::Path, process::Command, sync::Arc};
const CHILD: &str = "restore_app::tests::app_restore_child";
fn content() -> &'static str {
    r#"{ "type":"doc", "content":[{"type":"paragraph","content":[{"type":"text","text":"Fåglar åäö 🌿"}]}] }"#
}
fn create_portable_backup(root: &Path) {
    create_portable_backup_with_setting(root, "active");
}
fn create_portable_backup_with_setting(root: &Path, setting: &str) {
    // The entire original installation is gone before the receiving app runs.
    let source_install = tempfile::tempdir_in("/private/tmp").unwrap();
    let original = DatabaseState::default();
    let source_path = original
        .initialize_schema4_startup(&source_install.path().join("data"))
        .unwrap();
    let source = Connection::open(source_path).unwrap();
    for (id, date, title) in [
        ("portable-a", "2026-10-01", "Fåglar"),
        ("portable-b", "2026-10-02", "Väder"),
    ] {
        source.execute("INSERT INTO journal_entries VALUES (?1,?2,?3,?4,1,'created bytes','updated bytes')",rusqlite::params![id,date,title,content()]).unwrap();
    }
    if setting == "active" {
        source.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES('00000000-0000-4000-8000-000000000001','Backup place',59.3,18.1,2)",[]).unwrap();
        source.execute("INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,'00000000-0000-4000-8000-000000000001',5)",[]).unwrap();
    } else if setting == "tombstone" {
        source
            .execute(
                "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,NULL,9)",
                [],
            )
            .unwrap();
    }
    source.execute("INSERT INTO journal_weather(entry_id,format_version,manual_json,fetched_json,revision) VALUES('portable-a',1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-10-01\"},\"temperatureC\":8}',NULL,3)",[]).unwrap();
    if setting == "active" {
        source.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES('portable-a','00000000-0000-4000-8000-000000000001','Historic place',58.0,17.0,1,4)",[]).unwrap();
    }
    backup::create_backup(
        &source,
        &root.join("portable.mno-backup"),
        BackupKind::Manual,
    )
    .unwrap();
    source.close().unwrap();
    let deleted = source_install.path().to_path_buf();
    drop(original);
    source_install.close().unwrap();
    assert!(!deleted.exists());
    fs::write(root.join("deleted-source"), deleted.to_str().unwrap()).unwrap();
    fs::write(root.join("expected-setting"), setting).unwrap();
}
fn assert_rows(path: &Path) {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut stmt = db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i32>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row.0,
            if index == 0 {
                "portable-a"
            } else {
                "portable-b"
            }
        );
        assert_eq!(
            row.1,
            if index == 0 {
                "2026-10-01"
            } else {
                "2026-10-02"
            }
        );
        assert_eq!(row.2, if index == 0 { "Fåglar" } else { "Väder" });
        assert_eq!(row.3, content());
        assert_eq!(row.4, 1);
        assert_eq!(row.5, "created bytes");
        assert_eq!(row.6, "updated bytes");
    }
    drop(stmt);
    let setting: Option<(Option<String>, i64)> = db
        .query_row(
            "SELECT place_id,revision FROM weather_default_place WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .unwrap();
    let expected = fs::read_to_string(
        path.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("expected-setting"),
    )
    .unwrap();
    assert_eq!(
        setting,
        match expected.as_str() {
            "active" => Some((Some("00000000-0000-4000-8000-000000000001".into()), 5,)),
            "tombstone" => Some((None, 9)),
            "absent" => None,
            _ => panic!("unknown expected setting"),
        }
    );
    let weather: (String, i64) = db
        .query_row(
            "SELECT manual_json,revision FROM journal_weather WHERE entry_id='portable-a'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(weather.1, 3);
    assert!(weather.0.contains("\"temperatureC\":8"));
    if expected == "active" {
        let snapshot: (String, f64, f64, i64, i64) = db
            .query_row(
                "SELECT place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision FROM journal_places WHERE entry_id='portable-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_eq!(snapshot, ("Historic place".into(), 58.0, 17.0, 1, 4));
    }
    db.close().unwrap();
}

#[test]
fn schema3_ready_state_rejects_schema2_restore_without_changes() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let source = root.path().join("schema2.sqlite");
    crate::database::create_new_database(&source).unwrap();
    let mut source_db = Connection::open(&source).unwrap();
    crate::database::place_schema::migrate_candidate(&mut source_db).unwrap();
    let archive = root.path().join("schema2.mno-backup");
    backup::create_backup(&source_db, &archive, BackupKind::Manual).unwrap();
    source_db.close().unwrap();

    let data = root.path().join("schema3-data");
    let state = DatabaseState::default();
    let active = state.initialize_startup(&data).unwrap();
    let before = fs::read(&active).unwrap();
    assert!(matches!(
        validate_selection(&state, archive),
        Err(Error::InvalidBackup)
    ));
    assert_eq!(fs::read(active).unwrap(), before);
    assert!(state.operation(None).is_ok());
    assert!(!data.join("recovery").exists());
}

#[test]
fn app_restore_child() {
    let Ok(mode) = std::env::var("MNO_APP_RESTORE_MODE") else {
        return;
    };
    crate::panic_policy::install();
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let state = DatabaseState::default();
    let data = root.join("data");
    if matches!(mode.as_str(), "restart" | "restart-before") {
        assert!(state.initialize_startup(&data).is_err());
        assert!(state.operation(None).is_err());
        let action = if mode == "restart-before" {
            startup::Action::Continue
        } else {
            startup::Action::FinishNew
        };
        assert!(startup::status(&state).unwrap().actions.contains(&action));
        startup::run(&state, action).unwrap();
        let _operation = state.operation(None).unwrap();
        assert_rows(&data.join("mno.sqlite"));
        std::process::exit(0);
    }
    if mode == "normal" {
        state.initialize_startup(&data).unwrap();
        let _operation = state.operation(None).unwrap();
        assert_rows(&data.join("mno.sqlite"));
        std::process::exit(0);
    }
    // Frozen schema-4 Restore marker contract; schema-5 startup has its own integration coverage.
    let path = state.initialize_schema4_startup(&data).unwrap();
    let before = fs::read(&path).unwrap();
    let archive = root.join("portable.mno-backup");
    if mode == "invalid" {
        fs::write(&archive, b"broken archive").unwrap();
        assert!(matches!(
            validate_selection(&state, archive),
            Err(Error::InvalidBackup)
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(state.operation(None).is_ok());
        assert!(!data.join("recovery").exists());
        std::process::exit(0);
    }
    let choice = validate_selection(&state, archive.clone()).unwrap();
    if mode == "changed" {
        fs::write(&archive, b"changed after confirmation").unwrap();
        assert!(activate_selection(&state, choice).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(state.operation(None).is_ok());
        std::process::exit(0);
    }
    if mode == "stale" {
        state.maintenance().unwrap().advance_generation().unwrap();
        assert_eq!(activate_selection(&state, choice), Err(Error::Changed));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(state.operation(None).is_ok());
        std::process::exit(0);
    }
    if matches!(mode.as_str(), "early-error" | "late-error") {
        assert_eq!(activate_selection(&state, choice), Err(Error::Failed));
        assert!(state.operation(None).is_err());
        assert!(data.join("recovery/activation-v1.json").exists());
        std::process::exit(87);
    }
    activate_selection(&state, choice).unwrap();
    assert!(state.operation(None).is_err(), "disk swap is never Ready");
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(data.join("recovery/activation-v1.json")).unwrap())
            .unwrap();
    assert_eq!(marker["format"], "mno-database-activation");
    assert_eq!(marker["version"], 5);
    assert_eq!(marker["schema_version"], 4);
    assert_eq!(marker["old_schema_version"], 4);
    assert_eq!(marker["backup_format_version"], 5);
    assert_eq!(marker["old_content"]["version"], 5);
    assert_eq!(marker["new_content"]["version"], 5);
    assert!(!Path::new(&fs::read_to_string(root.join("deleted-source")).unwrap()).exists());
    std::process::exit(89);
}
fn launch(root: &Path, mode: &str, code: i32) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_APP_RESTORE_MODE", mode)
        .env(
            "MNO_ACTIVATION_MODE",
            if mode == "early-error" {
                "fail:before-replace"
            } else if mode == "late-error" {
                "fail:disk-verified"
            } else {
                "success"
            },
        )
        .env("TMPDIR", root)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(code),
        "{mode}: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn portable_backup_from_deleted_installation_uses_app_chain() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    create_portable_backup(root.path());
    launch(root.path(), "restore", 89);
    let protections = fs::read_dir(root.path().join("data/backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(protections.len(), 1);
    let protection = fs::read(&protections[0]).unwrap();
    launch(root.path(), "restart", 0);
    launch(root.path(), "normal", 0);
    assert_eq!(fs::read(&protections[0]).unwrap(), protection);
    let backup =
        backup::restore::prepare_candidate(&protections[0], Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        backup
            .source()
            .unwrap()
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i32>(0))
            .unwrap(),
        0
    );
}
#[test]
fn app_restore_preserves_absent_and_tombstone_weather_default_states() {
    for setting in ["absent", "tombstone"] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        create_portable_backup_with_setting(root.path(), setting);
        launch(root.path(), "restore", 89);
        launch(root.path(), "restart", 0);
        launch(root.path(), "normal", 0);
    }
}
#[test]
fn app_rejects_invalid_changed_and_stale_confirmation_without_writes() {
    for mode in ["invalid", "changed", "stale"] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        create_portable_backup(root.path());
        launch(root.path(), mode, 0);
    }
}
#[test]
fn app_selection_reservation_excludes_concurrent_commands() {
    let shared = RestoreState::default();
    let busy = shared.reserve().unwrap();
    assert!(matches!(shared.reserve(), Err(Error::Busy)));
    drop(busy);
    assert!(shared.reserve().is_ok());
}

#[test]
fn app_interruption_before_and_after_replacement_recovers_verified_format5() {
    for mode in ["early-error", "late-error"] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        create_portable_backup(root.path());
        launch(root.path(), mode, 87);
        launch(
            root.path(),
            if mode == "early-error" {
                "restart-before"
            } else {
                "restart"
            },
            0,
        );
        launch(root.path(), "normal", 0);
    }
}

#[test]
fn manual_quit_rejects_ready_and_active_restore_without_bypassing_editor_guard() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let ready = DatabaseState::default();
    ready.initialize_startup(&root.path().join("data")).unwrap();
    let shared = RestoreState::default();
    assert_eq!(
        request_quit(&ready, &shared, || panic!("must use editor close guard")),
        Err(Error::Changed)
    );
    let busy = shared.reserve().unwrap();
    assert_eq!(
        request_quit(&ready, &shared, || panic!("operation is still active")),
        Err(Error::Busy)
    );
    drop(busy);
    assert!(ready.operation(None).is_ok());
}

#[test]
fn manual_quit_requests_exit_once_without_releasing_lease_or_granting_ready() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    fs::create_dir(root.path().join("recovery")).unwrap();
    let state = DatabaseState::default();
    assert!(state.initialize_startup(root.path()).is_err());
    let shared = RestoreState::default();
    let requested = std::cell::Cell::new(0);
    request_quit(&state, &shared, || {
        assert!(
            !shared.is_busy(),
            "normal exit must not block its own ExitRequested"
        );
        requested.set(requested.get() + 1);
    })
    .unwrap();
    assert_eq!(requested.get(), 1);
    assert!(state.operation(None).is_err());
    assert!(state.startup_recovery_pending());
    // The request does not drop/unlock the live process owner. The real exit
    // releases the OS lock; only a later, independently launched process opens it.
    assert_eq!(
        DatabaseState::default()
            .initialize_startup(root.path())
            .unwrap_err(),
        "database_directory_in_use"
    );
}

#[test]
fn editor_window_close_uses_the_shared_work_reservation_and_releases_it() {
    let shared = RestoreState::default();
    // Export, import and restore all reserve this same owner.
    for _ in ["export", "import", "restore"] {
        let busy = shared.reserve().unwrap();
        assert_eq!(
            request_window_close(&shared, || panic!("must stay open")),
            Err(Error::Busy)
        );
        drop(busy);
    }
    let closed = std::cell::Cell::new(0);
    request_window_close(&shared, || {
        assert!(shared.is_busy());
        assert!(shared.reserve().is_err());
        closed.set(closed.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(closed.get(), 1);
    assert!(!shared.is_busy());
    assert_eq!(
        request_window_close(&shared, || Err(Error::Failed)),
        Err(Error::Failed)
    );
    assert!(!shared.is_busy());
    request_window_close(&shared, || Ok(())).unwrap();
}
