use super::*;
use crate::backup::restore_context::pending::classification::result::receipt::hooks;
use crate::backup::restore_context::pending::classification::{
    Status,
    tests::{Fixture, flag, tree},
};
use crate::database_state::DatabaseState;
use std::{cell::Cell, fs, rc::Rc};
thread_local! {
    static MODE: Cell<u8> = const { Cell::new(0) };
    static VERIFIED: Cell<bool> = const { Cell::new(false) };
    static DENIED: Cell<u32> = const { Cell::new(0) };
    static STATEMENT: Cell<*mut rusqlite::ffi::sqlite3_stmt> = const { Cell::new(std::ptr::null_mut()) };
}
pub(super) fn snapshot_verified() {
    VERIFIED.set(true);
}
pub(super) fn configure_validation(database: &mut BoundSqlite) {
    match MODE.get() {
        1 => database
            .connection_mut()
            .authorizer(Some(|_: rusqlite::hooks::AuthContext<'_>| {
                rusqlite::hooks::Authorization::Deny
            }))
            .unwrap(),
        2 => database
            .connection_mut()
            .authorizer(Some(|c: rusqlite::hooks::AuthContext<'_>| {
                if matches!(
                    c.action,
                    rusqlite::hooks::AuthAction::Transaction {
                        operation: rusqlite::hooks::TransactionOperation::Rollback
                    }
                ) {
                    assert!(VERIFIED.get());
                    DENIED.set(DENIED.get() + 1);
                    rusqlite::hooks::Authorization::Deny
                } else {
                    rusqlite::hooks::Authorization::Allow
                }
            }))
            .unwrap(),
        _ => {}
    }
}
pub(super) fn configure_close(database: &mut BoundSqlite) {
    match MODE.get() {
        3 => STATEMENT.set(database.test_statement()),
        4 => database.test_close_error(rusqlite::ffi::SQLITE_IOERR),
        5 => database.test_unregister_failure(),
        _ => {}
    }
}
fn handoff<'s>(f: &Fixture, state: &'s DatabaseState, new: bool) -> StartupHandoff<'s> {
    if new {
        f.title("New");
    }
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let a = classification::classify_session(state.recovery_session().unwrap(), flag());
    assert_eq!(a.status(), if new { Status::New } else { Status::Old });
    a.into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap()
        .remove_pending()
        .unwrap()
        .close_resources()
        .unwrap()
        .confirm_startup()
        .unwrap()
        .verify_final()
        .unwrap()
        .prepare_q_removal()
        .unwrap()
        .unlink_q()
        .unwrap()
        .complete_disk()
        .unwrap()
        .into_startup_handoff()
        .unwrap()
}
fn blocked(state: &DatabaseState, f: &Fixture) {
    state.assert_startup_recovery_generation_for_test(1);
    assert!(state.operation(None).is_err());
    assert!(state.maintenance().is_err());
    assert_eq!(
        state.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
#[test]
fn startup_admission_old_new_preserve_owner_generation_and_allow_only_verified_ready() {
    for new in [false, true] {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let h = handoff(&f, &state, new);
        let before = tree(&f.root);
        let journal_before = fs::read(&f.path).unwrap();
        let protection_before = fs::read(&f.protection).unwrap();
        let marker = f.root.join(".mno-initialized-v1.json");
        let marker_before = fs::read(&marker).unwrap();
        let owners = state.test_startup_owner();
        let id = h.0.session.session_id();
        let slot = h.0.session.slot();
        blocked(&state, &f);
        let seen = Rc::new(Cell::new(false));
        let hit = seen.clone();
        hooks::with(
            move |p| {
                if p == "startup-admission-before-ready" {
                    hit.set(true);
                }
                Ok(())
            },
            |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
            || state.admit_startup_verified(h),
        )
        .unwrap();
        assert!(seen.get());
        assert_eq!(owners, state.test_startup_owner());
        assert_eq!(state.operation(Some(1)).unwrap().snapshot(()).generation, 1);
        assert!(state.recovery_session().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        // Read the expected paths directly: absence fails before byte comparison.
        // Staging may be cleaned without weakening these persistent-file checks.
        assert_eq!(
            fs::read(&f.path).expect("journal must remain after admission"),
            journal_before
        );
        assert_eq!(
            fs::read(&f.protection).expect("protected backup must remain after admission"),
            protection_before
        );
        assert_eq!(
            fs::read(&marker).expect("establishment marker must remain after admission"),
            marker_before
        );
        let after = tree(&f.root);
        for (p, b) in after {
            assert_eq!(before.get(&p), Some(&b));
        }
        let m = state.maintenance().unwrap();
        let next = m.recovery_session().unwrap();
        assert_ne!(next.session_id(), id);
        assert_eq!(next.test_reservation_slot(), slot);
    }
}
#[test]
fn startup_admission_checkpoint_errors_and_cancellation_keep_recovery() {
    for point in [
        "startup-admission-before-validation",
        "startup-admission-after-open",
        "startup-admission-before-close",
        "startup-admission-after-close",
        "startup-admission-before-ready",
    ] {
        for cancel in [false, true] {
            let f = Fixture::new();
            let state = DatabaseState::default();
            let h = handoff(&f, &state, false);
            let id = h.0.session.session_id();
            let slot = h.0.session.slot();
            let (_, files, flag) = h.0._files.view();
            let flag = flag.clone();
            let weak = std::sync::Arc::downgrade(&files.active_file);
            let before = tree(&f.root);
            let seen = Rc::new(Cell::new(false));
            let hit = seen.clone();
            let failure = hooks::with(
                move |p| {
                    if p == point {
                        hit.set(true);
                        if cancel {
                            flag.store(true, std::sync::atomic::Ordering::Release);
                        } else {
                            return Err(io::Error::other("checkpoint"));
                        }
                    }
                    Ok(())
                },
                |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
                || state.admit_startup_verified(h),
            )
            .unwrap_err();
            assert!(seen.get());
            assert!(if cancel {
                matches!(failure.error, AdmissionError::Files(Reason::Cancelled))
            } else {
                matches!(failure.error, AdmissionError::Checkpoint(_))
            });
            assert_eq!(failure.retained.handoff.0.session.session_id(), id);
            assert_eq!(failure.retained.handoff.0.session.slot(), slot);
            assert!(weak.upgrade().is_some());
            assert_eq!(tree(&f.root), before);
            blocked(&state, &f);
            drop(failure);
            state.assert_startup_recovery_generation_for_test(1);
            assert!(state.operation(None).is_err());
        }
    }
}
#[test]
fn startup_admission_sqlite_rollback_close_and_unregister_errors_retain_owners() {
    for mode in 1..=5 {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let h = handoff(&f, &state, false);
        let before = tree(&f.root);
        let id = h.0.session.session_id();
        MODE.set(mode);
        VERIFIED.set(false);
        DENIED.set(0);
        let mut failure = state.admit_startup_verified(h).unwrap_err();
        MODE.set(0);
        assert_eq!(failure.retained.handoff.0.session.session_id(), id);
        match mode {
            1 | 2 => {
                assert!(matches!(failure.error, AdmissionError::Sqlite));
                let Some(Database::Live(db)) = failure.retained.database.as_mut() else {
                    panic!()
                };
                assert!(db.test_is_open());
                let name = db.test_name();
                assert!(!unsafe { rusqlite::ffi::sqlite3_vfs_find(name.as_ptr()) }.is_null());
                if mode == 2 {
                    assert!(VERIFIED.get());
                    assert!(DENIED.get() > 0);
                    assert!(!db.connection_mut().is_autocommit());
                }
                db.connection_mut()
                    .authorizer(
                        None::<
                            fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization,
                        >,
                    )
                    .unwrap();
            }
            3 | 4 => {
                assert!(
                    matches!(failure.error,AdmissionError::Close{code:Some(c)} if c==if mode==3{rusqlite::ffi::SQLITE_BUSY}else{rusqlite::ffi::SQLITE_IOERR})
                );
                if mode == 3 {
                    let stmt = STATEMENT.replace(std::ptr::null_mut());
                    assert_eq!(
                        unsafe { rusqlite::ffi::sqlite3_step(stmt) },
                        rusqlite::ffi::SQLITE_ROW
                    );
                    assert_eq!(
                        unsafe { rusqlite::ffi::sqlite3_finalize(stmt) },
                        rusqlite::ffi::SQLITE_OK
                    );
                }
            }
            5 => assert!(matches!(failure.error, AdmissionError::Unregister)),
            _ => unreachable!(),
        }
        assert_eq!(tree(&f.root), before);
        blocked(&state, &f);
        drop(failure);
        assert!(state.operation(None).is_err());
    }
}
#[test]
fn startup_admission_wrong_state_origin_operation_generation_and_reservation_reject_before_open() {
    for case in 0..5 {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let mut h = handoff(&f, &state, false);
        if case < 3 {
            h.0.session.test_startup_corrupt(case);
        }
        if case == 3 {
            state.test_startup_reserved(false);
        }
        let other = DatabaseState::default();
        let target = if case == 4 { &other } else { &state };
        let failure = target.admit_startup_verified(h).unwrap_err();
        assert!(matches!(failure.error, AdmissionError::Binding));
        assert!(failure.retained.database.is_none());
        if case == 3 {
            state.test_startup_reserved(true);
        }
        blocked(&state, &f);
    }
}
#[test]
fn startup_admission_late_file_changes_artifacts_and_missing_marker_block_ready() {
    for case in 0..9 {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let h = handoff(&f, &state, false);
        let root = f.root.clone();
        let journal = f.path.clone();
        let archive = f.protection.clone();
        let seen = Rc::new(Cell::new(false));
        let hit = seen.clone();
        let failure = hooks::with(
            move |p| {
                if p == "startup-admission-after-close" {
                    hit.set(true);
                    match case {
                        0 => fs::write(&journal, b"changed")?,
                        1 => {
                            let bytes = fs::read(&journal)?;
                            fs::rename(&journal, root.join("old.sqlite"))?;
                            fs::write(&journal, bytes)?;
                        }
                        2 => fs::write(&archive, b"changed")?,
                        3 => fs::write(root.join(".mno-initialized-v1.json"), b"changed")?,
                        4 => fs::write(root.join("mno.sqlite-wal"), b"")?,
                        5 => fs::write(root.join(".MNO-COMPLETION-orphan"), b"")?,
                        6 => fs::create_dir(root.join("recovery"))?,
                        7 => fs::write(root.join(".mno-restore-completion-v1.json"), b"returned")?,
                        8 => fs::remove_file(root.join(".mno-initialized-v1.json"))?,
                        _ => unreachable!(),
                    }
                }
                Ok(())
            },
            |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
            || state.admit_startup_verified(h),
        )
        .unwrap_err();
        assert!(seen.get());
        assert!(!matches!(failure.error, AdmissionError::Checkpoint(_)));
        blocked(&state, &f);
    }
}
#[test]
fn startup_admission_panic_before_ready_retains_session_files_and_process_lock() {
    for point in [
        "startup-admission-before-validation",
        "startup-admission-after-open",
        "startup-admission-before-close",
        "startup-admission-after-close",
        "startup-admission-before-ready",
    ] {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let h = handoff(&f, &state, false);
        let (_, files, _) = h.0._files.view();
        let weak = std::sync::Arc::downgrade(&files.active_file);
        let before = tree(&f.root);
        let hit = Rc::new(Cell::new(false));
        let seen = hit.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |p| {
                    if p == point {
                        seen.set(true);
                        panic!("startup admission panic");
                    }
                    Ok(())
                },
                |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
                || state.admit_startup_verified(h),
            )
        }));
        assert!(result.is_err());
        assert!(hit.get());
        assert!(weak.upgrade().is_some());
        // Unwinding poisons the write guard conservatively; the phase/generation stay blocked.
        state.assert_startup_recovery_generation_for_test_poisoned(1);
        assert!(state.operation(None).is_err());
        assert!(state.recovery_session().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(tree(&f.root), before);
    }
}

