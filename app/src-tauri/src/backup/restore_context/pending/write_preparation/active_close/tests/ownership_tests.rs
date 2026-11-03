use super::*;
use std::os::fd::AsRawFd;

fn inputs<'m, 's>(f: &Fixture, m: &'m Maintenance<'s>) -> WriteInputs<'m, 's> {
    prepared(f, m).close_active().unwrap()
}
fn parts<'p, 'm, 's>(p: &'p WriteInputs<'m, 's>) -> &'p ActiveCloseParts<'m, 's> {
    p.owner.after.as_ref().unwrap()
}
fn descriptors(p: &ActiveCloseParts<'_, '_>) -> [i32; 4] {
    let ActiveState::Closed(a) = &p.active else {
        panic!("A must already be closed")
    };
    let pending = p.pending.as_ref().unwrap();
    [
        a.file.as_raw_fd(),
        pending.root_file.as_raw_fd(),
        pending.directory_file.as_raw_fd(),
        pending.marker_file.as_raw_fd(),
    ]
}
#[test]
fn epoch_inputs_consumes_entire_real_owner_without_io_or_close() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let input = inputs(&f, &m);
    let p = parts(&input);
    let fds = descriptors(p);
    let id = p.session.as_ref().unwrap().session_id();
    let pending = p.pending.as_ref().unwrap();
    let bytes = pending.bytes.clone();
    let identities = format!("{:?}", pending.identities);
    let b = p
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .database_path();
    let c = p
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .database_path();
    let before = fs::read(&f.path).unwrap();
    crate::backup::restore::close_results::take();
    // This is exactly the production operation. No checkpoint, verification,
    // file lookup, allocation or callback is hidden in this pure move.
    let epoch = input.into_epoch_inputs();
    assert!(crate::backup::restore::close_results::take().is_empty());
    let p = parts(&epoch.inputs);
    assert_eq!(descriptors(p), fds);
    assert!(std::ptr::eq(p.maintenance, &m));
    assert_eq!(
        format!("{:?}", p.pending.as_ref().unwrap().identities),
        identities
    );
    assert_eq!(p.pending.as_ref().unwrap().bytes, bytes);
    observe(p, id, &m, &f);
    assert!(b.is_file() && c.is_file());
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    // Normal Drop is safe in this UNOPENED state: no writer API exists.
    let seen = Rc::new(Cell::new(false));
    let hit = seen.clone();
    crate::database_state::recovery_session::reservation_drop_probe::with(
        move || {
            assert!(!b.exists() && !c.exists());
            hit.set(true);
        },
        || drop(epoch),
    );
    assert!(seen.get());
    assert_eq!(
        crate::backup::restore::close_results::take(),
        [rusqlite::ffi::SQLITE_OK, rusqlite::ffi::SQLITE_OK]
    );
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    blocked(&m, &f);
}
#[test]
fn epoch_inputs_error_return_keeps_the_same_resources_and_reservation() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let input = inputs(&f, &m);
    let id = parts(&input).session.as_ref().unwrap().session_id();
    let fds = descriptors(parts(&input));
    let expected = parts(&input).pending.as_ref().unwrap().bytes.clone();
    // The production move is infallible. Model a caller's subsequent error by
    // returning the ENTIRE owner; there is no partial owner or success proof.
    fn rejected<'m, 's>(
        input: WriteInputs<'m, 's>,
    ) -> std::result::Result<(), Box<WriteEpochInputs<'m, 's>>> {
        Err(Box::new(input.into_epoch_inputs()))
    }
    let Err(epoch) = rejected(input) else {
        panic!("intended error return")
    };
    assert_eq!(descriptors(parts(&epoch.inputs)), fds);
    observe(parts(&epoch.inputs), id, &m, &f);
    assert_eq!(
        fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
        expected
    );
    drop(epoch);
    blocked(&m, &f);
}
#[test]
fn epoch_inputs_panics_before_and_after_move_retain_whole_chain() {
    for after in [false, true] {
        let f = Fixture::new();
        let m = f.state.maintenance().unwrap();
        let input = inputs(&f, &m);
        let p = parts(&input);
        let expected = p.pending.as_ref().unwrap().bytes.clone();
        let b = p
            .sources
            .live()
            .unwrap()
            .incoming
            .as_ref()
            .unwrap()
            .database_path();
        let c = p
            .sources
            .live()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .candidate()
            .database_path();
        let main = fs::read(&f.path).unwrap();
        let hit = Cell::new(false);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if after {
                let _epoch = input.into_epoch_inputs();
                hit.set(true);
                panic!("after whole-owner move");
            } else {
                let _input = input;
                hit.set(true);
                panic!("before whole-owner move");
            }
        }));
        let payload = caught.unwrap_err();
        assert!(hit.get());
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&if after {
                "after whole-owner move"
            } else {
                "before whole-owner move"
            })
        );
        assert!(b.is_file() && c.is_file());
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        assert_eq!(
            fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
            expected
        );
        assert_eq!(fs::read(&f.path).unwrap(), main);
        blocked(&m, &f);
    }
}

