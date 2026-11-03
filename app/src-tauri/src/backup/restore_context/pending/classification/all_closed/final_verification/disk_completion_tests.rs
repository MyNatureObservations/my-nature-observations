use super::super::super::super::super::hooks;
pub(super) use super::super::super::tests::{actual, both};
use super::*;
use crate::backup::restore_context::pending::classification::tests::{Fixture, tree};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::Write,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
};
const Q: &str = ".mno-restore-completion-v1.json";
const BARRIERS: [&str; 4] = [
    "disk-root-directory",
    "disk-journal-final",
    "disk-protection-final",
    "disk-establishment-final",
];
pub(super) fn unlinked<'a, 's>(all: AllConnectionsClosed<'a, 's>) -> QUnlinked<'a, 's> {
    all.verify_final()
        .unwrap()
        .prepare_q_removal()
        .unwrap()
        .unlink_q()
        .unwrap()
}
pub(super) fn snapshot(q: &QUnlinked<'_, '_>) -> (uuid::Uuid, *const (), uuid::Uuid, u32) {
    let (all, _) = q.post_unlink_parts();
    let view = all.held().resources().closed().view();
    let classification::Access::Session(s) = view.access else {
        panic!("session")
    };
    (
        s.session_id(),
        s.test_reservation_slot(),
        view.binding.restore_id,
        all.generation,
    )
}
#[test]
fn disk_completion_running_startup_use_only_linked_barriers_and_preserve_guards() {
    use std::os::unix::fs::MetadataExt;
    both(|f, all, guard| {
        let q = unlinked(all);
        let binding = snapshot(&q);
        let (owner, marker) = q.post_unlink_parts();
        let closed = owner.held().resources().closed();
        let view = closed.view();
        let expected = [
            &view.files.root_file,
            &*view.files.active_file,
            &view.files.protection_file,
            marker.retained_file(),
        ]
        .map(|file| {
            let m = file.metadata().unwrap();
            (m.dev(), m.ino())
        });
        let staging = closed.retained_staging_path().to_path_buf();
        let active = Arc::downgrade(&view.files.active_file);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let names = seen.clone();
        let before = tree(&f.root);
        let completed = hooks::with(
            |_| Ok(()),
            move |name, point, file| {
                let index = names.borrow().len();
                assert_eq!(name, BARRIERS[index]);
                assert_eq!(
                    point,
                    if index == 0 {
                        SyncPoint::Directory
                    } else {
                        SyncPoint::FinalFileBarrier
                    }
                );
                let m = file.metadata()?;
                assert_eq!((m.dev(), m.ino()), expected[index]);
                actual(name, point, file)?;
                names.borrow_mut().push(name.to_owned());
                Ok(())
            },
            || q.complete_disk(),
        )
        .unwrap();
        assert_eq!(&*seen.borrow(), &BARRIERS);
        assert_eq!(snapshot(completed.unlinked.as_ref().unwrap()), binding);
        assert!(staging.exists());
        assert!(active.upgrade().is_some());
        assert_eq!(tree(&f.root), before);
        assert!(!f.root.join(Q).exists());
        guard(true);
        drop(completed);
        assert!(!staging.exists());
        assert!(active.upgrade().is_none());
        guard(false);
    });
}
#[test]
fn disk_completion_each_barrier_error_reports_confirmed_progress_and_retains_chain() {
    for (index, target) in BARRIERS.into_iter().enumerate() {
        both(|f, all, guard| {
            let q = unlinked(all);
            let binding = snapshot(&q);
            let before = tree(&f.root);
            let staging = q
                .post_unlink_parts()
                .0
                .held()
                .resources()
                .closed()
                .retained_staging_path()
                .to_path_buf();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                |_| Ok(()),
                move |name, p, file| {
                    if name == target {
                        reached.set(true);
                        return Err(io::Error::other("required sync failed"));
                    }
                    actual(name, p, file)
                },
                || q.complete_disk(),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            assert!(
                matches!(failure.error(),DiskCompletionError::Barrier{name,..} if *name==target)
            );
            assert_eq!(
                failure.progress(),
                if index == 0 {
                    &DiskCompletionProgress::Unlinked
                } else {
                    &DiskCompletionProgress::DirectorySynced
                }
            );
            assert_eq!(snapshot(&failure.unlinked), binding);
            assert!(staging.exists());
            assert_eq!(tree(&f.root), before);
            assert!(!f.root.join(Q).exists());
            guard(true);
            drop(failure);
            assert!(!staging.exists());
            guard(false);
        });
    }
}
#[test]
fn disk_completion_checkpoint_errors_and_cancellation_never_regress_progress() {
    for (target, expected) in [
        ("disk-before-verification", DiskCompletionProgress::Unlinked),
        (
            "disk-before-directory-sync",
            DiskCompletionProgress::Unlinked,
        ),
        (
            "disk-after-directory-sync",
            DiskCompletionProgress::DirectorySynced,
        ),
        (
            "disk-after-final-barriers",
            DiskCompletionProgress::FinalBarriersSynced,
        ),
        (
            "disk-before-final-verification",
            DiskCompletionProgress::FinalBarriersSynced,
        ),
        (
            "disk-after-final-verification",
            DiskCompletionProgress::FilesVerified,
        ),
    ] {
        both(|f, all, guard| {
            let q = unlinked(all);
            let binding = snapshot(&q);
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        return Err(io::Error::other("checkpoint error"));
                    }
                    Ok(())
                },
                actual,
                || q.complete_disk(),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            assert_eq!(failure.progress(), &expected);
            assert_eq!(snapshot(&failure.unlinked), binding);
            assert_eq!(tree(&f.root), before);
            guard(true);
            drop(failure);
            guard(false);
        });
    }
    for (target, expected) in [
        ("disk-before-verification", DiskCompletionProgress::Unlinked),
        (
            "disk-after-directory-sync",
            DiskCompletionProgress::DirectorySynced,
        ),
        (
            "disk-after-final-barriers",
            DiskCompletionProgress::FinalBarriersSynced,
        ),
        (
            "disk-after-final-verification",
            DiskCompletionProgress::FilesVerified,
        ),
    ] {
        both(|f, all, guard| {
            let q = unlinked(all);
            let before = tree(&f.root);
            let cancel = q
                .post_unlink_parts()
                .0
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
                || q.complete_disk(),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            assert_eq!(failure.progress(), &expected);
            assert!(matches!(
                failure.error(),
                DiskCompletionError::Files(FileCheckError::Files(
                    classification::Reason::Cancelled
                )) | DiskCompletionError::Files(FileCheckError::Binding(AllCloseError::Cancelled))
            ));
            assert_eq!(tree(&f.root), before);
            assert!(!f.root.join(Q).exists());
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}
#[test]
fn disk_completion_rejects_changed_files_and_returned_artifacts_at_final_boundaries() {
    for target in [
        "disk-before-directory-sync",
        "disk-after-final-barriers",
        "disk-after-final-verification",
    ] {
        for case in [
            "q",
            "q-alias",
            "p",
            "recovery",
            "sidecar",
            "temporary",
            "journal-bytes",
            "journal-inode",
            "protection-bytes",
            "protection-inode",
            "marker-bytes",
            "marker-inode",
            "marker-missing",
        ] {
            both(|f, all, guard| {
                let q = unlinked(all);
                let data = crate::backup::restore_context::pending::parse(
                    &q.post_unlink_parts()
                        .0
                        .held()
                        .resources()
                        .closed()
                        .view()
                        .files
                        .evidence
                        .bytes,
                )
                .unwrap();
                let protection = f.root.join(data.protection_path);
                let root = f.root.clone();
                let hit = Rc::new(Cell::new(false));
                let reached = hit.clone();
                let changed = Rc::new(RefCell::new(None));
                let captured = changed.clone();
                let failure = hooks::with(
                    move |name| {
                        if name == target {
                            reached.set(true);
                            match case {
                                "q" => fs::write(root.join(Q), b"returned")?,
                                "q-alias" => fs::write(
                                    root.join(".MNO-RESTORE-COMPLETION-v1.json"),
                                    b"returned",
                                )?,
                                "p" => fs::write(root.join("pending-restore.json"), b"returned")?,
                                "recovery" => fs::create_dir(root.join("recovery"))?,
                                "sidecar" => fs::write(root.join("mno.sqlite-shm"), b"")?,
                                "temporary" => fs::write(root.join(".MNO-COMPLETION-orphan"), b"")?,
                                "journal-bytes" => fs::OpenOptions::new()
                                    .append(true)
                                    .open(root.join("mno.sqlite"))?
                                    .write_all(b"changed")?,
                                "protection-bytes" => fs::OpenOptions::new()
                                    .append(true)
                                    .open(&protection)?
                                    .write_all(b"changed")?,
                                "marker-bytes" => {
                                    fs::write(root.join(".mno-initialized-v1.json"), b"{}")?
                                }
                                "marker-missing" => {
                                    fs::remove_file(root.join(".mno-initialized-v1.json"))?
                                }
                                _ => {
                                    let path = match case {
                                        "journal-inode" => root.join("mno.sqlite"),
                                        "protection-inode" => protection.clone(),
                                        _ => root.join(".mno-initialized-v1.json"),
                                    };
                                    let bytes = fs::read(&path)?;
                                    fs::rename(&path, root.join("kept-original"))?;
                                    fs::write(&path, bytes)?;
                                }
                            }
                            *captured.borrow_mut() = Some(tree(&root));
                        }
                        Ok(())
                    },
                    actual,
                    || q.complete_disk(),
                )
                .err()
                .unwrap_or_else(|| panic!("accepted {case} at {target}"));
                assert!(hit.get());
                assert_eq!(
                    failure.progress(),
                    match target {
                        "disk-before-directory-sync" => &DiskCompletionProgress::Unlinked,
                        "disk-after-final-barriers" => &DiskCompletionProgress::FinalBarriersSynced,
                        _ => &DiskCompletionProgress::FilesVerified,
                    }
                );
                assert_eq!(&tree(&f.root), changed.borrow().as_ref().unwrap());
                guard(true);
                drop(failure);
                guard(false);
            });
        }
    }
}
#[test]
fn disk_completion_receipt_evidence_cannot_be_mixed_between_operations() {
    use super::super::super::super::tests::resources;
    use crate::database_state::DatabaseState;
    let first = Fixture::new();
    let second = Fixture::new();
    let a = DatabaseState::default();
    let b = DatabaseState::default();
    assert!(a.initialize_legacy_startup(&first.root).is_err());
    assert!(b.initialize_legacy_startup(&second.root).is_err());
    let qa = unlinked(
        resources(a.recovery_session().unwrap())
            .confirm_startup()
            .unwrap(),
    );
    let qb = unlinked(
        resources(b.recovery_session().unwrap())
            .confirm_startup()
            .unwrap(),
    );
    let receipt = qb
        .post_unlink_parts()
        .0
        .held()
        .resources()
        .closed()
        .view()
        .files
        .evidence
        .receipt
        .as_ref()
        .unwrap();
    assert!(qa.require_receipt(receipt).is_err());
    assert!(receipt.check_after_unlink(&second.root, &qa).is_err());
    receipt.check_after_unlink(&second.root, &qb).unwrap();
    file_checks::check_unlinked(&qa).unwrap();
    file_checks::check_unlinked(&qb).unwrap();
    assert_ne!(snapshot(&qa).0, snapshot(&qb).0);
}
#[test]
fn disk_completion_detects_changes_to_the_unlinked_retained_receipt_itself() {
    use std::os::unix::fs::FileExt;
    both(|f, all, guard| {
        // A test-only writable handle to the existing Q survives its unlink.
        let writer = fs::OpenOptions::new()
            .write(true)
            .open(f.root.join(Q))
            .unwrap();
        let q = unlinked(all);
        writer.write_at(b"!", 0).unwrap();
        let failure = q.complete_disk().err().unwrap();
        assert_eq!(failure.progress(), &DiskCompletionProgress::Unlinked);
        assert!(!f.root.join(Q).exists());
        guard(true);
        drop(failure);
        guard(false);
    });
}
#[test]
fn disk_completion_panics_keep_reservation_and_closed_staging() {
    for target in [
        "disk-before-directory-sync",
        "disk-after-directory-sync",
        "disk-journal-final",
        "disk-after-final-barriers",
        "disk-after-final-verification",
    ] {
        both(|f, all, guard| {
            let q = unlinked(all);
            let (all, _) = q.post_unlink_parts();
            let closed = all.held().resources().closed();
            let staging = closed.retained_staging_path().to_path_buf();
            let active = Arc::downgrade(&closed.view().files.active_file);
            let before = tree(&f.root);
            let hit = Rc::new(Cell::new(false));
            let event_hit = hit.clone();
            let sync_hit = hit.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == target {
                            event_hit.set(true);
                            panic!("disk checkpoint panic");
                        }
                        Ok(())
                    },
                    move |name, p, file| {
                        actual(name, p, file)?;
                        if name == target {
                            sync_hit.set(true);
                            panic!("disk sync panic");
                        }
                        Ok(())
                    },
                    || q.complete_disk(),
                )
            }));
            assert!(outcome.is_err());
            assert!(hit.get());
            assert!(staging.exists());
            assert!(active.upgrade().is_some());
            assert_eq!(tree(&f.root), before);
            guard(true);
        });
    }
}

