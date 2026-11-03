//! Ownership only. No admission, generation change or SQL access.
use super::super::super::super::super::resource_close::ClosedFiles;
use super::*;
use crate::{
    backup::restore_context::close::ClosedContextFiles,
    database_state::{
        Maintenance,
        recovery_session::{DetachedSession, DetachedSessionBinding, RecoverySession},
    },
};

// Preserve files-before-reservation destruction, including on unwind.
struct Detached<'s> {
    _marker: Option<RetainedMarker>,
    _context: Option<ClosedContextFiles<'s>>,
    _files: ClosedFiles,
    session: DetachedSession<'s>,
}
pub(crate) struct RunningHandoff<'s>(Detached<'s>);
pub(crate) struct StartupHandoff<'s>(Detached<'s>);
#[derive(Debug)]
pub(crate) enum HandoffError {
    Binding,
    Files(FileCheckError),
    Checkpoint(io::Error),
}
// Every stage has one complete aggregate owner. Only closed resources enter it.
enum Stage<'m, 's> {
    Disk(DiskRecoveryCompleted<'m, 's>),
    Unwrapped {
        marker: Option<RetainedMarker>,
        context: Option<Box<ContextConnectionsClosed<'m, 's>>>,
        resources: ResourcesClosed<'m, 's>,
    },
    Context {
        marker: Option<RetainedMarker>,
        context: Option<ClosedContextFiles<'s>>,
        resources: ResourcesClosed<'m, 's>,
    },
    Files {
        marker: Option<RetainedMarker>,
        context: Option<ClosedContextFiles<'s>>,
        files: ClosedFiles,
        session: RecoverySession<'m, 's>,
    },
    Detached(Detached<'s>),
}
pub(crate) struct HandoffFailure<'m, 's> {
    stage: Box<Stage<'m, 's>>,
    error: HandoffError,
}
impl std::fmt::Debug for HandoffFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandoffFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
struct Owner<'m, 's> {
    stage: Option<Stage<'m, 's>>,
}
impl Drop for Owner<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::mem::forget(self.stage.take());
        }
    }
}
impl<'m, 's> Owner<'m, 's> {
    fn step(&mut self, binding: &mut Option<DetachedSessionBinding<'s>>) {
        // No fallible work, allocation or callbacks between take and installation.
        self.stage = Some(match self.stage.take().expect("handoff owner") {
            Stage::Disk(mut disk) => {
                let mut q = disk.unlinked.take().expect("disk owner");
                let Retained { marker, mut all } = q.retained.take().expect("unlinked owner");
                match all.held.take().expect("combined owner") {
                    Held::Running { context, resources } => Stage::Unwrapped {
                        marker,
                        context: Some(context),
                        resources,
                    },
                    Held::Startup { resources } => Stage::Unwrapped {
                        marker,
                        context: None,
                        resources,
                    },
                }
            }
            Stage::Unwrapped {
                marker,
                context,
                resources,
            } => Stage::Context {
                marker,
                context: context.map(|c| (*c).into_closed_files()),
                resources,
            },
            Stage::Context {
                marker,
                context,
                mut resources,
            } => {
                let (files, access) =
                    (*resources.closed.take().expect("closed owner")).into_closed_parts();
                let classification::Access::Session(session) = access else {
                    unreachable!("verified session")
                };
                Stage::Files {
                    marker,
                    context,
                    files,
                    session,
                }
            }
            Stage::Files {
                marker,
                context,
                files,
                session,
            } => Stage::Detached(Detached {
                _marker: marker,
                _context: context,
                _files: files,
                session: session.into_detached_session(binding.take().expect("verified binding")),
            }),
            Stage::Detached(done) => Stage::Detached(done),
        });
    }
}
enum Route<'r, 's> {
    Running(&'r Maintenance<'s>),
    Startup,
}
fn transfer<'m, 's>(
    disk: DiskRecoveryCompleted<'m, 's>,
    route: Route<'_, 's>,
) -> Result<Detached<'s>, HandoffFailure<'m, 's>> {
    let mut owner = Owner {
        stage: Some(Stage::Disk(disk)),
    };
    let checked = (|| {
        event("handoff-before-check").map_err(HandoffError::Checkpoint)?;
        let Stage::Disk(disk) = owner.stage.as_ref().expect("handoff owner") else {
            unreachable!()
        };
        let q = disk.unlinked.as_ref().expect("disk owner");
        file_checks::check_unlinked(q).map_err(HandoffError::Files)?;
        let (all, _) = q.post_unlink_parts();
        let view = all.held().resources().closed().view();
        let classification::Access::Session(session) = view.access else {
            return Err(HandoffError::Binding);
        };
        match (all.held(), route) {
            (Held::Running { .. }, Route::Running(m)) => {
                session.bind_context(m).map_err(|_| HandoffError::Binding)?;
            }
            (Held::Startup { .. }, Route::Startup) => {
                session
                    .startup_generation()
                    .map_err(|_| HandoffError::Binding)?;
            }
            _ => return Err(HandoffError::Binding),
        }
        let binding = session
            .detach_binding()
            .map_err(|_| HandoffError::Binding)?;
        event("handoff-after-check").map_err(HandoffError::Checkpoint)?;
        file_checks::check_unlinked(q).map_err(HandoffError::Files)?;
        Ok(binding)
    })();
    let mut binding = match checked {
        Ok(b) => Some(b),
        Err(error) => {
            return Err(HandoffFailure {
                stage: Box::new(owner.stage.take().expect("handoff owner")),
                error,
            });
        }
    };
    for point in [
        "handoff-unwrapped",
        "handoff-context",
        "handoff-files",
        "handoff-detached",
    ] {
        owner.step(&mut binding);
        if let Err(error) = event(point) {
            return Err(HandoffFailure {
                stage: Box::new(owner.stage.take().expect("handoff owner")),
                error: HandoffError::Checkpoint(error),
            });
        }
    }
    let Stage::Detached(done) = owner.stage.take().expect("handoff owner") else {
        unreachable!()
    };
    Ok(done)
}
// Consuming handoff entry methods.
impl<'m, 's> DiskRecoveryCompleted<'m, 's> {
    pub(crate) fn into_running_handoff(
        self,
        maintenance: &Maintenance<'s>,
    ) -> Result<RunningHandoff<'s>, HandoffFailure<'m, 's>> {
        transfer(self, Route::Running(maintenance)).map(RunningHandoff)
    }
    pub(crate) fn into_startup_handoff(self) -> Result<StartupHandoff<'s>, HandoffFailure<'m, 's>> {
        transfer(self, Route::Startup).map(StartupHandoff)
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

