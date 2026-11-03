use super::super::super::hooks;
use super::super::tests::{resources, running_changed};
use super::*;
use crate::{
    backup::restore_context::pending::classification::tests::{Fixture, tree},
    database_state::DatabaseState,
};
use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
    sync::atomic::Ordering,
};

pub(super) fn both(
    mut action: impl for<'a, 's> FnMut(&Fixture, AllConnectionsClosed<'a, 's>, &dyn Fn(bool)),
) {
    running_changed(|f, m, context, closed| {
        let all = closed.combine_context(context).unwrap();
        action(f, all, &|busy| {
            m.assert_running_recovery_generation_for_test(1);
            assert_eq!(m.generation(), 1);
            assert!(m.ensure_usable().is_err());
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            if busy {
                assert!(m.recovery_session().is_err());
            } else {
                drop(m.recovery_session().unwrap());
            }
        });
    });
    let f = Fixture::new();
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let all = resources(state.recovery_session().unwrap())
        .confirm_startup()
        .unwrap();
    action(&f, all, &|busy| {
        state.assert_startup_recovery_generation_for_test(1);
        assert!(state.operation(Some(1)).is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        if busy {
            assert!(state.recovery_session().is_err());
        } else {
            let s = state.recovery_session().unwrap();
            assert_eq!(s.startup_generation().unwrap(), 1);
        }
    });
}
pub(super) const BARRIERS: [&str; 11] = [
    "final-journal-file",
    "final-protection-file",
    "final-q-file",
    "final-establishment-file",
    "final-protection-directory",
    "final-backup-directory",
    "final-root-directory",
    "final-journal-barrier",
    "final-protection-barrier",
    "final-q-barrier",
    "final-establishment-barrier",
];
pub(super) fn actual(_: &str, p: SyncPoint, f: &fs::File) -> io::Result<()> {
    crate::backup::pre_restore::durability::sync_opened(p, f)
}
#[test]
fn running_and_startup_verify_all_barriers_in_order_without_releasing_guards() {
    both(|f, all, guard| {
        let before = tree(&f.root);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let hit = seen.clone();
        let verified = hooks::with(
            |_| Ok(()),
            move |name, p, file| {
                hit.borrow_mut().push(name.to_owned());
                actual(name, p, file)
            },
            || all.verify_final(),
        )
        .unwrap();
        assert_eq!(*seen.borrow(), BARRIERS);
        assert!(verified.retained().marker.is_some());
        assert_eq!(verified.retained().all.generation, 1);
        guard(true);
        assert_eq!(tree(&f.root), before);
        drop(verified);
        guard(false);
    });
}
#[test]
fn each_required_sync_error_keeps_the_complete_owner_and_receipt() {
    for point in BARRIERS {
        both(|f, all, guard| {
            let before = tree(&f.root);
            let reached = Rc::new(Cell::new(false));
            let hit = reached.clone();
            let failed = hooks::with(
                |_| Ok(()),
                move |name, p, file| {
                    if name == point {
                        hit.set(true);
                        return Err(io::Error::other("required barrier failed"));
                    }
                    actual(name, p, file)
                },
                || all.verify_final(),
            )
            .err()
            .unwrap();
            assert!(reached.get());
            assert!(
                matches!(failed.error(),FinalVerificationError::Durability{barrier,..} if *barrier==point)
            );
            assert!(failed.retained.marker.is_some());
            guard(true);
            assert_eq!(tree(&f.root), before);
            drop(failed);
            guard(false);
        });
    }
}
#[test]
fn changes_before_and_after_barriers_never_issue_a_final_proof() {
    for point in ["final-before-barriers", "final-after-barriers"] {
        for kind in 0..11 {
            both(|f, all, guard| {
                let root = f.root.clone();
                let path = f.path.clone();
                let pending = crate::backup::restore_context::pending::parse(
                    &all.held().resources().closed().view().files.evidence.bytes,
                )
                .unwrap();
                let archive = f.root.join(pending.protection_path);
                let reached = Rc::new(Cell::new(false));
                let hit = reached.clone();
                let snapshot = Rc::new(RefCell::new(None));
                let saved = snapshot.clone();
                let failed = hooks::with(
                    move |name| {
                        if name == point {
                            hit.set(true);
                            match kind {
                                0 => {
                                    use std::io::Write;
                                    fs::OpenOptions::new()
                                        .append(true)
                                        .open(&path)?
                                        .write_all(b"changed")?;
                                }
                                1 => {
                                    let bytes = fs::read(&path)?;
                                    fs::rename(&path, path.with_extension("old"))?;
                                    fs::write(&path, bytes)?;
                                }
                                2 => {
                                    use std::io::Write;
                                    fs::OpenOptions::new()
                                        .append(true)
                                        .open(&archive)?
                                        .write_all(b"changed")?;
                                }
                                3 => {
                                    use std::io::Write;
                                    fs::OpenOptions::new()
                                        .append(true)
                                        .open(root.join(".mno-restore-completion-v1.json"))?
                                        .write_all(b"changed")?;
                                }
                                4 => {
                                    fs::write(root.join(".mno-initialized-v1.json"), b"invalid")?;
                                }
                                5 => {
                                    fs::write(root.join("pending-restore.json"), b"partial")?;
                                }
                                6 => {
                                    fs::create_dir(root.join("recovery"))?;
                                }
                                7 => {
                                    fs::write(root.join("mno.sqlite-journal"), b"")?;
                                }
                                8 => {
                                    fs::write(root.join("mno.sqlite-wal"), b"")?;
                                }
                                9 => {
                                    fs::write(root.join("mno.sqlite-shm"), b"")?;
                                }
                                _ => {
                                    fs::write(root.join(".mno-completion-orphan"), b"partial")?;
                                }
                            }
                            *saved.borrow_mut() = Some(tree(&root));
                        }
                        Ok(())
                    },
                    actual,
                    || all.verify_final(),
                )
                .err()
                .unwrap_or_else(|| panic!("unexpected success at {point} kind {kind}"));
                assert!(reached.get());
                assert!(matches!(failed.error(), FinalVerificationError::Files(_)));
                guard(true);
                assert_eq!(tree(&f.root), snapshot.borrow().as_ref().unwrap().clone());
                assert!(f.root.join(".mno-restore-completion-v1.json").exists());
                // Drop cleans only known closed context staging, never recovery artifacts.
                drop(failed);
                guard(false);
            });
        }
    }
}
#[test]
fn preexisting_changed_journal_fails_before_any_barrier() {
    both(|f, all, guard| {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&f.path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        let before = tree(&f.root);
        let reached = Rc::new(Cell::new(false));
        let hit = reached.clone();
        let failed = hooks::with(
            |_| Ok(()),
            move |_, _, _| {
                hit.set(true);
                Ok(())
            },
            || all.verify_final(),
        )
        .err()
        .unwrap();
        assert!(!reached.get());
        guard(true);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        guard(false);
    });
}
#[test]
fn cancellation_before_and_after_all_barriers_keeps_q_and_reservation() {
    for point in ["final-before-barriers", "final-after-barriers"] {
        both(|f, all, guard| {
            let cancel = all.held().resources().closed().view().cancel.clone();
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failed = hooks::with(
                move |name| {
                    if name == point {
                        reached.set(true);
                        cancel.store(true, Ordering::Release);
                    }
                    Ok(())
                },
                actual,
                || all.verify_final(),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            // Cancellation is reported by the first applicable checked layer.
            match point {
                "final-before-barriers" => assert!(matches!(
                    failed.error(),
                    FinalVerificationError::Files(FileCheckError::Files(
                        classification::Reason::Cancelled
                    ))
                )),
                "final-after-barriers" => assert!(matches!(
                    failed.error(),
                    FinalVerificationError::Files(FileCheckError::Binding(
                        AllCloseError::Cancelled
                    ))
                )),
                _ => unreachable!(),
            }
            guard(true);
            assert_eq!(tree(&f.root), before);
            drop(failed);
            guard(false);
        });
    }
}
#[test]
fn panic_before_and_after_barriers_never_unlocks_or_removes_q() {
    for point in ["final-before-barriers", "final-after-barriers"] {
        both(|f, all, guard| {
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let q = fs::read(f.root.join(".mno-restore-completion-v1.json")).unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == point {
                            reached.set(true);
                            panic!("final checkpoint panic");
                        }
                        Ok(())
                    },
                    actual,
                    || all.verify_final(),
                )
            }));
            assert!(result.is_err());
            assert!(hit.get());
            guard(true);
            assert_eq!(
                fs::read(f.root.join(".mno-restore-completion-v1.json")).unwrap(),
                q
            );
            // Unwinding retains the complete closed owner: even staging remains.
            assert_eq!(tree(&f.root), before);
        });
    }
}

