use super::*;
use crate::{
    bird_observations as birds, database_state::DatabaseState, journal_search::SearchState,
};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};
struct Fixture {
    temp: tempfile::TempDir,
    state: DatabaseState,
    path: std::path::PathBuf,
    original: std::path::PathBuf,
    bytes: Vec<u8>,
    row: birds::SpeciesCount,
}
fn fixture() -> Fixture {
    fixture_version(false)
}
fn fixture_version(legacy: bool) -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    let root = std::env::var_os("MNO_ACTIVATION_ROOT")
        .filter(|_| std::env::var_os("MNO_BIRD_PHOTO_ACTIVATION").is_some())
        .map(std::path::PathBuf::from)
        .map(|p| p.join("data"))
        .unwrap_or_else(|| temp.path().join("journal"));
    let path = if legacy {
        state.initialize_schema10_startup(&root)
    } else {
        state.initialize_startup(&root)
    }
    .unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO places VALUES('11111111-1111-4111-8111-111111111111','Synthetic lake',59,18,1)",[]).unwrap();
    for id in ["first", "second"] {
        db.execute("INSERT INTO journal_entries VALUES(?1,'2026-11-17','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'created','updated')",[id]).unwrap();
    }
    db.close().unwrap();
    let original = temp.path().join("original.png");
    image::RgbImage::from_pixel(8, 6, image::Rgb([40, 80, 60]))
        .save(&original)
        .unwrap();
    let bytes = fs::read(&original).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut photo = Photo::from_asset(crate::photographs::prepare(&original, id.clone()).unwrap());
    photo.caption = if legacy {
        None
    } else {
        Some("Caption separate from comment".into())
    };
    Fixture {
        temp,
        state,
        path,
        original,
        bytes,
        row: birds::SpeciesCount {
            row_id: Some(id),
            photo: Some(photo),
            avibase_id: "avibase-FBBBA943".into(),
            taxonomy_version: crate::species_registry::VERSION.into(),
            count: 4,
            comment: Some("Original comment".into()),
        },
    }
}
fn create(f: &Fixture, linked: bool) -> String {
    birds::create_with_visit(
        &f.state,
        &SearchState::default(),
        1,
        "2026-11-17",
        "11111111-1111-4111-8111-111111111111",
        std::slice::from_ref(&f.row),
        linked.then_some("first"),
        Some("08:07"),
        None,
    )
    .unwrap()
}
fn update(
    f: &Fixture,
    id: &str,
    token: &str,
    rows: &[birds::SpeciesCount],
    date: &str,
) -> Result<String, String> {
    birds::update_in_context(
        &f.state,
        &SearchState::default(),
        1,
        id,
        None,
        token,
        date,
        "11111111-1111-4111-8111-111111111111",
        Some("08:07"),
        None,
        rows,
    )
}
#[test]
fn photo_add_replace_remove_preserve_row_links_comments_original_and_restart() {
    let f = fixture();
    let id = create(&f, true);
    birds::set_link(&f.state, &SearchState::default(), 1, &id, "second", true).unwrap();
    let first = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(first.rows[0], f.row);
    assert_eq!(first.journal_entries, vec!["first", "second"]);
    let mut replacement = first.rows.clone();
    replacement[0].photo = Some(Photo::from_asset(
        crate::photographs::prepare(&f.original, f.row.row_id.clone().unwrap()).unwrap(),
    ));
    replacement[0].photo.as_mut().unwrap().caption =
        first.rows[0].photo.as_ref().unwrap().caption.clone();
    let token = update(
        &f,
        &id,
        &birds::edit_token(&first),
        &replacement,
        &first.visit_date,
    )
    .unwrap();
    let saved = birds::reopen(&f.state, 1, &id).unwrap();
    assert_eq!(saved.rows, replacement);
    assert_eq!(saved.rows[0].row_id, first.rows[0].row_id);
    assert_eq!(saved.journal_entries, first.journal_entries);
    // Invalid content fails after transaction work and rolls the photo replacement back.
    let before = fs::read(&f.path).unwrap();
    let mut invalid = saved.rows.clone();
    invalid[0].photo.as_mut().unwrap().caption = Some("a".repeat(4001));
    assert!(update(&f, &id, &token, &invalid, &first.visit_date).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert!(update(&f, &id, &token, &first.rows, "invalid-date").is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert_eq!(birds::reopen(&f.state, 1, &id).unwrap(), saved);
    assert!(
        update(
            &f,
            &id,
            &birds::edit_token(&first),
            &first.rows,
            &first.visit_date
        )
        .is_err()
    );
    let mut removed = saved.rows.clone();
    removed[0].photo = None;
    let token = update(&f, &id, &token, &removed, &first.visit_date).unwrap();
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    let absent = birds::reopen(&f.state, 1, &id).unwrap();
    assert!(absent.rows[0].photo.is_none());
    assert_eq!(absent.rows[0].row_id, f.row.row_id);
    assert_eq!(absent.rows[0].comment, f.row.comment);
    assert!(
        birds::update_in_context(
            &f.state,
            &SearchState::default(),
            999,
            &id,
            None,
            &token,
            &first.visit_date,
            &first.place_id,
            None,
            None,
            &replacement
        )
        .is_err()
    );
    birds::delete(&f.state, &SearchState::default(), 1, &id, &token, true).unwrap();
    assert!(update(&f, &id, &token, &replacement, &first.visit_date).is_err());
    assert!(!birds::exists(&f.state, 1, &id).unwrap());
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
    let root = f.path.parent().unwrap().to_owned();
    drop(f.state);
    let state = DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    assert!(!birds::exists(&state, 1, &id).unwrap());
}
#[test]
fn photo_archive_candidates_and_protected_activation_preserve_linked_unlinked_and_missing_original()
{
    if std::env::var_os("MNO_BIRD_PHOTO_ACTIVATION").is_none() {
        for mode in ["restore", "import"] {
            let t = tempfile::TempDir::new().unwrap();
            let output=std::process::Command::new(std::env::current_exe().unwrap()).arg("--exact").arg("bird_photos::tests::photo_archive_candidates_and_protected_activation_preserve_linked_unlinked_and_missing_original").env("MNO_BIRD_PHOTO_ACTIVATION",mode).env("MNO_ACTIVATION_ROOT",t.path()).env("MNO_ACTIVATION_MODE","bird-photo").output().unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        return;
    }
    for import in [std::env::var("MNO_BIRD_PHOTO_ACTIVATION").unwrap() == "import"] {
        let mut f = fixture();
        let id = create(&f, true);
        birds::set_link(&f.state, &SearchState::default(), 1, &id, "second", true).unwrap();
        f.row.row_id = Some(uuid::Uuid::new_v4().to_string());
        f.row.photo = Some(Photo::from_asset(
            crate::photographs::prepare(&f.original, f.row.row_id.clone().unwrap()).unwrap(),
        ));
        let unlinked = create(&f, false);
        let original_bytes = fs::read(&f.original).unwrap();
        fs::rename(&f.original, f.temp.path().join("moved-original.png")).unwrap();
        let before = birds::reopen(&f.state, 1, &id).unwrap();
        let second = birds::reopen(&f.state, 1, &unlinked).unwrap();
        let flag = AtomicBool::new(false);
        let mut db =
            Connection::open_with_flags(&f.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let proof = crate::storage_content::verify_v12(&mut db, &flag).unwrap();
        let backup = f.temp.path().join("photos.mno-backup");
        crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
        let export = f.temp.path().join("photos.mno-export");
        crate::open_export::export(&mut db, &export, &flag).unwrap();
        db.close().unwrap();
        let candidate =
            crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
                .unwrap();
        assert_eq!(
            crate::storage_content::verified_snapshot_v12(candidate.source().unwrap(), &flag)
                .unwrap(),
            proof
        );
        drop(candidate);
        let candidate = crate::open_import::build_v12(
            &mut fs::File::open(&export).unwrap(),
            f.temp.path(),
            &flag,
        )
        .unwrap();
        assert_eq!(candidate.proof().sha256, proof.sha256);
        drop(candidate);
        let old = crate::backup::restore_context::activation::journal_proof(&f.path).unwrap();
        let maintenance = f.state.maintenance().unwrap();
        if import {
            drop(
                crate::backup::restore_context::activation::importing::activate_schema11(
                    &maintenance,
                    &mut fs::File::open(&export).unwrap(),
                    &old,
                    &old,
                    Arc::new(AtomicBool::new(false)),
                    12,
                )
                .unwrap(),
            );
        } else {
            drop(
                crate::backup::restore_context::activation::activate(
                    &maintenance,
                    &backup,
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap(),
            );
        }
        drop(maintenance);
        let root = f.path.parent().unwrap().to_owned();
        drop(f.state);
        let restarted = DatabaseState::default();
        assert!(restarted.initialize_startup(&root).is_err());
        crate::backup::restore_context::activation::startup::run(
            &restarted,
            crate::backup::restore_context::activation::startup::Action::FinishNew,
        )
        .unwrap();
        drop(restarted);
        let ready = DatabaseState::default();
        ready.initialize_startup(&root).unwrap();
        assert_eq!(birds::reopen(&ready, 1, &id).unwrap(), before);
        assert_eq!(birds::reopen(&ready, 1, &unlinked).unwrap(), second);
        let protection = fs::read_dir(root.join("backups/pre-restore"))
            .unwrap()
            .find_map(|e| {
                let p = e.unwrap().path();
                (p.extension() == Some(std::ffi::OsStr::new("mno-backup"))).then_some(p)
            })
            .unwrap();
        let protected = crate::backup::restore::prepare_candidate(
            &protection,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(
            crate::storage_content::verified_snapshot_v12(protected.source().unwrap(), &flag)
                .unwrap(),
            proof
        );
        drop(protected);
        assert_eq!(
            fs::read(f.temp.path().join("moved-original.png")).unwrap(),
            original_bytes
        );
    }
}
#[test]
fn malformed_or_cross_owned_photos_are_rejected_atomically() {
    let mut f = fixture();
    let before = fs::read(&f.path).unwrap();
    f.row.photo.as_mut().unwrap().row_id = uuid::Uuid::new_v4().to_string();
    assert!(
        birds::create_with_visit(
            &f.state,
            &SearchState::default(),
            1,
            "2026-11-17",
            "11111111-1111-4111-8111-111111111111",
            &[f.row.clone()],
            None,
            None,
            None
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    f.row.photo.as_mut().unwrap().row_id = f.row.row_id.clone().unwrap();
    f.row.photo.as_mut().unwrap().preview = vec![0, 1, 2];
    assert!(
        birds::create_with_visit(
            &f.state,
            &SearchState::default(),
            1,
            "2026-11-17",
            "11111111-1111-4111-8111-111111111111",
            &[f.row.clone()],
            None,
            None,
            None
        )
        .is_err()
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
}

#[test]
fn photo_creation_receipts_reject_changed_payload_and_replay_after_deletion() {
    let f = fixture();
    let search = SearchState::default();
    let request = birds::reserve(&f.state, &search, 1).unwrap();
    let save = |rows: &[birds::SpeciesCount]| {
        birds::create_with_identity(
            &f.state,
            &search,
            1,
            "2026-11-17",
            "11111111-1111-4111-8111-111111111111",
            rows,
            None,
            None,
            None,
            Some(&request),
        )
    };
    let before = fs::read(&f.path).unwrap();
    let mut invalid = f.row.clone();
    invalid.count = 0;
    assert!(save(&[invalid]).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let id = save(std::slice::from_ref(&f.row)).unwrap();
    let before = fs::read(&f.path).unwrap();
    assert_eq!(save(std::slice::from_ref(&f.row)).unwrap(), id);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let mut changed = f.row.clone();
    changed.photo = Some(Photo::from_asset(
        crate::photographs::prepare(&f.original, f.row.row_id.clone().unwrap()).unwrap(),
    ));
    assert_eq!(save(&[changed]).unwrap_err(), "bird_request_conflict");
    let list = birds::reopen(&f.state, 1, &id).unwrap();
    birds::delete(&f.state, &search, 1, &id, &birds::edit_token(&list), true).unwrap();
    assert_eq!(
        save(std::slice::from_ref(&f.row)).unwrap_err(),
        "bird_request_retired"
    );
    assert!(!birds::exists(&f.state, 1, &id).unwrap());
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
}

#[test]
fn older_photo_archives_normalize_to_captions_without_losing_content_or_receipts() {
    let f = fixture_version(true);
    let id = create(&f, true);
    let flag = AtomicBool::new(false);
    let before = birds::reopen(&f.state, 1, &id).unwrap();
    let mut db =
        Connection::open_with_flags(&f.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let backup = f.temp.path().join("old.mno-backup");
    crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
    let export = f.temp.path().join("old.mno-export");
    crate::open_export::export(&mut db, &export, &flag).unwrap();
    let proof = crate::storage_content::verify_v11(&mut db, &flag).unwrap();
    db.close().unwrap();
    let restored =
        crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v11(restored.source().unwrap(), &flag).unwrap(),
        proof
    );
    drop(restored);
    let candidate = crate::open_import::build_for_schema11(
        &mut fs::File::open(&export).unwrap(),
        f.temp.path(),
        &flag,
        11,
    )
    .unwrap();
    assert_eq!(
        {
            let db = Connection::open_with_flags(
                candidate.path(),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            db.execute_batch("BEGIN").unwrap();
            crate::storage_content::verified_snapshot_v11(&db, &flag).unwrap()
        },
        proof
    );
    drop(candidate);
    let root = f.path.parent().unwrap().to_owned();
    drop(f.state);
    let state = DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    assert_eq!(birds::reopen(&state, 1, &id).unwrap(), before);
    assert_eq!(fs::read(&f.original).unwrap(), f.bytes);
}