#[test]
fn borrowed_verification_preserves_real_owner_and_bindings() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let p = parts(&epoch.inputs);
    let fds = descriptors(p);
    let bytes = p.pending.as_ref().unwrap().bytes.clone();
    let id = p.session.as_ref().unwrap().session_id();
    crate::backup::restore::close_results::take();
    let reached = Cell::new(false);
    assert_eq!(
        epoch.with_verification(|v| {
            reached.set(true);
            assert_eq!(v.pending_bytes(), bytes);
            assert_eq!(v.session_id(), id);
            assert_eq!(v.operation_id(), m.operation_id());
            assert_eq!(v.generation(), m.generation());
            assert_eq!(
                m.recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
        }),
        Some(())
    );
    assert!(reached.get());
    assert_eq!(descriptors(parts(&epoch.inputs)), fds);
    assert!(crate::backup::restore::close_results::take().is_empty());
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    observe(parts(&epoch.inputs), id, &m, &f);
    drop(epoch);
    blocked(&m, &f);
}

#[test]
fn borrowed_verification_callback_panic_keeps_owner_available() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let id = parts(&epoch.inputs).session.as_ref().unwrap().session_id();
    let expected = parts(&epoch.inputs).pending.as_ref().unwrap().bytes.clone();
    let reached = Cell::new(false);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        epoch.with_verification(|v| {
            assert_eq!(v.session_id(), id);
            reached.set(true);
            panic!("borrowed inspection");
        });
    }));
    assert!(reached.get());
    assert_eq!(
        caught.unwrap_err().downcast_ref::<&str>(),
        Some(&"borrowed inspection")
    );
    observe(parts(&epoch.inputs), id, &m, &f);
    assert_eq!(
        epoch.with_verification(|v| v.pending_bytes() == expected),
        Some(true)
    );
    drop(epoch);
    blocked(&m, &f);
}

#[test]
fn preflight_checks_actual_resources_without_writer_or_mutation() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let id = parts(&epoch.inputs).session.as_ref().unwrap().session_id();
    let main = fs::read(&f.path).unwrap();
    let bytes = parts(&epoch.inputs).pending.as_ref().unwrap().bytes.clone();
    let fds = descriptors(parts(&epoch.inputs));
    epoch.check_before_writer().unwrap();
    assert_eq!(descriptors(parts(&epoch.inputs)), fds);
    assert_eq!(fs::read(&f.path).unwrap(), main);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    observe(parts(&epoch.inputs), id, &m, &f);
}

#[test]
fn preflight_rejects_changed_resources_and_keeps_original_session() {
    for case in 0..10 {
        let f = Fixture::new();
        let m = f.state.maintenance().unwrap();
        let mut epoch = inputs(&f, &m).into_epoch_inputs();
        let p = epoch.inputs.owner.after.as_mut().unwrap();
        let id = p.session.as_ref().unwrap().session_id();
        let pending_path = f.root.join(DIRECTORY).join(NAME);
        let b = p
            .sources
            .live()
            .unwrap()
            .incoming
            .as_ref()
            .unwrap()
            .database_path();
        let c = p
            .sources
            .live()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .candidate()
            .database_path();
        match case {
            0 => {
                fs::write(&pending_path, b"changed P").unwrap();
            }
            1 => {
                fs::rename(&pending_path, f.root.join("old-p")).unwrap();
                fs::write(&pending_path, &p.pending.as_ref().unwrap().bytes).unwrap();
            }
            2 => {
                fs::rename(&f.path, f.root.join("old-main")).unwrap();
                fs::write(&f.path, b"replacement").unwrap();
            }
            3 => {
                p.generation += 1;
            }
            4 => {
                p.binding
                    .as_mut()
                    .unwrap()
                    .test_set_session_id(uuid::Uuid::new_v4());
            }
            5 => {
                fs::write(&b, b"changed B").unwrap();
            }
            6 => {
                fs::write(&c, b"changed C").unwrap();
            }
            7 => {
                fs::write(f.root.join("mno.sqlite-wal"), b"").unwrap();
            }
            8 => {
                fs::write(f.root.join(".MNO-COMPLETION-orphan"), b"").unwrap();
            }
            9 => {
                fs::write(
                    p.sources
                        .live()
                        .unwrap()
                        .protection
                        .as_ref()
                        .unwrap()
                        .path(),
                    b"changed backup",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let result = epoch.check_before_writer();
        if case == 4 {
            assert!(matches!(
                result,
                Err(PreflightError::Existing(ActiveCloseError::Session(_)))
            ));
        } else if case >= 7 {
            assert!(matches!(result, Err(PreflightError::Binding)));
        } else {
            assert!(matches!(
                result,
                Err(PreflightError::Existing(ActiveCloseError::Binding))
            ));
        }
        let p = parts(&epoch.inputs);
        assert_eq!(p.session.as_ref().unwrap().session_id(), id);
        assert!(p.pending.is_some() && p.sources.live().unwrap().protection.is_some());
        assert!(b.exists() && c.exists());
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        blocked(&m, &f);
    }
}

#[test]
fn preflight_panic_preserves_borrowed_owner_and_reservation() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let id = parts(&epoch.inputs).session.as_ref().unwrap().session_id();
    let bytes = parts(&epoch.inputs).pending.as_ref().unwrap().bytes.clone();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        preflight::panic_probe::with(|| epoch.check_before_writer())
    }));
    assert_eq!(
        caught.unwrap_err().downcast_ref::<&str>(),
        Some(&"preflight checkpoint")
    );
    observe(parts(&epoch.inputs), id, &m, &f);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    epoch.check_before_writer().unwrap();
}

