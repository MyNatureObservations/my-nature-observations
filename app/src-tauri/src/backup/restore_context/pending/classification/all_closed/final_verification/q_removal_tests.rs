use super::super::super::super::hooks;
use super::super::tests::{BARRIERS, actual, both};
use super::*;
use crate::backup::restore_context::pending::classification::tests::{Fixture, tree};
use std::{cell::Cell, fs, io::Write, rc::Rc, sync::atomic::Ordering};
const Q: &str = ".mno-restore-completion-v1.json";
fn session(retained: &Retained<'_, '_>) -> uuid::Uuid {
    let view = retained.all.held().resources().closed().view();
    let classification::Access::Session(s) = view.access else {
        panic!("session")
    };
    s.session_id()
}
#[test]
fn q_removal_running_and_startup_preserve_every_owner_and_only_unlink_q() {
    both(|f, all, guard| {
        let final_proof = all.verify_final().unwrap();
        let id = session(final_proof.retained());
        let mut expected = tree(&f.root);
        let prepared = final_proof.prepare_q_removal().unwrap();
        assert_eq!(session(prepared.retained.as_ref().unwrap()), id);
        guard(true);
        assert!(f.root.join(Q).exists());
        let removed = prepared.unlink_q().unwrap();
        let retained = removed.retained.as_ref().unwrap();
        assert_eq!(session(retained), id);
        assert_eq!(retained.all.generation, 1);
        // Q evidence is still owned, although its name has disappeared.
        assert!(
            retained
                .all
                .held()
                .resources()
                .closed()
                .view()
                .files
                .evidence
                .receipt
                .is_some()
        );
        expected.remove(&f.root.join(Q));
        assert_eq!(tree(&f.root), expected);
        guard(true);
        drop(removed);
        guard(false);
    });
}
#[test]
fn q_removal_all_prebarrier_errors_retain_q_and_complete_chain() {
    for barrier in BARRIERS {
        for during_unlink in [false, true] {
            both(|f, all, guard| {
                let mut proof = Some(all.verify_final().unwrap());
                let prepared = if during_unlink {
                    Some(proof.take().unwrap().prepare_q_removal().unwrap())
                } else {
                    None
                };
                let before = tree(&f.root);
                let hit = Rc::new(Cell::new(false));
                let reached = hit.clone();
                // Both stages run the same real barrier engine. The None branch
                // needs a new proof because the previous value was consumed.
                let failure = if let Some(prepared) = prepared {
                    hooks::with(
                        |_| Ok(()),
                        move |name, p, file| {
                            if name == barrier {
                                reached.set(true);
                                return Err(io::Error::other("sync failed"));
                            }
                            actual(name, p, file)
                        },
                        || prepared.unlink_q(),
                    )
                    .err()
                    .unwrap()
                } else {
                    hooks::with(
                        |_| Ok(()),
                        move |name, p, file| {
                            if name == barrier {
                                reached.set(true);
                                return Err(io::Error::other("sync failed"));
                            }
                            actual(name, p, file)
                        },
                        || proof.take().unwrap().prepare_q_removal(),
                    )
                    .err()
                    .unwrap()
                };
                assert!(hit.get());
                assert_eq!(failure.progress(), &QRemovalProgress::QRequired);
                assert!(failure.retained.marker.is_some());
                assert_eq!(tree(&f.root), before);
                guard(true);
                drop(failure);
                guard(false);
            });
        }
    }
}
#[test]
fn q_removal_rechecks_every_artifact_before_mutation() {
    for case in [
        "missing",
        "replacement",
        "symlink",
        "hardlink",
        "q-bytes",
        "journal",
        "protection",
        "establishment",
        "sidecar",
        "temporary",
        "p",
        "recovery",
    ] {
        both(|f, all, guard| {
            let prepared = all.verify_final().unwrap().prepare_q_removal().unwrap();
            let path = f.root.join(Q);
            match case {
                "missing" => fs::remove_file(&path).unwrap(),
                "replacement" => {
                    fs::rename(&path, f.root.join("kept-q")).unwrap();
                    fs::copy(f.root.join("kept-q"), &path).unwrap();
                }
                "symlink" => {
                    fs::rename(&path, f.root.join("kept-q")).unwrap();
                    std::os::unix::fs::symlink(f.root.join("kept-q"), &path).unwrap();
                }
                "hardlink" => fs::hard_link(&path, f.root.join("q-alias")).unwrap(),
                "q-bytes" => {
                    let mut bytes = fs::read(&path).unwrap();
                    bytes.push(b' ');
                    fs::write(&path, bytes).unwrap();
                }
                "journal" => fs::OpenOptions::new()
                    .append(true)
                    .open(&f.path)
                    .unwrap()
                    .write_all(b"changed")
                    .unwrap(),
                "protection" => fs::OpenOptions::new()
                    .append(true)
                    .open(
                        f.root.join(
                            crate::backup::restore_context::pending::parse(
                                &prepared
                                    .retained
                                    .as_ref()
                                    .unwrap()
                                    .all
                                    .held()
                                    .resources()
                                    .closed()
                                    .view()
                                    .files
                                    .evidence
                                    .bytes,
                            )
                            .unwrap()
                            .protection_path,
                        ),
                    )
                    .unwrap()
                    .write_all(b"changed")
                    .unwrap(),
                "establishment" => {
                    fs::write(f.root.join(".mno-initialized-v1.json"), b"{}").unwrap()
                }
                "sidecar" => fs::write(f.root.join("mno.sqlite-wal"), b"").unwrap(),
                "temporary" => fs::write(f.root.join(".MNO-COMPLETION-orphan"), b"").unwrap(),
                "p" => fs::write(f.root.join("pending-restore.json"), b"unknown").unwrap(),
                _ => fs::create_dir(f.root.join("recovery")).unwrap(),
            }
            let changed = tree(&f.root);
            let failure = prepared
                .unlink_q()
                .err()
                .unwrap_or_else(|| panic!("accepted changed {case}"));
            assert_eq!(failure.progress(), &QRemovalProgress::QRequired, "{case}");
            assert_eq!(tree(&f.root), changed, "{case}");
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn q_removal_errors_cancellation_and_late_failure_report_actual_disk_progress() {
    for target in [
        "q-prepare-before-verification",
        "q-prepare-after-barriers",
        "q-before-unlink",
        "q-unlink-syscall",
        "q-after-unlink",
    ] {
        both(|f, all, guard| {
            let proof = all.verify_final().unwrap();
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        return Err(io::Error::other("checkpoint failure"));
                    }
                    Ok(())
                },
                actual,
                || proof.prepare_q_removal().and_then(|p| p.unlink_q()),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            let unlinked = target == "q-after-unlink";
            assert_eq!(
                failure.progress(),
                if unlinked {
                    &QRemovalProgress::Unlinked
                } else {
                    &QRemovalProgress::QRequired
                }
            );
            let mut expected = before;
            if unlinked {
                expected.remove(&f.root.join(Q));
            }
            assert_eq!(tree(&f.root), expected);
            let closed = failure.retained.all.held().resources().closed();
            assert!(closed.retained_staging_path().exists());
            assert!(closed.view().files.evidence.receipt.is_some());
            guard(true);
            drop(failure);
            guard(false);
        });
    }
    for target in ["q-unlink-syscall", "q-after-unlink"] {
        both(|f, all, guard| {
            let proof = all.verify_final().unwrap();
            let cancel = proof
                .retained()
                .all
                .held()
                .resources()
                .closed()
                .view()
                .cancel
                .clone();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        cancel.store(true, Ordering::Release);
                    }
                    Ok(())
                },
                actual,
                || proof.prepare_q_removal().and_then(|p| p.unlink_q()),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            assert!(matches!(failure.error(), QRemovalError::Cancelled));
            assert_eq!(f.root.join(Q).exists(), target != "q-after-unlink");
            assert_eq!(
                failure.progress(),
                if target == "q-after-unlink" {
                    &QRemovalProgress::Unlinked
                } else {
                    &QRemovalProgress::QRequired
                }
            );
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn q_removal_panic_retains_whole_chain_at_each_transfer_boundary() {
    for target in [
        "q-prepare-before-verification",
        "q-prepare-after-barriers",
        "q-before-unlink",
        "q-after-unlink",
    ] {
        both(|f, all, guard| {
            let proof = all.verify_final().unwrap();
            let staging = proof
                .retained()
                .all
                .held()
                .resources()
                .closed()
                .retained_staging_path()
                .to_path_buf();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == target {
                            reached.set(true);
                            panic!("transfer boundary");
                        }
                        Ok(())
                    },
                    actual,
                    || proof.prepare_q_removal().and_then(|p| p.unlink_q()),
                )
            }));
            assert!(outcome.is_err());
            assert!(hit.get());
            assert!(staging.exists());
            assert_eq!(f.root.join(Q).exists(), target != "q-after-unlink");
            // Same existing conservative unwind guard retains the slot and files.
            guard(true);
        });
    }
}

