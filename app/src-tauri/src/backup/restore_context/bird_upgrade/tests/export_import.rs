use super::*;
use crate::{database_state::DatabaseState, open_import};
use std::io::{Read, Write};

fn export_fixture_for_format(
    root: &Path,
    format: u32,
) -> (
    PathBuf,
    crate::storage_content::Proof,
    Vec<crate::bird_observations::List>,
) {
    fs::create_dir(root).unwrap();
    let (path, _, _) = fixture(root);
    let lists = if format == 6 {
        initialize(&path).unwrap();
        add_lists(&path)
    } else {
        Vec::new()
    };
    let db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('entry-two','2026-11-12','Second',?1,1,'created','updated')",[DOC]).unwrap();
    if format == 6 {
        db.execute(
            "INSERT INTO journal_bird_lists VALUES ('entry-two',?1)",
            [&lists[0].id],
        )
        .unwrap();
    }
    close(db);
    let state = DatabaseState::default();
    if format == 6 {
        state.initialize_startup(root).unwrap();
    } else {
        state.initialize_schema4_startup(root).unwrap();
    }
    let generation = state.operation(None).unwrap().snapshot(()).generation;
    let lists = lists
        .iter()
        .map(|l| crate::bird_observations::reopen(&state, generation, &l.id).unwrap())
        .collect();
    let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = if format == 6 {
        crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false)).unwrap()
    } else {
        crate::storage_content::verify_v5(&mut db, &AtomicBool::new(false)).unwrap()
    };
    let archive = root.join("birds.mno-export");
    let m = crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        (
            m.format_version,
            m.database_schema_version,
            m.document_version
        ),
        (format, format - 1, 2)
    );
    if format == 6 {
        assert_eq!(m.files["bird_lists.jsonl"].row_count, Some(2));
        assert_eq!(m.files["bird_rows.jsonl"].row_count, Some(4));
        assert_eq!(m.files["journal_bird_lists.jsonl"].row_count, Some(2));
    }
    assert_eq!(m.files["images.jsonl"].row_count, Some(1));
    close(db);
    (archive, proof, lists)
}
fn export_fixture(
    root: &Path,
) -> (
    PathBuf,
    crate::storage_content::Proof,
    Vec<crate::bird_observations::List>,
) {
    export_fixture_for_format(root, 6)
}
fn candidate(archive: &Path, parent: &Path) -> open_import::Candidate {
    assert!(open_import::is_v6(&mut File::open(archive).unwrap()).unwrap());
    open_import::build_v6(
        &mut File::open(archive).unwrap(),
        parent,
        &AtomicBool::new(false),
    )
    .unwrap()
}
#[test]
fn format6_roundtrip_and_protected_activation_survive_restart_and_retry() {
    run_import_cases(6);
}
#[test]
fn normalized_format5_activation_preserves_content_and_retries() {
    run_import_cases(5);
}
fn run_import_cases(source_format: u32) {
    for mode in [
        "success",
        "fail:import-v3-before-marker-publish",
        "fail:import-v3-after-marker-publish",
        "fail:before-replace",
        "fail:after-replace",
        "fail:import-v3-sync-root",
        "fail:startup-before-retire",
        "crash:import-v3-after-marker-publish",
        "crash:after-replace",
    ] {
        let root = TempDir::new().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::bird_upgrade::tests::export_import::format6_activation_child","--nocapture"]).env("MNO_BIRD_SOURCE_FORMAT",source_format.to_string()).env("MNO_BIRD_IMPORT_CASE",mode).env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE",mode).output().unwrap();
        if mode.starts_with("crash:") {
            assert_eq!(output.status.code(), Some(88), "{mode}: {output:?}");
            let retry=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::bird_upgrade::tests::export_import::format6_finish_child","--nocapture"]).env("MNO_BIRD_SOURCE_FORMAT",source_format.to_string()).env("MNO_BIRD_IMPORT_FINISH","1").env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE","success").output().unwrap();
            assert!(retry.status.success(), "{mode}: {retry:?}");
        } else {
            assert!(output.status.success(), "{mode}: {output:?}");
        }
    }
}
#[test]
fn format6_activation_child() {
    let Ok(mode) = std::env::var("MNO_BIRD_IMPORT_CASE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    let source_format = std::env::var("MNO_BIRD_SOURCE_FORMAT")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let (archive, expected, lists) = export_fixture_for_format(&root.join("source"), source_format);
    let archive_bytes = fs::read(&archive).unwrap();
    let prepared = open_import::build_for_schema5(
        &mut File::open(&archive).unwrap(),
        &root,
        &AtomicBool::new(false),
        source_format,
    )
    .unwrap();
    if source_format == 6 {
        assert_eq!(prepared.proof().sha256, expected.sha256);
    } else {
        let mut readback =
            Connection::open_with_flags(prepared.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let tx = readback.transaction().unwrap();
        assert_eq!(
            crate::storage_content::verified_snapshot_v5(&tx, &AtomicBool::new(false)).unwrap(),
            expected
        );
        tx.commit().unwrap();
        close(readback);
    }
    let new = prepared.proof().clone();
    drop(prepared);
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let (target, _, _) = fixture(&data);
    initialize(&target).unwrap();
    add_lists(&target);
    let db = crate::database::open_existing(&target).unwrap();
    db.execute("UPDATE journal_entries SET title='Previous journal'", [])
        .unwrap();
    close(db);
    let state = DatabaseState::default();
    let target = state.initialize_startup(&data).unwrap();
    let before = fs::read(&target).unwrap();
    let old = super::super::super::activation::journal_proof(&target).unwrap();
    assert_ne!(old.sha256, new.sha256);
    fs::write(root.join("expected-previous.sha256"), &old.sha256).unwrap();
    let maintenance = state.maintenance().unwrap();
    let result = super::super::super::activation::importing::activate_schema5(
        &maintenance,
        &mut File::open(&archive).unwrap(),
        &old,
        &new,
        Arc::new(AtomicBool::new(false)),
        source_format,
    );
    if mode == "success" || mode == "fail:startup-before-retire" {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
    drop(result);
    drop(maintenance);
    assert!(state.operation(None).is_err());
    let protection: Vec<_> = fs::read_dir(data.join("backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mno-backup"))
        .collect();
    assert_eq!(protection.len(), 1);
    let protected =
        crate::backup::restore::prepare_candidate(&protection[0], Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v6(
            protected.source().unwrap(),
            &AtomicBool::new(false)
        )
        .unwrap()
        .sha256,
        old.sha256
    );
    drop(protected);
    if mode == "fail:import-v3-before-marker-publish" {
        assert_eq!(fs::read(&target).unwrap(), before);
        assert!(!data.join("recovery").exists());
        drop(state);
        let restarted = DatabaseState::default();
        restarted.initialize_startup(&data).unwrap();
        assert_eq!(
            super::super::super::activation::journal_proof(&target).unwrap(),
            old
        );
        assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
        return;
    }
    // Strict marker validation is exercised through production startup. Restore
    // the exact original evidence after each synthetic corruption.
    drop(state);
    if mode == "success" {
        let marker_path = data.join("recovery/activation-v1.json");
        let bytes = fs::read(&marker_path).unwrap();
        let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(original["version"], if source_format == 5 { 8 } else { 7 });
        assert_eq!(original["export_format_version"], source_format);
        for field in [
            "version",
            "schema_version",
            "old_schema_version",
            "export_format_version",
            "old_content",
            "new_content",
            "candidate_file",
            "unknown",
        ] {
            let mut altered = original.clone();
            match field {
                "old_content" | "new_content" => altered[field]["version"] = serde_json::json!(5),
                "version" => {
                    altered[field] = serde_json::json!(if source_format == 5 { 7 } else { 8 })
                }
                "candidate_file" => altered[field] = serde_json::json!("../mno.sqlite"),
                _ => altered[field] = serde_json::json!(0),
            }
            fs::write(&marker_path, serde_json::to_vec(&altered).unwrap()).unwrap();
            let bytes_before = fs::read(&target).unwrap();
            let blocked = DatabaseState::default();
            assert!(blocked.initialize_startup(&data).is_err());
            assert!(
                super::super::super::activation::startup::status(&blocked).is_err(),
                "{field}"
            );
            assert!(blocked.operation(None).is_err());
            assert_eq!(fs::read(&target).unwrap(), bytes_before);
            drop(blocked);
        }
        fs::write(&marker_path, bytes).unwrap();
    }

    let restarted = DatabaseState::default();
    assert!(restarted.initialize_startup(&data).is_err());
    use super::super::super::activation::startup;
    let view = startup::status(&restarted).unwrap();
    assert_eq!(view.operation, startup::Operation::Import);
    let action = if view.actions.contains(&startup::Action::FinishNew) {
        startup::Action::FinishNew
    } else {
        assert!(view.actions.contains(&startup::Action::Continue));
        startup::Action::Continue
    };
    if mode != "success" {
        if mode == "fail:startup-before-retire" {
            assert!(startup::run(&restarted, action).is_err());
            assert!(restarted.operation(None).is_err());
        }
        drop(restarted);
        // A fresh process below retries with the failure injection removed.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::bird_upgrade::tests::export_import::format6_finish_child",
                "--nocapture",
            ])
            .env("MNO_BIRD_IMPORT_FINISH", "1")
            .env("MNO_ACTIVATION_ROOT", &root)
            .env("MNO_ACTIVATION_MODE", "success")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    } else {
        startup::run(&restarted, action).unwrap();
        assert!(restarted.operation(None).is_ok());
        drop(restarted);
    }
    let final_start = DatabaseState::default();
    final_start.initialize_startup(&data).unwrap();
    assert_eq!(
        super::super::super::activation::journal_proof(&target).unwrap(),
        new
    );
    let generation = final_start.operation(None).unwrap().snapshot(()).generation;
    for list in lists {
        assert_eq!(
            crate::bird_observations::reopen(&final_start, generation, &list.id).unwrap(),
            list
        );
    }
    assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
}
#[test]
fn format6_finish_child() {
    if std::env::var_os("MNO_BIRD_IMPORT_FINISH").is_none() {
        return;
    }
    let data = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap()).join("data");
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&data).is_err());
    use super::super::super::activation::startup;
    let view = startup::status(&state).unwrap();
    let action = if view.actions.contains(&startup::Action::FinishNew) {
        startup::Action::FinishNew
    } else {
        startup::Action::Continue
    };
    startup::run(&state, action).unwrap();
    assert!(state.operation(None).is_ok());
    let source_format = std::env::var("MNO_BIRD_SOURCE_FORMAT")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let expected_candidate = open_import::build_for_schema5(
        &mut File::open(data.parent().unwrap().join("source/birds.mno-export")).unwrap(),
        data.parent().unwrap(),
        &AtomicBool::new(false),
        source_format,
    )
    .unwrap();
    let expected = expected_candidate.proof().clone();
    drop(expected_candidate);
    let target = state.ready_database_path().unwrap();
    assert_eq!(
        super::super::super::activation::journal_proof(&target).unwrap(),
        expected
    );
    let protected = fs::read_dir(data.join("backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mno-backup"))
        .unwrap();
    let candidate =
        crate::backup::restore::prepare_candidate(&protected, Arc::new(AtomicBool::new(false)))
            .unwrap();
    assert_eq!(
        crate::storage_content::verified_snapshot_v6(
            candidate.source().unwrap(),
            &AtomicBool::new(false)
        )
        .unwrap()
        .sha256,
        fs::read_to_string(data.parent().unwrap().join("expected-previous.sha256")).unwrap()
    );
}

