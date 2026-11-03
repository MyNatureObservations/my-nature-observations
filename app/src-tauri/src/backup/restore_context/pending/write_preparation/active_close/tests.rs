use super::super::tests::{blocked, context, staging};
use super::*;
use crate::backup::restore_context::close::hooks;
use crate::backup::restore_context::pending::tests::Fixture;
use std::{cell::Cell, rc::Rc};

fn prepared<'m, 's>(f: &Fixture, m: &'m Maintenance<'s>) -> PendingWrite<'m, 's> {
    context(f, m)
        .publish_write_preparation()
        .unwrap()
        .prepare_write(m.recovery_session().unwrap())
        .unwrap()
}
fn observe(parts: &ActiveCloseParts<'_, '_>, id: uuid::Uuid, m: &Maintenance<'_>, f: &Fixture) {
    assert!(matches!(&parts.sources, SourcePhase::Live(_)));
    let session = parts.session.as_ref().unwrap();
    assert_eq!(session.session_id(), id);
    parts
        .binding
        .as_ref()
        .unwrap()
        .check_session(session)
        .unwrap();
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(
        !parts
            .sources
            .live()
            .unwrap()
            .incoming
            .as_ref()
            .unwrap()
            .source()
            .unwrap()
            .is_autocommit()
    );
    assert!(
        !parts
            .sources
            .live()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .candidate()
            .source()
            .unwrap()
            .is_autocommit()
    );
    blocked(m, f);
}
#[test]
fn active_only_close_keeps_b_c_pending_files_and_original_session() {
    let f = Fixture::new();
    let before = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let pw = prepared(&f, &m);
    let original = pw.owner.0.as_ref().unwrap();
    let paths = staging(&original.context);
    use std::os::unix::fs::MetadataExt;
    let source_identities = paths.each_ref().map(|path| {
        let metadata = fs::metadata(path).unwrap();
        (metadata.dev(), metadata.ino())
    });
    let bytes = original.pending.as_ref().unwrap().bytes.clone();
    let id = original.session.as_ref().unwrap().session_id();
    let a_identity = original.context.active.identity();
    let archive = original
        .context
        .protection
        .as_ref()
        .unwrap()
        .path()
        .to_owned();
    let archive_bytes = fs::read(&archive).unwrap();
    crate::backup::restore::close_results::take();
    let visits = Rc::new(Cell::new(0));
    let seen = visits.clone();
    let mut checkpoints = Vec::new();
    let inputs = hooks::with(
        move |p| {
            assert_eq!(p, "active-close-guard");
            seen.set(seen.get() + 1);
        },
        || {
            pw.close_active_inner(&mut |p| {
                assert_eq!(
                    m.recovery_session().err().as_deref(),
                    Some("database_recovery_busy")
                );
                checkpoints.push(p);
                Ok(())
            })
        },
    )
    .unwrap();
    assert_eq!(visits.get(), 1, "exactly one real A-close attempt");
    assert_eq!(
        checkpoints,
        [
            ClosePoint::BeforeClose,
            ClosePoint::OwnersMoved,
            ClosePoint::AfterClose
        ]
    );
    assert!(
        crate::backup::restore::close_results::take().is_empty(),
        "neither B nor C attempted close"
    );
    assert!(inputs.owner.before.is_none());
    let parts = inputs.owner.after.as_ref().unwrap();
    let ActiveState::Closed(files) = &parts.active else {
        panic!("A must be confirmed closed")
    };
    assert!(matches!(&parts.sources, SourcePhase::Live(_)));
    assert_eq!(
        paths.each_ref().map(|path| {
            let metadata = fs::metadata(path).unwrap();
            (metadata.dev(), metadata.ino())
        }),
        source_identities
    );
    assert_eq!(files.identity, a_identity);
    assert!(private::same_file(&files.file, &f.path).unwrap());
    observe(parts, id, &m, &f);
    assert!(paths.iter().all(|p| p.is_file()));
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let drop_seen = Rc::new(Cell::new(false));
    let reached = drop_seen.clone();
    let stage = paths.clone();
    crate::database_state::recovery_session::reservation_drop_probe::with(
        move || {
            assert!(stage.iter().all(|p| !p.exists()));
            reached.set(true);
        },
        || drop(inputs),
    );
    assert!(drop_seen.get());
    assert_eq!(
        crate::backup::restore::close_results::take(),
        [rusqlite::ffi::SQLITE_OK, rusqlite::ffi::SQLITE_OK]
    );
    assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    blocked(&m, &f);
    let next = m.recovery_session().unwrap();
    assert_ne!(next.session_id(), id);
    drop(next);
    drop(m);
    assert!(f.state.operation(None).is_err());
}
#[test]
fn active_only_close_real_busy_retains_live_a_and_b_c() {
    let f = Fixture::new();
    let before = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let pw = prepared(&f, &m);
    let original = pw.owner.0.as_ref().unwrap();
    let paths = staging(&original.context);
    let id = original.session.as_ref().unwrap().session_id();
    let bytes = original.pending.as_ref().unwrap().bytes.clone();
    // Real outstanding statement in A, without unsafe/raw handle manipulation.
    std::mem::forget(
        original
            .context
            .active
            .connection()
            .prepare("SELECT 1")
            .unwrap(),
    );
    crate::backup::restore::close_results::take();
    let failure = pw.close_active().unwrap_err();
    assert!(
        matches!(failure.error(),ActiveCloseError::Sqlite(rusqlite::Error::SqliteFailure(e,_)) if e.extended_code==rusqlite::ffi::SQLITE_BUSY)
    );
    let parts = failure.owner.after.as_ref().unwrap();
    let ActiveState::Live(active) = &parts.active else {
        panic!("BUSY must retain live connection")
    };
    assert_eq!(
        active
            .connection()
            .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    active.ensure_current().unwrap();
    observe(parts, id, &m, &f);
    assert!(paths.iter().all(|p| p.is_file()));
    assert!(crate::backup::restore::close_results::take().is_empty());
    drop(failure);
    assert!(paths.iter().all(|p| !p.exists()));
    assert_eq!(fs::read(&f.path).unwrap(), before);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    blocked(&m, &f);
}
#[test]
fn active_only_close_injected_io_error_retains_connection_and_reports_exact_error() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let pw = prepared(&f, &m);
    let id = pw
        .owner
        .0
        .as_ref()
        .unwrap()
        .session
        .as_ref()
        .unwrap()
        .session_id();
    let failure =
        hooks::with_close_error(rusqlite::ffi::SQLITE_IOERR, || pw.close_active()).unwrap_err();
    assert!(
        matches!(failure.error(),ActiveCloseError::Sqlite(rusqlite::Error::SqliteFailure(e,_)) if e.extended_code==rusqlite::ffi::SQLITE_IOERR)
    );
    let parts = failure.owner.after.as_ref().unwrap();
    let ActiveState::Live(active) = &parts.active else {
        panic!("live A")
    };
    assert!(active.connection().is_autocommit());
    observe(parts, id, &m, &f);
}
#[test]
fn active_only_close_checkpoint_errors_keep_correct_progress_and_all_guards() {
    for target in [
        ClosePoint::BeforeClose,
        ClosePoint::OwnersMoved,
        ClosePoint::AfterClose,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let pw = prepared(&f, &m);
        let original = pw.owner.0.as_ref().unwrap();
        let paths = staging(&original.context);
        let id = original.session.as_ref().unwrap().session_id();
        let bytes = original.pending.as_ref().unwrap().bytes.clone();
        let hit = Cell::new(false);
        let visits = Rc::new(Cell::new(0));
        let reached = visits.clone();
        let failure = hooks::with(
            move |p| {
                assert_eq!(p, "active-close-guard");
                reached.set(reached.get() + 1)
            },
            || {
                pw.close_active_inner(&mut |p| {
                    assert_eq!(
                        m.recovery_session().err().as_deref(),
                        Some("database_recovery_busy")
                    );
                    if p == target {
                        hit.set(true);
                        Err(io::Error::other("checkpoint"))
                    } else {
                        Ok(())
                    }
                })
            },
        )
        .unwrap_err();
        assert!(hit.get());
        assert!(matches!(failure.error(), ActiveCloseError::Checkpoint));
        assert_eq!(visits.get(), usize::from(target == ClosePoint::AfterClose));
        if target == ClosePoint::BeforeClose {
            let parts = failure.owner.before.as_ref().unwrap().0.as_ref().unwrap();
            assert_eq!(parts.session.as_ref().unwrap().session_id(), id);
            assert!(failure.owner.after.is_none());
        } else {
            let parts = failure.owner.after.as_ref().unwrap();
            observe(parts, id, &m, &f);
            assert_eq!(
                matches!(parts.active, ActiveState::Closed(_)),
                target == ClosePoint::AfterClose
            );
        }
        assert!(paths.iter().all(|p| p.is_file()));
        blocked(&m, &f);
        drop(failure);
        assert!(paths.iter().all(|p| !p.exists()));
        assert_eq!(fs::read(&f.path).unwrap(), before);
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
        blocked(&m, &f);
    }
}
#[test]
fn active_only_close_panics_retain_original_session_and_b_c_at_every_boundary() {
    // The fourth case panics INSIDE the existing real A-close guard, with a
    // real outstanding statement. The aggregate and A's file guard retain deps.
    for target in 0..4 {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let pw = prepared(&f, &m);
        let original = pw.owner.0.as_ref().unwrap();
        let paths = staging(&original.context);
        let bytes = original.pending.as_ref().unwrap().bytes.clone();
        if target == 3 {
            std::mem::forget(
                original
                    .context
                    .active
                    .connection()
                    .prepare("SELECT 1")
                    .unwrap(),
            );
        }
        let hit = Rc::new(Cell::new(false));
        let internal = hit.clone();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |p| {
                    assert_eq!(p, "active-close-guard");
                    if target == 3 {
                        internal.set(true);
                        panic!("inside A close");
                    }
                },
                || {
                    pw.close_active_inner(&mut |p| {
                        assert_eq!(
                            m.recovery_session().err().as_deref(),
                            Some("database_recovery_busy")
                        );
                        if matches!(
                            (target, p),
                            (0, ClosePoint::BeforeClose)
                                | (1, ClosePoint::OwnersMoved)
                                | (2, ClosePoint::AfterClose)
                        ) {
                            hit.set(true);
                            panic!("A transfer");
                        }
                        Ok(())
                    })
                },
            )
        }));
        let payload = caught.unwrap_err();
        assert!(hit.get());
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&if target == 3 {
                "inside A close"
            } else {
                "A transfer"
            })
        );
        assert!(paths.iter().all(|p| p.is_file()));
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
        assert_eq!(fs::read(&f.path).unwrap(), before);
        blocked(&m, &f);
    }
}
#[test]
fn active_only_close_rejects_changed_p_before_first_close() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let pw = prepared(&f, &m);
    let hit = Cell::new(false);
    fs::write(f.root.join(DIRECTORY).join(NAME), b"changed").unwrap();
    let failure = pw
        .close_active_inner(&mut |_| {
            hit.set(true);
            Ok(())
        })
        .unwrap_err();
    assert!(!hit.get());
    assert!(matches!(
        failure.error(),
        ActiveCloseError::Marker(MarkerError::Binding)
    ));
    assert!(failure.owner.after.is_none());
    assert!(
        failure
            .owner
            .before
            .as_ref()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .context
            .active
            .connection()
            .is_autocommit()
    );
    blocked(&m, &f);
}

