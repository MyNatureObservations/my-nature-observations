//! Internal native-VFS writer. No application entry or completion authority.
//! Metadata checks implement the cooperative local-directory threat model;
//! they are NOT descriptor binding or protection against adversarial pathname ABA.
use super::*;

impl<'m, 's> PreparedWriteEpisode<'m, 's> {
    fn open_writer(
        self,
        cancel: &AtomicBool,
    ) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Box::new(ActiveCloseFailure {
                owner: self.inputs.inputs.owner,
                error: ActiveCloseError::WriterCancelled,
            }));
        }
        let mut episode = self.begin_writer()?;
        let result = episode.open_writer_inner(cancel);
        match result {
            Ok(()) => Ok(episode),
            Err(error) => Err(Box::new(ActiveCloseFailure {
                owner: episode.inputs.inputs.owner,
                error,
            })),
        }
    }

    fn open_writer_inner(
        &mut self,
        cancel: &AtomicBool,
    ) -> std::result::Result<(), ActiveCloseError> {
        use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
        use std::sync::atomic::Ordering;
        let owner = &mut self.inputs.inputs.owner;
        if !owner.terminal_policy.is_some_and(TerminalPolicy::is_armed) {
            return Err(ActiveCloseError::Binding);
        }
        let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
        if !matches!(parts.writer, WriterState::Opening) {
            return Err(ActiveCloseError::Binding);
        }
        let ActiveState::Closed(active) = &parts.active else {
            return Err(ActiveCloseError::Binding);
        };
        #[cfg(test)]
        opening_probe::point(OpeningPoint::Armed);
        if cancel.load(Ordering::Acquire) {
            return Err(ActiveCloseError::WriterCancelled);
        }
        // Name and original A descriptor are checked before acquiring another fd.
        // This is point-in-time validation, not an ABA-resistant binding.
        check_target(active)?;
        no_sidecars(&active.path)?;
        let size = active
            .file
            .metadata()
            .map_err(ActiveCloseError::WriterOpenIo)?
            .len();
        let anchor = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&active.path)
            .map_err(ActiveCloseError::WriterOpenIo)?;
        // Store even a rejected descriptor before any fallible post-open check.
        parts.writer = WriterState::Owned {
            connection: None,
            anchor,
            outcome: None,
        };
        let WriterState::Owned {
            connection, anchor, ..
        } = &mut parts.writer
        else {
            unreachable!()
        };
        #[cfg(test)]
        opening_probe::point(OpeningPoint::Anchor);
        if private::file_identity(anchor).map_err(ActiveCloseError::WriterOpenIo)?
            != active.identity
            || anchor
                .metadata()
                .map_err(ActiveCloseError::WriterOpenIo)?
                .nlink()
                != 1
        {
            return Err(ActiveCloseError::WriterIdentity);
        }
        let mut header = [0u8; 100];
        anchor
            .read_exact_at(&mut header, 0)
            .map_err(ActiveCloseError::WriterOpenIo)?;
        if &header[..16] != b"SQLite format 3\0"
            || header[18..20] != [1, 1]
            || header[60..64] != [0, 0, 0, 0]
        {
            return Err(ActiveCloseError::WriterIdentity);
        }
        check_target(active)?;
        no_sidecars(&active.path)?;
        if cancel.load(Ordering::Acquire) {
            return Err(ActiveCloseError::WriterCancelled);
        }
        #[cfg(test)]
        opening_probe::point(OpeningPoint::BeforeSqlite);
        // No CREATE, URI or shared cache. Explicit bundled native unix VFS.
        // Do NOT execute SQL here: even a read can trigger hot-journal recovery.
        *connection = Some(
            Connection::open_with_flags_and_vfs(
                &active.path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE
                    | OpenFlags::SQLITE_OPEN_NOFOLLOW,
                "unix",
            )
            .map_err(ActiveCloseError::WriterOpenSqlite)?,
        );
        #[cfg(test)]
        opening_probe::point(OpeningPoint::Sqlite);
        let db = connection.as_ref().expect("retained connection");
        db.busy_timeout(std::time::Duration::ZERO)
            .map_err(ActiveCloseError::WriterOpenSqlite)?;
        // Native file control performs no SQL/schema access. It is an additional
        // name observation, not proof of immutable identity across observations.
        let mut moved: std::ffi::c_int = -1;
        let rc = unsafe {
            rusqlite::ffi::sqlite3_file_control(
                db.handle(),
                c"main".as_ptr(),
                rusqlite::ffi::SQLITE_FCNTL_HAS_MOVED,
                (&mut moved as *mut std::ffi::c_int).cast(),
            )
        };
        // sqlite3_db_readonly inspects pager flags without starting a transaction.
        // Native unix may fall back to READONLY after an EACCES on READWRITE.
        let readonly = unsafe { rusqlite::ffi::sqlite3_db_readonly(db.handle(), c"main".as_ptr()) };
        if rc != rusqlite::ffi::SQLITE_OK || moved != 0 || readonly != 0 {
            return Err(ActiveCloseError::WriterIdentity);
        }
        check_target(active)?;
        no_sidecars(&active.path)?;
        if anchor
            .metadata()
            .map_err(ActiveCloseError::WriterOpenIo)?
            .len()
            != size
        {
            return Err(ActiveCloseError::WriterIdentity);
        }
        if cancel.load(Ordering::Acquire) {
            return Err(ActiveCloseError::WriterCancelled);
        }
        Ok(())
    }

    fn begin_writer(mut self) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        let result = (|| {
            let owner = &mut self.inputs.inputs.owner;
            let policy = owner.terminal_policy.ok_or(ActiveCloseError::Binding)?;
            let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
            if !policy.is_never_armed() || !matches!(parts.writer, WriterState::Unopened) {
                return Err(ActiveCloseError::Binding);
            }
            // Pre-lock only. No resource has been acquired, extracted or opened.
            self.inputs
                .check_before_writer()
                .map_err(|_| ActiveCloseError::Binding)?;
            let owner = &mut self.inputs.inputs.owner;
            let policy = owner.terminal_policy.ok_or(ActiveCloseError::Binding)?;
            policy.arm_for_writer();
            owner.after.as_mut().expect("validated owner").writer = WriterState::Opening;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(Box::new(ActiveCloseFailure {
                owner: self.inputs.inputs.owner,
                error,
            })),
        }
    }

    fn close_writer(mut self) -> std::result::Result<Self, Box<ActiveCloseFailure<'m, 's>>> {
        let result = self.close_writer_inner();
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err(Box::new(ActiveCloseFailure {
                owner: self.inputs.inputs.owner,
                error,
            })),
        }
    }
    // Borrowed internal operation lets negative subprocess controls retain the
    // outer owner outside catch_unwind; production callers consume self above.
    fn close_writer_inner(&mut self) -> std::result::Result<(), ActiveCloseError> {
        let owner = &mut self.inputs.inputs.owner;
        if !owner.terminal_policy.is_some_and(TerminalPolicy::is_armed) {
            return Err(ActiveCloseError::Binding);
        }
        let parts = owner.after.as_mut().ok_or(ActiveCloseError::Binding)?;
        if matches!(parts.copy, CopyState::InProgress | CopyState::Failed) {
            return Err(ActiveCloseError::Binding);
        }
        let WriterState::Owned {
            connection,
            outcome,
            ..
        } = &mut parts.writer
        else {
            return Err(ActiveCloseError::Binding);
        };
        // Failed close is retained, never retried or implicitly dropped.
        if connection.is_none() || outcome.is_some() {
            return Err(ActiveCloseError::Binding);
        }
        #[cfg(test)]
        {
            *outcome = Some(probe::close(connection.take().expect("validated writer")));
        }
        #[cfg(not(test))]
        {
            *outcome = Some(connection.take().expect("validated writer").close());
        }
        // Consuming-call temporaries are covered by the app's pre-unwind
        // policy. The real returned owner is stored before inspection.
        #[cfg(test)]
        probe::after_outcome();
        if outcome.as_ref().expect("close result").is_err() {
            return Err(ActiveCloseError::WriterClose);
        }
        // Infallible empty-slot handoff. The anchor stays in the same armed
        // aggregate; all A/B/C/P/session resources are untouched.
        let WriterState::Owned { anchor, .. } =
            std::mem::replace(&mut parts.writer, WriterState::Opening)
        else {
            unreachable!()
        };
        parts.writer = WriterState::Closed { _anchor: anchor };
        Ok(())
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use std::cell::Cell;
    thread_local! {
        pub(super) static ERROR: Cell<bool> = const { Cell::new(false) };
        pub(super) static PANIC: Cell<bool> = const { Cell::new(false) };
    }
    pub(super) fn close(
        connection: Connection,
    ) -> std::result::Result<(), (Connection, rusqlite::Error)> {
        if ERROR.with(|v| v.replace(false)) {
            return Err((
                connection,
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
                    None,
                ),
            ));
        }
        connection.close()
    }
    pub(super) fn after_outcome() {
        if PANIC.with(|v| v.replace(false)) {
            panic!("writer outcome checkpoint");
        }
    }
}

