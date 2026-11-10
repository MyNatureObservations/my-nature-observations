use super::*;
use crate::{
    backup::{self, BackupKind},
    database, storage_content,
};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

fn fixture(dir: &Path, name: &str) -> (std::path::PathBuf, Connection) {
    let path = dir.join(name);
    database::create_current_database(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    (path, db)
}
fn source(dir: &Path) -> Asset {
    let path = dir.join("original.png");
    image::RgbaImage::from_pixel(80, 40, image::Rgba([20, 100, 80, 128]))
        .save(&path)
        .unwrap();
    prepare(&path, "entry".into()).unwrap()
}
fn document(asset: &Asset) -> String {
    serde_json::json!({"type":"doc","content":[{"type":"journalImage","attrs":{
        "nodeId":uuid::Uuid::new_v4().to_string(),"assetId":asset.asset_id,
        "placement":"left","size":"small","caption":"A caption åäö","alt":"A green square"}}]})
    .to_string()
}
fn layout_document(asset: &Asset) -> String {
    let original: serde_json::Value = serde_json::from_str(&document(asset)).unwrap();
    let mut nodes = Vec::new();
    for placement in ["left", "right", "row"] {
        for size in ["small", "medium", "large"] {
            let mut node = original["content"][0].clone();
            node["attrs"]["nodeId"] = uuid::Uuid::new_v4().to_string().into();
            node["attrs"]["placement"] = placement.into();
            node["attrs"]["size"] = size.into();
            nodes.push(node);
        }
    }
    serde_json::json!({"type":"doc","content":nodes}).to_string()
}
const TEXT: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Kept text"}]}]}"#;

#[test]
fn document_v2_is_atomic_strict_and_keeps_node_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let asset = source(dir.path());
    let json = document(&asset);
    assert_eq!(
        validate_document(&json).unwrap(),
        vec![asset.asset_id.clone()]
    );
    assert!(!backup::restore::document_is_editable(&json));
    let original: serde_json::Value = serde_json::from_str(&json).unwrap();
    for (field, value) in [
        ("nodeId", serde_json::json!("bad")),
        ("assetId", serde_json::json!("bad")),
        ("placement", serde_json::json!("center")),
        ("size", serde_json::json!("huge")),
        ("alt", serde_json::json!(null)),
        ("caption", serde_json::json!(4)),
        ("path", serde_json::json!("/tmp/x")),
    ] {
        let mut invalid = original.clone();
        invalid["content"][0]["attrs"][field] = value;
        assert!(validate_document(&invalid.to_string()).is_err(), "{field}");
    }
    let mut invalid = original.clone();
    invalid["content"][0]["content"] = serde_json::json!([]);
    assert!(validate_document(&invalid.to_string()).is_err());
    let mut duplicate = original.clone();
    duplicate["content"]
        .as_array_mut()
        .unwrap()
        .push(original["content"][0].clone());
    assert!(validate_document(&duplicate.to_string()).is_err());
    let mut optional = original.clone();
    optional["content"][0]["attrs"]
        .as_object_mut()
        .unwrap()
        .remove("caption");
    optional["content"][0]["attrs"]
        .as_object_mut()
        .unwrap()
        .remove("alt");
    assert!(validate_document(&optional.to_string()).is_ok());
    let nested = serde_json::json!({"type":"doc","content":[{"type":"blockquote","content":[original["content"][0].clone()]}]});
    assert!(validate_document(&nested.to_string()).is_err());
    assert!(
        validate_document(&json.replace(
            "\"placement\":\"left\"",
            "\"placement\":\"left\",\"placement\":\"right\""
        ))
        .is_err()
    );
}

