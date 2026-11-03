use std::{fs, path::Path, process::Command};
#[test]
fn compiler_enforces_private_noncloneable_sessions_and_owner_lifetimes() {
    let dir = tempfile::TempDir::new().unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/database_state/recovery_session.rs");
    // Compile the actual session implementation. Stubs stand in only for its
    // owners/UUID; this checks the real access visibility and borrowed lifetimes.
    let original = fs::read_to_string(&source).unwrap();
    let projected = dir.path().join("session.rs");
    fs::write(
        &projected,
        original.replace("#[cfg(all(test, target_os = \"macos\"))]\nmod tests;", ""),
    )
    .unwrap();
    let source = projected;
    let common = format!(
        r#"
        extern crate self as uuid;
        #[derive(Debug,Clone,Copy,PartialEq)] pub struct Uuid;
        impl Uuid {{ pub fn new_v4()->Self {{Self}} }}
        mod startup {{
            pub enum StartupOutcome {{ Recovery(()) }}
            pub struct DirectoryLease;
            impl DirectoryLease {{
                pub fn ensure_current(&self)->Result<(),String>{{Ok(())}}
                pub fn database_path(&self)->std::path::PathBuf{{"book/mno.sqlite".into()}}
                pub fn directory_identity(&self)->Result<(u64,u64),String>{{Ok((1,2))}}
            }}

        }}
        mod owner {{
            enum Phase {{ Recovery }}
            pub struct DatabaseState {{
                lease:crate::startup::DirectoryLease,
                directory:std::sync::OnceLock<crate::startup::StartupOutcome>,
                startup_admitted:std::sync::atomic::AtomicBool,
                session_active:std::sync::atomic::AtomicBool,
                recovery_required:std::sync::atomic::AtomicBool,
                maintenance:std::sync::atomic::AtomicBool,
                phase:std::sync::atomic::AtomicU8,
                generation:std::sync::RwLock<u32>,
            }}
            impl DatabaseState {{
                pub fn new()->Self{{Self{{lease:crate::startup::DirectoryLease,directory:std::sync::OnceLock::new(),startup_admitted:std::sync::atomic::AtomicBool::new(false),recovery_required:std::sync::atomic::AtomicBool::new(true),maintenance:std::sync::atomic::AtomicBool::new(true),session_active:std::sync::atomic::AtomicBool::new(false),phase:std::sync::atomic::AtomicU8::new(Phase::Recovery as u8),generation:std::sync::RwLock::new(1)}}}}
                fn lease(&self)->Result<&crate::startup::DirectoryLease,String>{{Ok(&self.lease)}}
            }}
            pub struct Maintenance<'a>{{pub state:&'a DatabaseState}}

            impl Maintenance<'_> {{
                pub fn operation_id(&self)->crate::Uuid{{crate::Uuid}}
                pub fn generation(&self)->u32{{1}}
            }}
            #[path={source:?}] pub(crate) mod api;
            pub fn startup(g:&DatabaseState)->api::RecoverySession<'_, '_>{{api::RecoverySession::startup(g).unwrap()}}
            pub fn running<'a, 's>(m:&'a Maintenance<'s>)->api::RecoverySession<'a, 's>{{api::RecoverySession::running(m).unwrap()}}
        }}
    "#
    );
    let compile_mode = |probe: &str, test_mode: bool| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, format!("{common}\n{probe}")).unwrap();
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        if test_mode {
            command.arg("--cfg=test");
        }
        command
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg(path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let compile = |probe: &str| compile_mode(probe, true);
    let detached = compile(
        "fn detached<'m,'s>(m:&'m owner::Maintenance<'s>)->owner::api::TestSession<'s>{let s=owner::running(m);let b=s.test_transfer_binding().unwrap();s.into_test_session(b)}",
    );
    assert!(
        detached.status.success(),
        "{}",
        String::from_utf8_lossy(&detached.stderr)
    );
    let unavailable = compile_mode("fn unavailable(_:owner::api::TestSession<'_>){}", false);
    assert!(
        !unavailable.status.success()
            && String::from_utf8_lossy(&unavailable.stderr).contains("E0425"),
        "{}",
        String::from_utf8_lossy(&unavailable.stderr)
    );
    let positive = compile(
        "fn inspect(g:&owner::DatabaseState)->Result<(),String>{let s=owner::startup(g);let _=s.database_path()?;s.ensure_current()}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    let positive_binding = compile(
        "fn move_bound_session(m:&owner::Maintenance<'_>)->Result<(),String>{let s=owner::running(m);let b=s.bind_context(m)?;let moved=s;b.check_session(&moved)}",
    );
    assert!(
        positive_binding.status.success(),
        "{}",
        String::from_utf8_lossy(&positive_binding.stderr)
    );
    for (probe, diagnostic) in [
        (
            "fn clone(s:owner::api::TestSession<'_>){s.clone();}",
            "E0599",
        ),
        (
            "fn fake()->owner::api::TestSession<'static>{owner::api::TestSession{}}",
            "private",
        ),
        (
            "fn twice(s:owner::api::TestSession<'_>){drop(s);drop(s);}",
            "E0382",
        ),
        (
            "fn escape<'a>(s:owner::api::TestSession<'a>)->owner::api::TestSession<'static>{s}",
            "lifetime may not live long enough",
        ),
        (
            "fn exclusive(m:&mut owner::Maintenance<'_>){let s=owner::running(m);let _exclusive=&mut *m;drop(s);}",
            "E0502",
        ),
        (
            "fn binding_exclusive(m:&mut owner::Maintenance<'_>){let s=owner::running(m);let b=s.bind_context(m).unwrap();drop(s);let _exclusive=&mut *m;drop(b);}",
            "E0502",
        ),
        (
            "fn ended(m:&owner::Maintenance<'_>){let s=owner::running(m);drop(s);let _=s.bind_context(m);}",
            "E0382",
        ),
        (
            "fn clone(b:owner::api::ContextSessionBinding<'_, '_>){let _=b.clone();}",
            "E0599",
        ),
        (
            "fn fabricated()->owner::api::ContextSessionBinding<'static, 'static>{owner::api::ContextSessionBinding{}}",
            "private",
        ),
        (
            "fn escape_binding(g:&owner::DatabaseState)->owner::api::ContextSessionBinding<'static, 'static>{let m=owner::Maintenance{state:g};owner::running(&m).bind_context(&m).unwrap()}",
            "E0515",
        ),
        (
            "fn forge(){let _=owner::api::RecoverySession {access:panic!(),session_id:Uuid,_reservation:panic!()};}",
            "E0451",
        ),
        (
            "fn forge(g:&owner::DatabaseState){let _=owner::api::RecoverySession::startup(g);}",
            "E0624",
        ),
        (
            "fn reserve(){let _=owner::api::Reservation(&std::sync::atomic::AtomicBool::new(false));}",
            "E0603",
        ),
        (
            "fn copy(s:owner::api::RecoverySession<'_, '_>){let _=s.clone();}",
            "E0599",
        ),
        (
            "fn escape(g:&owner::DatabaseState)->owner::api::RecoverySession<'static, 'static>{owner::startup(g)}",
            "lifetime may not live long enough",
        ),
        (
            "fn drop_owner(){let g=owner::DatabaseState::new();let s=owner::startup(&g);drop(g);let _=s.ensure_current();}",
            "E0505",
        ),
        (
            "fn drop_guard(){let d=owner::DatabaseState::new();let m=owner::Maintenance{state:&d};let s=owner::running(&m);drop(m);let _=s.ensure_current();}",
            "E0505",
        ),
    ] {
        let output = compile(probe);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains(diagnostic),
            "{stderr}"
        );
    }
}

