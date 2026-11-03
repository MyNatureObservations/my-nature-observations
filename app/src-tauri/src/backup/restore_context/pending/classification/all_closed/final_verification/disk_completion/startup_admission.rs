//! Startup-only fresh verification. Ready publication belongs to DatabaseState.
use super::*;
use crate::backup::restore_context::pending::classification::bound_sqlite::BoundCloseFailure;
use classification::{BoundSqlite, Control, Reason};
#[derive(Debug)]
pub(crate) enum AdmissionError {
    Unsupported,
    Coordination,
    Binding,
    Files(Reason),
    Establishment(String),
    Sqlite,
    Close { code: Option<i32> },
    Unregister,
    Checkpoint(io::Error),
}
enum Database {
    Live(BoundSqlite),
    Failed(BoundCloseFailure),
}
// A live/failed SQLite binding must drop before any retained file owners.
struct Resources<'s> {
    database: Option<Database>,
    handoff: StartupHandoff<'s>,
}
pub(crate) struct StartupValidated<'s>(Resources<'s>);
pub(crate) struct StartupAdmissionFailure<'s> {
    retained: Box<Resources<'s>>,
    error: AdmissionError,
}
impl std::fmt::Debug for StartupAdmissionFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartupAdmissionFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
struct Owner<'s>(Option<Resources<'s>>);
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::mem::forget(self.0.take());
        }
    }
}
fn point(name: &str) -> Result<(), AdmissionError> {
    event(name).map_err(AdmissionError::Checkpoint)
}
fn verify(
    h: &StartupHandoff<'_>,
    state: &crate::database_state::DatabaseState,
    generation: u32,
) -> Result<(), AdmissionError> {
    if !cfg!(target_os = "macos") {
        return Err(AdmissionError::Unsupported);
    }
    h.0.session
        .check_startup(state, generation)
        .map_err(|_| AdmissionError::Binding)?;
    let (binding, files, cancel) = h.0._files.view();
    if h.0
        .session
        .database_path()
        .map_err(|_| AdmissionError::Binding)?
        != files.root.join("mno.sqlite")
    {
        return Err(AdmissionError::Binding);
    }
    let directory =
        h.0.session
            .directory_identity()
            .map_err(|_| AdmissionError::Binding)?;
    if binding.directory != classification::Identity::from_parts(directory.0, directory.1) {
        return Err(AdmissionError::Binding);
    }
    let artifacts = || {
        if crate::startup::artifacts::unexpected_root(&files.root, None)
            .map_err(|_| AdmissionError::Files(Reason::Io))?
        {
            return Err(AdmissionError::Files(Reason::Marker));
        }
        h.0._marker
            .as_ref()
            .ok_or(AdmissionError::Binding)?
            .check(&files.root_file)
            .map_err(AdmissionError::Establishment)
    };
    artifacts()?;
    binding
        .check_contents(files, cancel, &classification::ReceiptCheck::Startup(h))
        .map_err(AdmissionError::Files)?;
    artifacts()?;
    h.0.session
        .check_startup(state, generation)
        .map_err(|_| AdmissionError::Binding)?;
    classification::cancelled(cancel).map_err(AdmissionError::Files)
}
impl<'s> StartupHandoff<'s> {
    pub(crate) fn matches_startup_target(
        &self,
        state: &crate::database_state::DatabaseState,
    ) -> bool {
        self.0.session.matches_startup_target(state)
    }
    pub(crate) fn reject_binding(self) -> StartupAdmissionFailure<'s> {
        StartupAdmissionFailure {
            retained: Box::new(Resources {
                database: None,
                handoff: self,
            }),
            error: AdmissionError::Binding,
        }
    }
    pub(crate) fn reject_coordination(self) -> StartupAdmissionFailure<'s> {
        StartupAdmissionFailure {
            retained: Box::new(Resources {
                database: None,
                handoff: self,
            }),
            error: AdmissionError::Coordination,
        }
    }
    pub(crate) fn validate_startup(
        self,
        state: &crate::database_state::DatabaseState,
        generation: u32,
    ) -> Result<StartupValidated<'s>, StartupAdmissionFailure<'s>> {
        let mut owner = Owner(Some(Resources {
            database: None,
            handoff: self,
        }));
        let result = owner
            .0
            .as_mut()
            .expect("admission owner")
            .run(state, generation);
        let retained = owner.0.take().expect("admission owner");
        match result {
            Ok(()) => Ok(StartupValidated(retained)),
            Err(error) => Err(StartupAdmissionFailure {
                retained: Box::new(retained),
                error,
            }),
        }
    }
}
impl Resources<'_> {
    fn run(
        &mut self,
        state: &crate::database_state::DatabaseState,
        generation: u32,
    ) -> Result<(), AdmissionError> {
        point("startup-admission-before-validation")?;
        verify(&self.handoff, state, generation)?;
        let (_, files, cancel) = self.handoff.0._files.view();
        self.database = Some(Database::Live(
            BoundSqlite::open(
                &files.root.join("mno.sqlite"),
                files.active_file.clone(),
                Control::Real,
            )
            .map_err(AdmissionError::Files)?,
        ));
        point("startup-admission-after-open")?;
        verify(&self.handoff, state, generation)?;
        let Some(Database::Live(database)) = self.database.as_mut() else {
            unreachable!()
        };
        database.verify().map_err(AdmissionError::Files)?;
        #[cfg(all(test, target_os = "macos"))]
        tests::configure_validation(database);
        {
            let snapshot = crate::backup::journal_content::VerifiedJournal::verify(
                database.connection_mut(),
                cancel.clone(),
            )
            .map_err(|_| AdmissionError::Sqlite)?;
            // Full integrity/schema/Tiptap validation, read transaction only.
            #[cfg(all(test, target_os = "macos"))]
            tests::snapshot_verified();
            snapshot.finish().map_err(|_| AdmissionError::Sqlite)?;
        }
        database.verify().map_err(AdmissionError::Files)?;
        point("startup-admission-before-close")?;
        #[cfg(all(test, target_os = "macos"))]
        tests::configure_close(database);
        let Some(Database::Live(database)) = self.database.take() else {
            unreachable!()
        };
        match database.try_close() {
            Ok(_) => {}
            Err(failure) => {
                let error = match &failure.error {
                    classification::bound_sqlite::CloseError::Sqlite(error) => {
                        AdmissionError::Close {
                            code: match error {
                                rusqlite::Error::SqliteFailure(code, _) => Some(code.extended_code),
                                _ => None,
                            },
                        }
                    }
                    classification::bound_sqlite::CloseError::Unregister => {
                        AdmissionError::Unregister
                    }
                };
                self.database = Some(Database::Failed(failure));
                return Err(error);
            }
        }
        point("startup-admission-after-close")?;
        verify(&self.handoff, state, generation)?;
        point("startup-admission-before-ready")?;
        verify(&self.handoff, state, generation)
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "startup_admission_tests.rs"]
mod tests;
