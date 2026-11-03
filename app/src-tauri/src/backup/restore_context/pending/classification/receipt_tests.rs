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
const Q: &str = ".mno-restore-completion-v1.json";
const P: &str = "recovery/pending-restore.json";
fn state(f: &Fixture) -> DatabaseState {
    let s = DatabaseState::default();
    assert!(s.initialize_legacy_startup(&f.root).is_err());
    s
}
fn proof(s: &DatabaseState) -> CompletionProof<'_, '_> {
    classification::classify_session(s.recovery_session().unwrap(), flag())
        .into_completion_proof()
        .unwrap()
}
fn receipt(f: &Fixture, outcome: &str) {
    fs::write(f.root.join(Q),serde_json::to_vec(&serde_json::json!({"format":"mno-restore-completion","version":1,"pending_json":fs::read_to_string(f.root.join(P)).unwrap(),"verified_outcome":outcome})).unwrap()).unwrap();
}
fn guards(s: &DatabaseState, root: &Path) {
    assert_eq!(
        s.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(s.operation(Some(1)).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(root).is_err());
}
#[test]
fn new_and_existing_receipts_keep_the_entire_proof_and_sources() {
    for outcome in ["old", "new"] {
        for existing in [false, true] {
            let f = Fixture::new();
            f.title(if outcome == "old" { "Old" } else { "New" });
            if existing {
                receipt(&f, outcome);
            }
            let before = tree(&f.root);
            let s = state(&f);
            let durable = proof(&s).publish_receipt().unwrap();
            guards(&s, &f.root);
            let q = completion::read(&f.root).unwrap().unwrap();
            assert_eq!(q.pending_bytes(), fs::read(f.root.join(P)).unwrap());
            assert_eq!(
                durable.proof.assessment.status(),
                if outcome == "old" {
                    Status::Old
                } else {
                    Status::New
                }
            );
            let mut after = tree(&f.root);
            if !existing {
                after.remove(&f.root.join(Q));
            }
            let mut original = before.clone();
            if !existing {
                original.remove(&f.root.join(Q));
            }
            assert_eq!(after, original);
            let writer = rusqlite::Connection::open(&f.path).unwrap();
            writer.busy_timeout(std::time::Duration::ZERO).unwrap();
            writer
                .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='blocked'")
                .unwrap();
            assert!(writer.execute_batch("COMMIT").is_err());
            drop(durable);
            writer.execute_batch("ROLLBACK").unwrap();
            drop(writer);
            drop(s.recovery_session().unwrap());
            assert!(s.operation(Some(1)).is_err());
        }
    }
}
#[test]
fn every_sync_failure_retains_guards_and_preserves_all_published_files() {
    for fail in [
        "journal-file",
        "journal-directory",
        "journal-final",
        "receipt-file",
        "receipt-directory",
        "receipt-final",
        "existing-file",
        "existing-directory",
        "existing-final",
    ] {
        let f = Fixture::new();
        let existing = fail.starts_with("existing");
        if existing {
            receipt(&f, "old");
        }
        let before = tree(&f.root);
        let s = state(&f);
        let p = proof(&s);
        let failed = hooks::with(
            |_| Ok(()),
            move |name, point, file| {
                if name == fail {
                    Err(io::Error::other(format!("injected {fail}")))
                } else {
                    durability::sync_opened(point, file)
                }
            },
            || p.publish_receipt(),
        )
        .err()
        .unwrap();
        assert!(format!("{:?}", failed.error()).contains(fail));
        guards(&s, &f.root);
        let published = existing || matches!(fail, "receipt-directory" | "receipt-final");
        assert_eq!(failed.may_have_receipt(), published);
        assert_eq!(f.root.join(Q).exists(), published);
        if published {
            completion::read(&f.root).unwrap().unwrap();
        }
        let mut after = tree(&f.root);
        let mut before = before;
        if !existing {
            after.remove(&f.root.join(Q));
            before.remove(&f.root.join(Q));
        }
        assert_eq!(after, before);
        drop(failed);
        drop(s.recovery_session().unwrap());
        assert!(s.operation(Some(1)).is_err());
    }
}
#[test]
fn exact_barrier_order_and_prepublication_cancellation() {
    let f = Fixture::new();
    let s = state(&f);
    let p = proof(&s);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let e = calls.clone();
    let b = calls.clone();
    let d = hooks::with(
        move |name| {
            e.lock().unwrap().push(name.to_owned());
            Ok(())
        },
        move |name, point, file| {
            b.lock().unwrap().push(name.to_owned());
            durability::sync_opened(point, file)
        },
        || p.publish_receipt(),
    )
    .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "journal-file",
            "journal-directory",
            "journal-final",
            "before-temporary",
            "partial-write",
            "receipt-file",
            "before-publication",
            "rename-before-syscall",
            "published",
            "receipt-directory",
            "receipt-final",
            "final-check"
        ]
    );
    drop(d);
    let f = Fixture::new();
    let s = state(&f);
    let p = proof(&s);
    let cancel = p.assessment.observation.as_ref().unwrap().cancel.clone();
    let failed = hooks::with(
        move |point| {
            if point == "before-publication" {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
            Ok(())
        },
        |_, p, f| durability::sync_opened(p, f),
        || p.publish_receipt(),
    )
    .err()
    .unwrap();
    assert!(!failed.may_have_receipt());
    assert!(!f.root.join(Q).exists());
    guards(&s, &f.root);
}
#[test]
fn changed_receipt_or_source_is_rejected_without_overwrite() {
    for which in [
        "receipt-bytes",
        "receipt-identity",
        "marker",
        "archive",
        "database",
    ] {
        let f = Fixture::new();
        receipt(&f, "old");
        let s = state(&f);
        let p = proof(&s);
        match which {
            "receipt-bytes" => receipt(&f, "new"),
            "receipt-identity" => {
                fs::rename(f.root.join(Q), f.root.join("kept-receipt")).unwrap();
                receipt(&f, "old");
            }
            "marker" => {
                let mut b = fs::read(f.root.join(P)).unwrap();
                b.push(b' ');
                fs::write(f.root.join(P), b).unwrap();
            }
            "archive" => {
                fs::write(&f.protection, b"altered").unwrap();
            }
            "database" => {
                fs::rename(&f.path, f.root.join("kept.sqlite")).unwrap();
                fs::copy(f.root.join("kept.sqlite"), &f.path).unwrap();
            }
            _ => unreachable!(),
        }
        let before = tree(&f.root);
        let failed = p.publish_receipt().err().unwrap();
        guards(&s, &f.root);
        assert_eq!(tree(&f.root), before);
        assert!(failed.may_have_receipt());
    }
}
#[test]
fn late_errors_and_collisions_never_remove_or_overwrite_receipts() {
    for fail in [
        "partial-write",
        "before-publication",
        "published",
        "final-check",
    ] {
        let f = Fixture::new();
        let s = state(&f);
        let p = proof(&s);
        let failed = hooks::with(
            move |point| {
                if point == fail {
                    Err(io::Error::other(fail))
                } else {
                    Ok(())
                }
            },
            |_, p, f| durability::sync_opened(p, f),
            || p.publish_receipt(),
        )
        .err()
        .unwrap();
        let late = matches!(fail, "published" | "final-check");
        assert_eq!(failed.may_have_receipt(), late);
        assert_eq!(f.root.join(Q).exists(), late);
        guards(&s, &f.root);
    }
    let f = Fixture::new();
    let s = state(&f);
    let p = proof(&s);
    let path = f.root.join(Q);
    let failed = hooks::with(
        move |point| {
            if point == "before-publication" {
                fs::write(&path, b"do not overwrite")?;
            }
            Ok(())
        },
        |_, p, f| durability::sync_opened(p, f),
        || p.publish_receipt(),
    )
    .err()
    .unwrap();
    assert_eq!(fs::read(f.root.join(Q)).unwrap(), b"do not overwrite");
    guards(&s, &f.root);
    drop(failed);
}
#[test]
fn publication_crash_child() {
    let Ok(root) = std::env::var("MNO_RECEIPT_CRASH_ROOT") else {
        return;
    };
    let point = std::env::var("MNO_RECEIPT_CRASH_POINT").unwrap();
    let s = DatabaseState::default();
    assert!(s.initialize_legacy_startup(Path::new(&root)).is_err());
    let p = proof(&s);
    let _ = hooks::with(
        move |name| {
            if name == point {
                println!("receipt-crash-ready");
                std::io::stdout().flush().unwrap();
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
        |_, p, f| durability::sync_opened(p, f),
        || p.publish_receipt(),
    );
    panic!("crash checkpoint not reached");
}
#[test]
fn subprocess_crashes_preserve_journal_and_require_blocked_restart() {
    for point in [
        "before-temporary",
        "partial-write",
        "before-publication",
        "published",
        "final-check",
    ] {
        let f = Fixture::new();
        let journal = fs::read(&f.path).unwrap();
        let p = fs::read(f.root.join(P)).unwrap();
        let backup = fs::read(&f.protection).unwrap();
        let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","backup::restore_context::pending::classification::result::receipt::tests::publication_crash_child","--nocapture"])
            .env("MNO_RECEIPT_CRASH_ROOT",&f.root).env("MNO_RECEIPT_CRASH_POINT",point).stdout(Stdio::piped()).spawn().unwrap();
        // Subprocess protocol; bounded reader, kill on timeout, never poll sleeps.
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.unwrap().contains("receipt-crash-ready") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        let reached = rx.recv_timeout(std::time::Duration::from_secs(30));
        child.kill().unwrap();
        child.wait().unwrap();
        reached.expect("child failed to reach crash checkpoint");
        assert_eq!(fs::read(&f.path).unwrap(), journal);
        assert_eq!(fs::read(f.root.join(P)).unwrap(), p);
        assert_eq!(fs::read(&f.protection).unwrap(), backup);
        let s = state(&f);
        assert!(s.operation(Some(1)).is_err());
        if matches!(point, "published" | "final-check") {
            completion::read(&f.root).unwrap().unwrap();
            assert_eq!(
                classification::classify_session(s.recovery_session().unwrap(), flag()).status(),
                Status::Old
            );
        } else {
            assert!(!f.root.join(Q).exists());
        }
    }
}

#[test]
fn receipt_only_and_empty_recovery_reuse_exact_retained_receipt() {
    for empty in [false, true] {
        let f = Fixture::new();
        receipt(&f, "old");
        fs::remove_file(f.root.join(P)).unwrap();
        if !empty {
            fs::remove_dir(f.root.join("recovery")).unwrap();
        }
        let before = tree(&f.root);
        let s = state(&f);
        let d = proof(&s).publish_receipt().unwrap();
        guards(&s, &f.root);
        assert_eq!(tree(&f.root), before);
        drop(d);
        assert!(s.operation(None).is_err());
    }
}
#[test]
fn in_process_receipt_keeps_maintenance_and_generation() {
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
        let proof =
            classification::classify_session(ctx.maintenance.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap();
        let durable = proof.publish_receipt().unwrap();
        assert_eq!(
            ctx.maintenance.recovery_session().err().unwrap(),
            "database_recovery_busy"
        );
        assert_eq!(ctx.maintenance.generation(), generation);
        drop(durable);
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        Ok(())
    })
    .unwrap();
    assert!(s.operation(Some(1)).is_err());
}

#[test]
fn mutation_at_publication_boundaries_is_rejected_and_never_cleaned_blindly() {
    for case in [
        "source",
        "sidecar",
        "q-symlink",
        "q-replaced",
        "late-cancel",
        "unknown-temporary",
    ] {
        let f = Fixture::new();
        let s = state(&f);
        let p = proof(&s);
        let root = f.root.clone();
        let cancel = p.assessment.observation.as_ref().unwrap().cancel.clone();
        let failed = hooks::with(
            move |point| {
                if point == "before-publication" {
                    match case {
                        "source" => {
                            fs::rename(root.join("mno.sqlite"), root.join("kept.sqlite"))?;
                            fs::copy(root.join("kept.sqlite"), root.join("mno.sqlite"))?;
                        }
                        "sidecar" => {
                            fs::write(root.join("mno.sqlite-journal"), b"untrusted journal")?;
                        }
                        "unknown-temporary" => {
                            fs::write(root.join(".mno-completion-untrusted"), b"preserve")?;
                        }
                        "q-symlink" => {
                            std::os::unix::fs::symlink(root.join(P), root.join(Q))?;
                        }
                        _ => (),
                    }
                }
                if point == "published" {
                    match case {
                        "q-replaced" => {
                            fs::rename(root.join(Q), root.join("kept-q"))?;
                            fs::write(root.join(Q), b"replacement")?;
                        }
                        "late-cancel" => cancel.store(true, std::sync::atomic::Ordering::Release),
                        _ => (),
                    }
                }
                Ok(())
            },
            |_, p, f| durability::sync_opened(p, f),
            || p.publish_receipt(),
        )
        .err()
        .unwrap();
        guards(&s, &f.root);
        if matches!(case, "q-replaced" | "late-cancel") {
            assert!(failed.may_have_receipt());
            assert!(f.root.join(Q).exists());
        }
        if case == "q-replaced" {
            assert_eq!(fs::read(f.root.join(Q)).unwrap(), b"replacement");
            assert!(f.root.join("kept-q").exists());
        }
        if case == "sidecar" {
            assert_eq!(
                fs::read(f.root.join("mno.sqlite-journal")).unwrap(),
                b"untrusted journal"
            );
        }
        if case == "unknown-temporary" {
            assert_eq!(
                fs::read(f.root.join(".mno-completion-untrusted")).unwrap(),
                b"preserve"
            );
        }
        assert!(f.root.join(P).exists());
        assert!(f.protection.exists());
    }
}

#[test]
fn collision_in_actual_rename_syscall_preserves_conflict_and_proof_guards() {
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
        let before = tree(&f.root);
        let proof =
            classification::classify_session(ctx.maintenance.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let events = observed.clone();
        let target = f.root.join(Q);
        let failed = hooks::with(
            move |point| {
                events.lock().unwrap().push(point.to_owned());
                if point == "rename-before-syscall" {
                    // After auth.check, byte/identity checks and destination creation,
                    // directly before the real renameatx_np(RENAME_EXCL) call.
                    fs::write(&target, b"conflicting receipt: preserve exact bytes")?;
                }
                Ok(())
            },
            |_, p, f| durability::sync_opened(p, f),
            || proof.publish_receipt(),
        )
        .err()
        .unwrap();
        let events = observed.lock().unwrap();
        let before_call = events
            .iter()
            .position(|p| p == "rename-before-syscall")
            .unwrap();
        let returned_error = events
            .iter()
            .position(|p| p == "rename-syscall-error")
            .unwrap();
        assert!(
            before_call < returned_error,
            "the real syscall must return an error"
        );
        assert!(!events.iter().any(|p| p == "published"));
        assert!(matches!(
            failed.error(),
            PublicationError::Receipt(completion::ReceiptError::AlreadyExists)
        ));
        assert_eq!(
            fs::read(f.root.join(Q)).unwrap(),
            b"conflicting receipt: preserve exact bytes"
        );
        assert_eq!(
            ctx.maintenance.recovery_session().err().unwrap(),
            "database_recovery_busy"
        );
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        // Failure retains the actual SQLite reservation, not only metadata.
        let writer = rusqlite::Connection::open(&f.path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='blocked'")
            .unwrap();
        assert!(writer.execute_batch("COMMIT").is_err());
        writer.execute_batch("ROLLBACK").unwrap();
        drop(writer);
        let mut after = tree(&f.root);
        after.remove(&f.root.join(Q));
        assert_eq!(after, before);
        drop(failed);
        assert_eq!(
            fs::read(f.root.join(Q)).unwrap(),
            b"conflicting receipt: preserve exact bytes"
        );
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        Ok(())
    })
    .unwrap();
    assert!(s.operation(Some(1)).is_err());
}

#[test]
fn panic_after_name_publication_keeps_q_without_claiming_durability_or_ready() {
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
        let before = tree(&f.root);
        let expected =
            completion::receipt_bytes(&fs::read(f.root.join(P)).unwrap(), completion::Outcome::Old)
                .unwrap();
        let proof =
            classification::classify_session(ctx.maintenance.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap();
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hit = reached.clone();
        let target = f.root.join(Q);
        let expected_in_hook = expected.clone();
        let barriers = Arc::new(Mutex::new(Vec::new()));
        let observed = barriers.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |point| {
                    if point == "published" {
                        assert_eq!(fs::read(&target).unwrap(), expected_in_hook);
                        hit.store(true, std::sync::atomic::Ordering::Release);
                        panic!("controlled panic after rename and before receipt directory sync");
                    }
                    Ok(())
                },
                move |name, p, f| {
                    observed.lock().unwrap().push(name.to_owned());
                    durability::sync_opened(p, f)
                },
                || proof.publish_receipt(),
            )
        }));
        assert!(
            result.is_err(),
            "panic must unwind; no ReceiptDurable was returned"
        );
        assert!(reached.load(std::sync::atomic::Ordering::Acquire));
        let barriers = barriers.lock().unwrap();
        assert!(barriers.iter().any(|p| p == "receipt-file"));
        assert!(
            !barriers
                .iter()
                .any(|p| p == "receipt-directory" || p == "receipt-final")
        );
        // Existence/readability are NOT evidence of durable name publication.
        assert_eq!(fs::read(f.root.join(Q)).unwrap(), expected);
        let mut after = tree(&f.root);
        after.remove(&f.root.join(Q));
        assert_eq!(
            after, before,
            "journal, P and protection must remain byte-identical"
        );
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        // Unwinding releases the slot, but never Recovery/Ready or the OS lock.
        let session = ctx.maintenance.recovery_session().unwrap();
        session.ensure_publication_phase().unwrap();
        drop(session);
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        Ok(())
    })
    .unwrap();
    assert!(s.operation(Some(1)).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}