#[test]
fn startup_admission_directory_replacement_rejects_before_sqlite_open() {
    let f = Fixture::new();
    let state = DatabaseState::default();
    let h = handoff(&f, &state, false);
    let moved = f.root.with_extension("retained");
    fs::rename(&f.root, &moved).unwrap();
    fs::create_dir(&f.root).unwrap();
    let failure = state.admit_startup_verified(h).unwrap_err();
    assert!(matches!(failure.error, AdmissionError::Binding));
    assert!(failure.retained.database.is_none());
    state.assert_startup_recovery_generation_for_test(1);
    assert!(state.operation(None).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&moved).is_err());
    fs::remove_dir(&f.root).unwrap();
    fs::rename(moved, &f.root).unwrap();
    blocked(&state, &f);
}
#[test]
fn startup_admission_queued_operation_waits_for_publication_and_cannot_start_recovery() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let f = Fixture::new();
    let state = Arc::new(DatabaseState::default());
    let h = handoff(&f, &state, false);
    let (tx, rx) = mpsc::channel();
    state.on_blocked(move || tx.send(()).unwrap());
    let worker = Rc::new(std::cell::RefCell::new(None));
    let saved = worker.clone();
    let other = state.clone();
    let seen = Rc::new(Cell::new(false));
    let hit = seen.clone();
    let check = state.clone();
    hooks::with(
        move |point| {
            if point == "startup-admission-before-ready" {
                hit.set(true);
                let other = other.clone();
                *saved.borrow_mut() = Some(std::thread::spawn(move || {
                    other
                        .operation(Some(1))
                        .map(|op| op.snapshot(()).generation)
                }));
                rx.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(check.maintenance().is_err());
                assert_eq!(
                    check.recovery_session().err().as_deref(),
                    Some("database_recovery_busy")
                );
            }
            Ok(())
        },
        |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
        || state.admit_startup_verified(h),
    )
    .unwrap();
    assert!(seen.get());
    assert_eq!(
        worker.borrow_mut().take().unwrap().join().unwrap().unwrap(),
        1
    );
    assert!(state.recovery_session().is_err());
}
#[test]
fn startup_admission_compiler_requires_consumed_startup_handoff() {
    use std::process::Command;
    let source = include_str!("../../../../../../../database_state.rs");
    let start = source
        .find("    pub(crate) fn admit_startup_verified<'s>(")
        .unwrap();
    let mut depth = 0;
    let mut opened = false;
    let mut end = 0;
    for (offset, ch) in source[start..].char_indices() {
        if ch == '{' {
            opened = true;
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if opened && depth == 0 {
                end = start + offset + 1;
                break;
            }
        }
    }
    let method = source[start..end].replace(
        "crate::backup::restore_context::pending::classification::",
        "crate::api::",
    );
    let common = format!(
        r#"
    extern crate serde;
    mod api {{
        use std::sync::{{RwLock,RwLockWriteGuard,atomic::{{AtomicBool,AtomicU8,Ordering}}}};
        enum Phase{{Recovery,Ready}}
        pub struct DatabaseState{{generation:RwLock<u32>,recovery_required:AtomicBool,phase:AtomicU8,startup_admitted:AtomicBool}}
        pub struct Maintenance<'s>{{state:&'s DatabaseState,generation:RwLockWriteGuard<'s,u32>}}
        struct Detached<'s>(&'s DatabaseState);
        pub struct RunningHandoff<'s>(Detached<'s>);
        pub struct StartupHandoff<'s>(Detached<'s>);
        pub struct DiskRecoveryCompleted<'s>(Detached<'s>);
        #[derive(Debug)] pub struct StartupAdmissionFailure<'s>(&'s DatabaseState);
        impl std::fmt::Debug for DatabaseState{{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{{f.write_str("state")}}}}
        impl<'s> StartupHandoff<'s>{{pub fn matches_startup_target(&self,_:&DatabaseState)->bool{{true}} pub fn reject_binding(self)->StartupAdmissionFailure<'s>{{unimplemented!()}} pub fn reject_coordination(self)->StartupAdmissionFailure<'s>{{unimplemented!()}} pub fn validate_startup(self,_:&DatabaseState,_:u32)->Result<Validated<'s>,StartupAdmissionFailure<'s>>{{unimplemented!()}}}}
        pub struct Validated<'s>(Detached<'s>);
        impl DatabaseState{{{method}}}
    }}
    "#
    );
    let dir = tempfile::TempDir::new().unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let serde = fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libserde-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .unwrap();
    let compile = |probe: &str| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, format!("{common}\n{probe}")).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-type=lib"])
            .arg(path)
            .arg("--extern")
            .arg(format!("serde={}", serde.display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--out-dir")
            .arg(dir.path())
            .output()
            .unwrap()
    };
    let positive = compile(
        "fn valid<'s>(m:&'s api::DatabaseState,h:api::StartupHandoff<'s>){m.admit_startup_verified(h).unwrap();}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, reason) in [
        (
            "fn wrong<'s>(m:&'s api::DatabaseState,h:api::RunningHandoff<'s>){m.admit_startup_verified(h);}",
            "E0308",
        ),
        (
            "fn wrong(m:&api::DatabaseState){m.admit_startup_verified(true);}",
            "E0308",
        ),
        (
            "fn wrong<'s>(m:&'s api::DatabaseState,h:api::DiskRecoveryCompleted<'s>){m.admit_startup_verified(h);}",
            "E0308",
        ),
        (
            "fn twice<'s>(m:&'s api::DatabaseState,h:api::StartupHandoff<'s>){m.admit_startup_verified(h);m.admit_startup_verified(h);}",
            "E0382",
        ),
        ("fn clone(h:api::StartupHandoff<'_>){h.clone();}", "E0599"),
        ("fn fake(){let _=api::StartupHandoff;}", "private"),
        (
            "fn escape<'s>(h:api::StartupHandoff<'s>)->api::StartupHandoff<'static>{h}",
            "lifetime may not live long enough",
        ),
        (
            "fn bound<T:serde::de::DeserializeOwned>(){} fn fake(){bound::<api::StartupHandoff<'static>>();}",
            "E0277",
        ),
    ] {
        let result = compile(probe);
        let err = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && err.contains(reason),
            "{probe}: {err}"
        );
    }
}

