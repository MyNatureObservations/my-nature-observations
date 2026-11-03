use super::super::tests::Fixture;
use super::*;
use crate::backup::{self, BackupKind};
use std::os::fd::AsRawFd;
use std::{cell::Cell, rc::Rc};

pub(super) fn context<'m, 's>(f: &Fixture, m: &'m Maintenance<'s>) -> RestoreContext<'m, 's> {
    let archive = f
        ._dir
        .path()
        .join(format!("candidate-{}.mno-backup", uuid::Uuid::new_v4()));
    let source = Connection::open_with_flags(&f.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    let incoming =
        backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
    let active = ActiveDatabase::open(&f.path).unwrap();
    let protection =
        pre_restore::create_bound(m, &active, Arc::new(AtomicBool::new(false))).unwrap();
    let mut context = RestoreContext {
        terminal_policy: None,
        maintenance: m,
        active,
        incoming,
        protection: None,
        generation: m.generation(),
        next_generation: m.checked_next_generation().unwrap(),
        claimed: false,
    };
    context.attach(protection).unwrap();
    context
}
pub(super) fn blocked(m: &Maintenance<'_>, f: &Fixture) {
    m.assert_running_recovery_generation_for_test(1);
    assert!(m.ensure_usable().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
pub(super) fn staging(ctx: &RestoreContext<'_, '_>) -> [PathBuf; 2] {
    [
        ctx.incoming.database_path(),
        ctx.protection.as_ref().unwrap().candidate().database_path(),
    ]
}

fn observe(parts: &WriteParts<'_, '_>, id: uuid::Uuid, m: &Maintenance<'_>, f: &Fixture) {
    let session = parts.session.as_ref().unwrap();
    assert_eq!(session.session_id(), id);
    session.ensure_publication_phase().unwrap();
    session
        .bind_context(m)
        .unwrap()
        .check_session(session)
        .unwrap();
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    blocked(m, f);
}
#[test]
fn write_preparation_real_publication_consumes_original_handles_and_session() {
    let f = Fixture::new();
    let before = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let paths = staging(&ctx);
    let active = ctx.active.identity();
    let archive = ctx.protection.as_ref().unwrap().path().to_owned();
    let archive_bytes = fs::read(&archive).unwrap();
    let mut seen = Vec::new();
    let prep = ctx
        .publish_write_preparation_inner(&mut |p| {
            seen.push(p);
            Ok(())
        })
        .unwrap();
    assert!(seen.contains(&PreparationPoint::Publication(Point::FileSync)));
    assert!(seen.contains(&PreparationPoint::Publication(Point::DirectorySync)));
    assert_eq!(seen.last(), Some(&PreparationPoint::DurablePending));
    let pending = prep.owner.0.as_ref().unwrap().pending.as_ref().unwrap();
    let fds = (
        pending.root_file.as_raw_fd(),
        pending.directory_file.as_raw_fd(),
        pending.marker_file.as_raw_fd(),
    );
    let bytes = pending.bytes.clone();
    assert!(pending.identities.is_some());
    let session = m.recovery_session().unwrap();
    let id = session.session_id();
    let mut boundaries = Vec::new();
    let prepared = prep
        .prepare_write_inner(session, &mut |p| {
            boundaries.push(p);
            assert_eq!(
                m.recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
            blocked(&m, &f);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        boundaries,
        [PreparationPoint::SessionOwned, PreparationPoint::Bound]
    );
    let held = prepared.owner.0.as_ref().unwrap();
    observe(held, id, &m, &f);
    held.binding
        .as_ref()
        .unwrap()
        .check_session(held.session.as_ref().unwrap())
        .unwrap();
    assert!(std::ptr::eq(held.context.maintenance, &m));
    assert_eq!(held.context.active.identity(), active);
    let p = held.pending.as_ref().unwrap();
    assert_eq!(
        (
            p.root_file.as_raw_fd(),
            p.directory_file.as_raw_fd(),
            p.marker_file.as_raw_fd()
        ),
        fds
    );
    assert_eq!(p.bytes, bytes);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    let checked = Rc::new(Cell::new(false));
    let observed = checked.clone();
    let stage = paths.clone();
    crate::database_state::recovery_session::reservation_drop_probe::with(
        move || {
            assert!(
                stage.iter().all(|p| !p.exists()),
                "staging closes before reservation releases"
            );
            observed.set(true);
        },
        || drop(prepared),
    );
    assert!(checked.get());
    assert!(paths.iter().all(|p| !p.exists()));
    assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
    assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    assert_eq!(fs::read(&f.path).unwrap(), before);
    blocked(&m, &f);
    let next = m.recovery_session().unwrap();
    assert_ne!(next.session_id(), id);
    drop(next);
    drop(m);
    assert!(f.state.operation(None).is_err());
}
#[test]
fn write_preparation_errors_before_during_and_after_p_retain_context() {
    for target in [
        PreparationPoint::BeforeAuthorization,
        PreparationPoint::Publication(Point::FileSync),
        PreparationPoint::Publication(Point::Published),
        PreparationPoint::Publication(Point::DirectorySync),
        PreparationPoint::Publication(Point::FinalSync),
        PreparationPoint::DurablePending,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let hit = Cell::new(false);
        let failure = ctx
            .publish_write_preparation_inner(&mut |p| {
                if p == target {
                    hit.set(true);
                    Err(io::Error::other("injected preparation"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(hit.get());
        match target {
            PreparationPoint::Publication(_) => assert!(matches!(
                failure.error,
                WritePreparationError::Marker(MarkerError::Io(_))
            )),
            _ => assert!(matches!(failure.error, WritePreparationError::Checkpoint)),
        }
        assert!(paths.iter().all(|p| p.exists()));
        assert_eq!(fs::read(&f.path).unwrap(), before);
        if target == PreparationPoint::BeforeAuthorization {
            assert!(m.ensure_usable().is_ok());
            assert!(!f.root.join(DIRECTORY).exists());
        } else {
            blocked(&m, &f);
        }
        let exists = f.root.join(DIRECTORY).join(NAME).exists();
        drop(failure);
        assert!(paths.iter().all(|p| !p.exists()));
        assert_eq!(f.root.join(DIRECTORY).join(NAME).exists(), exists);
    }
}
#[test]
fn write_preparation_session_errors_preserve_all_owners_until_drop() {
    for target in [PreparationPoint::SessionOwned, PreparationPoint::Bound] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let prep = ctx.publish_write_preparation().unwrap();
        let bytes = fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap();
        let session = m.recovery_session().unwrap();
        let id = session.session_id();
        let hit = Cell::new(false);
        let failure = prep
            .prepare_write_inner(session, &mut |p| {
                assert_eq!(
                    m.recovery_session().err().as_deref(),
                    Some("database_recovery_busy")
                );
                if p == target {
                    hit.set(true);
                    Err(io::Error::other("transfer"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert!(hit.get());
        assert!(matches!(failure.error, WritePreparationError::Checkpoint));
        observe(failure.owner.0.as_ref().unwrap(), id, &m, &f);
        assert!(paths.iter().all(|p| p.exists()));
        drop(failure);
        assert!(paths.iter().all(|p| !p.exists()));
        blocked(&m, &f);
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
        assert_eq!(fs::read(&f.path).unwrap(), before);
        let next = m.recovery_session().unwrap();
        assert_ne!(next.session_id(), id);
    }
}
#[test]
fn write_preparation_wrong_maintenance_session_is_retained_and_rejected() {
    let f = Fixture::new();
    let other = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let foreign = other.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let paths = staging(&ctx);
    let prep = ctx.publish_write_preparation().unwrap();
    let session = foreign.recovery_session().unwrap();
    let id = session.session_id();
    let failure = prep.prepare_write(session).unwrap_err();
    assert!(
        matches!(&failure.error,WritePreparationError::Session(e) if e=="database_context_session_maintenance")
    );
    assert_eq!(
        failure
            .owner
            .0
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .session_id(),
        id
    );
    assert_eq!(
        foreign.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(paths.iter().all(|p| p.exists()));
    blocked(&m, &f);
    drop(failure);
    assert!(paths.iter().all(|p| !p.exists()));
}
#[test]
fn write_preparation_startup_session_cannot_bind_running_context() {
    let f = Fixture::new();
    let other = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let prep = ctx.publish_write_preparation().unwrap();
    fs::create_dir(other.root.join(DIRECTORY)).unwrap();
    drop(other.state);
    let startup = crate::database_state::DatabaseState::default();
    assert!(startup.initialize_legacy_startup(&other.root).is_err());
    let session = startup.recovery_session().unwrap();
    let id = session.session_id();
    let failure = prep.prepare_write(session).unwrap_err();
    assert!(
        matches!(&failure.error,WritePreparationError::Session(e) if e=="database_context_session_origin")
    );
    assert_eq!(
        failure
            .owner
            .0
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .session_id(),
        id
    );
    assert_eq!(
        startup.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    blocked(&m, &f);
}
#[test]
fn write_preparation_changed_p_fails_after_session_is_safely_owned() {
    let f = Fixture::new();
    let m = f.state.maintenance().unwrap();
    let prep = context(&f, &m).publish_write_preparation().unwrap();
    let session = m.recovery_session().unwrap();
    let id = session.session_id();
    fs::write(f.root.join(DIRECTORY).join(NAME), b"changed").unwrap();
    let failure = prep.prepare_write(session).unwrap_err();
    assert!(matches!(
        failure.error,
        WritePreparationError::Marker(MarkerError::Binding)
    ));
    observe(failure.owner.0.as_ref().unwrap(), id, &m, &f);
    assert_eq!(
        fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
        b"changed"
    );
}
#[test]
fn write_preparation_panics_retain_context_after_p_and_original_reserved_session() {
    for target in [PreparationPoint::SessionOwned, PreparationPoint::Bound] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let prep = ctx.publish_write_preparation().unwrap();
        let bytes = fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap();
        let session = m.recovery_session().unwrap();
        let hit = Cell::new(false);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prep.prepare_write_inner(session, &mut |p| {
                assert_eq!(
                    m.recovery_session().err().as_deref(),
                    Some("database_recovery_busy")
                );
                if p == target {
                    hit.set(true);
                    panic!("owned session transfer");
                }
                Ok(())
            })
        }));
        let payload = caught.unwrap_err();
        assert!(hit.get());
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"owned session transfer")
        );
        assert!(paths.iter().all(|p| p.exists()));
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
        assert_eq!(fs::read(&f.path).unwrap(), before);
        blocked(&m, &f);
    }
    for target in [
        PreparationPoint::BeforeAuthorization,
        PreparationPoint::Publication(Point::Published),
        PreparationPoint::DurablePending,
    ] {
        let f = Fixture::new();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let hit = Cell::new(false);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.publish_write_preparation_inner(&mut |p| {
                if p == target {
                    hit.set(true);
                    panic!("prepare");
                }
                Ok(())
            })
        }));
        let payload = caught.unwrap_err();
        assert!(hit.get());
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"prepare"));
        assert!(paths.iter().all(|p| p.exists()));
        if target != PreparationPoint::BeforeAuthorization {
            blocked(&m, &f);
        }
    }
}

#[test]
fn write_preparation_busy_protection_close_keeps_connection_and_staging_together() {
    let f = Fixture::new();
    let before = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let paths = staging(&ctx);
    let prep = ctx.publish_write_preparation().unwrap();
    // A real outstanding SQLite statement, leaked using safe Rust. This test
    // deliberately retains it for the process lifetime; no raw handle access.
    let statement = prep
        .owner
        .0
        .as_ref()
        .unwrap()
        .context
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .source()
        .unwrap()
        .prepare("SELECT 1")
        .unwrap();
    std::mem::forget(statement);
    let prepared = prep.prepare_write(m.recovery_session().unwrap()).unwrap();
    crate::backup::restore::close_results::take();
    drop(prepared);
    let results = crate::backup::restore::close_results::take();
    assert!(
        results.contains(&rusqlite::ffi::SQLITE_BUSY),
        "must reach actual SQLITE_BUSY"
    );
    assert!(!paths[0].exists());
    assert!(
        paths[1].is_file(),
        "busy connection must retain its staging owner"
    );
    assert!(f.root.join(DIRECTORY).join(NAME).is_file());
    assert_eq!(fs::read(&f.path).unwrap(), before);
    blocked(&m, &f);
}

#[test]
fn write_preparation_direct_durable_drop_preserves_p_and_closes_before_staging_cleanup() {
    for busy in [false, true] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let generation = m.generation();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let archive = ctx.protection.as_ref().unwrap().path().to_owned();
        let archive_bytes = fs::read(&archive).unwrap();
        let mut points = Vec::new();
        let prep = ctx
            .publish_write_preparation_inner(&mut |point| {
                points.push(point);
                Ok(())
            })
            .unwrap();
        // Returning this actual capability requires successful sync calls, not
        // merely entry into their hooks. Require their order as well.
        let at = |point| points.iter().position(|p| *p == point).unwrap();
        assert!(
            at(PreparationPoint::Publication(Point::FileSync))
                < at(PreparationPoint::Publication(Point::Published))
        );
        assert!(
            at(PreparationPoint::Publication(Point::Published))
                < at(PreparationPoint::Publication(Point::DirectorySync))
        );
        assert!(
            at(PreparationPoint::Publication(Point::DirectorySync))
                < at(PreparationPoint::Publication(Point::FinalSync))
        );
        assert_eq!(points.last(), Some(&PreparationPoint::DurablePending));
        let held = prep.owner.0.as_ref().unwrap();
        assert!(held.session.is_none());
        assert!(held.binding.is_none());
        let pending = held.pending.as_ref().unwrap();
        let bytes = pending.bytes.clone();
        let name = f.root.join(DIRECTORY).join(NAME);
        assert_eq!(
            parse(&bytes).unwrap().restore_id,
            m.operation_id().to_string()
        );
        assert_eq!(fs::read(&name).unwrap(), bytes);
        assert!(paths.iter().all(|p| p.is_file()));
        blocked(&m, &f);
        if busy {
            // Safe Rust creates a REAL outstanding statement. The existing
            // retention contract intentionally keeps it and staging thereafter.
            let statement = held
                .context
                .protection
                .as_ref()
                .unwrap()
                .candidate()
                .source()
                .unwrap()
                .prepare("SELECT 1")
                .unwrap();
            std::mem::forget(statement);
        }
        crate::backup::restore::close_results::take();
        let reservation_drops = Rc::new(Cell::new(0));
        let observed = reservation_drops.clone();
        crate::database_state::recovery_session::reservation_drop_probe::with(
            move || observed.set(observed.get() + 1),
            || drop(prep), // Direct intermediate-owner Drop; no prepare_write().
        );
        assert_eq!(reservation_drops.get(), 0);
        let close_results = crate::backup::restore::close_results::take();
        assert_eq!(
            close_results,
            [
                rusqlite::ffi::SQLITE_OK,
                if busy {
                    rusqlite::ffi::SQLITE_BUSY
                } else {
                    rusqlite::ffi::SQLITE_OK
                }
            ]
        );
        assert!(
            !paths[0].exists(),
            "confirmed incoming close permits cleanup"
        );
        assert_eq!(
            paths[1].is_file(),
            busy,
            "protection staging must survive BUSY and clean after confirmed close"
        );
        assert_eq!(
            fs::read(&name).unwrap(),
            bytes,
            "P must survive direct Drop unchanged"
        );
        assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
        assert_eq!(fs::read(&f.path).unwrap(), before);
        assert_eq!(m.generation(), generation);
        blocked(&m, &f);
        drop(m);
        assert!(
            f.state.operation(Some(generation)).is_err(),
            "Drop must not admit Ready"
        );
    }
}

mod compiler_tests;
