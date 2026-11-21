use super::*;
#[test]
fn synthetic_validation_keeps_x_and_rejects_invalid_source() {
    let f: Fields = [
        ("Submission ID", "S100"),
        ("Common Name", "Synthetic bird"),
        ("Scientific Name", "Aves sp."),
        ("Count", "X"),
        ("Location ID", "L100"),
        ("Location", "Synthetic lake"),
        ("Latitude", "59"),
        ("Longitude", "18"),
        ("Date", "2026-11-21"),
        ("Time", "8:15 AM"),
        ("ML Catalog Numbers", "10001 10002"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    validate_visit(&InputVisit {
        submission_id: "S100".into(),
        rows: vec![f.clone()],
    })
    .unwrap();
    for (k, v) in [
        ("Count", "0"),
        ("Count", "1.5"),
        ("Latitude", "NaN"),
        ("Date", "2026-02-30"),
        ("ML Catalog Numbers", "../bad"),
    ] {
        let mut bad = f.clone();
        bad.insert(k.into(), v.into());
        assert!(validate_fields(&bad).is_err());
    }
}

fn input(id: &str) -> InputVisit {
    InputVisit {
        submission_id: id.into(),
        rows: vec![
            [
                ("Submission ID", id),
                ("Common Name", "Synthetic bird"),
                ("Scientific Name", "Aves sp."),
                ("Count", "X"),
                ("Location ID", "L100"),
                ("Location", "Synthetic lake"),
                ("Latitude", "59"),
                ("Longitude", "18"),
                ("Date", "2026-11-21"),
                ("Time", "8:15 AM"),
                ("Observation Details", "Source comment"),
                ("ML Catalog Numbers", "10001 10002"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        ],
    }
}
fn fixture() -> (tempfile::TempDir, DatabaseState, std::path::PathBuf) {
    let temp = tempfile::TempDir::new().unwrap();
    let (state, path) = fixture_at(&temp.path().join("journal"));
    (temp, state, path)
}
fn fixture_at(root: &std::path::Path) -> (DatabaseState, std::path::PathBuf) {
    fixture_at_schema(root, 13)
}
pub(crate) fn fixture_at_schema(
    root: &std::path::Path,
    schema: u32,
) -> (DatabaseState, std::path::PathBuf) {
    let state = DatabaseState::default();
    let path = if schema == 12 {
        state.initialize_schema12_startup(root)
    } else {
        state.initialize_schema13_startup(root)
    }
    .unwrap();
    let db = database::open_existing(&path).unwrap();
    for id in ["entry", "other"] {
        db.execute("INSERT INTO journal_entries VALUES(?1,'2026-11-21','Synthetic journal','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[id]).unwrap();
    }
    let image = root.parent().unwrap().join("disposable-original.png");
    image::RgbImage::from_pixel(3, 2, image::Rgb([20, 80, 100]))
        .save(&image)
        .unwrap();
    let photo = crate::photographs::prepare(&image, "entry".into()).unwrap();
    photo.insert(&db).unwrap();
    let doc=serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Synthetic journal text"}]},{"type":"journalImage","attrs":{"nodeId":uuid::Uuid::new_v4().to_string(),"assetId":photo.asset_id,"placement":"left","size":"small","caption":"Journal caption","alt":"Synthetic"}}]}).to_string();
    db.execute(
        "UPDATE journal_entries SET content_json=?1 WHERE id='entry'",
        [doc],
    )
    .unwrap();
    db.execute("INSERT INTO journal_weather VALUES('entry',1,'{\"period\":{\"kind\":\"day\",\"date\":\"2026-11-21\"},\"temperatureC\":7}',NULL,1)",[]).unwrap();
    let place = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO places VALUES(?1,'Synthetic lake',59,18,1)",
        [&place],
    )
    .unwrap();
    db.close().unwrap();
    let row = uuid::Uuid::new_v4().to_string();
    let mut photo = crate::bird_photos::Photo::from_asset(
        crate::photographs::prepare(&image, row.clone()).unwrap(),
    );
    photo.caption = Some("Bird caption".into());
    let rows = [crate::bird_observations::SpeciesCount {
        row_id: Some(row),
        photo: Some(photo),
        avibase_id: "avibase-synthetic".into(),
        taxonomy_version: "Synthetic catalog".into(),
        count: 2,
        comment: Some("Manual row comment".into()),
    }];
    let list = crate::bird_observations::create(
        &state,
        &SearchState::default(),
        1,
        "2026-11-21",
        &place,
        &rows,
        Some("entry"),
    )
    .unwrap();
    let db = database::open_existing(&path).unwrap();
    let context = crate::bird_visit_weather::context(&db, &list).unwrap();
    let weather = crate::bird_visit_weather::Record::new(
        &list,
        "manual",
        &context,
        "{\"period\":{\"kind\":\"day\",\"date\":\"2026-11-21\"},\"temperatureC\":5}",
    )
    .unwrap();
    let tx = db.unchecked_transaction().unwrap();
    crate::bird_visit_weather::apply(
        &tx,
        &list,
        &[crate::bird_visit_weather::Change::Set {
            record: weather,
            confirmed: false,
        }],
    )
    .unwrap();
    tx.commit().unwrap();
    db.close().unwrap();
    (state, path)
}
fn saved(path: &std::path::Path) -> Vec<Record> {
    let db = database::open_existing(path).unwrap();
    let r = records(&db).unwrap();
    db.close().unwrap();
    r
}
fn apply(state: &DatabaseState, visits: &[InputVisit]) -> Result<()> {
    import(
        state,
        &SearchState::default(),
        &crate::bird_module::test_module(true),
        1,
        visits,
    )
}
fn additions(path: &std::path::Path) {
    let db = database::open_existing(path).unwrap();
    let rec = records(&db).unwrap();
    let (visit, row) = rec
        .iter()
        .find_map(|r| {
            if let Record::Row { id, visit_id, .. } = r {
                Some((visit_id.clone(), id.clone()))
            } else {
                None
            }
        })
        .unwrap();
    insert(
        &db,
        &Record::Addition {
            id: uuid::Uuid::new_v4().to_string(),
            visit_id: visit.clone(),
            row_id: Some(row),
            content: "MNO note retained for review".into(),
        },
    )
    .unwrap();
    for entry in ["entry", "other"] {
        insert(
            &db,
            &Record::Link {
                entry_id: entry.into(),
                visit_id: visit.clone(),
            },
        )
        .unwrap();
    }
    db.close().unwrap();
}
#[test]
fn ebird_atomic_repeat_changes_missing_visits_and_additions() {
    let (_temp, state, path) = fixture();
    let first = input("S100");
    let other = input("S101");
    apply(&state, &[first.clone(), other]).unwrap();
    additions(&path);
    let before = saved(&path);
    let bytes = std::fs::read(&path).unwrap();
    apply(&state, &[first.clone()]).unwrap();
    assert_eq!(saved(&path), before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut changed = first.clone();
    changed.rows[0].insert("Count".into(), "4".into());
    changed.rows[0].insert(
        "Observation Details".into(),
        "Changed source comment".into(),
    );
    apply(&state, &[changed.clone()]).unwrap();
    let after = saved(&path);
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::Visit { .. }))
            .count(),
        2
    );
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::Row { active: false, .. }))
            .count(),
        0
    );
    for r in before
        .iter()
        .filter(|r| matches!(r, Record::Addition { .. } | Record::Link { .. }))
    {
        assert!(after.contains(r));
    }
    let bytes = std::fs::read(&path).unwrap();
    apply(&state, &[changed]).unwrap();
    assert_eq!(saved(&path), after);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Same date/place with a new eBird identity is a separate visit.
    apply(&state, &[input("S102")]).unwrap();
    assert_eq!(
        saved(&path)
            .iter()
            .filter(|r| matches!(r, Record::Visit { .. }))
            .count(),
        3
    );
}
#[test]
fn ebird_conflicts_failure_module_and_generation_are_atomic() {
    let (_temp, state, path) = fixture();
    let first = input("S100");
    let before = saved(&path);
    let mut bad = input("S101");
    bad.rows[0].insert("Count".into(), "0".into());
    assert!(apply(&state, &[first.clone(), bad]).is_err());
    assert_eq!(saved(&path), before);
    let mut conflict = first.clone();
    let mut row = conflict.rows[0].clone();
    row.insert("Scientific Name".into(), "Other taxon".into());
    row.insert("Latitude".into(), "60".into());
    conflict.rows.push(row);
    assert!(apply(&state, &[conflict]).is_err());
    assert!(apply(&state, &[first.clone(), first.clone()]).is_err());
    let db = database::open_existing(&path).unwrap();
    db.execute_batch("CREATE TEMP TABLE unused(x);").unwrap();
    // Persistent trigger is deliberately invalid as a schema: import rejects before mutation.
    db.execute_batch("CREATE TRIGGER fail_import BEFORE INSERT ON ebird_rows BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(apply(&state, &[first.clone()]).is_err());
    assert_eq!(saved(&path), before);
    db.execute_batch("DROP TRIGGER fail_import").unwrap();
    db.close().unwrap();
    assert!(
        import(
            &state,
            &SearchState::default(),
            &crate::bird_module::test_module(false),
            1,
            &[first.clone()]
        )
        .is_err()
    );
    state.set_generation_for_test(2);
    assert!(apply(&state, &[first]).is_err());
    assert_eq!(saved(&path), before);
}
#[test]
fn ebird_manual_paths_cannot_edit_source_and_deleting_journal_keeps_visits() {
    let (temp, state, path) = fixture();
    apply(&state, &[input("S100")]).unwrap();
    additions(&path);
    let before = saved(&path);
    let id = before
        .iter()
        .find_map(|r| {
            if let Record::Visit { id, .. } = r {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        crate::bird_observations::delete(&state, &SearchState::default(), 1, &id, "", true)
            .is_err()
    );
    assert_eq!(saved(&path), before);
    let db = database::open_existing(&path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    db.close().unwrap();
    assert!(
        crate::bird_observations::update_in_context(
            &state,
            &SearchState::default(),
            1,
            &id,
            None,
            "irrelevant-token",
            "2026-11-21",
            &place,
            None,
            None,
            &[crate::bird_observations::SpeciesCount {
                row_id: None,
                photo: None,
                avibase_id: "avibase-synthetic".into(),
                taxonomy_version: "Synthetic catalog".into(),
                count: 99,
                comment: None
            }]
        )
        .is_err()
    );
    assert_eq!(saved(&path), before);
    let mut db = database::open_existing(&path).unwrap();
    crate::journal::delete_entry(&mut db, "entry").unwrap();
    db.close().unwrap();
    let after = saved(&path);
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::Visit { .. }))
            .count(),
        1
    );
    assert!(after.contains(&Record::Link {
        entry_id: "other".into(),
        visit_id: id
    }));
    drop(state);
    let reopened = DatabaseState::default();
    reopened
        .initialize_startup(&temp.path().join("journal"))
        .unwrap();
    assert_eq!(saved(&path), after);
}
#[test]
fn ebird_backups_exports_and_older_reader_normalization() {
    use std::{fs::File, sync::Arc};
    let (temp, state, path) = fixture();
    apply(&state, &[input("S100")]).unwrap();
    additions(&path);
    let prior = saved(&path);
    apply(&state, &[input("S101")]).unwrap();
    let mut changed = input("S100");
    changed.rows[0].insert("Count".into(), "5".into());
    changed.rows[0].insert(
        "Observation Details".into(),
        "Corrected source comment".into(),
    );
    changed.rows[0].insert("ML Catalog Numbers".into(), "10002 10003".into());
    apply(&state, &[changed]).unwrap();
    let expected = saved(&path);
    for record in &prior {
        match record {
            Record::Addition { .. } | Record::Link { .. } => assert!(expected.contains(record)),
            Record::Row { id, .. } => assert!(
                expected
                    .iter()
                    .any(|r| matches!(r,Record::Row{id:current,active:true,..} if current==id))
            ),
            _ => {}
        }
    }
    let flag = AtomicBool::new(false);
    let mut db =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = crate::storage_content::verify_v14(&mut db, &flag).unwrap();
    let backup = temp.path().join("synthetic.mno-backup");
    let export = temp.path().join("synthetic.mno-export");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    db.close().unwrap();
    let restore =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(records(restore.source().unwrap()).unwrap(), expected);
    assert_eq!(
        crate::storage_content::verified_snapshot_v14(restore.source().unwrap(), &flag).unwrap(),
        proof
    );
    drop(restore);
    let candidate = crate::open_import::build_for_schema13(
        &mut File::open(&export).unwrap(),
        temp.path(),
        &flag,
        14,
    )
    .unwrap();
    assert_eq!(candidate.proof().sha256, proof.sha256);
    assert_eq!(saved(candidate.path()), expected);
    drop(candidate);
    // Older schema12 export normalizes to a verified schema13 with empty eBird tables.
    let old = DatabaseState::default();
    let oldpath = old
        .initialize_schema12_startup(&temp.path().join("old"))
        .unwrap();
    let mut db =
        Connection::open_with_flags(&oldpath, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let oldexport = temp.path().join("older.mno-export");
    crate::open_export::export(&mut db, &oldexport, &flag).unwrap();
    db.close().unwrap();
    let candidate = crate::open_import::build_for_schema13(
        &mut File::open(oldexport).unwrap(),
        temp.path(),
        &flag,
        13,
    )
    .unwrap();
    assert!(saved(candidate.path()).is_empty());
    assert_eq!(candidate.proof().version, 14);
    assert_eq!(saved(&path), expected);
}

#[test]
fn ebird_failed_transaction_reverts_all_writes() {
    let (_temp, state, path) = fixture();
    let before = saved(&path);
    assert!(
        import_checked(
            &state,
            &SearchState::default(),
            &crate::bird_module::test_module(true),
            1,
            &[input("S100")],
            |tx| {
                assert_eq!(records(tx).unwrap().len(), 2);
                tx.execute(
                    "INSERT INTO journal_ebird_visits VALUES('missing-entry','missing-visit')",
                    [],
                )
                .map_err(sql)?;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(saved(&path), before);
}
#[test]
fn ebird_reordered_unchanged_rows_keep_identity_but_disappearing_rows_keep_additions() {
    let (_temp, state, path) = fixture();
    let mut first = input("S100");
    let mut second = first.rows[0].clone();
    second.insert("Common Name".into(), "Other bird".into());
    second.insert("Scientific Name".into(), "Other source taxon".into());
    first.rows.push(second);
    apply(&state, &[first.clone()]).unwrap();
    additions(&path);
    let before = saved(&path);
    first.rows.reverse();
    apply(&state, &[first.clone()]).unwrap();
    let after = saved(&path);
    for old in &before {
        if let Record::Row {
            id, source_json, ..
        } = old
        {
            assert!(after.iter().any(|r|matches!(r,Record::Row{id:new,source_json:source,active:true,..} if new==id&&source==source_json)));
        }
    }
    first.rows.clear();
    first.rows.push(input("S100").rows.remove(0));
    apply(&state, &[first]).unwrap();
    let after = saved(&path);
    for a in before
        .iter()
        .filter(|r| matches!(r, Record::Addition { .. } | Record::Link { .. }))
    {
        assert!(after.contains(a));
    }
    assert_eq!(
        after
            .iter()
            .filter(|r| matches!(r, Record::Row { active: false, .. }))
            .count(),
        1
    );
}
#[test]
fn ebird_archive_activation_preserves_replaced_content_and_restarts() {
    use std::{fs, sync::Arc};
    const CHILD: &str = "MNO_EBIRD_ACTIVATION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        for mode in ["restore", "import", "restore12", "import13"] {
            let root = tempfile::Builder::new()
                .prefix("mno-ebird-activation-")
                .tempdir()
                .unwrap();
            let token = uuid::Uuid::new_v4().to_string();
            fs::write(root.path().join("synthetic-token"), &token).unwrap();
            let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","ebird_persistence::tests::ebird_archive_activation_preserves_replaced_content_and_restarts","--nocapture"])
       .env(CHILD,mode).env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE","").env("MNO_EBIRD_TOKEN",token).output().unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        return;
    }
    let mode = std::env::var(CHILD).unwrap();
    let raw = std::path::PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    assert!(!fs::symlink_metadata(&raw).unwrap().file_type().is_symlink());
    let root = fs::canonicalize(raw).unwrap();
    assert_eq!(
        root.parent().unwrap(),
        fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mno-ebird-activation-")
    );
    assert_eq!(
        fs::read_to_string(root.join("synthetic-token")).unwrap(),
        std::env::var("MNO_EBIRD_TOKEN").unwrap()
    );
    assert!(!root.join("data").exists());
    let older = mode == "restore12" || mode == "import13";
    let restoring = mode.starts_with("restore");
    let source_format = if older { 13 } else { 14 };
    {
        let (state, path) = fixture_at(&root.join("data"));
        apply(&state, &[input("S100"), input("S101")]).unwrap();
        additions(&path);
        let expected = if older { Vec::new() } else { saved(&path) };
        let flag = AtomicBool::new(false);
        let source = if older {
            let (old, oldpath) = fixture_at_schema(&root.join("older"), 12);
            drop(old);
            oldpath
        } else {
            path.clone()
        };
        let mut db =
            Connection::open_with_flags(&source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let archive = root.as_path().join(if restoring {
            "roundtrip.mno-backup"
        } else {
            "roundtrip.mno-export"
        });
        if restoring {
            crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        } else {
            crate::open_export::export(&mut db, &archive, &flag).unwrap();
        }
        db.close().unwrap();
        apply(&state, &[input("S102")]).unwrap();
        let replaced = saved(&path);
        let old = crate::backup::restore_context::activation::journal_proof(&path).unwrap();
        let maintenance = state.maintenance().unwrap();
        if restoring {
            drop(
                crate::backup::restore_context::activation::activate(
                    &maintenance,
                    &archive,
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap(),
            );
        } else {
            let candidate = crate::open_import::build_for_schema13(
                &mut fs::File::open(&archive).unwrap(),
                root.as_path(),
                &flag,
                source_format,
            )
            .unwrap();
            let proof = candidate.proof().clone();
            drop(candidate);
            drop(
                crate::backup::restore_context::activation::importing::activate_schema13(
                    &maintenance,
                    &mut fs::File::open(&archive).unwrap(),
                    &old,
                    &proof,
                    Arc::new(AtomicBool::new(false)),
                    source_format,
                )
                .unwrap(),
            );
        }
        drop(maintenance);
        drop(state);
        let installed = fs::read(&path).unwrap();
        let recovery = DatabaseState::default();
        assert!(
            recovery
                .initialize_startup(&root.as_path().join("data"))
                .is_err()
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            installed,
            "pending Recovery must not upgrade its journal"
        );
        use crate::backup::restore_context::activation::startup::{self, Action};
        assert!(
            startup::status(&recovery)
                .unwrap()
                .actions
                .contains(&Action::FinishNew)
        );
        startup::run(&recovery, Action::FinishNew).unwrap();
        drop(recovery);
        let ready = DatabaseState::default();
        ready
            .initialize_startup(&root.as_path().join("data"))
            .unwrap();
        assert_eq!(saved(&path), expected);
        let db = database::open_existing(&path).unwrap();
        assert!(database::ebird_schema::validate_schema(&db).unwrap());
        db.close().unwrap();
        if mode == "restore12" {
            assert!(
                fs::read_dir(path.parent().unwrap().join("backups/bird-schema-upgrade"))
                    .unwrap()
                    .any(|e| fs::read(e.unwrap().path().join("schema12.sqlite"))
                        .ok()
                        .as_ref()
                        == Some(&installed))
            );
        }
        let backups = fs::read_dir(path.parent().unwrap().join("backups/pre-restore")).unwrap();
        let mut protected = false;
        for entry in backups {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "mno-backup") {
                if let Ok(candidate) = crate::backup::restore::prepare_candidate(
                    &path,
                    Arc::new(AtomicBool::new(false)),
                ) {
                    if records(candidate.source().unwrap()).ok().as_ref() == Some(&replaced) {
                        protected = true;
                    }
                }
            }
        }
        assert!(
            protected,
            "replaced source snapshots/additions must remain in protection"
        );
    }
}

#[test]
fn ebird_all_supported_modern_exports_normalize_without_changing_source_proofs() {
    use std::fs::File;
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("older.sqlite");
    database::create_current_database(&path).unwrap();
    let flag = AtomicBool::new(false);
    for schema in 4..=12 {
        let mut db = database::open_existing(&path).unwrap();
        match schema {
            5 => database::bird_schema::upgrade(&mut db).unwrap(),
            6 => database::bird_visits::upgrade(&mut db).unwrap(),
            7 => database::bird_visit_identity::upgrade(&mut db).unwrap(),
            8 => database::bird_comments::upgrade(&mut db).unwrap(),
            9 => database::bird_requests::upgrade(&mut db).unwrap(),
            10 => database::bird_photos::upgrade(&mut db).unwrap(),
            11 => database::bird_captions::upgrade(&mut db).unwrap(),
            12 => database::bird_visit_weather::upgrade(&mut db).unwrap(),
            _ => {}
        }
        db.close().unwrap();
        let mut read =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let archive = temp.path().join(format!("schema{schema}.mno-export"));
        crate::open_export::export(&mut read, &archive, &flag).unwrap();
        let proof =
            crate::open_import::verify_database_version(&mut read, &flag, schema + 1).unwrap();
        read.close().unwrap();
        let candidate = crate::open_import::build_for_schema13(
            &mut File::open(archive).unwrap(),
            temp.path(),
            &flag,
            schema + 1,
        )
        .unwrap();
        let mut normalized = Connection::open_with_flags(
            candidate.path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let tx = normalized.transaction().unwrap();
        let preserved = match schema {
            4 => crate::storage_content::verified_snapshot_v5(&tx, &flag),
            5 => crate::storage_content::verified_snapshot_v6(&tx, &flag),
            6 => crate::storage_content::verified_snapshot_v7(&tx, &flag),
            7 => crate::storage_content::verified_snapshot_v8(&tx, &flag),
            8 => crate::storage_content::verified_snapshot_v9(&tx, &flag),
            9 => crate::storage_content::verified_snapshot_v10(&tx, &flag),
            10 => crate::storage_content::verified_snapshot_v11(&tx, &flag),
            11 => crate::storage_content::verified_snapshot_v12(&tx, &flag),
            12 => crate::storage_content::verified_snapshot_v13(&tx, &flag),
            _ => unreachable!(),
        }
        .unwrap();
        assert_eq!(preserved.sha256, proof.sha256);
        assert_eq!(preserved.entry_count, proof.entry_count);
        assert!(records(&tx).unwrap().is_empty());
        tx.commit().unwrap();
        normalized.close().unwrap();
    }
}
#[test]
fn ebird_tampered_archive_with_recomputed_checksums_is_rejected_without_active_mutation() {
    use sha2::{Digest, Sha256};
    use std::{
        fs::File,
        io::{Read, Write},
    };
    let (temp, state, path) = fixture();
    apply(&state, &[input("S100")]).unwrap();
    let archive = temp.path().join("valid.mno-export");
    let flag = AtomicBool::new(false);
    let mut db =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    crate::open_export::export(&mut db, &archive, &flag).unwrap();
    db.close().unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(entry.name().to_string(), bytes);
    }
    drop(zip);
    for mode in ["count", "reference", "duplicate-field"] {
        let mut payload = files.clone();
        let raw = String::from_utf8(payload["ebird.jsonl"].clone()).unwrap();
        let mut lines = raw
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        for record in &mut lines {
            if record["record"] == "Row" {
                if mode == "reference" {
                    record["visit_id"] = serde_json::json!("99999999-9999-4999-8999-999999999999");
                } else {
                    let mut fields: Fields =
                        serde_json::from_str(record["source_json"].as_str().unwrap()).unwrap();
                    fields.insert("Count".into(), "0".into());
                    let source = if mode == "duplicate-field" {
                        serde_json::to_string(&fields).unwrap().replacen(
                            '{',
                            "{\"Count\":\"X\",",
                            1,
                        )
                    } else {
                        serde_json::to_string(&fields).unwrap()
                    };
                    record["source_json"] = serde_json::json!(source);
                }
            }
        }
        let bytes = lines
            .iter()
            .map(|r| serde_json::to_string(r).unwrap() + "\n")
            .collect::<String>()
            .into_bytes();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&payload["manifest.json"]).unwrap();
        manifest["files"]["ebird.jsonl"]["sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
        manifest["files"]["ebird.jsonl"]["size"] = serde_json::json!(bytes.len());
        payload.insert("ebird.jsonl".into(), bytes);
        payload.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let bad = temp.path().join(format!("{mode}.mno-export"));
        let mut writer = zip::ZipWriter::new(File::create(&bad).unwrap());
        for (name, bytes) in payload {
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        assert!(
            crate::open_import::build_for_schema13(
                &mut File::open(&bad).unwrap(),
                temp.path(),
                &flag,
                14
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn bounded_continuity_exact_first_and_ambiguous_candidates_never_guess() {
    let a = input("S100").rows.remove(0);
    let mut corrected = a.clone();
    corrected.insert("Count".into(), "4".into());
    assert_eq!(
        match_rows(&[a.clone()], &[corrected.clone()]),
        vec![Some(0)]
    );
    assert_eq!(
        match_rows(
            &[a.clone(), corrected.clone()],
            &[corrected.clone(), a.clone()]
        ),
        vec![Some(1), Some(0)]
    );
    let mut further = a.clone();
    further.insert("Count".into(), "6".into());
    assert_eq!(
        match_rows(
            &[a.clone(), corrected.clone()],
            &[corrected.clone(), further.clone()]
        ),
        vec![Some(1), Some(0)]
    );
    assert_eq!(
        match_rows(&[a.clone(), a.clone()], &[corrected.clone()]),
        vec![None]
    );
    assert_eq!(
        match_rows(&[a.clone()], &[corrected.clone(), further.clone()]),
        vec![None, None]
    );
    assert_eq!(
        match_rows(&[a.clone(), corrected.clone()], &[further.clone()]),
        vec![None]
    );
    for field in [
        "Scientific Name",
        "Common Name",
        "Taxonomic Order",
        "Location",
        "Checklist Comments",
        "Date",
    ] {
        let mut changed = a.clone();
        changed.insert(field.into(), "changed".into());
        assert_eq!(match_rows(&[a.clone()], &[changed]), vec![None], "{field}");
    }
}
#[test]
fn bounded_corrections_keep_row_additions_links_and_existing_manual_photos() {
    let (temp, state, path) = fixture();
    let first = input("S100");
    apply(&state, &[first.clone()]).unwrap();
    additions(&path);
    let original = saved(&path);
    let row = original
        .iter()
        .find_map(|r| {
            if let Record::Row { id, .. } = r {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    let db = database::open_existing(&path).unwrap();
    let photos: Vec<(String, Vec<u8>, Option<String>)> = db
        .prepare("SELECT row_id,preview,caption FROM bird_row_photos ORDER BY row_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(!photos.is_empty());
    db.close().unwrap();
    let original_file = temp.path().join("disposable-original.png");
    let original_bytes = std::fs::read(&original_file).unwrap();
    let mut visit = first;
    for (field, value) in [
        ("Count", "4"),
        ("Observation Details", "Corrected source comment"),
        ("ML Catalog Numbers", "10002 10003 10004"),
    ] {
        visit.rows[0].insert(field.into(), value.into());
        apply(&state, &[visit.clone()]).unwrap();
        let current = saved(&path);
        assert!(current.iter().any(|r|matches!(r,Record::Row{id,active:true,source_json,..} if id==&row&&serde_json::from_str::<Fields>(source_json).unwrap()==visit.rows[0])));
        assert_eq!(
            current
                .iter()
                .filter(|r| matches!(r, Record::Row { .. }))
                .count(),
            1
        );
        for r in original.iter().filter(|r| {
            matches!(
                r,
                Record::Addition { .. } | Record::Link { .. } | Record::Visit { .. }
            )
        }) {
            assert!(current.contains(r));
        }
    }
    let before = saved(&path);
    let bytes = std::fs::read(&path).unwrap();
    apply(&state, &[visit.clone()]).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut correction = visit.clone();
    correction.rows[0].insert("Count".into(), "8".into());
    assert!(
        import_checked(
            &state,
            &SearchState::default(),
            &crate::bird_module::test_module(true),
            1,
            &[correction],
            |_| Err("injected after correction".into())
        )
        .is_err()
    );
    assert_eq!(saved(&path), before);
    let db = database::open_existing(&path).unwrap();
    let after: Vec<(String, Vec<u8>, Option<String>)> = db
        .prepare("SELECT row_id,preview,caption FROM bird_row_photos ORDER BY row_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(after, photos);
    db.close().unwrap();
    assert_eq!(std::fs::read(original_file).unwrap(), original_bytes);
    let mut renamed = visit;
    renamed.rows[0].insert("Scientific Name".into(), "Changed taxon name".into());
    apply(&state, &[renamed]).unwrap();
    assert!(
        saved(&path)
            .iter()
            .any(|r| matches!(r,Record::Row{id,active:false,..} if id==&row))
    );
    for r in original
        .iter()
        .filter(|r| matches!(r, Record::Addition { .. } | Record::Link { .. }))
    {
        assert!(saved(&path).contains(r));
    }
}

#[test]
fn duplicate_source_candidates_reject_atomically_for_explicit_review() {
    let (_temp, state, path) = fixture();
    let original = input("S100");
    apply(&state, &[original.clone()]).unwrap();
    additions(&path);
    let before = saved(&path);
    let mut duplicate = original.clone();
    let mut row = duplicate.rows[0].clone();
    row.insert("Count".into(), "6".into());
    duplicate.rows.push(row);
    assert!(apply(&state, &[duplicate]).is_err());
    assert_eq!(saved(&path), before);
    for field in ["Common Name", "Taxonomic Order", "Checklist Comments"] {
        let mut changed = original.clone();
        changed.rows[0].insert(
            field.into(),
            if field == "Taxonomic Order" {
                "1"
            } else {
                "Changed source value"
            }
            .into(),
        );
        apply(&state, &[changed]).unwrap();
        let after = saved(&path);
        for a in before
            .iter()
            .filter(|r| matches!(r, Record::Addition { .. } | Record::Link { .. }))
        {
            assert!(after.contains(a));
        }
    }
}
