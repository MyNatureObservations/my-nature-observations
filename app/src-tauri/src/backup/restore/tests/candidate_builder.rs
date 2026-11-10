//! C1: existing preparation boundary only; no startup, migration or activation.
use super::*;

#[test]
fn current_backup_candidate_is_complete_closed_reopenable_and_isolated() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let source = dir.path().join("source.sqlite");
    // Private fixture construction, not application startup: deliberately does
    // not bypass or invoke Windows establishment/durability policy.
    database::create_current_database(&source).unwrap();
    let mut db = Connection::open(&source).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('entry','2026-10-25','Synthetic',?1,2,'created','updated')", [DOC]).unwrap();
    let image = dir.path().join("synthetic.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 100, 80, 255]))
        .save(&image)
        .unwrap();
    let asset = crate::photographs::prepare(&image, "entry".into()).unwrap();
    asset.insert(&db).unwrap();
    let document = serde_json::json!({"type":"doc","content":[{"type":"journalImage","attrs":{
        "nodeId":uuid::Uuid::new_v4().to_string(),"assetId":asset.asset_id,
        "placement":"left","size":"small","caption":"Synthetic caption","alt":"Green square"}}]})
    .to_string();
    db.execute(
        "UPDATE journal_entries SET content_json=?1 WHERE id='entry'",
        [&document],
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
    db.execute(
        "INSERT INTO weather_default_place VALUES (1,?1,1)",
        [&place],
    )
    .unwrap();
    let manual = serde_json::json!({"period":{"kind":"day","date":"2026-10-25"},"location":{"name":"Synthetic lake","coordinates":{"latitude":59.33,"longitude":18.07}},"temperatureC":7}).to_string();
    let fetched = serde_json::json!({"period":{"kind":"day","date":"2026-10-25"},"location":{"name":"Synthetic lake","coordinates":{"latitude":59.33,"longitude":18.07}},"timezone":"Europe/Stockholm","retrievedAt":"2026-10-26T08:30:00Z","source":{"provider":"open-meteo","product":"historical-weather","dataKind":"reanalysis","model":"ERA5","attribution":"Weather data by Open-Meteo","license":"CC-BY-4.0","licenseUrl":"https://creativecommons.org/licenses/by/4.0/"},"temperatureMeanC":7}).to_string();
    db.execute(
        "INSERT INTO journal_weather VALUES ('entry',1,?1,?2,1)",
        params![manual, fetched],
    )
    .unwrap();
    let flag = AtomicBool::new(false);
    let expected = crate::storage_content::verify_v5(&mut db, &flag).unwrap();
    let archive = dir.path().join("selected.mno-backup");
    let manifest = create_backup(&db, &archive, BackupKind::Manual).unwrap();
    assert_eq!(
        (manifest.format_version, manifest.database_schema_version),
        (5, 4)
    );
    db.close().unwrap();
    fs::remove_file(image).unwrap(); // Stored images must not depend on originals.
    let source_before = fs::read(&source).unwrap();
    let archive_before = fs::read(&archive).unwrap();
    // An unrelated damaged journal and its sidecars must never be consulted.
    let old = dir.path().join("existing");
    fs::create_dir(&old).unwrap();
    for name in [
        "mno.sqlite",
        "mno.sqlite-wal",
        "mno.sqlite-shm",
        "mno.sqlite-journal",
    ] {
        fs::write(old.join(name), format!("synthetic damaged {name}")).unwrap();
    }
    let old_before: Vec<_> = fs::read_dir(&old)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
    for _ in 0..3 {
        let candidate = prepare(&archive, staging.path()).unwrap();
        let path = candidate.database_path();
        assert_ne!(path, source);
        assert_eq!(
            crate::photographs::get(candidate.source().unwrap(), &asset.asset_id).unwrap(),
            Some(asset.clone())
        );
        let closed = candidate
            .try_close()
            .unwrap_or_else(|_| panic!("candidate close failed"));
        assert_eq!(
            fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1,
            "candidate has sidecars"
        );
        let mut reopened =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            reopened
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            crate::storage_content::verify_v5(&mut reopened, &flag).unwrap(),
            expected
        );
        reopened.close().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
            manifest.database_sha256
        );
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        drop(closed); // Only after the independent connection has also closed.
        assert!(!path.exists());
        assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
        assert_eq!(fs::read(&archive).unwrap(), archive_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        for (path, bytes) in &old_before {
            assert_eq!(&fs::read(path).unwrap(), bytes);
        }
        assert_eq!(fs::read_dir(&old).unwrap().count(), old_before.len());
    }
}

#[test]
fn mid_extraction_disk_and_permission_failures_leave_no_candidate() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let content = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Synthetic ".repeat(20_000)}]}]}).to_string();
    let (archive, source) = fixture(dir.path(), |db| {
        db.execute("UPDATE journal_entries SET content_json=?1", [&content])
            .unwrap();
    });
    let archive_before = fs::read(&archive).unwrap();
    let source_before = fs::read(&source).unwrap();
    for (kind, expected) in [
        (io::ErrorKind::StorageFull, PrepareError::InsufficientSpace),
        (io::ErrorKind::PermissionDenied, PrepareError::Io),
    ] {
        let calls = std::cell::Cell::new(0);
        let result = prepare_in(
            &archive,
            staging.path(),
            Arc::new(AtomicBool::new(false)),
            |_| {
                let call = calls.get();
                calls.set(call + 1);
                // Permit one 64 KiB chunk, then fail with a genuinely partial file.
                if call > 1 {
                    Err(io::Error::from(kind))
                } else {
                    Ok(u64::MAX)
                }
            },
        );
        assert_eq!(error(result), expected);
        assert!(calls.get() > 2);
        assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
        assert_eq!(fs::read(&archive).unwrap(), archive_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
    }
}

#[test]
fn truncated_backup_is_rejected_without_partial_candidate_or_input_changes() {
    let dir = TempDir::new().unwrap();
    let staging = TempDir::new().unwrap();
    let (archive, source) = fixture(dir.path(), |_| {});
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();
    let truncated = fs::read(&archive).unwrap();
    let source_before = fs::read(&source).unwrap();
    assert_eq!(
        error(prepare(&archive, staging.path())),
        PrepareError::DamagedArchive
    );
    assert_eq!(fs::read_dir(staging.path()).unwrap().count(), 0);
    assert_eq!(fs::read(&archive).unwrap(), truncated);
    assert_eq!(fs::read(&source).unwrap(), source_before);
}

#[cfg(windows)]
#[path = "candidate_handoff.rs"]
mod candidate_handoff;