impl RunningHandoff<'_> {
    pub(in crate::backup::restore_context::pending) fn require_receipt(
        &self,
        receipt: &crate::backup::restore_context::pending::completion::LoadedReceipt,
    ) -> Result<(), crate::backup::restore_context::pending::completion::ReceiptError> {
        let (_, files, _) = self.0._files.view();
        match files.evidence.receipt.as_ref() {
            Some(actual) if std::ptr::eq(actual, receipt) => Ok(()),
            _ => Err(crate::backup::restore_context::pending::completion::ReceiptError::Binding),
        }
    }
}
impl StartupHandoff<'_> {
    pub(in crate::backup::restore_context::pending) fn require_receipt(
        &self,
        receipt: &crate::backup::restore_context::pending::completion::LoadedReceipt,
    ) -> Result<(), crate::backup::restore_context::pending::completion::ReceiptError> {
        let (_, files, _) = self.0._files.view();
        match files.evidence.receipt.as_ref() {
            Some(actual) if std::ptr::eq(actual, receipt) => Ok(()),
            _ => Err(crate::backup::restore_context::pending::completion::ReceiptError::Binding),
        }
    }
}
pub(in crate::backup::restore_context::pending) mod admission;

pub(in crate::backup::restore_context::pending) mod startup_admission;
