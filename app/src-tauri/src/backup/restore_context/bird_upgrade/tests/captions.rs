use super::*;
fn source(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    fs::create_dir_all(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize_schema9(&path).unwrap();
    add_lists(&path);
    initialize_schema10(&path).unwrap();
    let mut db = crate::database::open_existing(&path).unwrap();
    let original = root.join("disposable.png");
    image::RgbImage::from_pixel(3, 2, image::Rgb([30, 60, 90]))
        .save(&original)
        .unwrap();
    let row_id: String = db
        .query_row("SELECT id FROM bird_rows ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    crate::bird_photos::Photo::from_asset(crate::photographs::prepare(&original, row_id).unwrap())
        .insert(&db)
        .unwrap();
    let proof = full(&mut db, 10).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}
#[test]
fn caption_schema_upgrade_preserves_source_and_retries_across_publication_boundaries() {
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
        let result = initialize_to(&path, 11, |point, _| {
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
        initialize_schema11(&path).unwrap();
        assert_eq!(verify_for(&path, 11, 10).unwrap(), proof);
        let mut db = crate::database::open_existing(&path).unwrap();
        full(&mut db, 11).unwrap();
        assert!(!crate::database::bird_requests::validate_schema(&db).unwrap());
        assert!(crate::storage_content::verify_v10(&mut db, &AtomicBool::new(false)).is_err());
        close(db);
        assert!(
            fs::read_dir(t.path().join("backups/bird-schema-upgrade"))
                .unwrap()
                .any(|e| fs::read(e.unwrap().path().join("schema10.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes))
        );
    }
}
