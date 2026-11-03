use super::*;
use crate::database_state::DatabaseState;
use std::process::{Command, Output};
const CHILD: &str = "backup::restore_context::activation::importing::tests::child";
const DOCUMENT: &str = "{ \"text\":\"Original åäö 🌿\", \"text\":\"duplicate\", \"future\":{\"src\":\"/never/open.jpg\"} }";
fn launch(root: &Path, task: &str, mode: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_IMPORT_ACTIVATION_TASK", task)
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_ACTIVATION_MODE", mode)
        .env("TMPDIR", root)
        .output()
        .unwrap()
}
fn require(out: Output, code: i32) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn export(root: &Path) -> PathBuf {
    let source = root.join("source.sqlite");
    database::create_legacy_database(&source).unwrap();
    let db = Connection::open(&source).unwrap();
    for (id, doc) in [
        ("a", "{}"),
        ("b", DOCUMENT),
        ("c", "{\"text\":\"Original\"}"),
    ] {
        db.execute("INSERT INTO journal_entries VALUES (?1,'older date','Titel ö',?2,1,'original creation','original update')",rusqlite::params![id,doc]).unwrap();
    }
    db.close().unwrap();
    let mut db = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let archive = root.join("incoming.mno-export");
    crate::open_export::export(&mut db, &archive, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    fs::remove_file(source).unwrap();
    archive
}
fn protection_unchanged(root: &Path) {
    let archive = fs::read_dir(root.join("data/backups/pre-restore"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mno-backup"))
        .unwrap();
    assert_eq!(
        fs::read(archive).unwrap(),
        fs::read(root.join("protection-before")).unwrap()
    );
}
fn rows(state: &DatabaseState, root: &Path, old: bool) {
    let permit = state.operation(None).unwrap();
    let db = Connection::open_with_flags(
        root.join("data/mno.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut q=db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
    let mut rs = q.query([]).unwrap();
    if !old {
        for (id, doc) in [
            ("a", "{}"),
            ("b", DOCUMENT),
            ("c", "{\"text\":\"Original\"}"),
        ] {
            let r = rs.next().unwrap().unwrap();
            assert_eq!(r.get::<_, String>(0).unwrap(), id);
            assert_eq!(r.get::<_, String>(1).unwrap(), "older date");
            assert_eq!(r.get::<_, String>(2).unwrap(), "Titel ö");
            assert_eq!(r.get::<_, String>(3).unwrap(), doc);
            assert_eq!(r.get::<_, i64>(4).unwrap(), 1);
            assert_eq!(r.get::<_, String>(5).unwrap(), "original creation");
            assert_eq!(r.get::<_, String>(6).unwrap(), "original update");
        }
    }
    assert!(rs.next().unwrap().is_none());
    drop(rs);
    drop(q);
    db.close().unwrap();
    drop(permit);
}
#[test]
fn child() {
    let Ok(task) = std::env::var("MNO_IMPORT_ACTIVATION_TASK") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    crate::panic_policy::install();
    let state = DatabaseState::default();
    if task == "activate" || task == "occupied" {
        let target = state.initialize_legacy_startup(&root.join("data")).unwrap();
        if task == "occupied" {
            let db = Connection::open(&target).unwrap();
            // A safely empty document is still a stored row and must block import.
            db.execute("INSERT INTO journal_entries VALUES('hidden','2026-10-24','','{\"type\":\"doc\",\"content\":[]}',1,'','')",[]).unwrap();
            db.close().unwrap();
        }
        let before = fs::read(&target).unwrap();
        let archive = export(&root);
        let maintenance = state.maintenance().unwrap();
        let result = activate(
            &maintenance,
            &mut File::open(archive).unwrap(),
            Arc::new(AtomicBool::new(false)),
        );
        if task == "occupied" {
            assert!(result.is_err());
            assert_eq!(fs::read(&target).unwrap(), before);
            assert!(!root.join("data/recovery").exists());
            std::process::exit(71);
        }
        if std::env::var("MNO_ACTIVATION_MODE")
            .unwrap()
            .starts_with("fail:")
        {
            assert!(result.is_err());
            let m = std::env::var("MNO_ACTIVATION_MODE").unwrap();
            if m != "fail:after-replace" {
                assert_eq!(fs::read(&target).unwrap(), before);
            }
            drop(result);
            drop(maintenance);
            assert!(state.operation(None).is_err());
            std::process::exit(72);
        }
        let activated = result.unwrap();
        assert!(activated.marker.is_import());
        assert_eq!(activated.marker.old_content.sha256, EMPTY_RESTORE_SHA256);
        drop(activated);
        drop(maintenance);
        assert!(state.operation(None).is_err());
        assert!(root.join("data/recovery/activation-v1.json").exists());
        protection_unchanged(&root);
        std::process::exit(0);
    }
    let startup = state.initialize_legacy_startup(&root.join("data"));
    if task == "ready-old" || task == "ready-new" {
        startup.unwrap();
        rows(&state, &root, task == "ready-old");
        std::process::exit(0);
    }
    assert!(startup.is_err());
    assert!(state.operation(None).is_err());
    if task == "blocked" {
        assert!(
            startup::status(&state).is_err() || startup::status(&state).unwrap().actions.is_empty()
        );
        assert!(startup::run(&state, startup::Action::FinishNew).is_err());
        assert!(state.operation(None).is_err());
        std::process::exit(73);
    }
    if task == "keep-damaged" {
        let view = startup::status(&state).unwrap();
        assert_eq!(view.operation, startup::Operation::Import);
        assert_eq!(serde_json::to_value(&view).unwrap()["operation"], "import");
        assert!(!view.actions.contains(&startup::Action::Continue));
        assert!(startup::run(&state, startup::Action::Continue).is_err());
        assert!(state.operation(None).is_err());
    }
    let action = match task.as_str() {
        "finish" => startup::Action::FinishNew,
        "continue" => startup::Action::Continue,
        "keep" | "keep-damaged" => startup::Action::KeepPrevious,
        _ => panic!("unknown task"),
    };
    let view = startup::status(&state).unwrap();
    assert_eq!(view.operation, startup::Operation::Import);
    assert_eq!(serde_json::to_value(&view).unwrap()["operation"], "import");
    assert!(view.actions.contains(&action));
    startup::run(&state, action).unwrap();
    rows(&state, &root, task == "keep" || task == "keep-damaged");
    protection_unchanged(&root);
    std::process::exit(0);
}
#[test]
fn import_activation_and_two_real_startups_preserve_all_fields() {
    let root = tempfile::tempdir().unwrap();
    require(launch(root.path(), "activate", ""), 0);
    require(launch(root.path(), "finish", ""), 0);
    require(launch(root.path(), "ready-new", ""), 0);
    let path = root.path().join("data/mno.sqlite").canonicalize().unwrap();
    // A path/open failure must not satisfy the strict-validation assertion.
    assert!(matches!(
        super::super::verify_file(&path),
        Err(Error::Verification)
    ));
    let db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    let legacy: String = db
        .query_row(
            "SELECT content_json FROM journal_entries WHERE id='a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(legacy, "{}");
    // The fixture also has a legacy date. Use valid metadata so it cannot
    // mask document rejection by the existing strict parser's test adapter.
    assert_eq!(
        crate::backup::restore::validate_journal_for_schema_test("a", "2026-10-24", &legacy),
        Err(PrepareError::IncompatibleDocument)
    );
    db.close().unwrap();
}
#[test]
fn stored_empty_row_rejects_import_without_modifying_active() {
    let root = tempfile::tempdir().unwrap();
    require(launch(root.path(), "occupied", ""), 71);
}
#[test]
fn crashes_before_marker_leave_only_private_staging_and_normal_old_startup() {
    for at in [
        "import-before-build",
        "import-built",
        "protection-durable",
        "all-closed",
        "import-sync-built-file",
        "import-sync-built-directory",
        "sync-recovery-created",
        "before-marker",
        "sync-marker-file",
        "sync-marker-directory",
        "marker-durable",
        "import-staged",
        "import-sync-staging-source",
        "import-sync-publication",
        "import-before-marker-publish",
    ] {
        let root = tempfile::tempdir().unwrap();
        require(launch(root.path(), "activate", &format!("crash:{at}")), 88);
        assert!(!root.path().join("data/recovery").exists());
        require(launch(root.path(), "ready-old", ""), 0);
    }
}
#[test]
fn crashes_before_and_after_replacement_recover_through_shared_actions() {
    for at in [
        "import-after-marker-rename",
        "import-sync-publication-source",
        "import-sync-marker-root",
        "import-marker-published",
        "sync-candidate-file",
        "before-replace",
        "after-replace",
        "sync-source-directory",
        "sync-active-directory",
        "disk-verified",
    ] {
        let root = tempfile::tempdir().unwrap();
        require(launch(root.path(), "activate", &format!("crash:{at}")), 88);
        let action = if [
            "import-after-marker-rename",
            "import-sync-publication-source",
            "import-sync-marker-root",
            "import-marker-published",
            "sync-candidate-file",
            "before-replace",
        ]
        .contains(&at)
        {
            "continue"
        } else {
            "finish"
        };
        require(launch(root.path(), action, ""), 0);
        require(
            launch(
                root.path(),
                if action == "keep" {
                    "ready-old"
                } else {
                    "ready-new"
                },
                "",
            ),
            0,
        );
    }
}
#[test]
fn startup_interruptions_never_grant_premature_ready_and_can_be_repeated() {
    for at in [
        "startup-verified",
        "startup-final-verified",
        "startup-before-retire",
        "startup-after-retire",
        "startup-before-admit",
    ] {
        let root = tempfile::tempdir().unwrap();
        require(launch(root.path(), "activate", ""), 0);
        require(launch(root.path(), "finish", &format!("crash:{at}")), 88);
        let action = if ["startup-after-retire", "startup-before-admit"].contains(&at) {
            "ready-new"
        } else {
            "finish"
        };
        require(launch(root.path(), action, ""), 0);
        require(launch(root.path(), "ready-new", ""), 0);
    }
    let root = tempfile::tempdir().unwrap();
    require(launch(root.path(), "activate", "crash:before-replace"), 88);
    require(
        launch(root.path(), "continue", "crash:startup-before-replace"),
        88,
    );
    require(
        launch(root.path(), "continue", "crash:startup-after-replace"),
        88,
    );
    require(launch(root.path(), "finish", ""), 0);
    require(launch(root.path(), "ready-new", ""), 0);
}
#[test]
fn unknown_marker_variants_versions_proofs_and_damaged_active_block_ready() {
    for change in [
        "unknown-format",
        "unknown-version",
        "schema",
        "old-proof",
        "new-proof",
        "strict-confusion",
        "damage",
    ] {
        let root = tempfile::tempdir().unwrap();
        require(launch(root.path(), "activate", ""), 0);
        let path = root.path().join("data/recovery/activation-v1.json");
        let mut marker: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match change {
            "unknown-format" => marker["format"] = serde_json::json!("unknown"),
            "unknown-version" => marker["version"] = serde_json::json!(2),
            "schema" => marker["schema_version"] = serde_json::json!(1),
            "old-proof" => marker["old_content"]["sha256"] = serde_json::json!("0".repeat(64)),
            "new-proof" => marker["new_content"]["sha256"] = serde_json::json!("0".repeat(64)),
            "strict-confusion" => marker["format"] = serde_json::json!("mno-database-activation"),
            _ => fs::write(root.path().join("data/mno.sqlite"), b"damaged").unwrap(),
        }
        fs::write(path, serde_json::to_vec(&marker).unwrap()).unwrap();
        require(launch(root.path(), "blocked", ""), 73);
        protection_unchanged(root.path());
    }
}
#[test]
fn modified_staged_candidate_cannot_be_continued_but_old_can_be_kept() {
    let root = tempfile::tempdir().unwrap();
    require(launch(root.path(), "activate", "crash:before-replace"), 88);
    let marker = read_marker(&root.path().join("data")).unwrap();
    let db = Connection::open(
        root.path()
            .join("data/recovery")
            .join(&marker.candidate_file),
    )
    .unwrap();
    db.execute(
        "UPDATE journal_entries SET content_json='{}' WHERE id='b'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    require(launch(root.path(), "keep-damaged", ""), 0);
    require(launch(root.path(), "ready-old", ""), 0);
}
#[test]
fn empty_old_proof_matches_frozen_strict_schema_zero_algorithm() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("empty.sqlite");
    database::create_legacy_database(&path).unwrap();
    assert_eq!(
        super::super::verify_file(&path.canonicalize().unwrap())
            .unwrap()
            .sha256,
        EMPTY_RESTORE_SHA256
    );
}

#[test]
fn injected_errors_before_and_after_publication_preserve_recoverable_content() {
    for at in [
        "import-built",
        "sync-marker-file",
        "import-staged",
        "import-marker-published",
        "before-replace",
        "after-replace",
    ] {
        let root = tempfile::tempdir().unwrap();
        require(launch(root.path(), "activate", &format!("fail:{at}")), 72);
        let action = if ["import-built", "sync-marker-file", "import-staged"].contains(&at) {
            "ready-old"
        } else if at == "after-replace" {
            "finish"
        } else {
            "continue"
        };
        require(launch(root.path(), action, ""), 0);
    }
}
