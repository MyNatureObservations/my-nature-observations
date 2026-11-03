use super::super::hooks;
use super::*;
use crate::{
    backup::restore_context::{
        self,
        close::ContextConnectionsClosed,
        pending::classification::{
            self,
            tests::{Fixture, flag, tree},
        },
    },
    database_state::{DatabaseState, Maintenance, recovery_session::RecoverySession},
};
use std::{cell::Cell, fs, rc::Rc, sync::atomic::Ordering};

pub(super) fn resources<'a, 's>(session: RecoverySession<'a, 's>) -> ResourcesClosed<'a, 's> {
    classification::classify_session(session, flag())
        .into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap()
        .remove_pending()
        .unwrap()
        .close_resources()
        .unwrap()
}
pub(super) fn running(
    action: impl for<'a, 's> FnOnce(
        &Fixture,
        &Maintenance<'a>,
        ContextConnectionsClosed<'a, 's>,
        ResourcesClosed<'a, 's>,
    ),
) {
    running_inner(action, true)
}
pub(super) fn running_changed(
    action: impl for<'a, 's> FnOnce(
        &Fixture,
        &Maintenance<'a>,
        ContextConnectionsClosed<'a, 's>,
        ResourcesClosed<'a, 's>,
    ),
) {
    running_inner(action, false)
}
fn running_inner(
    action: impl for<'a, 's> FnOnce(
        &Fixture,
        &Maintenance<'a>,
        ContextConnectionsClosed<'a, 's>,
        ResourcesClosed<'a, 's>,
    ),
    unchanged: bool,
) {
    let f = Fixture::new();
    fs::remove_file(f.root.join("recovery/pending-restore.json")).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let incoming = crate::backup::restore::prepare_candidate(
        &f._dir.path().join("incoming.mno-backup"),
        flag(),
    )
    .unwrap();
    restore_context::with_active(&state, &f.path, incoming, flag(), |mut ctx| {
        let maintenance = ctx.maintenance;
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let context_staging = ctx.protection.as_ref().unwrap().candidate().database_path();
        let session = maintenance.recovery_session().unwrap();
        let context = ctx.close_owned_connections(&session).unwrap();
        let closed = resources(session);
        let mut before = tree(&f.root);
        // Normal Drop may clean this exact, already-closed private staging file.
        // Every persistent file, including the safety archive, remains compared.
        before.remove(&context_staging);
        action(&f, maintenance, context, closed);
        assert_eq!(maintenance.generation(), 1);
        assert!(maintenance.ensure_usable().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        let mut after = tree(&f.root);
        after.remove(&context_staging);
        if unchanged {
            assert!(
                after == before,
                "persistent journal/recovery/backup files must be unchanged"
            );
        }
        Ok(())
    })
    .unwrap();
    assert!(state.operation(Some(1)).is_err());
}
fn running_guards(f: &Fixture, maintenance: &Maintenance<'_>) {
    assert_eq!(
        maintenance.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    assert_eq!(maintenance.generation(), 1);
    assert!(maintenance.ensure_usable().is_err());
}
#[test]
fn valid_running_combination_keeps_session_reservation_files_and_generation() {
    running(|f, m, context, closed| {
        let id = match closed.closed().view().access {
            classification::Access::Session(s) => s.session_id(),
            _ => panic!(),
        };
        let before = tree(&f.root);
        let all = closed.combine_context(context).unwrap();
        assert!(tree(&f.root) == before, "live capability retains all files");
        assert_eq!(all.generation, 1);
        let Held::Running { resources, .. } = all.held() else {
            panic!("running origin")
        };
        match resources.closed().view().access {
            classification::Access::Session(s) => assert_eq!(s.session_id(), id),
            _ => panic!(),
        }
        assert_eq!(
            resources
                .closed()
                .view()
                .files
                .active_file
                .metadata()
                .unwrap()
                .len(),
            fs::metadata(&f.path).unwrap().len()
        );
        running_guards(f, m);
        drop(all);
        // Dropping authority releases only the session slot, never Recovery.
        drop(m.recovery_session().unwrap());
        assert!(m.ensure_usable().is_err());
    });
}
#[test]
fn successor_session_under_same_maintenance_rejects_old_context_proof() {
    running(|f, m, context, original| {
        let first = match original.closed().view().access {
            classification::Access::Session(s) => s.session_id(),
            _ => panic!(),
        };
        drop(original);
        let successor = m.recovery_session().unwrap();
        assert_ne!(first, successor.session_id());
        let second = resources(successor);
        let failed = second
            .combine_context(context)
            .err()
            .expect("different session must fail");
        assert!(matches!(failed.error(), AllCloseError::Binding));
        assert!(matches!(failed.held, Held::Running { .. }));
        running_guards(f, m);
    });
}
#[test]
fn tampered_resource_bindings_never_issue_combined_capability_and_keep_both_proofs() {
    for field in 0..5 {
        running(|f, m, context, mut closed| {
            let (binding, files) = closed.closed_mut().test_parts_mut();
            match field {
                0 => binding.restore_id = uuid::Uuid::new_v4(),
                1 => binding.database.file = "0000000000000000".into(),
                2 => binding.directory.file = "0000000000000000".into(),
                3 => files.root.push("wrong"),
                _ => {
                    let other = tempfile::tempfile().unwrap();
                    files.active_file = std::sync::Arc::new(other);
                }
            }
            let before = tree(&f.root);
            let failed = closed.combine_context(context).err().unwrap();
            assert!(
                tree(&f.root) == before,
                "failure retains both closed proofs and their staging"
            );
            assert!(matches!(failed.error(), AllCloseError::Binding));
            assert!(matches!(failed.held, Held::Running { .. }));
            running_guards(f, m);
        });
    }
}
#[test]
fn running_session_cannot_select_startup_route() {
    running(|f, m, context, closed| {
        let failed = closed.confirm_startup().err().unwrap();
        assert!(
            matches!(failed.error(), AllCloseError::Session(e) if e=="database_recovery_startup_required")
        );
        assert!(matches!(failed.held, Held::Startup { .. }));
        running_guards(f, m);
        drop(context);
    });
}
#[test]
fn startup_recovery_without_context_is_valid_but_never_unlocks() {
    let f = Fixture::new();
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let session = state.recovery_session().unwrap();
    let generation = session.startup_generation().unwrap();
    let id = session.session_id();
    let closed = resources(session);
    let before = tree(&f.root);
    let all = closed.confirm_startup().unwrap();
    assert_eq!(all.generation, generation);
    let Held::Startup { resources } = all.held() else {
        panic!("startup origin")
    };
    match resources.closed().view().access {
        classification::Access::Session(s) => assert_eq!(s.session_id(), id),
        _ => panic!(),
    }
    assert!(state.recovery_session().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    drop(all);
    let next = state.recovery_session().unwrap();
    assert_eq!(next.startup_generation().unwrap(), generation);
    assert!(state.operation(Some(generation)).is_err());
    assert_eq!(tree(&f.root), before);
}
#[test]
fn startup_cannot_supply_resources_for_a_running_context() {
    running(|f, m, context, _closed| {
        let other = Fixture::new();
        let state = DatabaseState::default();
        assert!(state.initialize_legacy_startup(&other.root).is_err());
        let closed = resources(state.recovery_session().unwrap());
        let before = tree(&other.root);
        let failed = closed.combine_context(context).err().unwrap();
        assert!(matches!(failed.error(), AllCloseError::Binding));
        assert!(state.recovery_session().is_err());
        assert_eq!(tree(&other.root), before);
        running_guards(f, m); // Original running session is also still held.
    });
}
#[test]
fn proofs_from_different_contexts_and_maintenance_operations_are_rejected() {
    running(|f, m, context, _original| {
        running(|other, other_m, other_context, resources| {
            let failed = resources.combine_context(context).err().unwrap();
            assert!(matches!(failed.error(), AllCloseError::Binding));
            running_guards(other, other_m);
            running_guards(f, m);
            drop(other_context);
        });
    });
}
#[test]
fn errors_and_cancellation_keep_all_guards_without_disk_changes() {
    for point in ["all-close-before-binding", "all-close-after-binding"] {
        running(|f, m, context, closed| {
            let reached = Rc::new(Cell::new(false));
            let hit = reached.clone();
            let failed = hooks::with(
                move |p| {
                    if p == point {
                        hit.set(true);
                        return Err(io::Error::other("injected combination error"));
                    }
                    Ok(())
                },
                |_, _, _| unreachable!("combination does no durability I/O"),
                || closed.combine_context(context),
            )
            .err()
            .unwrap();
            assert!(reached.get());
            assert!(matches!(failed.error(), AllCloseError::Io(_)));
            running_guards(f, m);
        });
    }
    running(|f, m, context, closed| {
        closed.closed().view().cancel.store(true, Ordering::Release);
        let failed = closed.combine_context(context).err().unwrap();
        assert!(matches!(failed.error(), AllCloseError::Cancelled));
        running_guards(f, m);
    });
}
#[test]
fn panic_at_combination_boundaries_drops_only_closed_resources_and_session_slot() {
    for point in ["all-close-before-binding", "all-close-after-binding"] {
        running(|f, m, context, closed| {
            let reached = Rc::new(Cell::new(false));
            let hit = reached.clone();
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |p| {
                        if p == point {
                            hit.set(true);
                            panic!("injected {point}");
                        }
                        Ok(())
                    },
                    |_, _, _| unreachable!(),
                    || closed.combine_context(context),
                )
            }));
            assert!(panicked.is_err());
            assert!(reached.get());
            assert!(m.ensure_usable().is_err());
            assert_eq!(m.generation(), 1);
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            drop(m.recovery_session().unwrap());
        });
    }
}

