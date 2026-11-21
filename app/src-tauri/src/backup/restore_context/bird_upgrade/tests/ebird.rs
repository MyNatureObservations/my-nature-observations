use super::*;
#[test]
fn ebird_protected_upgrade_preserves_schema12_and_retries_publication_boundaries() {
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
        let (path, _, _) = fixture(temp.path());
        initialize_current(&path).unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("INSERT INTO bird_creation_requests VALUES('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',NULL,1)",[]).unwrap();
        let place: String = db
            .query_row("SELECT id FROM places", [], |r| r.get(0))
            .unwrap();
        db.execute("INSERT INTO bird_lists(id,visit_date,place_id) VALUES('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa','2026-11-11',?1)",[place]).unwrap();
        db.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count) VALUES('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb','aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',0,'avibase-test','Synthetic catalog',2)",[]).unwrap();
        close(db);
        let mut db = crate::database::open_existing(&path).unwrap();
        let proof = full(&mut db, 12).unwrap();
        close(db);
        let bytes = fs::read(&path).unwrap();
        let mut seen = false;
        assert!(
            initialize_to(&path, 13, |point, _| {
                if point == boundary {
                    seen = true;
                    return Err("injected eBird upgrade failure".into());
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
        initialize_ebird(&path).unwrap();
        assert_eq!(verify_for(&path, 13, 12).unwrap(), proof);
        assert!(
            fs::read_dir(temp.path().join("backups/bird-schema-upgrade"))
                .unwrap()
                .any(|e| fs::read(e.unwrap().path().join("schema12.sqlite"))
                    .ok()
                    .as_ref()
                    == Some(&bytes))
        );
        let before = fs::read(&path).unwrap();
        initialize_ebird(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn ebird_invalid_upgrade_candidate_preserves_original_and_ordinary_startup_stays12() {
    let temp = TempDir::new().unwrap();
    let (path, _, _) = fixture(temp.path());
    initialize_current(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(
        initialize_to(&path, 13, |point, candidate| {
            if point == "upgraded" {
                let db = Connection::open(candidate).unwrap();
                db.execute("UPDATE journal_entries SET title='tampered'", [])
                    .unwrap();
                close(db);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    initialize_current(&path).unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    assert_eq!(
        db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        12
    );
    close(db);
    initialize_ebird(&path).unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    assert!(!crate::database::bird_visit_weather::validate_schema(&db).unwrap());
    close(db);
}
