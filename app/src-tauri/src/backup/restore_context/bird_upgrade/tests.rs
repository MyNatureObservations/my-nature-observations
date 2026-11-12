use super::*;
use tempfile::TempDir;
const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Preserved synthetic journal"}]}]}"#;

fn fixture(root: &Path) -> (PathBuf, Vec<u8>, crate::storage_content::Proof) {
    let path = root.join("mno.sqlite");
    crate::database::create_current_database(&path).unwrap();
    let mut db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('entry','2026-11-11','Synthetic',?1,2,'created','updated')",[DOC]).unwrap();
    let image = root.join("original.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 100, 80, 255]))
        .save(&image)
        .unwrap();
    let asset = crate::photographs::prepare(&image, "entry".into()).unwrap();
    asset.insert(&db).unwrap();
    let document = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Preserved synthetic journal"}]},{"type":"journalImage","attrs":{"nodeId":uuid::Uuid::new_v4().to_string(),"assetId":asset.asset_id,"placement":"left","size":"small","caption":"Synthetic","alt":"Green"}}]}).to_string();
    db.execute(
        "UPDATE journal_entries SET content_json=?1 WHERE id='entry'",
        [document],
    )
    .unwrap();
    let place = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO places VALUES (?1,'Synthetic lake',59.33,18.07,1)",
        [&place],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_places VALUES ('entry',?1,'Synthetic lake',59.33,18.07,1,1)",
        [&place],
    )
    .unwrap();
    let weather=serde_json::json!({"period":{"kind":"day","date":"2026-11-11"},"location":{"name":"Synthetic lake","coordinates":{"latitude":59.33,"longitude":18.07}},"temperatureC":7}).to_string();
    db.execute(
        "INSERT INTO journal_weather VALUES ('entry',1,?1,NULL,1)",
        [weather],
    )
    .unwrap();
    let proof = crate::storage_content::verify_v5(&mut db, &AtomicBool::new(false)).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}
