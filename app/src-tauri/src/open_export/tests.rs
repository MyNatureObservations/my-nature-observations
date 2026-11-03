use super::*;
use rusqlite::{OpenFlags, OptionalExtension, params};
use serde_json::{Value, json};
use std::{fs, process::Command};
const DOC: &str = r#"{ "type": "doc", "content": [{"type":"paragraph","content":[{"type":"text","text":"Åäö 🌿","marks":[{"type":"bold"}]}]}] }"#;
struct Fixture {
    dir: tempfile::TempDir,
    path: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sqlite");
        let db = Connection::open(&path).unwrap();
        crate::database::initialize_schema(&db).unwrap();
        db.close().unwrap();
        Self { dir, path }
    }
    fn db(&self) -> Connection {
        Connection::open(&self.path).unwrap()
    }
    fn source(&self) -> Connection {
        Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
    }
    fn destination(&self) -> std::path::PathBuf {
        self.dir.path().join("journal.mno-export")
    }
    fn add(&self, id: &str, document: &str) {
        let db = self.db();
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-10-23','Titel åäö',?2,1,'exact creation text','exact updated text')",params![id,document]).unwrap();
        db.close().unwrap();
    }
}
fn flag() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn schema4_export_import_v5_preserves_weather_default_states() {
    for setting in ["absent", "active", "tombstone"] {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("schema3.sqlite");
        crate::database::create_current_database(&source_path).unwrap();
        let mut source = Connection::open(&source_path).unwrap();
        source.execute("INSERT INTO journal_entries VALUES('entry','2026-10-23','Title',?1,1,'created','updated')", [DOC]).unwrap();
        source.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES('00000000-0000-4000-8000-000000000001','Current place',59.3,18.1,2)",[]).unwrap();
        source.execute("INSERT INTO journal_places VALUES('entry','00000000-0000-4000-8000-000000000001','Historical place',59.0,18.0,1,5)",[]).unwrap();
        source
            .execute(
                "INSERT INTO journal_weather VALUES('entry',1,?1,NULL,4)",
                [r#"{"period":{"kind":"day","date":"2026-10-23"},"temperatureC":0}"#],
            )
            .unwrap();
        if setting == "active" {
            source.execute("INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,'00000000-0000-4000-8000-000000000001',7)",[]).unwrap();
        } else if setting == "tombstone" {
            source.execute("INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,NULL,9)",[]).unwrap();
        }
        let before = crate::open_import::verify_database_version(&mut source, &flag(), 5).unwrap();
        source.close().unwrap();
        let destination = dir.path().join(format!("{setting}.mno-export"));
        let mut readonly =
            Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let manifest = export(&mut readonly, &destination, &flag()).unwrap();
        assert_eq!(
            (manifest.format_version, manifest.database_schema_version),
            (5, 4)
        );
        let files = unpack(&destination);
        assert_eq!(files.len(), 8);
        let wire: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        assert_eq!(
            wire["files"]["weather_default_place.jsonl"]["row_count"],
            if setting == "absent" {
                json!(0)
            } else {
                json!(1)
            }
        );
        let candidate = crate::open_import::build_v5(
            &mut File::open(&destination).unwrap(),
            dir.path(),
            &flag(),
        )
        .unwrap();
        assert_eq!(candidate.proof(), &before);
        let db = Connection::open(candidate.path()).unwrap();
        let restored: Option<(Option<String>, i64)> = db
            .query_row(
                "SELECT place_id,revision FROM weather_default_place WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap();
        assert_eq!(
            restored,
            match setting {
                "absent" => None,
                "active" => Some((Some("00000000-0000-4000-8000-000000000001".into()), 7)),
                _ => Some((None, 9)),
            }
        );
        assert_eq!(
            db.query_row(
                "SELECT place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision FROM journal_places WHERE entry_id='entry'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)),
            )
            .unwrap(),
            ("Historical place".into(), 59.0, 18.0, 1, 5)
        );
        assert_eq!(
            db.query_row(
                "SELECT manual_json,fetched_json,revision FROM journal_weather WHERE entry_id='entry'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
            )
            .unwrap(),
            (
                r#"{"period":{"kind":"day","date":"2026-10-23"},"temperatureC":0}"#.into(),
                None,
                4,
            )
        );
    }
}
fn unpack(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        let mut b = Vec::new();
        e.read_to_end(&mut b).unwrap();
        assert!(out.insert(e.name().to_owned(), b).is_none());
    }
    out
}
fn pack_files(path: &Path, files: &BTreeMap<String, Vec<u8>>) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}
fn records(files: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    std::str::from_utf8(&files["journal.jsonl"])
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn format5_import_rejects_missing_duplicate_and_dangling_weather_default_data() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("schema3.sqlite");
    crate::database::create_current_database(&source_path).unwrap();
    let source = Connection::open(&source_path).unwrap();
    source.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES('00000000-0000-4000-8000-000000000001','Weather place',59.3,18.1,1)",[]).unwrap();
    source.execute("INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(1,'00000000-0000-4000-8000-000000000001',2)",[]).unwrap();
    source.close().unwrap();
    let valid = dir.path().join("valid.mno-export");
    let mut readonly =
        Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    export(&mut readonly, &valid, &flag()).unwrap();
    let original = unpack(&valid);
    for case in ["missing", "duplicate", "dangling", "checksum", "row_count"] {
        let mut files = original.clone();
        match case {
            "missing" => {
                files.remove("weather_default_place.jsonl");
            }
            "duplicate" => {
                let row = files["weather_default_place.jsonl"].clone();
                files
                    .get_mut("weather_default_place.jsonl")
                    .unwrap()
                    .extend(row);
            }
            "dangling" => {
                files.insert("places.jsonl".into(), Vec::new());
            }
            "checksum" => files
                .get_mut("weather_default_place.jsonl")
                .unwrap()
                .push(b' '),
            "row_count" => (),
            _ => unreachable!(),
        }
        if !matches!(case, "missing" | "checksum") {
            let name = if case == "dangling" {
                "places.jsonl"
            } else {
                "weather_default_place.jsonl"
            };
            let mut manifest: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
            manifest["files"][name]["size"] = json!(files[name].len());
            manifest["files"][name]["sha256"] =
                json!(format!("{:x}", Sha256::digest(&files[name])));
            manifest["files"][name]["row_count"] = json!(if case == "duplicate" {
                2
            } else if case == "dangling" {
                0
            } else {
                2
            });
            files.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
        }
        let altered = dir.path().join(format!("{case}.mno-export"));
        pack_files(&altered, &files);
        assert!(
            crate::open_import::build_v5(&mut File::open(altered).unwrap(), dir.path(), &flag())
                .is_err(),
            "{case}"
        );
    }
}
#[test]
fn schema2_export_import_preserves_places_links_tombstones_weather_and_proof() {
    let f = Fixture::new();
    let mut db = f.db();
    crate::database::weather_schema::migrate(&mut db).unwrap();
    crate::database::place_schema::migrate_candidate(&mut db).unwrap();
    for id in ["a", "b", "c"] {
        f.add(id, DOC);
    }
    let p1 = "00000000-0000-4000-8000-000000000001";
    let p2 = "00000000-0000-4000-8000-000000000002";
    db.execute("INSERT INTO places VALUES(?1,'Same',59.5,18.25,3)", [p1])
        .unwrap();
    db.execute("INSERT INTO places VALUES(?1,'Same',NULL,NULL,1)", [p2])
        .unwrap();
    for entry in ["a", "b"] {
        db.execute(
            "INSERT INTO journal_places VALUES(?1,?2,'Historical',59.0,18.0,2,5)",
            params![entry, p1],
        )
        .unwrap();
    }
    db.execute(
        "INSERT INTO journal_places VALUES('c',NULL,NULL,NULL,NULL,NULL,7)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO journal_weather VALUES('a',1,?1,NULL,4)",
        [r#"{"period":{"kind":"day","date":"2026-10-23"},"temperatureC":0}"#],
    )
    .unwrap();
    let before = crate::open_import::verify_database_version(&mut db, &flag(), 3).unwrap();
    db.close().unwrap();
    let manifest = export(&mut f.source(), &f.destination(), &flag()).unwrap();
    assert_eq!(
        (
            manifest.format_version,
            manifest.database_schema_version,
            manifest.entry_count
        ),
        (3, 2, 3)
    );
    let files = unpack(&f.destination());
    assert_eq!(files.len(), 6);
    let wire: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(wire["files"]["places.jsonl"]["row_count"], 2);
    assert_eq!(wire["files"]["journal_places.jsonl"]["row_count"], 3);
    let candidate = crate::open_import::build_v3(
        &mut File::open(f.destination()).unwrap(),
        f.dir.path(),
        &flag(),
    )
    .unwrap();
    assert_eq!(candidate.proof(), &before);
    let mut imported = Connection::open(candidate.path()).unwrap();
    assert_eq!(
        crate::open_import::verify_database_version(&mut imported, &flag(), 3).unwrap(),
        before
    );
    assert_eq!(
        imported
            .query_row("SELECT COUNT(*) FROM places WHERE name='Same'", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        2
    );
    assert_eq!(
        imported
            .query_row(
                "SELECT revision FROM journal_places WHERE entry_id='c'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        7
    );
}
#[test]
fn format3_empty_places_roundtrip() {
    let f = Fixture::new();
    let mut db = f.db();
    crate::database::weather_schema::migrate(&mut db).unwrap();
    crate::database::place_schema::migrate_candidate(&mut db).unwrap();
    f.add("a", DOC);
    let proof = crate::open_import::verify_database_version(&mut db, &flag(), 3).unwrap();
    db.close().unwrap();
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let files = unpack(&f.destination());
    assert!(files["places.jsonl"].is_empty());
    assert!(files["journal_places.jsonl"].is_empty());
    let candidate = crate::open_import::build_v3(
        &mut File::open(f.destination()).unwrap(),
        f.dir.path(),
        &flag(),
    )
    .unwrap();
    assert_eq!(candidate.proof(), &proof);
}

#[test]
fn format3_place_registry_without_journals_is_preserved_as_content() {
    let f = Fixture::new();
    let mut db = f.db();
    crate::database::weather_schema::migrate(&mut db).unwrap();
    crate::database::place_schema::migrate_candidate(&mut db).unwrap();
    db.execute("INSERT INTO places(id,name,latitude,longitude,revision) VALUES('00000000-0000-4000-8000-000000000001','Only place',59.0,18.0,1)", []).unwrap();
    let proof = crate::open_import::verify_database_version(&mut db, &flag(), 3).unwrap();
    assert_eq!(proof.entry_count, 0);
    db.close().unwrap();
    let manifest = export(&mut f.source(), &f.destination(), &flag()).unwrap();
    assert_eq!((manifest.format_version, manifest.entry_count), (3, 0));
    let files = unpack(&f.destination());
    let wire: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(wire["files"]["places.jsonl"]["row_count"], 1);
    let candidate = crate::open_import::build_v3(
        &mut File::open(f.destination()).unwrap(),
        f.dir.path(),
        &flag(),
    )
    .unwrap();
    assert_eq!(candidate.proof(), &proof);
    let imported = Connection::open(candidate.path()).unwrap();
    assert_eq!(
        imported
            .query_row("SELECT name FROM places", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "Only place"
    );
}
#[test]
fn format3_import_rejects_tampering_and_invalid_place_references() {
    let f = Fixture::new();
    let mut db = f.db();
    crate::database::weather_schema::migrate(&mut db).unwrap();
    crate::database::place_schema::migrate_candidate(&mut db).unwrap();
    f.add("a", DOC);
    db.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Place',1)", []).unwrap();
    db.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES('a','00000000-0000-4000-8000-000000000001','Place',1,1)", []).unwrap();
    db.close().unwrap();
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let original = unpack(&f.destination());
    for case in [
        "checksum",
        "missing_place",
        "bad_coordinate",
        "bad_tombstone",
    ] {
        let mut files = original.clone();
        let altered = f.dir.path().join(format!("{case}.mno-export"));
        match case {
            "checksum" => files.get_mut("places.jsonl").unwrap().push(b' '),
            "missing_place" => files
                .insert("places.jsonl".into(), Vec::new())
                .map(|_| ())
                .unwrap(),
            "bad_coordinate" => {
                let mut place: Value =
                    serde_json::from_slice(files["places.jsonl"].trim_ascii()).unwrap();
                place["latitude"] = json!(200.0);
                files.insert("places.jsonl".into(), format!("{}\n", place).into_bytes());
            }
            "bad_tombstone" => {
                let mut link: Value =
                    serde_json::from_slice(files["journal_places.jsonl"].trim_ascii()).unwrap();
                link["place_id"] = Value::Null;
                files.insert(
                    "journal_places.jsonl".into(),
                    format!("{}\n", link).into_bytes(),
                );
            }
            _ => unreachable!(),
        }
        if case != "checksum" {
            let name = if case == "bad_tombstone" {
                "journal_places.jsonl"
            } else {
                "places.jsonl"
            };
            let mut manifest: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
            manifest["files"][name]["size"] = json!(files[name].len());
            manifest["files"][name]["sha256"] =
                json!(format!("{:x}", Sha256::digest(&files[name])));
            manifest["files"][name]["row_count"] =
                json!(if case == "missing_place" { 0 } else { 1 });
            files.insert(
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            );
        }
        let output = File::create(&altered).unwrap();
        let mut zip = zip::ZipWriter::new(output);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            zip.start_file(name, options).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        assert!(
            crate::open_import::build_v3(&mut File::open(altered).unwrap(), f.dir.path(), &flag())
                .is_err(),
            "{case}"
        );
    }
}
#[test]
fn empty_export_has_exact_four_files_and_verified_checksums() {
    let f = Fixture::new();
    let m = export(&mut f.source(), &f.destination(), &flag()).unwrap();
    assert_eq!(m.entry_count, 0);
    let files = unpack(&f.destination());
    assert_eq!(
        files.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["README.md", "journal.jsonl", "journal.md", "manifest.json"]
    );
    assert!(files["journal.jsonl"].is_empty());
    let manifest: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
    assert_eq!(manifest["format"], "mno-export");
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["database_schema_version"], 0);
    assert_eq!(manifest["document_version"], 1);
    assert_eq!(manifest["external_originals_included"], false);
    for name in ["README.md", "journal.jsonl", "journal.md"] {
        assert_eq!(manifest["files"][name]["size"], files[name].len());
        assert_eq!(
            manifest["files"][name]["sha256"],
            format!("{:x}", Sha256::digest(&files[name]))
        );
    }
    assert!(String::from_utf8_lossy(&files["README.md"]).contains("NOT encrypted"));
}
#[test]
fn exact_seven_fields_raw_json_unicode_empty_legacy_rows_and_binary_order() {
    let f = Fixture::new();
    for (id, doc) in [
        ("z", DOC),
        ("a", "{}"),
        ("å", "{\"text\":\"legacy\"}"),
        (
            "empty",
            r#"{"type":"doc","content":[{"type":"paragraph"}]}"#,
        ),
    ] {
        f.add(id, doc);
    }
    let before = fs::read(&f.path).unwrap();
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let files = unpack(&f.destination());
    let rows = records(&files);
    assert_eq!(
        rows.iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["a", "empty", "z", "å"]
    );
    assert_eq!(
        rows[2],
        json!({"id":"z","entry_date":"2026-10-23","title":"Titel åäö","content_json":DOC,"document_version":1,"created_at":"exact creation text","updated_at":"exact updated text"})
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert!(!f.path.with_extension("sqlite-journal").exists());
    let md = String::from_utf8_lossy(&files["journal.md"]);
    assert!(md.contains("**Åäö 🌿**"));
    assert!(md.contains("Original-JSON"));
    assert!(md.contains("{\"text\":\"legacy\"}"));
}
#[test]
fn unknown_nodes_images_and_duplicate_keys_use_exact_fallback() {
    let f = Fixture::new();
    let docs = [
        r#"{"type":"doc","content":[{"type":"future","secret":"keep"}]}"#,
        r#"{"type":"doc","content":[{"type":"image","attrs":{"src":"/not/opened/photo.jpg"}}]}"#,
        r#"{"type":"doc","type":"other","text":"```\n~~~"}"#,
    ];
    for (i, d) in docs.iter().enumerate() {
        f.add(&i.to_string(), d)
    }
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let files = unpack(&f.destination());
    let rows = records(&files);
    let md = String::from_utf8_lossy(&files["journal.md"]);
    for (i, d) in docs.iter().enumerate() {
        assert_eq!(rows[i]["content_json"], *d);
        assert!(md.contains(d));
    }
    assert_eq!(md.matches("fallback/reservrepresentation").count(), 3);
}
#[test]
fn formatted_blocks_are_readable_and_unknown_attributes_fall_back_whole() {
    let f = Fixture::new();
    let d = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Heading"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"Item","marks":[{"type":"italic"}]}]}]}]},{"type":"blockquote","content":[{"type":"paragraph","content":[{"type":"text","text":"Quote"}]}]},{"type":"codeBlock","content":[{"type":"text","text":"code"}]},{"type":"horizontalRule"}]}"#;
    f.add("a", d);
    let unknown =
        r#"{"type":"doc","custom":"must not disappear","content":[{"type":"paragraph"}]}"#;
    f.add("b", unknown);
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let files = unpack(&f.destination());
    let md = String::from_utf8_lossy(&files["journal.md"]);
    for part in [
        "## Heading",
        "- ",
        "*Item*",
        "> Quote",
        "```\ncode",
        "---",
        unknown,
    ] {
        assert!(md.contains(part), "{part}")
    }
}
#[test]
fn large_valid_document_uses_fallback_without_normalization() {
    let f = Fixture::new();
    let d = format!("{{\"text\":\"{}\"}}", "å".repeat(600_000));
    f.add("a", &d);
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let files = unpack(&f.destination());
    assert_eq!(records(&files)[0]["content_json"], d);
    assert!(String::from_utf8_lossy(&files["journal.md"]).contains(&d));
}
#[test]
fn corrupt_nonobject_and_trailing_json_never_publish() {
    for d in ["{", "[]", "null", "{} {}"] {
        let f = Fixture::new();
        f.add("a", d);
        assert!(matches!(
            export(&mut f.source(), &f.destination(), &flag()),
            Err(Error::InvalidJson)
        ));
        assert!(!f.destination().exists());
    }
}
#[test]
fn storage_types_and_document_versions_are_not_coerced() {
    for update in [
        "content_json=x'7b7d'",
        "id=NULL",
        "title=x'61'",
        "document_version=2",
        "document_version=1.5",
        "content_json=CAST(x'ff' AS TEXT)",
    ] {
        let f = Fixture::new();
        f.add("a", DOC);
        f.db()
            .execute_batch(&format!("UPDATE journal_entries SET {update}"))
            .unwrap();
        assert!(matches!(
            export(&mut f.source(), &f.destination(), &flag()),
            Err(Error::InvalidStorage)
        ));
        assert!(!f.destination().exists());
    }
}
#[test]
fn schema_one_and_extra_objects_are_rejected() {
    for sql in ["PRAGMA user_version=1", "CREATE TABLE extra(x)"] {
        let f = Fixture::new();
        f.db().execute_batch(sql).unwrap();
        assert!(matches!(
            export(&mut f.source(), &f.destination(), &flag()),
            Err(Error::IncompatibleSchema)
        ));
        assert!(!f.destination().exists());
    }
}
#[test]
fn resource_limits_are_explicit_and_count_utf8_bytes() {
    let d = format!("{{\"text\":\"{}\"}}", "x".repeat(CONTENT_LIMIT - 11));
    assert_eq!(d.len(), CONTENT_LIMIT);
    assert!(!validate_json(&d).unwrap());
    assert!(matches!(
        validate_json(&(d + " ")),
        Err(Error::ResourceLimit)
    ));
    let d = format!(
        "{{\"text\":{}0{}}}",
        "[".repeat(DEPTH_LIMIT + 1),
        "]".repeat(DEPTH_LIMIT + 1)
    );
    assert!(matches!(validate_json(&d), Err(Error::ResourceLimit)));
    let f = Fixture::new();
    f.add("a", DOC);
    let title = "å".repeat(METADATA_LIMIT / 2 + 1);
    f.db()
        .execute("UPDATE journal_entries SET title=?1", [title])
        .unwrap();
    assert!(matches!(
        export(&mut f.source(), &f.destination(), &flag()),
        Err(Error::ResourceLimit)
    ));
    assert!(!f.destination().exists());
}
#[test]
fn value_count_limit_stops_before_unbounded_tree_construction() {
    let d = format!("{{\"a\":[{}0]}}", "0,".repeat(VALUE_LIMIT));
    assert!(matches!(validate_json(&d), Err(Error::ResourceLimit)));
}
#[test]
fn writable_or_already_transactional_sources_and_wrong_extension_are_rejected() {
    let f = Fixture::new();
    assert!(matches!(
        export(&mut f.db(), &f.destination(), &flag()),
        Err(Error::SourceContract)
    ));
    let mut source = f.source();
    source.execute_batch("BEGIN").unwrap();
    assert!(matches!(
        export(&mut source, &f.destination(), &flag()),
        Err(Error::SourceContract)
    ));
    source.execute_batch("ROLLBACK").unwrap();
    assert!(matches!(
        export(&mut source, &f.dir.path().join("bad.zip"), &flag()),
        Err(Error::InvalidDestination)
    ));
}
#[test]
fn wal_changes_after_snapshot_cannot_mix_rows_or_views() {
    let f = Fixture::new();
    f.add("a", DOC);
    let writer = f.db();
    writer.execute_batch("PRAGMA journal_mode=WAL").unwrap();
    let mut source = f.source();
    export_with(&mut source,&f.destination(),&flag(),|at|{if at=="snapshot"{writer.execute("UPDATE journal_entries SET title='later'",[])?;writer.execute("INSERT INTO journal_entries SELECT 'b',entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries WHERE id='a'",[])?;}Ok(())}).unwrap();
    let files = unpack(&f.destination());
    let rows = records(&files);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "Titel åäö");
    assert!(!String::from_utf8_lossy(&files["journal.md"]).contains("later"));
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn cancellation_and_injected_io_failure_preserve_source_and_do_not_publish() {
    for at in ["snapshot", "row", "before-publish"] {
        for cancel in [false, true] {
            let f = Fixture::new();
            f.add("a", DOC);
            let before = fs::read(&f.path).unwrap();
            let c = flag();
            let mut source = f.source();
            let result = export_with(&mut source, &f.destination(), &c, |point| {
                if point == at {
                    if cancel {
                        c.store(true, Ordering::Release)
                    } else {
                        return Err(io::Error::other("simulated disk failure").into());
                    }
                }
                Ok(())
            });
            assert!(result.is_err());
            assert!(!f.destination().exists());
            assert_eq!(fs::read(&f.path).unwrap(), before);
            assert!(source.is_autocommit());
            assert_eq!(
                fs::read_dir(f.dir.path()).unwrap().count(),
                1,
                "temporary files cleaned"
            );
        }
    }
}
#[test]
fn publication_is_noclobber_including_late_destination_collision() {
    for late in [false, true] {
        let f = Fixture::new();
        if !late {
            fs::write(f.destination(), b"keep").unwrap();
        }
        assert!(
            export_with(&mut f.source(), &f.destination(), &flag(), |at| {
                if late && at == "before-publish" {
                    fs::write(f.destination(), b"keep")?;
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(f.destination()).unwrap(), b"keep");
    }
}
#[test]
fn output_is_reproducible_for_unchanged_data() {
    let f = Fixture::new();
    f.add("a", DOC);
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let second = f.dir.path().join("second.mno-export");
    export(&mut f.source(), &second, &flag()).unwrap();
    assert_eq!(unpack(&f.destination()), unpack(&second));
}
#[test]
fn export_crash_child() {
    let Ok(at) = std::env::var("MNO_EXPORT_CRASH_AT") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("MNO_EXPORT_CRASH_ROOT").unwrap());
    let mut source =
        Connection::open_with_flags(root.join("source.sqlite"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    export_with(
        &mut source,
        &root.join("journal.mno-export"),
        &flag(),
        |p| {
            if p == at {
                std::process::exit(91)
            }
            Ok(())
        },
    )
    .unwrap();
    panic!("checkpoint not reached");
}
#[test]
fn process_interruption_never_exposes_a_partial_final_archive() {
    for at in ["row", "archive-block", "before-publish", "published"] {
        let f = Fixture::new();
        f.add("a", DOC);
        let before = fs::read(&f.path).unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "open_export::tests::export_crash_child",
                "--nocapture",
            ])
            .env("MNO_EXPORT_CRASH_AT", at)
            .env("MNO_EXPORT_CRASH_ROOT", f.dir.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(91),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if at == "published" {
            let files = unpack(&f.destination());
            assert_eq!(records(&files).len(), 1);
        } else {
            assert!(!f.destination().exists());
        }
        assert_eq!(fs::read(&f.path).unwrap(), before);
    }
}

#[test]
fn busy_and_physically_invalid_sources_never_publish() {
    let f = Fixture::new();
    f.add("a", DOC);
    let mut source = f.source();
    source.busy_timeout(std::time::Duration::ZERO).unwrap();
    let lock = f.db();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(export(&mut source, &f.destination(), &flag()).is_err());
    assert!(!f.destination().exists());
    lock.execute_batch("ROLLBACK").unwrap();
    drop(source);
    drop(lock);
    fs::write(&f.path, b"not SQLite").unwrap();
    assert!(export(&mut f.source(), &f.destination(), &flag()).is_err());
    assert!(!f.destination().exists());
}
#[test]
fn completed_archive_remains_valid_on_a_late_reported_error() {
    let f = Fixture::new();
    f.add("a", DOC);
    assert!(
        export_with(&mut f.source(), &f.destination(), &flag(), |point| {
            if point == "published" {
                Err(io::Error::other("simulated late failure").into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(records(&unpack(&f.destination())).len(), 1);
}
#[test]
fn exact_metadata_boundary_and_verbatim_noncanonical_metadata() {
    let f = Fixture::new();
    f.add("", DOC);
    let title = "å".repeat(METADATA_LIMIT / 2);
    f.db()
        .execute(
            "UPDATE journal_entries SET title=?1,entry_date='legacy date'",
            [&title],
        )
        .unwrap();
    export(&mut f.source(), &f.destination(), &flag()).unwrap();
    let rows = records(&unpack(&f.destination()));
    assert_eq!(rows[0]["id"], "");
    assert_eq!(rows[0]["entry_date"], "legacy date");
    assert_eq!(rows[0]["title"], title);
}

// Parse actual CommonMark events, not substrings that could be inside a code block.
fn markdown_structure(md: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
    let (mut headings, mut blocks, mut inline) = (Vec::new(), Vec::new(), Vec::new());
    let mut heading: Option<String> = None;
    let mut block: Option<String> = None;
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => heading = Some(String::new()),
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => headings.push(heading.take().unwrap()),
            Event::Start(Tag::CodeBlock(_)) => block = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => blocks.push(block.take().unwrap()),
            Event::Text(text) => {
                if let Some(s) = block.as_mut() {
                    s.push_str(&text);
                }
                if let Some(s) = heading.as_mut() {
                    s.push_str(&text);
                }
            }
            Event::Code(text) => inline.push(text.into_string()),
            _ => {}
        }
    }
    (headings, blocks, inline)
}

#[test]
fn multiline_inline_code_uses_closed_original_json_fallback_before_next_entry() {
    for text in [
        "first\nsecond",
        "first\rsecond",
        "first\r\nsecond",
        "first\n``` and ``````\nsecond",
    ] {
        let f = Fixture::new();
        // Pretty JSON adds real line breaks outside the encoded string too.
        let document = serde_json::to_string_pretty(&json!({
            "type":"doc", "content":[{"type":"paragraph","content":[
                {"type":"text","text":text,"marks":[{"type":"code"}]}
            ]}]
        }))
        .unwrap();
        f.add("a", &document);
        f.add("b", DOC);
        export(&mut f.source(), &f.destination(), &flag()).unwrap();
        let files = unpack(&f.destination());
        let md = std::str::from_utf8(&files["journal.md"]).unwrap();
        let (headings, blocks, inline) = markdown_structure(md);
        assert_eq!(
            headings,
            vec!["2026-10-23 — Titel åäö"; 2],
            "next entry must be a real heading: {text:?}"
        );
        assert_eq!(
            blocks,
            vec![format!("{document}\n")],
            "fallback must contain exactly the original JSON"
        );
        assert!(inline.is_empty());
        assert!(md.contains("Original-JSON (fallback/reservrepresentation"));
        let rows = records(&files);
        assert_eq!(rows[0]["content_json"], document);
        assert_eq!(rows[1]["content_json"], DOC);
    }
}

#[test]
fn ordinary_inline_code_stays_inline_and_keeps_the_next_entry_separate() {
    for text in ["ordinary code", "one ` backtick", "one `` two"] {
        let f = Fixture::new();
        let document = json!({"type":"doc", "content":[{"type":"paragraph","content":[
            {"type":"text","text":text,"marks":[{"type":"code"}]}
        ]}]})
        .to_string();
        f.add("a", &document);
        f.add("b", DOC);
        export(&mut f.source(), &f.destination(), &flag()).unwrap();
        let files = unpack(&f.destination());
        let md = std::str::from_utf8(&files["journal.md"]).unwrap();
        let (headings, blocks, inline) = markdown_structure(md);
        assert_eq!(headings.len(), 2);
        assert!(blocks.is_empty());
        assert_eq!(inline, vec![text]);
        assert!(!md.contains("fallback/reservrepresentation"));
        assert_eq!(records(&files)[0]["content_json"], document);
    }
}

#[test]
fn markdown_parser_negative_control_detects_the_original_cross_entry_code_block() {
    let broken =
        "## First entry\n\n``` first\nsecond ```\n\n## Second entry\n\nVisible second entry\n";
    let (headings, blocks, _) = markdown_structure(broken);
    assert_eq!(headings, vec!["First entry"]);
    assert_eq!(blocks.len(), 1);
    assert!(
        blocks[0].contains("## Second entry"),
        "the old defect must fail the separate-heading assertion"
    );
}
