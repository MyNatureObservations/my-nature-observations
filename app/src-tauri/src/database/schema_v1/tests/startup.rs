//! Subprocess-only SQLite instrumentation; production startup is never replaced.
use super::*;
use crate::database_state::DatabaseState;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

const CHILD: &str = "schema_v1::tests::startup::scenario_child";
const ROOT_ENV: &str = "MNO_MIGRATION_STARTUP_TEST_ROOT";
const CASE_ENV: &str = "MNO_MIGRATION_STARTUP_TEST_CASE";
const EARLY_SQLITE: i32 = 86;
const CONTRACT_REJECTED: i32 = 87;
const CRASH: i32 = 79;
static OPENS: AtomicUsize = AtomicUsize::new(0);
static FORBID_SQLITE: AtomicBool = AtomicBool::new(false);

// Called by the real bundled SQLite when initializing an actual connection.
// Only atomics and a terminal test exit; no SQLite calls or pointer access.
unsafe extern "C" fn sqlite_opened(
    _: *mut rusqlite::ffi::sqlite3,
    _: *mut *mut std::ffi::c_char,
    _: *const rusqlite::ffi::sqlite3_api_routines,
) -> std::ffi::c_int {
    OPENS.fetch_add(1, Ordering::SeqCst);
    if FORBID_SQLITE.load(Ordering::SeqCst) {
        std::process::exit(EARLY_SQLITE);
    }
    rusqlite::ffi::SQLITE_OK
}

struct Fixture {
    _directory: TempDir,
    root: PathBuf,
    original: Vec<Vec<Value>>,
}
fn established_fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("book");
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root).unwrap();
    assert!(state.operation(Some(1)).is_ok());
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    legacy(&db, true);
    let original = rows(&db);
    db.close().unwrap();
    drop(state);
    fs::write(
        directory.path().join("fixture-kind"),
        b"MNO-1C3-disposable-fixture",
    )
    .unwrap();
    assert!(root.join(".mno-initialized-v1.json").is_file());
    Fixture {
        _directory: directory,
        root: root.canonicalize().unwrap(),
        original,
    }
}
fn database(root: &Path) -> PathBuf {
    root.join("mno.sqlite")
}

