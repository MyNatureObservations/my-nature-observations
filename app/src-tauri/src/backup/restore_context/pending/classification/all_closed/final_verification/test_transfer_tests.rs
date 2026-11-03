use super::super::super::tests::{resources, running_changed};
use super::*;
use crate::{
    backup::{pre_restore, restore, restore_context},
    database_state::DatabaseState,
};
use classification::tests::{Fixture, flag, tree};
use std::{cell::Cell, fs};

fn running_state(f: &Fixture) -> DatabaseState {
    fs::remove_file(f.root.join("recovery/pending-restore.json")).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    state
}
fn running_final<'m, 's>(f: &Fixture, m: &'m Maintenance<'s>) -> FinalFilesVerified<'m, 's> {
    let incoming =
        restore::prepare_candidate(&f._dir.path().join("incoming.mno-backup"), flag()).unwrap();
    let active = restore_context::ActiveDatabase::open(&f.path).unwrap();
    let protection = pre_restore::create_bound(m, &active, flag()).unwrap();
    let mut ctx = restore_context::RestoreContext {
        terminal_policy: None,
        maintenance: m,
        active,
        incoming,
        protection: None,
        generation: m.generation(),
        next_generation: m.checked_next_generation().unwrap(),
        claimed: false,
    };
    ctx.attach(protection).unwrap();
    {
        let auth = ctx.authorize().unwrap();
        drop(auth.publish_pending().unwrap());
    }
    let session = m.recovery_session().unwrap();
    let context = ctx.close_owned_connections(&session).unwrap();
    resources(session)
        .combine_context(context)
        .unwrap()
        .verify_final()
        .unwrap()
}
fn snapshot(proof: &FinalFilesVerified<'_, '_>) -> (uuid::Uuid, *const ()) {
    let view = proof.retained().all.held().resources().closed().view();
    let Access::Session(s) = view.access else {
        panic!("session")
    };
    (s.session_id(), s.test_reservation_slot())
}
fn blocked(state: &DatabaseState, f: &Fixture) {
    assert!(state.operation(Some(1)).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
// A real mutable borrow, with no generation mutation or admission.
fn exclusive_probe(_: &mut Maintenance<'_>) {}
const POINTS: [Point; 5] = [
    Point::Before,
    Point::Unwrapped,
    Point::ContextClosed,
    Point::FilesClosed,
    Point::Detached,
];

#[test]
fn running_moves_the_same_slot_and_ends_all_real_maintenance_borrows() {
    let f = Fixture::new();
    let state = running_state(&f);
    let mut m = state.maintenance().unwrap();
    let final_owner = running_final(&f, &m);
    let (id, slot) = snapshot(&final_owner);
    let before = tree(&f.root);
    let mut seen = Vec::new();
    let transferred = final_owner
        .transfer_running(&m, |point| {
            seen.push(point);
            // A genuinely separate thread competes at each stable owner boundary.
            // Join completes before the next move; no sleeps or timing race.
            std::thread::scope(|scope| {
                let competitor = scope.spawn(|| m.recovery_session().err());
                assert_eq!(
                    competitor.join().unwrap().as_deref(),
                    Some("database_recovery_busy")
                );
            });
            assert!(m.ensure_usable().is_err());
            assert_eq!(m.generation(), 1);
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            Ok(())
        })
        .unwrap_or_else(|_| panic!("transfer"));
    exclusive_probe(&mut m);
    assert_eq!(seen, POINTS);
    assert_eq!(transferred.0.session.session_id(), id);
    assert_eq!(transferred.0.session.slot(), slot);
    assert!(transferred.0.session.matches_running(&m));
    assert_eq!(transferred.0.session.generation(), 1);
    assert_eq!(tree(&f.root), before);
    assert!(m.recovery_session().is_err());
    drop(transferred);
    let next = m.recovery_session().unwrap();
    assert_ne!(next.session_id(), id);
    drop(next);
    assert!(m.ensure_usable().is_err());
    assert_eq!(m.generation(), 1);
    drop(m);
    blocked(&state, &f);
}

#[test]
fn startup_has_a_separate_transfer_and_can_never_select_running() {
    let f = Fixture::new();
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let proof = resources(state.recovery_session().unwrap())
        .confirm_startup()
        .unwrap()
        .verify_final()
        .unwrap();
    let (id, slot) = snapshot(&proof);
    let before = tree(&f.root);
    let moved = proof
        .transfer_startup(|_| {
            std::thread::scope(|scope| {
                assert_eq!(
                    scope
                        .spawn(|| state.recovery_session().err())
                        .join()
                        .unwrap()
                        .as_deref(),
                    Some("database_recovery_busy")
                );
            });
            blocked(&state, &f);
            Ok(())
        })
        .unwrap_or_else(|_| panic!("startup transfer"));
    assert_eq!(moved.0.session.session_id(), id);
    assert_eq!(moved.0.session.slot(), slot);
    assert!(!moved.0.session.running());
    assert_eq!(moved.0.session.generation(), 1);
    assert_eq!(tree(&f.root), before);
    drop(moved);
    blocked(&state, &f);
    drop(state.recovery_session().unwrap());
    // Wrong origin must be rejected before any move, even with another real guard.
    let proof = resources(state.recovery_session().unwrap())
        .confirm_startup()
        .unwrap()
        .verify_final()
        .unwrap();
    let other = Fixture::new();
    let other_state = running_state(&other);
    let m = other_state.maintenance().unwrap();
    let called = Cell::new(false);
    let failure = proof
        .transfer_running(&m, |_| {
            called.set(true);
            Ok(())
        })
        .err()
        .unwrap();
    assert!(!called.get());
    assert!(matches!(failure.error, Error::Binding));
    assert!(state.recovery_session().is_err());
    drop(failure);
    blocked(&state, &f);
}

#[test]
fn wrong_guard_operation_and_running_as_startup_keep_the_original_owner() {
    let f = Fixture::new();
    let state = running_state(&f);
    let m = state.maintenance().unwrap();
    let proof = running_final(&f, &m);
    let before = tree(&f.root);
    let other = Fixture::new();
    let other_state = running_state(&other);
    let guard = other_state.maintenance().unwrap();
    let failure = proof
        .transfer_running(&guard, |_| panic!("must reject before checkpoints"))
        .err()
        .unwrap();
    assert!(matches!(failure.error, Error::Binding));
    assert!(m.recovery_session().is_err());
    assert_eq!(tree(&f.root), before);
    let Stage::Before(proof) = *failure._stage else {
        panic!("original retained")
    };
    let failure = proof
        .transfer_startup(|_| panic!("wrong origin"))
        .err()
        .unwrap();
    assert!(matches!(failure.error, Error::Binding));
    assert!(m.recovery_session().is_err());
    drop(failure);
    assert_eq!(m.generation(), 1);
    assert!(m.ensure_usable().is_err());
}

#[test]
fn every_stable_point_retains_all_owners_on_error_and_panic() {
    for panic in [false, true] {
        for target in POINTS {
            running_changed(|f, m, context, closed| {
                let proof = closed
                    .combine_context(context)
                    .unwrap()
                    .verify_final()
                    .unwrap();
                let before = tree(&f.root);
                let reached = Cell::new(false);
                let failure = proof
                    .transfer_running(m, |point| {
                        assert!(m.recovery_session().is_err());
                        if point == target {
                            reached.set(true);
                            if panic {
                                panic!("stable checkpoint")
                            };
                            return Err(Error::Check);
                        }
                        Ok(())
                    })
                    .err()
                    .unwrap();
                assert!(reached.get());
                assert!(matches!(failure.error, Error::Panic) == panic);
                assert!(m.recovery_session().is_err());
                assert_eq!(tree(&f.root), before);
                assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
                drop(failure);
                drop(m.recovery_session().unwrap());
                assert!(m.ensure_usable().is_err());
                assert_eq!(m.generation(), 1);
            });
        }
    }
}

#[test]
fn changed_operations_generations_and_successor_context_ids_reject_before_split() {
    for field in 0..3 {
        let f = Fixture::new();
        let state = running_state(&f);
        let m = state.maintenance().unwrap();
        let mut proof = running_final(&f, &m);
        let all = &mut proof.retained.as_mut().unwrap().all;
        if field == 0 {
            all.generation += 1;
        } else {
            let Held::Running { context, resources } = all.held.as_mut().unwrap() else {
                panic!()
            };
            if field == 1 {
                resources.closed_mut().test_parts_mut().0.restore_id = uuid::Uuid::new_v4();
            } else {
                context.test_corrupt_binding(2);
            }
        }
        let called = Cell::new(false);
        let failed = proof
            .transfer_running(&m, |_| {
                called.set(true);
                Ok(())
            })
            .err()
            .unwrap();
        assert!(!called.get());
        assert!(matches!(*failed._stage, Stage::Before(_)));
        assert!(m.recovery_session().is_err());
        assert_eq!(m.generation(), 1);
        assert!(m.ensure_usable().is_err());
        drop(failed);
        drop(m.recovery_session().unwrap());
    }
}

#[test]
fn transferred_files_drop_before_the_same_reservation_and_never_unlock_recovery() {
    let f = Fixture::new();
    let state = running_state(&f);
    let m = state.maintenance().unwrap();
    let proof = running_final(&f, &m);
    let staging = proof
        .retained()
        .all
        .held()
        .resources()
        .closed()
        .retained_staging_path()
        .to_path_buf();
    let moved = proof
        .transfer_running(&m, |_| Ok(()))
        .unwrap_or_else(|_| panic!());
    assert!(staging.exists());
    let seen = std::rc::Rc::new(Cell::new(false));
    let observed = seen.clone();
    let root = f.root.clone();
    crate::database_state::recovery_session::reservation_drop_probe::with(
        move || {
            observed.set(
                !staging.exists() && crate::startup::DirectoryLease::open_existing(&root).is_err(),
            );
        },
        || drop(moved),
    );
    assert!(seen.get());
    assert!(m.ensure_usable().is_err());
    assert_eq!(m.generation(), 1);
    drop(m.recovery_session().unwrap());
}

#[test]
fn startup_errors_and_panics_retain_the_same_chain_at_every_stable_point() {
    for panic in [false, true] {
        for target in POINTS {
            let f = Fixture::new();
            let state = DatabaseState::default();
            assert!(state.initialize_legacy_startup(&f.root).is_err());
            let proof = resources(state.recovery_session().unwrap())
                .confirm_startup()
                .unwrap()
                .verify_final()
                .unwrap();
            let staging = proof
                .retained()
                .all
                .held()
                .resources()
                .closed()
                .retained_staging_path()
                .to_path_buf();
            let before = tree(&f.root);
            let hit = Cell::new(false);
            let failure = proof
                .transfer_startup(|point| {
                    assert!(state.recovery_session().is_err());
                    blocked(&state, &f);
                    if point == target {
                        hit.set(true);
                        if panic {
                            panic!("startup stable point")
                        };
                        return Err(Error::Check);
                    }
                    Ok(())
                })
                .err()
                .unwrap();
            assert!(hit.get());
            assert!(matches!(failure.error, Error::Panic) == panic);
            assert!(staging.is_dir());
            assert!(state.recovery_session().is_err());
            assert_eq!(tree(&f.root), before);
            drop(failure);
            assert!(!staging.exists());
            let next = state.recovery_session().unwrap();
            assert_eq!(next.startup_generation().unwrap(), 1);
            drop(next);
            blocked(&state, &f);
        }
    }
}

#[test]
fn panicking_payload_and_callback_destructors_are_retained_and_safely_dropped() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Payload(Arc<AtomicUsize>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::panic::panic_any(Payload(self.0.clone()));
        }
    }
    struct CallbackDrop(Arc<AtomicUsize>, Arc<AtomicUsize>);
    impl Drop for CallbackDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::panic::panic_any(Payload(self.1.clone()));
        }
    }
    for (panic_in_body, drop_during_unwind) in [(false, false), (true, false), (true, true)] {
        let f = Fixture::new();
        let state = running_state(&f);
        let m = state.maintenance().unwrap();
        let proof = running_final(&f, &m);
        let before = tree(&f.root);
        let body_drops = Arc::new(AtomicUsize::new(0));
        let callback_drops = Arc::new(AtomicUsize::new(0));
        let callback_payload_drops = Arc::new(AtomicUsize::new(0));
        let callback = CallbackDrop(callback_drops.clone(), callback_payload_drops.clone());
        let payload_counter = body_drops.clone();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            proof
                .transfer_running(&m, move |point| {
                    std::hint::black_box(&callback);
                    if panic_in_body && point == Point::FilesClosed {
                        std::panic::panic_any(Payload(payload_counter.clone()));
                    }
                    Ok(())
                })
                .err()
                .expect("callback destruction must reject transfer")
        }))
        .unwrap_or_else(|_| panic!("panic escaped failure construction"));
        assert!(matches!(failure.error, Error::Panic));
        assert_eq!(callback_drops.load(Ordering::SeqCst), 1);
        assert_eq!(body_drops.load(Ordering::SeqCst), 0);
        assert_eq!(callback_payload_drops.load(Ordering::SeqCst), 0);
        assert_eq!(
            m.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        assert!(m.ensure_usable().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(m.generation(), 1);
        assert_eq!(tree(&f.root), before);
        let destruction = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _failure = failure;
            if drop_during_unwind {
                panic!("outer unwind while failure retains both payloads");
            }
        }));
        assert_eq!(destruction.is_err(), drop_during_unwind);
        // Secondary payloads have the same panicking Drop, but are quarantined.
        assert_eq!(
            body_drops.load(Ordering::SeqCst),
            usize::from(panic_in_body)
        );
        assert_eq!(callback_payload_drops.load(Ordering::SeqCst), 1);
        assert_eq!(callback_drops.load(Ordering::SeqCst), 1);
        assert!(m.recovery_session().is_ok());
        assert!(m.ensure_usable().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(m.generation(), 1);
    }
}