#[test]
fn compiler_enforces_private_consuming_final_capabilities_and_lifetimes() {
    // Actual result owners and consuming method; only the I/O backend is stubbed.
    let source = include_str!("final_verification.rs");
    let start = source.find("struct Retained<'a, 's>").unwrap();
    let declarations = source[start..]
        .split("// Verification backend.")
        .next()
        .unwrap();
    let common = format!(
        r#"
        extern crate serde;
        mod api {{
            pub struct AllConnectionsClosed<'a, 's>(pub &'a (), pub &'s ());
            pub struct RetainedMarker;
            #[derive(Debug)] pub struct FileCheckError;
            pub mod final_check {{ use super::*; use std::io;
                {declarations}
                impl Retained<'_, '_> {{ fn run(&mut self)->Result<(),FinalVerificationError>{{Ok(())}} }}
            }}
            pub fn closed(owner:&())->AllConnectionsClosed<'_, '_>{{AllConnectionsClosed(owner,owner)}}
        }}
        use api::{{AllConnectionsClosed,final_check::{{FinalFilesVerified,FinalVerificationFailure}}}};
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
        "fn valid<'a, 's>(all:AllConnectionsClosed<'a, 's>){match all.verify_final(){Ok(proof)=>drop(proof),Err(failure)=>drop(failure)}}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, reason) in [
        (
            "fn fake()->FinalFilesVerified<'static, 'static>{FinalFilesVerified{}}",
            "private",
        ),
        (
            "fn fake()->FinalVerificationFailure<'static, 'static>{FinalVerificationFailure{}}",
            "private",
        ),
        (
            "fn clone(p:FinalFilesVerified<'_, '_>){p.clone();}",
            "E0599",
        ),
        (
            "fn clone(p:FinalVerificationFailure<'_, '_>){p.clone();}",
            "E0599",
        ),
        (
            "fn copy<'a, 's>(){fn bound<T:Copy>(){} bound::<FinalFilesVerified<'a, 's>>();}",
            "E0277",
        ),
        (
            "fn serde<'a, 's>(){fn bound<T:serde::de::DeserializeOwned>(){} bound::<FinalFilesVerified<'a, 's>>();}",
            "E0277",
        ),
        (
            "fn twice(a:AllConnectionsClosed<'_, '_>){let _=a.verify_final();let _=a.verify_final();}",
            "E0382",
        ),
        (
            "fn consume(_:FinalFilesVerified<'_, '_>){} fn twice(p:FinalFilesVerified<'_, '_>){consume(p);consume(p);}",
            "E0382",
        ),
        (
            "fn escape(owner:&())->FinalFilesVerified<'static, 'static>{api::closed(owner).verify_final().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let p=api::closed(&owner).verify_final().unwrap();drop(owner);drop(p);}",
            "E0505",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let p=api::closed(&owner).verify_final().err().unwrap();drop(owner);drop(p);}",
            "E0505",
        ),
        (
            "fn bypass<'a,'s>(e:FinalVerificationFailure<'a, 's>)->FinalFilesVerified<'a, 's>{e.into()}",
            "E0277",
        ),
        (
            "fn ready(p:FinalFilesVerified<'_, '_>){p.unlock();}",
            "E0599",
        ),
        (
            "fn remove(p:FinalFilesVerified<'_, '_>){p.remove_receipt();}",
            "E0599",
        ),
    ] {
        let result = compile(probe);
        let errors = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && errors.contains(reason),
            "{probe}: {errors}"
        );
    }
}

