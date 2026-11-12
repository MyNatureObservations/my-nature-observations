use super::*;
use std::{fs, path::Path, sync::mpsc};

struct Fixture {
    root: tempfile::TempDir,
    state: DatabaseState,
    active: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap().join("data");
        let state = DatabaseState::default();
        let active = state.initialize_startup(&directory).unwrap();
        let db = Connection::open(&active).unwrap();
        for (id, doc) in [
            ("a", "{}"),
            (
                "b",
                "{ \"text\":\"Åäö 🌿\", \"text\":\"Original\", \"future\":{\"src\":\"/never/follow\"} }",
            ),
        ] {
            db.execute("INSERT INTO journal_entries VALUES (?1,'old date','Titel',?2,1,'created exactly','updated exactly')", rusqlite::params![id,doc]).unwrap();
        }
        db.close().unwrap();
        Self {
            root,
            state,
            active,
        }
    }
    fn destination(&self, name: &str) -> PathBuf {
        self.root.path().canonicalize().unwrap().join(name)
    }
    fn export(&self, destination: PathBuf) -> Result<Outcome, Error> {
        run(
            &self.state,
            &RestoreState::default(),
            || Ok(Some(destination)),
            || {
                self.state.assert_normal_operation_held_for_test();
                Ok(self.active.clone())
            },
        )
    }
}
fn rows(path: &Path) -> Vec<(String, String, String, String, i64, String, String)> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut statement = db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    drop(statement);
    db.close().unwrap();
    rows
}
#[test]
fn app_export_round_trip_preserves_all_fields_and_active_bytes() {
    let fixture = Fixture::new();
    let before = fs::read(&fixture.active).unwrap();
    let expected = rows(&fixture.active);
    let target = fixture.destination("journal.mno-export");
    assert_eq!(fixture.export(target.clone()), Ok(Outcome::Created));
    assert_eq!(fs::read(&fixture.active).unwrap(), before);
    let mut archive = fs::File::open(&target).unwrap();
    let candidate = if cfg!(target_os = "macos") {
        crate::open_import::build_v6(&mut archive, fixture.root.path(), &AtomicBool::new(false))
    } else {
        crate::open_import::build_v5(&mut archive, fixture.root.path(), &AtomicBool::new(false))
    }
    .unwrap();
    assert_eq!(candidate.entry_count, 2);
    assert_eq!(rows(candidate.path()), expected);
    assert_eq!(fs::read(&fixture.active).unwrap(), before);
    assert!(fixture.state.operation(None).is_ok());
    assert!(fixture.state.maintenance().is_ok());
}
#[test]
fn cancelled_dialog_uses_no_database_and_releases_shared_reservation() {
    let state = DatabaseState::ready_for_test();
    let shared = RestoreState::default();
    let outcome = run(
        &state,
        &shared,
        || {
            assert!(shared.is_busy());
            // The picker is deliberately before the database permit.
            assert!(state.maintenance().is_ok());
            Ok(None)
        },
        || panic!("cancel must not resolve/open the database"),
    );
    assert_eq!(outcome, Ok(Outcome::Cancelled));
    assert!(!shared.is_busy());
}
#[test]
fn existing_and_invalid_targets_do_not_open_source_or_replace_files() {
    let fixture = Fixture::new();
    let shared = RestoreState::default();
    let existing = fixture.destination("keep.mno-export");
    fs::write(&existing, b"external original bytes").unwrap();
    assert_eq!(
        run(
            &fixture.state,
            &shared,
            || Ok(Some(existing.clone())),
            || panic!("must not open source")
        ),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(existing).unwrap(), b"external original bytes");
    for target in [
        fixture.destination("wrong.txt"),
        fixture.destination("missing/journal.mno-export"),
    ] {
        assert_eq!(
            run(
                &fixture.state,
                &shared,
                || Ok(Some(target)),
                || panic!("invalid target")
            ),
            Err(Error::InvalidDestination)
        );
    }
    assert!(!shared.is_busy());
}
#[test]
fn target_appearing_after_precheck_is_not_overwritten_by_engine() {
    let fixture = Fixture::new();
    let shared = RestoreState::default();
    let target = fixture.destination("late.mno-export");
    let before = fs::read(&fixture.active).unwrap();
    assert_eq!(
        run(
            &fixture.state,
            &shared,
            || Ok(Some(target.clone())),
            || {
                fs::write(&target, b"late winner").unwrap();
                Ok(fixture.active.clone())
            }
        ),
        Err(Error::AlreadyExists)
    );
    assert_eq!(fs::read(target).unwrap(), b"late winner");
    assert_eq!(fs::read(&fixture.active).unwrap(), before);
    assert!(!shared.is_busy());
}
#[test]
fn invalid_content_fails_without_partial_export_or_journal_changes() {
    let fixture = Fixture::new();
    let db = Connection::open(&fixture.active).unwrap();
    db.execute(
        "UPDATE journal_entries SET content_json='broken JSON' WHERE id='a'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let before = fs::read(&fixture.active).unwrap();
    let target = fixture.destination("invalid.mno-export");
    assert_eq!(fixture.export(target.clone()), Err(Error::InvalidContent));
    assert!(!target.exists());
    assert_eq!(fs::read(&fixture.active).unwrap(), before);
    assert_eq!(
        fs::read_dir(fixture.root.path()).unwrap().count(),
        1,
        "only data directory remains"
    );
    assert!(fixture.state.operation(None).is_ok());
}
#[test]
fn missing_source_is_not_created_and_extension_is_added() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.export(fixture.destination("no-extension")),
        Ok(Outcome::Created)
    );
    assert!(fixture.destination("no-extension.mno-export").exists());
    let missing = fixture.destination("missing.sqlite");
    let target = fixture.destination("missing-source.mno-export");
    assert_eq!(
        run(
            &fixture.state,
            &RestoreState::default(),
            || Ok(Some(target.clone())),
            || Ok(missing.clone())
        ),
        Err(Error::Failed)
    );
    assert!(!missing.exists());
    assert!(!target.exists());
}
#[test]
fn concurrent_exports_and_restore_share_one_reservation() {
    let state = DatabaseState::ready_for_test();
    let shared = RestoreState::default();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let worker_state = &state;
        let worker_shared = &shared;
        let first = scope.spawn(move || {
            run(
                worker_state,
                worker_shared,
                || {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(None)
                },
                || panic!("cancelled"),
            )
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(shared.is_busy());
        assert!(shared.reserve().is_err(), "same gate as restore and quit");
        assert_eq!(
            run(
                &state,
                &shared,
                || panic!("second picker must not open"),
                || panic!("no source")
            ),
            Err(Error::Busy)
        );
        release_tx.send(()).unwrap();
        assert_eq!(first.join().unwrap(), Ok(Outcome::Cancelled));
    });
    assert!(!shared.is_busy());
    assert_eq!(
        run(
            &state,
            &shared,
            || Err(Error::InvalidDestination),
            || panic!("no source")
        ),
        Err(Error::InvalidDestination)
    );
    assert!(!shared.is_busy());
}