#[test]
fn current_temporary_requires_exact_case_as_well_as_existing_identity_checks() {
    for rename_case in [false, true] {
        let f = Fixture::new();
        let s = state(&f);
        let p = proof(&s);
        let before_p = fs::read(f.root.join(P)).unwrap();
        let before_archive = fs::read(&f.protection).unwrap();
        let before_db = fs::read(&f.path).unwrap();
        let root = f.root.clone();
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hit = reached.clone();
        let result = hooks::with(
            move |point| {
                if point == "before-publication" {
                    let entry = fs::read_dir(&root)?
                        .map(|e| e.unwrap().path())
                        .find(|p| {
                            p.file_name()
                                .is_some_and(crate::startup::artifacts::completion_temporary)
                        })
                        .unwrap();
                    let name = entry.file_name().unwrap().to_str().unwrap();
                    if rename_case {
                        fs::rename(&entry, root.join(name.to_ascii_uppercase()))?;
                    }
                    hit.store(true, std::sync::atomic::Ordering::Release);
                }
                Ok(())
            },
            |_, point, file| durability::sync_opened(point, file),
            || p.publish_receipt(),
        );
        assert!(reached.load(std::sync::atomic::Ordering::Acquire));
        if rename_case {
            let failed = result.err().unwrap();
            guards(&s, &f.root);
            assert!(!f.root.join(Q).exists());
            drop(failed);
        } else {
            let verified = result.unwrap();
            guards(&s, &f.root);
            assert!(f.root.join(Q).exists());
            drop(verified);
        }
        assert_eq!(fs::read(f.root.join(P)).unwrap(), before_p);
        assert_eq!(fs::read(&f.protection).unwrap(), before_archive);
        assert_eq!(fs::read(&f.path).unwrap(), before_db);
    }
}