#[cfg(test)]
mod tests;

fn check_target(active: &ActiveFiles) -> std::result::Result<(), ActiveCloseError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(&active.path).map_err(ActiveCloseError::WriterOpenIo)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || private::file_identity(&active.file).map_err(ActiveCloseError::WriterOpenIo)?
            != active.identity
        || !private::same_file(&active.file, &active.path)
            .map_err(ActiveCloseError::WriterOpenIo)?
    {
        return Err(ActiveCloseError::WriterIdentity);
    }
    Ok(())
}
fn no_sidecars(path: &Path) -> std::result::Result<(), ActiveCloseError> {
    if crate::startup::artifacts::sidecars(path, &mut |p| fs::symlink_metadata(p))
        .map_err(ActiveCloseError::WriterOpenIo)?
    {
        return Err(ActiveCloseError::WriterIdentity);
    }
    Ok(())
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpeningPoint {
    Armed,
    Anchor,
    BeforeSqlite,
    Sqlite,
}
#[cfg(test)]
mod opening_probe {
    use super::*;
    use std::cell::RefCell;
    type Hook = Box<dyn FnMut(OpeningPoint)>;
    thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }
    pub(super) fn with<T>(
        hook: impl FnMut(OpeningPoint) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset(Option<Hook>);
        impl Drop for Reset {
            fn drop(&mut self) {
                HOOK.with(|h| *h.borrow_mut() = self.0.take());
            }
        }
        let _reset = Reset(HOOK.with(|h| h.borrow_mut().replace(Box::new(hook))));
        action()
    }
    pub(super) fn point(point: OpeningPoint) {
        HOOK.with(|h| {
            if let Some(hook) = h.borrow_mut().as_mut() {
                hook(point);
            }
        });
    }
}
#[cfg(test)]
mod opening_tests;

mod copying;

mod completion;