#[test]
fn startup_admission_preserves_maximum_process_local_generation_without_addition() {
    let f = Fixture::new();
    let state = DatabaseState::default();
    state.set_generation_for_test(u32::MAX);
    let h = handoff(&f, &state, false);
    assert_eq!(h.0.session.generation(), u32::MAX);
    state.admit_startup_verified(h).unwrap();
    assert_eq!(
        state
            .operation(Some(u32::MAX))
            .unwrap()
            .snapshot(())
            .generation,
        u32::MAX
    );
}

#[test]
fn startup_admission_foreign_state_with_held_maintenance_rejects_without_lock_wait() {
    use std::{sync::mpsc, time::Duration};
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let f = Fixture::new();
        let state = DatabaseState::default();
        let h = handoff(&f, &state, false);
        let dir = tempfile::TempDir::new().unwrap();
        let other = DatabaseState::default();
        other
            .initialize_legacy_startup(&dir.path().join("other"))
            .unwrap();
        let maintenance = other.maintenance().unwrap();
        let failure = other.admit_startup_verified(h).unwrap_err();
        assert!(matches!(failure.error, AdmissionError::Binding));
        assert!(failure.retained.database.is_none());
        blocked(&state, &f);
        assert_eq!(maintenance.generation(), 1);
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("foreign target must not wait for its held Maintenance lock");
    worker.join().unwrap();
}