#[test]
fn option_owners_drop_closed_files_and_staging_before_the_session_slot() {
    both(|f, all, guard| {
        // Observe the actual Arc-owned file allocation, not a reusable fd number.
        // This proves owner destruction, not the OS close syscall's return value.
        let active_owner =
            std::sync::Arc::downgrade(&all.held().resources().closed().view().files.active_file);
        // Inspect the real retained staging owner, not the managed data tree:
        // candidate staging may live outside the journal's data directory.
        let staging = all
            .held()
            .resources()
            .closed()
            .retained_staging_path()
            .to_path_buf();
        assert!(staging.is_dir());
        let verified = all.verify_final().unwrap();
        guard(true);
        assert!(staging.is_dir());
        let seen = Rc::new(RefCell::new(None));
        let observed = seen.clone();
        let root = f.root.clone();
        crate::database_state::recovery_session::reservation_drop_probe::with(
            move || {
                let closed = active_owner.upgrade().is_none();
                let cleaned = !staging.exists();
                let locked = crate::startup::DirectoryLease::open_existing(&root).is_err();
                *observed.borrow_mut() = Some((closed, cleaned, locked));
            },
            || drop(verified),
        );
        assert_eq!(*seen.borrow(), Some((true, true, true)));
        guard(false);
    });
}

