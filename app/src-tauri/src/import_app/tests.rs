use super::*;
use crate::backup::restore_context::activation::startup;
use std::{fs, path::PathBuf, process::Command};

fn archive(root: &Path) -> PathBuf {
    let installation = root.join("installation-a");
    let state = DatabaseState::default();
    let source = state.initialize_legacy_startup(&installation).unwrap();
    let db = Connection::open(&source).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('legacy','old date','Åäö','{ \"text\":\"Original\",\"text\":\"🌿\" }',1,'created','updated')", []).unwrap();
    let path = root.join("journal.mno-export");
    db.close().unwrap();
    let mut db = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut db, &path, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    drop(state);
    fs::remove_dir_all(&installation).unwrap();
    assert!(!installation.exists(), "B must work without installation A");
    path
}
fn archive_v5(root: &Path) -> PathBuf {
    let path = root.join("source-v5.sqlite");
    crate::database::create_current_database(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('legacy','old date','Åäö','{ \"text\":\"Original\",\"text\":\"🌿\" }',1,'created','updated')", []).unwrap();
    db.execute(
        "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,NULL,6)",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let archive = root.join("journal.mno-export");
    let mut source = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut source, &archive, &AtomicBool::new(false)).unwrap();
    source.close().unwrap();
    archive
}
fn fresh(root: &Path) -> (DatabaseState, PathBuf) {
    let state = DatabaseState::default();
    let active = state.initialize_legacy_startup(&root.join("data")).unwrap();
    (state, active)
}
fn current(root: &Path) -> (DatabaseState, PathBuf) {
    let state = DatabaseState::default();
    let active = state
        .initialize_schema4_startup(&root.join("current-data"))
        .unwrap();
    (state, active)
}

#[test]
fn schema3_ready_state_rejects_legacy_import_without_changes() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = current(root.path());
    let before = fs::read(&active).unwrap();
    assert!(matches!(
        prepare(&state, &package),
        Err(Error::InvalidPackage)
    ));
    assert_eq!(fs::read(active).unwrap(), before);
    assert!(state.operation(None).is_ok());
    assert!(!root.path().join("current-data/recovery").exists());
}

