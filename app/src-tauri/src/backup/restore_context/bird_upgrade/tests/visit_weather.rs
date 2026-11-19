use super::*;

fn source(root: &Path) -> (PathBuf, Vec<u8>, Proof) {
    fs::create_dir_all(root).unwrap();
    let (path, _, _) = fixture(root);
    initialize_schema9(&path).unwrap();
    add_lists(&path);
    initialize_schema11(&path).unwrap();
    let mut db = crate::database::open_existing(&path).unwrap();
    let original = root.join("synthetic.png");
    image::RgbImage::from_pixel(3, 2, image::Rgb([30, 60, 90]))
        .save(&original)
        .unwrap();
    let row: String = db
        .query_row("SELECT id FROM bird_rows ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut photo =
        crate::bird_photos::Photo::from_asset(crate::photographs::prepare(&original, row).unwrap());
    photo.caption = Some("Preserved caption".into());
    photo.insert(&db).unwrap();
    let proof = full(&mut db, 11).unwrap();
    close(db);
    let bytes = fs::read(&path).unwrap();
    (path, bytes, proof)
}

#[test]
fn visit_weather_upgrade_preserves_source_and_retries_each_boundary() {
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
        let temp = TempDir::new().unwrap();
        let (path, bytes, proof) = source(temp.path());
        let mut seen = false;
        assert!(
            initialize_to(&path, 12, |point, _| {
                if point == boundary {
                    seen = true;
                    return Err("injected visit-weather upgrade failure".into());
                }
                Ok(())
            })
            .is_err()
        );
        assert!(seen);
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
        initialize_current(&path).unwrap();
        assert_eq!(verify_for(&path, 12, 11).unwrap(), proof);
        let db = crate::database::open_existing(&path).unwrap();
        assert!(crate::bird_visit_weather::read_all(&db).unwrap().is_empty());
        assert!(!crate::database::bird_captions::validate_schema(&db).unwrap());
        close(db);
        assert!(
            fs::read_dir(temp.path().join("backups/bird-schema-upgrade"))
                .unwrap()
                .any(|e| fs::read(e.unwrap().path().join("schema11.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes))
        );
    }
}

#[test]
fn invalid_weather_upgrade_candidate_never_replaces_the_source() {
    let temp = TempDir::new().unwrap();
    let (path, bytes, _) = source(temp.path());
    assert!(
        initialize_to(&path, 12, |point, candidate| {
            if point == "upgraded" {
                let db = Connection::open(candidate).unwrap();
                db.execute(
                    "UPDATE journal_entries SET title='Changed private candidate'",
                    [],
                )
                .unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    initialize_current(&path).unwrap();
}
