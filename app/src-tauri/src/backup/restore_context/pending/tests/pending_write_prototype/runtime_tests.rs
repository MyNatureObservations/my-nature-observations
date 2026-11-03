use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn context<'m, 's>(f: &Fixture, m: &'m Maintenance<'s>) -> RestoreContext<'m, 's> {
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
fn blocked(m: &Maintenance<'_>, f: &Fixture) {
    m.assert_running_recovery_generation_for_test(1);
    assert!(m.ensure_usable().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
// The coordinator owns this one session outside prepare(), including unwind.
// Observe the real slot at EACH stable transfer boundary, not just afterwards.
fn observe_session(
    session: &crate::database_state::recovery_session::RecoverySession<'_, '_>,
    expected: uuid::Uuid,
    m: &Maintenance<'_>,
    f: &Fixture,
) {
    assert_eq!(session.session_id(), expected);
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
fn transfer_checkpoint<'m, 's>(
    point: Checkpoint,
    session: &mut Option<crate::database_state::recovery_session::RecoverySession<'m, 's>>,
    id: &mut Option<uuid::Uuid>,
    observed: &mut Vec<Checkpoint>,
    m: &'m Maintenance<'s>,
    f: &Fixture,
) {
    if point == Checkpoint::DurablePending {
        assert!(session.is_none(), "must acquire exactly once");
        let original = m.recovery_session().unwrap();
        *id = Some(original.session_id());
        *session = Some(original);
    }
    if matches!(
        point,
        Checkpoint::DurablePending | Checkpoint::BeforeMove | Checkpoint::AfterMove
    ) {
        observe_session(
            session.as_ref().expect("session before transfer"),
            id.unwrap(),
            m,
            f,
        );
        observed.push(point);
    }
}
fn staging(ctx: &RestoreContext<'_, '_>) -> [PathBuf; 2] {
    [
        ctx.incoming.database_path(),
        ctx.protection.as_ref().unwrap().candidate().database_path(),
    ]
}
#[test]
fn pending_write_prototype_moves_real_context_and_durable_pending_without_new_guards() {
    let f = Fixture::new();
    let journal = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let paths = staging(&ctx);
    let active = ctx.active.identity();
    let operation = m.operation_id();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    let mut session = None;
    let mut id = None;
    let mut observed = Vec::new();
    let mut checkpoint = |point| {
        transfer_checkpoint(point, &mut session, &mut id, &mut observed, &m, &f);
        record.borrow_mut().push(point);
        Ok(())
    };
    let mut prepared = prepare(ctx, &mut checkpoint).unwrap();
    let held = prepared.retained.as_mut().unwrap();
    assert!(std::ptr::eq(held.context.maintenance, &m));
    assert_eq!(held.context.active.identity(), active);
    let p = held.pending.as_mut().unwrap();
    p.verify(&held.context).unwrap();
    assert_eq!(p.operation, operation);
    assert_eq!(p.generation, 1);
    assert!(p.identities.is_some());
    assert_eq!(
        fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
        p.bytes
    );
    assert_eq!(
        identity(&p.marker_file).unwrap(),
        p.identities.as_ref().unwrap().2
    );
    // Real publish_with returns only after all mandatory sync calls succeeded.
    let points = seen.borrow();
    assert_eq!(points.first(), Some(&Checkpoint::BeforeAuthorization));
    let published = points
        .iter()
        .position(|p| *p == Checkpoint::Publication(Point::Published))
        .unwrap();
    let final_sync = points
        .iter()
        .position(|p| *p == Checkpoint::Publication(Point::FinalSync))
        .unwrap();
    let durable = points
        .iter()
        .position(|p| *p == Checkpoint::DurablePending)
        .unwrap();
    assert!(published < final_sync && final_sync < durable);
    assert!(
        points[published + 1..final_sync].contains(&Checkpoint::Publication(Point::DirectorySync))
    );
    assert_eq!(points.last(), Some(&Checkpoint::AfterMove));
    assert!(paths.iter().all(|p| p.exists()));
    assert_eq!(fs::read(&f.path).unwrap(), journal);
    blocked(&m, &f);
    assert_eq!(
        observed,
        [
            Checkpoint::DurablePending,
            Checkpoint::BeforeMove,
            Checkpoint::AfterMove
        ]
    );
    let session = session.unwrap();
    let id = id.unwrap();
    observe_session(&session, id, &m, &f);
    let binding = session.bind_context(&m).unwrap();
    drop(prepared);
    assert!(paths.iter().all(|p| !p.exists()));
    assert_eq!(session.session_id(), id);
    binding.check_session(&session).unwrap();
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(f.root.join(DIRECTORY).join(NAME).exists());
    assert_eq!(fs::read(&f.path).unwrap(), journal);
    blocked(&m, &f);
    drop(binding);
    drop(session);
    // Negative control: this exact Busy observation stops holding when the
    // original slot is released. A premature release would fail checkpoints.
    let successor = m.recovery_session().unwrap();
    assert_ne!(successor.session_id(), id);
    drop(successor);
    drop(m);
    assert!(f.state.operation(Some(1)).is_err());
}
#[test]
fn pending_write_prototype_failure_before_p_retains_context_and_allows_only_prior_ready() {
    let f = Fixture::new();
    let before = fs::read(&f.path).unwrap();
    let m = f.state.maintenance().unwrap();
    let ctx = context(&f, &m);
    let paths = staging(&ctx);
    let failure = prepare(ctx, &mut |p| {
        if p == Checkpoint::BeforeAuthorization {
            Err(io::Error::other("before P"))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(matches!(failure.error, PrepareError::Checkpoint));
    assert!(failure.retained.pending.is_none());
    assert!(paths.iter().all(|p| p.exists()));
    assert!(!f.root.join(DIRECTORY).exists());
    assert!(m.ensure_usable().is_ok());
    assert_eq!(m.generation(), 1);
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    drop(failure);
    assert!(paths.iter().all(|p| !p.exists()));
    drop(m);
    assert_eq!(
        f.state.operation(Some(1)).unwrap().snapshot(()).generation,
        1
    );
    assert_eq!(fs::read(&f.path).unwrap(), before);
}
#[test]
fn pending_write_prototype_publication_errors_never_issue_prepared_capability_or_unlock() {
    for target in [
        Point::FileSync,
        Point::Published,
        Point::DirectorySync,
        Point::FinalSync,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let hit = Cell::new(false);
        let failure = prepare(ctx, &mut |p| {
            if p == Checkpoint::Publication(target) {
                hit.set(true);
                Err(io::Error::other("publication"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(hit.get());
        assert!(matches!(
            failure.error,
            PrepareError::Marker(MarkerError::Io(_))
        ));
        assert!(failure.retained.pending.is_none());
        assert!(paths.iter().all(|p| p.exists()));
        assert!(f.root.join(DIRECTORY).exists());
        assert_eq!(
            f.root.join(DIRECTORY).join(NAME).exists(),
            target != Point::FileSync
        );
        blocked(&m, &f);
        drop(failure);
        assert!(paths.iter().all(|p| !p.exists()));
        assert_eq!(fs::read(&f.path).unwrap(), before);
        drop(m);
        assert!(f.state.operation(None).is_err());
    }
}
#[test]
fn pending_write_prototype_post_publication_failures_keep_actual_p_files_and_context() {
    for target in [
        Checkpoint::DurablePending,
        Checkpoint::BeforeMove,
        Checkpoint::AfterMove,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let hit = Cell::new(false);
        let mut session = None;
        let mut id = None;
        let mut observed = Vec::new();
        let failure = prepare(ctx, &mut |p| {
            transfer_checkpoint(p, &mut session, &mut id, &mut observed, &m, &f);
            if p == target {
                hit.set(true);
                Err(io::Error::other("after P"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(hit.get());
        assert!(matches!(failure.error, PrepareError::Checkpoint));
        let p = failure.retained.pending.as_ref().unwrap();
        let bytes = p.bytes.clone();
        assert!(p.identities.is_some());
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
        assert!(paths.iter().all(|p| p.exists()));
        blocked(&m, &f);
        let expected = [
            Checkpoint::DurablePending,
            Checkpoint::BeforeMove,
            Checkpoint::AfterMove,
        ];
        let count = expected.iter().position(|p| *p == target).unwrap() + 1;
        assert_eq!(observed, expected[..count]);
        let session = session.unwrap();
        let id = id.unwrap();
        observe_session(&session, id, &m, &f);
        drop(failure);
        assert!(paths.iter().all(|p| !p.exists()));
        observe_session(&session, id, &m, &f);
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
fn pending_write_prototype_panics_during_transfer_keep_complete_aggregate() {
    for target in [
        Checkpoint::DurablePending,
        Checkpoint::BeforeMove,
        Checkpoint::AfterMove,
    ] {
        let f = Fixture::new();
        let before = fs::read(&f.path).unwrap();
        let m = f.state.maintenance().unwrap();
        let ctx = context(&f, &m);
        let paths = staging(&ctx);
        let hit = Cell::new(false);
        let mut session = None;
        let mut id = None;
        let mut observed = Vec::new();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prepare(ctx, &mut |p| {
                transfer_checkpoint(p, &mut session, &mut id, &mut observed, &m, &f);
                if p == target {
                    hit.set(true);
                    panic!("transfer");
                }
                Ok(())
            })
        }));
        let payload = caught.expect_err("injected transfer panic must be caught");
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"transfer"));
        assert!(hit.get());
        assert!(paths.iter().all(|p| p.exists()));
        assert!(f.root.join(DIRECTORY).join(NAME).exists());
        assert_eq!(fs::read(&f.path).unwrap(), before);
        blocked(&m, &f);
        let expected = [
            Checkpoint::DurablePending,
            Checkpoint::BeforeMove,
            Checkpoint::AfterMove,
        ];
        let count = expected.iter().position(|p| *p == target).unwrap() + 1;
        assert_eq!(observed, expected[..count]);
        let session = session.unwrap();
        observe_session(&session, id.unwrap(), &m, &f);
    }
}
