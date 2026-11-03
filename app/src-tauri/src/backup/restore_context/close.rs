//! Context-only shutdown. No recovery completion authority.
use super::*;
use crate::backup::{
    pre_restore::ProtectionRecord,
    restore::{ClosedPreparedRestore, PreparedCloseFailure},
};
use crate::database_state::recovery_session::{ContextSessionBinding, RecoverySession};
// Constructed before close. On unwind, retain descriptors rather than assume
// that SQLite released them. Safe Option transfers only; no manual field drops.
struct FilesDuringClose(Option<ActiveFiles>);
impl Drop for FilesDuringClose {
    fn drop(&mut self) {
        if let Some(files) = self.0.take() {
            std::mem::forget(files);
        }
    }
}
impl ActiveDatabase {
    pub(super) fn close_inner(&mut self) -> rusqlite::Result<ActiveFiles> {
        let connection = self.connection.take().expect("active connection");
        let mut files = FilesDuringClose(self.files.take());
        #[cfg(test)]
        hooks::point("active-close-guard");
        #[cfg(test)]
        let result = hooks::close(connection);
        #[cfg(not(test))]
        let result = connection.close();
        match result {
            Ok(()) => Ok(files.0.take().expect("active files")),
            Err((connection, error)) => {
                self.connection = Some(connection);
                self.files = files.0.take();
                Err(error)
            }
        }
    }
}
impl Drop for ActiveDatabase {
    fn drop(&mut self) {
        if self.connection.is_some() {
            match self.close_inner() {
                Ok(files) => drop(files),
                Err(_) => {
                    std::mem::forget(self.connection.take());
                    std::mem::forget(self.files.take());
                }
            }
        } else {
            std::mem::forget(self.files.take());
        }
    }
}
#[derive(Debug)]
pub(crate) enum ContextCloseError {
    Binding,
    SessionBinding(String),
    Active(rusqlite::Error),
    Incoming(rusqlite::Error),
    Protection(rusqlite::Error),
}
// Private fields; no SQL, Clone/Copy/Serde or cleanup/Ready authority.
pub(crate) struct ContextConnectionsClosed<'a, 's> {
    parts: Option<ContextClosed<'a, 's>>,
}
// Field order is unchanged: closed files precede borrowed bindings.
struct ContextClosed<'a, 's> {
    _writer_anchor: Option<std::fs::File>,
    _active: ActiveFiles,
    _incoming: ClosedPreparedRestore,
    _protection: ClosedPreparedRestore,
    _record: ProtectionRecord<'a, 's>,
    _binding: ContextBinding<'a, 's>,
    _generation: u32,
    _maintenance: &'a Maintenance<'s>,
}
impl<'a, 's> ContextConnectionsClosed<'a, 's> {
    #[cfg(test)]
    pub(crate) fn test_maintenance(&self) -> &'a Maintenance<'s> {
        self.parts()._maintenance
    }
    fn parts(&self) -> &ContextClosed<'a, 's> {
        self.parts.as_ref().expect("context closed owner")
    }
    #[cfg(test)]
    pub(super) fn test_corrupt_binding(&mut self, field: u8) {
        let parts = self.parts.as_mut().expect("context closed owner");
        match field {
            0 => parts._generation += 1,
            1 => parts._binding.restore_id = uuid::Uuid::new_v4(),
            _ => parts
                ._binding
                .session
                .test_set_session_id(uuid::Uuid::new_v4()),
        }
    }
    // A comparison only, never a factory or authority from free IDs. The sole
    // production caller supplies bindings retained by ResourcesClosed.
    pub(super) fn matches_resources(
        &self,
        session: &RecoverySession<'_, '_>,
        restore_id: uuid::Uuid,
        directory: (u64, u64),
        database: (u64, u64),
        database_path: &Path,
    ) -> Result<()> {
        self.parts()
            ._binding
            .session
            .check_session(session)
            .map_err(ContextError::Coordination)?;
        if self.parts()._binding.restore_id != restore_id
            || session.maintenance_binding() != Some((restore_id, self.parts()._generation))
            || session
                .directory_identity()
                .map_err(ContextError::Coordination)?
                != directory
            || self.parts()._binding.database.components() != database
            || self.parts()._active.identity.components() != database
            || self.parts()._active.path != database_path
        {
            return Err(ContextError::InvalidProof);
        }
        Ok(())
    }
}
impl Drop for ContextConnectionsClosed<'_, '_> {
    fn drop(&mut self) {}
}
// All live owners implement controlled Drop. Earlier errors and unwinding
// therefore use the same close-before-cleanup rule as explicit shutdown.
enum ActiveOwner {
    Live(ActiveDatabase),
    Closed(ActiveFiles),
}
enum PreparedOwner {
    Live(PreparedRestore),
    Closed(ClosedPreparedRestore),
    Failed(PreparedCloseFailure),
}
struct ContextBinding<'a, 's> {
    session: ContextSessionBinding<'a, 's>,
    restore_id: uuid::Uuid,
    database: FileIdentity,
}
struct Closing<'a, 's> {
    active: Option<ActiveOwner>,
    incoming: Option<PreparedOwner>,
    protection: Option<PreparedOwner>,
    record: ProtectionRecord<'a, 's>,
    binding: ContextBinding<'a, 's>,
    generation: u32,
    maintenance: &'a Maintenance<'s>,
}
enum Retained<'a, 's> {
    Before(RestoreContext<'a, 's>),
    Closing(Closing<'a, 's>),
}
pub(crate) struct ContextCloseFailure<'a, 's> {
    error: ContextCloseError,
    _retained: Box<Retained<'a, 's>>,
}
impl Drop for ContextCloseFailure<'_, '_> {
    fn drop(&mut self) {}
}
impl std::fmt::Debug for ContextCloseFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextCloseFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl ContextCloseFailure<'_, '_> {
    pub(crate) fn error(&self) -> &ContextCloseError {
        &self.error
    }
}
impl<'a, 's> RestoreContext<'a, 's> {
    pub(crate) fn close_owned_connections(
        self,
        session: &RecoverySession<'_, '_>,
    ) -> std::result::Result<ContextConnectionsClosed<'a, 's>, ContextCloseFailure<'a, 's>> {
        let session_binding = match session.bind_context(self.maintenance) {
            Ok(binding) => binding,
            Err(error) => {
                return Err(ContextCloseFailure {
                    error: ContextCloseError::SessionBinding(error),
                    _retained: Box::new(Retained::Before(self)),
                });
            }
        };
        // Establish the binding before taking any connection out of its owner.
        // The retained ActiveDatabase identity must still match both its handle
        // and the database path assigned by the session's one DirectoryLease.
        let valid_path = self.active.path() == session_binding.database_path();
        let valid_identity = self.active.files.as_ref().is_some_and(|files| {
            private::file_identity(&files.file).is_ok_and(|identity| identity == files.identity)
        });
        if !valid_path
            || !valid_identity
            || session_binding.operation_id() != self.maintenance.operation_id()
            || session_binding.check_session(session).is_err()
            || self.active.ensure_current().is_err()
            || self.generation != self.maintenance.generation()
            || !self
                .protection
                .as_ref()
                .is_some_and(|p| p.is_bound_to(self.maintenance, &self.active))
        {
            return Err(ContextCloseFailure {
                error: ContextCloseError::Binding,
                _retained: Box::new(Retained::Before(self)),
            });
        }
        let binding = ContextBinding {
            restore_id: session_binding.operation_id(),
            database: self.active.identity(),
            session: session_binding,
        };
        let Self {
            active,
            incoming,
            protection,
            generation,
            maintenance,
            ..
        } = self;
        let (protection, record) = protection.expect("bound protection").into_parts();
        let mut closing = Closing {
            active: Some(ActiveOwner::Live(active)),
            incoming: Some(PreparedOwner::Live(incoming)),
            protection: Some(PreparedOwner::Live(protection)),
            record,
            binding,
            generation,
            maintenance,
        };
        #[cfg(test)]
        hooks::point("owners-transferred");
        if let Err(error) = closing.run() {
            return Err(ContextCloseFailure {
                error,
                _retained: Box::new(Retained::Closing(closing)),
            });
        }
        let Closing {
            active,
            incoming,
            protection,
            record,
            binding,
            generation,
            maintenance,
        } = closing;
        let Some(ActiveOwner::Closed(active)) = active else {
            unreachable!()
        };
        let Some(PreparedOwner::Closed(incoming)) = incoming else {
            unreachable!()
        };
        let Some(PreparedOwner::Closed(protection)) = protection else {
            unreachable!()
        };
        Ok(ContextConnectionsClosed {
            parts: Some(ContextClosed {
                _writer_anchor: None,
                _active: active,
                _incoming: incoming,
                _protection: protection,
                _record: record,
                _binding: binding,
                _generation: generation,
                _maintenance: maintenance,
            }),
        })
    }
}
impl Closing<'_, '_> {
    fn run(&mut self) -> std::result::Result<(), ContextCloseError> {
        let Some(ActiveOwner::Live(mut active)) = self.active.take() else {
            unreachable!()
        };
        match active.close_inner() {
            Ok(files) => self.active = Some(ActiveOwner::Closed(files)),
            Err(error) => {
                self.active = Some(ActiveOwner::Live(active));
                return Err(ContextCloseError::Active(error));
            }
        }
        #[cfg(test)]
        hooks::point("after-active");
        Self::close_prepared(&mut self.incoming).map_err(ContextCloseError::Incoming)?;
        #[cfg(test)]
        hooks::point("after-incoming");
        Self::close_prepared(&mut self.protection).map_err(ContextCloseError::Protection)?;
        Ok(())
    }
    fn close_prepared(owner: &mut Option<PreparedOwner>) -> rusqlite::Result<()> {
        let Some(PreparedOwner::Live(prepared)) = owner.take() else {
            unreachable!()
        };
        match prepared.try_close() {
            Ok(closed) => {
                *owner = Some(PreparedOwner::Closed(closed));
                Ok(())
            }
            Err(failed) => {
                let error = match &failed.error {
                    rusqlite::Error::SqliteFailure(code, message) => {
                        rusqlite::Error::SqliteFailure(*code, message.clone())
                    }
                    other => rusqlite::Error::InvalidParameterName(other.to_string()),
                };
                // Keep the original error owner and its connection/staging intact.
                *owner = Some(PreparedOwner::Failed(failed));
                Err(error)
            }
        }
    }
}
#[cfg(test)]
pub(super) mod hooks {
    use std::cell::{Cell, RefCell};
    thread_local! { static CLOSE_ERROR: Cell<Option<i32>> = const { Cell::new(None) }; }
    // Test seam mirrors Connection::close, including its retained large error owner.
    #[expect(clippy::result_large_err)]
    pub(super) fn close(
        connection: rusqlite::Connection,
    ) -> Result<(), (rusqlite::Connection, rusqlite::Error)> {
        if let Some(code) = CLOSE_ERROR.take() {
            return Err((
                connection,
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(code),
                    Some("injected close error".into()),
                ),
            ));
        }
        connection.close()
    }
    pub(in crate::backup::restore_context) fn with_close_error<T>(
        code: i32,
        action: impl FnOnce() -> T,
    ) -> T {
        assert!(CLOSE_ERROR.replace(Some(code)).is_none());
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                CLOSE_ERROR.set(None);
            }
        }
        let _reset = Reset;
        action()
    }
    type Hook = Box<dyn FnMut(&str)>;
    thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }
    pub(super) fn point(name: &str) {
        HOOK.with(|h| {
            if let Some(h) = h.borrow_mut().as_mut() {
                h(name);
            }
        });
    }
    pub(in crate::backup::restore_context) fn with<T>(
        hook: impl FnMut(&str) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                HOOK.with(|h| *h.borrow_mut() = None);
            }
        }
        HOOK.with(|h| {
            assert!(h.borrow().is_none());
            *h.borrow_mut() = Some(Box::new(hook));
        });
        let _reset = Reset;
        action()
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

