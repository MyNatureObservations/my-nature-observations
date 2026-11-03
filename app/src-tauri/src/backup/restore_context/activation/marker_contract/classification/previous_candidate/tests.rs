use super::super::tests::{fixture, owner, save, tree};
use super::*;
use crate::{backup::restore_context::ActiveDatabase, database, database_state::DatabaseState};
use rusqlite::{Connection, types::Value};
use std::{collections::BTreeMap, process::Command};
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn setup(root: &Path) -> ImportV2 {
    let mut m = fixture(root, false);
    fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    m.target = Target::Old;
    save(root, &m);
    m
}
fn preserved(root: &Path) -> BTreeMap<PathBuf, String> {
    tree(root)
        .into_iter()
        .filter(|(p, _)| {
            p == Path::new("data/mno.sqlite")
                || p == Path::new("data/recovery")
                    .join(activation::MARKER)
                    .as_path()
                || p.parent() == Some(Path::new("data/backups/pre-restore"))
                || p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("next-"))
        })
        .collect()
}
fn rows(path: &Path) -> Vec<Vec<Value>> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let result = {
        let mut stmt = db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
        stmt.query_map([], |r| {
            (0..7)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<Value>>>()
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    db.close().unwrap();
    result
}
#[test]
fn builds_closed_lossless_candidate_without_changing_episode_or_admitting_ready() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    // Build protection with raw legacy objects, empty/unusual text and INTEGER type.
    let state = DatabaseState::default();
    let active_path = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let db = Connection::open(&active_path).unwrap();
    for (id, content) in [
        ("", "{}"),
        ("å🌿", "{ \"text\":\"Original\", \"text\":\"dubblett\" }"),
        (
            "unknown",
            "{\"type\":\"future\",\"image\":\"/never/open/me\"}",
        ),
    ] {
        db.execute(
            "INSERT INTO journal_entries VALUES(?1,'',?3,?2,1,'created raw','updated raw')",
            rusqlite::params![id, content, "\n\u{0}titel"],
        )
        .unwrap();
    }
    db.close().unwrap();
    let expected_rows = rows(&active_path);
    let mut active = ActiveDatabase::open(&active_path).unwrap();
    let maintenance = state.maintenance().unwrap();
    let protection = import_protection::create(&maintenance, &mut active, flag()).unwrap();
    let next = root.join("new.sqlite");
    let mut db = Connection::open(&next).unwrap();
    database::initialize_schema(&db).unwrap();
    let new = open_import::verify_database(&mut db, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    let m = ImportV2::new(
        maintenance.operation_id().to_string(),
        Target::Old,
        super::super::super::PreviousContent::try_from(protection.content().clone()).unwrap(),
        super::super::super::ReplacementContent::try_from(new).unwrap(),
        protection.archive_sha256().to_owned(),
    )
    .unwrap();
    drop(protection);
    drop(active);
    drop(maintenance);
    drop(state);
    fs::create_dir(root.join("data/recovery")).unwrap();
    save(&root, &m);
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let before = preserved(&root);
    build(&mut session, flag()).unwrap();
    let path = root.join("data/recovery").join(&m.previous_candidate_file);
    assert_eq!(rows(&path), expected_rows);
    assert_eq!(
        inspect(&path, flag()).unwrap(),
        Some(Ok(proof(
            &m.old_content.0.sha256,
            m.old_content.0.entry_count
        )))
    );
    assert_eq!(preserved(&root), before);
    assert!(state.operation(None).is_err());
    assert!(matches!(
        activation::read_marker(&root.join("data")),
        Err(activation::Error::Binding)
    ));
    assert!(matches!(
        build(&mut session, flag()).unwrap_err().cause,
        Error::Exists
    ));
}
#[test]
fn rejects_wrong_target_closed_episode_and_existing_candidate_without_overwriting() {
    for mode in ["new", "closed", "exists"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut m = setup(&root);
        if mode == "new" {
            m.target = Target::New;
            save(&root, &m);
        }
        if mode == "closed" {
            fs::create_dir_all(root.join("data/backups/activations").join(&m.operation_id))
                .unwrap();
        }
        if mode == "exists" {
            fs::write(
                root.join("data/recovery").join(&m.previous_candidate_file),
                b"never replace",
            )
            .unwrap();
        }
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let before = tree(&root);
        let e = build(&mut session, flag()).unwrap_err();
        assert!(!e.published_or_uncertain);
        assert!(matches!(
            (mode, e.cause),
            ("new", Error::NotOld) | ("closed", Error::Closed) | ("exists", Error::Exists)
        ));
        assert_eq!(tree(&root), before);
    }
}
#[test]
fn archive_and_late_marker_changes_never_publish_a_candidate() {
    for mode in [
        "missing",
        "corrupt",
        "hash",
        "marker",
        "generation",
        "archive-inode",
        "sidecar",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut m = setup(&root);
        let archive = root
            .join("data/backups/pre-restore")
            .join(&m.protection_file);
        if mode == "missing" {
            fs::remove_file(&archive).unwrap();
        }
        if mode == "corrupt" {
            fs::write(&archive, b"not a zip").unwrap();
            m.protection_sha256 = backup::digest_file(&archive).unwrap();
            save(&root, &m);
        }
        if mode == "hash" {
            m.protection_sha256 = "0".repeat(64);
            save(&root, &m);
        }
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let mut before = preserved(&root);
        let e = build_with(&mut session, flag(), |p, _| {
            if p == Point::BeforePublish {
                match mode {
                    "marker" => {
                        m.new_content.0.entry_count += 1;
                        save(&root, &m);
                    }
                    "generation" => state.set_generation_for_test(77),
                    "archive-inode" => {
                        let copy = root.join("copy");
                        fs::copy(&archive, &copy)?;
                        fs::rename(copy, &archive)?;
                    }
                    "sidecar" => fs::write(root.join("data/mno.sqlite-wal"), b"block")?,
                    _ => {}
                }
                before = preserved(&root);
            }
            Ok(())
        })
        .unwrap_err();
        assert!(!e.published_or_uncertain);
        assert!(matches!(
            (mode, e.cause),
            ("missing" | "corrupt", Error::Archive)
                | (
                    "hash" | "marker" | "generation" | "archive-inode",
                    Error::Changed
                )
                | ("sidecar", Error::Observation(Blocked::Sidecar))
        ));
        assert!(
            !root
                .join("data/recovery")
                .join(&m.previous_candidate_file)
                .exists()
        );
        assert_eq!(preserved(&root), before);
    }
}
#[test]
fn corrupt_built_content_is_not_accepted_as_verified() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let m = setup(&root);
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let before = preserved(&root);
    let e = build_with(&mut session, flag(), |p, path| {
        if p == Point::Verified {
            let db = Connection::open(path).unwrap();
            db.execute("UPDATE journal_entries SET title='wrong'", [])
                .unwrap();
            db.close().unwrap();
        }
        Ok(())
    })
    .unwrap_err();
    assert!(matches!(e.cause, Error::Proof));
    assert!(!e.published_or_uncertain);
    assert!(
        !root
            .join("data/recovery")
            .join(&m.previous_candidate_file)
            .exists()
    );
    assert_eq!(preserved(&root), before);
}
const POINTS: [Point; 10] = [
    Point::Validated,
    Point::Extracted,
    Point::PartialWrite,
    Point::Verified,
    Point::BeforePublish,
    Point::Published,
    Point::SourceSync,
    Point::DestinationSync,
    Point::FinalSync,
    Point::Durable,
];
fn after(p: &Point) -> bool {
    matches!(
        p,
        Point::Published
            | Point::SourceSync
            | Point::DestinationSync
            | Point::FinalSync
            | Point::Durable
    )
}
#[test]
fn write_and_late_sync_errors_preserve_source_and_never_claim_success() {
    for selected in POINTS {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root);
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let before = preserved(&root);
        let e = build_with(&mut session, flag(), |p, _| {
            if p == selected {
                Err(io::ErrorKind::StorageFull.into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(matches!(e.cause,Error::Io(ref e) if e.kind()==io::ErrorKind::StorageFull));
        assert_eq!(e.published_or_uncertain, after(&selected));
        assert_eq!(preserved(&root), before);
        assert!(state.operation(None).is_err());
        let candidate = root.join("data/recovery").join(&m.previous_candidate_file);
        if after(&selected) {
            assert_eq!(
                inspect(&candidate, flag()).unwrap(),
                Some(Ok(proof(
                    &m.old_content.0.sha256,
                    m.old_content.0.entry_count
                )))
            );
            assert!(matches!(
                build(&mut session, flag()).unwrap_err().cause,
                Error::Exists
            ));
        } else {
            assert!(!candidate.exists());
            build(&mut session, flag()).unwrap();
        }
    }
}
const CHILD: &str = "backup::restore_context::activation::marker_contract::classification::previous_candidate::tests::interruption_child";
#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_PREVIOUS_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let selected = std::env::var("MNO_PREVIOUS_POINT").unwrap();
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    build_with(&mut session, flag(), |p, _| {
        if format!("{p:?}") == selected {
            eprintln!("PREVIOUS_INTERRUPT:{p:?}");
            std::process::exit(86);
        }
        Ok(())
    })
    .unwrap();
    panic!("interruption not reached");
}
#[test]
fn process_interruptions_leave_only_private_work_or_a_complete_candidate() {
    for selected in POINTS {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root);
        let before = preserved(&root);
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env("MNO_PREVIOUS_ROOT", &root)
            .env("MNO_PREVIOUS_POINT", format!("{selected:?}"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(86),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains(&format!("PREVIOUS_INTERRUPT:{selected:?}"))
        );
        assert_eq!(preserved(&root), before);
        // Actual startup precedes every diagnostic SQLite open.
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let candidate = root.join("data/recovery").join(&m.previous_candidate_file);
        if after(&selected) {
            assert_eq!(
                classify(&session, flag()).unwrap().previous_candidate,
                Candidate::Verified
            );
            assert!(matches!(
                build(&mut session, flag()).unwrap_err().cause,
                Error::Exists
            ));
        } else {
            assert!(!candidate.exists());
            build(&mut session, flag()).unwrap();
        }
        assert_eq!(rows(&candidate), rows(&root.join("old.sqlite")));
        assert_eq!(preserved(&root), before);
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn race_at_destination_is_rejected_by_atomic_no_clobber_rename() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let m = setup(&root);
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let before = preserved(&root);
    // Exercise the same primitive directly: an existing final entry cannot be replaced.
    let source = root.join("source");
    let dest = root.join("data/recovery").join(&m.previous_candidate_file);
    fs::write(&source, b"candidate").unwrap();
    fs::write(&dest, b"existing").unwrap();
    let e = rename_exclusive(&source, &dest).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&dest).unwrap(), b"existing");
    assert!(source.exists());
    assert!(matches!(
        build(&mut session, flag()).unwrap_err().cause,
        Error::Exists
    ));
    assert_eq!(preserved(&root), before);
}

#[test]
fn cancellation_and_private_sqlite_sidecars_cannot_publish() {
    for mode in ["cancel", "source-sidecar", "candidate-sidecar"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root);
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let before = preserved(&root);
        let cancel = flag();
        let e = build_with(&mut session, cancel.clone(), |p, path| {
            if mode == "cancel" && p == Point::PartialWrite {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
            if (mode == "source-sidecar" && p == Point::Extracted)
                || (mode == "candidate-sidecar" && p == Point::PartialWrite)
            {
                let mut sidecar = path.as_os_str().to_os_string();
                sidecar.push("-journal");
                fs::write(PathBuf::from(sidecar), b"unsafe journal")?;
            }
            Ok(())
        })
        .unwrap_err();
        let expected = if mode == "cancel" {
            Blocked::Cancelled
        } else {
            Blocked::Sidecar
        };
        assert!(matches!(e.cause, Error::Observation(ref actual) if *actual == expected));
        assert!(!e.published_or_uncertain);
        assert!(
            !root
                .join("data/recovery")
                .join(&m.previous_candidate_file)
                .exists()
        );
        assert_eq!(preserved(&root), before);
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn wrong_old_proof_is_rejected_by_extracted_content_verification() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut m = setup(&root);
    m.old_content.0.entry_count += 1;
    save(&root, &m);
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let before = preserved(&root);
    let e = build(&mut session, flag()).unwrap_err();
    assert!(matches!(e.cause, Error::Archive));
    assert!(!e.published_or_uncertain);
    assert!(
        !root
            .join("data/recovery")
            .join(&m.previous_candidate_file)
            .exists()
    );
    assert_eq!(preserved(&root), before);
}
