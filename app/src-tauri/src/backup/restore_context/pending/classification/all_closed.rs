//! Join closed-resource capabilities only. No final content check or mutation.
use super::{ResourcesClosed, event};
use crate::backup::restore_context::close::ContextConnectionsClosed;
use std::io;

// Context files precede the classifier's files and its retained session. All
// SQLite connections are already confirmed closed before either constructor.
enum Held<'a, 's> {
    Running {
        context: Box<ContextConnectionsClosed<'a, 's>>,
        resources: ResourcesClosed<'a, 's>,
    },
    Startup {
        resources: ResourcesClosed<'a, 's>,
    },
}
pub(crate) struct AllConnectionsClosed<'a, 's> {
    held: Option<Held<'a, 's>>,
    generation: u32,
}
impl<'a, 's> AllConnectionsClosed<'a, 's> {
    fn held(&self) -> &Held<'a, 's> {
        self.held.as_ref().expect("all closed owner")
    }
}
impl Drop for AllConnectionsClosed<'_, '_> {
    fn drop(&mut self) {}
}
#[derive(Debug)]
pub(crate) enum AllCloseError {
    Unsupported,
    Binding,
    Session(String),
    Cancelled,
    Io(io::Error),
}
pub(crate) struct AllCloseFailure<'a, 's> {
    held: Held<'a, 's>,
    error: AllCloseError,
}
impl Drop for AllCloseFailure<'_, '_> {
    fn drop(&mut self) {}
}
impl std::fmt::Debug for AllCloseFailure<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AllCloseFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl AllCloseFailure<'_, '_> {
    pub(crate) fn error(&self) -> &AllCloseError {
        &self.error
    }
}
impl<'a, 's> ResourcesClosed<'a, 's> {
    pub(crate) fn combine_context(
        self,
        context: ContextConnectionsClosed<'a, 's>,
    ) -> Result<AllConnectionsClosed<'a, 's>, AllCloseFailure<'a, 's>> {
        combine(Held::Running {
            context: Box::new(context),
            resources: self,
        })
    }
    pub(crate) fn confirm_startup(
        self,
    ) -> Result<AllConnectionsClosed<'a, 's>, AllCloseFailure<'a, 's>> {
        combine(Held::Startup { resources: self })
    }
}
// Verification backend. Compiler probes retain all definitions above and stub
// only this backend; runtime tests exercise these actual binding checks.
fn combine<'a, 's>(
    held: Held<'a, 's>,
) -> Result<AllConnectionsClosed<'a, 's>, AllCloseFailure<'a, 's>> {
    let checked = (|| {
        event("all-close-before-binding").map_err(AllCloseError::Io)?;
        let generation = held.check()?;
        event("all-close-after-binding").map_err(AllCloseError::Io)?;
        // Cancellation or identity loss at the boundary cannot issue a capability.
        if held.check()? != generation {
            return Err(AllCloseError::Binding);
        }
        Ok(generation)
    })();
    match checked {
        Ok(generation) => Ok(AllConnectionsClosed {
            held: Some(held),
            generation,
        }),
        Err(error) => Err(AllCloseFailure { held, error }),
    }
}
impl<'a, 's> Held<'a, 's> {
    fn resources(&self) -> &ResourcesClosed<'a, 's> {
        match self {
            Self::Running { resources, .. } | Self::Startup { resources } => resources,
        }
    }
    fn check(&self) -> Result<u32, AllCloseError> {
        use super::super::super::{Access, Identity, identity};
        use std::sync::atomic::Ordering;
        if !cfg!(target_os = "macos") {
            return Err(AllCloseError::Unsupported);
        }
        let closed = self.resources().closed().view();
        if closed.cancel.load(Ordering::Acquire) {
            return Err(AllCloseError::Cancelled);
        }
        let Access::Session(session) = closed.access else {
            return Err(AllCloseError::Binding);
        };
        session
            .ensure_publication_phase()
            .map_err(AllCloseError::Session)?;
        // These are retained objects and previously verified bindings, not a new
        // journal verification or a new SQLite-open. No paths are modified.
        let directory = session
            .directory_identity()
            .map_err(AllCloseError::Session)?;
        let database_path = session.database_path().map_err(AllCloseError::Session)?;
        if closed.files.root.join("mno.sqlite") != database_path
            || closed.binding.directory != Identity::from_parts(directory.0, directory.1)
            || identity(&closed.files.root_file).map_err(|_| AllCloseError::Binding)?
                != closed.binding.directory
            || identity(&closed.files.active_file).map_err(|_| AllCloseError::Binding)?
                != closed.binding.database
        {
            return Err(AllCloseError::Binding);
        }
        match self {
            Self::Running { context, .. } => {
                let Some((operation, generation)) = session.maintenance_binding() else {
                    return Err(AllCloseError::Binding);
                };
                if closed.binding.restore_id != operation {
                    return Err(AllCloseError::Binding);
                }
                // Convert the already-verified Identity with the same canonical
                // encoding used by the marker and the retained file metadata.
                let volume = u64::from_str_radix(&closed.binding.database.volume, 16)
                    .map_err(|_| AllCloseError::Binding)?;
                let file = u64::from_str_radix(&closed.binding.database.file, 16)
                    .map_err(|_| AllCloseError::Binding)?;
                context
                    .matches_resources(
                        session,
                        closed.binding.restore_id,
                        directory,
                        (volume, file),
                        &database_path,
                    )
                    .map_err(|_| AllCloseError::Binding)?;
                Ok(generation)
            }
            Self::Startup { .. } => session.startup_generation().map_err(AllCloseError::Session),
        }
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "all_closed_tests.rs"]
mod tests;

#[path = "all_closed/file_checks.rs"]
mod file_checks;

#[path = "all_closed/final_verification.rs"]
mod final_verification;
pub(crate) type FinalFilesVerified<'a, 's> = final_verification::FinalFilesVerified<'a, 's>;
pub(crate) type FinalVerificationFailure<'a, 's> =
    final_verification::FinalVerificationFailure<'a, 's>;

// Opaque proof: only the successful consuming unlink path constructs it.
pub(in crate::backup::restore_context::pending) type QUnlinked<'a, 's> =
    final_verification::q_removal::QUnlinked<'a, 's>;

// Running-only admission types retain their private construction paths.
pub(crate) type RunningHandoff<'s> =
    final_verification::q_removal::disk_completion::handoff::RunningHandoff<'s>;
pub(crate) type RunningAdmissionFailure<'s> =
    final_verification::q_removal::disk_completion::handoff::admission::RunningAdmissionFailure<'s>;

pub(crate) type StartupHandoff<'s> =
    final_verification::q_removal::disk_completion::handoff::StartupHandoff<'s>;
pub(crate) type StartupAdmissionFailure<'s> = final_verification::q_removal::disk_completion::handoff::startup_admission::StartupAdmissionFailure<'s>;