#[cfg(all(test, target_os = "macos"))]
mod session_tests;

// Only confirmed-closed resources enter this inert projection. Preserve original order.
pub(crate) struct ClosedContextFiles<'s> {
    _writer_anchor: Option<std::fs::File>,
    _active: ActiveFiles,
    _incoming: ClosedPreparedRestore,
    _protection: ClosedPreparedRestore,
    _record: crate::backup::pre_restore::ClosedProtectionRecord,
    _session: crate::database_state::recovery_session::ClosedContextBinding<'s>,
    _restore_id: uuid::Uuid,
    _database: FileIdentity,
    _generation: u32,
}
impl<'s> ContextConnectionsClosed<'_, 's> {
    pub(crate) fn into_closed_files(mut self) -> ClosedContextFiles<'s> {
        let ContextClosed {
            _writer_anchor,
            _active,
            _incoming,
            _protection,
            _record,
            _binding,
            _generation,
            _maintenance: _,
        } = self.parts.take().expect("closed context owner");
        let ContextBinding {
            session,
            restore_id,
            database,
        } = _binding;
        ClosedContextFiles {
            _writer_anchor,
            _active,
            _incoming,
            _protection,
            _record: _record.into_closed_record(),
            _session: session.into_closed_record(),
            _restore_id: restore_id,
            _database: database,
            _generation,
        }
    }
}