#[test]
fn panic_after_option_owner_construction_keeps_recovery_and_cleans_closed_staging() {
    both(|f, all, guard| {
        let q_path = f.root.join(".mno-restore-completion-v1.json");
        let q = fs::read(&q_path).unwrap();
        let journal = fs::read(&f.path).unwrap();
        let staging = all
            .held()
            .resources()
            .closed()
            .retained_staging_path()
            .to_path_buf();
        let verified = all.verify_final().unwrap();
        guard(true);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = verified;
            panic!("after complete Option owner construction");
        }));
        assert!(caught.is_err());
        assert!(
            !staging.exists(),
            "closed staging is cleaned during normal unwind"
        );
        guard(false);
        assert_eq!(fs::read(&q_path).unwrap(), q);
        assert_eq!(fs::read(&f.path).unwrap(), journal);
    });
}

#[test]
fn option_owner_failure_retains_staging_and_reservation_until_explicit_drop() {
    both(|f, all, guard| {
        let staging = all
            .held()
            .resources()
            .closed()
            .retained_staging_path()
            .to_path_buf();
        let before = tree(&f.root);
        let reached = Rc::new(Cell::new(false));
        let hit = reached.clone();
        let failed = hooks::with(
            |_| Ok(()),
            move |name, p, file| {
                if name == "final-journal-file" {
                    hit.set(true);
                    Err(io::Error::other("retained Option owner failure"))
                } else {
                    actual(name, p, file)
                }
            },
            || all.verify_final(),
        )
        .err()
        .unwrap();
        assert!(reached.get());
        assert!(matches!(
            failed.error(),
            FinalVerificationError::Durability {
                barrier: "final-journal-file",
                ..
            }
        ));
        assert!(staging.is_dir());
        guard(true);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        assert!(!staging.exists());
        guard(false);
        // Normal closed-staging cleanup is allowed; persistent Q/journal remain.
        assert!(f.root.join(".mno-restore-completion-v1.json").is_file());
    });
}