#[test]
fn disk_completion_rejects_root_directory_replacement_without_touching_replacement() {
    for target in ["disk-before-directory-sync", "disk-after-directory-sync"] {
        both(|f, all, guard| {
            let q = unlinked(all);
            let before = tree(&f.root);
            let root = f.root.clone();
            let moved = root.with_extension("kept-root");
            let kept = moved.clone();
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let failure = hooks::with(
                move |name| {
                    if name == target {
                        reached.set(true);
                        fs::rename(&root, &moved)?;
                        fs::create_dir(&root)?;
                    }
                    Ok(())
                },
                actual,
                || q.complete_disk(),
            )
            .err()
            .unwrap();
            assert!(hit.get());
            assert_eq!(fs::read_dir(&f.root).unwrap().count(), 0);
            fs::remove_dir(&f.root).unwrap();
            fs::rename(kept, &f.root).unwrap();
            assert_eq!(tree(&f.root), before);
            assert!(!f.root.join(Q).exists());
            assert_eq!(
                failure.progress(),
                if target == "disk-before-directory-sync" {
                    &DiskCompletionProgress::Unlinked
                } else {
                    &DiskCompletionProgress::FinalBarriersSynced
                }
            );
            guard(true);
            drop(failure);
            guard(false);
        });
    }
}

