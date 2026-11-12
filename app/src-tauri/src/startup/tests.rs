use super::*;
use crate::database_state::DatabaseState;
use std::{
    cell::Cell,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};
use tempfile::TempDir;
#[test]
fn pure_resolution_does_not_create_directory_or_database() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("absent");
    assert_eq!(
        database::expected_database_path(&root),
        root.join("mno.sqlite")
    );
    assert!(!root.exists());
}
#[test]
fn normal_first_startup_initializes_and_holds_lock_until_state_drops() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("new").join("book");
    let state = DatabaseState::default();
    assert_eq!(
        state.operation(None).err().unwrap(),
        "database_startup_blocked"
    );
    let path = state.initialize_startup(&root).unwrap();
    assert!(path.is_file());
    assert!(state.operation(None).is_ok());
    let mut db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        if cfg!(target_os = "macos") { 5 } else { 4 }
    );
    #[cfg(target_os = "macos")]
    crate::storage_content::verify_v6(&mut db, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    #[cfg(not(target_os = "macos"))]
    crate::storage_content::verify_v5(&mut db, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    assert!(DirectoryLease::acquire(&root).is_err());
    drop(state);
    assert!(DirectoryLease::acquire(&root).is_ok());
}
#[test]
fn existing_journal_data_is_preserved() {
    let dir = TempDir::new().unwrap();
    let path = database::expected_database_path(dir.path());
    database::create_current_database(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES ('one','2026-10-17','preserve','{}',1,'created','updated')",[]).unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    assert!(!established::read(dir.path()).unwrap());
    let state = DatabaseState::default();
    state.initialize_startup(dir.path()).unwrap();
    #[cfg(not(target_os = "macos"))]
    assert_eq!(fs::read(&path).unwrap(), before);
    #[cfg(target_os = "macos")]
    {
        let attempts = dir.path().join("backups/bird-schema-upgrade");
        let copies: Vec<_> = fs::read_dir(attempts)
            .unwrap()
            .map(|e| e.unwrap().path().join("schema4.sqlite"))
            .collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(fs::read(&copies[0]).unwrap(), before);
        let db = database::open_existing(&path).unwrap();
        assert_eq!(db.query_row("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))).unwrap(),("one".into(),"2026-10-17".into(),"preserve".into(),"{}".into(),1,"created".into(),"updated".into()));
    }
    assert!(established::read(dir.path()).unwrap());
}
#[test]
fn every_reserved_artifact_blocks_before_initializer_and_preserves_existing_data() {
    for kind in 0..3 {
        let dir = TempDir::new().unwrap();
        let database = database::expected_database_path(dir.path());
        fs::write(&database, b"untouched even if not sqlite").unwrap();
        let recovery = dir.path().join("recovery");
        match kind {
            0 => fs::write(&recovery, b"unknown").unwrap(),
            1 => fs::create_dir(&recovery).unwrap(),
            _ => {
                fs::create_dir(&recovery).unwrap();
                fs::write(recovery.join("pending-restore.json.tmp"), b"partial").unwrap();
            }
        }
        let called = Cell::new(false);
        let result = initialize_with(
            dir.path(),
            |p| fs::symlink_metadata(p),
            |_, _| {
                called.set(true);
                Ok(())
            },
        );
        assert!(matches!(result,Err(ref e) if e=="database_recovery_artifact_present"));
        assert!(!called.get());
        assert_eq!(fs::read(database).unwrap(), b"untouched even if not sqlite");
        let state = DatabaseState::default();
        assert!(state.initialize_startup(dir.path()).is_err());
        assert_eq!(
            state.operation(None).err().unwrap(),
            "database_startup_blocked"
        );
        assert!(state.maintenance().is_err());
    }
}
#[test]
fn windows_managed_names_block_before_the_initializer() {
    for name in [
        "generations",
        "generation-selection.json",
        ".generation-selection-staged",
        ".generation-selection-in-progress",
    ] {
        for kind in ["directory", "file", "dangling symlink"] {
            let root = TempDir::new().unwrap();
            let database = database::expected_database_path(root.path());
            fs::write(&database, b"untouched").unwrap();
            let artifact = root.path().join(name);
            match kind {
                "directory" => fs::create_dir(&artifact).unwrap(),
                "file" => fs::write(&artifact, b"malformed").unwrap(),
                _ => std::os::unix::fs::symlink("missing-target", &artifact).unwrap(),
            }
            let result = initialize_with(
                root.path(),
                |p| fs::symlink_metadata(p),
                |_, _| panic!("initializer called for {kind} at {name}"),
            );
            assert!(
                matches!(result, Err(ref e) if e == "database_recovery_artifact_present"),
                "{name}: {kind}"
            );
            assert!(!root.path().join(established::NAME).exists());
            assert_eq!(fs::read(database).unwrap(), b"untouched");
            match kind {
                "directory" => assert_eq!(fs::read_dir(&artifact).unwrap().count(), 0),
                "file" => assert_eq!(fs::read(&artifact).unwrap(), b"malformed"),
                _ => assert_eq!(
                    fs::read_link(&artifact).unwrap(),
                    Path::new("missing-target")
                ),
            }
        }
    }
}

#[test]
fn copied_windows_admitted_generation_blocks_without_changing_either_journal() {
    use generation_selection::{Phase, Record};
    use sha2::{Digest, Sha256};

    fn insert_entry(path: &Path, title: &str) {
        let db = database::open_existing(path).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('entry','2026-10-25',?1,?2,1,'created','updated')",
            rusqlite::params![
                title,
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Synthetic entry"}]}]}"#
            ],
        )
        .unwrap();
        db.close().unwrap();
    }

    let parent = TempDir::new().unwrap();
    let root = parent.path().join("journal");
    let state = DatabaseState::default();
    let legacy = state.initialize_startup(&root).unwrap();
    insert_entry(&legacy, "Retained root");
    drop(state);

    let legacy_bytes = fs::read(&legacy).unwrap();
    let control = DatabaseState::default();
    assert_eq!(control.initialize_startup(&root).unwrap(), legacy);
    assert!(control.operation(None).is_ok());
    assert_eq!(fs::read(&legacy).unwrap(), legacy_bytes);
    drop(control);

    let id = uuid::Uuid::new_v4();
    let generation = root.join("generations").join(id.to_string());
    fs::create_dir_all(&generation).unwrap();
    let selected = generation.join("journal.sqlite");
    database::create_current_database(&selected).unwrap();
    insert_entry(&selected, "Selected generation");
    database::initialize_database(&selected).unwrap();
    let selected_bytes = fs::read(&selected).unwrap();
    let record = Record::new(
        id,
        Phase::Admitted,
        format!("{:x}", Sha256::digest(&selected_bytes)),
    )
    .encode()
    .unwrap();
    let head = root.join("generation-selection.json");
    fs::write(&head, &record).unwrap();

    let copied = DatabaseState::default();
    let result = copied.initialize_startup(&root);
    assert_eq!(fs::read(&legacy).unwrap(), legacy_bytes);
    assert_eq!(fs::read(&selected).unwrap(), selected_bytes);
    assert_eq!(fs::read(&head).unwrap(), record);
    assert_eq!(result.unwrap_err(), "database_recovery_artifact_present");
    assert!(copied.operation(None).is_err());
    assert!(copied.maintenance().is_err());
    assert!(copied.recovery_session().is_err());
}

#[test]
fn filesystem_permission_errors_do_not_masquerade_as_absence() {
    let dir = TempDir::new().unwrap();
    let called = Cell::new(false);
    let result = initialize_with(
        dir.path(),
        |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        |_, _| {
            called.set(true);
            Ok(())
        },
    );
    assert!(matches!(result,Err(ref e) if e.starts_with("database_startup_io:")));
    assert!(!called.get());
    assert!(!database::expected_database_path(dir.path()).exists());
}
#[test]
fn initialization_failure_keeps_gate_closed_and_releases_os_lock() {
    let dir = TempDir::new().unwrap();
    let state = DatabaseState::default();
    fs::write(
        database::expected_database_path(dir.path()),
        b"invalid sqlite",
    )
    .unwrap();
    assert!(state.initialize_startup(dir.path()).is_err());
    assert!(state.operation(None).is_err());
    assert!(state.initialize_startup(dir.path()).is_err());
    assert!(DirectoryLease::acquire(dir.path()).is_ok());
}
#[test]
fn startup_can_only_be_approved_once_for_one_directory() {
    let a_parent = TempDir::new().unwrap();
    let a_root = a_parent.path().join("data");
    let b = TempDir::new().unwrap();
    let state = DatabaseState::default();
    state.initialize_startup(&a_root).unwrap();
    assert!(state.initialize_startup(b.path()).is_err());
    assert!(!database::expected_database_path(b.path()).exists());
}
#[test]
fn lock_is_bound_to_directory_and_is_not_deleted() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let first = DirectoryLease::acquire(a.path()).unwrap();
    assert!(DirectoryLease::acquire(a.path()).is_err());
    assert!(DirectoryLease::acquire(b.path()).is_ok());
    drop(first);
    assert!(a.path().join(LOCK).exists());
    assert!(DirectoryLease::acquire(a.path()).is_ok());
}
#[cfg(unix)]
#[test]
fn unsafe_symlinks_are_rejected_without_touching_external_files() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let root = dir.path().join("linked");
    symlink(external.path(), &root).unwrap();
    assert!(DirectoryLease::acquire(&root).is_err());
    let victim = external.path().join("victim");
    fs::write(&victim, b"keep").unwrap();
    symlink(&victim, dir.path().join(LOCK)).unwrap();
    assert!(DirectoryLease::acquire(dir.path()).is_err());
    assert_eq!(fs::read(&victim).unwrap(), b"keep");
    fs::remove_file(dir.path().join(LOCK)).unwrap();
    symlink(external.path().join("missing"), dir.path().join("recovery")).unwrap();
    assert!(initialize(dir.path()).is_err());
    assert!(!database::expected_database_path(dir.path()).exists());
}
#[cfg(unix)]
#[test]
fn replacing_lockfile_blocks_existing_state_instead_of_silently_switching_locks() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("data");
    let state = DatabaseState::default();
    state.initialize_startup(root.as_path()).unwrap();
    fs::rename(root.as_path().join(LOCK), root.as_path().join("old-lock")).unwrap();
    fs::write(root.as_path().join(LOCK), b"").unwrap();
    assert!(state.operation(None).is_err());
}
#[test]
fn lock_child() {
    let Some(root) = std::env::var_os("MNO_STARTUP_LOCK_CHILD") else {
        return;
    };
    let state = DatabaseState::default();
    state.initialize_startup(Path::new(&root)).unwrap();
    println!("MNO_LOCK_ACQUIRED");
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
#[test]
fn separate_process_is_excluded_and_crash_does_not_leave_stale_lock() {
    for crash in [false, true] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("data");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "startup::tests::lock_child", "--nocapture"])
            .env("MNO_STARTUP_LOCK_CHILD", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert!(
                output.read_line(&mut line).unwrap() > 0,
                "child exited before lock acknowledgement"
            );
            if line.contains("MNO_LOCK_ACQUIRED") {
                break;
            }
        }
        let state = DatabaseState::default();
        assert!(
            matches!(state.initialize_startup(&root),Err(ref e) if e=="database_directory_in_use")
        );
        assert!(state.operation(None).is_err());
        if crash {
            child.kill().unwrap();
        } else {
            child.stdin.take().unwrap().write_all(b"release\n").unwrap();
        }
        child.wait().unwrap();
        let restarted = DatabaseState::default();
        assert!(restarted.initialize_startup(&root).is_ok());
    }
}