#[test]
fn prepared_episode_consumes_whole_owner_and_drops_staging_before_reservation() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let p = parts(&epoch.inputs);
    let id = p.session.as_ref().unwrap().session_id();
    let fds = descriptors(p);
    let bytes = p.pending.as_ref().unwrap().bytes.clone();
    let b = p
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .database_path();
    let c = p
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .database_path();
    let main = fs::read(&f.path).unwrap();
    let prepared = epoch.prepare_episode().unwrap();
    let p = parts(&prepared.inputs.inputs);
    assert_eq!(descriptors(p), fds);
    assert!(std::ptr::eq(p.maintenance, &m));
    observe(p, id, &m, &f);
    assert_eq!(p.pending.as_ref().unwrap().bytes, bytes);
    assert_eq!(fs::read(&f.path).unwrap(), main);
    let hit = Rc::new(Cell::new(false));
    let seen = hit.clone();
    crate::backup::restore::close_results::take();
    crate::database_state::recovery_session::reservation_drop_probe::with(
        move || {
            assert!(!b.exists() && !c.exists());
            seen.set(true);
        },
        || drop(prepared),
    );
    assert!(hit.get());
    assert_eq!(
        crate::backup::restore::close_results::take(),
        [rusqlite::ffi::SQLITE_OK, rusqlite::ffi::SQLITE_OK]
    );
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    blocked(&m, &f);
}

#[test]
fn episode_preparation_failure_returns_original_owner_and_can_be_formatted() {
    for replace_main in [false, true] {
        let f = Fixture::new();
        let m = f.state.maintenance().unwrap();
        let epoch = inputs(&f, &m).into_epoch_inputs();
        let p = parts(&epoch.inputs);
        let id = p.session.as_ref().unwrap().session_id();
        let fds = descriptors(p);
        if replace_main {
            fs::rename(&f.path, f.root.join("old-main")).unwrap();
            fs::write(&f.path, b"wrong inode").unwrap();
        } else {
            fs::write(f.root.join(DIRECTORY).join(NAME), b"changed P").unwrap();
        }
        let failure = match epoch.prepare_episode() {
            Err(failure) => failure,
            Ok(_) => panic!("must reject changed input"),
        };
        assert!(matches!(
            failure.error(),
            PreflightError::Existing(ActiveCloseError::Binding)
        ));
        assert!(format!("{failure:?}").contains("EpisodePreparationFailure"));
        let epoch = failure.into_inputs();
        let p = parts(&epoch.inputs);
        assert_eq!(descriptors(p), fds);
        observe(p, id, &m, &f);
        assert!(p.pending.is_some() && p.sources.live().unwrap().protection.is_some());
    }
}

#[test]
fn episode_preparation_panic_retains_entire_consumed_owner() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let epoch = inputs(&f, &m).into_epoch_inputs();
    let p = parts(&epoch.inputs);
    let b = p
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .database_path();
    let c = p
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .database_path();
    let bytes = p.pending.as_ref().unwrap().bytes.clone();
    let main = fs::read(&f.path).unwrap();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        preflight::panic_probe::with(|| epoch.prepare_episode())
    }));
    let payload = match caught {
        Err(payload) => payload,
        Ok(_) => panic!("must panic in preflight"),
    };
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"preflight checkpoint")
    );
    assert!(b.is_file() && c.is_file());
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    assert_eq!(fs::read(&f.path).unwrap(), main);
    blocked(&m, &f);
}