#[test]
fn format5_preview_then_explicit_confirmation_replaces_existing_journal() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let source = root.path().join("source-v5.sqlite");
    crate::database::create_current_database(&source).unwrap();
    let db = Connection::open(&source).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES('incoming','2026-10-23','new','{}',1,'c','u')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Unused',1)", []).unwrap();
    db.execute(
        "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,NULL,8)",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let package = root.path().join("incoming.mno-export");
    let mut readonly =
        Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut readonly, &package, &AtomicBool::new(false)).unwrap();
    readonly.close().unwrap();
    let (state, active) = current(root.path());
    let old = Connection::open(&active).unwrap();
    old.execute(
        "INSERT INTO journal_entries VALUES('previous','2026-10-22','old','{}',1,'c','u')",
        [],
    )
    .unwrap();
    old.close().unwrap();
    let before = fs::read(&active).unwrap();
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(
        (
            choice.old_content.version,
            choice.old_content.entry_count,
            choice.new_content.version,
            choice.entry_count
        ),
        (5, 1, 5, 1)
    );
    assert_eq!(fs::read(&active).unwrap(), before);
    assert!(!root.path().join("data/recovery").exists());
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let reply = confirm(&state, &RestoreState::default(), &imports, &token, || {});
    assert_eq!(reply.error, None);
    assert!(reply.activated);
    assert_eq!(reply.mode, Mode::Blocked);
    let marker: serde_json::Value = serde_json::from_slice(
        &fs::read(root.path().join("current-data/recovery/activation-v1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(marker["format"], "mno-open-import-activation");
    assert_eq!(marker["version"], 6);
    assert_eq!(marker["schema_version"], 4);
    assert_eq!(marker["old_schema_version"], 4);
    assert_eq!(marker["export_format_version"], 5);
    assert_eq!(marker["old_content"]["version"], 5);
    assert_eq!(marker["new_content"]["version"], 5);
    let mut current =
        Connection::open_with_flags(&active, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        crate::open_import::verify_database_version(&mut current, &AtomicBool::new(false), 5)
            .unwrap()
            .entry_count,
        1
    );
    assert_eq!(
        current
            .query_row("SELECT COUNT(*) FROM places", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        current
            .query_row("SELECT revision FROM weather_default_place", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        8
    );
    current.close().unwrap();
    assert!(state.operation(None).is_err());
    drop(state);
    let restarted = DatabaseState::default();
    assert!(
        restarted
            .initialize_startup(&root.path().join("current-data"))
            .is_err()
    );
    let view = startup::status(&restarted).unwrap();
    assert_eq!(view.actions, vec![startup::Action::FinishNew]);
    startup::run(&restarted, startup::Action::FinishNew).unwrap();
    assert!(restarted.operation(None).is_ok());
}
#[test]
fn format5_preview_counts_default_tombstone_as_existing_data() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive_v5(root.path());
    let (state, active) = current(root.path());
    let db = Connection::open(&active).unwrap();
    db.execute(
        "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,NULL,11)",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let before = fs::read(&active).unwrap();
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(choice.old_content.entry_count, 0);
    assert_eq!(choice.existing_place_count, 0);
    assert_eq!(choice.existing_setting_count, 1);
    assert_eq!(choice.imported_setting_count, 1);
    assert_eq!(fs::read(active).unwrap(), before);
    assert!(state.operation(None).is_ok());
}
#[test]
fn format5_preview_counts_places_even_when_current_journal_has_no_entries() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let source = root.path().join("source-v5.sqlite");
    crate::database::create_current_database(&source).unwrap();
    let archive = root.path().join("incoming.mno-export");
    let mut readonly =
        Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut readonly, &archive, &AtomicBool::new(false)).unwrap();
    readonly.close().unwrap();
    let (state, active) = current(root.path());
    let current_db = Connection::open(&active).unwrap();
    current_db.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Only place',1)", []).unwrap();
    current_db.close().unwrap();
    let before = fs::read(&active).unwrap();
    let choice = prepare(&state, &archive).unwrap();
    assert_eq!(choice.old_content.entry_count, 0);
    assert_eq!(choice.existing_place_count, 1);
    assert_eq!(choice.imported_place_count, 0);
    assert_eq!(fs::read(active).unwrap(), before);
    assert!(!root.path().join("data/recovery").exists());
}

#[test]
fn ordinary_schema2_import_rejects_pre_current_packages_without_writes() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = current(root.path());
    let before = fs::read(&active).unwrap();
    assert!(matches!(
        prepare(&state, &package),
        Err(Error::InvalidPackage)
    ));
    assert_eq!(fs::read(active).unwrap(), before);
    assert!(state.operation(None).is_ok());
    assert!(!root.path().join("current-data/recovery").exists());
}
#[test]
fn preview_cancel_and_bad_package_never_latch_or_change_journal() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = fresh(root.path());
    let bytes = fs::read(&active).unwrap();
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(choice.entry_count, 1);
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    imports.cancel(&token).unwrap();
    imports.cancel(&token).unwrap();
    assert!(matches!(imports.take("old"), Err(Error::Changed)));
    fs::write(&package, b"broken").unwrap();
    assert!(matches!(
        prepare(&state, &package),
        Err(Error::InvalidPackage)
    ));
    assert!(state.operation(None).is_ok());
    assert_eq!(fs::read(&active).unwrap(), bytes);
    assert!(!root.path().join("data/recovery").exists());
}
#[test]
fn hidden_empty_stored_row_is_counted_in_replacement_preview() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = fresh(root.path());
    let db = Connection::open(&active).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('hidden','2026-10-24','','{}',1,'c','u')",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let bytes = fs::read(&active).unwrap();
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(choice.old_content.entry_count, 1);
    assert_eq!(choice.entry_count, 1);
    assert!(state.operation(None).is_ok());
    assert_eq!(fs::read(active).unwrap(), bytes);
}
#[test]
fn content_change_with_same_length_is_rejected_before_session() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = fresh(root.path());
    let choice = prepare(&state, &package).unwrap();
    let mut bytes = fs::read(&package).unwrap();
    bytes[0] ^= 1;
    fs::write(&package, bytes).unwrap();
    let before = fs::read(&active).unwrap();
    assert_eq!(activate(&state, choice), Err(Error::Changed));
    assert!(state.operation(None).is_ok());
    assert_eq!(fs::read(active).unwrap(), before);
}
#[test]
fn token_is_one_use_and_reservation_excludes_parallel_confirmation_and_quit() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, _) = fresh(root.path());
    let imports = ImportState::default();
    let choice = prepare(&state, &package).unwrap();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let shared = RestoreState::default();
    std::thread::scope(|scope| {
        let reservation = shared.reserve().unwrap();
        scope
            .spawn(|| {
                assert!(shared.is_busy());
                assert!(shared.reserve().is_err());
            })
            .join()
            .unwrap();
        let choice = imports.take(&token).unwrap();
        assert!(matches!(imports.take(&token), Err(Error::Changed)));
        drop(choice);
        drop(reservation);
    });
    assert!(!shared.is_busy());
    let newer = prepare(&state, &package).unwrap();
    let newer_token = newer.token.clone();
    *imports.selected.lock().unwrap() = Some(newer);
    assert!(matches!(imports.take("invalid"), Err(Error::Changed)));
    assert!(imports.take(&newer_token).is_ok());
}
#[test]
fn stale_generation_is_pre_session_and_report_keeps_ready() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, _) = fresh(root.path());
    let choice = prepare(&state, &package).unwrap();
    state.maintenance().unwrap().advance_generation().unwrap();
    let reply = report(&state, activate(&state, choice), || {
        panic!("no latch event")
    });
    assert_eq!(reply.error, Some(Error::Changed));
    assert!(!reply.activated);
    assert_eq!(reply.mode, Mode::Ready);
}
const CHILD: &str = "import_app::tests::import_app_child";
#[test]
fn import_app_child() {
    let Ok(scenario) = std::env::var("MNO_IMPORT_APP_SCENARIO") else {
        return;
    };
    crate::panic_policy::install();
    let root = PathBuf::from(std::env::var_os("MNO_IMPORT_APP_ROOT").unwrap());
    let state = DatabaseState::default();
    if matches!(scenario.as_str(), "restart" | "restart-before") {
        assert!(
            state
                .initialize_schema4_startup(&root.join("data"))
                .is_err()
        );
        assert_eq!(mode(&state), Mode::Recovery);
        startup::run(
            &state,
            if scenario == "restart-before" {
                startup::Action::Continue
            } else {
                startup::Action::FinishNew
            },
        )
        .unwrap();
        let db = Connection::open(state.ready_database_path().unwrap()).unwrap();
        let row = db.query_row("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries", [], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))).unwrap();
        assert_eq!(
            row,
            (
                "legacy".into(),
                "old date".into(),
                "Åäö".into(),
                "{ \"text\":\"Original\",\"text\":\"🌿\" }".into(),
                1,
                "created".into(),
                "updated".into()
            )
        );
        db.close().unwrap();
        std::process::exit(0);
    }
    let package = archive_v5(&root);
    let active = state
        .initialize_schema4_startup(&root.join("data"))
        .unwrap();
    let choice = prepare(&state, &package).unwrap();
    if scenario == "occupied-late" {
        let db = Connection::open(&active).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('late','2026-10-24','','{}',1,'c','u')",
            [],
        )
        .unwrap();
        db.close().unwrap();
    }
    let shared = RestoreState::default();
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let mut events = 0;
    let reply = confirm(&state, &shared, &imports, &token, || {
        assert!(
            shared.is_busy(),
            "reservation lasts through status publication"
        );
        assert!(
            shared.reserve().is_err(),
            "quit/concurrent confirmation excluded"
        );
        events += 1;
    });
    assert_eq!(
        reply.mode,
        if scenario == "occupied-late" {
            Mode::Ready
        } else {
            Mode::Blocked
        }
    );
    assert_eq!(events, usize::from(scenario != "occupied-late"));
    assert_eq!(state.operation(None).is_ok(), scenario == "occupied-late");
    assert!(!shared.is_busy());
    assert!(matches!(imports.take(&token), Err(Error::Changed)));
    assert_eq!(reply.activated, scenario == "success");
    if scenario != "success" {
        assert_eq!(
            reply.error,
            Some(if scenario == "occupied-late" {
                Error::Changed
            } else {
                Error::Failed
            })
        );
    }
    if scenario == "occupied-late" {
        assert!(!root.join("data/recovery").exists());
        std::process::exit(82);
    }
    assert!(root.join("data/recovery/activation-v1.json").exists());
    std::process::exit(81);
}
fn launch(root: &Path, scenario: &str, fail: bool, code: i32) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_IMPORT_APP_SCENARIO", scenario)
        .env("MNO_IMPORT_APP_ROOT", root)
        .env("MNO_ACTIVATION_ROOT", root)
        .env("TMPDIR", root)
        .env(
            "MNO_ACTIVATION_MODE",
            if scenario == "early-error" {
                "fail:before-replace"
            } else if fail {
                "fail:disk-verified"
            } else {
                "success"
            },
        )
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(code),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn format5_activation_interruptions_expose_recovery_then_separate_startup_verifies() {
    for (scenario, fail) in [
        ("success", false),
        ("early-error", true),
        ("late-error", true),
    ] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        launch(root.path(), scenario, fail, 81);
        let backups = fs::read_dir(root.path().join("data/backups/pre-restore"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let protection = fs::read(&backups[0]).unwrap();
        launch(
            root.path(),
            if scenario == "early-error" {
                "restart-before"
            } else {
                "restart"
            },
            false,
            0,
        );
        assert_eq!(fs::read(&backups[0]).unwrap(), protection);
    }
}
#[test]
fn final_content_check_failure_before_session_keeps_current_database_ready() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    launch(root.path(), "occupied-late", false, 82);
    let state = DatabaseState::default();
    let active = state.initialize_startup(&root.path().join("data")).unwrap();
    let db = Connection::open(active).unwrap();
    assert_eq!(
        db.query_row("SELECT id FROM journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "late"
    );
}

#[test]
fn busy_confirmation_keeps_selection_and_reports_unknown_without_waiting_for_maintenance() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, _) = fresh(root.path());
    let choice = prepare(&state, &package).unwrap();
    let token = choice.token.clone();
    let imports = ImportState::default();
    *imports.selected.lock().unwrap() = Some(choice);
    let shared = RestoreState::default();
    let busy = shared.reserve().unwrap();
    let maintenance = state.maintenance().unwrap();
    let reply = confirm(&state, &shared, &imports, &token, || {
        panic!("no false event")
    });
    assert_eq!(reply.error, Some(Error::Busy));
    assert_eq!(reply.mode, Mode::Unknown);
    assert!(!reply.activated);
    drop(maintenance);
    drop(busy);
    assert!(imports.take(&token).is_ok());
}

