//! Running-only fresh verification. The sole Ready publisher is Maintenance.
use super::*;
use crate::backup::restore_context::pending::classification::bound_sqlite::BoundCloseFailure;
use classification::{BoundSqlite, Control, Reason};
#[derive(Debug)]
pub(crate) enum AdmissionError {
    Unsupported,
    Overflow,
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
    handoff: RunningHandoff<'s>,
}
pub(crate) struct RunningValidated<'s>(Resources<'s>);
pub(crate) struct RunningAdmissionFailure<'s> {
    retained: Box<Resources<'s>>,
    error: AdmissionError,
}
impl std::fmt::Debug for RunningAdmissionFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningAdmissionFailure")
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
fn verify(h: &RunningHandoff<'_>, m: &Maintenance<'_>) -> Result<(), AdmissionError> {
    if !cfg!(target_os = "macos") {
        return Err(AdmissionError::Unsupported);
    }
    h.0.session
        .check_running(m)
        .map_err(|_| AdmissionError::Binding)?;
    let (binding, files, cancel) = h.0._files.view();
    if binding.restore_id != m.operation_id()
        || h.0
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
        .check_contents(files, cancel, &classification::ReceiptCheck::Running(h))
        .map_err(AdmissionError::Files)?;
    artifacts()?;
    h.0.session
        .check_running(m)
        .map_err(|_| AdmissionError::Binding)?;
    classification::cancelled(cancel).map_err(AdmissionError::Files)
}
impl<'s> RunningHandoff<'s> {
    pub(crate) fn reject_overflow(self) -> RunningAdmissionFailure<'s> {
        RunningAdmissionFailure {
            retained: Box::new(Resources {
                database: None,
                handoff: self,
            }),
            error: AdmissionError::Overflow,
        }
    }
    pub(crate) fn validate_running(
        self,
        m: &Maintenance<'_>,
    ) -> Result<RunningValidated<'s>, RunningAdmissionFailure<'s>> {
        let mut owner = Owner(Some(Resources {
            database: None,
            handoff: self,
        }));
        let result = owner.0.as_mut().expect("admission owner").run(m);
        let retained = owner.0.take().expect("admission owner");
        match result {
            Ok(()) => Ok(RunningValidated(retained)),
            Err(error) => Err(RunningAdmissionFailure {
                retained: Box::new(retained),
                error,
            }),
        }
    }
}
impl Resources<'_> {
    fn run(&mut self, m: &Maintenance<'_>) -> Result<(), AdmissionError> {
        point("admission-before-validation")?;
        verify(&self.handoff, m)?;
        let (_, files, cancel) = self.handoff.0._files.view();
        self.database = Some(Database::Live(
            BoundSqlite::open(
                &files.root.join("mno.sqlite"),
                files.active_file.clone(),
                Control::Real,
            )
            .map_err(AdmissionError::Files)?,
        ));
        point("admission-after-open")?;
        verify(&self.handoff, m)?;
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
        point("admission-before-close")?;
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
        point("admission-after-close")?;
        verify(&self.handoff, m)?;
        point("admission-before-ready")?;
        verify(&self.handoff, m)
    }
}
#[cfg(all(test, target_os = "macos"))]
#[path = "admission_tests.rs"]
mod tests;
