use super::*;
use crate::{
    backup::{self, BackupKind},
    database::initialize_schema,
    database_state::{GENERATION_CONFLICT, RECOVERY_REQUIRED},
};
use tempfile::TempDir;
const DOC: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Blåmes 🌿"}]}]}"#;
fn fixture(dir: &Path) -> PathBuf {
    let path = dir.join("mno.sqlite");
    let connection = Connection::open(&path).unwrap();
    initialize_schema(&connection).unwrap();
    connection.execute("INSERT INTO journal_entries VALUES ('entry','2026-10-17','Saved',?1,1,'created','updated')",[DOC]).unwrap();
    path
}
fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn candidate(source: &Path) -> PreparedRestore {
    let archive = source
        .parent()
        .unwrap()
        .join(format!("incoming-{}.mno-backup", uuid::Uuid::new_v4()));
    let source = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    backup::restore::prepare_candidate(&archive, cancel()).unwrap()
}
fn empty_context<'a, 's>(
    guard: &'a Maintenance<'s>,
    active: ActiveDatabase,
    incoming: PreparedRestore,
) -> RestoreContext<'a, 's> {
    RestoreContext {
        terminal_policy: None,
        maintenance: guard,
        active,
        incoming,
        protection: None,
        generation: guard.generation(),
        next_generation: guard.checked_next_generation().unwrap(),
        claimed: false,
    }
}
#[test]
fn correct_context_requires_its_bound_proof_and_never_changes_active_data_or_generation() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let incoming = candidate(&path);
    let original = fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    let saved = with_active(&state, &path, incoming, cancel(), |mut context| {
        let authorization = context.authorize()?;
        assert_eq!(authorization.generations(), (1, 2));
        assert_eq!(
            authorization
                .incoming()?
                .query_row("SELECT content_json FROM journal_entries", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            DOC
        );
        let safety = authorization.protection_path().to_owned();
        backup::restore::prepare_candidate(&safety, cancel()).unwrap();
        drop(authorization);
        assert!(matches!(
            context.authorize(),
            Err(ContextError::ProofAlreadyUsed)
        ));
        Ok(safety)
    })
    .unwrap();
    assert!(saved.is_file());
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(state.operation(None).unwrap().snapshot(()).generation, 1);
}
#[test]
fn proof_for_another_file_is_rejected_even_with_the_same_maintenance_guard() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let path_a = fixture(a.path());
    let path_b = fixture(b.path());
    let originals = (fs::read(&path_a).unwrap(), fs::read(&path_b).unwrap());
    let state = DatabaseState::ready_for_test();
    let guard = state.maintenance().unwrap();
    let active_a = ActiveDatabase::open(&path_a).unwrap();
    let active_b = ActiveDatabase::open(&path_b).unwrap();
    let proof = pre_restore::create_bound(&guard, &active_a, cancel()).unwrap();
    let mut context = empty_context(&guard, active_b, candidate(&path_b));
    assert!(matches!(
        context.attach(proof),
        Err(ContextError::InvalidProof)
    ));
    assert!(matches!(
        context.authorize(),
        Err(ContextError::InvalidProof)
    ));
    assert_eq!(fs::read(path_a).unwrap(), originals.0);
    assert_eq!(fs::read(path_b).unwrap(), originals.1);
}
#[test]
fn proof_from_another_operation_and_state_cannot_authorize_the_same_file() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let a = DatabaseState::ready_for_test();
    let b = DatabaseState::ready_for_test();
    let guard_a = a.maintenance().unwrap();
    let guard_b = b.maintenance().unwrap();
    let active = ActiveDatabase::open(&path).unwrap();
    let proof = pre_restore::create_bound(&guard_a, &active, cancel()).unwrap();
    let mut context = empty_context(&guard_b, active, candidate(&path));
    assert_ne!(guard_a.operation_id(), guard_b.operation_id());
    assert!(matches!(
        context.attach(proof),
        Err(ContextError::InvalidProof)
    ));
    let proof = pre_restore::create_bound(&guard_b, &context.active, cancel()).unwrap();
    context.attach(proof).unwrap();
    drop(context.authorize().unwrap());
    assert!(matches!(
        context.authorize(),
        Err(ContextError::ProofAlreadyUsed)
    ));
}
#[test]
fn preexisting_backup_never_substitutes_for_a_new_bound_proof() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    let guard = state.maintenance().unwrap();
    let active = ActiveDatabase::open(&path).unwrap();
    let proof = pre_restore::create_bound(&guard, &active, cancel()).unwrap();
    let existing = proof.path().to_owned();
    drop(proof);
    let mut context = empty_context(&guard, active, candidate(&path));
    assert!(existing.is_file());
    assert!(matches!(
        context.authorize(),
        Err(ContextError::InvalidProof)
    ));
}
#[test]
fn generation_exhaustion_is_rejected_before_any_protection_is_created() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let incoming = candidate(&path);
    let original = fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    state.set_generation_for_test(u32::MAX);
    let result: Result<()> = with_active(&state, &path, incoming, cancel(), |_| {
        panic!("must not enter context")
    });
    assert!(matches!(result, Err(ContextError::GenerationExhausted)));
    assert!(!root.path().join("backups").exists());
    assert_eq!(
        state.operation(None).unwrap().snapshot(()).generation,
        u32::MAX
    );
    assert_eq!(fs::read(path).unwrap(), original);
}
#[test]
fn queued_operations_wait_until_the_entire_context_and_authorization_have_ended() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let state = Arc::new(DatabaseState::ready_for_test());
    let mut worker = None;
    with_active(&state, &path, candidate(&path), cancel(), |mut context| {
        let (tx, rx) = std::sync::mpsc::channel();
        state.on_blocked(move || tx.send(()).unwrap());
        let other = state.clone();
        worker = Some(std::thread::spawn(move || {
            other.operation(Some(1)).unwrap().snapshot(()).generation
        }));
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let authorization = context.authorize()?;
        assert!(!worker.as_ref().unwrap().is_finished());
        drop(authorization);
        assert!(!worker.as_ref().unwrap().is_finished());
        Ok(())
    })
    .unwrap();
    assert_eq!(worker.unwrap().join().unwrap(), 1);
}
#[test]
fn file_replacement_is_detected_before_authorization() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let original = fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    with_active(&state, &path, candidate(&path), cancel(), |mut context| {
        let old = path.with_extension("old");
        fs::rename(&path, &old).unwrap();
        fs::copy(&old, &path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(matches!(
            context.authorize(),
            Err(ContextError::ActiveDatabaseChanged)
        ));
        Ok(())
    })
    .unwrap();
}
#[test]
fn canonical_alias_resolves_to_the_same_pinned_file() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let alias = root.path().join(".").join("mno.sqlite");
    let a = ActiveDatabase::open(&path).unwrap();
    let b = ActiveDatabase::open(&alias).unwrap();
    assert_eq!(a.identity(), b.identity());
    assert_eq!(a.path(), b.path());
}
#[test]
fn old_save_generation_stays_rejected_and_context_does_not_advance_it() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    {
        let mut guard = state.maintenance().unwrap();
        guard.advance_generation().unwrap();
    }
    with_active(&state, &path, candidate(&path), cancel(), |mut context| {
        assert_eq!(context.authorize()?.generations(), (2, 3));
        Ok(())
    })
    .unwrap();
    assert_eq!(state.operation(Some(1)).err().unwrap(), GENERATION_CONFLICT);
    assert_eq!(state.operation(Some(2)).unwrap().snapshot(()).generation, 2);
}
#[test]
fn recovery_latch_survives_maintenance_drop_without_any_persistent_marker() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let state = DatabaseState::ready_for_test();
    {
        let guard = state.maintenance().unwrap();
        guard.require_recovery();
    }
    assert_eq!(state.operation(None).err().unwrap(), RECOVERY_REQUIRED);
    assert_eq!(state.operation(Some(1)).err().unwrap(), RECOVERY_REQUIRED);
    let result: Result<()> = with_active(&state, &path, candidate(&path), cancel(), |_| {
        panic!("recovery required")
    });
    assert!(matches!(result,Err(ContextError::Coordination(code)) if code==RECOVERY_REQUIRED));
    assert!(!root.path().join("backups").exists());
}
#[test]
fn failure_releases_maintenance_without_changing_journal_or_generation() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let original = fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    let result: Result<()> = with_active(&state, &path, candidate(&path), cancel(), |_| {
        Err::<(), _>(ContextError::InvalidProof)
    });
    assert!(matches!(result, Err(ContextError::InvalidProof)));
    assert!(state.operation(Some(1)).is_ok());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn wal_is_rejected_before_sqlite_can_create_side_files_and_mode_is_not_changed() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let incoming = candidate(&path);
    {
        let db = Connection::open(&path).unwrap();
        let mode: String = db
            .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
    let original = fs::read(&path).unwrap();
    let count = fs::read_dir(root.path()).unwrap().count();
    let state = DatabaseState::ready_for_test();
    let result: Result<()> = with_active(&state, &path, incoming, cancel(), |_| {
        panic!("WAL not supported")
    });
    assert!(matches!(result, Err(ContextError::UnsupportedJournalMode)));
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), count);
    assert!(state.operation(Some(1)).is_ok());
}
#[test]
fn context_connections_cannot_write_active_or_incoming_journal() {
    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let original = fs::read(&path).unwrap();
    with_active(
        &DatabaseState::ready_for_test(),
        &path,
        candidate(&path),
        cancel(),
        |mut context| {
            assert!(
                context
                    .active
                    .connection()
                    .execute("DELETE FROM journal_entries", [])
                    .is_err()
            );
            let authorization = context.authorize()?;
            assert!(
                authorization
                    .incoming()?
                    .execute("DELETE FROM journal_entries", [])
                    .is_err()
            );
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn maintenance_scope_unwind_preserves_existing_guard_poisoning_and_release() {
    // Baseline: the existing guard's ordinary unwind poisons the generation lock.
    let baseline = DatabaseState::ready_for_test();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _maintenance = baseline.maintenance().unwrap();
        panic!("baseline guard unwind");
    }));
    assert_eq!(
        panic.unwrap_err().downcast_ref::<&str>(),
        Some(&"baseline guard unwind")
    );
    let baseline_operation = baseline.operation(Some(1)).err().unwrap();
    let baseline_maintenance = baseline.maintenance().err().unwrap();
    assert_eq!(baseline_maintenance, "database_coordination_failed");

    let root = TempDir::new().unwrap();
    let path = fixture(root.path());
    let incoming = candidate(&path);
    let original = fs::read(&path).unwrap();
    let state = DatabaseState::ready_for_test();
    let reached = std::cell::Cell::new(false);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<()> = with_active(&state, &path, incoming, cancel(), |context| {
            assert_eq!(context.generation, 1);
            reached.set(true);
            panic!("context scope unwind");
        });
    }));
    assert!(reached.get());
    assert_eq!(
        panic.unwrap_err().downcast_ref::<&str>(),
        Some(&"context scope unwind")
    );
    assert_eq!(state.operation(Some(1)).err().unwrap(), baseline_operation);
    // Coordination failure, rather than maintenance-in-progress, also checks
    // that ordinary Maintenance::Drop released its admission flag on unwind.
    assert_eq!(state.maintenance().err().unwrap(), baseline_maintenance);
    assert_eq!(fs::read(path).unwrap(), original);
}
