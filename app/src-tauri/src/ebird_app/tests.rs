use super::*;
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
                ("Date", "2026-11-23"),
                ("Time", "8:15 AM"),
                ("ML Catalog Numbers", "10001 10002"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        ],
    }
}
fn fixture() -> (tempfile::TempDir, DatabaseState, std::path::PathBuf) {
    let root = tempfile::TempDir::new().unwrap();
    let (state, path) =
        crate::ebird_persistence::tests::fixture_at_schema(&root.path().join("journal"), 13);
    (root, state, path)
}
#[test]
fn confirmed_import_comparison_noop_status_cancel_and_stale_reviews() {
    let (_root, state, path) = fixture();
    let module = crate::bird_module::test_module(true);
    let slots = ImportState::default();
    let search = SearchState::default();
    let original = std::fs::read(&path).unwrap();
    let review = prepare(
        &state,
        &module,
        &slots,
        1,
        vec![input("S100"), input("S101")],
    )
    .unwrap();
    assert!(review.changes.iter().all(|c| c.kind == "new"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // Cancelling a review drops only its transient capability.
    slots.0.lock().unwrap().remove(&review.operation_id);
    assert_eq!(
        status(&state, &module, &slots, 1, &review.operation_id).unwrap(),
        "unknown"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let review = prepare(
        &state,
        &module,
        &slots,
        1,
        vec![input("S100"), input("S101")],
    )
    .unwrap();
    confirm(&state, &module, &search, &slots, 1, &review.operation_id).unwrap();
    // Lost response: resolve the same operation, never apply it again.
    assert_eq!(
        status(&state, &module, &slots, 1, &review.operation_id).unwrap(),
        "committed"
    );
    assert!(confirm(&state, &module, &search, &slots, 1, &review.operation_id).is_err());
    let bytes = std::fs::read(&path).unwrap();
    let review = prepare(&state, &module, &slots, 1, vec![input("S100")]).unwrap();
    assert_eq!(
        review
            .changes
            .iter()
            .filter(|c| c.kind == "retained")
            .count(),
        1
    );
    assert_eq!(
        review
            .changes
            .iter()
            .filter(|c| c.kind == "unchanged")
            .count(),
        1
    );
    confirm(&state, &module, &search, &slots, 1, &review.operation_id).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut change = input("S100");
    change.rows[0].insert("Count".into(), "4".into());
    let review = prepare(&state, &module, &slots, 1, vec![change]).unwrap();
    assert_eq!(review.changes[0].kind, "changed");
    crate::ebird_persistence::import(&state, &search, &module, 1, &[input("S102")]).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        confirm(&state, &module, &search, &slots, 1, &review.operation_id).unwrap_err(),
        "ebird_review_stale"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let review = prepare(&state, &module, &slots, 1, vec![input("S100")]).unwrap();
    state.maintenance().unwrap().advance_generation().unwrap();
    assert!(confirm(&state, &module, &search, &slots, 1, &review.operation_id).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn expired_disabled_and_reenabled_reviews_cannot_write() {
    let (_root, state, path) = fixture();
    let module = crate::bird_module::test_module(true);
    let slots = ImportState::default();
    let before = std::fs::read(&path).unwrap();
    let review = prepare(&state, &module, &slots, 1, vec![input("S100")]).unwrap();
    slots
        .0
        .lock()
        .unwrap()
        .get_mut(&review.operation_id)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    assert!(
        confirm(
            &state,
            &module,
            &SearchState::default(),
            &slots,
            1,
            &review.operation_id
        )
        .is_err()
    );
    assert!(
        prepare(
            &state,
            &crate::bird_module::test_module(false),
            &slots,
            1,
            vec![input("S100")]
        )
        .is_err()
    );
    let review = prepare(&state, &module, &slots, 1, vec![input("S100")]).unwrap();
    slots.invalidate().unwrap();
    assert!(
        confirm(
            &state,
            &module,
            &SearchState::default(),
            &slots,
            1,
            &review.operation_id
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn confirmed_import_reopens_and_survives_both_archive_paths() {
    use std::{
        fs::File,
        sync::{Arc, atomic::AtomicBool},
    };
    let (root, state, path) = fixture();
    let module = crate::bird_module::test_module(true);
    let slots = ImportState::default();
    let search = SearchState::default();
    let review = prepare(
        &state,
        &module,
        &slots,
        1,
        vec![input("S100"), input("S101")],
    )
    .unwrap();
    confirm(&state, &module, &search, &slots, 1, &review.operation_id).unwrap();
    // Simulate an uncertain commit reply; status proves content without replaying writes.
    slots
        .0
        .lock()
        .unwrap()
        .get_mut(&review.operation_id)
        .unwrap()
        .status = "uncertain".into();
    assert_eq!(
        status(&state, &module, &slots, 1, &review.operation_id).unwrap(),
        "committed"
    );
    let connection = ConnectionPreference::load(root.path().join("bird-module-v1.json"));
    assert!(!connection.0.enabled());
    let flag = AtomicBool::new(false);
    let mut db =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let expected = store::records(&db).unwrap();
    let proof = crate::storage_content::verify_v14(&mut db, &flag).unwrap();
    let backup = root.path().join("confirmed.mno-backup");
    let export = root.path().join("confirmed.mno-export");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    drop(db);
    let reopened = crate::database::open_existing(&path).unwrap();
    assert_eq!(store::records(&reopened).unwrap(), expected);
    drop(reopened);
    let restored =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        store::records(restored.source().unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        crate::storage_content::verified_snapshot_v14(restored.source().unwrap(), &flag).unwrap(),
        proof
    );
    let imported = crate::open_import::build_for_schema13(
        &mut File::open(export).unwrap(),
        root.path(),
        &flag,
        14,
    )
    .unwrap();
    let db = crate::database::open_existing(imported.path()).unwrap();
    assert_eq!(store::records(&db).unwrap(), expected);
    assert_eq!(imported.proof().sha256, proof.sha256);
    assert!(!connection.0.enabled());
}

#[test]
fn connection_defaults_off_and_is_isolated_from_bird_and_production_preferences() {
    let root = tempfile::tempdir().unwrap();
    let dev = crate::bird_module::preference_path(root.path(), "dev");
    let production = crate::bird_module::preference_path(root.path(), "production");
    let bird = BirdModule::load(dev.clone());
    let connection = ConnectionPreference::load(dev.clone());
    let slots = ImportState::default();
    assert!(connection.0.command().is_err());
    bird.set(true, || Ok(())).unwrap();
    assert!(connection.0.command().is_err());
    connection.0.set(true, || slots.invalidate()).unwrap();
    assert!(ConnectionPreference::load(dev.clone()).0.enabled());
    assert!(!ConnectionPreference::load(production).0.enabled());
    bird.set(false, || slots.invalidate()).unwrap();
    assert!(bird.command().is_err());
    assert!(connection.0.enabled());
    bird.set(true, || slots.invalidate()).unwrap();
    assert!(ConnectionPreference::load(dev).0.enabled());
}

#[test]
fn disable_waits_for_admitted_commit_and_reenable_rejects_old_review_and_picker() {
    let (root, state, path) = fixture();
    let module = crate::bird_module::test_module(true);
    let connection = ConnectionPreference::load(root.path().join("bird-module-v1.json"));
    let slots = ImportState::default();
    connection.0.set(true, || slots.invalidate()).unwrap();
    let epoch = slots.1.load(Ordering::Relaxed);
    let review = prepare(&state, &module, &slots, 1, vec![input("S100")]).unwrap();
    let unused = prepare(&state, &module, &slots, 1, vec![input("S101")]).unwrap();
    let search = SearchState::default();
    std::thread::scope(|scope| {
        let (admitted, started) = std::sync::mpsc::channel();
        let permit = connection.0.command().unwrap();
        let (setting, reviews) = (&connection, &slots);
        let disable = scope.spawn(move || {
            admitted.send(()).unwrap();
            setting.0.set(false, || reviews.invalidate()).unwrap();
        });
        started.recv().unwrap();
        confirm(&state, &module, &search, &slots, 1, &review.operation_id).unwrap();
        assert!(slots.check_epoch(epoch).is_ok());
        drop(permit);
        disable.join().unwrap();
    });
    assert!(connection.0.command().is_err());
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(
        store::records(&crate::database::open_existing(&path).unwrap())
            .unwrap()
            .iter()
            .filter(|record| matches!(record, Record::Visit { .. }))
            .count(),
        1
    );
    connection.0.set(true, || slots.invalidate()).unwrap();
    assert!(slots.check_epoch(epoch).is_err());
    assert!(confirm(&state, &module, &search, &slots, 1, &unused.operation_id).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
}
