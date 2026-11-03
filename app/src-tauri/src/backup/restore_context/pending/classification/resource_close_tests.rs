use super::*;
use crate::{
    backup::restore_context::pending::classification::{
        self,
        tests::{Fixture, flag, tree},
    },
    database_state::{DatabaseState, Maintenance},
};
use rusqlite::ffi;
use std::{
    fs, ptr,
    sync::{Arc, atomic::Ordering},
};
const Q: &str = ".mno-restore-completion-v1.json";
fn scenario(
    run: impl FnOnce(&Fixture, &DatabaseState, &Maintenance<'_>, RecoveryDirectoryRemoved<'_, '_>),
) {
    let f = Fixture::new();
    fs::remove_file(f.root.join("recovery/pending-restore.json")).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let s = DatabaseState::default();
    s.initialize_legacy_startup(&f.root).unwrap();
    let candidate = crate::backup::restore::prepare_candidate(
        &f._dir.path().join("incoming.mno-backup"),
        flag(),
    )
    .unwrap();
    crate::backup::restore_context::with_active(&s, &f.path, candidate, flag(), |mut ctx| {
        {
            let auth = ctx.authorize()?;
            drop(auth.publish_pending().unwrap());
        }
        let removed =
            classification::classify_session(ctx.maintenance.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap()
                .publish_receipt()
                .unwrap()
                .remove_pending()
                .unwrap();
        let generation = ctx.maintenance.generation();
        run(&f, &s, ctx.maintenance, removed);
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        let session = ctx.maintenance.recovery_session().unwrap();
        session.ensure_publication_phase().unwrap();
        drop(session);
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        Ok(())
    })
    .unwrap();
    assert!(s.operation(Some(1)).is_err());
    assert!(f.root.join(Q).is_file());
}
fn guards(f: &Fixture, m: &Maintenance<'_>) {
    assert_eq!(
        m.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(m.ensure_usable().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
fn obs<'p>(r: &'p mut RecoveryDirectoryRemoved<'_, '_>) -> &'p mut Observation {
    r.receipt.proof.assessment.observation.as_mut().unwrap()
}
fn registered(name: &std::ffi::CStr) -> bool {
    unsafe { !ffi::sqlite3_vfs_find(name.as_ptr()).is_null() }
}
fn blocks_commit(f: &Fixture) {
    let writer = rusqlite::Connection::open(&f.path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='must not commit'")
        .unwrap();
    assert!(writer.execute_batch("COMMIT").is_err());
    writer.execute_batch("ROLLBACK").unwrap();
}
#[test]
fn explicit_close_shuts_both_connections_then_unregisters_without_changing_q_or_phase() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let name = obs(&mut removed).database.test_name();
        let staging = obs(&mut removed).protection.database_path();
        blocks_commit(f);
        let closed = removed.close_resources().unwrap();
        guards(f, m);
        assert!(!registered(&name));
        closed.closed().access.check().unwrap();
        assert!(closed.closed().files.evidence.receipt.is_some());
        // The snapshot really is closed: a separate writer can now commit an
        // empty transaction. This does not grant MNO permission for normal I/O.
        let writer = rusqlite::Connection::open(&f.path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer.execute_batch("BEGIN EXCLUSIVE; COMMIT").unwrap();
        drop(writer);
        assert_eq!(tree(&f.root), before);
        assert!(staging.is_file());
        drop(closed);
        assert!(!staging.exists());
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn real_outstanding_statement_causes_busy_and_keeps_connection_vfs_snapshot_and_guards() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let o = obs(&mut removed);
        let name = o.database.test_name();
        let stmt = o.database.test_statement();
        let failed = removed
            .close_resources()
            .err()
            .expect("BUSY cannot close resources");
        assert!(matches!(
            failed.error(),
            ResourceCloseError::ActiveSqlite {
                code: Some(ffi::SQLITE_BUSY),
                ..
            }
        ));
        assert_eq!(failed.progress(), ResourceCloseProgress::Unchanged);
        guards(f, m);
        assert!(registered(&name));
        blocks_commit(f);
        let Retained::Closing(c) = &*failed.retained else {
            panic!("must retain closing owner")
        };
        let Some(Active::Failed(a)) = &c.active else {
            panic!("must retain failed binding")
        };
        assert!(a.binding.test_is_open());
        a.binding.verify().unwrap();
        assert!(matches!(c.protection, Some(Protection::Live(_))));
        assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        assert!(!registered(&name));
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn other_sqlite_close_errors_do_not_unregister_or_discard_the_snapshot() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let o = obs(&mut removed);
        let name = o.database.test_name();
        o.database.test_close_error(ffi::SQLITE_IOERR);
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::ActiveSqlite {
                code: Some(ffi::SQLITE_IOERR),
                ..
            }
        ));
        guards(f, m);
        assert!(registered(&name));
        blocks_commit(f);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        assert!(!registered(&name));
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn unregister_error_retains_registered_allocations_but_never_reports_resources_closed() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let o = obs(&mut removed);
        let name = o.database.test_name();
        let held = o.files.active_file.clone();
        let references = Arc::strong_count(&held);
        o.database.test_unregister_failure();
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(failed.error(), ResourceCloseError::VfsUnregister));
        assert_eq!(failed.progress(), ResourceCloseProgress::ActiveSqliteClosed);
        let Retained::Closing(c) = &*failed.retained else {
            panic!()
        };
        let Some(Active::Failed(a)) = &c.active else {
            panic!()
        };
        assert!(!a.binding.test_is_open());
        assert!(matches!(c.protection, Some(Protection::Live(_))));
        guards(f, m);
        assert!(registered(&name));
        assert_eq!(Arc::strong_count(&held), references);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        // Failed fallback unregister deliberately leaks its live callback storage
        // and descriptor ownership, rather than create a dangling registry entry.
        assert!(registered(&name));
        assert!(Arc::strong_count(&held) > 1);
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn protection_busy_retains_its_connection_and_private_files_after_active_close() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let o = obs(&mut removed);
        let name = o.database.test_name();
        let staging = o.protection.database_path();
        let mut stmt = ptr::null_mut();
        assert_eq!(
            unsafe {
                ffi::sqlite3_prepare_v2(
                    o.protection.source().unwrap().handle(),
                    c"SELECT 1".as_ptr(),
                    -1,
                    &mut stmt,
                    ptr::null_mut(),
                )
            },
            ffi::SQLITE_OK
        );
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::ProtectionSqlite {
                code: Some(ffi::SQLITE_BUSY),
                ..
            }
        ));
        assert_eq!(failed.progress(), ResourceCloseProgress::ActiveClosed);
        guards(f, m);
        assert!(!registered(&name));
        assert!(staging.is_file());
        assert_eq!(unsafe { ffi::sqlite3_step(stmt) }, ffi::SQLITE_ROW);
        assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
        assert_eq!(tree(&f.root), before);
        drop(failed);
        assert!(
            !staging.exists(),
            "finalized statement permits confirmed close and staging cleanup"
        );
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn cancellation_before_shutdown_retains_the_original_snapshot_and_q() {
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        obs(&mut removed).cancel.store(true, Ordering::Release);
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::Verification(Reason::Cancelled)
        ));
        assert!(matches!(*failed.retained, Retained::Before(_)));
        guards(f, m);
        blocks_commit(f);
        assert_eq!(tree(&f.root), before);
        drop(failed);
    });
}
#[test]
fn panics_at_each_shutdown_boundary_leave_q_phase_and_generation_unchanged() {
    for point in [
        "resources-before-active-close",
        "resources-after-active-close",
        "resources-after-protection-close",
    ] {
        scenario(|f, _, _, removed| {
            let before = tree(&f.root);
            let hit = Arc::new(AtomicBool::new(false));
            let reached = hit.clone();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == point {
                            reached.store(true, Ordering::Release);
                            panic!("controlled shutdown panic");
                        }
                        Ok(())
                    },
                    |_, p, f| durability::sync_opened(p, f),
                    || removed.close_resources(),
                )
            }));
            assert!(panic.is_err());
            assert!(hit.load(Ordering::Acquire));
            assert_eq!(tree(&f.root), before);
        });
    }
}
#[test]
fn shutdown_checkpoint_io_failures_preserve_progress_and_all_remaining_owners() {
    for (point, progress) in [
        (
            "resources-before-active-close",
            ResourceCloseProgress::Unchanged,
        ),
        (
            "resources-after-active-close",
            ResourceCloseProgress::ActiveClosed,
        ),
        (
            "resources-after-protection-close",
            ResourceCloseProgress::ProtectionClosed,
        ),
    ] {
        scenario(|f, _, m, removed| {
            let before = tree(&f.root);
            let failed = hooks::with(
                move |name| {
                    if name == point {
                        Err(io::Error::other("controlled close checkpoint error"))
                    } else {
                        Ok(())
                    }
                },
                |_, p, f| durability::sync_opened(p, f),
                || removed.close_resources(),
            )
            .err()
            .unwrap();
            assert!(matches!(failed.error(), ResourceCloseError::Io(_)));
            assert_eq!(failed.progress(), progress);
            guards(f, m);
            assert_eq!(tree(&f.root), before);
            drop(failed);
        });
    }
}

