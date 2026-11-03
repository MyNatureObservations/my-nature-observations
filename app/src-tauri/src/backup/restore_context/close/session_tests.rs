use super::tests::scenario;
use super::*;
use std::{cell::Cell, rc::Rc};

fn rejected_before_close<'a, 's>(
    ctx: RestoreContext<'a, 's>,
    session: &RecoverySession<'_, '_>,
) -> ContextCloseFailure<'a, 's> {
    let hits = Rc::new(Cell::new(0));
    let counted = hits.clone();
    let failure = hooks::with(
        move |_| counted.set(counted.get() + 1),
        || ctx.close_owned_connections(session),
    )
    .err()
    .expect("invalid binding must not issue a proof");
    assert_eq!(
        hits.get(),
        0,
        "binding must be rejected before ownership transfer or close"
    );
    assert!(matches!(&*failure._retained, Retained::Before(_)));
    failure
}
fn all_live(failure: &ContextCloseFailure<'_, '_>) {
    let Retained::Before(ctx) = &*failure._retained else {
        panic!("original owners required")
    };
    assert_eq!(
        ctx.active
            .connection()
            .query_row("SELECT 1", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        1
    );
    ctx.incoming.source().unwrap();
    ctx.protection
        .as_ref()
        .unwrap()
        .candidate()
        .source()
        .unwrap();
    assert!(ctx.incoming.database_path().is_file());
    assert!(
        ctx.protection
            .as_ref()
            .unwrap()
            .candidate()
            .database_path()
            .is_file()
    );
}
#[test]
fn startup_origin_is_rejected_before_any_close_and_retains_both_reservations() {
    scenario(|ctx, original, _| {
        let directory = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(directory.path().join("recovery")).unwrap();
        let state = DatabaseState::default();
        assert!(state.initialize_legacy_startup(directory.path()).is_err());
        let startup = state.recovery_session().unwrap();
        let failed = rejected_before_close(ctx, &startup);
        assert!(
            matches!(failed.error(), ContextCloseError::SessionBinding(e) if e=="database_context_session_origin")
        );
        all_live(&failed);
        assert!(state.recovery_session().is_err());
        original.ensure_publication_phase().unwrap();
        assert!(!directory.path().join("mno.sqlite").exists());
        drop(failed);
    });
}
#[test]
fn another_maintenance_instance_is_rejected_before_any_close() {
    scenario(|ctx, original, _| {
        let directory = tempfile::TempDir::new().unwrap();
        let state = DatabaseState::default();
        state
            .initialize_legacy_startup(&directory.path().join("other"))
            .unwrap();
        let maintenance = state.maintenance().unwrap();
        let wrong = maintenance.recovery_session().unwrap();
        let failed = rejected_before_close(ctx, &wrong);
        assert!(
            matches!(failed.error(), ContextCloseError::SessionBinding(e) if e=="database_context_session_maintenance")
        );
        all_live(&failed);
        assert!(maintenance.recovery_session().is_err());
        original.ensure_publication_phase().unwrap();
    });
}
#[test]
fn wrong_restore_operation_is_rejected_before_any_close() {
    scenario(|mut ctx, session, _| {
        ctx.protection
            .as_mut()
            .unwrap()
            .test_set_operation(uuid::Uuid::new_v4());
        let failed = rejected_before_close(ctx, session);
        assert!(matches!(failed.error(), ContextCloseError::Binding));
        all_live(&failed);
    });
}
#[test]
fn wrong_generation_is_rejected_before_any_close() {
    scenario(|mut ctx, session, _| {
        ctx.generation += 1;
        let failed = rejected_before_close(ctx, session);
        assert!(matches!(failed.error(), ContextCloseError::Binding));
        all_live(&failed);
    });
}
#[test]
fn wrong_physical_database_identity_is_rejected_before_any_close() {
    scenario(|mut ctx, session, _| {
        let file = tempfile::tempfile().unwrap();
        ctx.active.files.as_mut().unwrap().identity = private::file_identity(&file).unwrap();
        let failed = rejected_before_close(ctx, session);
        assert!(matches!(failed.error(), ContextCloseError::Binding));
        all_live(&failed);
    });
}
#[test]
fn replaced_directory_is_rejected_and_nothing_is_closed() {
    scenario(|ctx, session, root| {
        let moved = root.with_extension("moved");
        std::fs::rename(root, &moved).unwrap();
        std::fs::create_dir(root).unwrap();
        // Keep a plain, readable lock name so rejection reaches the identity
        // comparison rather than merely failing on a missing lockfile.
        std::fs::copy(moved.join(".mno-data.lock"), root.join(".mno-data.lock")).unwrap();
        let failed = rejected_before_close(ctx, session);
        assert!(matches!(
            failed.error(),
            ContextCloseError::SessionBinding(error) if error == "database_directory_changed"
        ));
        // Restore only test-owned names before inspecting source snapshots.
        std::fs::remove_file(root.join(".mno-data.lock")).unwrap();
        std::fs::remove_dir(root).unwrap();
        std::fs::rename(&moved, root).unwrap();
        all_live(&failed);
        session.ensure_publication_phase().unwrap();
    });
}
#[test]
fn replaced_database_is_rejected_before_any_close() {
    scenario(|ctx, session, root| {
        let path = root.join("mno.sqlite");
        let moved = root.join("original.sqlite");
        std::fs::rename(&path, &moved).unwrap();
        std::fs::copy(&moved, &path).unwrap();
        let failed = rejected_before_close(ctx, session);
        assert!(matches!(failed.error(), ContextCloseError::Binding));
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&moved, &path).unwrap();
        all_live(&failed);
    });
}

#[test]
fn bound_context_proof_allows_moving_same_session_forward_but_rejects_successor() {
    use crate::backup::{self, BackupKind};
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("book");
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root).unwrap();
    let archive = dir.path().join("incoming.mno-backup");
    backup::create_backup(
        &Connection::open(&path).unwrap(),
        &archive,
        BackupKind::Manual,
    )
    .unwrap();
    let flag = || Arc::new(AtomicBool::new(false));
    let incoming = backup::restore::prepare_candidate(&archive, flag()).unwrap();
    with_active(&state, &path, incoming, flag(), |mut ctx| {
        let maintenance = ctx.maintenance;
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let session = maintenance.recovery_session().unwrap();
        let first_id = session.session_id();
        let closed = ctx.close_owned_connections(&session).unwrap();
        closed
            .parts()
            ._binding
            .session
            .check_session(&session)
            .unwrap();
        let resources = pending::classification::classify_session(session, flag())
            .into_completion_proof()
            .unwrap()
            .publish_receipt()
            .unwrap()
            .remove_pending()
            .unwrap()
            .close_resources()
            .unwrap();
        // The SAME session moved into ResourcesClosed without borrowing itself.
        assert!(maintenance.recovery_session().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
        drop(resources);
        let successor = maintenance.recovery_session().unwrap();
        assert_ne!(first_id, successor.session_id());
        assert!(
            closed
                .parts()
                ._binding
                .session
                .check_session(&successor)
                .is_err()
        );
        assert_eq!(maintenance.generation(), 1);
        assert!(maintenance.ensure_usable().is_err());
        drop(closed);
        assert!(maintenance.recovery_session().is_err());
        Ok(())
    })
    .unwrap();
    assert!(state.operation(Some(1)).is_err());
}