#[test]
fn changed_context_generation_restore_and_session_ids_are_rejected() {
    for field in 0..3 {
        running(|f, m, mut context, resources| {
            context.test_corrupt_binding(field);
            let failed = resources.combine_context(context).err().unwrap();
            assert!(matches!(failed.error(), AllCloseError::Binding));
            running_guards(f, m);
        });
    }
}

#[test]
fn cancellation_after_binding_does_not_issue_a_combined_proof() {
    running(|f, m, context, closed| {
        let cancel = closed.closed().view().cancel.clone();
        let hit = Rc::new(Cell::new(false));
        let reached = hit.clone();
        let failed = hooks::with(
            move |p| {
                if p == "all-close-after-binding" {
                    reached.set(true);
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            },
            |_, _, _| unreachable!(),
            || closed.combine_context(context),
        )
        .err()
        .unwrap();
        assert!(hit.get());
        assert!(matches!(failed.error(), AllCloseError::Cancelled));
        running_guards(f, m);
    });
}

#[test]
fn compiler_checks_consumption_private_construction_and_guard_lifetimes() {
    // Keep actual types, private fields, Drop-check and consuming entry methods.
    // Only the comparison backend is stubbed; real runtime tests above cover it.
    let source = include_str!("all_closed.rs");
    let declarations = source.split("// Verification backend.").next().unwrap();
    let common = format!(
        r#"
        extern crate serde;
        mod backup {{pub mod restore_context {{pub mod close {{
            pub struct ContextConnectionsClosed<'a, 's>(pub &'a (), pub &'s ());
        }}}}}}
        mod api {{
            pub struct ResourcesClosed<'a, 's>(pub &'a (), pub &'s ());
            fn event(_:&str)->std::io::Result<()>{{Ok(())}}
            pub mod completion {{
                {declarations}
                fn combine<'a, 's>(_:Held<'a, 's>)->Result<AllConnectionsClosed<'a, 's>,AllCloseFailure<'a, 's>>{{unimplemented!()}}
            }}
            pub fn resources(owner:&())->ResourcesClosed<'_, '_>{{ResourcesClosed(owner,owner)}}
        }}
        use api::{{ResourcesClosed,completion::{{AllConnectionsClosed,AllCloseFailure}}}};
        use backup::restore_context::close::ContextConnectionsClosed;
    "#
    );
    let dir = tempfile::TempDir::new().unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
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
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--extern")
            .arg(format!("serde={}", serde.display()))
            .arg(path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        "fn running<'a, 's>(r:ResourcesClosed<'a, 's>,c:ContextConnectionsClosed<'a, 's>){let _=r.combine_context(c);} fn startup(r:ResourcesClosed<'_, '_>){let _=r.confirm_startup();}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, diagnostic) in [
        (
            "fn consume(_:AllConnectionsClosed<'_, '_>){} fn twice_proof(p:AllConnectionsClosed<'_, '_>){consume(p);consume(p);}",
            "E0382",
        ),
        (
            "fn twice<'a, 's>(r:ResourcesClosed<'a, 's>,c:ContextConnectionsClosed<'a, 's>){let _=r.combine_context(c);let _=r.confirm_startup();}",
            "E0382",
        ),
        (
            "fn twice_context<'a, 's>(a:ResourcesClosed<'a, 's>,b:ResourcesClosed<'a, 's>,c:ContextConnectionsClosed<'a, 's>){let _=a.combine_context(c);let _=b.combine_context(c);}",
            "E0382",
        ),
        (
            "fn clone(p:AllConnectionsClosed<'_, '_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "fn copy<'a, 's>(){fn copy<T:Copy>(){} copy::<AllConnectionsClosed<'a, 's>>();}",
            "E0277",
        ),
        (
            "fn deserialize<'a, 's>(){fn de<T:serde::de::DeserializeOwned>(){} de::<AllConnectionsClosed<'a, 's>>();}",
            "E0277",
        ),
        (
            "fn serialize(p:AllConnectionsClosed<'_, '_>){fn ser<T:serde::Serialize>(_:T){} ser(p);}",
            "E0277",
        ),
        (
            "fn fake()->AllConnectionsClosed<'static, 'static>{AllConnectionsClosed{}}",
            "private",
        ),
        (
            "fn boolean(r:ResourcesClosed<'_, '_>){let _=r.combine_context(true);}",
            "E0308",
        ),
        (
            "fn bypass(r:ResourcesClosed<'_, '_>){let _=r.confirm_startup(None::<()>);}",
            "E0061",
        ),
        (
            "fn sql(p:AllConnectionsClosed<'_, '_>){let _=p.connection();}",
            "E0599",
        ),
        (
            "fn ready(p:AllConnectionsClosed<'_, '_>){p.unlock();}",
            "E0599",
        ),
        (
            "fn failure<'a,'s>(e:AllCloseFailure<'a, 's>)->AllConnectionsClosed<'a, 's>{e.into()}",
            "E0277",
        ),
        (
            "fn escape(owner:&())->AllConnectionsClosed<'static, 'static>{api::resources(owner).confirm_startup().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn drop_owner(){let owner=Box::new(());let p=api::resources(&owner).confirm_startup().unwrap();drop(owner);drop(p);}",
            "E0505",
        ),
        (
            "fn drop_error_owner(){let owner=Box::new(());let p=api::resources(&owner).confirm_startup().err().unwrap();drop(owner);drop(p);}",
            "E0505",
        ),
    ] {
        let output = compile(probe);
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && errors.contains(diagnostic),
            "{probe}: {errors}"
        );
    }
}

#[test]
fn running_file_checks_borrow_the_complete_closed_chain_without_unlocking() {
    running(|f, m, context, resources| {
        let all = resources.combine_context(context).unwrap();
        let before = tree(&f.root);
        {
            let checks = all.file_checks().unwrap();
            checks.check().unwrap();
            running_guards(f, m);
            assert_eq!(tree(&f.root), before);
        }
        // Dropping observations does not consume the owning capability or its slot.
        running_guards(f, m);
        all.file_checks().unwrap().check().unwrap();
    });
}