#[test]
fn changed_q_is_rejected_before_any_connection_closes() {
    scenario(|f, _, m, mut removed| {
        let name = obs(&mut removed).database.test_name();
        let mut bytes = fs::read(f.root.join(Q)).unwrap();
        bytes.push(b' ');
        fs::write(f.root.join(Q), &bytes).unwrap();
        let before = tree(&f.root);
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::Verification(_)
        ));
        assert_eq!(failed.progress(), ResourceCloseProgress::Unchanged);
        assert!(matches!(*failed.retained, Retained::Before(_)));
        guards(f, m);
        assert!(registered(&name));
        blocks_commit(f);
        assert_eq!(tree(&f.root), before);
        drop(failed);
    });
}

// Keep a real native statement alive across owner Drop. A successful step after
// Drop proves SQLite still uses the retained database; the Drop result log proves
// the real sqlite3_close returned BUSY, rather than merely skipping cleanup.
fn protection_statement(
    removed: &mut RecoveryDirectoryRemoved<'_, '_>,
) -> (std::path::PathBuf, *mut ffi::sqlite3_stmt) {
    let protection = &obs(removed).protection;
    let staging = protection.database_path();
    let mut stmt = ptr::null_mut();
    assert_eq!(
        unsafe {
            ffi::sqlite3_prepare_v2(
                protection.source().unwrap().handle(),
                c"SELECT 1".as_ptr(),
                -1,
                &mut stmt,
                ptr::null_mut(),
            )
        },
        ffi::SQLITE_OK
    );
    (staging, stmt)
}
fn retained_after_busy_drop(staging: &std::path::Path, stmt: *mut ffi::sqlite3_stmt) {
    let results = crate::backup::restore::close_results::take();
    assert!(
        results.contains(&ffi::SQLITE_BUSY),
        "real close must have returned SQLITE_BUSY: {results:?}"
    );
    assert!(
        staging.is_file(),
        "staging must survive a failed close during Drop"
    );
    assert_eq!(unsafe { ffi::sqlite3_step(stmt) }, ffi::SQLITE_ROW);
    assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
    // The owner already had to retain the failed connection conservatively.
    // Finalizing now does not retroactively authorize automatic file deletion.
    assert!(staging.is_file());
}
#[test]
fn earlier_active_close_error_preserves_busy_protection_staging_on_drop() {
    scenario(|f, _, m, mut removed| {
        let (staging, stmt) = protection_statement(&mut removed);
        obs(&mut removed)
            .database
            .test_close_error(ffi::SQLITE_IOERR);
        let before = tree(&f.root);
        crate::backup::restore::close_results::take();
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::ActiveSqlite {
                code: Some(ffi::SQLITE_IOERR),
                ..
            }
        ));
        assert_eq!(failed.progress(), ResourceCloseProgress::Unchanged);
        guards(f, m);
        assert!(staging.is_file());
        drop(failed);
        retained_after_busy_drop(&staging, stmt);
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn earlier_unregister_error_preserves_busy_protection_staging_on_drop() {
    scenario(|f, _, m, mut removed| {
        let (staging, stmt) = protection_statement(&mut removed);
        obs(&mut removed).database.test_unregister_failure();
        let before = tree(&f.root);
        crate::backup::restore::close_results::take();
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(failed.error(), ResourceCloseError::VfsUnregister));
        assert_eq!(failed.progress(), ResourceCloseProgress::ActiveSqliteClosed);
        guards(f, m);
        drop(failed);
        retained_after_busy_drop(&staging, stmt);
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn cancellation_before_close_preserves_busy_protection_staging_on_drop() {
    scenario(|f, _, m, mut removed| {
        let (staging, stmt) = protection_statement(&mut removed);
        obs(&mut removed).cancel.store(true, Ordering::Release);
        let before = tree(&f.root);
        crate::backup::restore::close_results::take();
        let failed = removed.close_resources().err().unwrap();
        assert!(matches!(
            failed.error(),
            ResourceCloseError::Verification(Reason::Cancelled)
        ));
        assert!(matches!(*failed.retained, Retained::Before(_)));
        guards(f, m);
        drop(failed);
        retained_after_busy_drop(&staging, stmt);
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn panic_before_protection_close_preserves_busy_staging_on_drop() {
    for point in [
        "resources-before-active-close",
        "resources-after-active-close",
    ] {
        scenario(|f, _, m, mut removed| {
            let (staging, stmt) = protection_statement(&mut removed);
            let before = tree(&f.root);
            let hit = Arc::new(AtomicBool::new(false));
            let reached = hit.clone();
            crate::backup::restore::close_results::take();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |name| {
                        if name == point {
                            reached.store(true, Ordering::Release);
                            panic!("before protection close");
                        }
                        Ok(())
                    },
                    |_, p, f| durability::sync_opened(p, f),
                    || removed.close_resources(),
                )
            }));
            assert!(panic.is_err());
            assert!(hit.load(Ordering::Acquire));
            retained_after_busy_drop(&staging, stmt);
            assert_eq!(tree(&f.root), before);
            let session = m.recovery_session().unwrap();
            session.ensure_publication_phase().unwrap();
            drop(session);
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        });
    }
}
#[test]
fn confirmed_protection_close_cleans_staging_without_leaking_on_success_or_failure_drop() {
    for fail_first in [false, true] {
        scenario(|f, _, m, mut removed| {
            // No outstanding statement remains: Drop is allowed to clean staging
            // even when a preceding active-close error prevented explicit close.
            let staging = obs(&mut removed).protection.database_path();
            if fail_first {
                obs(&mut removed)
                    .database
                    .test_close_error(ffi::SQLITE_IOERR);
            }
            let before = tree(&f.root);
            crate::backup::restore::close_results::take();
            let result = removed.close_resources();
            assert_eq!(result.is_err(), fail_first);
            guards(f, m);
            assert!(staging.is_file());
            drop(result);
            assert!(
                !staging.exists(),
                "confirmed close should permit normal staging cleanup"
            );
            assert_eq!(
                crate::backup::restore::close_results::take(),
                [ffi::SQLITE_OK]
            );
            assert_eq!(tree(&f.root), before);
        });
    }
}
#[test]
fn cancellation_after_last_check_may_finish_resource_close_without_unlocking() {
    scenario(|f, _, m, mut removed| {
        let cancel = obs(&mut removed).cancel.clone();
        let before = tree(&f.root);
        let hit = Arc::new(AtomicBool::new(false));
        let reached = hit.clone();
        let closed = hooks::with(
            move |name| {
                if name == "resources-after-active-close" {
                    reached.store(true, Ordering::Release);
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            },
            |_, p, f| durability::sync_opened(p, f),
            || removed.close_resources(),
        )
        .unwrap();
        assert!(hit.load(Ordering::Acquire));
        guards(f, m);
        assert_eq!(tree(&f.root), before);
        drop(closed);
    });
}

#[test]
fn panic_inside_prepared_close_after_guard_preserves_live_sqlite_and_staging() {
    use crate::backup::restore::close_results;
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let protection = &obs(&mut removed).protection;
        let staging = protection.database_path();
        let bytes = fs::read(&staging).unwrap();
        let database = unsafe { protection.source().unwrap().handle() };
        let mut statement = ptr::null_mut();
        assert_eq!(
            unsafe {
                ffi::sqlite3_prepare_v2(
                    database,
                    c"SELECT title FROM journal_entries ORDER BY id".as_ptr(),
                    -1,
                    &mut statement,
                    ptr::null_mut(),
                )
            },
            ffi::SQLITE_OK
        );
        close_results::take();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            close_results::with_guard_panic(|| removed.close_resources())
        }));
        let payload = match panic {
            Err(payload) => payload,
            Ok(_) => panic!("close_inner did not panic"),
        };
        assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("injected panic after StagingDuringClose")
        );
        assert_eq!(
            close_results::guard_visits(),
            1,
            "PreparedRestore must not try closing again during unwind"
        );
        assert!(
            close_results::take().is_empty(),
            "explicit close was never confirmed"
        );
        assert!(staging.is_file());
        assert_eq!(fs::read(&staging).unwrap(), bytes);
        // The local Connection's unwind Drop attempted sqlite3_close, which
        // must leave the native connection live while this statement exists.
        assert_eq!(unsafe { ffi::sqlite3_db_handle(statement) }, database);
        assert_eq!(unsafe { ffi::sqlite3_step(statement) }, ffi::SQLITE_ROW);
        let title =
            unsafe { std::ffi::CStr::from_ptr(ffi::sqlite3_column_text(statement, 0).cast()) };
        assert_eq!(title.to_bytes(), b"Old");
        // Test-only cleanup of the retained native connection: each handle is
        // finalized/closed exactly once. No Rust owner remains to close it later.
        assert_eq!(unsafe { ffi::sqlite3_finalize(statement) }, ffi::SQLITE_OK);
        assert_eq!(unsafe { ffi::sqlite3_close(database) }, ffi::SQLITE_OK);
        assert!(
            staging.is_file(),
            "unwind retention must not trigger late staging cleanup"
        );
        assert_eq!(tree(&f.root), before);
        let session = m.recovery_session().unwrap();
        session.ensure_publication_phase().unwrap();
        drop(session);
        assert!(m.ensure_usable().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    });
    // Positive control: without the panic, confirmed close still permits cleanup.
    scenario(|f, _, m, mut removed| {
        let before = tree(&f.root);
        let staging = obs(&mut removed).protection.database_path();
        close_results::take();
        let closed = removed.close_resources().unwrap();
        assert_eq!(close_results::take(), [ffi::SQLITE_OK]);
        guards(f, m);
        drop(closed);
        assert!(!staging.exists());
        assert_eq!(tree(&f.root), before);
    });
}