#[test]
fn deleting_or_moving_established_journal_never_creates_replacement() {
    for moved in [false, true] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("data");
        let state = DatabaseState::default();
        let path = state.initialize_startup(&root).unwrap();
        drop(state);
        let original = fs::read(&path).unwrap();
        if moved {
            fs::rename(&path, parent.path().join("moved.sqlite")).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let restarted = DatabaseState::default();
        assert_eq!(
            restarted.initialize_startup(&root).unwrap_err(),
            "database_missing_established"
        );
        assert!(restarted.operation(None).is_err());
        assert!(!path.exists());
        assert!(established::read(&root).unwrap());
        if moved {
            assert_eq!(
                fs::read(parent.path().join("moved.sqlite")).unwrap(),
                original
            );
        }
    }
}
#[test]
fn valid_marker_without_database_blocks_even_without_old_lockfile() {
    let dir = TempDir::new().unwrap();
    established::publish(dir.path()).unwrap();
    let state = DatabaseState::default();
    assert_eq!(
        state.initialize_startup(dir.path()).unwrap_err(),
        "database_missing_established"
    );
    assert!(!database::expected_database_path(dir.path()).exists());
}
#[test]
fn corrupt_or_unknown_initialization_marker_never_means_fresh_install() {
    for content in [
        b"".as_slice(),
        b"partial",
        b"{\"format\":\"mno-initialized\",\"version\":2}",
    ] {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(established::NAME), content).unwrap();
        let state = DatabaseState::default();
        assert_eq!(
            state.initialize_startup(dir.path()).unwrap_err(),
            "database_initialization_marker_invalid"
        );
        assert!(!database::expected_database_path(dir.path()).exists());
        assert_eq!(
            fs::read(dir.path().join(established::NAME)).unwrap(),
            content
        );
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn failure_after_marker_before_or_during_database_creation_blocks_restart() {
    for partial in [false, true] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("data");
        let result = initialize_with(
            &root,
            |p| fs::symlink_metadata(p),
            |path, new| {
                assert!(new);
                assert!(established::read(&root).unwrap());
                if partial {
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .unwrap();
                }
                Err("injected initialization failure".into())
            },
        );
        assert!(result.is_err());
        assert!(established::read(&root).unwrap());
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        let path = database::expected_database_path(&root);
        if partial {
            assert_eq!(fs::metadata(path).unwrap().len(), 0);
        } else {
            assert!(!path.exists());
        }
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn failed_marker_publication_does_not_allow_database_creation_or_remove_published_marker() {
    for point in [
        established::Point::BeforePublish,
        established::Point::AfterPublish,
    ] {
        let dir = TempDir::new().unwrap();
        let lock = DirectoryLease::acquire(dir.path()).unwrap();
        assert!(
            established::publish_with(&lock.root, |at| if at == point {
                Err(io::Error::from(io::ErrorKind::StorageFull))
            } else {
                Ok(())
            })
            .is_err()
        );
        assert_eq!(
            established::read(dir.path()).unwrap(),
            point == established::Point::AfterPublish
        );
        drop(lock);
        let state = DatabaseState::default();
        assert!(state.initialize_startup(dir.path()).is_err());
        assert!(!database::expected_database_path(dir.path()).exists());
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn marker_publication_never_overwrites_existing_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(established::NAME), b"original").unwrap();
    assert!(established::publish(dir.path()).is_err());
    assert_eq!(
        fs::read(dir.path().join(established::NAME)).unwrap(),
        b"original"
    );
}
#[test]
fn existing_database_disappearing_before_open_is_not_recreated() {
    let dir = TempDir::new().unwrap();
    let path = database::expected_database_path(dir.path());
    database::create_new_database(&path).unwrap();
    let result = initialize_with(
        dir.path(),
        |p| fs::symlink_metadata(p),
        |path, new| {
            assert!(!new);
            fs::remove_file(path).unwrap();
            database::initialize_database(path)
        },
    );
    assert!(result.is_err());
    assert!(!path.exists());
    assert!(established::read(dir.path()).unwrap());
}
#[test]
fn concurrent_file_at_new_database_name_is_not_overwritten() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("data");
    let path = database::expected_database_path(&root);
    let result = initialize_with(
        &root,
        |p| fs::symlink_metadata(p),
        |path, new| {
            assert!(new);
            fs::write(path, b"competing file").unwrap();
            database::create_new_database(path)
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(path).unwrap(), b"competing file");
}
#[test]
fn recovery_artifact_takes_precedence_over_missing_established_database() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("data");
    let state = DatabaseState::default();
    let path = state.initialize_startup(root.as_path()).unwrap();
    drop(state);
    fs::remove_file(&path).unwrap();
    fs::create_dir(root.as_path().join("recovery")).unwrap();
    let result = initialize_with(
        root.as_path(),
        |p| fs::symlink_metadata(p),
        |_, _| panic!("must not initialize"),
    );
    assert!(matches!(result,Err(ref e) if e=="database_recovery_artifact_present"));
    assert!(!path.exists());
}
#[test]
fn legacy_artifacts_or_previous_attempt_prevent_fresh_creation_without_marker() {
    for previous_lock in [false, true] {
        let dir = TempDir::new().unwrap();
        if previous_lock {
            drop(DirectoryLease::acquire(dir.path()).unwrap());
        } else {
            fs::create_dir(dir.path().join("backups")).unwrap();
        }
        let state = DatabaseState::default();
        assert_eq!(
            state.initialize_startup(dir.path()).unwrap_err(),
            "database_missing_established"
        );
        assert!(!database::expected_database_path(dir.path()).exists());
    }
}

#[test]
fn preexisting_empty_directory_is_not_assumed_to_be_a_new_installation() {
    let dir = TempDir::new().unwrap();
    let state = DatabaseState::default();
    assert_eq!(
        state.initialize_startup(dir.path()).unwrap_err(),
        "database_missing_established"
    );
    assert!(!database::expected_database_path(dir.path()).exists());
    assert!(!dir.path().join(established::NAME).exists());
    assert!(state.operation(None).is_err());
}

#[test]
fn incompatible_existing_schemas_are_rejected_without_database_changes() {
    let definition = "CREATE TABLE journal_entries (id TEXT PRIMARY KEY, entry_date TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', content_json TEXT NOT NULL, document_version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE INDEX idx_journal_entries_date ON journal_entries(entry_date);";
    let cases = [
        "CREATE TABLE journal_entries(entry_date TEXT)".to_owned(),
        definition.replace("content_json TEXT NOT NULL, ", ""),
        definition.replace("title TEXT", "title INTEGER"),
        definition.replace("entry_date TEXT NOT NULL", "entry_date TEXT"),
        definition.replace("id TEXT PRIMARY KEY", "id TEXT"),
        "CREATE TABLE unrelated(value TEXT)".to_owned(),
        definition.replace(
            "CREATE INDEX idx_journal_entries_date ON journal_entries(entry_date);",
            "",
        ),
        definition.replace(
            "ON journal_entries(entry_date)",
            "ON journal_entries(title)",
        ),
        format!("{definition} PRAGMA user_version=3;"),
        format!(
            "{definition} CREATE TRIGGER unexpected BEFORE INSERT ON journal_entries BEGIN SELECT RAISE(ABORT,'blocked'); END;"
        ),
    ];
    for (case, sql) in cases.iter().enumerate() {
        for marked in [false, true] {
            let dir = TempDir::new().unwrap();
            let path = database::expected_database_path(dir.path());
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch(sql).unwrap();
            drop(db);
            if marked {
                established::publish(dir.path()).unwrap();
            }
            let before = fs::read(&path).unwrap();
            let state = DatabaseState::default();
            let expected = "database_schema_upgrade_required";
            assert_eq!(
                state.initialize_startup(dir.path()).unwrap_err(),
                expected,
                "case {case}, marked={marked}"
            );
            assert!(state.operation(None).is_err());
            assert!(state.maintenance().is_err());
            assert_eq!(fs::read(&path).unwrap(), before, "case {case}");
            // A second start remains blocked and does not repair missing indexes.
            assert!(
                DatabaseState::default()
                    .initialize_startup(dir.path())
                    .is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn valid_existing_schema_is_read_only_with_or_without_initialization_marker() {
    for marked in [false, true] {
        let dir = TempDir::new().unwrap();
        let path = database::expected_database_path(dir.path());
        database::create_current_database(&path).unwrap();
        #[cfg(target_os = "macos")]
        crate::backup::restore_context::bird_upgrade::initialize(&path).unwrap();
        if marked {
            established::publish(dir.path()).unwrap();
        }
        let before = fs::read(&path).unwrap();
        let state = DatabaseState::default();
        state.initialize_startup(dir.path()).unwrap();
        assert!(state.operation(None).is_ok());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(established::read(dir.path()).unwrap());
    }
}

#[test]
fn current_startup_rejects_older_and_unknown_schemas_without_writes() {
    assert_eq!(database::SCHEMA_VERSION, 4);
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    // Independent legacy DDL: do not derive this compatibility fixture from the initializer.
    db.execute_batch(
        "CREATE TABLE journal_entries (
        id TEXT PRIMARY KEY,
        entry_date TEXT NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        content_json TEXT NOT NULL,
        document_version INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    ); CREATE INDEX idx_journal_entries_date ON journal_entries(entry_date);",
    )
    .unwrap();
    assert!(database::validate_schema(&db).unwrap());
    drop(db);
    let original = fs::read(&path).unwrap();
    assert_eq!(
        database::initialize_database(&path).unwrap_err(),
        "database_schema_upgrade_required"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    for version in [1, 99] {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        assert!(!database::validate_schema(&db).unwrap());
        drop(db);
        let before = fs::read(&path).unwrap();
        let expected = if version < 4 {
            "database_schema_upgrade_required"
        } else {
            "database_schema_incompatible"
        };
        assert_eq!(database::initialize_database(&path).unwrap_err(), expected);
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    db.pragma_update(None, "user_version", 0).unwrap();
    db.execute_batch("CREATE TABLE unexpected(value TEXT)")
        .unwrap();
    assert!(!database::validate_schema(&db).unwrap());
    let new_path = dir.path().join("new.sqlite");
    database::create_current_database(&new_path).unwrap();
    let mut new = rusqlite::Connection::open(new_path).unwrap();
    assert_eq!(
        new.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        4
    );
    assert!(crate::photographs::validate_schema(&new).unwrap());
    crate::storage_content::verify_v5(&mut new, &std::sync::atomic::AtomicBool::new(false))
        .unwrap();
}

#[test]
fn current_startup_rejects_schema2_byte_identically() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema2.sqlite");
    database::create_new_database(&path).unwrap();
    let mut db = rusqlite::Connection::open(&path).unwrap();
    database::place_schema::migrate_candidate(&mut db).unwrap();
    db.close().unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        database::initialize_database(&path).unwrap_err(),
        "database_schema_upgrade_required"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn invalid_schema4_is_rejected_byte_identically() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema3.sqlite");
    database::create_current_database(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE unexpected(value TEXT)")
        .unwrap();
    db.close().unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        database::initialize_database(&path).unwrap_err(),
        "database_schema_incompatible"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn current_startup_rejects_invalid_schema4_content_without_rewriting_it() {
    let dir = TempDir::new().unwrap();
    let path = database::expected_database_path(dir.path());
    database::create_current_database(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO places(id,name,revision) VALUES('not-a-uuid','Invalid',1)",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let before = fs::read(&path).unwrap();
    let state = DatabaseState::default();
    assert!(matches!(
        state.initialize_startup(dir.path()),
        Err(ref e) if e.starts_with("database_content_validation_failed:")
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(state.operation(None).is_err());
}
