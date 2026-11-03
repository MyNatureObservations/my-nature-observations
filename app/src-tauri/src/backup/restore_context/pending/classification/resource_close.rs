//! Consuming resource shutdown while Q remains the durable recovery evidence.
use super::super::super::{
    Access, Inputs, Observation, OperationBinding,
    bound_sqlite::{BoundCloseFailure, BoundSqlite, CloseError, ClosedBinding},
};
use super::*;
use crate::backup::restore::{ClosedPreparedRestore, PreparedCloseFailure, PreparedRestore};
use std::sync::{Arc, atomic::AtomicBool};

// Field order is deliberate: SQLite/VFS and prepared databases before retained
// descriptors, and all of these before the borrowed session/reservation.
pub(super) struct Closed<'a, 's> {
    _active: ClosedBinding,
    _protection: Box<ClosedPreparedRestore>,
    binding: OperationBinding,
    files: Inputs,
    cancel: Arc<AtomicBool>,
    access: Access<'a, 's>,
}
// Read-only projection for the combination stage. No mutable production
// accessor can relabel the previously verified bindings or swap their owners.
pub(super) struct ClosedView<'v, 'a, 's> {
    pub(super) binding: &'v OperationBinding,
    pub(super) files: &'v Inputs,
    pub(super) cancel: &'v Arc<AtomicBool>,
    pub(super) access: &'v Access<'a, 's>,
}
impl<'a, 's> Closed<'a, 's> {
    #[cfg(test)]
    pub(super) fn retained_staging_path(&self) -> &std::path::Path {
        self._protection.retained_staging_path()
    }
    pub(super) fn view(&self) -> ClosedView<'_, 'a, 's> {
        ClosedView {
            binding: &self.binding,
            files: &self.files,
            cancel: &self.cancel,
            access: &self.access,
        }
    }
    #[cfg(test)]
    pub(super) fn test_parts_mut(&mut self) -> (&mut OperationBinding, &mut Inputs) {
        (&mut self.binding, &mut self.files)
    }
}
pub(super) enum Retained<'a, 's> {
    Before(RecoveryDirectoryRemoved<'a, 's>),
    Closing(Box<Closing<'a, 's>>),
}
pub(super) struct Closing<'a, 's> {
    active: Option<Active>,
    protection: Option<Protection>,
    binding: OperationBinding,
    files: Inputs,
    cancel: Arc<AtomicBool>,
    access: Access<'a, 's>,
}
enum Active {
    Live(Box<BoundSqlite>),
    Failed(BoundCloseFailure),
    Closed(ClosedBinding),
}
enum Protection {
    Live(Box<PreparedRestore>),
    Failed(PreparedCloseFailure),
    Closed(Box<ClosedPreparedRestore>),
}
fn sqlite_error(error: &rusqlite::Error, protection: bool) -> ResourceCloseError {
    let code = match error {
        rusqlite::Error::SqliteFailure(e, _) => Some(e.extended_code),
        _ => None,
    };
    if protection {
        ResourceCloseError::ProtectionSqlite {
            code,
            message: error.to_string(),
        }
    } else {
        ResourceCloseError::ActiveSqlite {
            code,
            message: error.to_string(),
        }
    }
}
fn before_check(removed: &RecoveryDirectoryRemoved<'_, '_>) -> Result<(), ResourceCloseError> {
    let a = &removed.receipt.proof.assessment;
    let access = a
        .access
        .as_ref()
        .ok_or(ResourceCloseError::Verification(Reason::Identity))?;
    match access {
        Access::Session(s) => s
            .ensure_publication_phase()
            .map_err(|_| ResourceCloseError::Verification(Reason::Identity))?,
        _ => return Err(ResourceCloseError::Verification(Reason::Identity)),
    }
    let o = a
        .observation
        .as_ref()
        .ok_or(ResourceCloseError::Verification(Reason::Identity))?;
    // Q must still exist; a disk-cleanup capability is not authority to ignore it.
    if o.files.evidence.receipt.is_none() {
        return Err(ResourceCloseError::Verification(Reason::Marker));
    }
    o.recheck(access).map_err(ResourceCloseError::Verification)
}
fn checkpoint(name: &str) -> Result<(), ResourceCloseError> {
    event(name).map_err(ResourceCloseError::Io)
}
impl Closing<'_, '_> {
    fn run(&mut self, progress: &mut ResourceCloseProgress) -> Result<(), ResourceCloseError> {
        checkpoint("resources-before-active-close")?;
        let Active::Live(active) = self.active.take().expect("active owner") else {
            unreachable!()
        };
        match (*active).try_close() {
            Ok(closed) => self.active = Some(Active::Closed(closed)),
            Err(failed) => {
                let error = match &failed.error {
                    CloseError::Sqlite(e) => sqlite_error(e, false),
                    CloseError::Unregister => {
                        *progress = ResourceCloseProgress::ActiveSqliteClosed;
                        ResourceCloseError::VfsUnregister
                    }
                };
                self.active = Some(Active::Failed(failed));
                return Err(error);
            }
        }
        *progress = ResourceCloseProgress::ActiveClosed;
        checkpoint("resources-after-active-close")?;
        let Protection::Live(protection) = self.protection.take().expect("protection owner") else {
            unreachable!()
        };
        match (*protection).try_close() {
            Ok(closed) => self.protection = Some(Protection::Closed(Box::new(closed))),
            Err(failed) => {
                let error = sqlite_error(&failed.error, true);
                self.protection = Some(Protection::Failed(failed));
                return Err(error);
            }
        }
        *progress = ResourceCloseProgress::ProtectionClosed;
        checkpoint("resources-after-protection-close")?;
        Ok(())
    }
}
pub(super) fn close<'a, 's>(
    mut removed: RecoveryDirectoryRemoved<'a, 's>,
) -> Result<ResourcesClosed<'a, 's>, ResourceCloseFailure<'a, 's>> {
    let error = if cfg!(target_os = "macos") {
        before_check(&removed).err()
    } else {
        Some(ResourceCloseError::Unsupported)
    };
    if let Some(error) = error {
        return Err(ResourceCloseFailure {
            retained: Box::new(Retained::Before(removed)),
            error,
            progress: ResourceCloseProgress::Unchanged,
        });
    }
    let a = &mut removed.receipt.proof.assessment;
    let Observation {
        database,
        protection,
        binding,
        files,
        cancel,
    } = a.observation.take().expect("verified observation");
    let access = a.access.take().expect("verified session");
    let mut closing = Closing {
        active: Some(Active::Live(Box::new(database))),
        protection: Some(Protection::Live(Box::new(protection))),
        binding,
        files,
        cancel,
        access,
    };
    // The consumed empty Assessment owns no guards; Closing now holds all of them.
    drop(removed);
    let mut progress = ResourceCloseProgress::Unchanged;
    if let Err(error) = closing.run(&mut progress) {
        return Err(ResourceCloseFailure {
            retained: Box::new(Retained::Closing(Box::new(closing))),
            error,
            progress,
        });
    }
    let Closing {
        active,
        protection,
        binding,
        files,
        cancel,
        access,
    } = closing;
    let Some(Active::Closed(active)) = active else {
        unreachable!()
    };
    let Some(Protection::Closed(protection)) = protection else {
        unreachable!()
    };
    Ok(ResourcesClosed {
        closed: Some(Box::new(Closed {
            _active: active,
            _protection: protection,
            binding,
            files,
            cancel,
            access,
        })),
    })
}
#[cfg(all(test, target_os = "macos"))]
#[path = "resource_close_tests.rs"]
mod tests;

// Inert closed owner retains every non-session field in the original order.
pub(super) struct ClosedFiles {
    _active: ClosedBinding,
    _protection: Box<ClosedPreparedRestore>,
    _binding: OperationBinding,
    _files: Inputs,
    _cancel: Arc<AtomicBool>,
}
impl<'a, 's> Closed<'a, 's> {
    pub(super) fn into_closed_parts(self) -> (ClosedFiles, Access<'a, 's>) {
        let Self {
            _active,
            _protection,
            binding,
            files,
            cancel,
            access,
        } = self;
        (
            ClosedFiles {
                _active,
                _protection,
                _binding: binding,
                _files: files,
                _cancel: cancel,
            },
            access,
        )
    }
}

#[cfg(test)]
pub(super) type TestClosedFiles = ClosedFiles;
#[cfg(test)]
impl<'a, 's> Closed<'a, 's> {
    pub(super) fn into_test_parts(self) -> (TestClosedFiles, Access<'a, 's>) {
        self.into_closed_parts()
    }
}

impl ClosedFiles {
    pub(super) fn view(&self) -> (&OperationBinding, &Inputs, &Arc<AtomicBool>) {
        (&self._binding, &self._files, &self._cancel)
    }
}
