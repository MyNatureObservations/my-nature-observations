use super::*;
use serde_json::{Value, json};
use std::{fs, process::Command};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const LEGACY: &str =
    "{ \"text\": \"Original Åäö 🌿\", \"unknown\": [1, null], \"text\": \"duplicate\" }";
fn flag() -> AtomicBool {
    AtomicBool::new(false)
}
struct Fixture {
    root: tempfile::TempDir,
    archive: PathBuf,
    active: PathBuf,
}
impl Fixture {
    fn new(documents: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.sqlite");
        crate::database::create_legacy_database(&source).unwrap();
        let db = Connection::open(&source).unwrap();
        for (id, doc) in documents {
            db.execute("INSERT INTO journal_entries VALUES (?1,'legacy date','Titel åäö',?2,1,'created exactly','updated exactly')", params![id,doc]).unwrap();
        }
        db.close().unwrap();
        let archive = root.path().join("journal.mno-export");
        let mut db =
            Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        crate::open_export::export(&mut db, &archive, &flag()).unwrap();
        db.close().unwrap();
        // Import must work with the source installation completely absent.
        fs::remove_file(&source).unwrap();
        let active = root.path().join("mno.sqlite");
        crate::database::create_legacy_database(&active).unwrap();
        let db = Connection::open(&active).unwrap();
        db.execute("INSERT INTO journal_entries VALUES ('keep','2026-10-24','ACTIVE','{}',1,'before','before')", []).unwrap();
        db.close().unwrap();
        Self {
            root,
            archive,
            active,
        }
    }
    fn build(&self) -> Result<Candidate> {
        build(
            &mut File::open(&self.archive).unwrap(),
            self.root.path(),
            &flag(),
        )
    }
    fn files(&self) -> BTreeMap<String, Vec<u8>> {
        let mut zip = ZipArchive::new(File::open(&self.archive).unwrap()).unwrap();
        let mut files = BTreeMap::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).unwrap();
            let mut b = Vec::new();
            entry.read_to_end(&mut b).unwrap();
            files.insert(entry.name().to_owned(), b);
        }
        files
    }
    fn rewrite(&self, change: impl FnOnce(&mut BTreeMap<String, Vec<u8>>), update_hashes: bool) {
        let mut files = self.files();
        change(&mut files);
        if update_hashes {
            let mut m: Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
            for name in ["journal.jsonl", "journal.md", "README.md"] {
                if let Some(b) = files.get(name) {
                    m["files"][name] =
                        json!({"size":b.len(),"sha256":format!("{:x}",Sha256::digest(b))});
                }
            }
            files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        }
        let mut zip = ZipWriter::new(File::create(&self.archive).unwrap());
        for (name, bytes) in files {
            zip.start_file(name, SimpleFileOptions::default().large_file(true))
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    fn change_rows(&self, change: impl FnOnce(&mut Vec<Value>)) {
        self.rewrite(
            |f| {
                let mut rows: Vec<Value> = std::str::from_utf8(&f["journal.jsonl"])
                    .unwrap()
                    .lines()
                    .map(|s| serde_json::from_str(s).unwrap())
                    .collect();
                change(&mut rows);
                let mut b = Vec::new();
                for r in rows {
                    serde_json::to_writer(&mut b, &r).unwrap();
                    b.push(b'\n');
                }
                f.insert("journal.jsonl".into(), b);
            },
            true,
        );
    }
    fn change_manifest(&self, change: impl FnOnce(&mut Value)) {
        self.rewrite(
            |f| {
                let mut m: Value = serde_json::from_slice(&f["manifest.json"]).unwrap();
                change(&mut m);
                f.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
            },
            false,
        );
    }
    fn no_candidates(&self) {
        assert!(!fs::read_dir(self.root.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".mno-import-")
        }));
    }
}
#[test]
fn empty_export_builds_valid_empty_schema_zero_candidate() {
    let f = Fixture::new(&[]);
    let c = f.build().unwrap();
    assert_eq!(c.entry_count, 0);
    let db = Connection::open(c.path()).unwrap();
    assert!(crate::database::validate_schema(&db).unwrap());
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.close().unwrap();
    drop(c);
    f.no_candidates();
}
#[test]
fn source_independent_round_trip_preserves_all_seven_fields_and_active_bytes() {
    let docs = [
        ("", "{}"),
        ("a", LEGACY),
        (
            "b",
            "{\"type\":\"futureNode\",\"attrs\":{\"src\":\"/never/read/me.jpg\"}}",
        ),
        ("å", "{\"text\":\"Original\"}"),
    ];
    let f = Fixture::new(&docs);
    let before = fs::read(&f.active).unwrap();
    let c = f.build().unwrap();
    assert_eq!(c.entry_count, 4);
    let db = Connection::open(c.path()).unwrap();
    for (id, doc) in docs {
        let row=db.query_row("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries WHERE id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))).unwrap();
        assert_eq!(
            row,
            (
                id.into(),
                "legacy date".into(),
                "Titel åäö".into(),
                doc.into(),
                1,
                "created exactly".into(),
                "updated exactly".into()
            )
        );
    }
    db.close().unwrap();
    assert_eq!(fs::read(&f.active).unwrap(), before);
    assert!(!Path::new("/never/read/me.jpg").exists());
}
#[test]
fn corrupt_zip_missing_extra_and_duplicate_entries_are_rejected() {
    let f = Fixture::new(&[]);
    fs::write(&f.archive, b"not a ZIP").unwrap();
    assert!(f.build().is_err());
    for missing in NAMES {
        let f = Fixture::new(&[]);
        f.rewrite(
            |v| {
                v.remove(missing);
            },
            false,
        );
        assert!(f.build().is_err());
        f.no_candidates();
    }
    let f = Fixture::new(&[]);
    f.rewrite(
        |v| {
            v.insert("../extra".into(), vec![]);
        },
        false,
    );
    assert!(f.build().is_err());
    // Exactly four records, but two README names: zip's name index alone hides one.
    let f = Fixture::new(&[]);
    f.rewrite(
        |v| {
            let md = v.remove("journal.md").unwrap();
            v.insert("readme.md".into(), md);
        },
        false,
    );
    let mut b = fs::read(&f.archive).unwrap();
    for i in 0..=b.len() - 9 {
        if &b[i..i + 9] == b"readme.md" {
            b[i..i + 9].copy_from_slice(b"README.md");
        }
    }
    fs::write(&f.archive, b).unwrap();
    assert!(f.build().is_err());
}
#[test]
fn checksums_and_sizes_for_each_payload_are_required() {
    for name in ["journal.jsonl", "journal.md", "README.md"] {
        let f = Fixture::new(&[("a", "{}")]);
        f.rewrite(
            |v| {
                v.get_mut(name).unwrap().push(b' ');
            },
            false,
        );
        assert!(matches!(f.build(), Err(Error::Checksum)));
        f.no_candidates();
        let f = Fixture::new(&[("a", "{}")]);
        f.change_manifest(|m| m["files"][name]["sha256"] = json!("0".repeat(64)));
        assert!(matches!(f.build(), Err(Error::Checksum)));
        f.no_candidates();
    }
}
#[test]
fn wrong_versions_counts_and_manifest_contracts_are_rejected() {
    for field in [
        "format_version",
        "database_schema_version",
        "document_version",
        "entry_count",
    ] {
        let f = Fixture::new(&[("a", "{}")]);
        f.change_manifest(|m| m[field] = json!(99));
        assert!(f.build().is_err());
        f.no_candidates();
    }
    for field in ["format", "external_originals_included", "unknown"] {
        let f = Fixture::new(&[]);
        f.change_manifest(|m| m[field] = json!(true));
        assert!(f.build().is_err());
    }
}
#[test]
fn duplicate_ids_nonbinary_order_and_incomplete_rows_are_rejected() {
    for mutation in 0..4 {
        let f = Fixture::new(&[("a", "{}"), ("b", "{}")]);
        f.change_rows(|r| match mutation {
            0 => r[1]["id"] = r[0]["id"].clone(),
            1 => r.reverse(),
            2 => {
                r[0].as_object_mut().unwrap().remove("title");
            }
            _ => r[0]["extra"] = json!(1),
        });
        assert!(f.build().is_err());
        f.no_candidates();
    }
}
#[test]
fn field_types_versions_corrupt_documents_and_nonobjects_are_rejected() {
    for field in [
        "id",
        "entry_date",
        "title",
        "content_json",
        "created_at",
        "updated_at",
    ] {
        for bad in [Value::Null, json!(42), json!([])] {
            let f = Fixture::new(&[("a", "{}")]);
            f.change_rows(|r| r[0][field] = bad);
            assert!(f.build().is_err());
            f.no_candidates();
        }
    }
    for bad in [json!("1"), json!(1.0), json!(0), Value::Null] {
        let f = Fixture::new(&[("a", "{}")]);
        f.change_rows(|r| r[0]["document_version"] = bad);
        assert!(f.build().is_err());
    }
    for bad in ["{", "null", "[]", "1", "{} {}"] {
        let f = Fixture::new(&[("a", "{}")]);
        f.change_rows(|r| r[0]["content_json"] = json!(bad));
        assert!(f.build().is_err());
    }
}
#[test]
fn outer_duplicate_keys_blank_lines_missing_lf_and_invalid_utf8_are_rejected() {
    for kind in 0..4 {
        let f = Fixture::new(&[("a", "{}")]);
        f.rewrite(
            |v| {
                let b = v.get_mut("journal.jsonl").unwrap();
                match kind {
                    0 => {
                        let mut s = b"{\"id\":\"other\",".to_vec();
                        s.extend_from_slice(&b[1..]);
                        *b = s;
                    }
                    1 => b.push(b'\n'),
                    2 => {
                        b.pop();
                    }
                    _ => b[0] = 0xff,
                }
            },
            true,
        );
        assert!(f.build().is_err());
        f.no_candidates();
    }
}
#[test]
fn utf8_metadata_boundary_is_in_bytes_and_zero_and_empty_strings_survive() {
    let f = Fixture::new(&[("a", "{}")]);
    let title = "å".repeat(METADATA_LIMIT / 2);
    f.change_rows(|r| {
        r[0]["title"] = json!(title);
        r[0]["created_at"] = json!("");
        r[0]["content_json"] = json!("{\"zero\":0}");
    });
    let c = f.build().unwrap();
    let db = Connection::open(c.path()).unwrap();
    assert_eq!(
        db.query_row("SELECT title FROM journal_entries", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        title
    );
    db.close().unwrap();
    f.change_rows(|r| r[0]["title"] = json!(format!("{title}x")));
    assert!(matches!(f.build(), Err(Error::ResourceLimit)));
}
#[test]
fn document_size_and_depth_limits_match_export_validator() {
    let f = Fixture::new(&[("a", "{}")]);
    let doc = format!("{{\"text\":\"{}\"}}", "x".repeat(CONTENT_LIMIT - 11));
    assert_eq!(doc.len(), CONTENT_LIMIT);
    f.change_rows(|r| r[0]["content_json"] = json!(doc));
    let c = f.build().unwrap();
    assert_eq!(c.entry_count, 1);
    drop(c);
    f.change_rows(|r| r[0]["content_json"] = json!(format!("{doc} ")));
    assert!(matches!(f.build(), Err(Error::ResourceLimit)));
    let deep = format!("{{\"x\":{}0{}}}", "[".repeat(66), "]".repeat(66));
    f.change_rows(|r| r[0]["content_json"] = json!(deep));
    assert!(matches!(
        f.build(),
        Err(Error::ResourceLimit) | Err(Error::Record)
    ));
}
#[test]
fn errors_and_cancellation_before_and_after_commit_never_publish_or_change_active() {
    for at in [
        "package-verified",
        "before-insert",
        "row",
        "before-commit",
        "committed",
        "verified",
    ] {
        for cancel_at in [false, true] {
            let f = Fixture::new(&[("a", "{}"), ("b", LEGACY)]);
            let before = fs::read(&f.active).unwrap();
            let flag = flag();
            let result = build_with(
                &mut File::open(&f.archive).unwrap(),
                f.root.path(),
                &flag,
                |p| {
                    if p == at {
                        if cancel_at {
                            flag.store(true, Ordering::Release);
                        } else {
                            return Err(io::Error::other("injected disk failure").into());
                        }
                    }
                    Ok(())
                },
            );
            assert!(result.is_err(), "{at}");
            f.no_candidates();
            assert_eq!(fs::read(&f.active).unwrap(), before);
        }
    }
}
#[test]
fn process_crash_child() {
    let Ok(at) = std::env::var("MNO_IMPORT_CRASH_AT") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_IMPORT_CRASH_ROOT").unwrap());
    build_with(
        &mut File::open(root.join("journal.mno-export")).unwrap(),
        &root,
        &flag(),
        |p| {
            if p == at {
                std::process::exit(89);
            }
            Ok(())
        },
    )
    .unwrap();
    panic!("checkpoint not reached");
}
#[test]
fn real_process_interruptions_leave_only_private_staging_and_preserve_active() {
    for at in [
        "before-insert",
        "row",
        "before-commit",
        "committed",
        "verified",
    ] {
        let f = Fixture::new(&[("a", "{}"), ("b", LEGACY)]);
        let before = fs::read(&f.active).unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "open_import::tests::process_crash_child",
                "--nocapture",
            ])
            .env("MNO_IMPORT_CRASH_AT", at)
            .env("MNO_IMPORT_CRASH_ROOT", f.root.path())
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(89),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(&f.active).unwrap(), before);
        assert!(!f.active.with_extension("sqlite-journal").exists());
        let c = f.build().unwrap();
        assert_eq!(c.entry_count, 2); // Independent retry, never reuse crashed file.
    }
}

