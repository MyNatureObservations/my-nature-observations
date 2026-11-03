use super::*;
use crate::backup::restore_context::pending::classification::result::receipt::hooks;
use crate::backup::restore_context::pending::classification::{
    Status,
    tests::{Fixture, flag, tree},
};
use crate::{
    backup::{pre_restore, restore, restore_context},
    database_state::DatabaseState,
};
use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
};
thread_local! {static CLOSE:Cell<u8>=const {Cell::new(0)};static STATEMENT:Cell<*mut rusqlite::ffi::sqlite3_stmt>=const{Cell::new(std::ptr::null_mut())};}
thread_local! {
    static SNAPSHOT_VERIFIED: Cell<bool> = const { Cell::new(false) };
    static ROLLBACK_DENIED: Cell<usize> = const { Cell::new(0) };
}
pub(super) fn snapshot_verified() {
    if CLOSE.get() == 5 {
        SNAPSHOT_VERIFIED.set(true);
    }
}
pub(super) fn configure_close(database: &mut BoundSqlite) {
    match CLOSE.get() {
        1 => STATEMENT.set(database.test_statement()),
        2 => database.test_close_error(rusqlite::ffi::SQLITE_IOERR),
        3 => database.test_unregister_failure(),
        _ => {}
    }
}
pub(super) fn configure_validation(database: &mut BoundSqlite) {
    if CLOSE.get() == 4 {
        database
            .connection_mut()
            .authorizer(Some(|_: rusqlite::hooks::AuthContext<'_>| {
                rusqlite::hooks::Authorization::Deny
            }))
            .unwrap();
    }
    if CLOSE.get() == 5 {
        database
            .connection_mut()
            .authorizer(Some(|context: rusqlite::hooks::AuthContext<'_>| {
                if matches!(
                    context.action,
                    rusqlite::hooks::AuthAction::Transaction {
                        operation: rusqlite::hooks::TransactionOperation::Rollback
                    }
                ) {
                    assert!(
                        SNAPSHOT_VERIFIED.get(),
                        "validation must succeed before denying ROLLBACK"
                    );
                    ROLLBACK_DENIED.set(ROLLBACK_DENIED.get() + 1);
                    rusqlite::hooks::Authorization::Deny
                } else {
                    rusqlite::hooks::Authorization::Allow
                }
            }))
            .unwrap();
    }
}
fn state(f: &Fixture) -> DatabaseState {
    fs::remove_file(f.root.join("recovery/pending-restore.json")).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    state
}
fn handoff<'s>(f: &Fixture, m: &Maintenance<'s>, new: bool) -> RunningHandoff<'s> {
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
    drop(ctx.authorize().unwrap().publish_pending().unwrap());
    let session = m.recovery_session().unwrap();
    let context = ctx.close_owned_connections(&session).unwrap();
    if new {
        f.title("New");
    }
    let assessment = classification::classify_session(session, flag());
    assert_eq!(
        assessment.status(),
        if new { Status::New } else { Status::Old }
    );
    let resources = assessment
        .into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap()
        .remove_pending()
        .unwrap()
        .close_resources()
        .unwrap();
    resources
        .combine_context(context)
        .unwrap()
        .verify_final()
        .unwrap()
        .prepare_q_removal()
        .unwrap()
        .unlink_q()
        .unwrap()
        .complete_disk()
        .unwrap()
        .into_running_handoff(m)
        .unwrap()
}
fn blocked(m: &Maintenance<'_>, f: &Fixture) {
    assert_eq!(m.generation(), 1);
    assert!(m.ensure_usable().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
}
#[test]
fn running_admission_old_and_new_increment_once_and_reject_old_autosave() {
    for new in [false, true] {
        let f = Fixture::new();
        let state = state(&f);
        let mut m = state.maintenance().unwrap();
        let h = handoff(&f, &m, new);
        let journal = fs::read(&f.path).unwrap();
        m.resume_verified(h).unwrap();
        assert_eq!(m.generation(), 2);
        assert!(m.ensure_usable().is_ok());
        assert_eq!(fs::read(&f.path).unwrap(), journal);
        drop(m);
        assert_eq!(
            state.operation(Some(1)).err().as_deref(),
            Some(crate::database_state::GENERATION_CONFLICT)
        );
        assert!(state.operation(Some(2)).is_ok());
        assert!(state.maintenance().is_ok());
        assert!(!f.root.join(".mno-restore-completion-v1.json").exists());
    }
}
#[test]
fn running_admission_errors_keep_handoff_at_every_boundary() {
    for target in [
        "admission-before-validation",
        "admission-after-open",
        "admission-before-close",
        "admission-after-close",
        "admission-before-ready",
    ] {
        let f = Fixture::new();
        let state = state(&f);
        let mut m = state.maintenance().unwrap();
        let h = handoff(&f, &m, false);
        let before = tree(&f.root);
        let hit = Rc::new(Cell::new(false));
        let reached = hit.clone();
        let failure = hooks::with(
            move |name| {
                if name == target {
                    reached.set(true);
                    return Err(io::Error::other("admission error"));
                }
                Ok(())
            },
            |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
            || m.resume_verified(h),
        )
        .unwrap_err();
        assert!(hit.get());
        assert!(matches!(failure.error, AdmissionError::Checkpoint(_)));
        blocked(&m, &f);
        assert_eq!(tree(&f.root), before);
        drop(failure);
        assert!(m.recovery_session().is_ok());
        assert!(m.ensure_usable().is_err());
    }
}
#[test]
fn running_admission_late_mutations_and_cancellation_block_ready() {
    for target in [
        "admission-before-validation",
        "admission-after-open",
        "admission-after-close",
        "admission-before-ready",
    ] {
        for case in [
            "journal",
            "journal-inode",
            "protection",
            "marker",
            "q",
            "recovery",
            "sidecar",
            "temporary",
            "cancel",
        ] {
            let f = Fixture::new();
            let state = state(&f);
            let mut m = state.maintenance().unwrap();
            let h = handoff(&f, &m, false);
            let (_, files, cancel) = h.0._files.view();
            let cancel = cancel.clone();
            let data = classification::parse(&files.evidence.bytes).unwrap();
            let archive = f.root.join(data.protection_path);
            let root = f.root.clone();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let captured = Rc::new(RefCell::new(None));
            let changed = captured.clone();
            let failure = hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        match case {
                            "journal" | "protection" => {
                                use std::io::Write;
                                let path = if case == "journal" {
                                    root.join("mno.sqlite")
                                } else {
                                    archive.clone()
                                };
                                fs::OpenOptions::new()
                                    .append(true)
                                    .open(path)?
                                    .write_all(b"changed")?;
                            }
                            "journal-inode" => {
                                let path = root.join("mno.sqlite");
                                let bytes = fs::read(&path)?;
                                fs::rename(&path, root.join("kept-original.sqlite"))?;
                                fs::write(path, bytes)?;
                            }
                            "marker" => fs::write(root.join(".mno-initialized-v1.json"), b"{}")?,
                            "q" => fs::write(
                                root.join(".mno-restore-completion-v1.json"),
                                b"returned",
                            )?,
                            "recovery" => fs::create_dir(root.join("recovery"))?,
                            "sidecar" => fs::write(root.join("mno.sqlite-wal"), b"")?,
                            "temporary" => fs::write(root.join(".MNO-COMPLETION-orphan"), b"")?,
                            _ => cancel.store(true, std::sync::atomic::Ordering::Release),
                        }
                        *changed.borrow_mut() = Some(tree(&root));
                    }
                    Ok(())
                },
                |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
                || m.resume_verified(h),
            )
            .unwrap_err();
            assert!(hit.get(), "{case}/{target}");
            blocked(&m, &f);
            assert_eq!(&tree(&f.root), captured.borrow().as_ref().unwrap());
            drop(failure);
        }
    }
}
#[test]
fn running_admission_real_busy_other_close_errors_and_unregister_failure_retain_binding() {
    for control in [1, 2, 3] {
        let f = Fixture::new();
        let state = state(&f);
        let mut m = state.maintenance().unwrap();
        let h = handoff(&f, &m, false);
        let before = tree(&f.root);
        CLOSE.set(control);
        let failure = m.resume_verified(h).unwrap_err();
        CLOSE.set(0);
        assert!(matches!(
            failure.error,
            AdmissionError::Close { .. } | AdmissionError::Unregister
        ));
        assert!(matches!(
            failure.retained.database,
            Some(Database::Failed(_))
        ));
        blocked(&m, &f);
        assert_eq!(tree(&f.root), before);
        if control == 1 {
            assert!(matches!(
                failure.error,
                AdmissionError::Close {
                    code: Some(rusqlite::ffi::SQLITE_BUSY)
                }
            ));
            let statement = STATEMENT.replace(std::ptr::null_mut());
            assert!(!statement.is_null());
            unsafe {
                assert_eq!(
                    rusqlite::ffi::sqlite3_step(statement),
                    rusqlite::ffi::SQLITE_ROW
                );
                assert_eq!(
                    rusqlite::ffi::sqlite3_finalize(statement),
                    rusqlite::ffi::SQLITE_OK
                );
            }
        }
        drop(failure);
        assert!(m.ensure_usable().is_err());
    }
}
#[test]
fn running_admission_wrong_guard_generation_reservation_and_overflow_retain_recovery() {
    for case in ["guard", "generation", "reservation", "overflow"] {
        let f = Fixture::new();
        let state = state(&f);
        let mut m = state.maintenance().unwrap();
        let h = handoff(&f, &m, false);
        let other = DatabaseState::ready_for_test();
        let mut other_m = other.maintenance().unwrap();
        match case {
            "generation" => m.test_admission_state(Some(2), None),
            "overflow" => m.test_admission_state(Some(u32::MAX), None),
            "reservation" => m.test_admission_state(None, Some(false)),
            _ => {}
        }
        let failure = if case == "guard" {
            other_m.resume_verified(h)
        } else {
            m.resume_verified(h)
        }
        .unwrap_err();
        assert!(matches!(
            failure.error,
            AdmissionError::Binding | AdmissionError::Overflow
        ));
        assert_eq!(
            m.generation(),
            match case {
                "overflow" => u32::MAX,
                "generation" => 2,
                _ => 1,
            }
        );
        if case == "overflow" {
            assert!(matches!(failure.error, AdmissionError::Overflow));
            assert!(failure.retained.database.is_none());
        }
        assert!(m.ensure_usable().is_err());
        assert!(!f.root.join(".mno-restore-completion-v1.json").exists());
        drop(failure);
        assert!(m.ensure_usable().is_err());
    }
}
#[test]
fn running_admission_panic_before_commit_preserves_all_owners() {
    for target in [
        "admission-before-validation",
        "admission-after-open",
        "admission-before-close",
        "admission-after-close",
        "admission-before-ready",
    ] {
        let f = Fixture::new();
        let state = state(&f);
        let mut m = state.maintenance().unwrap();
        let h = handoff(&f, &m, false);
        let (_, files, _) = h.0._files.view();
        let weak = std::sync::Arc::downgrade(&files.active_file);
        let before = tree(&f.root);
        let hit = Rc::new(Cell::new(false));
        let reached = hit.clone();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        panic!("admission panic");
                    }
                    Ok(())
                },
                |_, p, file| crate::backup::pre_restore::durability::sync_opened(p, file),
                || m.resume_verified(h),
            )
        }));
        assert!(caught.is_err());
        assert!(hit.get());
        assert!(weak.upgrade().is_some());
        blocked(&m, &f);
        assert_eq!(tree(&f.root), before);
    }
}