#[test]
fn concurrent_sessions_reserve_one_slot_without_relocking_or_unlocking() {
    use super::super::{DatabaseState, Phase};
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::{Duration, Instant};
    let timeout = Duration::from_secs(30);
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("book");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("recovery")).unwrap();
    let state = Arc::new(DatabaseState::default());
    assert!(state.initialize_startup(&root).is_err());
    let start = Arc::new(Barrier::new(9));
    let release = Arc::new(Barrier::new(9));
    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let workers: Vec<_> = (0..8)
        .map(|id| {
            let (state, start, release, tx, done_tx) = (
                state.clone(),
                start.clone(),
                release.clone(),
                tx.clone(),
                done_tx.clone(),
            );
            std::thread::spawn(move || {
                start.wait();
                // A failed reservation attempt must still reach the release
                // barrier. Preserve a successful session outside the catch.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    state.recovery_session()
                }));
                let report = match &result {
                    Ok(result) => result.as_ref().map(|_| ()).map_err(Clone::clone),
                    Err(_) => Err(format!("worker {id} panicked during reservation")),
                };
                // A receiver timeout must not cause a second worker panic.
                let _ = tx.send(report);
                // Winner retains its reservation until every attempt is reported.
                release.wait();
                let dropped =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(result)));
                let _ = done_tx.send((id, dropped.is_ok()));
            })
        })
        .collect();
    start.wait();
    let deadline = Instant::now() + timeout;
    let outcomes: Vec<_> = (0..8).map(|reported| {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|error| panic!("only {reported}/8 reservation attempts reported within {timeout:?}: {error}"))
    }).collect();
    let lock_held = crate::startup::DirectoryLease::open_existing(&root).is_err();
    // Release workers BEFORE assertions: an unexpected outcome must not strand
    // workers behind the barrier. On a receive timeout we deliberately never
    // enter this barrier or join incomplete workers (JoinHandle drop detaches).
    release.wait();
    let deadline = Instant::now() + timeout;
    for finished in 0..8 {
        let (id, dropped) = done_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|error| {
                panic!("only {finished}/8 workers finished within {timeout:?}: {error}")
            });
        assert!(
            dropped,
            "worker {id} panicked while dropping its reservation"
        );
    }
    // All workers reported completion; no barrier or user operation remains.
    for worker in workers {
        worker.join().expect("recovery worker panicked");
    }
    assert_eq!(
        outcomes.iter().filter(|r| r.is_ok()).count(),
        1,
        "worker outcomes: {outcomes:?}"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| r
                .as_ref()
                .err()
                .is_some_and(|e| e == "database_recovery_busy"))
            .count(),
        7,
        "worker outcomes: {outcomes:?}"
    );
    assert!(lock_held);
    assert!(
        !state
            .session_active
            .load(std::sync::atomic::Ordering::Acquire)
    );
    let session = state.recovery_session().unwrap();
    drop(session);
    assert_eq!(
        state.phase.load(std::sync::atomic::Ordering::Acquire),
        Phase::Recovery as u8
    );
    assert!(state.operation(None).is_err());
    assert!(state.maintenance().is_err());
    assert_eq!(*state.generation.read().unwrap(), 1);
    assert!(!root.join("mno.sqlite").exists());
    assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
    drop(state);
    assert!(crate::startup::DirectoryLease::open_existing(&root).is_ok());
}

