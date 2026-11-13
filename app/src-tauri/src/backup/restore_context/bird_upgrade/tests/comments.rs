use super::*;
use crate::{bird_observations as birds, database_state::DatabaseState};
pub(super) fn legacy(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    fs::create_dir_all(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize_schema7(&path).unwrap();
    add_lists(&path);
    let db = crate::database::open_existing(&path).unwrap();
    // Deliberately retained old duplicate: migration and archives must not merge it.
    db.execute("INSERT INTO bird_rows SELECT ?1,list_id,2,avibase_id,taxonomy_version,9 FROM bird_rows WHERE position=0 ORDER BY list_id LIMIT 1",[uuid::Uuid::new_v4().to_string()]).unwrap();
    close(db);
    let mut db = crate::database::open_existing(&path).unwrap();
    let proof = full(&mut db, 7).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}
pub(super) fn current(root: &Path) -> (DatabaseState, PathBuf, String) {
    let (_, bytes, proof) = legacy(root);
    let state = DatabaseState::default();
    let path = state.initialize_schema8_startup(root).unwrap();
    assert_eq!(verify_for(&path, 8, 7).unwrap(), proof);
    assert!(
        fs::read_dir(root.join("backups/bird-schema-upgrade"))
            .unwrap()
            .any(|e| fs::read(e.unwrap().path().join("schema7.sqlite"))
                .ok()
                .as_ref()
                == Some(&bytes))
    );
    let db = crate::database::open_existing(&path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM bird_rows", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    close(db);
    let generation = state.operation(None).unwrap().snapshot(()).generation;
    let id = birds::create_with_identity(
        &state,
        &crate::journal_search::SearchState::default(),
        generation,
        "2026-11-12",
        &place,
        &[
            birds::SpeciesCount {
                avibase_id: "avibase-E15BC0CB".into(),
                taxonomy_version: crate::species_registry::VERSION.into(),
                count: 6,
                comment: Some("I björken.\n<script>plain text</script>".into()),
            },
            birds::SpeciesCount {
                avibase_id: "avibase-9BE53D34".into(),
                taxonomy_version: crate::species_registry::VERSION.into(),
                count: 4,
                comment: None,
            },
        ],
        Some("entry"),
        None,
        None,
        Some(&uuid::Uuid::new_v4().to_string()),
    )
    .unwrap();
    (state, path, id)
}
#[test]
fn comments_migrate_preserve_legacy_duplicates_and_roundtrip_archives() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("source");
    let (state, path, id) = current(&root);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let saved = birds::reopen(&state, g, &id).unwrap();
    assert_eq!(
        saved.rows[0].comment.as_deref(),
        Some("I björken.\n<script>plain text</script>")
    );
    let mut edited = saved.rows.clone();
    edited[0].count = 12;
    edited[0].comment = Some("Edited comment survives archives".into());
    birds::update(
        &state,
        &crate::journal_search::SearchState::default(),
        g,
        &id,
        "entry",
        &birds::edit_token(&saved),
        "2026-11-13",
        &saved.place_id,
        Some("09:00"),
        Some("10:00"),
        &edited,
    )
    .unwrap();
    let saved = birds::reopen(&state, g, &id).unwrap();
    assert_eq!(saved.rows, edited);
    assert_eq!(saved.visit_date, "2026-11-13");
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = full(&mut db, 8).unwrap();
    let backup = temp.path().join("comments.mno-backup");
    let manifest =
        crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (9, 8)
    );
    let candidate =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v9(
            candidate.source().unwrap(),
            &AtomicBool::new(false)
        )
        .unwrap(),
        proof
    );
    drop(candidate.try_close().map_err(|_| ()).unwrap());
    let archive = temp.path().join("comments.mno-export");
    crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
    close(db);
    let c = crate::open_import::build_v9(
        &mut File::open(&archive).unwrap(),
        temp.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(c.proof().sha256, proof.sha256);
    drop(c);
    let before = fs::read(&path).unwrap();
    let mut changed = saved.rows.clone();
    changed[0].comment = Some("changed".into());
    assert_eq!(
        birds::create_with_identity(
            &state,
            &crate::journal_search::SearchState::default(),
            g,
            &saved.visit_date,
            &saved.place_id,
            &changed,
            Some("entry"),
            None,
            None,
            Some(&id)
        )
        .unwrap_err(),
        "bird_request_conflict"
    );
    changed[0].comment = Some("bad\0comment".into());
    assert_eq!(
        birds::create_with_identity(
            &state,
            &crate::journal_search::SearchState::default(),
            g,
            &saved.visit_date,
            &saved.place_id,
            &changed,
            Some("entry"),
            None,
            None,
            Some(&uuid::Uuid::new_v4().to_string())
        )
        .unwrap_err(),
        "bird_comment_invalid"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(state);
    let state = DatabaseState::default();
    state.initialize_schema8_startup(&root).unwrap();
    let g = state.operation(None).unwrap().snapshot(()).generation;
    assert_eq!(birds::reopen(&state, g, &id).unwrap(), saved);
}
#[test]
fn comments_upgrade_interruption_retry_preserves_source_and_invalid_candidate_blocks() {
    for boundary in ["upgraded", "before-publish", "after-publish"] {
        let root = TempDir::new().unwrap();
        let (path, bytes, proof) = legacy(root.path());
        assert!(
            initialize_to(&path, 8, |name, _| if name == boundary {
                Err("injected interruption".into())
            } else {
                Ok(())
            })
            .is_err()
        );
        if boundary != "after-publish" {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        initialize_schema8(&path).unwrap();
        assert_eq!(verify_for(&path, 8, 7).unwrap(), proof);
    }
    let root = TempDir::new().unwrap();
    let (path, bytes, _) = legacy(root.path());
    assert!(
        initialize_to(&path, 8, |name, next| {
            if name == "upgraded" {
                let db = crate::database::open_existing(next).unwrap();
                db.execute(
                    "UPDATE bird_rows SET comment=?1",
                    ["x".repeat(crate::database::bird_comments::LIMIT + 1)],
                )
                .unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[test]
fn comments_restore_import_activation_and_older_contracts() {
    for mode in [
        "restore9", "import9", "import8", "import7", "import6", "import5",
    ] {
        let root = TempDir::new().unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::bird_upgrade::tests::comments::comments_activation_child",
                "--nocapture",
            ])
            .env("MNO_COMMENTS", mode)
            .env("MNO_ACTIVATION_ROOT", root.path())
            .env("MNO_ACTIVATION_MODE", "success")
            .output()
            .unwrap();
        assert!(out.status.success(), "{mode}: {out:?}");
    }
}
#[test]
fn comments_activation_child() {
    let Ok(mode) = std::env::var("MNO_COMMENTS") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let archive = root.join(if mode == "restore9" {
        "incoming.mno-backup"
    } else {
        "incoming.mno-export"
    });
    if mode.ends_with('9') {
        let (state, path, _) = current(&root.join("source"));
        let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        if mode == "restore9" {
            crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        } else {
            crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        }
        close(db);
        drop(state);
    } else {
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        let (path, _, _) = if mode == "import8" {
            legacy(&source)
        } else {
            fixture(&source)
        };
        if mode == "import6" {
            initialize(&path).unwrap();
            add_lists(&path);
        } else if mode == "import7" {
            initialize_schema6(&path).unwrap();
            add_lists(&path);
        }
        let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        close(db);
    }
    let format = mode.chars().last().unwrap().to_digit(10).unwrap();
    let (data_state, active, _) = current(&root.join("data"));
    let old = crate::backup::restore_context::activation::journal_proof(&active).unwrap();
    let expected = if mode == "restore9" {
        let c =
            crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
                .unwrap();
        let p = crate::storage_content::verified_snapshot_v9(
            c.source().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap();
        drop(c.try_close().map_err(|_| ()).unwrap());
        crate::open_import::ContentProof {
            version: 9,
            sha256: p.sha256,
            entry_count: p.entry_count,
        }
    } else {
        let c = crate::open_import::build_for_schema8(
            &mut File::open(&archive).unwrap(),
            &root,
            &AtomicBool::new(false),
            format,
        )
        .unwrap();
        c.proof().clone()
    };
    let m = data_state.maintenance().unwrap();
    if mode == "restore9" {
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
            crate::backup::restore_context::activation::importing::activate_schema8(
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
    drop(data_state);
    let protection = fs::read_dir(root.join("data/backups/pre-restore"))
        .unwrap()
        .find_map(|e| {
            let p = e.unwrap().path();
            (p.extension().is_some_and(|x| x == "mno-backup")).then_some(p)
        })
        .unwrap();
    let c =
        crate::backup::restore::prepare_candidate(&protection, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v9(c.source().unwrap(), &AtomicBool::new(false))
            .unwrap()
            .sha256,
        old.sha256
    );
    drop(c.try_close().map_err(|_| ()).unwrap());
    let state = DatabaseState::default();
    assert!(
        state
            .initialize_schema8_startup(&root.join("data"))
            .is_err()
    );
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

#[test]
fn edits_preserve_links_duplicates_and_rollback_partial_failure_reject_stale_or_missing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("journal");
    let (state, path, id) = current(&root);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries SELECT 'second',entry_date,title,?1,document_version,created_at,updated_at FROM journal_entries WHERE id='entry'",[DOC]).unwrap();
    db.execute(
        "INSERT INTO journal_bird_lists(entry_id,list_id) VALUES ('second',?1)",
        [&id],
    )
    .unwrap();
    close(db);
    let old = birds::reopen(&state, g, &id).unwrap();
    let search = crate::journal_search::SearchState::default();
    let token = birds::edit_token(&old);
    let mut rows = old.rows.clone();
    rows.push(rows[0].clone()); // retained duplicate remains independently editable.
    let update = |expected: &str, rows: &[birds::SpeciesCount]| {
        birds::update(
            &state,
            &search,
            g,
            &id,
            "entry",
            expected,
            "2026-11-14",
            &old.place_id,
            None,
            None,
            rows,
        )
    };
    update(&token, &rows).unwrap();
    let saved = birds::reopen(&state, g, &id).unwrap();
    assert_eq!(saved.journal_entries, old.journal_entries);
    assert_eq!(saved.rows, rows);
    assert_eq!(saved.id, old.id);
    let token = birds::edit_token(&saved);
    let before = fs::read(&path).unwrap();
    assert!(update(&token, &[]).is_err());
    assert!(update("stale", &rows).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    let mut changed = rows.clone();
    changed[0].count = 99;
    // Invalid date is rejected by full content verification after SQL changes,
    // proving rollback of the changed metadata and replacement species rows.
    let error = birds::update(
        &state,
        &search,
        g,
        &id,
        "entry",
        &token,
        "2026-02-30",
        &old.place_id,
        None,
        None,
        &changed,
    )
    .unwrap_err();
    assert!(error.contains("bird_list_invalid"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(birds::reopen(&state, g, &id).unwrap(), saved);
    state.set_generation_for_test(g + 1);
    assert!(update(&token, &rows).is_err());
    state.set_generation_for_test(g);
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("DELETE FROM bird_lists WHERE id=?1", [&id])
        .unwrap();
    close(db);
    assert!(update(&token, &rows).is_err());
    let db = crate::database::open_existing(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM bird_lists WHERE id=?1", [&id], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
    close(db);
}

#[test]
fn list_deletion_is_atomic_preserves_shared_content_and_rejects_late_updates_after_restart() {
    fn inventory(db: &Connection) -> crate::storage_content::Proof {
        let tx = db.unchecked_transaction().unwrap();
        let proof =
            crate::storage_content::verified_snapshot_v5(&tx, &AtomicBool::new(false)).unwrap();
        tx.rollback().unwrap();
        proof
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("journal");
    let (state, path, id) = current(&root);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries SELECT 'second',entry_date,title,?1,document_version,created_at,updated_at FROM journal_entries WHERE id='entry'",[DOC]).unwrap();
    db.execute("INSERT INTO journal_bird_lists VALUES ('second',?1)", [&id])
        .unwrap();
    let others: Vec<String> = db
        .prepare("SELECT id FROM bird_lists WHERE id<>?1 ORDER BY id")
        .unwrap()
        .query_map([&id], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let journal_proof = inventory(&db);
    close(db);
    let unrelated: Vec<_> = others
        .iter()
        .map(|id| birds::reopen(&state, g, id).unwrap())
        .collect();
    let original_photo = fs::read(root.join("original.png")).unwrap();
    let old = birds::reopen(&state, g, &id).unwrap();
    assert_eq!(old.journal_entries, vec!["entry", "second"]);
    let token = birds::edit_token(&old);
    let search = crate::journal_search::SearchState::default();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        birds::delete(&state, &search, g, &id, &token, false).unwrap_err(),
        "bird_delete_confirmation_required"
    );
    assert_eq!(
        birds::delete(&state, &search, g, &id, "stale", true).unwrap_err(),
        "bird_edit_stale"
    );
    state.set_generation_for_test(g + 1);
    assert_eq!(
        birds::delete(&state, &search, g, &id, &token, true).unwrap_err(),
        crate::database_state::GENERATION_CONFLICT
    );
    state.set_generation_for_test(g);
    let mut reached = false;
    assert_eq!(
        birds::delete_with_hook(&state, &search, g, &id, &token, true, || {
            reached = true;
            Err("injected_after_delete".into())
        })
        .unwrap_err(),
        "injected_after_delete"
    );
    assert!(reached);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(birds::reopen(&state, g, &id).unwrap(), old);
    birds::delete(&state, &search, g, &id, &token, true).unwrap();
    assert!(!birds::exists(&state, g, &id).unwrap());
    assert!(birds::delete(&state, &search, g, &id, &token, true).is_err());
    assert!(
        birds::update(
            &state,
            &search,
            g,
            &id,
            "entry",
            &token,
            &old.visit_date,
            &old.place_id,
            old.visit_start.as_deref(),
            old.visit_end.as_deref(),
            &old.rows
        )
        .is_err()
    );
    let db = crate::database::open_existing(&path).unwrap();
    for table in ["bird_rows", "journal_bird_lists"] {
        assert_eq!(
            db.query_row(
                &format!("SELECT count(*) FROM {table} WHERE list_id=?1"),
                [&id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    assert_eq!(inventory(&db), journal_proof);
    close(db);
    for old in &unrelated {
        assert_eq!(birds::reopen(&state, g, &old.id).unwrap(), *old);
    }
    assert_eq!(fs::read(root.join("original.png")).unwrap(), original_photo);
    drop(state);
    let reopened = DatabaseState::default();
    reopened.initialize_schema8_startup(&root).unwrap();
    let g = reopened.operation(None).unwrap().snapshot(()).generation;
    assert!(!birds::exists(&reopened, g, &id).unwrap());
    for old in unrelated {
        assert_eq!(birds::reopen(&reopened, g, &old.id).unwrap(), old);
    }
}