#[test]
fn running_admission_actual_sql_validation_error_retains_live_connection() {
    let f = Fixture::new();
    let state = state(&f);
    let mut m = state.maintenance().unwrap();
    let h = handoff(&f, &m, false);
    let before = tree(&f.root);
    CLOSE.set(4);
    let failure = m.resume_verified(h).unwrap_err();
    CLOSE.set(0);
    assert!(matches!(failure.error, AdmissionError::Sqlite));
    assert!(matches!(failure.retained.database, Some(Database::Live(_))));
    blocked(&m, &f);
    assert_eq!(tree(&f.root), before);
    drop(failure);
    assert!(m.ensure_usable().is_err());
}
#[test]
fn running_admission_rollback_failure_after_validation_blocks_ready() {
    let f = Fixture::new();
    let state = state(&f);
    let mut m = state.maintenance().unwrap();
    let h = handoff(&f, &m, false);
    let session_id = h.0.session.session_id();
    let (_, files, _) = h.0._files.view();
    let weak = std::sync::Arc::downgrade(&files.active_file);
    let before = tree(&f.root);
    let journal_before = fs::read(&f.path).unwrap();
    SNAPSHOT_VERIFIED.set(false);
    ROLLBACK_DENIED.set(0);
    CLOSE.set(5);
    let mut failure = m.resume_verified(h).unwrap_err();
    CLOSE.set(0);
    assert!(SNAPSHOT_VERIFIED.get());
    assert!(ROLLBACK_DENIED.get() > 0);
    assert!(matches!(failure.error, AdmissionError::Sqlite));
    assert_eq!(failure.retained.handoff.0.session.session_id(), session_id);
    assert!(weak.upgrade().is_some());
    let Some(Database::Live(database)) = failure.retained.database.as_mut() else {
        panic!("finish failure must retain the live SQLite/VFS binding");
    };
    assert!(database.test_is_open());
    database.verify().unwrap();
    let vfs = database.test_name();
    assert!(!unsafe { rusqlite::ffi::sqlite3_vfs_find(vfs.as_ptr()) }.is_null());
    assert!(!database.connection_mut().is_autocommit());
    blocked(&m, &f);
    assert_eq!(tree(&f.root), before);
    // Permit safe cleanup only after proving that the failed finish retained everything.
    database
        .connection_mut()
        .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>)
        .unwrap();
    drop(failure);
    assert_eq!(m.generation(), 1);
    assert!(m.ensure_usable().is_err());
    drop(m);
    assert!(state.operation(None).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), journal_before);
}