#[test]
fn active_only_close_after_check_rejects_p_bytes_changed_after_successful_close() {
    let f = Fixture::new();
    let journal = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let pw = prepared(&f, &m);
    let original = pw.owner.0.as_ref().unwrap();
    let paths = staging(&original.context);
    let id = original.session.as_ref().unwrap().session_id();
    let expected = original.pending.as_ref().unwrap().bytes.clone();
    let mut changed = expected.clone();
    // Same length and same file object: exercise exact-byte comparison, not
    // rejection by length, identity, or an earlier publication/binding check.
    changed[0] ^= 1;
    let marker = f.root.join(DIRECTORY).join(NAME);
    let mut checkpoints = Vec::new();
    let failure = pw
        .close_active_inner(&mut |point| {
            assert_eq!(
                m.recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
            checkpoints.push(point);
            if point == ClosePoint::AfterClose {
                fs::write(&marker, &changed).unwrap();
            }
            // Do not inject Checkpoint: continue into the real check_after().
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        checkpoints,
        [
            ClosePoint::BeforeClose,
            ClosePoint::OwnersMoved,
            ClosePoint::AfterClose
        ]
    );
    assert!(
        matches!(failure.error(), ActiveCloseError::Binding),
        "expected byte-check Binding, got {:?}",
        failure.error()
    );
    assert!(failure.owner.before.is_none());
    let parts = failure.owner.after.as_ref().unwrap();
    assert!(
        matches!(parts.active, ActiveState::Closed(_)),
        "A-close must have succeeded before rejection"
    );
    observe(parts, id, &m, &f);
    assert_eq!(parts.pending.as_ref().unwrap().bytes, expected);
    assert_eq!(fs::read(&marker).unwrap(), changed);
    assert!(paths.iter().all(|p| p.is_file()));
    assert_eq!(fs::read(&f.path).unwrap(), journal);
    // Dropping the error may safely close B/C, but cannot remove P or Ready
    // the database. All original guards were present until this explicit Drop.
    drop(failure);
    assert_eq!(fs::read(&marker).unwrap(), changed);
    assert_eq!(fs::read(&f.path).unwrap(), journal);
    blocked(&m, &f);
}

mod ownership_tests;

mod borrow_integration;

mod internal_close_panic;

mod terminal_guard;

mod terminal_policy;

#[test]
fn live_sources_without_b_rejects_the_borrow_before_resource_checks() {
    // No real B is extracted or dropped to construct this invalid test-only phase.
    // Both check_after and preflight require this loan before their first I/O.
    let sources = SourcePhase::Live(LiveSources {
        incoming: None,
        protection: None,
    });
    assert!(matches!(sources.live(), Err(ActiveCloseError::Binding)));
    assert!(matches!(
        sources,
        SourcePhase::Live(LiveSources {
            incoming: None,
            protection: None
        })
    ));
}

mod b_close_outcome;

mod production_b_close;

mod production_c_close;

mod early_pending;

mod borrowed_pending;

mod publication_files;