#[test]
fn newer_selection_survives_stale_confirmation_and_stale_or_unknown_cancellation() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, _) = fresh(root.path());
    let imports = ImportState::default();
    let old = prepare(&state, &package).unwrap();
    let old_token = old.token.clone();
    *imports.selected.lock().unwrap() = Some(old);
    let newer = prepare(&state, &package).unwrap();
    let newer_token = newer.token.clone();
    assert_ne!(old_token, newer_token);
    *imports.selected.lock().unwrap() = Some(newer);
    let shared = RestoreState::default();
    let reply = confirm(&state, &shared, &imports, &old_token, || {
        panic!("no RecoverySession")
    });
    assert_eq!(reply.error, Some(Error::Changed));
    assert_eq!(reply.mode, Mode::Ready);
    assert!(!reply.activated);
    assert_eq!(
        imports.selected.lock().unwrap().as_ref().unwrap().token,
        newer_token
    );
    imports.cancel(&old_token).unwrap();
    imports.cancel("unknown").unwrap();
    assert_eq!(
        imports.selected.lock().unwrap().as_ref().unwrap().token,
        newer_token
    );
    let consumed = imports.take(&newer_token).unwrap();
    assert_eq!(consumed.token, newer_token);
    assert!(matches!(imports.take(&newer_token), Err(Error::Changed)));
    imports.cancel(&newer_token).unwrap();
    assert!(state.operation(None).is_ok());
}