#[test]
fn phases_are_private_fail_closed_and_generation_is_unchanged() {
    use super::super::{DatabaseState, Phase};
    use std::sync::atomic::Ordering;
    let dir = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Starting as u8);
    assert!(state.operation(None).is_err());
    let root = dir.path().join("new-book");
    state.initialize_startup(&root).unwrap();
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Ready as u8);
    assert_eq!(state.operation(Some(1)).unwrap().snapshot(()).generation, 1);
    let maintenance = state.maintenance().unwrap();
    let session = maintenance.recovery_session().unwrap();
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Recovery as u8);
    drop(session);
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Recovery as u8);
    drop(maintenance);
    assert_eq!(
        state.operation(Some(1)).err().unwrap(),
        super::super::RECOVERY_REQUIRED
    );
    assert_eq!(*state.generation.read().unwrap(), 1);
    assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
    let failed = DatabaseState::default();
    let established = dir.path().join("missing-book");
    std::fs::create_dir(&established).unwrap();
    assert!(failed.initialize_startup(&established).is_err());
    assert_eq!(failed.phase.load(Ordering::Acquire), Phase::Blocked as u8);
    assert!(failed.operation(None).is_err());
    assert!(!established.join("mno.sqlite").exists());
}

#[test]
fn unwinding_releases_only_the_session_slot_and_not_recovery() {
    use super::super::{DatabaseState, Phase};
    use std::sync::atomic::Ordering;
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("book");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    assert!(state.initialize_startup(&root).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _session = state.recovery_session().unwrap();
            panic!("controlled test unwind");
        }))
        .is_err()
    );
    drop(state.recovery_session().unwrap());
    assert_eq!(state.phase.load(Ordering::Acquire), Phase::Recovery as u8);
    assert_eq!(*state.generation.read().unwrap(), 1);
    assert!(state.operation(None).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
    assert!(!root.join("mno.sqlite").exists());
}

