use super::*;
use crate::{bird_observations as birds, database_state::DatabaseState};
fn schema6(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    fs::create_dir_all(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize_schema6(&path).unwrap();
    add_lists(&path);
    let db = crate::database::open_existing(&path).unwrap();
    db.execute(
        "UPDATE bird_lists SET visit_start='08:15',visit_end='09:30' WHERE visit_date='2026-11-11'",
        [],
    )
    .unwrap();
    close(db);
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = full(&mut db, 6).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}
fn current(root: &Path) -> (DatabaseState, PathBuf) {
    let (path, bytes, proof) = schema6(root);
    let state = DatabaseState::default();
    state.initialize_schema7_startup(root).unwrap();
    assert_eq!(verify_for(&path, 7, 6).unwrap(), proof);
    assert!(
        fs::read_dir(root.join("backups/bird-schema-upgrade"))
            .unwrap()
            .any(|e| fs::read(e.unwrap().path().join("schema6.sqlite"))
                .ok()
                .as_ref()
                == Some(&bytes))
    );
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let lists = add_current_visits(&state, g, &path);
    assert_ne!(lists[0], lists[1]);
    (state, path)
}
fn add_current_visits(state: &DatabaseState, g: u32, path: &Path) -> Vec<String> {
    let db = crate::database::open_existing(path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    close(db);
    (0..2)
        .map(|_| {
            birds::create_with_identity(
                state,
                &crate::journal_search::SearchState::default(),
                g,
                "2026-11-11",
                &place,
                &[birds::SpeciesCount {
                    avibase_id: "avibase-E15BC0CB".into(),
                    taxonomy_version: crate::species_registry::VERSION.into(),
                    count: 6,
                    comment: None,
                }],
                Some("entry"),
                None,
                None,
                Some(&uuid::Uuid::new_v4().to_string()),
            )
            .unwrap()
        })
        .collect()
}
#[test]
fn multiple_visits_schema6_upgrade_and_archive_roundtrip_preserve_all_identities() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("source");
    let (state, path) = current(&root);
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = full(&mut db, 7).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM bird_lists WHERE visit_date='2026-11-11'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
    assert_eq!(db.query_row("SELECT count(*) FROM bird_lists b WHERE NOT EXISTS(SELECT 1 FROM journal_bird_lists j WHERE j.list_id=b.id)",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    let backup = temp.path().join("visits.mno-backup");
    let manifest =
        crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (8, 7)
    );
    let candidate =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v8(
            candidate.source().unwrap(),
            &AtomicBool::new(false)
        )
        .unwrap(),
        proof
    );
    drop(candidate.try_close().map_err(|_| ()).unwrap());
    let export = temp.path().join("visits.mno-export");
    crate::open_export::export(&mut db, &export, &AtomicBool::new(false)).unwrap();
    close(db);
    let c = crate::open_import::build_v8(
        &mut File::open(&export).unwrap(),
        temp.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(c.proof().sha256, proof.sha256);
    drop(c);
    drop(state);
    let state = DatabaseState::default();
    state.initialize_schema7_startup(&root).unwrap();
    assert_eq!(
        verify_for(&path, 7, 6).unwrap().entry_count,
        proof.entry_count
    );
}
#[test]
fn multiple_visits_schema6_interruption_retry_and_invalid_candidate_keep_source() {
    for boundary in ["upgraded", "before-publish", "after-publish"] {
        let root = TempDir::new().unwrap();
        let (path, bytes, proof) = schema6(root.path());
        assert!(
            initialize_to(&path, 7, |name, _| if name == boundary {
                Err("injected interruption".into())
            } else {
                Ok(())
            })
            .is_err()
        );
        if boundary != "after-publish" {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        initialize_schema7(&path).unwrap();
        assert_eq!(verify_for(&path, 7, 6).unwrap(), proof);
    }
    let root = TempDir::new().unwrap();
    let (path, bytes, _) = schema6(root.path());
    assert!(
        initialize_to(&path, 7, |name, next| {
            if name == "upgraded" {
                let db = crate::database::open_existing(next).unwrap();
                db.execute_batch("DROP TABLE bird_rows").unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[test]
fn multiple_visits_activation_preserves_new_and_older_archive_contracts() {
    for mode in ["restore8", "import8", "import7", "import6", "import5"] {
        let root = TempDir::new().unwrap();
        let out=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::bird_upgrade::tests::multiple_visits::multiple_visits_activation_child","--nocapture"]).env("MNO_MULTIPLE_VISITS",mode).env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE","success").output().unwrap();
        assert!(out.status.success(), "{mode}: {out:?}");
    }
}
#[test]
fn multiple_visits_activation_child() {
    let Ok(mode) = std::env::var("MNO_MULTIPLE_VISITS") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let archive = root.join(if mode == "restore8" {
        "incoming.mno-backup"
    } else {
        "incoming.mno-export"
    });
    if mode.ends_with('8') {
        let (state, path) = current(&root.join("source"));
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        if mode == "restore8" {
            crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        } else {
            crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        }
        close(db);
        drop(state);
    } else {
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        let (path, _, _) = fixture(&source);
        if mode == "import6" {
            initialize(&path).unwrap();
            add_lists(&path);
        } else if mode == "import7" {
            initialize_schema6(&path).unwrap();
            add_lists(&path);
        }
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        close(db);
    }
    let format = mode.chars().last().unwrap().to_digit(10).unwrap();
    let data = root.join("data");
    let state = DatabaseState::default();
    let active = state.initialize_schema7_startup(&data).unwrap();
    let old = crate::backup::restore_context::activation::journal_proof(&active).unwrap();
    let expected = if mode == "restore8" {
        let c =
            crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
                .unwrap();
        let p = crate::storage_content::verified_snapshot_v8(
            c.source().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap();
        drop(c.try_close().map_err(|_| ()).unwrap());
        crate::open_import::ContentProof {
            version: 8,
            sha256: p.sha256,
            entry_count: p.entry_count,
        }
    } else {
        let c = crate::open_import::build_for_schema7(
            &mut File::open(&archive).unwrap(),
            &root,
            &AtomicBool::new(false),
            format,
        )
        .unwrap();
        c.proof().clone()
    };
    let m = state.maintenance().unwrap();
    if mode == "restore8" {
        drop(
            crate::backup::restore_context::activation::activate(
                &m,
                &archive,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
    } else {
        drop(
            crate::backup::restore_context::activation::importing::activate_schema7(
                &m,
                &mut File::open(&archive).unwrap(),
                &old,
                &expected,
                Arc::new(AtomicBool::new(false)),
                format,
            )
            .unwrap(),
        );
    }
    drop(m);
    drop(state);
    let state = DatabaseState::default();
    assert!(state.initialize_schema7_startup(&data).is_err());
    crate::backup::restore_context::activation::startup::run(
        &state,
        crate::backup::restore_context::activation::startup::Action::FinishNew,
    )
    .unwrap();
    assert_eq!(
        crate::backup::restore_context::activation::journal_proof(&active).unwrap(),
        expected
    );
}
