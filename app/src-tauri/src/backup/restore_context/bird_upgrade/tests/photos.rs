use super::*;
fn source(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    fs::create_dir_all(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize_schema9(&path).unwrap();
    add_lists(&path);
    let mut db = crate::database::open_existing(&path).unwrap();
    let proof = full(&mut db, 9).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}
#[test]
fn photo_schema_upgrade_preserves_source_and_retries_across_publication_boundaries() {
    for boundary in [
        "copied",
        "upgraded",
        "source-flush",
        "candidate-flush",
        "before-publish",
        "after-publish",
        "publication-flush",
        "verified",
    ] {
        let t = TempDir::new().unwrap();
        let (path, bytes, proof) = source(t.path());
        let mut seen = false;
        let result = initialize_to(&path, 10, |point, _| {
            if point == boundary {
                seen = true;
                return Err("injected photo migration interruption".into());
            }
            Ok(())
        });
        assert!(seen);
        assert!(result.is_err());
        if [
            "copied",
            "upgraded",
            "source-flush",
            "candidate-flush",
            "before-publish",
        ]
        .contains(&boundary)
        {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        initialize_schema10(&path).unwrap();
        assert_eq!(verify_for(&path, 10, 9).unwrap(), proof);
        let mut db = crate::database::open_existing(&path).unwrap();
        full(&mut db, 10).unwrap();
        assert!(!crate::database::bird_requests::validate_schema(&db).unwrap());
        assert!(crate::storage_content::verify_v10(&mut db, &AtomicBool::new(false)).is_err());
        close(db);
        assert!(
            fs::read_dir(t.path().join("backups/bird-schema-upgrade"))
                .unwrap()
                .any(|e| fs::read(e.unwrap().path().join("schema9.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes))
        );
    }
}
#[test]
fn photo_upgrade_rejects_invalid_private_candidate_and_older_exports_normalize_without_content_loss()
 {
    let t = TempDir::new().unwrap();
    let (path, bytes, proof) = source(t.path());
    assert!(
        initialize_to(&path, 10, |point, next| {
            if point == "upgraded" {
                let db = crate::database::open_existing(next).unwrap();
                db.execute_batch("DROP TABLE bird_row_photos").unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let export = t.path().join("schema9.mno-export");
    crate::open_export::export(&mut db, &export, &AtomicBool::new(false)).unwrap();
    close(db);
    let candidate = crate::open_import::build_for_schema10(
        &mut File::open(export).unwrap(),
        t.path(),
        &AtomicBool::new(false),
        10,
    )
    .unwrap();
    let mut db =
        Connection::open_with_flags(candidate.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(full(&mut db, 10).unwrap().sha256, candidate.proof().sha256);
    db.execute_batch("BEGIN").unwrap();
    assert_eq!(preserved(&db, 9).unwrap(), proof);
    db.execute_batch("ROLLBACK").unwrap();
    close(db);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    initialize_schema10(&path).unwrap();
    assert_eq!(verify_for(&path, 10, 9).unwrap(), proof);
}