#[test]
fn q_removal_syscall_error_is_not_reported_as_success_or_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    both(|f, all, guard| {
        let prepared = all.verify_final().unwrap().prepare_q_removal().unwrap();
        let root = f.root.clone();
        let original = fs::metadata(&root).unwrap().permissions();
        // Actual unlinkat fails deterministically for this unprivileged test
        // process. Restore permissions before any assertions or fixture cleanup.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let hit = Rc::new(Cell::new(false));
        let reached = hit.clone();
        let result = hooks::with(
            move |name| {
                if name == "q-unlink-syscall" {
                    reached.set(true);
                    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
                }
                Ok(())
            },
            actual,
            || prepared.unlink_q(),
        );
        fs::set_permissions(&f.root, original).unwrap();
        assert!(hit.get());
        let failure = result
            .err()
            .expect("directory permission must deny unlinkat");
        assert_eq!(failure.progress(), &QRemovalProgress::UnlinkAttempted);
        assert!(
            matches!(failure.error(), QRemovalError::Unlink(e) if e.raw_os_error()==Some(libc::EACCES))
        );
        assert!(f.root.join(Q).exists());
        guard(true);
        drop(failure);
        guard(false);
    });
}

#[test]
fn q_removal_crash_child() {
    use super::super::super::tests::resources;
    use crate::database_state::DatabaseState;
    let Ok(root) = std::env::var("MNO_Q_REMOVAL_ROOT") else {
        return;
    };
    let target = std::env::var("MNO_Q_REMOVAL_POINT").unwrap();
    let state = DatabaseState::default();
    assert!(
        state
            .initialize_legacy_startup(std::path::Path::new(&root))
            .is_err()
    );
    let proof = resources(state.recovery_session().unwrap())
        .confirm_startup()
        .unwrap()
        .verify_final()
        .unwrap();
    let _ = hooks::with(
        move |name| {
            if name == target {
                println!("q-removal-crash-ready");
                std::io::stdout().flush().unwrap();
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
        actual,
        || proof.prepare_q_removal().and_then(|p| p.unlink_q()),
    );
    panic!("crash boundary not reached");
}

#[test]
fn q_removal_process_crashes_have_explicit_startup_behavior() {
    use crate::database_state::DatabaseState;
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    for target in ["q-before-unlink", "q-after-unlink"] {
        let f = Fixture::new();
        let mut child=Command::new(std::env::current_exe().unwrap())
            .args(["--exact","backup::restore_context::pending::classification::result::receipt::all_closed::final_verification::q_removal::tests::q_removal_crash_child","--nocapture"])
            .env("MNO_Q_REMOVAL_ROOT",&f.root).env("MNO_Q_REMOVAL_POINT",target)
            .stdout(Stdio::piped()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.unwrap().contains("q-removal-crash-ready") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        let reached = rx.recv_timeout(Duration::from_secs(30));
        // Always kill/reap before asserting, including timeout/error paths.
        let _ = child.kill();
        child.wait().unwrap();
        reader.join().unwrap();
        reached.expect("child checkpoint timeout");
        let present = target == "q-before-unlink";
        assert_eq!(f.root.join(Q).exists(), present);
        assert!(!f.root.join("recovery").exists());
        let state = DatabaseState::default();
        let startup = state.initialize_legacy_startup(&f.root);
        assert_eq!(startup.is_err(), present);
        if present {
            assert!(state.operation(Some(1)).is_err());
        } else {
            assert!(state.operation(Some(1)).is_ok());
        }
        // Process-kill exercises visibility after close, not power-loss durability.
    }
}

#[test]
fn q_removal_compiler_requires_private_consuming_capabilities_and_borrowed_guards() {
    let source = include_str!("q_removal.rs");
    let declarations = source.split("// Removal backend.").next().unwrap();
    let common = format!(
        r#"
        extern crate serde;
        mod api {{
            use std::io;
            struct Retained<'a,'s>(&'a (), &'s ());
            struct DuringVerification<'a,'s>(Option<Retained<'a,'s>>);
            pub struct FinalFilesVerified<'a,'s>{{retained:Option<Retained<'a,'s>>}}
            impl Drop for FinalFilesVerified<'_,'_>{{fn drop(&mut self){{}}}}
            #[derive(Debug)] struct FinalVerificationError;
            pub mod q {{
                {declarations}
                fn prepare_retained(_: &mut Retained<'_,'_>)->Result<(),QRemovalError>{{Ok(())}}
                fn unlink_retained(_: &mut Retained<'_,'_>,_: &mut QRemovalProgress)->Result<(),QRemovalError>{{Ok(())}}
            }}
            pub fn final_proof<'a,'s>(m:&'a (),s:&'s ())->FinalFilesVerified<'a,'s>{{FinalFilesVerified{{retained:Some(Retained(m,s))}}}}
        }}
    "#
    );
    let temp = tempfile::TempDir::new().unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let serde = fs::read_dir(deps)
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
        let path = temp.path().join("probe.rs");
        fs::write(&path, format!("{common}\n{probe}")).unwrap();
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-type=lib"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("serde={}", serde.display()))
            .arg("-L")
            .arg(format!("dependency={}", serde.parent().unwrap().display()))
            .arg("--out-dir")
            .arg(temp.path())
            .output()
            .unwrap()
    };
    let positive = compile(
        "fn valid(m:&(),s:&()){let p=api::final_proof(m,s).prepare_q_removal().unwrap();let _=p.unlink_q().unwrap();}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (probe, reason) in [
        (
            "fn fake()->api::q::QRemovalPrepared<'static,'static>{api::q::QRemovalPrepared{retained:None}}",
            "private",
        ),
        (
            "fn fake()->api::q::QUnlinked<'static,'static>{api::q::QUnlinked{retained:None}}",
            "private",
        ),
        (
            "fn clone(p:api::q::QRemovalPrepared<'_,'_>){p.clone();}",
            "E0599",
        ),
        ("fn clone(p:api::q::QUnlinked<'_,'_>){p.clone();}", "E0599"),
        (
            "fn twice(p:api::q::QRemovalPrepared<'_,'_>){let _=p.unlink_q();let _=p.unlink_q();}",
            "E0382",
        ),
        (
            "fn twice(p:api::q::QUnlinked<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "fn bypass(f:api::q::QRemovalFailure<'_,'_>){f.unlink_q();}",
            "E0599",
        ),
        (
            "fn escape<'a,'s>(p:api::q::QUnlinked<'a,'s>)->api::q::QUnlinked<'static,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "fn serde<T:serde::Serialize>(){} fn fake(){serde::<api::q::QUnlinked<'static,'static>>();}",
            "E0277",
        ),
        (
            "fn serde<T:serde::de::DeserializeOwned>(){} fn fake(){serde::<api::q::QRemovalPrepared<'static,'static>>();}",
            "E0277",
        ),
        ("fn ready(p:api::q::QUnlinked<'_,'_>){p.ready();}", "E0599"),
    ] {
        let output = compile(probe);
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && diagnostic.contains(reason),
            "{probe}: {diagnostic}"
        );
    }
}

#[test]
fn q_removal_last_syscall_boundary_rechecks_late_changes() {
    for case in ["replacement", "bytes", "hardlink", "sidecar"] {
        both(|f, all, guard| {
            let prepared = all.verify_final().unwrap().prepare_q_removal().unwrap();
            let root = f.root.clone();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let changed = Rc::new(std::cell::RefCell::new(None));
            let captured = changed.clone();
            let failure = hooks::with(
                move |name| {
                    if name == "q-unlink-syscall" {
                        reached.set(true);
                        let q = root.join(Q);
                        match case {
                            "replacement" => {
                                fs::rename(&q, root.join("kept-q"))?;
                                fs::copy(root.join("kept-q"), &q)?;
                            }
                            "bytes" => {
                                let mut bytes = fs::read(&q)?;
                                bytes.push(b' ');
                                fs::write(&q, bytes)?;
                            }
                            "hardlink" => fs::hard_link(&q, root.join("q-alias"))?,
                            _ => fs::write(root.join("mno.sqlite-shm"), b"")?,
                        }
                        *captured.borrow_mut() = Some(tree(&root));
                    }
                    Ok(())
                },
                actual,
                || prepared.unlink_q(),
            )
            .err()
            .expect("late change must reject unlink");
            assert!(hit.get());
            assert_eq!(failure.progress(), &QRemovalProgress::QRequired);
            assert_eq!(&tree(&f.root), changed.borrow().as_ref().unwrap());
            assert!(f.root.join(Q).exists());
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}

#[test]
fn q_removal_panic_inside_real_sync_callback_retains_guards_and_files() {
    use super::super::super::tests::{resources, running_changed};
    use crate::database_state::DatabaseState;
    use std::{cell::RefCell, os::unix::fs::MetadataExt, sync::Arc};

    fn exercise(f: &Fixture, all: AllConnectionsClosed<'_, '_>, blocked: &dyn Fn()) {
        let proof = all.verify_final().unwrap();
        let closed = proof.retained().all.held().resources().closed();
        let view = closed.view();
        // Weak observation does not keep the file alive independently of the
        // aggregate under test, and cannot confuse recycled descriptor numbers.
        let active = Arc::downgrade(&view.files.active_file);
        let observed = active.clone();
        let metadata = view.files.active_file.metadata().unwrap();
        let identity = (metadata.dev(), metadata.ino());
        let staging = closed.retained_staging_path().to_path_buf();
        let before = tree(&f.root);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let callbacks = seen.clone();
        let reached = Rc::new(Cell::new(false));
        let hit = reached.clone();
        assert!(staging.is_dir());

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                |_| Ok(()),
                move |name, point, file| {
                    // Run the actual durability function, not a pre-call event.
                    actual(name, point, file)?;
                    callbacks.borrow_mut().push(name.to_owned());
                    if name == "final-journal-barrier" {
                        assert_eq!(point, SyncPoint::FinalFileBarrier);
                        let retained = observed.upgrade().expect("original file owner");
                        assert!(std::ptr::eq(file, retained.as_ref()));
                        let metadata = file.metadata()?;
                        assert_eq!((metadata.dev(), metadata.ino()), identity);
                        hit.set(true);
                        panic!("q preparation sync callback panic");
                    }
                    Ok(())
                },
                || proof.prepare_q_removal(),
            )
        }));
        let payload = outcome
            .err()
            .expect("sync callback must unwind preparation");
        assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("q preparation sync callback panic")
        );
        assert!(reached.get());
        assert_eq!(&*seen.borrow(), &BARRIERS[..8]);
        // The unwind guard retained the original aggregate, including its
        // operation bindings and reserved session; no prepared capability exists.
        let retained = active.upgrade().expect("file owner retained after unwind");
        let after = retained.metadata().unwrap();
        assert_eq!((after.dev(), after.ino()), identity);
        assert!(staging.is_dir());
        assert_eq!(tree(&f.root), before); // Q, journal and protection untouched.
        assert_eq!(
            crate::startup::DirectoryLease::open_existing(&f.root)
                .err()
                .as_deref(),
            Some("database_directory_in_use")
        );
        blocked();
    }

    running_changed(|f, maintenance, context, closed| {
        exercise(f, closed.combine_context(context).unwrap(), &|| {
            assert_eq!(
                maintenance.recovery_session().err().as_deref(),
                Some("database_recovery_busy")
            );
            assert!(maintenance.ensure_usable().is_err());
            maintenance.assert_running_recovery_generation_for_test(1);
        });
    });
    let f = Fixture::new();
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let all = resources(state.recovery_session().unwrap())
        .confirm_startup()
        .unwrap();
    exercise(&f, all, &|| {
        assert_eq!(
            state.recovery_session().err().as_deref(),
            Some("database_recovery_busy")
        );
        assert!(state.operation(Some(1)).is_err());
        assert!(state.operation(Some(2)).is_err());
        state.assert_startup_recovery_generation_for_test(1);
    });
}