fn retained(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join("backups/bird-schema-upgrade"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("schema4.sqlite"))
        .filter(|path| path.exists())
        .collect()
}
fn add_lists(path: &Path) -> Vec<crate::bird_observations::List> {
    use crate::bird_observations::{self, SpeciesCount};
    let state = crate::database_state::DatabaseState::default();
    state
        .initialize_bird_startup(path.parent().unwrap())
        .unwrap();
    let search = crate::journal_search::SearchState::default();
    let db = crate::database::open_existing(path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    close(db);
    let rows = vec![
        SpeciesCount {
            avibase_id: "avibase-12345678".into(),
            taxonomy_version: "AviList v2026".into(),
            count: 6,
        },
        SpeciesCount {
            avibase_id: "avibase-87654321".into(),
            taxonomy_version: "AviList v2026".into(),
            count: 2,
        },
    ];
    let generation = state.operation(None).unwrap().snapshot(()).generation;
    [("2026-11-11", Some("entry")), ("2026-11-12", None)]
        .into_iter()
        .map(|(date, link)| {
            let id =
                bird_observations::create(&state, &search, generation, date, &place, &rows, link)
                    .unwrap();
            let reopened = bird_observations::reopen(&state, generation, &id).unwrap();
            assert_eq!(reopened.id, id);
            assert_eq!(reopened.rows, rows);
            assert_eq!(reopened.place_id, place);
            assert_eq!(reopened.visit_date, date);
            assert_eq!(
                reopened.journal_entries,
                link.into_iter().map(str::to_owned).collect::<Vec<_>>()
            );
            reopened
        })
        .collect()
}
#[test]
fn upgrade_preserves_source_and_content_and_roundtrips_linked_and_unlinked_lists() {
    let root = TempDir::new().unwrap();
    let (path, bytes, before) = fixture(root.path());
    initialize(&path).unwrap();
    assert_eq!(verify(&path, true).unwrap(), before);
    let sources = retained(root.path());
    assert_eq!(sources.len(), 1);
    assert_eq!(fs::read(&sources[0]).unwrap(), bytes);
    initialize(&path).unwrap();
    add_lists(&path);
    let mut db = crate::database::open_existing(&path).unwrap();
    let proof = crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap();
    let archive = root.path().join("birds.mno-backup");
    let manifest =
        crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (6, 5)
    );
    let candidate =
        crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v6(
            candidate.source().unwrap(),
            &AtomicBool::new(false)
        )
        .unwrap(),
        proof
    );
    let closed = candidate.try_close().map_err(|_| ()).unwrap();
    drop(closed);
    crate::journal::delete_entry(&mut db, "entry").unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM bird_lists", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_bird_lists", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
    close(db);
    assert_eq!(fs::read(&sources[0]).unwrap(), bytes);
}
#[test]
fn interruption_on_each_side_of_publication_retries_without_losing_source() {
    for boundary in [
        "copied",
        "upgraded",
        "before-publish",
        "after-publish",
        "verified",
    ] {
        let root = TempDir::new().unwrap();
        let (path, bytes, before) = fixture(root.path());
        let result = initialize_with(&path, |name, _| {
            if name == boundary {
                Err("injected interruption".into())
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        if ["copied", "upgraded", "before-publish"].contains(&boundary) {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        } else {
            assert!(
                retained(root.path())
                    .iter()
                    .any(|p| fs::read(p).unwrap() == bytes)
            );
        }
        initialize(&path).unwrap();
        assert_eq!(verify(&path, true).unwrap(), before);
    }
}
#[test]
fn invalid_candidate_never_replaces_source() {
    let root = TempDir::new().unwrap();
    let (path, bytes, _) = fixture(root.path());
    assert!(
        initialize_with(&path, |point, candidate| {
            if point == "before-publish" {
                fs::write(candidate, b"damaged synthetic candidate").unwrap();
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    initialize(&path).unwrap();
}
#[test]
fn older_backup_still_prepares_and_upgrades_without_modifying_archive() {
    let root = TempDir::new().unwrap();
    let (path, _, before) = fixture(root.path());
    let db = crate::database::open_existing(&path).unwrap();
    let archive = root.path().join("old.mno-backup");
    crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
    close(db);
    let bytes = fs::read(&archive).unwrap();
    let candidate =
        crate::backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
            .unwrap();
    let output = TempDir::new().unwrap();
    let restored = output.path().join("mno.sqlite");
    let mut db = Connection::open(&restored).unwrap();
    rusqlite::backup::Backup::new(candidate.source().unwrap(), &mut db)
        .unwrap()
        .run_to_completion(128, std::time::Duration::ZERO, None)
        .unwrap();
    close(db);
    drop(candidate.try_close().map_err(|_| ()).unwrap());
    initialize(&restored).unwrap();
    assert_eq!(verify(&restored, true).unwrap(), before);
    assert_eq!(fs::read(&archive).unwrap(), bytes);
}
#[test]
fn frozen_schema4_reader_rejects_upgrade_without_writing() {
    let root = TempDir::new().unwrap();
    let (path, _, _) = fixture(root.path());
    initialize(&path).unwrap();
    let before = fs::read(&path).unwrap();
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    // Same version and exact schema checks as the published schema-4 initializer.
    assert_ne!(
        db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        4
    );
    assert!(!crate::photographs::validate_schema(&db).unwrap());
    close(db);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn process_interruption_before_and_after_publication_is_restartable() {
    for boundary in ["before-publish", "after-publish"] {
        let root = TempDir::new().unwrap();
        let (path, bytes, before) = fixture(root.path());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::bird_upgrade::tests::upgrade_process_child",
                "--nocapture",
            ])
            .env("MNO_BIRD_UPGRADE_ROOT", root.path())
            .env("MNO_BIRD_UPGRADE_CRASH", boundary)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(77), "{output:?}");
        let state = crate::database_state::DatabaseState::default();
        assert_eq!(
            state.initialize_bird_startup(root.path()).unwrap(),
            path.canonicalize().unwrap()
        );
        assert_eq!(verify(&path, true).unwrap(), before);
        assert!(
            retained(root.path())
                .iter()
                .any(|p| fs::read(p).unwrap() == bytes)
        );
        let competing = crate::database_state::DatabaseState::default();
        assert!(competing.initialize_startup(root.path()).is_err());
        assert!(competing.operation(None).is_err());
    }
}
#[test]
fn upgrade_process_child() {
    let Some(root) = std::env::var_os("MNO_BIRD_UPGRADE_ROOT") else {
        return;
    };
    let state = crate::database_state::DatabaseState::default();
    state.initialize_bird_startup(Path::new(&root)).unwrap();
}
#[test]
fn publication_barrier_failures_do_not_lose_old_source() {
    for boundary in ["source-flush", "candidate-flush", "publication-flush"] {
        let root = TempDir::new().unwrap();
        let (path, bytes, before) = fixture(root.path());
        assert!(
            initialize_with(&path, |point, _| {
                if point == boundary {
                    Err("injected persistence failure".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        assert!(
            fs::read(&path).unwrap() == bytes
                || retained(root.path())
                    .iter()
                    .any(|p| fs::read(p).unwrap() == bytes)
        );
        initialize(&path).unwrap();
        assert_eq!(verify(&path, true).unwrap(), before);
    }
}

#[test]
fn restore_old_and_new_backups_over_upgraded_journal_use_existing_completion() {
    for format in ["5", "6"] {
        let root = TempDir::new().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::bird_upgrade::tests::restore_process_child",
                "--nocapture",
            ])
            .env("MNO_ACTIVATION_ROOT", root.path())
            .env("MNO_BIRD_RESTORE_FORMAT", format)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}
#[test]
fn restore_process_child() {
    let Ok(format) = std::env::var("MNO_BIRD_RESTORE_FORMAT") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let (path, _, before) = fixture(&data);
    let archive = root.join("input.mno-backup");
    let expected_lists = if format == "6" {
        initialize(&path).unwrap();
        add_lists(&path)
    } else {
        Vec::new()
    };
    let mut db = crate::database::open_existing(&path).unwrap();
    let expected = if format == "6" {
        Some(crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap())
    } else {
        None
    };
    crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
    close(db);
    let archive_before = fs::read(&archive).unwrap();
    let state = crate::database_state::DatabaseState::default();
    state.initialize_bird_startup(&data).unwrap();
    {
        let maintenance = state.maintenance().unwrap();
        let activated = super::super::activation::activate(
            &maintenance,
            &archive,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        drop(activated);
    }
    drop(state);
    let restarted = crate::database_state::DatabaseState::default();
    assert!(restarted.initialize_startup(&data).is_err());
    super::super::activation::startup::run(
        &restarted,
        super::super::activation::startup::Action::FinishNew,
    )
    .unwrap();
    assert!(restarted.operation(None).is_ok());
    drop(restarted);
    let final_start = crate::database_state::DatabaseState::default();
    final_start.initialize_bird_startup(&data).unwrap();
    assert_eq!(verify(&path, true).unwrap(), before);
    if format == "6" {
        let mut db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            Some(crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap()),
            expected
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM bird_lists", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            2
        );
        close(db);
    }
    let generation = final_start.operation(None).unwrap().snapshot(()).generation;
    for expected in expected_lists {
        assert_eq!(
            crate::bird_observations::reopen(&final_start, generation, &expected.id).unwrap(),
            expected
        );
    }
    assert_eq!(fs::read(archive).unwrap(), archive_before);
}

#[test]
fn schema5_content_rejects_invalid_rows_and_binds_counts_and_links() {
    let root = TempDir::new().unwrap();
    let (path, _, _) = fixture(root.path());
    initialize(&path).unwrap();
    add_lists(&path);
    let mut db = crate::database::open_existing(&path).unwrap();
    let flag = AtomicBool::new(false);
    let before = crate::storage_content::verify_v6(&mut db, &flag).unwrap();
    db.execute("UPDATE bird_rows SET count=7 WHERE list_id NOT IN (SELECT list_id FROM journal_bird_lists)", [])
        .unwrap();
    assert_ne!(
        crate::storage_content::verify_v6(&mut db, &flag).unwrap(),
        before
    );
    assert!(db.execute("DELETE FROM places", []).is_err());
    db.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE bird_rows SET count=0")
        .unwrap();
    assert!(crate::storage_content::verify_v6(&mut db, &flag).is_err());
    close(db);
}
#[test]
fn failed_upgrade_keeps_independent_source_copy() {
    let root = TempDir::new().unwrap();
    let (path, bytes, _) = fixture(root.path());
    assert!(
        initialize_with(&path, |point, _| {
            if point == "before-publish" {
                Err("interrupted".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let sources = retained(root.path());
    assert_eq!(sources.len(), 1);
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("UPDATE journal_entries SET title='later schema4 edit'", [])
        .unwrap();
    close(db);
    assert_eq!(fs::read(&sources[0]).unwrap(), bytes);
    initialize(&path).unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "later schema4 edit"
    );
    close(db);
}

#[test]
fn frozen_schema4_import_proof_rejects_schema5_without_writes() {
    let root = TempDir::new().unwrap();
    let (path, _, _) = fixture(root.path());
    initialize(&path).unwrap();
    add_lists(&path);
    let bytes = fs::read(&path).unwrap();
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert!(
        crate::open_import::verify_database_version(&mut db, &AtomicBool::new(false), 5).is_err()
    );
    close(db);
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn explicit_upgrade_and_ordinary_restart_keep_the_same_journal() {
    let root = TempDir::new().unwrap();
    let (path, bytes, before) = fixture(root.path());
    let ordinary = crate::database_state::DatabaseState::default();
    ordinary.initialize_startup(root.path()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(ordinary);
    let upgraded = crate::database_state::DatabaseState::default();
    upgraded.initialize_bird_startup(root.path()).unwrap();
    assert_eq!(verify(&path, true).unwrap(), before);
    drop(upgraded);
    for _ in 0..2 {
        let reopened = crate::database_state::DatabaseState::default();
        assert_eq!(
            reopened.initialize_startup(root.path()).unwrap(),
            path.canonicalize().unwrap()
        );
        assert_eq!(verify(&path, true).unwrap(), before);
        assert!(reopened.operation(None).is_ok());
        drop(reopened);
    }
}

#[test]
fn bird_persistence_rolls_back_invalid_rows_missing_link_and_stale_generation() {
    use crate::bird_observations::{self, SpeciesCount};
    let root = TempDir::new().unwrap();
    let (path, _, _) = fixture(root.path());
    let state = crate::database_state::DatabaseState::default();
    state.initialize_bird_startup(root.path()).unwrap();
    let search = crate::journal_search::SearchState::default();
    let generation = state.operation(None).unwrap().snapshot(()).generation;
    let db = crate::database::open_existing(&path).unwrap();
    let place: String = db
        .query_row("SELECT id FROM places", [], |r| r.get(0))
        .unwrap();
    close(db);
    let good = SpeciesCount {
        avibase_id: "synthetic-species".into(),
        taxonomy_version: "AviList v2026".into(),
        count: 3,
    };
    let mut bad = good.clone();
    bad.count = 0;
    let mut bad_id = good.clone();
    bad_id.avibase_id = " ".into();
    for rows in [vec![], vec![good.clone(), bad], vec![good.clone(), bad_id]] {
        assert!(
            bird_observations::create(
                &state,
                &search,
                generation,
                "2026-11-12",
                &place,
                &rows,
                Some("entry")
            )
            .is_err()
        );
    }
    assert!(
        bird_observations::create(
            &state,
            &search,
            generation,
            "2026-02-30",
            &place,
            std::slice::from_ref(&good),
            Some("entry")
        )
        .is_err()
    );
    // The final link INSERT fails after the list and rows have been inserted.
    assert!(
        bird_observations::create(
            &state,
            &search,
            generation,
            "2026-11-12",
            &place,
            std::slice::from_ref(&good),
            Some("missing-entry")
        )
        .is_err()
    );
    state.set_generation_for_test(generation + 1);
    assert_eq!(
        bird_observations::create(
            &state,
            &search,
            generation,
            "2026-11-12",
            &place,
            std::slice::from_ref(&good),
            Some("entry")
        )
        .unwrap_err(),
        crate::database_state::GENERATION_CONFLICT
    );
    let db = crate::database::open_existing(&path).unwrap();
    for table in ["bird_lists", "bird_rows", "journal_bird_lists"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            0
        );
    }
    close(db);
    let current = generation + 1;
    let id = bird_observations::create(
        &state,
        &search,
        current,
        "2026-11-12",
        &place,
        std::slice::from_ref(&good),
        Some("entry"),
    )
    .unwrap();
    let expected = bird_observations::reopen(&state, current, &id).unwrap();
    assert!(
        bird_observations::create(
            &state,
            &search,
            current,
            "2026-11-12",
            &place,
            std::slice::from_ref(&good),
            None
        )
        .is_err()
    );
    assert_eq!(
        bird_observations::reopen(&state, current, &id).unwrap(),
        expected
    );
    search
        .with_write(|| {
            let _permit = state.operation(Some(current))?;
            crate::journal::delete_entry(&mut crate::database::open_existing(&path)?, "entry")
        })
        .unwrap();
    let mut unlinked = expected;
    unlinked.journal_entries.clear();
    assert_eq!(
        bird_observations::reopen(&state, current, &id).unwrap(),
        unlinked
    );
    drop(state);
    let restarted = crate::database_state::DatabaseState::default();
    restarted.initialize_startup(root.path()).unwrap();
    let generation = restarted.operation(None).unwrap().snapshot(()).generation;
    assert_eq!(
        bird_observations::reopen(&restarted, generation, &id).unwrap(),
        unlinked
    );
}

#[test]
fn bird_list_places_keep_coordinates_and_remain_startup_and_backup_valid() {
    use crate::bird_observations::{self, SpeciesCount};
    for link in [Some("entry"), None] {
        let root = TempDir::new().unwrap();
        let (path, _, _) = fixture(root.path());
        let state = crate::database_state::DatabaseState::default();
        state.initialize_bird_startup(root.path()).unwrap();
        let generation = state.operation(None).unwrap().snapshot(()).generation;
        let search = crate::journal_search::SearchState::default();
        let db = crate::database::open_existing(&path).unwrap();
        let place_id: String = db
            .query_row("SELECT id FROM places", [], |r| r.get(0))
            .unwrap();
        let place_before = crate::place_registry::read_place(&db, &place_id).unwrap();
        close(db);
        let id = bird_observations::create(
            &state,
            &search,
            generation,
            "2026-11-12",
            &place_id,
            &[SpeciesCount {
                avibase_id: "synthetic-species".into(),
                taxonomy_version: "AviList v2026".into(),
                count: 3,
            }],
            link,
        )
        .unwrap();
        let list_before = bird_observations::reopen(&state, generation, &id).unwrap();
        let bytes_before = fs::read(&path).unwrap();
        assert_eq!(
            crate::place_registry::update_with(
                &state,
                generation,
                || crate::database::open_existing(&path),
                &place_id,
                1,
                "Changed name".into(),
                None,
                None
            )
            .err()
            .unwrap(),
            "place_coordinates_invalid"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes_before);
        assert_eq!(
            bird_observations::reopen(&state, generation, &id).unwrap(),
            list_before
        );
        let mut db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            crate::place_registry::read_place(&db, &place_id).unwrap(),
            place_before
        );
        let proof = crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap();
        let archive = root.path().join("after-rejection.mno-backup");
        crate::backup::create_backup(&db, &archive, crate::backup::BackupKind::Manual).unwrap();
        close(db);
        assert_eq!(
            crate::backup::verify_backup(&archive)
                .unwrap()
                .database_schema_version,
            5
        );
        drop(state);
        let reopened = crate::database_state::DatabaseState::default();
        reopened.initialize_startup(root.path()).unwrap();
        let generation = reopened.operation(None).unwrap().snapshot(()).generation;
        assert_eq!(
            bird_observations::reopen(&reopened, generation, &id).unwrap(),
            list_before
        );
        let mut db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap(),
            proof
        );
        close(db);

        // Schema 5 must still permit clearing an unrelated place's coordinates.
        let unused = crate::place_registry::create_with(
            &reopened,
            generation,
            || crate::database::open_existing(&path),
            "Unused".into(),
            Some(60.0),
            Some(19.0),
        )
        .unwrap()
        .data;
        let cleared = crate::place_registry::update_with(
            &reopened,
            generation,
            || crate::database::open_existing(&path),
            &unused.id,
            1,
            "Unused".into(),
            None,
            None,
        )
        .unwrap()
        .data;
        assert_eq!((cleared.latitude, cleared.longitude), (None, None));
        let mut db = crate::database::open_existing(&path).unwrap();
        crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap();
        close(db);

        // Keep the established weather-default error and its precedence.
        crate::place_registry::set_weather_default_place_with(
            &reopened,
            generation,
            || crate::database::open_existing(&path),
            0,
            Some(&place_id),
        )
        .unwrap();
        assert_eq!(
            crate::place_registry::update_with(
                &reopened,
                generation,
                || crate::database::open_existing(&path),
                &place_id,
                1,
                "Changed name".into(),
                None,
                None
            )
            .err()
            .unwrap(),
            "place_coordinates_required_by_weather_default"
        );
    }
}

mod export_import;