// Raw filesystem observation only: never SQLite, no symlink following. Only
// used when the subprocess is dead and this process holds no SQLite connection.
type Physical = BTreeMap<PathBuf, (u8, Vec<u8>, (u64, u64))>;
fn physical(root: &Path) -> Physical {
    use std::os::unix::fs::MetadataExt;
    fn walk(base: &Path, path: &Path, result: &mut Physical) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let value = if meta.file_type().is_symlink() {
                (
                    b'L',
                    fs::read_link(&path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                )
            } else if meta.is_dir() {
                walk(base, &path, result);
                (b'D', Vec::new())
            } else {
                assert!(meta.is_file());
                (b'F', fs::read(&path).unwrap())
            };
            result.insert(
                path.strip_prefix(base).unwrap().to_owned(),
                (value.0, value.1, (meta.dev(), meta.ino())),
            );
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn child(root: &Path, case: &str) -> Output {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(ROOT_ENV, root)
        .env(CASE_ENV, case)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("subprocess timeout: {case}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
fn success(output: &Output) -> bool {
    output.status.success()
        && String::from_utf8_lossy(&output.stdout).contains("MNO-STARTUP-CONTRACT-OK")
}
fn assert_probe(root: &Path, case: &str) {
    let before = physical(root);
    let output = child(root, case);
    assert!(
        success(&output),
        "{case}: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(physical(root), before, "startup altered files: {case}");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn probe(root: &Path, mode: &str, mutation: Option<&str>) {
    let before = physical(root);
    let no_sqlite = matches!(mode, "sidecar" | "recovery" | "marker" | "missing");
    FORBID_SQLITE.store(no_sqlite, Ordering::SeqCst);
    // SAFETY: this registration exists only in this dedicated child process.
    // The callback does not access SQLite handles or modify the extension list.
    unsafe {
        rusqlite::auto_extension::register_auto_extension(sqlite_opened).unwrap();
    }
    if mutation == Some("early-open") {
        let _db = Connection::open(database(root)).unwrap();
    }
    if mutation == Some("delete-sidecar") {
        fs::remove_file(root.join("mno.sqlite-journal")).unwrap();
    }
    let state = DatabaseState::default();
    let result = if mutation == Some("accept-schema-one") {
        // Deliberately faulty TEST initializer: the real startup coordinator
        // now reaches Ready without validating schema 1. The contract below
        // must reject the actual O/M admission, not a fabricated status flag.
        crate::startup::migration_probe::with_initializer(
            |_, _| Ok(()),
            || state.initialize_legacy_startup(root),
        )
    } else {
        state.initialize_legacy_startup(root)
    };
    let operation_allowed = state.operation(Some(1)).is_ok();
    let maintenance_allowed = state.maintenance().is_ok();
    let opens = OPENS.load(Ordering::SeqCst);
    let correct = match mode {
        "ready" => {
            result.as_ref().is_ok_and(|p| p == &database(root))
                && operation_allowed
                && maintenance_allowed
                && opens > 0
        }
        "schema" => {
            result.as_deref().map_err(|error| error.as_str()) == Err("database_schema_incompatible")
                && !operation_allowed
                && !maintenance_allowed
                && opens > 0
        }
        "sidecar" | "recovery" => {
            let reason = if mode == "sidecar" {
                "database_sqlite_sidecar_present"
            } else {
                "database_recovery_artifact_present"
            };
            result.as_deref().map_err(|error| error.as_str()) == Err(reason)
                && !operation_allowed
                && !maintenance_allowed
                && opens == 0
                && crate::startup::DirectoryLease::open_existing(root).is_err()
        }
        "marker" => {
            result.as_deref().map_err(|error| error.as_str())
                == Err("database_initialization_marker_invalid")
                && !operation_allowed
                && !maintenance_allowed
                && opens == 0
        }
        "missing" => {
            result.as_deref().map_err(|error| error.as_str()) == Err("database_missing_established")
                && !operation_allowed
                && !maintenance_allowed
                && opens == 0
        }
        _ => panic!("unknown mode"),
    };
    // Negative controls alter observed acceptance/content only AFTER running the
    // real startup, never a mocked startup or substitute schema validator.
    if matches!(mutation, Some("rewrite-row" | "delete-row")) {
        assert!(correct && mode == "ready");
        let db = Connection::open(database(root)).unwrap();
        db.execute(
            if mutation == Some("delete-row") {
                "DELETE FROM journal_entries"
            } else {
                "UPDATE journal_entries SET title='wrong'"
            },
            [],
        )
        .unwrap();
        db.close().unwrap();
    }
    assert!(rusqlite::auto_extension::cancel_auto_extension(
        sqlite_opened
    ));
    if !correct || physical(root) != before {
        eprintln!(
            "contract rejected mode={mode} result={result:?} O={operation_allowed} M={maintenance_allowed} opens={opens}"
        );
        std::process::exit(CONTRACT_REJECTED);
    }
    println!("MNO-STARTUP-CONTRACT-OK mode={mode} sqlite-opens={opens}");
}

#[test]
fn scenario_child() {
    let Some(root) = std::env::var_os(ROOT_ENV) else {
        return;
    };
    let root = PathBuf::from(root);
    let case = std::env::var(CASE_ENV).unwrap();
    assert_eq!(
        fs::read(root.parent().unwrap().join("fixture-kind")).unwrap(),
        b"MNO-1C3-disposable-fixture"
    );
    assert_eq!(root.file_name().unwrap(), "book");
    if case == "busy-start" {
        let state = DatabaseState::default();
        let result = crate::startup::migration_probe::with_initializer(busy_during_startup, || {
            state.initialize_legacy_startup(&root)
        });
        assert_eq!(result.unwrap_err(), "MNO-test-migration-busy");
        assert!(state.operation(None).is_err());
        assert!(state.maintenance().is_err());
        println!("MNO-STARTUP-CONTRACT-OK mode=busy-start");
        return;
    }
    if case.starts_with("crash-") {
        let state = DatabaseState::default();
        let result =
            crate::startup::migration_probe::with_initializer(migrate_during_startup, || {
                state.initialize_legacy_startup(&root)
            });
        panic!("crash checkpoint was not terminal: {result:?}");
    }
    let (mode, mutation) = case
        .split_once(':')
        .map_or((case.as_str(), None), |(a, b)| (a, Some(b)));
    probe(&root, mode, mutation);
}

#[test]
fn migration_crashes_are_gated_before_sqlite_and_only_then_inspected() {
    for point in ["table", "version", "before-commit", "after-commit"] {
        let fixture = established_fixture();
        let output = child(&fixture.root, &format!("crash-{point}"));
        assert_eq!(
            output.status.code(),
            Some(CRASH),
            "{point}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(fixture.root.parent().unwrap().join("witness")).unwrap(),
            point
        );
        // Observe actual physical state before ANY recovery-capable SQL read.
        let files = physical(&fixture.root);
        assert!(files.contains_key(Path::new("mno.sqlite")));
        let journal = files.get(Path::new("mno.sqlite-journal"));
        let raw = &files[Path::new("mno.sqlite")].1;
        println!(
            "MNO-MIGRATION-STATE point={point} raw-user-version={} journal-bytes={}",
            u32::from_be_bytes(raw[60..64].try_into().unwrap()),
            journal.map_or(0, |j| j.1.len())
        );
        if point == "after-commit" {
            assert!(journal.is_none(), "DELETE commit must remove its journal");
            let raw = &files[Path::new("mno.sqlite")].1;
            assert_eq!(u32::from_be_bytes(raw[60..64].try_into().unwrap()), 1);
            assert_probe(&fixture.root, "schema");
        } else {
            let bytes = &journal.expect("must exercise a real leftover journal").1;
            assert!(bytes.len() > 512);
            assert_eq!(
                &bytes[..8],
                &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7],
                "must exercise a hot rollback journal, not an empty sidecar"
            );
            assert_probe(&fixture.root, "sidecar");
        }
        // Deliberately separate phase AFTER real startup has been checked.
        // This may replay SQLite's hot journal in this disposable fixture only.
        let mut db = Connection::open(database(&fixture.root)).unwrap();
        assert_eq!(rows(&db), fixture.original);
        if point == "after-commit" {
            assert_eq!(version(&db), 1);
            validate(&mut db).unwrap();
        } else {
            assert_legacy(&db, &fixture.original);
        }
        db.close().unwrap();
        assert_probe(
            &fixture.root,
            if point == "after-commit" {
                "schema"
            } else {
                "ready"
            },
        );
    }
}

#[test]
fn established_startup_really_opens_sqlite_and_keeps_original_rows() {
    let fixture = established_fixture();
    fs::write(
        fixture.root.join("user-notes.txt"),
        b"unrelated files are permitted",
    )
    .unwrap();
    assert_probe(&fixture.root, "ready");
    let db = Connection::open(database(&fixture.root)).unwrap();
    assert_eq!(rows(&db), fixture.original);
    db.close().unwrap();
}

#[test]
fn reserved_artifacts_and_sidecar_types_block_without_sqlite_or_cleanup() {
    for (name, mode) in [
        ("recovery", "recovery"),
        ("pending-restore.json", "recovery"),
        (".mno-restore-completion-v1.json", "recovery"),
        (".mno-restore-completion-v2.json", "recovery"),
        (".mno-init-orphan", "recovery"),
        (".pending-orphan", "recovery"),
        (".mno-completion-orphan", "recovery"),
        ("RECOVERY", "recovery"),
        ("mno.sqlite-journal", "sidecar"),
        ("mno.sqlite-wal", "sidecar"),
        ("mno.sqlite-shm", "sidecar"),
    ] {
        for directory in [false, true] {
            let fixture = established_fixture();
            let path = fixture.root.join(name);
            if directory {
                fs::create_dir(&path).unwrap();
            } else {
                fs::write(&path, b"unknown partial data").unwrap();
            }
            assert_probe(&fixture.root, mode);
        }
    }
    for name in ["recovery", "mno.sqlite-wal"] {
        let fixture = established_fixture();
        let outside = fixture.root.parent().unwrap().join("untouched");
        fs::write(&outside, b"outside target").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.join(name)).unwrap();
        assert_probe(
            &fixture.root,
            if name == "recovery" {
                "recovery"
            } else {
                "sidecar"
            },
        );
        assert_eq!(fs::read(outside).unwrap(), b"outside target");
    }
}

#[test]
fn schema_one_unknown_and_partial_schemas_are_not_repaired_or_admitted() {
    for scenario in ["one", "unknown", "extra", "missing-index"] {
        let fixture = established_fixture();
        let mut db = Connection::open(database(&fixture.root)).unwrap();
        match scenario {
            "one" => migrate(&mut db).unwrap(),
            "unknown" => db.pragma_update(None, "user_version", 99).unwrap(),
            "extra" => db.execute_batch("CREATE TABLE alien(value TEXT)").unwrap(),
            "missing-index" => db
                .execute_batch("DROP INDEX idx_journal_entries_date")
                .unwrap(),
            _ => unreachable!(),
        }
        db.close().unwrap();
        assert_probe(&fixture.root, "schema");
        let db = Connection::open(database(&fixture.root)).unwrap();
        assert_eq!(rows(&db), fixture.original);
        db.close().unwrap();
    }
}

#[test]
fn initialization_marker_never_proves_migration_and_missing_database_is_not_recreated() {
    let fixture = established_fixture();
    fs::write(
        fixture.root.join(".mno-initialized-v1.json"),
        b"{\"format\":\"mno-initialized\",\"version\":2}\n",
    )
    .unwrap();
    assert_probe(&fixture.root, "marker");
    let fixture = established_fixture();
    fs::remove_file(database(&fixture.root)).unwrap();
    assert_probe(&fixture.root, "missing");
    assert!(!database(&fixture.root).exists());
}

#[test]
fn busy_migration_start_preserves_established_schema_and_startup() {
    let fixture = established_fixture();
    let other = Connection::open(database(&fixture.root)).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let output = child(&fixture.root, "busy-start");
    assert!(
        success(&output),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("MNO-BUSY-INITIALIZER"));
    other.execute_batch("ROLLBACK").unwrap();
    other.close().unwrap();
    let db = Connection::open(database(&fixture.root)).unwrap();
    assert_legacy(&db, &fixture.original);
    db.close().unwrap();
    assert_probe(&fixture.root, "ready");
}

#[test]
fn negative_controls_cannot_pass_via_later_ready_or_cleanup() {
    for mutation in ["early-open", "delete-sidecar"] {
        let fixture = established_fixture();
        fs::write(
            fixture.root.join("mno.sqlite-journal"),
            b"blocking evidence",
        )
        .unwrap();
        let output = child(&fixture.root, &format!("sidecar:{mutation}"));
        assert_eq!(output.status.code(), Some(EARLY_SQLITE));
        assert!(!success(&output));
    }
    let fixture = established_fixture();
    fs::write(fixture.root.join("recovery"), b"unknown").unwrap();
    let output = child(&fixture.root, "ready"); // deliberately wrong expected verdict
    assert_eq!(output.status.code(), Some(CONTRACT_REJECTED));
    assert!(!success(&output));
    let fixture = established_fixture();
    let mut db = Connection::open(database(&fixture.root)).unwrap();
    migrate(&mut db).unwrap();
    db.close().unwrap();
    let output = child(&fixture.root, "schema:accept-schema-one");
    assert_eq!(output.status.code(), Some(CONTRACT_REJECTED));
    assert!(!success(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains("O=true M=true"));
    for mutation in ["rewrite-row", "delete-row"] {
        let fixture = established_fixture();
        let output = child(&fixture.root, &format!("ready:{mutation}"));
        assert_eq!(output.status.code(), Some(CONTRACT_REJECTED));
        assert!(!success(&output));
    }
}

// The override changes only the initializer in this child. All real startup
// checks, lease acquisition and DatabaseState publication remain on their path.
fn migrate_during_startup(path: &Path, new: bool) -> std::result::Result<(), String> {
    assert!(!new, "must be an established installation");
    let root = path.parent().unwrap();
    assert!(crate::startup::DirectoryLease::open_existing(root).is_err());
    let mut db = Connection::open(path).unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=1; PRAGMA cache_spill=ON;").unwrap();
    let case = std::env::var(CASE_ENV).unwrap();
    let point = case.strip_prefix("crash-").unwrap().to_owned();
    migrate_with(&mut db, |here, _| {
        let hit = match point.as_str() {
            "table" => here == Point::AfterTable,
            "version" => here == Point::AfterVersion,
            "before-commit" => here == Point::BeforeCommit,
            "after-commit" => false,
            _ => panic!("unknown checkpoint"),
        };
        if hit {
            fs::write(root.parent().unwrap().join("witness"), &point).unwrap();
            std::process::exit(CRASH);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(point, "after-commit");
    // Successful test-only commit, but no normal startup/Ready decision.
    fs::write(root.parent().unwrap().join("witness"), &point).unwrap();
    std::process::exit(CRASH);
}

#[test]
fn initializer_adapter_respects_prior_barriers_and_resets_after_unwind() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn fail_if_reached(_: &Path, _: bool) -> std::result::Result<(), String> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        panic!("test initializer reached");
    }
    let fixture = established_fixture();
    let before = physical(&fixture.root);
    fs::write(fixture.root.join("mno.sqlite-shm"), b"blocked").unwrap();
    let state = DatabaseState::default();
    let outcome = crate::startup::migration_probe::with_initializer(fail_if_reached, || {
        state.initialize_legacy_startup(&fixture.root)
    });
    assert_eq!(outcome.unwrap_err(), "database_sqlite_sidecar_present");
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    assert!(state.operation(None).is_err());
    drop(state);
    fs::remove_file(fixture.root.join("mno.sqlite-shm")).unwrap();
    let state = DatabaseState::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            crate::startup::migration_probe::with_initializer(fail_if_reached, || {
                state.initialize_legacy_startup(&fixture.root)
            })
        }))
        .is_err()
    );
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    drop(state);
    assert_eq!(physical(&fixture.root), before);
    // Child is a fresh process (adapter cannot leak there); this same-thread
    // call additionally proves scoped reset after the caught unwind.
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&fixture.root).unwrap();
    assert!(state.operation(Some(1)).is_ok());
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(physical(&fixture.root), before);
}

fn busy_during_startup(path: &Path, new: bool) -> std::result::Result<(), String> {
    assert!(!new);
    assert!(crate::startup::DirectoryLease::open_existing(path.parent().unwrap()).is_err());
    let mut db = Connection::open(path).unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    assert!(matches!(migrate(&mut db), Err(Error::Sql(ref e))
        if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)));
    assert!(db.is_autocommit());
    db.close().unwrap();
    println!("MNO-BUSY-INITIALIZER");
    Err("MNO-test-migration-busy".into())
}