#[test]
fn cancelled_picker_cannot_clear_a_selection_published_after_its_reservation() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, _) = fresh(root.path());
    let imports = ImportState::default();
    let old = prepare(&state, &package).unwrap();
    let old_token = old.token.clone();
    *imports.selected.lock().unwrap() = Some(old);
    let shared = RestoreState::default();
    // Same boundaries as choose_open_import: reserve, clear, native picker.
    // A None from the native picker returns without publishing or late cleanup.
    let picker = shared.reserve().unwrap();
    imports.clear().unwrap();
    assert!(imports.selected.lock().unwrap().is_none());
    std::thread::scope(|scope| {
        scope
            .spawn(|| assert!(shared.reserve().is_err()))
            .join()
            .unwrap();
    });
    drop(picker); // Native cancellation: no token was created.
    let newer = prepare(&state, &package).unwrap();
    let newer_token = newer.token.clone();
    let selection = shared.reserve().unwrap();
    *imports.selected.lock().unwrap() = Some(newer);
    drop(selection);
    imports.cancel(&old_token).unwrap();
    assert!(imports.take(&newer_token).is_ok());
}

fn existing_journal(path: &Path) {
    let db = Connection::open(path).unwrap();
    for (id, doc) in [
        ("previous-a", "{}"),
        (
            "previous-b",
            "{ \"text\":\"Original\",\"text\":\"åäö 🌿\" }",
        ),
    ] {
        db.execute("INSERT INTO journal_entries VALUES(?1,'legacy date','old title',?2,1,'raw created','raw updated')", rusqlite::params![id,doc]).unwrap();
    }
    db.close().unwrap();
}
#[test]
fn existing_preview_counts_and_same_generation_content_change_require_new_confirmation() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = fresh(root.path());
    existing_journal(&active);
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(choice.old_content.entry_count, 2);
    assert_eq!(choice.entry_count, 1);
    assert_eq!(choice.new_content.entry_count, 1);
    let generation = choice.generation;
    let db = Connection::open(&active).unwrap();
    db.execute(
        "UPDATE journal_entries SET title='new title' WHERE id='previous-a'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    assert_eq!(
        state.operation(None).unwrap().snapshot(()).generation,
        generation
    );
    let before = fs::read(&active).unwrap();
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let reply = confirm(&state, &RestoreState::default(), &imports, &token, || {
        panic!("no latch")
    });
    assert_eq!(reply.error, Some(Error::Changed));
    assert_eq!(reply.mode, Mode::Ready);
    assert!(!reply.prepared && !reply.activated);
    assert!(state.operation(None).is_ok());
    assert!(matches!(imports.take(&token), Err(Error::Changed)));
    assert_eq!(fs::read(&active).unwrap(), before);
    assert!(!root.path().join("data/recovery").exists());
    let fresh = prepare(&state, &package).unwrap();
    assert_eq!(fresh.old_content.entry_count, 2);
}
#[test]
fn replacement_export_change_is_rejected_before_recovery() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let package = archive(root.path());
    let (state, active) = fresh(root.path());
    existing_journal(&active);
    let choice = prepare(&state, &package).unwrap();
    let mut changed = fs::read(&package).unwrap();
    changed[0] ^= 1;
    fs::write(&package, changed).unwrap();
    let before = fs::read(&active).unwrap();
    assert_eq!(activate(&state, choice), Err(Error::Changed));
    assert!(state.operation(None).is_ok());
    assert_eq!(fs::read(active).unwrap(), before);
}
const REPLACEMENT_CHILD: &str = "import_app::tests::replacement_child";
#[test]
fn replacement_child() {
    let Ok(scenario) = std::env::var("MNO_REPLACEMENT_SCENARIO") else {
        return;
    };
    crate::panic_policy::install();
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let package = archive(&root);
    let (state, active) = fresh(&root);
    existing_journal(&active);
    let before = fs::read(&active).unwrap();
    fs::write(root.join("active-before"), &before).unwrap();
    let choice = prepare(&state, &package).unwrap();
    let old = choice.old_content.clone();
    let new = choice.new_content.clone();
    if scenario == "protection-error" {
        fs::create_dir(root.join("data/backups")).unwrap();
        fs::write(root.join("data/backups/pre-restore"), b"keep existing file").unwrap();
    }
    if scenario == "candidate-error" {
        fs::create_dir_all(root.join("data/backups")).unwrap();
        fs::write(
            root.join("data/backups/import-staging"),
            b"keep existing file",
        )
        .unwrap();
    }
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let shared = RestoreState::default();
    let mut events = 0;
    let reply = confirm(&state, &shared, &imports, &token, || {
        assert!(shared.is_busy());
        events += 1;
    });
    assert_eq!(events, 1);
    assert_eq!(reply.mode, Mode::Blocked);
    assert!(!reply.activated);
    assert!(state.operation(None).is_err());
    assert!(!shared.is_busy());
    assert_eq!(
        fs::read(&active).unwrap(),
        before,
        "active SQLite bytes must never change"
    );
    assert!(matches!(imports.take(&token), Err(Error::Changed)));
    if scenario == "success" || scenario == "previous" {
        assert!(reply.prepared);
        assert_eq!(reply.error, None);
        let path = root.join("data/recovery/activation-v1.json");
        let m: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(m["format"], "mno-open-import-activation");
        assert_eq!(m["version"], 2);
        assert_eq!(m["target"], "new");
        assert_eq!(m["old_content"]["sha256"], old.sha256);
        assert_eq!(m["old_content"]["entry_count"], 2);
        assert_eq!(m["new_content"]["sha256"], new.sha256);
        assert_eq!(m["new_content"]["entry_count"], 1);
        let candidate = root
            .join("data/recovery")
            .join(m["candidate_file"].as_str().unwrap());
        assert_eq!(
            crate::backup::restore_context::activation::journal_proof(&candidate).unwrap(),
            new
        );
        let backup = root
            .join("data/backups/pre-restore")
            .join(m["protection_file"].as_str().unwrap());
        let protected = fs::read(&backup).unwrap();
        drop(state);
        let restart = DatabaseState::default();
        assert!(
            restart
                .initialize_legacy_startup(&root.join("data"))
                .is_err()
        );
        let view = startup::status(&restart).unwrap();
        let replacement = view.replacement.unwrap();
        assert_eq!(replacement.existing_entry_count, 2);
        assert_eq!(replacement.imported_entry_count, 1);
        assert_eq!(replacement.target, "new");
        // Mere status, or an unconfirmed rollback, must not change anything.
        assert!(startup::run(&restart, startup::Action::KeepPrevious).is_err());
        assert!(restart.operation(None).is_err());
        assert_eq!(fs::read(&active).unwrap(), before);
        if scenario == "previous" {
            let held = crate::recovery::run_owned(
                &restart,
                &shared,
                startup::Action::KeepPrevious,
                replacement.previous_confirmation.as_deref(),
            )
            .unwrap();
            assert!(
                shared.is_busy(),
                "normal exit remains reserved through publication"
            );
            assert!(shared.reserve().is_err());
            drop(held);
        } else {
            let held =
                crate::recovery::run_owned(&restart, &shared, startup::Action::Continue, None)
                    .unwrap();
            assert!(shared.is_busy());
            assert!(shared.reserve().is_err());
            drop(held);
        }
        assert!(restart.operation(None).is_ok());
        assert_eq!(
            crate::backup::restore_context::activation::journal_proof(&active).unwrap(),
            if scenario == "previous" {
                old.clone()
            } else {
                new.clone()
            }
        );
        assert_eq!(fs::read(backup).unwrap(), protected);
        assert!(
            startup::status(&restart).is_err(),
            "no historical rollback after Ready"
        );
        assert!(!root.join("data/recovery").exists());
        drop(restart);
        let normal = DatabaseState::default();
        let opened = normal
            .initialize_legacy_startup(&root.join("data"))
            .unwrap();
        assert!(normal.operation(None).is_ok());
        assert_eq!(
            crate::backup::restore_context::activation::journal_proof(&opened).unwrap(),
            if scenario == "previous" { old } else { new }
        );
    } else {
        assert!(!reply.prepared);
        assert_eq!(reply.error, Some(Error::Failed));
    }
    eprintln!("REPLACEMENT_VERIFIED:{scenario}");
    std::process::exit(84);
}
fn replacement_launch(root: &Path, scenario: &str, point: Option<&str>) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", REPLACEMENT_CHILD, "--nocapture"])
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_REPLACEMENT_SCENARIO", scenario)
        .env(
            "MNO_ACTIVATION_MODE",
            point.map(|p| format!("fail:{p}")).unwrap_or_default(),
        )
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(84),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(&format!("REPLACEMENT_VERIFIED:{scenario}"))
    );
}
#[test]
fn replacement_preparation_then_production_startup_selects_new_or_previous() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    replacement_launch(root.path(), "success", None);
    let previous = tempfile::tempdir_in("/private/tmp").unwrap();
    replacement_launch(previous.path(), "previous", None);
}
#[test]
fn replacement_preparation_failures_never_report_success_or_change_active_journal() {
    for point in [
        "replacement-before-protection",
        "replacement-protected",
        "replacement-before-build",
        "replacement-built",
        "replacement-closed",
        "replacement-sync-candidate",
        "replacement-before-publish",
        "replacement-after-publish",
        "replacement-sync-root",
        "replacement-prepared",
    ] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        replacement_launch(root.path(), "injected-error", Some(point));
        assert_eq!(
            fs::read(root.path().join("data/mno.sqlite")).unwrap(),
            fs::read(root.path().join("active-before")).unwrap()
        );
        let state = DatabaseState::default();
        let start = state.initialize_legacy_startup(&root.path().join("data"));
        if [
            "replacement-after-publish",
            "replacement-sync-root",
            "replacement-prepared",
        ]
        .contains(&point)
        {
            assert!(start.is_err());
            assert!(state.operation(None).is_err());
        } else {
            start.unwrap();
            assert!(state.operation(None).is_ok());
        }
    }
    for scenario in ["protection-error", "candidate-error"] {
        let root = tempfile::tempdir_in("/private/tmp").unwrap();
        replacement_launch(root.path(), scenario, None);
    }
}