#[test]
fn owner_publication_alone_cannot_admit_operations_or_maintenance() {
    use super::super::{DatabaseState, Phase};
    use std::sync::atomic::Ordering;
    let state = DatabaseState::ready_for_test();
    // Reproduce the internal initialize_startup window after installing the
    // owner, before publishing the final Ready phase. Test-only direct access.
    for phase in [Phase::Starting, Phase::Blocked] {
        state.phase.store(phase as u8, Ordering::Release);
        assert_eq!(
            state.operation(None).err().unwrap(),
            "database_startup_blocked"
        );
        assert_eq!(
            state.maintenance().err().unwrap(),
            "database_startup_blocked"
        );
    }
}

#[test]
fn binding_rejects_inactive_reservation_and_successive_sessions_under_same_guard() {
    use super::super::DatabaseState;
    use std::sync::atomic::Ordering;
    let dir = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    state.initialize_startup(&dir.path().join("book")).unwrap();
    let guard = state.maintenance().unwrap();
    let first = guard.recovery_session().unwrap();
    let binding = first.bind_context(&guard).unwrap();
    binding.check_session(&first).unwrap();
    let id = first.session_id();
    drop(first);
    let second = guard.recovery_session().unwrap();
    assert_ne!(id, second.session_id());
    assert!(binding.check_session(&second).is_err());
    // Test-only access to this module's actual slot; no production invalidation API.
    second._reservation.0.store(false, Ordering::Release);
    assert!(second.bind_context(&guard).is_err());
    second._reservation.0.store(true, Ordering::Release);
    assert!(guard.recovery_session().is_err());
    assert_eq!(guard.generation(), 1);
    assert!(guard.ensure_usable().is_err());
}

#[test]
fn context_binding_checks_guard_instance_even_when_ids_and_generation_match() {
    use super::super::DatabaseState;
    let dir = tempfile::TempDir::new().unwrap();
    let first_state = DatabaseState::default();
    let second_state = DatabaseState::default();
    first_state
        .initialize_startup(&dir.path().join("a"))
        .unwrap();
    second_state
        .initialize_startup(&dir.path().join("b"))
        .unwrap();
    let first = first_state.maintenance().unwrap();
    let mut other = second_state.maintenance().unwrap();
    // Only this child test can mutate private Maintenance fields. Production
    // cannot fabricate a guard from a UUID or use these equal IDs as authority.
    other.operation_id = first.operation_id();
    assert_eq!(first.generation(), other.generation());
    let session = other.recovery_session().unwrap();
    assert_eq!(first.operation_id(), other.operation_id());
    assert!(session.bind_context(&first).is_err());
    assert!(session.bind_context(&other).is_ok());
}

#[test]
fn binding_recheck_rejects_changed_operation_generation_and_directory_records() {
    use super::super::DatabaseState;
    let dir = tempfile::TempDir::new().unwrap();
    let state = DatabaseState::default();
    state.initialize_startup(&dir.path().join("book")).unwrap();
    let guard = state.maintenance().unwrap();
    let session = guard.recovery_session().unwrap();
    for field in 0..4 {
        let mut binding = session.bind_context(&guard).unwrap();
        match field {
            0 => binding.operation_id = uuid::Uuid::new_v4(),
            1 => binding.generation += 1,
            2 => binding.directory.1 ^= 1,
            _ => binding.database_path.push("wrong"),
        }
        assert!(binding.check_session(&session).is_err());
    }
    assert!(guard.recovery_session().is_err());
    assert_eq!(guard.generation(), 1);
}

// Assertions only, compiled solely by this macOS test module. The Running
// observer uses the existing write guard rather than reacquiring its lock.
impl super::DatabaseState {
    pub(crate) fn assert_startup_recovery_generation_for_test(&self, expected: u32) {
        assert_eq!(
            self.phase.load(std::sync::atomic::Ordering::Acquire),
            crate::database_state::Phase::Recovery as u8
        );
        assert_eq!(*self.generation.read().unwrap(), expected);
    }
}
impl super::Maintenance<'_> {
    pub(crate) fn assert_running_recovery_generation_for_test(&self, expected: u32) {
        assert_eq!(
            self.state.phase.load(std::sync::atomic::Ordering::Acquire),
            crate::database_state::Phase::Recovery as u8
        );
        assert_eq!(self.generation(), expected);
    }
}
