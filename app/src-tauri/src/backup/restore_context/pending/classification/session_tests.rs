use super::*;
const Q: &str = ".mno-restore-completion-v1.json";
fn receipt(f: &Fixture, outcome: &str) {
    let pending = fs::read_to_string(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::write(
        f.root.join(Q),
        serde_json::to_vec(&serde_json::json!({
            "format":"mno-restore-completion", "version":1,
            "pending_json":pending, "verified_outcome":outcome
        }))
        .unwrap(),
    )
    .unwrap();
}
fn startup(f: &Fixture) -> DatabaseState {
    let state = DatabaseState::default();
    assert_eq!(
        state.initialize_legacy_startup(&f.root).unwrap_err(),
        "database_recovery_artifact_present"
    );
    state
}
#[test]
fn session_matrix_verifies_p_pq_q_and_q_with_empty_directory() {
    for shape in 0..4 {
        for (title, outcome, expected) in [("Old", "old", Status::Old), ("New", "new", Status::New)]
        {
            let f = Fixture::new();
            f.title(title);
            if shape != 0 {
                receipt(&f, outcome);
            }
            if shape >= 2 {
                fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
            }
            if shape == 2 {
                fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
            }
            let before = tree(&f.root);
            let state = startup(&f);
            let assessment = classify_session(state.recovery_session().unwrap(), flag());
            assert_eq!(
                assessment.status(),
                expected,
                "shape {shape}: {:?}",
                assessment.reason()
            );
            assert_eq!(tree(&f.root), before);
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            assert!(state.operation(None).is_err());
            drop(assessment);
            // Neither Assessment nor Session Drop releases the owning gate.
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            assert!(state.operation(None).is_err());
            assert!(state.maintenance().is_err());
            drop(state);
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_ok());
        }
    }
}
#[test]
fn receipt_outcome_controls_equal_fingerprints_without_claiming_copy_history() {
    for outcome in [None, Some("old"), Some("new")] {
        let f = Fixture::new();
        let p = f.root.join(DIRECTORY).join(NAME);
        let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        data["new_content"] = data["old_content"].clone();
        fs::write(p, serde_json::to_vec(&data).unwrap()).unwrap();
        if let Some(outcome) = outcome {
            receipt(&f, outcome);
        }
        let state = startup(&f);
        let a = classify_session(state.recovery_session().unwrap(), flag());
        assert_eq!(
            a.status(),
            if outcome == Some("new") {
                Status::New
            } else {
                Status::Old
            }
        );
    }
}
#[test]
fn conflicts_invalid_artifacts_and_wrong_outcomes_never_approve() {
    for case in 0..15 {
        let f = Fixture::new();
        receipt(&f, "old");
        let p = f.root.join(DIRECTORY).join(NAME);
        match case {
            0 => {
                let mut b = fs::read(&p).unwrap();
                b.push(b' ');
                fs::write(&p, b).unwrap();
            }
            1 => f.title("New"), // receipt says Old
            2 => {
                receipt(&f, "new");
            } // active Old
            3 => {
                fs::write(f.root.join(Q), b"partial").unwrap();
            }
            4 => {
                fs::write(f.root.join(DIRECTORY).join("partial.tmp"), b"x").unwrap();
            }
            5 => {
                fs::write(f.root.join(".mno-completion-abandoned"), b"x").unwrap();
            }
            6 => {
                let q = f.root.join(Q);
                fs::rename(&q, f.root.join("saved-q")).unwrap();
                std::os::unix::fs::symlink("saved-q", q).unwrap();
            }
            7 => {
                let r = f.root.join(DIRECTORY);
                fs::rename(&r, f.root.join("saved-r")).unwrap();
                std::os::unix::fs::symlink("saved-r", r).unwrap();
            }
            8 => {
                fs::remove_file(&p).unwrap();
                fs::remove_file(f.root.join(Q)).unwrap();
            }
            9 => {
                fs::remove_file(&f.path).unwrap();
            }
            10 => {
                fs::write(f.root.join("mno.sqlite-wal"), b"x").unwrap();
            }
            11 => {
                fs::write(f.root.join("mno.sqlite-shm"), b"x").unwrap();
            }
            12 => {
                fs::write(f.root.join("mno.sqlite-journal"), b"x").unwrap();
            }
            13 => {
                fs::write(&f.protection, b"bad archive").unwrap();
            }
            _ => {
                fs::remove_file(f.root.join(Q)).unwrap();
                fs::create_dir(f.root.join(Q)).unwrap();
            }
        }
        let before = tree(&f.root);
        let state = startup(&f);
        let a = classify_session(state.recovery_session().unwrap(), flag());
        assert_eq!(a.status(), Status::Uncertain, "case {case}");
        drop(a);
        assert!(state.operation(None).is_err());
        assert_eq!(tree(&f.root), before);
    }
}
#[test]
fn retained_q_snapshot_prevents_external_commit_and_no_second_lock_is_taken() {
    let f = Fixture::new();
    receipt(&f, "old");
    fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
    let state = startup(&f);
    let a = classify_session(state.recovery_session().unwrap(), flag());
    assert_eq!(a.status(), Status::Old);
    let writer = Connection::open(&f.path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='New'")
        .unwrap();
    assert!(writer.execute_batch("COMMIT").is_err());
    drop(a);
    writer.execute_batch("COMMIT").unwrap();
    drop(writer);
    assert!(state.operation(None).is_err());
    // Receipt remains Old, so a fresh New observation is rejected.
    assert_eq!(
        classify_session(state.recovery_session().unwrap(), flag()).status(),
        Status::Uncertain
    );
}
#[test]
fn receipt_and_marker_replacements_at_controlled_boundaries_are_rejected() {
    for point in [Point::BeforeSqlite, Point::Snapshot, Point::Final] {
        for kind in 0..4 {
            let f = Fixture::new();
            receipt(&f, "old");
            let state = startup(&f);
            let before_db = fs::read(&f.path).unwrap();
            let a = assess(
                &f.root,
                Access::Session(state.recovery_session().unwrap()),
                flag(),
                |at| {
                    if at == point {
                        let target = if kind < 2 {
                            f.root.join(Q)
                        } else {
                            f.root.join(DIRECTORY).join(NAME)
                        };
                        let bytes = fs::read(&target).unwrap();
                        if kind % 2 == 0 {
                            fs::rename(&target, target.with_extension("saved")).unwrap();
                            fs::write(target, bytes).unwrap();
                        } else {
                            let mut changed = bytes;
                            changed.push(b' ');
                            fs::write(target, changed).unwrap();
                        }
                    }
                },
                Control::Real,
            );
            assert_eq!(a.status(), Status::Uncertain);
            drop(a);
            assert_eq!(fs::read(&f.path).unwrap(), before_db);
        }
    }
}
#[test]
fn cancellation_retains_blocking_and_no_artifacts_is_never_a_verified_outcome() {
    let f = Fixture::new();
    let state = startup(&f);
    for point in [Point::Locked, Point::Snapshot, Point::Final] {
        let cancel = flag();
        let before = tree(&f.root);
        let a = assess(
            &f.root,
            Access::Session(state.recovery_session().unwrap()),
            cancel.clone(),
            |at| {
                if at == point {
                    cancel.store(true, Ordering::Release);
                }
            },
            Control::Real,
        );
        assert_eq!(a.status(), Status::Uncertain);
        assert_eq!(a.reason(), Some(Reason::Cancelled));
        drop(a);
        assert_eq!(tree(&f.root), before);
        assert!(state.operation(None).is_err());
    }
    // Diagnostic API can report absence, with no verified observation.
    drop(state);
    fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
    let a = classify(&f.root, flag());
    assert_eq!(a.status(), Status::NotNeeded);
    assert_eq!(a.reason(), None);
}

#[test]
fn runtime_session_uses_same_operation_and_keeps_maintenance_after_assessment_drop() {
    let f = Fixture::new();
    fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let candidate =
        restore::prepare_candidate(&f._dir.path().join("incoming.mno-backup"), flag()).unwrap();
    super::super::super::super::with_active(&state, &f.path, candidate, flag(), |mut ctx| {
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let generation = ctx.maintenance.generation();
        let before = tree(&f.root);
        let a = classify_session(ctx.maintenance.recovery_session().unwrap(), flag());
        assert_eq!(a.status(), Status::Old, "{:?}", a.reason());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        drop(a);
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(tree(&f.root), before);
        Ok(())
    })
    .unwrap();
    assert!(state.operation(Some(1)).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
#[test]
fn unrelated_runtime_operation_cannot_classify_another_operations_marker() {
    let f = Fixture::new();
    let pending = fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let maintenance = state.maintenance().unwrap();
    fs::create_dir(f.root.join(DIRECTORY)).unwrap();
    fs::write(f.root.join(DIRECTORY).join(NAME), pending).unwrap();
    let a = classify_session(maintenance.recovery_session().unwrap(), flag());
    assert_eq!(a.status(), Status::Uncertain);
    assert_eq!(a.reason(), Some(Reason::Marker));
    drop(a);
    drop(maintenance);
    assert!(state.operation(None).is_err());
}
#[test]
fn no_artifacts_under_runtime_maintenance_is_diagnostic_only_and_stays_blocked() {
    let f = Fixture::new();
    fs::remove_file(f.root.join(DIRECTORY).join(NAME)).unwrap();
    fs::remove_dir(f.root.join(DIRECTORY)).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let guard = state.maintenance().unwrap();
    let a = classify_session(guard.recovery_session().unwrap(), flag());
    assert_eq!(a.status(), Status::NotNeeded);
    drop(a);
    assert!(guard.ensure_usable().is_err());
    drop(guard);
    assert!(state.operation(None).is_err());
}

#[test]
fn session_keeps_physical_binding_for_database_swaps_and_control_errors() {
    for point in [Point::BeforeSqlite, Point::Snapshot, Point::Final] {
        for replacement in ["Old", "New"] {
            let f = Fixture::new();
            receipt(&f, "old");
            // Final is reached only after the original content matches Q=Old.
            f.title(if point == Point::Final {
                "Old"
            } else {
                "Neither"
            });
            let foreign = f.root.join("foreign.sqlite");
            fs::copy(&f.path, &foreign).unwrap();
            let db = Connection::open(&foreign).unwrap();
            db.execute("UPDATE journal_entries SET title=?1", [replacement])
                .unwrap();
            drop(db);
            let state = startup(&f);
            let saved = f.root.join("saved.sqlite");
            let original = fs::read(&f.path).unwrap();
            let mut swapped = false;
            let a = assess(
                &f.root,
                Access::Session(state.recovery_session().unwrap()),
                flag(),
                |at| {
                    if at == point {
                        fs::rename(&f.path, &saved).unwrap();
                        fs::rename(&foreign, &f.path).unwrap();
                        swapped = true;
                    }
                },
                Control::Real,
            );
            assert!(swapped, "requested file-swap hook was not reached");
            assert!(saved.is_file());
            assert!(!foreign.exists());
            assert_eq!(a.status(), Status::Uncertain);
            drop(a);
            assert_eq!(fs::read(&saved).unwrap(), original);
            fs::rename(&f.path, &foreign).unwrap();
            fs::rename(&saved, &f.path).unwrap();
            assert_eq!(fs::read(&f.path).unwrap(), original);
        }
    }
    for control in [
        Control::FilePointerError,
        Control::FilePointerUnsupported,
        Control::FilePointerNull,
        Control::FilePointerUnexpected,
        Control::Error,
        Control::Unsupported,
    ] {
        let f = Fixture::new();
        receipt(&f, "old");
        let state = startup(&f);
        let before = tree(&f.root);
        let a = assess(
            &f.root,
            Access::Session(state.recovery_session().unwrap()),
            flag(),
            |_| {},
            control,
        );
        assert_eq!(a.status(), Status::Uncertain);
        assert_eq!(a.reason(), Some(Reason::Identity));
        drop(a);
        assert_eq!(tree(&f.root), before);
        assert!(state.operation(None).is_err());
    }
}

#[test]
fn ascii_case_reserved_artifacts_remain_uncertain_under_a_retained_session() {
    for name in [
        ".MNO-COMPLETION-orphan",
        ".Mno-Completion-orphan",
        ".MNO-INIT-orphan",
        ".PENDING-orphan",
        ".MNO-RESTORE-COMPLETION-v2.json",
        "MNO.SQLITE-WAL",
    ] {
        let f = Fixture::new();
        fs::write(f.root.join(name), b"partial").unwrap();
        let state = startup(&f);
        let before = tree(&f.root);
        let assessment = classify_session(state.recovery_session().unwrap(), flag());
        assert_eq!(assessment.status(), Status::Uncertain, "{name}");
        assert!(assessment.into_completion_proof().is_err());
        assert!(state.operation(Some(1)).is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(tree(&f.root), before);
    }
}