#[test]
fn duplicate_manifest_keys_and_extra_payload_records_are_rejected() {
    for nested in [false, true] {
        let f = Fixture::new(&[]);
        f.rewrite(|v| {
            let m=String::from_utf8(v["manifest.json"].clone()).unwrap();
            let changed=if nested {m.replace("\"files\": {", "\"files\": {\"README.md\":{\"size\":0,\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\"},")} else {m.replacen('{',"{\"format_version\":1,",1)};
            assert_ne!(m,changed);
            v.insert("manifest.json".into(),changed.into_bytes());
        },false);
        assert!(f.build().is_err());
        f.no_candidates();
    }
}
#[test]
fn post_commit_readback_detects_changes_to_every_stored_field() {
    for (column, value) in [
        ("id", "'other'"),
        ("entry_date", "'other'"),
        ("title", "'other'"),
        ("content_json", "'{\"other\":1}'"),
        ("document_version", "2"),
        ("created_at", "'other'"),
        ("updated_at", "'other'"),
    ] {
        let f = Fixture::new(&[("a", LEGACY)]);
        let result = build_with(
            &mut File::open(&f.archive).unwrap(),
            f.root.path(),
            &flag(),
            |at| {
                if at == "committed" {
                    let dir = fs::read_dir(f.root.path())
                        .unwrap()
                        .map(|e| e.unwrap().path())
                        .find(|p| {
                            p.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .starts_with(".mno-import-")
                        })
                        .unwrap();
                    let db = Connection::open(dir.join("candidate.sqlite")).unwrap();
                    db.execute_batch(&format!("UPDATE journal_entries SET {column}={value}"))
                        .unwrap();
                    db.close().unwrap();
                }
                Ok(())
            },
        );
        assert!(
            matches!(result, Err(Error::Verification)),
            "{column}: {result:?}"
        );
        f.no_candidates();
    }
}
#[test]
fn unwind_cleans_candidate_and_keeps_caller_owned_archive_usable() {
    let f = Fixture::new(&[("a", "{}")]);
    let before = fs::read(&f.active).unwrap();
    let mut file = File::open(&f.archive).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_with(&mut file, f.root.path(), &flag(), |at| {
            if at == "row" {
                panic!("isolated unwind");
            }
            Ok(())
        })
    }));
    assert!(result.is_err());
    f.no_candidates();
    assert_eq!(fs::read(&f.active).unwrap(), before);
    assert_eq!(
        build(&mut file, f.root.path(), &flag())
            .unwrap()
            .entry_count,
        1
    );
}

