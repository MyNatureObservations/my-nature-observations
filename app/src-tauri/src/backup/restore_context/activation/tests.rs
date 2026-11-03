use super::*;
use crate::{
    backup::{self, BackupKind},
    database_state::DatabaseState,
};
use std::{
    io,
    os::unix::process::ExitStatusExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const ROOT: &str = "MNO_ACTIVATION_ROOT";
const MODE: &str = "MNO_ACTIVATION_MODE";
fn mode() -> String {
    std::env::var(MODE).unwrap_or_default()
}
fn root() -> PathBuf {
    PathBuf::from(std::env::var_os(ROOT).expect("child-only hook"))
}
fn event(name: &str) {
    if std::env::var_os(ROOT).is_some() {
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root().join("events"))
                .unwrap(),
            "{name}"
        )
        .unwrap();
    }
}
pub(super) fn point(name: &str) -> Outcome<()> {
    event(name);
    let m = mode();
    if name == "protection-durable" {
        let directory = root().join("data/backups/pre-restore");
        let entries: Vec<_> = fs::read_dir(directory)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| e.path().extension().is_some_and(|v| v == "mno-backup"))
            .collect();
        assert_eq!(entries.len(), 1);
        fs::write(
            root().join("protection-before"),
            fs::read(entries[0].path())?,
        )?;
    }
    if m == format!("panic:{name}") {
        panic!("injected activation panic");
    }
    if m == format!("crash:{name}") {
        std::process::exit(88);
    }
    if m == format!("fail:{name}") || m == format!("schema2-old-fail:{name}") {
        return Err(io::Error::other("injected checkpoint or durability failure").into());
    }
    if m == "rename-error" && name == "before-replace" {
        let data = root().join("data");
        let marker = read_marker(&data)?;
        fs::rename(
            data.join("recovery").join(&marker.candidate_file),
            data.join("recovery/retained-candidate.sqlite"),
        )?;
    }
    if m == "cancel" && name == "marker-durable" {
        return Err(Error::Cancelled);
    }
    Ok(())
}
pub(super) fn configure(db: &Connection) -> Outcome<()> {
    if mode() == "full" {
        db.execute_batch("PRAGMA max_page_count=1;")?;
    }
    Ok(())
}
pub(super) fn close(db: Connection) -> std::result::Result<(), (Connection, rusqlite::Error)> {
    if mode() == "close-error" && root().join("close-error-armed").exists() {
        // Inject the API error while retaining the ORIGINAL owner, then let
        // Work's actual error branch perform its terminal action.
        event("close-error-terminal");
        return Err((
            db,
            rusqlite::Error::SqliteFailure(ffi::Error::new(ffi::SQLITE_IOERR), None),
        ));
    }
    db.close()
}
pub(super) fn configure_close(db: &Connection) {
    if mode() == "close-error" {
        fs::write(root().join("close-error-armed"), b"armed").unwrap();
    }
    if mode() == "close-busy" {
        std::mem::forget(db.prepare("SELECT 1").unwrap());
        event("close-busy-terminal");
    }
}
pub(super) fn step(rc: i32) -> i32 {
    if mode() == "step-ioerr" {
        ffi::SQLITE_IOERR
    } else {
        rc
    }
}
pub(super) fn finish(rc: i32) -> i32 {
    if mode() == "finish-ioerr" {
        ffi::SQLITE_IOERR
    } else {
        rc
    }
}
fn document(text: &str) -> String {
    serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}).to_string()
}
fn insert(db: &Connection, id: &str, text: &str) {
    db.execute("INSERT INTO journal_entries VALUES(?1,'2026-10-23',?2,?3,1,'creation bytes åäö','update bytes öäå')",rusqlite::params![id,text,document(text)]).unwrap();
}
fn fixture(root: &Path) -> (DatabaseState, PathBuf, Content, Content) {
    let state = DatabaseState::default();
    let target = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let db = Connection::open(&target).unwrap();
    insert(&db, "old-entry", "Gammal journal");
    db.close().unwrap();
    let old = verify_file(&target).unwrap();
    fs::write(root.join("old-bytes"), fs::read(&target).unwrap()).unwrap();
    let source = Connection::open(root.join("source.sqlite")).unwrap();
    database::initialize_schema(&source).unwrap();
    // Many pages: partial step, FULL and abandoned transaction are real SQLite cases.
    insert(
        &source,
        "new-entry",
        &format!("Ny journal åäö {}", "n".repeat(200_000)),
    );
    let archive = root.join("incoming.mno-backup");
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    let new = verify_file(&root.join("source.sqlite")).unwrap();
    fs::write(root.join("archive-before"), fs::read(&archive).unwrap()).unwrap();
    (state, archive, old, new)
}
#[test]
fn schema1_to_schema2_restore_uses_separate_proofs_and_recovers_new() {
    if std::env::var_os(ROOT).is_none() {
        for mode in [
            "schema1-old",
            "schema2-old",
            "fail:before-replace",
            "schema2-old-fail:before-replace",
            "fail:after-replace",
        ] {
            let d = tempfile::TempDir::new().unwrap();
            let output=Command::new(std::env::current_exe().unwrap())
            .args(["--exact","backup::restore_context::activation::tests::schema1_to_schema2_restore_uses_separate_proofs_and_recovers_new","--nocapture"])
            .env(ROOT,d.path()).env(MODE,mode).output().unwrap();
            assert!(
                output.status.success(),
                "mode {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let root_path = root();
    let root = root_path.as_path();
    let state = DatabaseState::default();
    let target = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let mut active = Connection::open(&target).unwrap();
    database::weather_schema::migrate(&mut active).unwrap();
    if mode().contains("schema2-old") {
        database::place_schema::migrate_candidate(&mut active).unwrap();
    }
    insert(&active, "old-entry", "old");
    active.close().unwrap();
    let mut source = Connection::open(root.join("source.sqlite")).unwrap();
    database::initialize_schema(&source).unwrap();
    database::weather_schema::migrate(&mut source).unwrap();
    database::place_schema::migrate_candidate(&mut source).unwrap();
    insert(&source, "new-entry", "new");
    source.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Field',1)",[]).unwrap();
    source.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES('new-entry','00000000-0000-4000-8000-000000000001','Field',1,1)",[]).unwrap();
    let archive = root.join("incoming.mno-backup");
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    let maintenance = state.maintenance().unwrap();
    let result = activate(&maintenance, &archive, Arc::new(AtomicBool::new(false)));
    if mode().contains("fail:") {
        assert!(result.is_err());
        drop(result);
        drop(maintenance);
        let marker = read_marker(&root.join("data")).unwrap();
        assert_eq!(marker.version, 3);
        let active = if mode().contains("before-replace") {
            &marker.old_content
        } else {
            &marker.new_content
        };
        assert_eq!(
            &verify_file_version(&target, active.version).unwrap(),
            active
        );
        drop(state);
        let recovery = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::restore_context::activation::tests::schema2_recovery_child",
                "--nocapture",
            ])
            .env(ROOT, root)
            .env(
                "MNO_SCHEMA2_RECOVERY_PHASE",
                if mode() == "schema2-old-fail:before-replace" {
                    "before-current"
                } else if mode().contains("before-replace") {
                    "before"
                } else {
                    "after"
                },
            )
            .output()
            .unwrap();
        assert!(
            recovery.status.success(),
            "{}",
            String::from_utf8_lossy(&recovery.stderr)
        );
        return;
    }
    let done = result.unwrap();
    assert_eq!(
        (
            done.marker.version,
            done.marker.old_schema_version,
            done.marker.schema_version,
            done.marker.backup_format_version
        ),
        (
            3,
            Some(if mode() == "schema2-old" { 2 } else { 1 }),
            2,
            Some(3)
        )
    );
    assert_eq!(
        (
            done.marker.old_content.version,
            done.marker.new_content.version
        ),
        (if mode() == "schema2-old" { 3 } else { 2 }, 3)
    );
    assert_eq!(inspect(&done._session).unwrap(), Observed::New);
    let original = serde_json::to_value(&done.marker).unwrap();
    for (field, value) in [
        ("schema_version", serde_json::json!(1)),
        ("backup_format_version", serde_json::json!(2)),
        ("old_schema_version", serde_json::json!(0)),
    ] {
        let mut altered = original.clone();
        altered[field] = value;
        assert!(
            parse_marker(&serde_json::to_vec(&altered).unwrap()).is_err(),
            "accepted {field}"
        );
    }
    let mut altered = original.clone();
    altered["new_content"]["version"] = serde_json::json!(2);
    assert!(parse_marker(&serde_json::to_vec(&altered).unwrap()).is_err());
    let mut altered = original.clone();
    altered["unknown"] = serde_json::json!(true);
    assert!(parse_marker(&serde_json::to_vec(&altered).unwrap()).is_err());
    let mut altered = original.clone();
    altered["export_format_version"] = serde_json::Value::Null;
    assert!(parse_marker(&serde_json::to_vec(&altered).unwrap()).is_err());
    assert_eq!(
        verify_file_version(&target, 3).unwrap(),
        done.marker.new_content
    );
}
#[test]
fn schema2_recovery_child() {
    let Some(phase) = std::env::var_os("MNO_SCHEMA2_RECOVERY_PHASE") else {
        return;
    };
    let _phase = phase.to_str().unwrap();
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&root().join("data")).is_err());
    let view = startup::status(&state).unwrap();
    assert_eq!(view.operation, startup::Operation::Restore);
    let active = root().join("data/mno.sqlite");
    let before = fs::read(&active).unwrap();
    assert!(view.actions.is_empty());
    assert!(startup::run(&state, startup::Action::Continue).is_err());
    assert!(state.operation(None).is_err());
    assert_eq!(fs::read(&active).unwrap(), before);
}
#[test]
fn format3_import_marker4_preserves_old_and_classifies_interruption() {
    if std::env::var_os(ROOT).is_none() {
        for mode in [
            "schema1-old",
            "schema2-old",
            "empty-old",
            "fail:import-v3-before-marker-publish",
            "fail:before-replace",
            "fail:after-replace",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let run = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "backup::restore_context::activation::tests::format3_import_marker4_preserves_old_and_classifies_interruption", "--nocapture"])
                .env(ROOT, dir.path()).env(MODE, mode).output().unwrap();
            assert!(
                run.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
        return;
    }
    let dir = root();
    let state = DatabaseState::default();
    let target = state.initialize_legacy_startup(&dir.join("data")).unwrap();
    let mut old = Connection::open(&target).unwrap();
    crate::database::weather_schema::migrate(&mut old).unwrap();
    if mode() == "schema2-old" {
        crate::database::place_schema::migrate_candidate(&mut old).unwrap();
    }
    if mode() != "empty-old" {
        insert(&old, "old-entry", "old");
    }
    old.close().unwrap();
    let mut new = Connection::open(dir.join("source.sqlite")).unwrap();
    crate::database::initialize_schema(&new).unwrap();
    crate::database::weather_schema::migrate(&mut new).unwrap();
    crate::database::place_schema::migrate_candidate(&mut new).unwrap();
    insert(&new, "new-entry", "new");
    new.execute("INSERT INTO places(id,name,revision) VALUES('00000000-0000-4000-8000-000000000001','Shared',1)", []).unwrap();
    new.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES('new-entry','00000000-0000-4000-8000-000000000001','Shared',1,1)", []).unwrap();
    let archive = dir.join("incoming.mno-export");
    new.close().unwrap();
    let mut readonly =
        Connection::open_with_flags(dir.join("source.sqlite"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    crate::open_export::export(&mut readonly, &archive, &AtomicBool::new(false)).unwrap();
    readonly.close().unwrap();
    let old_proof = super::marker_contract::preparation::journal_proof(&target).unwrap();
    let stage = tempfile::tempdir().unwrap();
    let candidate = crate::open_import::build_v3(
        &mut File::open(&archive).unwrap(),
        stage.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let new_proof = candidate.proof().clone();
    drop(candidate);
    let maintenance = state.maintenance().unwrap();
    let result = importing::activate_v3(
        &maintenance,
        &mut File::open(&archive).unwrap(),
        &old_proof,
        &new_proof,
        Arc::new(AtomicBool::new(false)),
    );
    if mode() == "fail:import-v3-before-marker-publish" {
        assert!(result.is_err());
        assert!(!dir.join("data/recovery").exists());
        assert_eq!(
            verify_file_version(&target, old_proof.version)
                .unwrap()
                .sha256,
            old_proof.sha256
        );
        return;
    }
    let marker = read_marker(&dir.join("data")).unwrap();
    assert_eq!(
        (
            marker.version,
            marker.export_format_version,
            marker.schema_version,
            marker.new_content.version
        ),
        (4, Some(3), 2, 3)
    );
    let original = serde_json::to_value(&marker).unwrap();
    for (field, value) in [
        ("export_format_version", serde_json::json!(2)),
        ("backup_format_version", serde_json::json!(3)),
        ("old_schema_version", serde_json::json!(0)),
        ("schema_version", serde_json::json!(1)),
        ("candidate_file", serde_json::json!("wrong.sqlite")),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        assert!(
            parse_marker(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{field}"
        );
    }
    let mut changed = original.clone();
    changed["new_content"]["version"] = serde_json::json!(2);
    assert!(parse_marker(&serde_json::to_vec(&changed).unwrap()).is_err());
    let mut changed = original.clone();
    changed["old_content"]["version"] = serde_json::json!(1);
    assert!(parse_marker(&serde_json::to_vec(&changed).unwrap()).is_err());
    let current = if mode() == "fail:before-replace" {
        &marker.old_content
    } else {
        &marker.new_content
    };
    assert_eq!(
        &verify_file_version(&target, current.version).unwrap(),
        current
    );
    if mode().starts_with("fail:") {
        assert!(result.is_err());
    } else {
        assert!(result.is_ok());
    }
    drop(result);
    drop(maintenance);
    drop(state);
    let restarted = DatabaseState::default();
    assert!(restarted.initialize_startup(&dir.join("data")).is_err());
    let view = startup::status(&restarted).unwrap();
    assert_eq!(view.operation, startup::Operation::Import);
    let before = fs::read(&target).unwrap();
    assert!(view.actions.is_empty());
    assert!(startup::run(&restarted, startup::Action::Continue).is_err());
    assert!(restarted.operation(None).is_err());
    assert_eq!(fs::read(&target).unwrap(), before);
}
#[test]
fn schema2_downgrade_and_missing_active_are_rejected_without_marker() {
    if std::env::var_os(ROOT).is_none() {
        for mode in ["older-backup", "missing-active"] {
            let d = tempfile::TempDir::new().unwrap();
            let output=Command::new(std::env::current_exe().unwrap())
                .args(["--exact","backup::restore_context::activation::tests::schema2_downgrade_and_missing_active_are_rejected_without_marker","--nocapture"])
                .env(ROOT,d.path()).env(MODE,mode).output().unwrap();
            assert!(
                output.status.success(),
                "mode {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let root_path = root();
    let root = root_path.as_path();
    let state = DatabaseState::default();
    let target = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let mut active = Connection::open(&target).unwrap();
    database::weather_schema::migrate(&mut active).unwrap();
    database::place_schema::migrate_candidate(&mut active).unwrap();
    insert(&active, "old-entry", "old");
    active.close().unwrap();
    let old = verify_file_version(&target, 3).unwrap();
    let mut source = Connection::open(root.join("source.sqlite")).unwrap();
    database::initialize_schema(&source).unwrap();
    database::weather_schema::migrate(&mut source).unwrap();
    if mode() == "missing-active" {
        database::place_schema::migrate_candidate(&mut source).unwrap();
    }
    insert(&source, "new-entry", "new");
    let archive = root.join("older.mno-backup");
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    if mode() == "missing-active" {
        fs::remove_file(&target).unwrap();
    }
    let maintenance = state.maintenance().unwrap();
    assert!(activate(&maintenance, &archive, Arc::new(AtomicBool::new(false))).is_err());
    assert!(!root.join("data/recovery").exists());
    if mode() == "older-backup" {
        assert_eq!(verify_file_version(&target, 3).unwrap(), old);
    }
}
fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(Instant::now() < deadline, "missing {}", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn holder_child() {
    let Some(p) = std::env::var_os("MNO_ACTIVATION_LOCK") else {
        return;
    };
    let db = Connection::open(p).unwrap();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    fs::write(root().join("locked"), b"locked").unwrap();
    wait_file(&root().join("release"));
    db.execute_batch("ROLLBACK").unwrap();
    db.close().unwrap();
}
pub(super) fn command(root: &Path, mode: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "backup::restore_context::activation::tests::activation_child",
        "--nocapture",
    ])
    .env(ROOT, root)
    .env(MODE, mode)
    .env("TMPDIR", root)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    c
}
#[test]
fn activation_child() {
    if std::env::var_os(ROOT).is_none() {
        return;
    }
    let root = root();
    let m = mode();
    let (state, archive, old, new) = fixture(&root);
    let mut holder = None;
    if m == "busy" {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args([
            "--exact",
            "backup::restore_context::activation::tests::holder_child",
            "--nocapture",
        ])
        .env(ROOT, &root)
        .env("MNO_ACTIVATION_LOCK", root.join("data/mno.sqlite"));
        holder = Some(c.spawn().unwrap());
        wait_file(&root.join("locked"));
    }
    if m == "invalid" {
        fs::write(&archive, b"not a backup").unwrap();
        fs::write(root.join("archive-before"), b"not a backup").unwrap();
    }
    if m == "legacy-p" {
        fs::create_dir(root.join("data/recovery")).unwrap();
        fs::write(
            root.join("data/recovery/pending-restore.json"),
            b"legacy P retained",
        )
        .unwrap();
    }
    if m == "legacy-q" {
        fs::write(
            root.join("data/.mno-restore-completion-v1.json"),
            b"legacy Q retained",
        )
        .unwrap();
    }
    if m == "sidecar" {
        fs::write(root.join("data/mno.sqlite-wal"), b"blocked sidecar").unwrap();
    }
    crate::panic_policy::install();
    let success = m == "success";
    {
        let maintenance = state.maintenance().unwrap();
        let result = activate(&maintenance, &archive, Arc::new(AtomicBool::new(false)));
        if let Some(mut child) = holder {
            fs::write(root.join("release"), b"release").unwrap();
            assert!(child.wait().unwrap().success());
        }
        match result {
            Ok(done) => {
                assert!(success, "unexpected success {m}");
                assert_eq!(done.marker.old_content, old);
                assert_eq!(done.marker.new_content, new);
                assert!(maintenance.ensure_usable().is_err());
                event("disk-only-no-ready");
                drop(done);
            }
            Err(e) => {
                assert!(!success, "activation failed {e:?}");
                if m == "full" {
                    assert!(
                        matches!(e,Error::Sqlite(rusqlite::Error::SqliteFailure(ref code,_)) if code.extended_code==ffi::SQLITE_FULL),
                        "expected real FULL: {e:?}"
                    );
                    event("real-full");
                }
                if m == "busy" {
                    assert!(matches!(e, Error::Binding));
                    event("real-busy");
                }
                if m == "invalid" {
                    assert!(matches!(e, Error::Candidate(_)));
                }
                if m == "rename-error" {
                    assert!(matches!(e,Error::Io(ref e) if e.kind()==io::ErrorKind::NotFound));
                }
                event("ordinary-error");
            }
        }
        drop(maintenance);
        if !["invalid", "fail:before-input", "fail:input-verified"].contains(&m.as_str()) {
            assert!(state.operation(None).is_err());
        } else {
            assert!(state.operation(None).is_ok());
        }
    }
    drop(state);
    event("child-checked");
    std::process::exit(if success { 0 } else { 72 });
}
fn cases() -> Vec<String> {
    let mut cases: Vec<String> = [
        "success",
        "panic:copy-step",
        "invalid",
        "busy",
        "full",
        "step-ioerr",
        "finish-ioerr",
        "rename-error",
        "cancel",
        "close-busy",
        "close-error",
        "legacy-p",
        "legacy-q",
        "sidecar",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for point in [
        "before-input",
        "input-verified",
        "protection-durable",
        "sync-recovery-created",
        "before-marker",
        "sync-marker-file",
        "sync-marker-directory",
        "sync-marker-root",
        "marker-durable",
        "before-copy",
        "before-copy-step",
        "copy-step",
        "copy-done",
        "before-close",
        "all-closed",
        "sync-candidate-file",
        "sync-candidate-directory",
        "before-replace",
        "after-replace",
        "sync-source-directory",
        "sync-active-directory",
        "sync-active-final",
        "disk-verified",
    ] {
        cases.push(format!("crash:{point}"));
        cases.push(format!("fail:{point}"));
    }
    cases
}
fn output(root: &Path, m: &str) -> std::process::Output {
    command(root, m).output().unwrap()
}
fn assert_exit(status: std::process::ExitStatus, m: &str) {
    if m == "close-busy" || m == "close-error" || m.starts_with("panic:") {
        assert_eq!(status.signal(), Some(libc::SIGABRT), "{m}: {status}");
    } else {
        assert_eq!(
            status.code(),
            Some(if m == "success" {
                0
            } else if m.starts_with("crash:") {
                88
            } else {
                72
            }),
            "{m}: {status}"
        );
    }
}
#[test]
fn subprocess_activation_matrix() {
    for m in cases() {
        let temp = tempfile::Builder::new()
            .prefix("MNO-activation-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let root = temp.path();
        let out = output(root, &m);
        eprintln!(
            "case {m}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_exit(out.status, &m);
        let events = fs::read_to_string(root.join("events")).unwrap_or_default();
        if let Some(point) = m.strip_prefix("fail:") {
            assert!(
                events.lines().any(|name| name == point),
                "fault checkpoint never reached: {m}: {events}"
            );
        }
        if let Some(p) = m.strip_prefix("crash:") {
            assert_eq!(events.lines().last(), Some(p), "{m}: {events}");
        } else if let Some(p) = m.strip_prefix("panic:") {
            assert_eq!(events.lines().last(), Some(p));
        } else if m == "close-busy" || m == "close-error" {
            assert_eq!(
                events.lines().last(),
                Some(if m == "close-busy" {
                    "close-busy-terminal"
                } else {
                    "close-error-terminal"
                })
            );
        } else {
            assert_eq!(
                events.lines().last(),
                Some("child-checked"),
                "{m}: {} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        // No SQLite inspection before the REAL startup decision. Observe raw files first.
        assert_eq!(
            fs::read(root.join("incoming.mno-backup")).unwrap(),
            fs::read(root.join("archive-before")).unwrap(),
            "{m}"
        );
        let data = root.join("data");
        let target = data.join("mno.sqlite");
        if events.lines().any(|v| v == "marker-durable") {
            assert!(
                data.join("recovery").join(MARKER).is_file(),
                "lost activation marker: {m}"
            );
        }
        if !["legacy-p", "legacy-q"].contains(&m.as_str()) {
            assert!(!data.join("recovery/pending-restore.json").exists());
            assert!(!data.join(".mno-restore-completion-v1.json").exists());
        }
        let swapped = events.lines().any(|s| s == "after-replace");
        if !swapped {
            assert_eq!(
                fs::read(&target).unwrap(),
                fs::read(root.join("old-bytes")).unwrap(),
                "old bytes changed {m}"
            );
        }
        let state = DatabaseState::default();
        let startup = state.initialize_legacy_startup(&data);
        let blocked =
            data.join("recovery").exists() || ["legacy-q", "sidecar"].contains(&m.as_str());
        if blocked {
            assert!(startup.is_err(), "startup admitted {m}");
            assert!(state.operation(None).is_err());
        } else {
            assert!(
                startup.is_ok(),
                "startup blocked unexpectedly {m}: {startup:?}"
            );
        }
        let expected_old = verify_file(&root.join("old-bytes")).unwrap();
        let expected_new = verify_file(&root.join("source.sqlite")).unwrap();
        if root.join("protection-before").exists() {
            let entries: Vec<_> = fs::read_dir(data.join("backups/pre-restore"))
                .unwrap()
                .collect::<std::io::Result<Vec<_>>>()
                .unwrap()
                .into_iter()
                .filter(|e| e.path().extension().is_some_and(|v| v == "mno-backup"))
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1);
            assert_eq!(
                fs::read(entries[0].path()).unwrap(),
                fs::read(root.join("protection-before")).unwrap(),
                "protection bytes changed {m}"
            );
            let candidate = backup::restore::prepare_candidate(
                &entries[0].path(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(
                fingerprint(&candidate, Arc::new(AtomicBool::new(false))).unwrap(),
                expected_old,
                "protection is not original journal: {m}"
            );
            match candidate.try_close() {
                Ok(files) => drop(files),
                Err(_) => panic!("protection close"),
            }
        }
        if data.join("recovery").join(MARKER).exists() {
            let marker = read_marker(&data).unwrap();
            assert_eq!(marker.old_content, expected_old);
            assert_eq!(marker.new_content, expected_new);
            let session = state.recovery_session().unwrap();
            assert_eq!(
                inspect(&session).unwrap(),
                if swapped {
                    Observed::New
                } else {
                    Observed::Old
                },
                "{m}"
            );
            drop(session);
            let protection = data
                .join("backups/pre-restore")
                .join(&marker.protection_file);
            let before = fs::read(&protection).unwrap();
            let verified =
                backup::restore::prepare_candidate(&protection, Arc::new(AtomicBool::new(false)))
                    .unwrap();
            assert_eq!(
                fingerprint(&verified, Arc::new(AtomicBool::new(false))).unwrap(),
                marker.old_content,
                "{m}"
            );
            match verified.try_close() {
                Ok(v) => drop(v),
                Err(_) => panic!("protection close"),
            }
            assert_eq!(fs::read(protection).unwrap(), before);
        }
        if m == "legacy-p" {
            assert_eq!(
                fs::read(data.join("recovery/pending-restore.json")).unwrap(),
                b"legacy P retained"
            );
        }
        if m == "legacy-q" {
            assert_eq!(
                fs::read(data.join(".mno-restore-completion-v1.json")).unwrap(),
                b"legacy Q retained"
            );
        }
        if m == "sidecar" {
            assert_eq!(
                fs::read(data.join("mno.sqlite-wal")).unwrap(),
                b"blocked sidecar"
            );
        }
        drop(state);
        eprintln!("activation case passed: {m}");
    }
}
#[test]
fn exact_exit_controls_reject_unrelated_abort_and_missing_checkpoint() {
    // Parent oracle must reject an unrelated abort for ordinary/crash cases,
    // and a swallowed intended abort (exit 77) for close failure cases.
    for (m, code) in [
        ("success", 77),
        ("crash:after-replace", 77),
        ("close-busy", 77),
    ] {
        let status = Command::new("/bin/sh")
            .args(["-c", &format!("exit {code}")])
            .status()
            .unwrap();
        assert!(std::panic::catch_unwind(|| assert_exit(status, m)).is_err());
    }
    let status = Command::new("/bin/sh")
        .args(["-c", "kill -ABRT $$"])
        .status()
        .unwrap();
    assert!(std::panic::catch_unwind(|| assert_exit(status, "success")).is_err());
}
#[test]
fn marker_rejects_unknown_version_and_malformed_record() {
    let temp = tempfile::TempDir::new().unwrap();
    fs::create_dir(temp.path().join("recovery")).unwrap();
    for bytes in [
        b"{}".as_slice(),
        b"{\"version\":2}".as_slice(),
        &vec![b'x'; LIMIT as usize + 1],
    ] {
        fs::write(temp.path().join("recovery").join(MARKER), bytes).unwrap();
        assert!(read_marker(temp.path()).is_err());
    }
}

#[test]
fn import_marker5_requires_exact_schema3_export4_and_proof4_bindings() {
    let id = "00000000-0000-4000-8000-000000000001";
    let hash = "a".repeat(64);
    let marker = serde_json::json!({
        "format": IMPORT_FORMAT,
        "version": 5,
        "operation_id": id,
        "schema_version": 3,
        "old_schema_version": 3,
        "export_format_version": 4,
        "old_content": { "version": 4, "sha256": hash },
        "new_content": { "version": 4, "sha256": "b".repeat(64) },
        "protection_file": "pre-restore-test.mno-backup",
        "protection_sha256": "c".repeat(64),
        "candidate_file": format!("next-{id}.sqlite")
    });
    assert!(parse_marker(&serde_json::to_vec(&marker).unwrap()).is_ok());
    for (pointer, value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/old_schema_version", serde_json::json!(2)),
        ("/export_format_version", serde_json::json!(3)),
        ("/old_content/version", serde_json::json!(3)),
        ("/new_content/version", serde_json::json!(3)),
    ] {
        let mut changed = marker.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            parse_marker(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "accepted changed binding at {pointer}"
        );
    }
}
