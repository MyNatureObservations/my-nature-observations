//! One bounded Backup API pass, retaining both connections in the armed episode.
use super::*;
use crate::backup::journal_content::{self, Comparison, VerifiedJournal};
use rusqlite::ffi;
use std::{ptr::NonNull, sync::atomic::Ordering};

impl<'m, 's> PreparedWriteEpisode<'m, 's> {
    pub(super) fn copy_candidate(
        mut self,
        cancel: &AtomicBool,
    ) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        let result = self.copy_candidate_inner(cancel);
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(Box::new(ActiveCloseFailure {
                owner: self.inputs.inputs.owner,
                error,
            })),
        }
    }
    fn copy_candidate_inner(
        &mut self,
        cancel: &AtomicBool,
    ) -> std::result::Result<(), ActiveCloseError> {
        let owner = &mut self.inputs.inputs.owner;
        if !owner.terminal_policy.is_some_and(TerminalPolicy::is_armed) {
            return Err(ActiveCloseError::Binding);
        }
        let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
        if parts.copy != CopyState::NotStarted {
            return Err(ActiveCloseError::Binding);
        }
        let WriterState::Owned {
            connection: Some(target),
            outcome: None,
            ..
        } = &mut parts.writer
        else {
            return Err(ActiveCloseError::Binding);
        };
        let ActiveState::Closed(active) = &parts.active else {
            return Err(ActiveCloseError::Binding);
        };
        let source = parts
            .sources
            .live()?
            .incoming
            .as_ref()
            .ok_or(ActiveCloseError::Binding)?;
        // Every exit from this point blocks checked writer-close and any retry.
        parts.copy = CopyState::InProgress;
        let result = (|| {
            if cancel.load(Ordering::Acquire) {
                return Err(ActiveCloseError::WriterCancelled);
            }
            check_target(active)?;
            no_sidecars(&active.path)?;
            // The first SQL below may begin a read transaction. The cooperative
            // directory contract and existing Maintenance/lease still apply.
            if !target.is_autocommit() {
                return Err(ActiveCloseError::Binding);
            }
            let mode: String = target
                .pragma_query_value(Some("main"), "journal_mode", |r| r.get(0))
                .map_err(ActiveCloseError::WriterOpenSqlite)?;
            if mode != "delete" {
                return Err(ActiveCloseError::Binding);
            }
            target
                .execute_batch("PRAGMA synchronous=EXTRA; PRAGMA fullfsync=ON;")
                .map_err(ActiveCloseError::WriterOpenSqlite)?;
            let sync: i32 = target
                .pragma_query_value(None, "synchronous", |r| r.get(0))
                .map_err(ActiveCloseError::WriterOpenSqlite)?;
            let full: bool = target
                .pragma_query_value(None, "fullfsync", |r| r.get(0))
                .map_err(ActiveCloseError::WriterOpenSqlite)?;
            if sync != 3 || !full {
                return Err(ActiveCloseError::Binding);
            }
            let verified =
                VerifiedJournal::from_candidate(source, Arc::new(AtomicBool::new(false)))
                    .map_err(|_| ActiveCloseError::CopySource)?;
            let source_db = source.source().map_err(|_| ActiveCloseError::CopySource)?;
            let handle = BackupHandle::new(source_db, target)?;
            let mut done = false;
            let failure = loop {
                #[cfg(test)]
                probe::point(Point::BeforeStep);
                if cancel.load(Ordering::Acquire) {
                    break Some(ActiveCloseError::WriterCancelled);
                }
                let rc = handle.step();
                #[cfg(test)]
                let rc = probe::step(rc);
                if rc == ffi::SQLITE_DONE {
                    done = true; // SQLite commits inside step, BEFORE returning DONE.
                    #[cfg(test)]
                    probe::point(Point::Done);
                    break None;
                }
                if rc != ffi::SQLITE_OK {
                    break Some(ActiveCloseError::CopyStep(rc));
                }
                #[cfg(test)]
                probe::point(Point::AfterStep);
            };
            // finish also rolls back an incomplete backup. OK alone is NOT DONE.
            let finish = handle.finish();
            #[cfg(test)]
            let finish = probe::finish(finish);
            if finish != ffi::SQLITE_OK {
                return Err(ActiveCloseError::CopyFinish(finish));
            }
            if !done {
                return Err(failure.expect("incomplete backup"));
            }
            // Cancellation after DONE cannot claim the original target survived.
            // Complete verification regardless of a late cancellation request.
            let result = VerifiedJournal::verify(target, Arc::new(AtomicBool::new(false)))
                .map_err(|_| ActiveCloseError::CopyVerification)?;
            let comparison = journal_content::compare(&verified, &result);
            let finished = result.finish();
            if finished.is_err() || !matches!(comparison, Ok(Comparison::Identical)) {
                return Err(ActiveCloseError::CopyVerification);
            }
            verified
                .finish()
                .map_err(|_| ActiveCloseError::CopySource)?;
            check_target(active)?;
            no_sidecars(&active.path)?;
            Ok(())
        })();
        parts.copy = if result.is_ok() {
            CopyState::Verified
        } else {
            CopyState::Failed
        };
        result
    }
}
// The lifetime holds exclusive access to the destination until explicit finish.
// Unfinished destruction is fail-stop, not an unobserved backup_finish/rollback.
struct BackupHandle<'a> {
    raw: Option<NonNull<ffi::sqlite3_backup>>,
    _source: &'a Connection,
    _target: &'a mut Connection,
}
impl<'a> BackupHandle<'a> {
    fn new(
        source: &'a Connection,
        target: &'a mut Connection,
    ) -> std::result::Result<Self, ActiveCloseError> {
        // SAFETY: distinct episode-owned live connections, fixed database names;
        // no destination operation is available while this exclusive borrow lives.
        let raw = unsafe {
            ffi::sqlite3_backup_init(
                target.handle(),
                c"main".as_ptr(),
                source.handle(),
                c"main".as_ptr(),
            )
        };
        let raw = NonNull::new(raw).ok_or_else(|| {
            ActiveCloseError::CopyStep(unsafe { ffi::sqlite3_errcode(target.handle()) })
        })?;
        Ok(Self {
            raw: Some(raw),
            _source: source,
            _target: target,
        })
    }
    fn step(&self) -> i32 {
        unsafe { ffi::sqlite3_backup_step(self.raw.unwrap().as_ptr(), 1) }
    }
    fn finish(mut self) -> i32 {
        unsafe { ffi::sqlite3_backup_finish(self.raw.take().unwrap().as_ptr()) }
    }
}
impl Drop for BackupHandle<'_> {
    fn drop(&mut self) {
        if self.raw.is_some() {
            std::process::abort();
        }
    }
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    BeforeStep,
    AfterStep,
    Done,
}
#[cfg(test)]
mod probe {
    use super::*;
    use std::cell::{Cell, RefCell};
    thread_local! {
        static HOOK: RefCell<Option<Box<dyn FnMut(Point)>>> = const { RefCell::new(None) };
        static STEP_STATUS: Cell<i32> = const { Cell::new(0) };
        static FINISH_ERROR: Cell<bool> = const { Cell::new(false) };
    }
    pub(super) fn with<T>(
        hook: impl FnMut(Point) + 'static,
        finish_error: bool,
        step_status: i32,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset(Option<Box<dyn FnMut(Point)>>, bool, i32);
        impl Drop for Reset {
            fn drop(&mut self) {
                HOOK.with(|h| *h.borrow_mut() = self.0.take());
                FINISH_ERROR.with(|f| f.set(self.1));
                STEP_STATUS.with(|s| s.set(self.2));
            }
        }
        let _reset = Reset(
            HOOK.with(|h| h.borrow_mut().replace(Box::new(hook))),
            FINISH_ERROR.with(|f| f.replace(finish_error)),
            STEP_STATUS.with(|s| s.replace(step_status)),
        );
        action()
    }
    pub(super) fn point(p: Point) {
        HOOK.with(|h| {
            if let Some(hook) = h.borrow_mut().as_mut() {
                hook(p);
            }
        });
    }
    // Injection follows a real successful partial step. The original handle
    // is still explicitly finished; this does not simulate an OS disk failure.
    pub(super) fn step(rc: i32) -> i32 {
        let injected = STEP_STATUS.with(|s| s.replace(0));
        if rc == ffi::SQLITE_OK && injected != 0 {
            injected
        } else {
            rc
        }
    }
    // Inject only AFTER destroying the real backup handle; no fake leaked handle.
    pub(super) fn finish(rc: i32) -> i32 {
        if FINISH_ERROR.with(|f| f.get()) {
            ffi::SQLITE_IOERR
        } else {
            rc
        }
    }
}
#[cfg(test)]
mod tests;