#[test]
fn import_proof_matches_candidate_and_rejects_nonautocommit_without_releasing_it() {
    let f = Fixture::new(&[("a", "{}"), ("b", LEGACY)]);
    let candidate = f.build().unwrap();
    let mut db = Connection::open(candidate.path()).unwrap();
    assert_eq!(
        verify_database(&mut db, &flag()).unwrap(),
        *candidate.proof()
    );
    assert_eq!(candidate.proof().entry_count, 2);
    db.execute_batch("BEGIN").unwrap();
    assert!(verify_database(&mut db, &flag()).is_err());
    assert!(!db.is_autocommit());
    db.execute_batch("ROLLBACK").unwrap();
    db.close().unwrap();
}
#[test]
fn import_proof_changes_for_each_of_the_seven_fields() {
    for (column, value) in [
        ("id", "'other'"),
        ("entry_date", "'other'"),
        ("title", "'other'"),
        ("content_json", "'{\"other\":1}'"),
        ("created_at", "'other'"),
        ("updated_at", "'other'"),
    ] {
        let f = Fixture::new(&[("a", LEGACY)]);
        let c = f.build().unwrap();
        let mut db = Connection::open(c.path()).unwrap();
        db.execute_batch(&format!("UPDATE journal_entries SET {column}={value}"))
            .unwrap();
        assert_ne!(
            verify_database(&mut db, &flag()).unwrap(),
            *c.proof(),
            "{column}"
        );
        db.close().unwrap();
    }
    let f = Fixture::new(&[("a", "{}")]);
    let c = f.build().unwrap();
    let mut db = Connection::open(c.path()).unwrap();
    db.execute_batch("UPDATE journal_entries SET document_version=2")
        .unwrap();
    assert!(verify_database(&mut db, &flag()).is_err());
    db.close().unwrap();
}
#[test]
fn import_proof_rejects_invalid_sql_types_unknown_schema_and_invalid_documents() {
    for sql in [
        "UPDATE journal_entries SET content_json=x'7b7d'",
        "UPDATE journal_entries SET id=NULL",
        "UPDATE journal_entries SET content_json='null'",
        "UPDATE journal_entries SET document_version='x'",
        "PRAGMA user_version=1",
        "CREATE TABLE extra(x)",
    ] {
        let f = Fixture::new(&[("a", "{}")]);
        let c = f.build().unwrap();
        let mut db = Connection::open(c.path()).unwrap();
        db.execute_batch(sql).unwrap();
        assert!(verify_database(&mut db, &flag()).is_err(), "{sql}");
        assert!(db.is_autocommit());
        db.close().unwrap();
    }
}

#[test]
fn frozen_independent_import_proof_vectors() {
    // Computed independently in Python using struct.pack BE and hashlib.sha256.
    let f = Fixture::new(&[]);
    assert_eq!(
        f.build().unwrap().proof().sha256,
        "838234c058261d8299f2f5c0697a048ea3223dcf1ad04fd8026757244b6fe3e5"
    );
    let f = Fixture::new(&[("a", "{}")]);
    assert_eq!(
        f.build().unwrap().proof().sha256,
        "c1303869ee7d548823d0315a640d89ff4883e5394053b7eecb23699b4474838a"
    );
}