#[test]
fn atomic_save_retries_unused_assets_missing_original_and_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut db) = fixture(dir.path(), "journal.sqlite");
    let asset = source(dir.path());
    let original = fs::read(&asset.original_path).unwrap();
    let json = document(&asset);
    assert!(save(&mut db, "entry", "2026-10-30", "", &json, &[], true).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    save(&mut db, "entry", "2026-10-30", "", &json, &[asset.clone()], true).unwrap();
    save(&mut db, "entry", "2026-10-30", "", &json, &[asset.clone()], false).unwrap();
    assert_eq!(fs::read(&asset.original_path).unwrap(), original);
    fs::remove_file(&asset.original_path).unwrap();
    assert_eq!(get(&db, &asset.asset_id).unwrap(), Some(asset.clone()));
    save(&mut db, "entry", "2026-10-30", "", TEXT, &[], false).unwrap();
    assert_eq!(get(&db, &asset.asset_id).unwrap(), Some(asset.clone()));
    save(&mut db, "entry", "2026-10-30", "", &json, &[], false).unwrap();
    for placement in ["left", "right", "row"] {
        for size in ["small", "medium", "large"] {
            let mut properties: serde_json::Value = serde_json::from_str(&json).unwrap();
            properties["content"][0]["attrs"]["placement"] = placement.into();
            properties["content"][0]["attrs"]["size"] = size.into();
            let content = properties.to_string();
            save(&mut db, "entry", "2026-10-30", "", &content, &[], false).unwrap();
            let stored: String = db
                .query_row(
                    "SELECT content_json FROM journal_entries WHERE id='entry'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(stored, content);
            assert_eq!(get(&db, &asset.asset_id).unwrap(), Some(asset.clone()));
        }
    }
    let mut changed = asset.clone();
    changed.original_path.push_str("changed");
    assert!(save(&mut db, "entry", "2026-10-30", "", &json, &[changed], false).is_err());
    assert!(save(&mut db, "other", "2026-10-30", "", &json, &[], true).is_err());
    assert!(storage_content::verify_v5(&mut db, &AtomicBool::new(false)).is_ok());
    let proof = storage_content::verify_v5(&mut db, &AtomicBool::new(false)).unwrap();
    db.execute(
        "UPDATE journal_image_assets SET original_path=original_path||'x'",
        [],
    )
    .unwrap();
    assert_ne!(
        proof.sha256,
        storage_content::verify_v5(&mut db, &AtomicBool::new(false))
            .unwrap()
            .sha256
    );
    db.execute("UPDATE journal_image_assets SET preview=x'00'", [])
        .unwrap();
    assert!(storage_content::verify_v5(&mut db, &AtomicBool::new(false)).is_err());
    drop(db);
    assert!(database::initialize_database(&path).is_err());
}

#[test]
fn open_archive_and_backup_round_trip_all_assets_without_originals() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut db) = fixture(dir.path(), "source.sqlite");
    let asset = source(dir.path());
    let json = layout_document(&asset);
    let unused = source(dir.path());
    save(
        &mut db,
        "entry",
        "2026-10-30",
        "",
        &json,
        &[asset.clone(), unused.clone()],
        true,
    )
    .unwrap();
    save(&mut db, "entry", "2026-10-30", "", TEXT, &[], false).unwrap();
    save(&mut db, "entry", "2026-10-30", "", &json, &[], false).unwrap();
    fs::remove_file(&asset.original_path).unwrap();
    let flag = AtomicBool::new(false);
    let before = storage_content::verify_v5(&mut db, &flag).unwrap();
    let archive = dir.path().join("test.mno-backup");
    assert_eq!(
        backup::create_backup(&db, &archive, BackupKind::Manual)
            .unwrap()
            .format_version,
        5
    );
    assert_eq!(
        backup::verify_backup(&archive)
            .unwrap()
            .database_schema_version,
        4
    );
    let candidate =
        backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
    assert_eq!(
        get(candidate.source().unwrap(), &asset.asset_id).unwrap(),
        Some(asset.clone())
    );
    assert_eq!(
        get(candidate.source().unwrap(), &unused.asset_id).unwrap(),
        Some(unused.clone())
    );
    let restored: String = candidate
        .source()
        .unwrap()
        .query_row("SELECT content_json FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(restored, json);
    let p = storage_content::verified_snapshot_v5(candidate.source().unwrap(), &flag).unwrap();
    assert_eq!(p, before);
    drop(candidate);
    drop(db);
    let mut readonly =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let export = dir.path().join("test.mno-export");
    crate::open_export::export(&mut readonly, &export, &flag).unwrap();
    let mut file = fs::File::open(&export).unwrap();
    assert!(crate::open_import::is_v5(&mut file).unwrap());
    let candidate = crate::open_import::build_v5(&mut file, dir.path(), &flag).unwrap();
    assert_eq!(candidate.proof().sha256, before.sha256);
    let imported = Connection::open(candidate.path()).unwrap();
    assert_eq!(get(&imported, &asset.asset_id).unwrap(), Some(asset));
    assert_eq!(get(&imported, &unused.asset_id).unwrap(), Some(unused));
    assert_eq!(
        imported
            .query_row("SELECT content_json FROM journal_entries", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        json
    );
}

#[test]
#[ignore = "Manual synthetic image measurements; run with --ignored --nocapture"]
fn preparation_measurements() {
    let dir = tempfile::tempdir().unwrap();
    for (name, w, h, format, noise) in [
        ("small-png", 640, 480, ImageFormat::Png, false),
        ("landscape-jpeg", 4000, 3000, ImageFormat::Jpeg, false),
        ("large-jpeg", 6000, 4000, ImageFormat::Jpeg, true),
        ("alpha-noise-png", 3200, 2400, ImageFormat::Png, true),
    ] {
        let mut state = 17u32;
        let image = image::RgbaImage::from_fn(w, h, |x, y| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            if noise {
                image::Rgba([
                    (state >> 24) as u8,
                    (state >> 16) as u8,
                    (state >> 8) as u8,
                    if format == ImageFormat::Png {
                        (state & 255) as u8
                    } else {
                        255
                    },
                ])
            } else {
                image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
            }
        });
        let path = dir.path().join(format!(
            "{name}.{}",
            if format == ImageFormat::Png {
                "png"
            } else {
                "jpg"
            }
        ));
        if format == ImageFormat::Png {
            image.save(&path).unwrap();
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                fs::File::create(&path).unwrap(),
                90,
            )
            .encode_image(&DynamicImage::ImageRgba8(image).to_rgb8())
            .unwrap();
        }
        let source_bytes = fs::read(&path).unwrap();
        let started = Instant::now();
        let asset = prepare(&path, "entry".into()).unwrap();
        let elapsed = started.elapsed();
        assert!(
            asset.width <= 2500 && asset.height <= 2500 && asset.width <= w && asset.height <= h
        );
        assert!(
            (f64::from(asset.width) / f64::from(asset.height) - f64::from(w) / f64::from(h)).abs()
                < 0.003
        );
        assert!(asset.preview.len() <= PREVIEW_LIMIT);
        assert_eq!(fs::read(&path).unwrap(), source_bytes);
        asset.validate().unwrap();
        let (_, mut db) = fixture(dir.path(), &format!("{name}.sqlite"));
        let started = Instant::now();
        save(
            &mut db,
            "entry",
            "2026-10-30",
            "",
            &document(&asset),
            &[asset.clone()],
            true,
        )
        .unwrap();
        println!(
            "PHOTO_MEASURE {name}: {w}x{h}, source={} bytes, preview={}x{}, {} bytes, prepare_ms={}, save_ms={}",
            source_bytes.len(),
            asset.width,
            asset.height,
            asset.preview.len(),
            elapsed.as_millis(),
            started.elapsed().as_millis()
        );
    }
}