fn rewrite(archive: &Path, destination: &Path, case: &str) {
    use sha2::{Digest, Sha256};
    let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
    let mut payloads = std::collections::BTreeMap::<String, Vec<u8>>::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        payloads.insert(f.name().into(), b);
    }
    let mut m: serde_json::Value = serde_json::from_slice(&payloads["manifest.json"]).unwrap();
    let name = match case {
        "legacy-reference" => "journal_places.jsonl",
        "legacy-document" => "journal.jsonl",
        "place" | "date" | "unknown" | "order" => "bird_lists.jsonl",
        "link" => "journal_bird_lists.jsonl",
        _ => "bird_rows.jsonl",
    };
    let mut rows: Vec<serde_json::Value> = std::str::from_utf8(&payloads[name])
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    match case {
        "legacy-reference" => rows[0]["place_id"] = serde_json::json!("missing-place"),
        "legacy-document" => rows[0]["content_json"] = serde_json::json!("not-json"),
        "place" => rows[0]["place_id"] = serde_json::json!("missing-place"),
        "date" => rows[0]["visit_date"] = serde_json::json!("2026-02-30"),
        "unknown" => rows[0]["unexpected"] = serde_json::json!(true),
        "order" => rows.reverse(),
        "link" => rows[0]["entry_id"] = serde_json::json!("missing-journal"),
        "list" => rows[0]["list_id"] = serde_json::json!("missing-list"),
        "count" => rows[0]["count"] = serde_json::json!(0),
        "type" => rows[0]["count"] = serde_json::json!("three"),
        "taxonomy" => rows[0]["taxonomy_version"] = serde_json::json!(" "),
        "missing" => {
            rows[0].as_object_mut().unwrap().remove("avibase_id");
        }
        "duplicate" => {
            rows[1]["id"] = rows[0]["id"].clone();
        }
        "position" => {
            let list = rows[0]["list_id"].clone();
            let pos = rows[0]["position"].clone();
            let row = rows
                .iter_mut()
                .skip(1)
                .find(|r| r["list_id"] == list)
                .unwrap();
            row["position"] = pos;
        }
        "manifest-count" | "checksum" => {}
        _ => panic!("unknown case"),
    }
    let mut bytes = Vec::new();
    for r in &rows {
        serde_json::to_writer(&mut bytes, r).unwrap();
        bytes.push(b'\n');
    }
    m["files"][name]["size"] = serde_json::json!(bytes.len());
    m["files"][name]["sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
    m["files"][name]["row_count"] = serde_json::json!(rows.len());
    if case == "manifest-count" {
        m["files"][name]["row_count"] = serde_json::json!(100);
    }
    if case == "checksum" {
        m["files"][name]["sha256"] = serde_json::json!("0".repeat(64));
    }
    payloads.insert(name.into(), bytes);
    payloads.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
    let mut out = zip::ZipWriter::new(File::create(destination).unwrap());
    for (name, bytes) in payloads {
        out.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
}
#[test]
fn format6_rejects_malformed_observations_without_changing_active_journal() {
    let root = TempDir::new().unwrap();
    let (archive, _, _) = export_fixture(&root.path().join("source"));
    let data = root.path().join("data");
    fs::create_dir(&data).unwrap();
    let (active, _, _) = fixture(&data);
    initialize(&active).unwrap();
    add_lists(&active);
    let before = fs::read(&active).unwrap();
    let original = fs::read(&archive).unwrap();
    for case in [
        "place",
        "date",
        "unknown",
        "order",
        "link",
        "list",
        "count",
        "type",
        "taxonomy",
        "missing",
        "duplicate",
        "position",
        "manifest-count",
        "checksum",
    ] {
        let damaged = root.path().join(format!("{case}.mno-export"));
        rewrite(&archive, &damaged, case);
        let mut file = File::open(&damaged).unwrap();
        assert!(
            open_import::build_v6(&mut file, root.path(), &AtomicBool::new(false)).is_err(),
            "{case}"
        );
        assert_eq!(fs::read(&active).unwrap(), before, "{case}");
        assert_eq!(fs::read(&archive).unwrap(), original);
        assert!(!data.join("recovery").exists());
    }
}

#[test]
fn normalized_format5_rejects_invalid_older_payloads_without_active_changes() {
    let root = TempDir::new().unwrap();
    let (archive, _, _) = export_fixture_for_format(&root.path().join("source"), 5);
    let data = root.path().join("data");
    fs::create_dir(&data).unwrap();
    let (active, _, _) = fixture(&data);
    initialize(&active).unwrap();
    add_lists(&active);
    let state = DatabaseState::default();
    state.initialize_startup(&data).unwrap();
    let before = fs::read(&active).unwrap();
    let original = fs::read(&archive).unwrap();
    for case in ["legacy-reference", "legacy-document"] {
        let damaged = root.path().join(format!("{case}.mno-export"));
        rewrite(&archive, &damaged, case);
        assert!(
            open_import::build_for_schema5(
                &mut File::open(&damaged).unwrap(),
                root.path(),
                &AtomicBool::new(false),
                5
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(fs::read(&active).unwrap(), before);
        assert_eq!(fs::read(&archive).unwrap(), original);
        assert!(state.operation(None).is_ok());
        assert!(!data.join("recovery").exists());
    }
}
