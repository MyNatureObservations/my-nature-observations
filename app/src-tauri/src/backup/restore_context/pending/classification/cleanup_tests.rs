use super::*;
use crate::backup::restore_context::pending::classification::{
    self, Status,
    tests::{Fixture, flag, tree},
};
use crate::database_state::DatabaseState;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
const P: &str = "recovery/pending-restore.json";
const Q: &str = ".mno-restore-completion-v1.json";
fn state(f: &Fixture) -> DatabaseState {
    let s = DatabaseState::default();
    assert!(s.initialize_legacy_startup(&f.root).is_err());
    s
}
fn durable(s: &DatabaseState) -> ReceiptDurable<'_, '_> {
    classification::classify_session(s.recovery_session().unwrap(), flag())
        .into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap()
}
fn guarded(s: &DatabaseState, f: &Fixture) {
    assert_eq!(
        s.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(s.operation(Some(1)).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
fn expected_files(f: &Fixture, before: &std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>) {
    let mut expected = before.clone();
    expected.remove(&f.root.join(P));
    assert_eq!(tree(&f.root), expected);
}
fn shape(f: &Fixture, n: u8, outcome: &str) {
    if n != 0 {
        let bytes = fs::read(f.root.join(P)).unwrap();
        let outcome = if outcome == "old" {
            completion::Outcome::Old
        } else {
            completion::Outcome::New
        };
        fs::write(
            f.root.join(Q),
            completion::receipt_bytes(&bytes, outcome).unwrap(),
        )
        .unwrap();
        fs::remove_file(f.root.join(P)).unwrap();
        if n == 2 {
            fs::remove_dir(f.root.join("recovery")).unwrap();
        }
    }
}
#[test]
fn cleanup_all_verified_shapes_and_outcomes_keeps_q_and_snapshots() {
    for n in 0..3 {
        for outcome in ["old", "new"] {
            let f = Fixture::new();
            f.title(if outcome == "old" { "Old" } else { "New" });
            shape(&f, n, outcome);
            let s = state(&f);
            let d = durable(&s);
            let before = tree(&f.root);
            let bytes = fs::read(f.root.join(Q)).unwrap();
            let result = d.remove_pending().unwrap();
            guarded(&s, &f);
            expected_files(&f, &before);
            assert!(!f.root.join("recovery").exists());
            assert_eq!(fs::read(f.root.join(Q)).unwrap(), bytes);
            assert_eq!(
                result.receipt.proof.assessment.status(),
                if outcome == "old" {
                    Status::Old
                } else {
                    Status::New
                }
            );
            let writer = rusqlite::Connection::open(&f.path).unwrap();
            writer.busy_timeout(std::time::Duration::ZERO).unwrap();
            writer
                .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='blocked'")
                .unwrap();
            assert!(writer.execute_batch("COMMIT").is_err());
            writer.execute_batch("ROLLBACK").unwrap();
            drop(writer);
            drop(result);
            let session = s.recovery_session().unwrap();
            session.ensure_publication_phase().unwrap();
            drop(session);
            assert!(s.operation(Some(1)).is_err());
        }
    }
}
#[test]
fn cleanup_sync_errors_stop_progress_keep_q_and_all_guards() {
    for fault in [
        "cleanup-q-file",
        "cleanup-q-directory",
        "cleanup-q-final",
        "cleanup-p-directory",
        "cleanup-p-final",
        "cleanup-root",
        "cleanup-root-final",
    ] {
        let f = Fixture::new();
        let s = state(&f);
        let d = durable(&s);
        let before = tree(&f.root);
        let failure = hooks::with(
            |_| Ok(()),
            move |name, p, f| {
                if name == fault {
                    Err(io::Error::other(fault))
                } else {
                    durability::sync_opened(p, f)
                }
            },
            || d.remove_pending(),
        )
        .err()
        .unwrap();
        assert!(format!("{:?}", failure.error()).contains(fault));
        guarded(&s, &f);
        let (pending, recovery, stage) = match fault {
            "cleanup-q-file" | "cleanup-q-directory" | "cleanup-q-final" => {
                (true, true, CleanupProgress::Unchanged)
            }
            "cleanup-p-directory" | "cleanup-p-final" => {
                (false, true, CleanupProgress::PendingUnlinked)
            }
            _ => (false, false, CleanupProgress::RecoveryUnlinked),
        };
        assert_eq!(failure.progress(), stage);
        assert_eq!(f.root.join(P).exists(), pending);
        assert_eq!(f.root.join("recovery").exists(), recovery);
        let mut expected = before;
        if !pending {
            expected.remove(&f.root.join(P));
        }
        assert_eq!(tree(&f.root), expected);
        assert!(f.root.join(Q).exists());
        if fault == "cleanup-root" {
            let writer = rusqlite::Connection::open(&f.path).unwrap();
            writer.busy_timeout(std::time::Duration::ZERO).unwrap();
            writer
                .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='blocked'")
                .unwrap();
            assert!(writer.execute_batch("COMMIT").is_err());
            writer.execute_batch("ROLLBACK").unwrap();
            drop(writer);
        }
        drop(failure);
        drop(s.recovery_session().unwrap());
        assert!(s.operation(None).is_err());
    }
}
#[test]
fn cleanup_checks_exact_order_and_detects_cancellation_before_and_after_names() {
    let f = Fixture::new();
    let s = state(&f);
    let d = durable(&s);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let a = calls.clone();
    let b = calls.clone();
    let done = hooks::with(
        move |n| {
            a.lock().unwrap().push(n.to_owned());
            Ok(())
        },
        move |n, p, f| {
            b.lock().unwrap().push(n.to_owned());
            durability::sync_opened(p, f)
        },
        || d.remove_pending(),
    )
    .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "cleanup-q-file",
            "cleanup-q-directory",
            "cleanup-q-final",
            "cleanup-before-p-unlink",
            "cleanup-p-unlink-syscall",
            "cleanup-p-unlinked",
            "cleanup-before-p-directory-sync",
            "cleanup-p-directory",
            "cleanup-after-p-directory-sync",
            "cleanup-p-final",
            "cleanup-before-recovery-rmdir",
            "cleanup-recovery-rmdir-syscall",
            "cleanup-recovery-unlinked",
            "cleanup-before-root-sync",
            "cleanup-root",
            "cleanup-after-root-sync",
            "cleanup-root-final",
            "cleanup-final-check"
        ]
    );
    drop(done);
    for point in [
        "cleanup-before-p-unlink",
        "cleanup-p-unlinked",
        "cleanup-before-recovery-rmdir",
        "cleanup-recovery-unlinked",
    ] {
        let f = Fixture::new();
        let s = state(&f);
        let d = durable(&s);
        let before = tree(&f.root);
        let cancel = observation(&d).cancel.clone();
        let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reached = hit.clone();
        let failed = hooks::with(
            move |name| {
                if name == point {
                    reached.store(true, std::sync::atomic::Ordering::Release);
                    cancel.store(true, std::sync::atomic::Ordering::Release);
                }
                Ok(())
            },
            |_, p, f| durability::sync_opened(p, f),
            || d.remove_pending(),
        )
        .err()
        .unwrap();
        assert!(hit.load(std::sync::atomic::Ordering::Acquire));
        assert!(matches!(
            failed.error(),
            CleanupError::Verification(Reason::Cancelled)
        ));
        guarded(&s, &f);
        let mut expected = before;
        if point != "cleanup-before-p-unlink" {
            expected.remove(&f.root.join(P));
        }
        assert_eq!(tree(&f.root), expected);
        assert_eq!(
            f.root.join("recovery").exists(),
            point != "cleanup-recovery-unlinked"
        );
    }
}
#[test]
fn changed_artifacts_are_never_accepted_or_recursively_removed() {
    for case in [
        "p-identity",
        "p-reappears",
        "recovery-reappears",
        "p-at-syscall",
        "directory-at-syscall",
        "p-bytes",
        "p-symlink",
        "recovery-before-p",
        "recovery-before-rmdir",
        "unknown",
        "q-before",
        "q-after-p",
    ] {
        let f = Fixture::new();
        let s = state(&f);
        let d = durable(&s);
        let root = f.root.clone();
        let pending_bytes = fs::read(f.root.join(P)).unwrap();
        let before_db = fs::read(&f.path).unwrap();
        let backup = fs::read(&f.protection).unwrap();
        let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reached = hit.clone();
        let failure = hooks::with(
            move |point| {
                let target = match case {
                    "recovery-before-rmdir" | "unknown" => "cleanup-before-recovery-rmdir",
                    "p-at-syscall" => "cleanup-p-unlink-syscall",
                    "directory-at-syscall" => "cleanup-recovery-rmdir-syscall",
                    "q-after-p" | "p-reappears" => "cleanup-p-unlinked",
                    "recovery-reappears" => "cleanup-recovery-unlinked",
                    _ => "cleanup-before-p-unlink",
                };
                if point == target {
                    reached.store(true, std::sync::atomic::Ordering::Release);
                    match case {
                        "p-reappears" => fs::write(root.join(P), &pending_bytes)?,
                        "recovery-reappears" => fs::create_dir(root.join("recovery"))?,
                        "p-identity" | "p-at-syscall" => {
                            fs::rename(root.join(P), root.join("kept-p"))?;
                            fs::copy(root.join("kept-p"), root.join(P))?;
                        }
                        "p-bytes" => {
                            let mut bytes = fs::read(root.join(P))?;
                            bytes.push(b' ');
                            fs::write(root.join(P), bytes)?;
                        }
                        "p-symlink" => {
                            fs::rename(root.join(P), root.join("kept-p"))?;
                            std::os::unix::fs::symlink(root.join("kept-p"), root.join(P))?;
                        }
                        "recovery-before-p" | "recovery-before-rmdir" | "directory-at-syscall" => {
                            fs::rename(root.join("recovery"), root.join("kept-recovery"))?;
                            fs::create_dir(root.join("recovery"))?;
                            if case == "recovery-before-p" {
                                fs::copy(
                                    root.join("kept-recovery/pending-restore.json"),
                                    root.join(P),
                                )?;
                            }
                        }
                        "unknown" => fs::write(root.join("recovery/unknown"), b"preserve me")?,
                        "q-before" | "q-after-p" => {
                            fs::rename(root.join(Q), root.join("kept-q"))?;
                            fs::copy(root.join("kept-q"), root.join(Q))?;
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(())
            },
            |_, p, f| durability::sync_opened(p, f),
            || d.remove_pending(),
        )
        .err()
        .unwrap();
        assert!(hit.load(std::sync::atomic::Ordering::Acquire));
        guarded(&s, &f);
        assert!(f.root.join("recovery").exists());
        assert!(f.root.join(Q).exists());
        if !matches!(
            case,
            "recovery-before-rmdir"
                | "directory-at-syscall"
                | "recovery-reappears"
                | "unknown"
                | "q-after-p"
        ) {
            assert!(fs::symlink_metadata(f.root.join(P)).is_ok());
        }
        if case == "unknown" {
            assert_eq!(
                fs::read(f.root.join("recovery/unknown")).unwrap(),
                b"preserve me"
            );
        }
        assert_eq!(fs::read(&f.path).unwrap(), before_db);
        assert_eq!(fs::read(&f.protection).unwrap(), backup);
        drop(failure);
    }
}
#[test]
fn in_process_cleanup_keeps_generation_maintenance_and_recovery_phase() {
    for fail in [false, true] {
        let f = Fixture::new();
        fs::remove_file(f.root.join(P)).unwrap();
        fs::remove_dir(f.root.join("recovery")).unwrap();
        let s = DatabaseState::default();
        s.initialize_legacy_startup(&f.root).unwrap();
        let candidate = crate::backup::restore::prepare_candidate(
            &f._dir.path().join("incoming.mno-backup"),
            flag(),
        )
        .unwrap();
        crate::backup::restore_context::with_active(&s, &f.path, candidate, flag(), |mut ctx| {
            let auth = ctx.authorize()?;
            drop(auth.publish_pending().unwrap());
            drop(auth);
            let generation = ctx.maintenance.generation();
            let d = classification::classify_session(
                ctx.maintenance.recovery_session().unwrap(),
                flag(),
            )
            .into_completion_proof()
            .unwrap()
            .publish_receipt()
            .unwrap();
            let done = hooks::with(
                |_| Ok(()),
                move |name, p, f| {
                    if fail && name == "cleanup-root" {
                        Err(io::Error::other("late cleanup failure"))
                    } else {
                        durability::sync_opened(p, f)
                    }
                },
                || d.remove_pending(),
            );
            assert_eq!(done.is_err(), fail);
            assert_eq!(
                ctx.maintenance.recovery_session().err().unwrap(),
                "database_recovery_busy"
            );
            assert_eq!(ctx.maintenance.generation(), generation);
            drop(done);
            let session = ctx.maintenance.recovery_session().unwrap();
            session.ensure_publication_phase().unwrap();
            drop(session);
            assert_eq!(ctx.maintenance.generation(), generation);
            assert!(ctx.maintenance.ensure_usable().is_err());
            Ok(())
        })
        .unwrap();
        assert!(s.operation(Some(1)).is_err());
        assert!(f.root.join(Q).exists());
    }
}
#[test]
fn cancellation_at_final_check_retains_guards_after_durable_removals() {
    let f = Fixture::new();
    fs::remove_file(f.root.join(P)).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let s = DatabaseState::default();
    s.initialize_legacy_startup(&f.root).unwrap();
    let candidate = crate::backup::restore::prepare_candidate(
        &f._dir.path().join("incoming.mno-backup"),
        flag(),
    )
    .unwrap();
    crate::backup::restore_context::with_active(&s, &f.path, candidate, flag(), |mut ctx| {
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let generation = ctx.maintenance.generation();
        let d =
            classification::classify_session(ctx.maintenance.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap()
                .publish_receipt()
                .unwrap();
        let before = tree(&f.root);
        let q = fs::read(f.root.join(Q)).unwrap();
        let cancel = observation(&d).cancel.clone();
        let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reached = hit.clone();
        let barriers = Arc::new(Mutex::new(Vec::new()));
        let checked = barriers.clone();
        let synced = barriers.clone();
        let root = f.root.clone();
        let failed = hooks::with(
            move |name| {
                if name == "cleanup-final-check" {
                    assert!(!root.join(P).exists());
                    assert!(!root.join("recovery").exists());
                    assert_eq!(
                        *checked.lock().unwrap(),
                        [
                            "cleanup-q-file",
                            "cleanup-q-directory",
                            "cleanup-q-final",
                            "cleanup-p-directory",
                            "cleanup-p-final",
                            "cleanup-root",
                            "cleanup-root-final",
                        ]
                    );
                    reached.store(true, std::sync::atomic::Ordering::Release);
                    cancel.store(true, std::sync::atomic::Ordering::Release);
                }
                Ok(())
            },
            move |name, point, file| {
                durability::sync_opened(point, file)?;
                synced.lock().unwrap().push(name.to_owned());
                Ok(())
            },
            || d.remove_pending(),
        )
        .err()
        .expect("final cancellation must not produce RecoveryDirectoryRemoved");
        assert!(hit.load(std::sync::atomic::Ordering::Acquire));
        assert!(matches!(
            failed.error(),
            CleanupError::Verification(Reason::Cancelled)
        ));
        // Disk cleanup succeeded, but the final verified result was rejected.
        assert_eq!(failed.progress(), CleanupProgress::RecoveryDurable);
        assert!(!f.root.join(P).exists());
        assert!(!f.root.join("recovery").exists());
        assert_eq!(fs::read(f.root.join(Q)).unwrap(), q);
        expected_files(&f, &before);
        assert_eq!(
            ctx.maintenance.recovery_session().err().unwrap(),
            "database_recovery_busy"
        );
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        observation(&failed.receipt).database.verify().unwrap();
        let writer = rusqlite::Connection::open(&f.path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='blocked'")
            .unwrap();
        assert!(writer.execute_batch("COMMIT").is_err());
        writer.execute_batch("ROLLBACK").unwrap();
        drop(writer);
        expected_files(&f, &before);
        drop(failed);
        let session = ctx.maintenance.recovery_session().unwrap();
        session.ensure_publication_phase().unwrap();
        drop(session);
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        Ok(())
    })
    .unwrap();
    assert!(s.operation(Some(1)).is_err());
}
#[test]
fn cleanup_crash_child() {
    let Ok(root) = std::env::var("MNO_CLEANUP_CRASH_ROOT") else {
        return;
    };
    let point = std::env::var("MNO_CLEANUP_CRASH_POINT").unwrap();
    let s = DatabaseState::default();
    assert!(s.initialize_legacy_startup(Path::new(&root)).is_err());
    let d = durable(&s);
    let _ = hooks::with(
        move |name| {
            if name == point {
                println!("cleanup-crash-ready");
                std::io::stdout().flush().unwrap();
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
        |_, p, f| durability::sync_opened(p, f),
        || d.remove_pending(),
    );
    panic!("crash checkpoint not reached");
}
#[test]
fn subprocess_crashes_at_every_cleanup_boundary_resume_from_fresh_verification() {
    for point in [
        "cleanup-before-p-unlink",
        "cleanup-p-unlinked",
        "cleanup-before-p-directory-sync",
        "cleanup-after-p-directory-sync",
        "cleanup-before-recovery-rmdir",
        "cleanup-recovery-unlinked",
        "cleanup-before-root-sync",
        "cleanup-after-root-sync",
    ] {
        let f = Fixture::new();
        let before = tree(&f.root);
        let expected_q =
            completion::receipt_bytes(&fs::read(f.root.join(P)).unwrap(), completion::Outcome::Old)
                .unwrap();
        let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::pending::classification::result::receipt::cleanup::tests::cleanup_crash_child","--nocapture"]).env("MNO_CLEANUP_CRASH_ROOT",&f.root).env("MNO_CLEANUP_CRASH_POINT",point).stdout(Stdio::piped()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.unwrap().contains("cleanup-crash-ready") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        let reached = rx.recv_timeout(std::time::Duration::from_secs(30));
        child.kill().unwrap();
        child.wait().unwrap();
        reached.expect("cleanup child failed to reach checkpoint");
        assert_eq!(fs::read(f.root.join(Q)).unwrap(), expected_q);
        let pending = point == "cleanup-before-p-unlink";
        let recovery = !matches!(
            point,
            "cleanup-recovery-unlinked" | "cleanup-before-root-sync" | "cleanup-after-root-sync"
        );
        assert_eq!(f.root.join(P).exists(), pending);
        assert_eq!(f.root.join("recovery").exists(), recovery);
        let mut expected = before;
        expected.insert(f.root.join(Q), expected_q.clone());
        if !pending {
            expected.remove(&f.root.join(P));
        }
        assert_eq!(tree(&f.root), expected);
        let s = state(&f);
        assert!(s.operation(None).is_err());
        let done = durable(&s).remove_pending().unwrap();
        assert!(!f.root.join("recovery").exists());
        assert_eq!(fs::read(f.root.join(Q)).unwrap(), expected_q);
        drop(done);
        assert!(s.operation(None).is_err());
    }
}

#[test]
fn injected_unlink_errors_keep_attempted_progress_and_do_not_continue() {
    for point in ["cleanup-p-unlink-syscall", "cleanup-recovery-rmdir-syscall"] {
        let f = Fixture::new();
        let s = state(&f);
        let d = durable(&s);
        let before = tree(&f.root);
        let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = hit.clone();
        let failed = hooks::with(
            move |name| {
                if name == point {
                    observed.store(true, std::sync::atomic::Ordering::Release);
                    Err(io::Error::from_raw_os_error(libc::EIO))
                } else {
                    Ok(())
                }
            },
            |_, p, f| durability::sync_opened(p, f),
            || d.remove_pending(),
        )
        .err()
        .unwrap();
        assert!(hit.load(std::sync::atomic::Ordering::Acquire));
        assert!(matches!(failed.error(),CleanupError::Io(e) if e.raw_os_error()==Some(libc::EIO)));
        assert_eq!(
            failed.progress(),
            if point == "cleanup-p-unlink-syscall" {
                CleanupProgress::PendingUnlinkAttempted
            } else {
                CleanupProgress::RecoveryUnlinkAttempted
            }
        );
        guarded(&s, &f);
        assert!(f.root.join("recovery").exists());
        let mut expected = before;
        if point == "cleanup-recovery-rmdir-syscall" {
            expected.remove(&f.root.join(P));
        }
        assert_eq!(tree(&f.root), expected);
    }
}
#[test]
fn real_rmdir_refuses_a_late_unknown_file_without_recursive_removal() {
    let f = Fixture::new();
    let s = state(&f);
    let d = durable(&s);
    let root = f.root.clone();
    let before = tree(&f.root);
    let failed = hooks::with(
        move |point| {
            if point == "cleanup-recovery-rmdir-syscall" {
                fs::write(root.join("recovery/late-unknown"), b"never remove")?;
            }
            Ok(())
        },
        |_, p, f| durability::sync_opened(p, f),
        || d.remove_pending(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(failed.error(),CleanupError::Io(e) if e.raw_os_error()==Some(libc::ENOTEMPTY))
    );
    assert_eq!(failed.progress(), CleanupProgress::RecoveryUnlinkAttempted);
    guarded(&s, &f);
    let mut expected = before;
    expected.remove(&f.root.join(P));
    expected.insert(
        f.root.join("recovery/late-unknown"),
        b"never remove".to_vec(),
    );
    assert_eq!(tree(&f.root), expected);
}
#[test]
fn panics_after_each_name_operation_preserve_q_and_never_ready() {
    for point in ["cleanup-p-unlinked", "cleanup-recovery-unlinked"] {
        let f = Fixture::new();
        let s = state(&f);
        let d = durable(&s);
        let before = tree(&f.root);
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hit = reached.clone();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |name| {
                    if name == point {
                        hit.store(true, std::sync::atomic::Ordering::Release);
                        panic!("controlled cleanup unwind");
                    }
                    Ok(())
                },
                |_, p, f| durability::sync_opened(p, f),
                || d.remove_pending(),
            )
        }));
        assert!(panic.is_err());
        assert!(reached.load(std::sync::atomic::Ordering::Acquire));
        expected_files(&f, &before);
        assert_eq!(
            f.root.join("recovery").exists(),
            point == "cleanup-p-unlinked"
        );
        let session = s.recovery_session().unwrap();
        session.ensure_publication_phase().unwrap();
        drop(session);
        assert!(s.operation(None).is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    }
}