#[test]
fn preparation_rejects_unsupported_or_corrupt_input_and_never_upscales() {
    let dir = tempfile::tempdir().unwrap();
    let asset = source(dir.path());
    assert_eq!((asset.width, asset.height), (80, 40));
    let path = dir.path().join("broken.jpg");
    fs::write(&path, b"not a jpeg").unwrap();
    assert!(prepare(&path, "entry".into()).is_err());
    assert!(prepare(Path::new("relative.jpg"), "entry".into()).is_err());
    let mut corrupt = asset.clone();
    corrupt.width += 1;
    assert!(corrupt.validate().is_err());
    corrupt = asset.clone();
    corrupt.mime = "image/jpeg".into();
    assert!(corrupt.validate().is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_child() {
    let Ok(root) = std::env::var("MNO_PHOTO_CHILD_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let state = crate::database_state::DatabaseState::default();
    let mode = std::env::var("MNO_PHOTO_CHILD_MODE").unwrap();
    if mode == "restore" {
        let path = state.initialize_startup(&root.join("data")).unwrap();
        let mut db = Connection::open(path).unwrap();
        fs::create_dir(root.join("previous-original")).unwrap();
        let mut asset = source(&root.join("previous-original"));
        fs::remove_file(&asset.original_path).unwrap();
        asset.entry_id = "previous".into();
        save(&mut db, "previous", "2026-10-30", "", TEXT, &[asset], true).unwrap();
        drop(db);
        let maintenance = state.maintenance().unwrap();
        let result = backup::restore_context::activation::activate(
            &maintenance,
            &root.join("incoming.mno-backup"),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(result.is_err());
    } else if mode == "import" {
        let path = state.initialize_startup(&root.join("data")).unwrap();
        let mut db = Connection::open(path).unwrap();
        let old = crate::open_import::verify_database_version(&mut db, &AtomicBool::new(false), 5)
            .unwrap();
        drop(db);
        let mut file = fs::File::open(root.join("incoming.mno-export")).unwrap();
        let candidate =
            crate::open_import::build_v5(&mut file, root, &AtomicBool::new(false)).unwrap();
        let new = candidate.proof().clone();
        drop(candidate);
        let maintenance = state.maintenance().unwrap();
        let result = backup::restore_context::activation::importing::activate_v5(
            &maintenance,
            &mut file,
            &old,
            &new,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(result.is_err());
    } else {
        assert!(state.initialize_startup(&root.join("data")).is_err());
        let action = if mode == "keep" {
            backup::restore_context::activation::startup::Action::KeepPrevious
        } else if mode == "continue" {
            backup::restore_context::activation::startup::Action::Continue
        } else {
            backup::restore_context::activation::startup::Action::FinishNew
        };
        let view = backup::restore_context::activation::startup::status(&state).unwrap();
        assert!(view.actions.contains(&action), "{view:?}");
        backup::restore_context::activation::startup::run(&state, action).unwrap();
        let path = state.ready_database_path().unwrap();
        let mut db = Connection::open(path).unwrap();
        storage_content::verify_v5(&mut db, &AtomicBool::new(false)).unwrap();
        let asset_id = fs::read_to_string(root.join("asset-id")).unwrap();
        if mode != "keep" {
            let content: String = db
                .query_row(
                    "SELECT content_json FROM journal_entries WHERE id='entry'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                content,
                fs::read_to_string(root.join("document.json")).unwrap()
            );
            assert_eq!(
                get(&db, &asset_id).unwrap().unwrap().preview,
                fs::read(root.join("preview")).unwrap()
            );
        } else {
            assert!(
                db.query_row("SELECT count(*) FROM journal_image_assets", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap()
                    > 0
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn interrupted_restore_and_import_preserve_photographs_through_startup_recovery() {
    for (operation, point, action) in [
        ("restore", "before-replace", "continue"),
        ("restore", "after-replace", "finish"),
        ("restore", "before-replace", "keep"),
        ("import", "import-v3-after-marker-publish", "continue"),
    ] {
        let dir = tempfile::tempdir_in("/private/tmp").unwrap();
        let root = dir.path();
        let (path, mut db) = fixture(root, "source.sqlite");
        let asset = source(root);
        let json = layout_document(&asset);
        fs::write(root.join("document.json"), &json).unwrap();
        save(&mut db, "entry", "2026-10-30", "", &json, &[asset.clone()], true).unwrap();
        backup::create_backup(&db, &root.join("incoming.mno-backup"), BackupKind::Manual).unwrap();
        drop(db);
        let mut db =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        crate::open_export::export(
            &mut db,
            &root.join("incoming.mno-export"),
            &AtomicBool::new(false),
        )
        .unwrap();
        drop(db);
        fs::write(root.join("asset-id"), &asset.asset_id).unwrap();
        fs::write(root.join("preview"), &asset.preview).unwrap();
        fs::remove_file(&asset.original_path).unwrap();
        for (mode, fail) in [
            (operation, format!("fail:{point}")),
            (action, String::new()),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "photographs::tests::recovery_child",
                    "--nocapture",
                ])
                .env("MNO_PHOTO_CHILD_ROOT", root)
                .env("MNO_PHOTO_CHILD_MODE", mode)
                .env("MNO_ACTIVATION_ROOT", root)
                .env("MNO_ACTIVATION_MODE", fail)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{operation}/{point}/{mode}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn jpeg_orientation_and_png_alpha_are_preserved_in_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    let png = source(dir.path());
    let decoded = image::load_from_memory(&png.preview).unwrap().to_rgba8();
    assert_eq!(decoded.get_pixel(0, 0).0[3], 128);
    let path = dir.path().join("rotated.jpeg");
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&image::RgbImage::from_pixel(
            80,
            40,
            image::Rgb([30, 100, 10]),
        ))
        .unwrap();
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut oriented = jpeg[..2].to_vec();
    oriented.extend_from_slice(&[0xff, 0xe1]);
    oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    oriented.extend_from_slice(exif);
    oriented.extend_from_slice(&jpeg[2..]);
    fs::write(&path, &oriented).unwrap();
    let asset = prepare(&path, "entry".into()).unwrap();
    assert_eq!((asset.width, asset.height), (40, 80));
    assert_eq!(fs::read(path).unwrap(), oriented);
}

#[test]
fn import_rejects_invalid_image_payloads_even_with_recomputed_checksums() {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let (path, mut db) = fixture(dir.path(), "source.sqlite");
    let asset = source(dir.path());
    save(
        &mut db,
        "entry",
        "2026-10-30",
        "",
        &document(&asset),
        &[asset],
        true,
    )
    .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    let mut db =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let archive = dir.path().join("valid.mno-export");
    crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
    drop(db);
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(entry.name().to_string(), bytes);
    }
    let original: serde_json::Value = serde_json::from_slice(&files["images.jsonl"]).unwrap();
    for case in [
        "corrupt_preview",
        "wrong_dimensions",
        "relative_path",
        "duplicate_asset",
        "missing_asset",
        "foreign_owner",
    ] {
        let mut files = files.clone();
        let mut row = original.clone();
        match case {
            "corrupt_preview" => row["preview"] = serde_json::json!(STANDARD.encode([0, 1, 2])),
            "wrong_dimensions" => row["width"] = serde_json::json!(81),
            "relative_path" => row["original_path"] = serde_json::json!("relative.png"),
            "foreign_owner" => row["entry_id"] = serde_json::json!("other"),
            _ => {}
        }
        let mut bytes = serde_json::to_vec(&row).unwrap();
        bytes.push(b'\n');
        let count = if case == "duplicate_asset" {
            let copy = bytes.clone();
            bytes.extend(copy);
            2
        } else if case == "missing_asset" {
            bytes.clear();
            0
        } else {
            1
        };
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest["files"]["images.jsonl"]["size"] = serde_json::json!(bytes.len());
        manifest["files"]["images.jsonl"]["sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
        manifest["files"]["images.jsonl"]["row_count"] = serde_json::json!(count);
        files.insert("images.jsonl".into(), bytes);
        files.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let path = dir.path().join(format!("{case}.mno-export"));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, bytes) in files {
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        assert!(
            crate::open_import::build_v5(
                &mut fs::File::open(path).unwrap(),
                dir.path(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "{case}"
        );
    }
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn preparation_resizes_without_cropping_and_reduces_oversized_png_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.png");
    image::RgbaImage::from_pixel(3000, 1200, image::Rgba([12, 120, 80, 255]))
        .save(&path)
        .unwrap();
    let asset = prepare(&path, "entry".into()).unwrap();
    assert_eq!((asset.width, asset.height), (2500, 1000));
    assert!(asset.preview.len() <= PREVIEW_LIMIT);
    let path = dir.path().join("noise.png");
    let mut state = 17u32;
    image::RgbaImage::from_fn(1500, 1500, |_, _| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        image::Rgba([
            (state >> 24) as u8,
            (state >> 16) as u8,
            (state >> 8) as u8,
            state as u8,
        ])
    })
    .save(&path)
    .unwrap();
    assert!(fs::metadata(&path).unwrap().len() > PREVIEW_LIMIT as u64);
    let asset = prepare(&path, "entry".into()).unwrap();
    assert!(asset.preview.len() <= PREVIEW_LIMIT);
    assert!(asset.width < 1500);
    assert_eq!(asset.width, asset.height);
    asset.validate().unwrap();
}

#[test]
fn v2_reopens_as_editable_and_keeps_caption_alt_and_unused_assets() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut db) = fixture(dir.path(), "reopen.sqlite");
    let asset = source(dir.path());
    let mut value: serde_json::Value = serde_json::from_str(&document(&asset)).unwrap();
    save(
        &mut db,
        "entry",
        "2026-10-31",
        "",
        &value.to_string(),
        &[asset.clone()],
        true,
    )
    .unwrap();
    fs::remove_file(&asset.original_path).unwrap();
    drop(db);
    let mut db = Connection::open(&path).unwrap();
    let rows = crate::journal::read_journal_entries_with_connection(&db).unwrap();
    let rows = serde_json::to_value(rows).unwrap();
    assert_eq!(rows[0]["editable"], true);
    assert_eq!(rows[0]["documentVersion"], 2);
    let state = crate::database_state::DatabaseState::ready_for_test();
    assert_eq!(
        crate::journal::save_in_session(
            &state,
            1,
            || Connection::open(&path).map_err(|e| e.to_string()),
            crate::journal::EntryTarget::Existing("entry".into()),
            "2026-10-31".into(),
            "".into(),
            TEXT.into()
        )
        .err()
        .as_deref(),
        Some("journal_document_incompatible")
    );
    value["content"][0]["attrs"]["caption"] = serde_json::json!("New caption");
    value["content"][0]["attrs"]["alt"] = serde_json::json!("Independent alternative text");
    save(&mut db, "entry", "2026-10-31", "", &value.to_string(), &[], false).unwrap();
    assert_eq!(
        db.query_row("SELECT content_json FROM journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        value.to_string()
    );
    save(&mut db, "entry", "2026-10-31", "", TEXT, &[], false).unwrap();
    assert_eq!(get(&db, &asset.asset_id).unwrap(), Some(asset));
    db.execute("UPDATE journal_entries SET document_version=3", [])
        .unwrap();
    let rows =
        serde_json::to_value(crate::journal::read_journal_entries_with_connection(&db).unwrap())
            .unwrap();
    assert_eq!(rows[0]["editable"], false);
    assert!(save(&mut db, "entry", "2026-10-31", "", TEXT, &[], false).is_err());
}

#[test]
fn delete_entry_is_atomic_and_late_saves_cannot_recreate_it() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut db) = fixture(dir.path(), "journal.sqlite");
    let asset = source(dir.path());
    let original = fs::read(&asset.original_path).unwrap();
    let json = document(&asset);
    let date = "2026-10-30";
    save(
        &mut db,
        "entry",
        date,
        "",
        &json,
        std::slice::from_ref(&asset),
        true,
    )
    .unwrap();
    save(&mut db, "other", date, "", TEXT, &[], true).unwrap();
    db.execute_batch(
        "INSERT INTO places(id,name,revision) VALUES ('shared','Forest',1);
         INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision)
           VALUES ('entry','shared','Forest',1,1), ('other','shared','Forest',1,1);
         INSERT INTO journal_weather(entry_id,format_version,revision) VALUES ('entry',1,1);
         CREATE TRIGGER reject_delete AFTER DELETE ON journal_entries
           BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END;",
    ).unwrap();
    let count = |db: &Connection, table: &str| {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap()
    };
    assert!(
        crate::journal::delete_entry(&mut db, "entry")
            .unwrap_err()
            .contains("injected delete failure")
    );
    assert_eq!(count(&db, "journal_entries"), 2);
    assert_eq!(count(&db, "journal_weather"), 1);
    assert_eq!(count(&db, "journal_image_assets"), 1);
    assert_eq!(count(&db, "journal_places"), 2);

    db.execute_batch("DROP TRIGGER reject_delete").unwrap();
    crate::journal::delete_entry(&mut db, "entry").unwrap();
    assert_eq!(count(&db, "journal_entries"), 1);
    assert_eq!(count(&db, "journal_weather"), 0);
    assert_eq!(count(&db, "journal_image_assets"), 0);
    assert_eq!(count(&db, "journal_places"), 1);
    assert_eq!(count(&db, "places"), 1);
    assert_eq!(fs::read(&asset.original_path).unwrap(), original);

    let state = crate::database_state::DatabaseState::ready_for_test();
    let late_text = crate::journal::save_in_session(
        &state,
        1,
        || Connection::open(&path).map_err(|error| error.to_string()),
        crate::journal::EntryTarget::Existing("entry".into()),
        date.into(),
        "Late text".into(),
        TEXT.into(),
    );
    assert_eq!(late_text.err().as_deref(), Some("journal_entry_not_found"));
    let late_photo = save(
        &mut db,
        "entry",
        date,
        "",
        &json,
        std::slice::from_ref(&asset),
        false,
    );
    assert_eq!(late_photo.err().as_deref(), Some("journal_entry_not_found"));
    assert_eq!(count(&db, "journal_entries"), 1);
    assert_eq!(count(&db, "journal_image_assets"), 0);
}
