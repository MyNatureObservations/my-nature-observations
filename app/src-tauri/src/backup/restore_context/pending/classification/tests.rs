use super::*;
use crate::{
    backup::{self, BackupKind},
    database_state::DatabaseState,
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
use tempfile::TempDir;
pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) protection: PathBuf,
    pub(super) _dir: TempDir,
}
pub(super) fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
impl Fixture {
    pub(super) fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let state = DatabaseState::default();
        let path = state
            .initialize_legacy_startup(&dir.path().join("book"))
            .unwrap();
        let root = path.parent().unwrap().to_owned();
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES ('entry','2026-10-18','Old',?1,1,'created','updated')",[r#"{"type":"doc","content":[{"type":"paragraph"}]}"#]).unwrap();
        drop(db);
        let incoming = dir.path().join("new.sqlite");
        fs::copy(&path, &incoming).unwrap();
        let db = Connection::open(&incoming).unwrap();
        db.execute("UPDATE journal_entries SET title='New'", [])
            .unwrap();
        let archive = dir.path().join("incoming.mno-backup");
        backup::create_backup(&db, &archive, BackupKind::Manual).unwrap();
        drop(db);
        let candidate = restore::prepare_candidate(&archive, flag()).unwrap();
        let protection =
            super::super::super::with_active(&state, &path, candidate, flag(), |mut ctx| {
                let auth = ctx.authorize()?;
                let marker = auth.publish_pending().unwrap();
                drop(marker);
                Ok(auth.protection_path().to_owned())
            })
            .unwrap();
        drop(state);
        Self {
            root,
            path,
            protection,
            _dir: dir,
        }
    }
    pub(super) fn change_marker(&self, key: &str, value: serde_json::Value) {
        let path = self.root.join(DIRECTORY).join(NAME);
        let mut data: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        data[key] = value;
        fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    }
    pub(super) fn title(&self, title: &str) {
        Connection::open(&self.path)
            .unwrap()
            .execute("UPDATE journal_entries SET title=?1", [title])
            .unwrap();
    }
}
pub(super) fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.is_dir() {
            result.extend(tree(&path));
        } else if metadata.is_file() {
            result.insert(path.clone(), fs::read(path).unwrap());
        } else if metadata.file_type().is_symlink() {
            result.insert(
                path.clone(),
                fs::read_link(path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            );
        }
    }
    result
}
#[test]
fn old_and_new_are_verified_without_modifying_any_input() {
    for (title, expected) in [("Old", Status::Old), ("New", Status::New)] {
        let f = Fixture::new();
        if title == "New" {
            f.title(title);
        }
        let before = tree(&f.root);
        let result = classify(&f.root, flag());
        assert_eq!(result.status(), expected);
        assert_eq!(result.reason(), None);
        assert_eq!(tree(&f.root), before);
        // Successful observations retain the process lock until they are dropped.
        assert_eq!(classify(&f.root, flag()).status(), Status::Uncertain);
        drop(result);
        assert_eq!(tree(&f.root), before);
        assert!(
            DatabaseState::default()
                .initialize_legacy_startup(&f.root)
                .is_err()
        );
    }
}
#[test]
fn content_identity_archive_schema_and_marker_failures_are_uncertain() {
    for case in 0..15 {
        let f = Fixture::new();
        match case {
            0=>f.title("Neither old nor new"),
            1=>fs::remove_file(&f.path).unwrap(),
            2=>{let old=f.root.join("old.sqlite");fs::rename(&f.path,&old).unwrap();fs::copy(old,&f.path).unwrap();},
            3=>f.change_marker("directory_identity",serde_json::json!({"platform":"unix","volume":"0000000000000000","file":"0000000000000000"})),
            4=>fs::remove_file(&f.protection).unwrap(),
            5=>f.change_marker("protection_sha256",serde_json::json!("0".repeat(64))),
            6=>f.change_marker("old_content",serde_json::json!({"version":1,"sha256":"0".repeat(64)})),
            7=>{Connection::open(&f.path).unwrap().execute_batch("DROP INDEX idx_journal_entries_date").unwrap();},
            8=>fs::write(&f.path,b"corrupt database").unwrap(),
            9=>f.change_marker("state",serde_json::json!("done")),
            10=>{f.title("New");f.change_marker("new_content",serde_json::json!({"version":1,"sha256":"0".repeat(64)}));},
            11=>{fs::write(&f.protection,b"bad archive").unwrap();let digest=backup::digest_file(&f.protection).unwrap();f.change_marker("protection_sha256",serde_json::json!(digest));},
            12=>{fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();},
            _=>f.change_marker("database_path",serde_json::json!("../mno.sqlite")),
        }
        let before = tree(&f.root);
        let result = classify(&f.root, flag());
        assert_eq!(result.status(), Status::Uncertain, "case {case}");
        assert!(result.reason().is_some());
        assert_eq!(tree(&f.root), before, "case {case}");
    }
}
#[test]
fn symlinks_and_missing_inputs_are_never_followed_or_created() {
    for case in 0..5 {
        let f = Fixture::new();
        let external = TempDir::new().unwrap();
        match case {
            0 => {
                let copy = external.path().join("db");
                fs::rename(&f.path, &copy).unwrap();
                std::os::unix::fs::symlink(copy, &f.path).unwrap();
            }
            1 => {
                let copy = external.path().join("backup");
                fs::rename(&f.protection, &copy).unwrap();
                std::os::unix::fs::symlink(copy, &f.protection).unwrap();
            }
            2 => {
                let path = f.root.join("backups/pre-restore");
                let copy = external.path().join("backups");
                fs::rename(&path, &copy).unwrap();
                std::os::unix::fs::symlink(copy, path).unwrap();
            }
            3 => {
                let path = f.root.join(DIRECTORY).join(NAME);
                let copy = external.path().join("marker");
                fs::rename(&path, &copy).unwrap();
                std::os::unix::fs::symlink(copy, path).unwrap();
            }
            _ => {
                fs::remove_file(f.root.join(".mno-data.lock")).unwrap();
            }
        }
        let before = tree(&f.root);
        let outside = tree(external.path());
        assert_eq!(classify(&f.root, flag()).status(), Status::Uncertain);
        assert_eq!(tree(&f.root), before);
        assert_eq!(tree(external.path()), outside);
    }
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing");
    assert_eq!(classify(&missing, flag()).status(), Status::Uncertain);
    assert!(!missing.exists());
}
#[test]
fn process_lock_and_external_writer_prevent_unsafe_classification() {
    let f = Fixture::new();
    let lock = open_plain(&f.root.join(".mno-data.lock"), false).unwrap();
    FileExt::try_lock_exclusive(&lock).unwrap();
    assert_eq!(classify(&f.root, flag()).reason(), Some(Reason::Busy));
    drop(lock);
    for write in [false, true] {
        let writer = Connection::open(&f.path).unwrap();
        writer.execute_batch("BEGIN EXCLUSIVE").unwrap();
        if write {
            writer
                .execute("UPDATE journal_entries SET title='Not committed'", [])
                .unwrap();
        }
        let before = tree(&f.root);
        assert_eq!(classify(&f.root, flag()).status(), Status::Uncertain);
        assert_eq!(tree(&f.root), before);
        writer.execute_batch("ROLLBACK").unwrap();
    }
}
#[test]
fn writer_commits_during_inspection_are_detected_and_snapshot_blocks_later_commits() {
    let f = Fixture::new();
    let mut changed = false;
    let result = classify_with(&f.root, flag(), |point| {
        if point == Point::BeforeSqlite {
            f.title("New");
            changed = true;
        }
    });
    assert!(changed);
    assert_eq!(result.status(), Status::Uncertain);
    drop(result);
    f.title("Old");
    let result = classify(&f.root, flag());
    assert_eq!(result.status(), Status::Old);
    let writer = Connection::open(&f.path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='New'")
        .unwrap();
    assert!(writer.execute_batch("COMMIT").is_err());
    drop(result);
    writer.execute_batch("COMMIT").unwrap();
    drop(writer);
    assert_eq!(classify(&f.root, flag()).status(), Status::New);
}
#[test]
fn input_changes_after_snapshot_and_cancel_are_uncertain() {
    for point in [Point::Snapshot, Point::Final] {
        let f = Fixture::new();
        let result = classify_with(&f.root, flag(), |at| {
            if at == point {
                fs::write(&f.protection, b"changed during check").unwrap();
            }
        });
        assert_eq!(result.status(), Status::Uncertain);
    }
    let f = Fixture::new();
    let cancel = flag();
    cancel.store(true, Ordering::Release);
    assert_eq!(classify(&f.root, cancel).reason(), Some(Reason::Cancelled));
}
#[test]
fn rollback_wal_and_shm_artifacts_are_conservatively_blocked_before_sqlite() {
    for suffix in ["-journal", "-wal", "-shm"] {
        let f = Fixture::new();
        fs::write(
            f.root.join(format!("mno.sqlite{suffix}")),
            b"uncertain state",
        )
        .unwrap();
        let before = tree(&f.root);
        let mut opened = false;
        let result = classify_with(&f.root, flag(), |p| {
            if p == Point::BeforeSqlite {
                opened = true;
            }
        });
        assert_eq!(result.reason(), Some(Reason::SqliteState));
        assert!(!opened);
        assert_eq!(tree(&f.root), before);
    }
}
#[test]
fn hot_child() {
    let Some(path) = std::env::var_os("MNO_CLASSIFIER_HOT_CHILD") else {
        return;
    };
    let path = PathBuf::from(path);
    assert!(path.parent().unwrap().join(DIRECTORY).join(NAME).is_file());
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    db.execute_batch(
        "PRAGMA synchronous=FULL; PRAGMA cache_size=1; PRAGMA cache_spill=ON; BEGIN IMMEDIATE",
    )
    .unwrap();
    db.execute(
        "UPDATE journal_entries SET title=?1",
        ["x".repeat(256 * 1024)],
    )
    .unwrap();
    println!("HOT_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
#[test]
fn real_hot_rollback_journal_is_not_recovered_or_modified() {
    let f = Fixture::new();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "backup::restore_context::pending::classification::tests::hot_child",
            "--nocapture",
        ])
        .env("MNO_CLASSIFIER_HOT_CHILD", &f.path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut ready = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        };
        if line.trim() == "HOT_READY" {
            ready = true;
            break;
        }
    }
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        panic!("hot-journal child failed before handshake");
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let journal = fs::read(f.root.join("mno.sqlite-journal")).unwrap();
    assert!(journal.len() > 512);
    assert_eq!(
        &journal[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    let before = tree(&f.root);
    let mut sqlite_attempted = false;
    let result = classify_with(&f.root, flag(), |p| {
        if p == Point::BeforeSqlite {
            sqlite_attempted = true;
        }
    });
    assert_eq!(result.reason(), Some(Reason::SqliteState));
    assert!(!sqlite_attempted);
    assert_eq!(tree(&f.root), before);
    // Independent SQLite read-only probe confirms that actual recovery would
    // require writes. No read-write recovery connection is ever opened here.
    let db = Connection::open_with_flags(&f.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let error = db
        .query_row("SELECT count(*) FROM journal_entries", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_err();
    assert!(
        matches!(error,rusqlite::Error::SqliteFailure(e,_) if e.extended_code==rusqlite::ffi::SQLITE_READONLY_ROLLBACK)
    );
    drop(db);
    assert_eq!(tree(&f.root), before);
}
#[test]
fn compiler_prevents_status_or_flags_from_becoming_a_verified_assessment() {
    let dir = TempDir::new().unwrap();
    let api = capability_probe(dir.path());
    let common = format!(
        "mod classification {{ pub(super) mod completion {{ #[derive(Debug)] pub struct ReceiptError; }} pub(super) mod durability {{}} pub(super) struct SyncPoint; pub(super) struct Observation; pub(super) struct Access<'a, 's>(&'a (), &'s ()); impl Access<'_, '_>{{fn is_session(&self)->bool{{true}}}} impl Observation{{fn recheck(&self,_:&Access<'_, '_>)->Result<(),result::Reason>{{Ok(())}}}} #[path={:?}] pub(crate) mod result; pub fn borrowed(owner:&())->result::Assessment<'_, '_>{{result::Assessment::uncertain(result::Reason::Io).retain(Access(owner,owner))}} }}",
        api
    );
    let compile = |probe: &str| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, format!("{common}\n{probe}")).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg(path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let ok = compile(
        "fn inspect(a:&classification::result::Assessment) { let _=a.status(); let _=a.reason(); }",
    );
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let positive_borrow = compile(
        "fn observe(){let owner=();let assessment=classification::borrowed(&owner);let _=assessment.status();}",
    );
    assert!(
        positive_borrow.status.success(),
        "{}",
        String::from_utf8_lossy(&positive_borrow.stderr)
    );
    for (probe, code) in [
        (
            "fn implicit_drop(){let owner=Box::new(());let _a=classification::borrowed(&owner);drop(owner);}",
            "E0505",
        ),
        (
            "fn escape(owner:&())->classification::result::Assessment<'static, 'static>{classification::borrowed(owner)}",
            "lifetime may not live long enough",
        ),
        (
            "fn drop_owner(){let owner=Box::new(());let a=classification::borrowed(&owner);drop(owner);let _=a.status();}",
            "E0505",
        ),
        (
            "fn duplicate(a:classification::result::Assessment<'_, '_>){let _=a.clone();}",
            "E0599",
        ),
        (
            "fn forge() { let _:classification::result::Assessment=classification::result::Status::Uncertain; }",
            "E0308",
        ),
        (
            "fn forge() { let _=classification::result::Assessment {status:classification::result::Status::Old,reason:None,observation:None,access:None}; }",
            "E0451",
        ),
        (
            "fn forge() { let _=classification::result::Assessment::uncertain(classification::result::Reason::Io); }",
            "E0624",
        ),
    ] {
        let out = compile(probe);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && stderr.contains(code), "{stderr}");
    }
}

#[test]
fn a_wal_mode_switch_race_does_not_create_or_modify_sqlite_sidecars() {
    let f = Fixture::new();
    let mut before = None;
    let result = classify_with(&f.root, flag(), |point| {
        if point == Point::BeforeSqlite {
            let writer = Connection::open(&f.path).unwrap();
            let mode: String = writer
                .pragma_query_value(None, "journal_mode", |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "delete");
            writer.pragma_update(None, "journal_mode", "WAL").unwrap();
            drop(writer);
            assert!(!f.root.join("mno.sqlite-wal").exists());
            assert!(!f.root.join("mno.sqlite-shm").exists());
            before = Some(tree(&f.root));
        }
    });
    assert_eq!(result.status(), Status::Uncertain);
    assert_eq!(tree(&f.root), before.unwrap());
}

#[test]
fn a_changed_operation_id_during_observation_is_not_accepted() {
    let f = Fixture::new();
    let result = classify_with(&f.root, flag(), |point| {
        if point == Point::Snapshot {
            f.change_marker(
                "restore_id",
                serde_json::json!(uuid::Uuid::new_v4().to_string()),
            );
        }
    });
    assert_eq!(result.status(), Status::Uncertain);
    assert_eq!(result.reason(), Some(Reason::Marker));
}

#[test]
fn reproduced_old_and_new_swap_back_sequences_never_approve_foreign_content() {
    for replacement in ["Old", "New"] {
        for restore_at in [Point::AfterSqliteOpen, Point::Snapshot] {
            let f = Fixture::new();
            f.title("Neither old nor new");
            let before = fs::read(&f.path).unwrap();
            let foreign = f.root.join("foreign.sqlite");
            fs::copy(&f.path, &foreign).unwrap();
            let db = Connection::open(&foreign).unwrap();
            db.execute("UPDATE journal_entries SET title=?1", [replacement])
                .unwrap();
            drop(db);
            let saved = f.root.join("saved-active.sqlite");
            let mut swapped = false;
            let mut restored = false;
            let result = classify_with(&f.root, flag(), |point| {
                if point == Point::BeforeSqlite {
                    fs::rename(&f.path, &saved).unwrap();
                    fs::rename(&foreign, &f.path).unwrap();
                    swapped = true;
                }
                if point == restore_at {
                    fs::rename(&f.path, &foreign).unwrap();
                    fs::rename(&saved, &f.path).unwrap();
                    restored = true;
                }
            });
            assert!(swapped);
            assert_eq!(result.status(), Status::Uncertain);
            drop(result);
            // An early identity rejection can occur before the restore hook.
            if !restored {
                fs::rename(&f.path, &foreign).unwrap();
                fs::rename(&saved, &f.path).unwrap();
            }
            assert_eq!(fs::read(&f.path).unwrap(), before);
        }
    }
}

#[test]
fn native_sqlite_reads_the_retained_inode_even_if_the_name_already_points_elsewhere() {
    for replacement in ["Old", "New"] {
        let f = Fixture::new();
        f.title("Neither old nor new");
        let retained = Arc::new(open_plain(&f.path, false).unwrap());
        let foreign = f.root.join("foreign.sqlite");
        fs::copy(&f.path, &foreign).unwrap();
        let db = Connection::open(&foreign).unwrap();
        db.execute("UPDATE journal_entries SET title=?1", [replacement])
            .unwrap();
        drop(db);
        let saved = f.root.join("saved-active.sqlite");
        fs::rename(&f.path, &saved).unwrap();
        fs::rename(&foreign, &f.path).unwrap();
        let mut bound = BoundSqlite::open(&f.path, retained, Control::Real).unwrap();
        let title: String = bound
            .connection_mut()
            .query_row("SELECT title FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Neither old nor new");
        bound.verify().unwrap();
        // Initial physical binding is proved independently of a same-name check.
        drop(bound);
        fs::rename(&f.path, &foreign).unwrap();
        fs::rename(&saved, &f.path).unwrap();
    }
}

#[test]
fn replacement_missing_file_and_symlink_at_each_identity_boundary_are_uncertain() {
    for point in [
        Point::BeforeSqlite,
        Point::AfterSqliteOpen,
        Point::Snapshot,
        Point::Final,
        Point::IdentityCheck,
    ] {
        for kind in 0..3 {
            let f = Fixture::new();
            let saved = f.root.join("saved-active.sqlite");
            let before = fs::read(&f.path).unwrap();
            let result = classify_with(&f.root, flag(), |at| {
                if at == point {
                    fs::rename(&f.path, &saved).unwrap();
                    match kind {
                        0 => {
                            fs::copy(&saved, &f.path).unwrap();
                        }
                        1 => (),
                        _ => std::os::unix::fs::symlink(&saved, &f.path).unwrap(),
                    }
                }
            });
            assert_eq!(result.status(), Status::Uncertain);
            drop(result);
            assert_eq!(fs::read(&saved).unwrap(), before);
        }
    }
}

#[test]
fn unsupported_or_failed_native_identity_control_never_returns_an_assessment_approval() {
    for control in [Control::Unsupported, Control::Error] {
        let f = Fixture::new();
        let before = tree(&f.root);
        let result = classify_with_control(&f.root, flag(), |_| {}, control);
        assert_eq!(result.status(), Status::Uncertain);
        assert_eq!(result.reason(), Some(Reason::Identity));
        drop(result);
        assert_eq!(tree(&f.root), before);
        // Native error cleanup closes handles and unregisters the private VFS.
        assert_eq!(classify(&f.root, flag()).status(), Status::Old);
    }
}

#[test]
fn identity_control_errors_before_and_after_reading_are_always_uncertain() {
    for code in [rusqlite::ffi::SQLITE_NOTFOUND, rusqlite::ffi::SQLITE_IOERR] {
        for call in 0..6 {
            let f = Fixture::new();
            let before = tree(&f.root);
            let result =
                classify_with_control(&f.root, flag(), |_| {}, Control::FailAt { call, code });
            assert_eq!(
                result.status(),
                Status::Uncertain,
                "call {call}, code {code}"
            );
            assert_eq!(result.reason(), Some(Reason::Identity));
            drop(result);
            assert_eq!(tree(&f.root), before);
            assert_eq!(classify(&f.root, flag()).status(), Status::Old);
        }
    }
}

#[test]
fn file_pointer_errors_null_and_foreign_pointers_are_uncertain() {
    for control in [
        Control::FilePointerError,
        Control::FilePointerUnsupported,
        Control::FilePointerNull,
        Control::FilePointerUnexpected,
    ] {
        let f = Fixture::new();
        let before = tree(&f.root);
        let result = classify_with_control(&f.root, flag(), |_| {}, control);
        assert_eq!(result.status(), Status::Uncertain);
        assert_eq!(result.reason(), Some(Reason::Identity));
        drop(result);
        assert_eq!(tree(&f.root), before);
        assert_eq!(classify(&f.root, flag()).status(), Status::Old);
    }
}

#[path = "session_tests.rs"]
mod session_tests;