#[cfg(test)]
pub(crate) type TestContextFiles<'s> = ClosedContextFiles<'s>;
#[cfg(test)]
impl<'s> ContextConnectionsClosed<'_, 's> {
    pub(crate) fn into_test_files(self) -> TestContextFiles<'s> {
        self.into_closed_files()
    }
}

// Internal writer-episode bridge. All SQLite owners have already closed.
#[cfg(all(target_os = "macos", any(test, panic = "abort")))]
impl<'m, 's> ContextConnectionsClosed<'m, 's> {
    pub(in crate::backup::restore_context) fn from_episode(
        active: ActiveFiles,
        incoming: ClosedPreparedRestore,
        protection: ClosedPreparedRestore,
        record: ProtectionRecord<'m, 's>,
        session: ContextSessionBinding<'m, 's>,
        maintenance: &'m Maintenance<'s>,
        anchor: File,
    ) -> Self {
        let database = active.identity;
        Self {
            parts: Some(ContextClosed {
                _writer_anchor: Some(anchor),
                _active: active,
                _incoming: incoming,
                _protection: protection,
                _record: record,
                _binding: ContextBinding {
                    session,
                    restore_id: maintenance.operation_id(),
                    database,
                },
                _generation: maintenance.generation(),
                _maintenance: maintenance,
            }),
        }
    }
}