#[test]
fn format6_preview_confirmation_and_restart_preserve_observations() {
    let root = tempfile::tempdir_in("/private/tmp").unwrap();
    let source = root.path().join("source-v6.sqlite");
    crate::database::create_current_database(&source).unwrap();
    let mut db = crate::database::open_existing(&source).unwrap();
    crate::database::bird_schema::upgrade(&mut db).unwrap();
    db.execute_batch("INSERT INTO journal_entries VALUES('incoming','2026-11-12','New','{}',1,'c','u');
        INSERT INTO places VALUES('00000000-0000-4000-8000-000000000001','Lake',59,18,1);
        INSERT INTO bird_lists VALUES('linked','2026-11-12','00000000-0000-4000-8000-000000000001');
        INSERT INTO bird_lists VALUES('unlinked','2026-11-13','00000000-0000-4000-8000-000000000001');
        INSERT INTO bird_rows VALUES('row1','linked',0,'synthetic-1','AviList v2026',6);
        INSERT INTO bird_rows VALUES('row2','unlinked',0,'synthetic-2','AviList v2026',2);
        INSERT INTO journal_bird_lists VALUES('incoming','linked');").unwrap();
    let expected =
        crate::open_import::verify_database_version(&mut db, &AtomicBool::new(false), 6).unwrap();
    db.close().unwrap();
    let package = root.path().join("birds.mno-export");
    let mut readonly =
        Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut readonly, &package, &AtomicBool::new(false)).unwrap();
    readonly.close().unwrap();
    let state = DatabaseState::default();
    let active = state
        .initialize_schema5_startup(&root.path().join("current-data"))
        .unwrap();
    let before = fs::read(&active).unwrap();
    // Recompute valid checksums around invalid observations: rejection must
    // happen on content, before any active journal or Recovery publication changes.
    let damaged = root.path().join("invalid-birds.mno-export");
    {
        let mut zip = zip::ZipArchive::new(File::open(&package).unwrap()).unwrap();
        let mut payloads = std::collections::BTreeMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            payloads.insert(f.name().to_owned(), bytes);
        }
        let mut rows: Vec<serde_json::Value> = std::str::from_utf8(&payloads["bird_rows.jsonl"])
            .unwrap()
            .lines()
            .map(|r| serde_json::from_str(r).unwrap())
            .collect();
        rows[0]["count"] = serde_json::json!(0);
        let mut bytes = Vec::new();
        for row in rows {
            serde_json::to_writer(&mut bytes, &row).unwrap();
            bytes.push(b'\n');
        }
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&payloads["manifest.json"]).unwrap();
        manifest["files"]["bird_rows.jsonl"]["size"] = serde_json::json!(bytes.len());
        manifest["files"]["bird_rows.jsonl"]["sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
        payloads.insert("bird_rows.jsonl".into(), bytes);
        payloads.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let mut out = zip::ZipWriter::new(File::create(&damaged).unwrap());
        for (name, bytes) in payloads {
            out.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            out.write_all(&bytes).unwrap();
        }
        out.finish().unwrap();
    }
    assert!(matches!(
        prepare(&state, &damaged),
        Err(Error::InvalidPackage)
    ));
    assert_eq!(fs::read(&active).unwrap(), before);
    assert!(state.operation(None).is_ok());
    assert!(!root.path().join("current-data/recovery").exists());
    let choice = prepare(&state, &package).unwrap();
    assert_eq!(choice.new_content, expected);
    assert_eq!(choice.old_content.version, 6);
    assert_eq!(fs::read(&active).unwrap(), before);
    assert!(!root.path().join("current-data/recovery").exists());
    let imports = ImportState::default();
    let token = choice.token.clone();
    *imports.selected.lock().unwrap() = Some(choice);
    let reply = confirm(&state, &RestoreState::default(), &imports, &token, || {});
    assert_eq!(reply.error, None);
    assert!(reply.activated);
    assert_eq!(reply.mode, Mode::Blocked);
    assert!(state.operation(None).is_err());
    drop(state);
    let restarted = DatabaseState::default();
    assert!(
        restarted
            .initialize_schema5_startup(&root.path().join("current-data"))
            .is_err()
    );
    assert_eq!(
        startup::status(&restarted).unwrap().operation,
        startup::Operation::Import
    );
    startup::run(&restarted, startup::Action::FinishNew).unwrap();
    assert_eq!(
        crate::backup::restore_context::activation::journal_proof(&active).unwrap(),
        expected
    );
    // A format-5 archive retains its source identity while the private candidate
    // is normalized to proof 6 before explicit confirmation.
    let old_archive = archive_v5(root.path());
    let bytes = fs::read(&active).unwrap();
    let previous = crate::backup::restore_context::activation::journal_proof(&active).unwrap();
    let normalized = prepare(&restarted, &old_archive).unwrap();
    assert_eq!(normalized.source_format, 5);
    assert_eq!(normalized.old_content.version, 6);
    assert_eq!(normalized.new_content.version, 6);
    let expected_normalized = normalized.new_content.clone();
    assert_eq!(fs::read(&active).unwrap(), bytes);
    assert!(restarted.operation(None).is_ok());
    let token = normalized.token.clone();
    *imports.selected.lock().unwrap() = Some(normalized);
    let result = confirm(
        &restarted,
        &RestoreState::default(),
        &imports,
        &token,
        || {},
    );
    assert_eq!(result.error, None);
    assert!(result.activated);
    drop(restarted);
    let final_start = DatabaseState::default();
    assert!(
        final_start
            .initialize_schema5_startup(&root.path().join("current-data"))
            .is_err()
    );
    startup::run(&final_start, startup::Action::FinishNew).unwrap();
    assert_eq!(
        crate::backup::restore_context::activation::journal_proof(&active).unwrap(),
        expected_normalized
    );
    let mut protected_previous = false;
    for archive in fs::read_dir(root.path().join("current-data/backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mno-backup"))
    {
        let candidate =
            crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
                .unwrap();
        let proof = crate::storage_content::verified_snapshot_v6(
            candidate.source().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap();
        if proof.sha256 == previous.sha256 {
            protected_previous = true;
            assert_eq!(
                candidate
                    .source()
                    .unwrap()
                    .query_row("SELECT count(*) FROM bird_lists", [], |r| r
                        .get::<_, u32>(0))
                    .unwrap(),
                2
            );
        }
    }
    assert!(protected_previous);
}
