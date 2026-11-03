use super::super::tests::{both, snapshot, unlinked};
use super::*;
use crate::backup::restore_context::pending::classification::result::receipt::hooks;
use crate::backup::restore_context::pending::classification::tests::{Fixture, flag, tree};
use std::{cell::Cell, fs, rc::Rc};
const POINTS: [&str; 6] = [
    "handoff-before-check",
    "handoff-after-check",
    "handoff-unwrapped",
    "handoff-context",
    "handoff-files",
    "handoff-detached",
];
fn binding(d: &DiskRecoveryCompleted<'_, '_>) -> (uuid::Uuid, *const (), uuid::Uuid, u32) {
    snapshot(d.unlinked.as_ref().unwrap())
}
fn completed<'a, 's>(all: AllConnectionsClosed<'a, 's>) -> DiskRecoveryCompleted<'a, 's> {
    unlinked(all).complete_disk().unwrap()
}
fn transfer_origin<'m, 's>(
    d: DiskRecoveryCompleted<'m, 's>,
) -> Result<Detached<'s>, HandoffFailure<'m, 's>> {
    let route = match d.unlinked.as_ref().unwrap().post_unlink_parts().0.held() {
        Held::Running { context, .. } => Route::Running(context.test_maintenance()),
        Held::Startup { .. } => Route::Startup,
    };
    transfer(d, route)
}
#[test]
fn handoff_both_origins_keep_same_session_slot_files_and_phase() {
    both(|f, all, guard| {
        let running = matches!(all.held(), Held::Running { .. });
        let d = completed(all);
        let before = binding(&d);
        let closed = d
            .unlinked
            .as_ref()
            .unwrap()
            .post_unlink_parts()
            .0
            .held()
            .resources()
            .closed();
        let staging = closed.retained_staging_path().to_path_buf();
        let file = std::sync::Arc::downgrade(&closed.view().files.active_file);
        let original = tree(&f.root);
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let record = seen.clone();
        let done = hooks::with(
            move |name| {
                if POINTS.contains(&name) {
                    record.borrow_mut().push(name.to_owned());
                }
                Ok(())
            },
            super::super::tests::actual,
            || {
                if running {
                    let classification::Access::Session(s) = d
                        .unlinked
                        .as_ref()
                        .unwrap()
                        .post_unlink_parts()
                        .0
                        .held()
                        .resources()
                        .closed()
                        .view()
                        .access
                    else {
                        panic!()
                    };
                    // Test caller obtains the real guard from the already retained context.
                    let Held::Running { context, .. } =
                        d.unlinked.as_ref().unwrap().post_unlink_parts().0.held()
                    else {
                        panic!()
                    };
                    let _ = s;
                    let m = context.test_maintenance();
                    d.into_running_handoff(m).map(|h| h.0)
                } else {
                    d.into_startup_handoff().map(|h| h.0)
                }
            },
        )
        .unwrap();
        assert_eq!(&*seen.borrow(), &POINTS);
        assert_eq!(done.session.session_id(), before.0);
        assert_eq!(done.session.slot(), before.1);
        assert_eq!(done.session.generation(), before.3);
        assert_eq!(done.session.running(), running);
        assert!(staging.exists());
        assert!(file.upgrade().is_some());
        assert_eq!(tree(&f.root), original);
        guard(true);
        let path = staging.clone();
        let weak = file.clone();
        let root = f.root.clone();
        crate::database_state::recovery_session::reservation_drop_probe::with(
            move || {
                assert!(
                    !path.exists(),
                    "closed staging must drop before reservation"
                );
                assert!(
                    weak.upgrade().is_none(),
                    "file owner must drop before reservation"
                );
                assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
            },
            || drop(done),
        );
        assert!(!staging.exists());
        assert!(file.upgrade().is_none());
        guard(false);
    });
}
#[test]
fn handoff_failures_at_every_boundary_retain_complete_owner() {
    for point in POINTS {
        both(|f, all, guard| {
            let d = completed(all);
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                move |name| {
                    if name == point {
                        reached.set(true);
                        return Err(io::Error::other("handoff failure"));
                    }
                    Ok(())
                },
                super::super::tests::actual,
                || transfer_origin(d),
            )
            .err()
            .unwrap();
            assert!(hit.get(), "injected checkpoint not reached: {point}");
            assert!(
                matches!(failure.error, HandoffError::Checkpoint(_)),
                "unexpected error at {point}: {:?}",
                failure.error
            );
            assert_eq!(tree(&f.root), before);
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn handoff_panics_keep_closed_staging_and_reservation() {
    for point in POINTS {
        both(|f, all, guard| {
            let d = completed(all);
            let path = d
                .unlinked
                .as_ref()
                .unwrap()
                .post_unlink_parts()
                .0
                .held()
                .resources()
                .closed()
                .retained_staging_path()
                .to_path_buf();
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == point {
                            reached.set(true);
                            panic!("transfer panic");
                        }
                        Ok(())
                    },
                    super::super::tests::actual,
                    || transfer_origin(d),
                )
            }));
            assert!(hit.get());
            assert!(caught.is_err());
            assert!(path.exists());
            assert_eq!(tree(&f.root), before);
            guard(true);
        });
    }
}
#[test]
fn handoff_changed_files_and_cancellation_fail_before_split() {
    for case in ["journal", "sidecar", "marker", "cancel"] {
        both(|f, all, guard| {
            let d = completed(all);
            let cancel = d
                .unlinked
                .as_ref()
                .unwrap()
                .post_unlink_parts()
                .0
                .held()
                .resources()
                .closed()
                .view()
                .cancel
                .clone();
            match case {
                "journal" => {
                    use std::io::Write;
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&f.path)
                        .unwrap()
                        .write_all(b"changed")
                        .unwrap();
                }
                "sidecar" => fs::write(f.root.join("mno.sqlite-wal"), b"").unwrap(),
                "marker" => fs::write(f.root.join(".mno-initialized-v1.json"), b"{}").unwrap(),
                _ => cancel.store(true, std::sync::atomic::Ordering::Release),
            }
            let before = tree(&f.root);
            let failure = d.into_startup_handoff().err().unwrap();
            assert!(matches!(failure.error, HandoffError::Files(_)));
            assert!(matches!(*failure.stage, Stage::Disk(_)));
            assert_eq!(tree(&f.root), before);
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn handoff_running_ends_all_maintenance_loans_without_changing_generation() {
    use crate::{
        backup::{pre_restore, restore, restore_context},
        database_state::DatabaseState,
    };
    let f = Fixture::new();
    fs::remove_file(f.root.join("recovery/pending-restore.json")).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let mut m = state.maintenance().unwrap();
    let incoming =
        restore::prepare_candidate(&f._dir.path().join("incoming.mno-backup"), flag()).unwrap();
    let active = restore_context::ActiveDatabase::open(&f.path).unwrap();
    let protection = pre_restore::create_bound(&m, &active, flag()).unwrap();
    let mut ctx = restore_context::RestoreContext {
        terminal_policy: None,
        maintenance: &m,
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
    let all=crate::backup::restore_context::pending::classification::result::receipt::all_closed::tests::resources(session).combine_context(context).unwrap();
    let d = completed(all);
    let before = binding(&d);
    let done = d.into_running_handoff(&m).unwrap();
    fn exclusive(_: &mut Maintenance<'_>) {}
    exclusive(&mut m);
    assert!(done.0.session.matches_running(&m));
    assert_eq!(done.0.session.session_id(), before.0);
    assert_eq!(done.0.session.slot(), before.1);
    assert_eq!(m.generation(), 1);
    assert!(m.ensure_usable().is_err());
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _done = done;
        panic!("after handoff");
    }));
    assert!(caught.is_err());
    let next = m.recovery_session().unwrap();
    assert_ne!(next.session_id(), before.0);
    drop(next);
    assert_eq!(m.generation(), 1);
    assert!(m.ensure_usable().is_err());
}
#[test]
fn handoff_rejects_wrong_origin_guard_and_stale_bindings_before_split() {
    for case in 0..5 {
        both(|f, all, guard| {
            let running = matches!(all.held(), Held::Running { .. });
            let mut d = completed(all);
            let other = crate::database_state::DatabaseState::ready_for_test();
            let m = other.maintenance().unwrap();
            if case >= 2 {
                let all = &mut d.unlinked.as_mut().unwrap().retained.as_mut().unwrap().all;
                if running {
                    let Held::Running { context, .. } = all.held.as_mut().unwrap() else {
                        panic!()
                    };
                    context.test_corrupt_binding(case - 2);
                } else {
                    all.generation += 1;
                }
            }
            let before = tree(&f.root);
            let failure = match case {
                0 => {
                    if running {
                        d.into_startup_handoff().map(|h| h.0)
                    } else {
                        d.into_running_handoff(&m).map(|h| h.0)
                    }
                }
                1 => d.into_running_handoff(&m).map(|h| h.0),
                _ => transfer_origin(d),
            }
            .err()
            .unwrap();
            assert!(matches!(*failure.stage, Stage::Disk(_)));
            assert_eq!(tree(&f.root), before);
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn handoff_startup_competitors_are_busy_at_every_stable_transfer_boundary() {
    let f = Fixture::new();
    let state = std::sync::Arc::new(crate::database_state::DatabaseState::default());
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let d=completed(crate::backup::restore_context::pending::classification::result::receipt::all_closed::tests::resources(state.recovery_session().unwrap()).confirm_startup().unwrap());
    let competing = state.clone();
    let seen = Rc::new(Cell::new(0));
    let count = seen.clone();
    let done = hooks::with(
        move |name| {
            if POINTS.contains(&name) {
                let state = competing.clone();
                let result = std::thread::spawn(move || state.recovery_session().err())
                    .join()
                    .unwrap();
                assert_eq!(result.as_deref(), Some("database_recovery_busy"));
                count.set(count.get() + 1);
            }
            Ok(())
        },
        super::super::tests::actual,
        || d.into_startup_handoff(),
    )
    .unwrap();
    assert_eq!(seen.get(), POINTS.len());
    state.assert_startup_recovery_generation_for_test(1);
    drop(done);
    assert!(state.recovery_session().is_ok());
    state.assert_startup_recovery_generation_for_test(1);
}
#[test]
fn handoff_compiler_enforces_consumption_origins_and_lifetimes() {
    use std::process::Command;
    let source = include_str!("handoff.rs");
    let methods = source
        .split("// Consuming handoff entry methods.")
        .nth(1)
        .unwrap()
        .split("#[cfg(all(test")
        .next()
        .unwrap();
    let wrappers = source
        .split("pub(crate) struct RunningHandoff")
        .nth(1)
        .unwrap()
        .split("#[derive(Debug)]")
        .next()
        .unwrap();
    let wrappers = format!("pub(crate) struct RunningHandoff{wrappers}");
    let common = format!(
        r#"
    extern crate serde;
    mod api {{
        #[derive(Debug)] pub struct State;
        #[derive(Debug)] pub struct Maintenance<'s>(&'s State);
        pub struct DiskRecoveryCompleted<'m,'s>(&'m Maintenance<'s>);
        pub struct FinalFilesVerified<'m,'s>(&'m Maintenance<'s>);
        pub struct QRemovalPrepared<'m,'s>(&'m Maintenance<'s>);
        pub struct QUnlinked<'m,'s>(&'m Maintenance<'s>);
        struct Detached<'s>(&'s State);
        {wrappers}
        #[derive(Debug)] pub struct HandoffFailure<'m,'s>(&'m Maintenance<'s>);
        enum Route<'r,'s> {{Running(&'r Maintenance<'s>),Startup}}
        type Result<T,E> = std::result::Result<T,E>;
        fn transfer<'m,'s>(_:DiskRecoveryCompleted<'m,'s>,_:Route<'_,'s>)->Result<Detached<'s>,HandoffFailure<'m,'s>> {{unimplemented!()}}
        {methods}
        pub fn maintenance(s:&State)->Maintenance<'_>{{Maintenance(s)}}
        pub fn disk<'m,'s>(m:&'m Maintenance<'s>)->DiskRecoveryCompleted<'m,'s>{{DiskRecoveryCompleted(m)}}
        pub fn exclusive(_: &mut Maintenance<'_>){{}}
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
        let p = dir.path().join("probe.rs");
        fs::write(&p, format!("{common}\n{probe}")).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-type=lib"])
            .arg(p)
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
        "fn valid(s:&api::State){let mut m=api::maintenance(s);let h=api::disk(&m).into_running_handoff(&m).unwrap();api::exclusive(&mut m);drop(h);}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, reason) in [
        ("fn fake(){let _=api::RunningHandoff;}", "private"),
        ("fn clone(h:api::RunningHandoff<'_>){h.clone();}", "E0599"),
        (
            "fn twice(d:api::DiskRecoveryCompleted<'_,'_>,m:&api::Maintenance<'_>){let _=d.into_running_handoff(m);let _=d.into_running_handoff(m);}",
            "E0382",
        ),
        (
            "fn origin(h:api::StartupHandoff<'_>)->api::RunningHandoff<'_>{h}",
            "E0308",
        ),
        (
            "fn escape<'s>(h:api::RunningHandoff<'s>)->api::RunningHandoff<'static>{h}",
            "lifetime may not live long enough",
        ),
        (
            "fn borrowed(s:&api::State){let mut m=api::maintenance(s);let d=api::disk(&m);api::exclusive(&mut m);drop(d);}",
            "E0502",
        ),
        (
            "fn premature(f:api::FinalFilesVerified<'_,'_>,m:&api::Maintenance<'_>){f.into_running_handoff(m);}",
            "E0599",
        ),
        (
            "fn premature(q:api::QRemovalPrepared<'_,'_>){q.into_startup_handoff();}",
            "E0599",
        ),
        (
            "fn premature(q:api::QUnlinked<'_,'_>){q.into_startup_handoff();}",
            "E0599",
        ),
        (
            "fn twice(h:api::RunningHandoff<'_>){drop(h);drop(h);}",
            "E0382",
        ),
        (
            "fn bound<T:serde::de::DeserializeOwned>(){} fn serde(){bound::<api::StartupHandoff<'static>>();}",
            "E0277",
        ),
        (
            "fn bound<T:serde::Serialize>(){} fn serde(){bound::<api::RunningHandoff<'static>>();}",
            "E0277",
        ),
    ] {
        let result = compile(probe);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && stderr.contains(reason),
            "{probe}: {stderr}"
        );
    }
}