#[test]
fn running_admission_queued_autosave_checks_old_generation_after_guard_release() {
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let f = Fixture::new();
    let state = Arc::new(state(&f));
    let mut m = state.maintenance().unwrap();
    let h = handoff(&f, &m, false);
    let (tx, rx) = mpsc::channel();
    state.on_blocked(move || tx.send(()).unwrap());
    let queued = state.clone();
    let worker = std::thread::spawn(move || queued.operation(Some(1)).err());
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let competitor = state.clone();
    assert_eq!(
        std::thread::spawn(move || competitor.maintenance().err())
            .join()
            .unwrap()
            .as_deref(),
        Some("database_maintenance")
    );
    m.resume_verified(h).unwrap();
    assert_eq!(m.generation(), 2);
    drop(m);
    assert_eq!(
        worker.join().unwrap().as_deref(),
        Some(crate::database_state::GENERATION_CONFLICT)
    );
    assert!(state.operation(Some(2)).is_ok());
}
#[test]
fn running_admission_compiler_requires_consumed_running_handoff() {
    use std::process::Command;
    let source = include_str!("../../../../../../../database_state.rs");
    let start = source.find("    pub(crate) fn resume_verified(").unwrap();
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
        pub struct State{{generation:RwLock<u32>,recovery_required:AtomicBool,phase:AtomicU8}}
        pub struct Maintenance<'s>{{state:&'s State,generation:RwLockWriteGuard<'s,u32>}}
        struct Detached<'s>(&'s State);
        pub struct RunningHandoff<'s>(Detached<'s>);
        pub struct StartupHandoff<'s>(Detached<'s>);
        pub struct DiskRecoveryCompleted<'s>(Detached<'s>);
        #[derive(Debug)] pub struct RunningAdmissionFailure<'s>(&'s State);
        impl std::fmt::Debug for State{{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{{f.write_str("state")}}}}
        impl<'s> RunningHandoff<'s>{{pub fn reject_overflow(self)->RunningAdmissionFailure<'s>{{unimplemented!()}} pub fn validate_running(self,_:&Maintenance<'_>)->Result<Validated<'s>,RunningAdmissionFailure<'s>>{{unimplemented!()}}}}
        pub struct Validated<'s>(Detached<'s>);
        impl<'s> Maintenance<'s>{{{method}}}
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
        "fn valid<'s>(m:&mut api::Maintenance<'s>,h:api::RunningHandoff<'s>){m.resume_verified(h).unwrap();}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, reason) in [
        (
            "fn wrong<'s>(m:&mut api::Maintenance<'s>,h:api::StartupHandoff<'s>){m.resume_verified(h);}",
            "E0308",
        ),
        (
            "fn wrong(m:&mut api::Maintenance<'_>){m.resume_verified(true);}",
            "E0308",
        ),
        (
            "fn wrong<'s>(m:&mut api::Maintenance<'s>,h:api::DiskRecoveryCompleted<'s>){m.resume_verified(h);}",
            "E0308",
        ),
        (
            "fn twice<'s>(m:&mut api::Maintenance<'s>,h:api::RunningHandoff<'s>){m.resume_verified(h);m.resume_verified(h);}",
            "E0382",
        ),
        ("fn clone(h:api::RunningHandoff<'_>){h.clone();}", "E0599"),
        ("fn fake(){let _=api::RunningHandoff;}", "private"),
        (
            "fn escape<'s>(h:api::RunningHandoff<'s>)->api::RunningHandoff<'static>{h}",
            "lifetime may not live long enough",
        ),
        (
            "fn bound<T:serde::de::DeserializeOwned>(){} fn fake(){bound::<api::RunningHandoff<'static>>();}",
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
