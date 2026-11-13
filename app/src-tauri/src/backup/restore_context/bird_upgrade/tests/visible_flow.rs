//! Schema-6 is exercised through ordinary startup; older fixtures remain frozen.
use super::*;
use crate::{bird_observations as birds, database_state::DatabaseState};
fn source(root: &Path) -> (DatabaseState, PathBuf, String) {
    fs::create_dir(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize(&path).unwrap();
    let old = fs::read(&path).unwrap();
    let state = DatabaseState::default();
    let path = state.initialize_schema6_startup(root).unwrap();
    let copies: Vec<_> = fs::read_dir(root.join("backups/bird-schema-upgrade"))
        .unwrap()
        .map(|e| e.unwrap().path().join("schema5.sqlite"))
        .filter(|p| p.exists())
        .collect();
    assert_eq!(copies.len(), 1);
    assert_eq!(fs::read(&copies[0]).unwrap(), old);
    let db = crate::database::open_existing(&path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    close(db);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let rows = [
        birds::SpeciesCount {
            avibase_id: "avibase-DF1E2A1D".into(),
            taxonomy_version: crate::species_registry::VERSION.into(),
            count: 6,
            comment: None,
        },
        birds::SpeciesCount {
            avibase_id: "avibase-40329BB6".into(),
            taxonomy_version: crate::species_registry::VERSION.into(),
            count: 2,
            comment: None,
        },
    ];
    let id = birds::create_with_visit(
        &state,
        &crate::journal_search::SearchState::default(),
        g,
        "2026-11-12",
        &place,
        &rows,
        Some("entry"),
        Some("08:15"),
        Some("09:30"),
    )
    .unwrap();
    (state, path, id)
}
#[test]
fn visit_flow_saves_atomically_reopens_and_rejects_invalid_visit() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("data");
    let (state, path, id) = source(&root);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let saved = birds::reopen(&state, g, &id).unwrap();
    assert_eq!(saved.visit_start.as_deref(), Some("08:15"));
    assert_eq!(saved.visit_end.as_deref(), Some("09:30"));
    assert_eq!(saved.journal_entries, vec!["entry"]);
    assert_eq!(saved.rows[0].count, 6);
    let before = fs::read(&path).unwrap();
    assert!(
        birds::create_with_visit(
            &state,
            &crate::journal_search::SearchState::default(),
            g,
            "2026-11-13",
            &saved.place_id,
            &saved.rows,
            Some("entry"),
            Some("25:00"),
            None
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(state);
    let reopened = DatabaseState::default();
    reopened.initialize_schema6_startup(&root).unwrap();
    let g = reopened.operation(None).unwrap().snapshot(()).generation;
    assert_eq!(birds::reopen(&reopened, g, &id).unwrap(), saved);
}
#[test]
fn visit_protocols_preserve_times_and_old_contracts_through_activation() {
    for mode in ["restore", "import7", "import6", "import5"] {
        let root = TempDir::new().unwrap();
        let out=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::bird_upgrade::tests::visible_flow::visit_activation_child","--nocapture"]).env("MNO_VISIT_PROTOCOL",mode).env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE","success").output().unwrap();
        assert!(out.status.success(), "{mode}: {out:?}");
    }
}
#[test]
fn visit_activation_child() {
    let Ok(mode) = std::env::var("MNO_VISIT_PROTOCOL") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let (state, path, id) = source(&root.join("source"));
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let saved = birds::reopen(&state, g, &id).unwrap();
    let archive = root.join(if mode == "restore" {
        "incoming.mno-backup"
    } else {
        "incoming.mno-export"
    });
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut expected =
        crate::open_import::verify_database_version(&mut db, &AtomicBool::new(false), 7).unwrap();
    if mode == "restore" {
        crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
    } else if mode == "import7" {
        crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
    }
    close(db);
    drop(state);
    if matches!(mode.as_str(), "import5" | "import6") {
        let olddir = root.join("old");
        fs::create_dir(&olddir).unwrap();
        let (old, _, _) = fixture(&olddir);
        if mode == "import6" {
            initialize(&old).unwrap();
            add_lists(&old);
        }
        let mut db = Connection::open_with_flags(&old, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        close(db);
        let c = crate::open_import::build_for_schema6(
            &mut File::open(&archive).unwrap(),
            &root,
            &AtomicBool::new(false),
            if mode == "import5" { 5 } else { 6 },
        )
        .unwrap();
        expected = c.proof().clone();
        drop(c);
    }
    let data = root.join("data");
    let target = DatabaseState::default();
    let active = target.initialize_schema6_startup(&data).unwrap();
    let old = super::super::super::activation::journal_proof(&active).unwrap();
    let maintenance = target.maintenance().unwrap();
    if mode == "restore" {
        let activated = super::super::super::activation::activate(
            &maintenance,
            &archive,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        drop(activated);
    } else {
        let activated = super::super::super::activation::importing::activate_schema6(
            &maintenance,
            &mut File::open(&archive).unwrap(),
            &old,
            &expected,
            Arc::new(AtomicBool::new(false)),
            match mode.as_str() {
                "import5" => 5,
                "import6" => 6,
                _ => 7,
            },
        )
        .unwrap();
        drop(activated);
    }
    drop(maintenance);
    drop(target);
    let target = DatabaseState::default();
    assert!(target.initialize_schema6_startup(&data).is_err());
    super::super::super::activation::startup::run(
        &target,
        super::super::super::activation::startup::Action::FinishNew,
    )
    .unwrap();
    assert_eq!(
        super::super::super::activation::journal_proof(&active).unwrap(),
        expected
    );
    if matches!(mode.as_str(), "restore" | "import7") {
        let g = target.operation(None).unwrap().snapshot(()).generation;
        assert_eq!(birds::reopen(&target, g, &id).unwrap(), saved);
    }
}

#[test]
fn visit_schema_upgrade_interruption_and_invalid_candidate_preserve_schema5_source() {
    for boundary in ["upgraded", "before-publish", "after-publish"] {
        let root = TempDir::new().unwrap();
        let (path, _, _) = fixture(root.path());
        initialize(&path).unwrap();
        add_lists(&path);
        let bytes = fs::read(&path).unwrap();
        let mut db = crate::database::open_existing(&path).unwrap();
        let proof = full(&mut db, 5).unwrap();
        close(db);
        assert!(
            initialize_to(&path, 6, |name, _| {
                if name == boundary {
                    Err("injected interruption".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        if boundary != "after-publish" {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        initialize_schema6(&path).unwrap();
        assert_eq!(verify_for(&path, 6, 5).unwrap(), proof);
        assert!(
            fs::read_dir(root.path().join("backups/bird-schema-upgrade"))
                .unwrap()
                .any(|e| fs::read(e.unwrap().path().join("schema5.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes))
        );
    }
    let root = TempDir::new().unwrap();
    let (path, _, _) = fixture(root.path());
    initialize(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        initialize_to(&path, 6, |name, candidate| {
            if name == "upgraded" {
                let db = crate::database::open_existing(candidate).unwrap();
                db.execute_batch("DROP TABLE bird_rows").unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