#[test]
fn disk_completion_crash_child() {
    use super::super::super::super::tests::resources;
    use crate::database_state::DatabaseState;
    let Ok(root) = std::env::var("MNO_DISK_COMPLETION_ROOT") else {
        return;
    };
    let target = std::env::var("MNO_DISK_COMPLETION_POINT").unwrap();
    fn ready_and_wait() -> ! {
        println!("disk-completion-crash-ready");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
    let state = DatabaseState::default();
    assert!(
        state
            .initialize_legacy_startup(std::path::Path::new(&root))
            .is_err()
    );
    let q = unlinked(
        resources(state.recovery_session().unwrap())
            .confirm_startup()
            .unwrap(),
    );
    let sync_target = target.clone();
    let completed = hooks::with(
        move |name| {
            if name == target {
                ready_and_wait();
            }
            Ok(())
        },
        move |name, point, file| {
            actual(name, point, file)?;
            if name == sync_target {
                ready_and_wait();
            }
            Ok(())
        },
        || q.complete_disk(),
    )
    .unwrap();
    // This is after the real success owner was constructed, while still Recovery.
    state.assert_startup_recovery_generation_for_test(1);
    assert_eq!(
        state.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    std::hint::black_box(&completed);
    ready_and_wait();
}
#[test]
fn disk_completion_subprocess_crashes_keep_startup_conservative_with_or_without_q() {
    use crate::{backup::restore_context::pending::completion, database_state::DatabaseState};
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    for target in [
        "disk-before-directory-sync",
        "disk-after-directory-sync",
        "disk-protection-final",
        "disk-after-final-barriers",
        "disk-before-final-verification",
        "completed",
    ] {
        let f = Fixture::new();
        let journal = fs::read(&f.path).unwrap();
        let protection = fs::read(&f.protection).unwrap();
        let pending = fs::read(f.root.join("recovery/pending-restore.json")).unwrap();
        let q_bytes = completion::receipt_bytes(&pending, completion::Outcome::Old).unwrap();
        let mut child=Command::new(std::env::current_exe().unwrap())
            .args(["--exact","backup::restore_context::pending::classification::result::receipt::all_closed::final_verification::q_removal::disk_completion::tests::disk_completion_crash_child","--nocapture"])
            .env("MNO_DISK_COMPLETION_ROOT",&f.root).env("MNO_DISK_COMPLETION_POINT",target).stdout(Stdio::piped()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.unwrap().contains("disk-completion-crash-ready") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        let reached = rx.recv_timeout(Duration::from_secs(30));
        let _ = child.kill();
        child.wait().unwrap();
        reader.join().unwrap();
        reached.expect("disk-completion child checkpoint timeout");
        assert!(!f.root.join(Q).exists());
        assert!(!f.root.join("recovery").exists());
        assert_eq!(fs::read(&f.path).unwrap(), journal);
        assert_eq!(fs::read(&f.protection).unwrap(), protection);
        let normal = DatabaseState::default();
        normal.initialize_legacy_startup(&f.root).unwrap();
        assert!(normal.operation(Some(1)).is_ok());
        drop(normal);
        // Independent defensive layout variant, not a claim that a completed
        // directory/file barrier can actually replay Q after a power failure.
        // Only the test writes Q; production never recreates it.
        fs::write(f.root.join(Q), &q_bytes).unwrap();
        let blocked = DatabaseState::default();
        assert!(blocked.initialize_legacy_startup(&f.root).is_err());
        blocked.assert_startup_recovery_generation_for_test(1);
        assert!(blocked.operation(Some(1)).is_err());
        assert_eq!(fs::read(f.root.join(Q)).unwrap(), q_bytes);
    }
}

#[test]
fn disk_completion_compiler_enforces_private_consuming_closed_capability() {
    let source = include_str!("disk_completion.rs");
    let declarations = source.split("// Completion backend.").next().unwrap();
    let common = format!(
        r#"
        extern crate serde;
        mod api {{
            use std::io;
            #[derive(Debug)] struct FileCheckError;
            pub struct QUnlinked<'a,'s>(&'a (), &'s ());
            impl Drop for QUnlinked<'_,'_>{{fn drop(&mut self){{}}}}
            pub mod disk {{
                {declarations}
                fn complete(_: &QUnlinked<'_,'_>,p:&mut DiskCompletionProgress)->Result<(),DiskCompletionError>{{*p=DiskCompletionProgress::FilesVerified;Ok(())}}
            }}
            pub fn unlinked<'a,'s>(a:&'a (),s:&'s ())->QUnlinked<'a,'s>{{QUnlinked(a,s)}}
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
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-type=lib"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("serde={}", serde.display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--out-dir")
            .arg(dir.path())
            .output()
            .unwrap()
    };
    let valid = compile(
        "fn valid(a:&(),s:&()){let done=api::unlinked(a,s).complete_disk().unwrap();drop(done);}",
    );
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    for (probe, reason) in [
        (
            "fn fake(q:api::QUnlinked<'static,'static>)->api::disk::DiskRecoveryCompleted<'static,'static>{api::disk::DiskRecoveryCompleted{unlinked:Some(q)}}",
            "private",
        ),
        (
            "fn clone(p:api::disk::DiskRecoveryCompleted<'_,'_>){p.clone();}",
            "E0599",
        ),
        (
            "fn twice(p:api::QUnlinked<'_,'_>){let _=p.complete_disk();let _=p.complete_disk();}",
            "E0382",
        ),
        (
            "fn twice(p:api::disk::DiskRecoveryCompleted<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "fn fake(p:api::disk::DiskCompletionFailure<'_,'_>){p.complete_disk();}",
            "E0599",
        ),
        (
            "fn escape<'a,'s>(p:api::disk::DiskRecoveryCompleted<'a,'s>)->api::disk::DiskRecoveryCompleted<'static,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "fn bound<T:serde::Serialize>(){} fn fake(){bound::<api::disk::DiskRecoveryCompleted<'static,'static>>();}",
            "E0277",
        ),
        (
            "fn bound<T:serde::de::DeserializeOwned>(){} fn fake(){bound::<api::disk::DiskRecoveryCompleted<'static,'static>>();}",
            "E0277",
        ),
        (
            "fn fake(p:api::disk::DiskRecoveryCompleted<'_,'_>){p.ready();}",
            "E0599",
        ),
        (
            "fn fake(p:api::disk::DiskRecoveryCompleted<'_,'_>){p.advance_generation();}",
            "E0599",
        ),
        (
            "fn fake(p:api::disk::DiskRecoveryCompleted<'_,'_>){p.connection();}",
            "E0599",
        ),
    ] {
        let result = compile(probe);
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && error.contains(reason),
            "{probe}: {error}"
        );
    }
}

#[test]
fn disk_completion_completed_drop_during_unwind_cleans_only_closed_staging() {
    both(|f, all, guard| {
        let q = unlinked(all);
        let closed = q.post_unlink_parts().0.held().resources().closed();
        let staging = closed.retained_staging_path().to_path_buf();
        let active = Arc::downgrade(&closed.view().files.active_file);
        let evidence = &closed.view().files.evidence.bytes;
        let pending = crate::backup::restore_context::pending::parse(evidence).unwrap();
        let protection = f.root.join(pending.protection_path);
        let journal_before = fs::read(&f.path).unwrap();
        let protection_before = fs::read(&protection).unwrap();
        let establishment = f.root.join(".mno-initialized-v1.json");
        let establishment_before = fs::read(&establishment).unwrap();
        let done = q.complete_disk().unwrap();
        guard(true);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _done = done;
            panic!("drop completed proof");
        }));
        assert!(caught.is_err());
        assert!(!staging.exists());
        assert!(active.upgrade().is_none());
        // Other already-closed context staging may also be cleaned by normal
        // Drop. Persistent journal, safety archive and establishment must remain.
        assert_eq!(fs::read(&f.path).unwrap(), journal_before);
        assert_eq!(fs::read(&protection).unwrap(), protection_before);
        assert_eq!(fs::read(&establishment).unwrap(), establishment_before);
        assert!(!f.root.join(Q).exists());
        guard(false);
    });
}
