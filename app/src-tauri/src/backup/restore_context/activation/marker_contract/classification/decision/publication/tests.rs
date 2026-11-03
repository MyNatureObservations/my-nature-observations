use super::super::super::tests::{fixture, owner, save, tree};
use super::*;
use crate::database_state::DatabaseState;
use std::{collections::BTreeMap, process::Command};

fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn setup(root: &Path, mode: &str) -> ImportV2 {
    let m = fixture(root, mode == "same");
    if mode != "old" {
        fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    }
    m
}
fn journals(root: &Path) -> BTreeMap<PathBuf, String> {
    tree(root)
        .into_iter()
        .filter(|(p, _)| {
            p == Path::new("data/mno.sqlite")
                || p == Path::new("data/.mno-initialized-v1.json")
                || (p.parent() == Some(Path::new("data/recovery"))
                    && p.file_name().unwrap() != activation::MARKER)
                || p.parent() == Some(Path::new("data/backups/pre-restore"))
        })
        .collect()
}
fn assert_marker(root: &Path, expected: &ImportV2, target: Target) {
    let mut actual = marker(&root.join("data")).unwrap();
    assert_eq!(actual.target, target);
    actual.target = Target::New;
    assert_eq!(&actual, expected, "only target may change");
}
#[test]
fn durable_publication_only_changes_target_and_never_admits_ready() {
    for mode in ["old", "new", "same"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, mode);
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let before = journals(&root);
        publish(&mut session, Some(&c), flag()).unwrap();
        assert_marker(&root, &m, Target::Old);
        assert_eq!(journals(&root), before);
        assert!(state.operation(None).is_err());
        assert!(matches!(
            activation::read_marker(&root.join("data")),
            Err(activation::Error::Binding)
        ));
        let bytes = fs::read(root.join("data/recovery").join(activation::MARKER)).unwrap();
        let failure = publish(&mut session, Some(&c), flag()).unwrap_err();
        assert_eq!(failure.progress, Progress::BeforeRename);
        assert!(matches!(failure.cause, Cause::Decision(Error::NotNew)));
        assert_eq!(
            fs::read(root.join("data/recovery").join(activation::MARKER)).unwrap(),
            bytes
        );
        assert_eq!(journals(&root), before);
    }
}
#[test]
fn stale_or_unconfirmed_decisions_cannot_prepare_a_publication() {
    for mode in [
        "operation",
        "session",
        "generation",
        "active",
        "marker",
        "old",
        "reverse",
        "cancel",
        "missing",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let mut c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let expected = match mode {
            "operation" => {
                c.binding.marker.operation_id = uuid::Uuid::new_v4().to_string();
                Error::Stale
            }
            "session" => {
                drop(session);
                session = state.recovery_session().unwrap();
                Error::Stale
            }
            "generation" => {
                state.set_generation_for_test(c.binding.generation + 1);
                Error::Stale
            }
            "active" => {
                let db = rusqlite::Connection::open(root.join("data/mno.sqlite")).unwrap();
                db.execute("UPDATE journal_entries SET title='changed'", [])
                    .unwrap();
                db.close().unwrap();
                Error::UnsafeState
            }
            "marker" => {
                m.new_content.0.entry_count += 1;
                save(&root, &m);
                Error::UnsafeState
            }
            "old" => {
                m.target = Target::Old;
                save(&root, &m);
                Error::NotNew
            }
            "reverse" => {
                m.target = Target::Old;
                save(&root, &m);
                c.choice = UserChoice::KeepNew;
                Error::NotConfirmed
            }
            "cancel" => {
                c.choice = UserChoice::Cancel;
                Error::NotConfirmed
            }
            _ => Error::MissingDecision,
        };
        let before = tree(&root);
        let failure = publish(
            &mut session,
            if mode == "missing" { None } else { Some(&c) },
            flag(),
        )
        .unwrap_err();
        assert_eq!(failure.progress, Progress::BeforeRename);
        match failure.cause {
            Cause::Decision(e) => assert_eq!(e, expected),
            e => panic!("wrong failure: {e:?}"),
        }
        assert_eq!(tree(&root), before);
    }
}
const POINTS: [Point; 10] = [
    Point::Prepare,
    Point::Write,
    Point::PartialWrite,
    Point::StagedSync,
    Point::BeforeRename,
    Point::AfterRename,
    Point::SourceSync,
    Point::DestinationSync,
    Point::FinalSync,
    Point::Durable,
];
fn after_rename(p: Point) -> bool {
    matches!(
        p,
        Point::AfterRename
            | Point::SourceSync
            | Point::DestinationSync
            | Point::FinalSync
            | Point::Durable
    )
}
#[test]
fn injected_io_errors_never_claim_durable_success_and_retries_read_actual_target() {
    for point in POINTS {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let before = journals(&root);
        let failure = publish_with(&mut session, Some(&c), flag(), |p, _| {
            if p == point {
                Err(io::Error::from(io::ErrorKind::StorageFull))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(failure.point, point);
        assert_eq!(
            failure.progress,
            if after_rename(point) {
                Progress::MayHaveChanged
            } else {
                Progress::BeforeRename
            }
        );
        assert!(
            matches!(failure.cause, Cause::Io(ref e) if e.kind() == io::ErrorKind::StorageFull)
        );
        assert_marker(
            &root,
            &m,
            if after_rename(point) {
                Target::Old
            } else {
                Target::New
            },
        );
        assert_eq!(journals(&root), before);
        if after_rename(point) {
            let failure = publish(&mut session, Some(&c), flag()).unwrap_err();
            assert!(matches!(failure.cause, Cause::Decision(Error::NotNew)));
        } else {
            publish(&mut session, Some(&c), flag()).unwrap();
        }
        assert_marker(&root, &m, Target::Old);
        assert_eq!(journals(&root), before);
    }
}
#[test]
fn changes_at_last_checkpoint_are_rechecked_before_overwriting_any_marker() {
    for mode in [
        "generation",
        "active",
        "sidecar",
        "marker-inode",
        "marker-bytes",
        "stage-inode",
        "stage-bytes",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let mut expected_journals = journals(&root);
        let marker_path = root.join("data/recovery").join(activation::MARKER);
        let mut expected_marker = fs::read(&marker_path).unwrap();
        let failure = publish_with(&mut session, Some(&c), flag(), |point, staged| {
            if point != Point::BeforeRename {
                return Ok(());
            }
            match mode {
                "generation" => state.set_generation_for_test(c.binding.generation + 1),
                "active" => {
                    let db = rusqlite::Connection::open(root.join("data/mno.sqlite")).unwrap();
                    db.execute("UPDATE journal_entries SET title='late change'", [])
                        .unwrap();
                    db.close().unwrap();
                }
                "sidecar" => fs::write(root.join("data/mno.sqlite-journal"), b"blocking").unwrap(),
                "marker-inode" => {
                    let swap = root.join("replacement-marker");
                    fs::write(&swap, &expected_marker).unwrap();
                    fs::rename(swap, &marker_path).unwrap();
                }
                "marker-bytes" => {
                    expected_marker.push(b' ');
                    fs::write(&marker_path, &expected_marker).unwrap();
                }
                "stage-inode" => {
                    let swap = root.join("replacement-stage");
                    fs::write(&swap, fs::read(staged).unwrap()).unwrap();
                    fs::rename(swap, staged).unwrap();
                }
                _ => fs::write(staged, b"partial or corrupt marker").unwrap(),
            }
            expected_journals = journals(&root);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(failure.progress, Progress::BeforeRename);
        match mode {
            "generation" => assert!(matches!(failure.cause, Cause::Decision(Error::Stale))),
            "active" => assert!(matches!(failure.cause, Cause::Decision(Error::UnsafeState))),
            "sidecar" => assert!(matches!(
                failure.cause,
                Cause::Decision(Error::Observation(Blocked::Sidecar))
            )),
            _ => assert!(matches!(failure.cause, Cause::Changed)),
        }
        assert_eq!(fs::read(marker_path).unwrap(), expected_marker);
        assert_eq!(journals(&root), expected_journals);
        assert_marker(&root, &m, Target::New);
    }
}
#[test]
fn unsafe_environment_is_rejected_without_staging_or_sqlite_open_attempts() {
    for mode in ["sidecar", "marker-link", "candidate-link", "recovery-link"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        if mode == "sidecar" {
            fs::write(root.join("data/mno.sqlite-wal"), b"blocked").unwrap();
        } else {
            #[cfg(unix)]
            {
                let path = match mode {
                    "marker-link" => root.join("data/recovery").join(activation::MARKER),
                    "candidate-link" => root.join("data/recovery").join(&m.candidate_file),
                    _ => root.join("data/recovery"),
                };
                let moved = root.join("moved");
                fs::rename(&path, &moved).unwrap();
                std::os::unix::fs::symlink(moved, path).unwrap();
            }
        }
        let before = tree(&root);
        let (result, attempts) =
            import_protection::observe_sqlite_opens(|| publish(&mut session, Some(&c), flag()));
        let failure = result.unwrap_err();
        assert_eq!(failure.progress, Progress::BeforeRename);
        let expected = if mode == "sidecar" {
            Blocked::Sidecar
        } else {
            Blocked::Namespace
        };
        assert!(
            matches!(failure.cause, Cause::Decision(Error::Observation(ref e)) if *e == expected)
        );
        assert!(attempts.is_empty());
        assert_eq!(tree(&root), before);
    }
}
#[cfg(unix)]
#[test]
fn actual_rename_permission_error_is_conservatively_uncertain_without_overwrite() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let m = setup(&root, "new");
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    let before = journals(&root);
    let recovery = root.join("data/recovery");
    let original_permissions = fs::metadata(&recovery).unwrap().permissions();
    let result = publish_with(&mut session, Some(&c), flag(), |point, _| {
        if point == Point::BeforeRename {
            fs::set_permissions(&recovery, fs::Permissions::from_mode(0o500))?;
        }
        Ok(())
    });
    fs::set_permissions(&recovery, original_permissions).unwrap();
    let failure = result.unwrap_err();
    assert_eq!(failure.point, Point::BeforeRename);
    assert_eq!(failure.progress, Progress::MayHaveChanged);
    assert!(
        matches!(failure.cause, Cause::Io(ref e) if e.kind() == io::ErrorKind::PermissionDenied)
    );
    assert_marker(&root, &m, Target::New);
    assert_eq!(journals(&root), before);
    publish(&mut session, Some(&c), flag()).unwrap();
    assert_marker(&root, &m, Target::Old);
}
#[test]
fn cancellation_on_each_side_of_rename_does_not_restore_new_or_report_success() {
    for point in [Point::BeforeRename, Point::AfterRename] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let before = journals(&root);
        let cancelled = flag();
        let failure = publish_with(&mut session, Some(&c), cancelled.clone(), |p, _| {
            if p == point {
                cancelled.store(true, Ordering::Release);
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(
            failure.progress,
            if point == Point::BeforeRename {
                Progress::BeforeRename
            } else {
                Progress::MayHaveChanged
            }
        );
        assert!(matches!(
            failure.cause,
            Cause::Decision(Error::Observation(Blocked::Cancelled))
        ));
        assert_marker(
            &root,
            &m,
            if point == Point::BeforeRename {
                Target::New
            } else {
                Target::Old
            },
        );
        assert_eq!(journals(&root), before);
    }
}
const CHILD: &str = "backup::restore_context::activation::marker_contract::classification::decision::publication::tests::publication_interrupt_child";
#[test]
fn publication_interrupt_child() {
    let Some(root) = std::env::var_os("MNO_1B3B2_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let selected = std::env::var("MNO_1B3B2_POINT").unwrap();
    let state = owner(&root);
    let mut session = state.recovery_session().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    publish_with(&mut session, Some(&c), flag(), |point, _| {
        if format!("{point:?}") == selected {
            eprintln!("PUBLICATION_INTERRUPT:{point:?}");
            std::process::exit(86);
        }
        Ok(())
    })
    .unwrap();
    panic!("intended interruption was not reached");
}
#[test]
fn process_interruptions_leave_complete_selected_markers_and_keep_startup_blocked() {
    for point in POINTS {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let before = journals(&root);
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env("MNO_1B3B2_ROOT", &root)
            .env("MNO_1B3B2_POINT", format!("{point:?}"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(86),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains(&format!("PUBLICATION_INTERRUPT:{point:?}"))
        );
        let target = if after_rename(point) {
            Target::Old
        } else {
            Target::New
        };
        assert_marker(&root, &m, target);
        assert_eq!(journals(&root), before);
        // Real startup runs first; no diagnostic SQLite open can recover a file.
        let state = DatabaseState::default();
        assert!(state.initialize_legacy_startup(&root.join("data")).is_err());
        assert!(state.operation(None).is_err());
        let mut session = state.recovery_session().unwrap();
        let files = tree(&root);
        let observed = classify(&session, flag()).unwrap();
        assert_eq!(observed.target, target);
        assert_eq!(observed.active, Active::New);
        assert_eq!(observed.protection, Protection::Verified);
        assert_eq!(tree(&root), files);
        assert!(matches!(
            activation::read_marker(&root.join("data")),
            Err(activation::Error::Binding)
        ));
        if target == Target::New {
            let c = preview(&session, flag())
                .unwrap()
                .confirm(UserChoice::ReturnPrevious);
            publish(&mut session, Some(&c), flag()).unwrap();
            assert_marker(&root, &m, Target::Old);
        } else {
            assert!(matches!(preview(&session, flag()), Err(Error::NotNew)));
        }
        assert_eq!(journals(&root), before);
        assert!(state.operation(None).is_err());
    }
}

#[test]
fn errors_from_each_actual_barrier_call_cannot_be_ignored() {
    for selected in [
        Point::StagedSync,
        Point::SourceSync,
        Point::DestinationSync,
        Point::FinalSync,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, "new");
        let state = owner(&root);
        let mut session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let before = journals(&root);
        let mut calls = Vec::new();
        let failure = publish_with_barrier(
            &mut session,
            Some(&c),
            flag(),
            |_, _| Ok(()),
            |point, path, file| {
                calls.push(point);
                if point == selected {
                    Err(io::Error::other("injected barrier error"))
                } else {
                    sync_barrier(point, path, file)
                }
            },
        )
        .unwrap_err();
        assert_eq!(calls.last(), Some(&selected));
        assert_eq!(failure.point, selected);
        assert_eq!(
            failure.progress,
            if selected == Point::StagedSync {
                Progress::BeforeRename
            } else {
                Progress::MayHaveChanged
            }
        );
        assert!(
            matches!(failure.cause, Cause::Io(ref e) if e.to_string() == "injected barrier error")
        );
        assert_marker(
            &root,
            &m,
            if selected == Point::StagedSync {
                Target::New
            } else {
                Target::Old
            },
        );
        assert_eq!(journals(&root), before);
        assert!(state.operation(None).is_err());
    }
}
