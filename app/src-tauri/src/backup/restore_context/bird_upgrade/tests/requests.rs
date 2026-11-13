use super::*;
use crate::{bird_observations as birds, database_state::DatabaseState};
fn current(root: &Path) -> (DatabaseState, PathBuf, String) {
    let (old, path, id) = super::comments::current(root);
    let bytes = fs::read(&path).unwrap();
    drop(old);
    let state = DatabaseState::default();
    state.initialize_startup(root).unwrap();
    assert!(
        fs::read_dir(root.join("backups/bird-schema-upgrade"))
            .unwrap()
            .any(|p| fs::read(p.unwrap().path().join("schema8.sqlite"))
                .ok()
                .as_ref()
                == Some(&bytes))
    );
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let old = birds::reopen(&state, g, &id).unwrap();
    let search = crate::journal_search::SearchState::default();
    let retired = "99999999-9999-4999-8999-999999999999";
    birds::reserve_id(&state, &search, g, retired).unwrap();
    birds::create_with_identity(
        &state,
        &search,
        g,
        &old.visit_date,
        &old.place_id,
        &old.rows,
        Some("entry"),
        None,
        None,
        Some(retired),
    )
    .unwrap();
    let saved = birds::reopen(&state, g, retired).unwrap();
    birds::delete(
        &state,
        &search,
        g,
        retired,
        &birds::edit_token(&saved),
        true,
    )
    .unwrap();
    (state, path, id)
}
fn legacy(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    super::comments::legacy(root)
}
#[test]
fn durable_creation_receipts_survive_deletion_restart_conflict_and_transaction_rollback() {
    let t = TempDir::new().unwrap();
    let root = t.path().join("journal");
    let (state, path, oldid) = current(&root);
    let g = state.operation(None).unwrap().snapshot(()).generation;
    let old = birds::reopen(&state, g, &oldid).unwrap();
    let search = crate::journal_search::SearchState::default();
    let id = uuid::Uuid::new_v4().to_string();
    let create = |state: &DatabaseState, id: &str, date: &str, rows: &[birds::SpeciesCount]| {
        birds::create_with_identity(
            state,
            &search,
            g,
            date,
            &old.place_id,
            rows,
            Some("entry"),
            None,
            None,
            Some(id),
        )
    };
    birds::reserve_id(&state, &search, g, &id).unwrap();
    create(&state, &id, &old.visit_date, &old.rows).unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(create(&state, &id, &old.visit_date, &old.rows).unwrap(), id);
    assert_eq!(fs::read(&path).unwrap(), before);
    let mut changed = old.rows.clone();
    changed[0].count += 1;
    assert_eq!(
        create(&state, &id, &old.visit_date, &changed).unwrap_err(),
        "bird_request_conflict"
    );
    let saved = birds::reopen(&state, g, &id).unwrap();
    birds::update(
        &state,
        &search,
        g,
        &id,
        "entry",
        &birds::edit_token(&saved),
        &old.visit_date,
        &old.place_id,
        None,
        None,
        &changed,
    )
    .unwrap();
    assert_eq!(create(&state, &id, &old.visit_date, &old.rows).unwrap(), id); // original receipt does not follow edits
    let saved = birds::reopen(&state, g, &id).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(
        birds::delete_with_hook(
            &state,
            &search,
            g,
            &id,
            &birds::edit_token(&saved),
            true,
            || Err("rollback".into())
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    birds::delete(&state, &search, g, &id, &birds::edit_token(&saved), true).unwrap();
    assert_eq!(
        create(&state, &id, &old.visit_date, &old.rows).unwrap_err(),
        "bird_request_retired"
    );
    assert!(!birds::exists(&state, g, &id).unwrap());
    assert_eq!(
        create(&state, &id, &old.visit_date, &changed).unwrap_err(),
        "bird_request_conflict"
    );
    let new = uuid::Uuid::new_v4().to_string();
    birds::reserve_id(&state, &search, g, &new).unwrap();
    create(&state, &new, &old.visit_date, &old.rows).unwrap();
    let failed = uuid::Uuid::new_v4().to_string();
    birds::reserve_id(&state, &search, g, &failed).unwrap();
    let before = fs::read(&path).unwrap();
    assert!(create(&state, &failed, "2026-02-30", &old.rows).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    let db = crate::database::open_existing(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT completed FROM bird_creation_requests WHERE request_id=?1",
            [failed],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    close(db);
    // Existing schema-8 IDs are reserved even though their original digest is unknown.
    birds::delete(&state, &search, g, &oldid, &birds::edit_token(&old), true).unwrap();
    assert_eq!(
        create(&state, &oldid, &old.visit_date, &old.rows).unwrap_err(),
        "bird_request_retired"
    );
    drop(state);
    let reopened = DatabaseState::default();
    reopened.initialize_startup(&root).unwrap();
    assert_eq!(
        create(&reopened, &id, &old.visit_date, &old.rows).unwrap_err(),
        "bird_request_retired"
    );
    assert!(!birds::exists(&reopened, g, &id).unwrap());
    assert_eq!(
        create(&reopened, &new, &old.visit_date, &old.rows).unwrap(),
        new
    );
}
#[test]
fn receipt_migration_interruption_retry_and_invalid_candidate_preserve_schema8_source() {
    for boundary in ["upgraded", "before-publish", "after-publish"] {
        let t = TempDir::new().unwrap();
        let root = t.path().join("journal");
        let (state, path, _) = super::comments::current(&root);
        drop(state);
        let bytes = fs::read(&path).unwrap();
        assert!(
            initialize_to(&path, 9, |point, _| if point == boundary {
                Err("interrupted".into())
            } else {
                Ok(())
            })
            .is_err()
        );
        if boundary != "after-publish" {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let state = DatabaseState::default();
        state.initialize_startup(&root).unwrap();
        let mut db = crate::database::open_existing(&path).unwrap();
        assert!(crate::storage_content::verify_v10(&mut db, &AtomicBool::new(false)).is_ok());
        close(db);
    }
    let t = TempDir::new().unwrap();
    let root = t.path().join("journal");
    let (state, path, _) = super::comments::current(&root);
    drop(state);
    let bytes = fs::read(&path).unwrap();
    assert!(
        initialize_to(&path, 9, |point, next| {
            if point == "upgraded" {
                let db = crate::database::open_existing(next).unwrap();
                db.execute("UPDATE bird_creation_requests SET payload_sha256='bad'", [])
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
fn receipts_restore_import_activation_and_older_contracts() {
    for mode in [
        "restore10",
        "import10",
        "restore9",
        "restore5",
        "import9",
        "import8",
        "import7",
        "import6",
        "import5",
    ] {
        let root = TempDir::new().unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::bird_upgrade::tests::requests::receipts_activation_child",
                "--nocapture",
            ])
            .env("MNO_REQUESTS", mode)
            .env("MNO_ACTIVATION_ROOT", root.path())
            .env("MNO_ACTIVATION_MODE", "success")
            .output()
            .unwrap();
        assert!(out.status.success(), "{mode}: {out:?}");
    }
}
#[test]
fn receipts_activation_child() {
    let Ok(mode) = std::env::var("MNO_REQUESTS") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let archive = root.join(if mode.starts_with("restore") {
        "incoming.mno-backup"
    } else {
        "incoming.mno-export"
    });
    if mode.ends_with("10") {
        let (state, path, _) = current(&root.join("source"));
        let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        if mode.starts_with("restore") {
            crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        } else {
            crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        }
        close(db);
        drop(state);
    } else if mode.ends_with('9') {
        let (state, path, _) = super::comments::current(&root.join("source"));
        let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        if mode.starts_with("restore") {
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
        if mode.starts_with("restore") {
            crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        } else {
            crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
        }
        close(db);
    }
    let format = mode
        .trim_start_matches("restore")
        .trim_start_matches("import")
        .parse::<u32>()
        .unwrap();
    let (data_state, active, _) = current(&root.join("data"));
    let old = crate::backup::restore_context::activation::journal_proof(&active).unwrap();
    let expected = if mode.starts_with("restore") {
        let c =
            crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
                .unwrap();
        let p = if format == 10 {
            crate::storage_content::verified_snapshot_v10(
                c.source().unwrap(),
                &AtomicBool::new(false),
            )
        } else if format == 5 {
            crate::storage_content::verified_snapshot_v5(
                c.source().unwrap(),
                &AtomicBool::new(false),
            )
        } else {
            crate::storage_content::verified_snapshot_v9(
                c.source().unwrap(),
                &AtomicBool::new(false),
            )
        }
        .unwrap();
        drop(c.try_close().map_err(|_| ()).unwrap());
        crate::open_import::ContentProof {
            version: format,
            sha256: p.sha256,
            entry_count: p.entry_count,
        }
    } else {
        let c = crate::open_import::build_for_schema9(
            &mut File::open(&archive).unwrap(),
            &root,
            &AtomicBool::new(false),
            format,
        )
        .unwrap();
        c.proof().clone()
    };
    let m = data_state.maintenance().unwrap();
    if mode.starts_with("restore") {
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
            crate::backup::restore_context::activation::importing::activate_schema9(
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
        crate::storage_content::verified_snapshot_v10(c.source().unwrap(), &AtomicBool::new(false))
            .unwrap()
            .sha256,
        old.sha256
    );
    drop(c.try_close().map_err(|_| ()).unwrap());
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&root.join("data")).is_err());
    crate::backup::restore_context::activation::startup::run(
        &state,
        crate::backup::restore_context::activation::startup::Action::FinishNew,
    )
    .unwrap();
    assert_eq!(
        crate::backup::restore_context::activation::journal_proof(&active).unwrap(),
        expected
    );
    drop(state);
    // Ordinary restart upgrades older restored databases; no receipt/ID is
    // invented for a missing old request when the archive predates receipts.
    let ready = DatabaseState::default();
    ready.initialize_startup(&root.join("data")).unwrap();
    let g = ready.operation(None).unwrap().snapshot(()).generation;
    let db = crate::database::open_existing(&active).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places LIMIT 1", [], |r| r.get(0))
        .unwrap();
    close(db);
    let rows = vec![birds::SpeciesCount {
        avibase_id: "avibase-E15BC0CB".into(),
        taxonomy_version: crate::species_registry::VERSION.into(),
        count: 6,
        comment: None,
    }];
    let error = birds::create_with_identity(
        &ready,
        &crate::journal_search::SearchState::default(),
        g,
        "2026-11-12",
        &place,
        &rows,
        Some("entry"),
        None,
        None,
        Some("99999999-9999-4999-8999-999999999999"),
    )
    .unwrap_err();
    if format < 10 {
        assert_eq!(error, "bird_request_unreserved");
    } else {
        assert!(error == "bird_request_conflict" || error == "bird_request_retired");
    }
    assert!(!birds::exists(&ready, g, "99999999-9999-4999-8999-999999999999").unwrap());
    if format == 10 {
        let db = crate::database::open_existing(&active).unwrap();
        let id: String = db
            .query_row(
                "SELECT list_id FROM bird_rows WHERE comment IS NOT NULL LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        close(db);
        let original = birds::reopen(&ready, g, &id).unwrap();
        assert_eq!(
            birds::create_with_identity(
                &ready,
                &crate::journal_search::SearchState::default(),
                g,
                &original.visit_date,
                &original.place_id,
                &original.rows,
                Some("entry"),
                None,
                None,
                Some("99999999-9999-4999-8999-999999999999")
            )
            .unwrap_err(),
            "bird_request_retired"
        );
    }
    let search = crate::journal_search::SearchState::default();
    let fresh = birds::reserve(&ready, &search, g).unwrap();
    birds::create_with_identity(
        &ready,
        &search,
        g,
        "2026-11-12",
        &place,
        &rows,
        Some("entry"),
        None,
        None,
        Some(&fresh),
    )
    .unwrap();
}

#[test]
fn malformed_receipt_export_is_rejected_without_active_changes() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let t = TempDir::new().unwrap();
    let root = t.path().join("journal");
    let (_state, path, _) = current(&root);
    let good = t.path().join("good.mno-export");
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut db, &good, &AtomicBool::new(false)).unwrap();
    close(db);
    let mut zip = zip::ZipArchive::new(File::open(&good).unwrap()).unwrap();
    let mut files = std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(entry.name().to_owned(), bytes);
    }
    let before = fs::read(&path).unwrap();
    for invalid in ["digest", "completed", "missing"] {
        let mut files = files.clone();
        let mut rows: Vec<serde_json::Value> =
            String::from_utf8(files["bird_creation_requests.jsonl"].clone())
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
        if invalid == "digest" {
            rows[0]["payload_sha256"] = "bad".into();
        } else if invalid == "completed" {
            rows[0]["completed"] = 2.into();
        } else {
            rows.clear();
        }
        let bytes = rows
            .iter()
            .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
            .collect::<String>()
            .into_bytes();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest["files"]["bird_creation_requests.jsonl"]["size"] = (bytes.len() as u64).into();
        manifest["files"]["bird_creation_requests.jsonl"]["sha256"] =
            format!("{:x}", Sha256::digest(&bytes)).into();
        manifest["files"]["bird_creation_requests.jsonl"]["row_count"] = (rows.len() as u64).into();
        files.insert("bird_creation_requests.jsonl".into(), bytes);
        files.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let bad = t.path().join(format!("{invalid}.mno-export"));
        let mut writer = zip::ZipWriter::new(File::create(&bad).unwrap());
        for (name, bytes) in files {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        assert!(
            crate::open_import::build_v10(
                &mut File::open(bad).unwrap(),
                t.path(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "{invalid}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
