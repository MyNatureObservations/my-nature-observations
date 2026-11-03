//! Test-only prototype. No arbitrary destination path, command, or restore authorization API.
use super::journal_content::{self, Comparison, VerifiedJournal};
use rusqlite::{Connection, OpenFlags, ffi};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    Cancelled,
    Busy,
    Locked,
    Io,
    Full,
    MissingTarget,
    Mode,
    Transaction,
    Settings,
    Source,
    Verification,
    Sqlite(i32),
    Finish(i32),
}
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Unchanged(Failure),
    Completed { cancel_requested: bool },
    Uncertain(Failure),
}
fn classify(rc: i32) -> Failure {
    match rc & 0xff {
        ffi::SQLITE_BUSY => Failure::Busy,
        ffi::SQLITE_LOCKED => Failure::Locked,
        ffi::SQLITE_IOERR => Failure::Io,
        ffi::SQLITE_FULL => Failure::Full,
        _ => Failure::Sqlite(rc),
    }
}
fn error(e: rusqlite::Error) -> Failure {
    match e {
        rusqlite::Error::SqliteFailure(e, _) => classify(e.extended_code),
        _ => Failure::Settings,
    }
}

// Only constructor creates an owned temporary directory and a brand-new fixture.
struct TestDestination {
    // Rust drops fields in declaration order: close SQLite before directory cleanup.
    database: Connection,
    directory: TempDir,
}
impl TestDestination {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("destination.sqlite");
        let created = Connection::open(&path).unwrap();
        crate::database::initialize_schema(&created).unwrap();
        drop(created);
        let database = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE,
        )
        .unwrap();
        Self {
            directory,
            database,
        }
    }
}
// Only this module can mint the destination capability from its owned test fixture.
pub(super) struct CopyDestination<'a> {
    connection: &'a mut Connection,
}
impl<'a> CopyDestination<'a> {
    pub(super) fn into_connection(self) -> &'a mut Connection {
        self.connection
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    BeforeStep,
    AfterStep { remaining: i32 },
    Committed,
}

fn settings(db: &Connection) -> Result<(), Failure> {
    if !db.is_autocommit() {
        return Err(Failure::Transaction);
    }
    let mode: String = db
        .pragma_query_value(Some("main"), "journal_mode", |r| r.get(0))
        .map_err(error)?;
    if mode != "delete" {
        return Err(Failure::Mode);
    }
    db.busy_timeout(Duration::ZERO).map_err(error)?;
    db.pragma_update(Some("main"), "synchronous", "EXTRA")
        .map_err(error)?;
    let sync: i32 = db
        .pragma_query_value(Some("main"), "synchronous", |r| r.get(0))
        .map_err(error)?;
    if sync != 3 {
        return Err(Failure::Settings);
    }
    #[cfg(target_os = "macos")]
    {
        db.pragma_update(None, "fullfsync", true).map_err(error)?;
        let full: bool = db
            .pragma_query_value(None, "fullfsync", |r| r.get(0))
            .map_err(error)?;
        if !full {
            return Err(Failure::Settings);
        }
    }
    Ok(())
}
fn copy(
    source: &VerifiedJournal<'_>,
    target: &mut TestDestination,
    cancel: Arc<AtomicBool>,
    hook: impl FnMut(Point),
) -> Outcome {
    copy_inner(source, target, cancel, hook, |rc| rc)
}
// finish_observer only injects a failure AFTER the real SQLite handle has closed.
fn copy_inner(
    source: &VerifiedJournal<'_>,
    target: &mut TestDestination,
    cancel: Arc<AtomicBool>,
    mut hook: impl FnMut(Point),
    finish_observer: impl FnOnce(i32) -> i32,
) -> Outcome {
    if cancel.load(Ordering::Acquire) {
        return Outcome::Unchanged(Failure::Cancelled);
    }
    if !target.directory.path().join("destination.sqlite").is_file() {
        return Outcome::Unchanged(Failure::MissingTarget);
    }
    if let Err(e) = settings(&target.database) {
        return Outcome::Unchanged(e);
    }
    let handle = match source.start_copy(CopyDestination {
        connection: &mut target.database,
    }) {
        Ok(h) => h,
        Err(journal_content::CopyStartError::Source) => return Outcome::Unchanged(Failure::Source),
        Err(journal_content::CopyStartError::Sqlite(code)) => {
            return Outcome::Unchanged(classify(code));
        }
    };
    let mut committed = false;
    let failure = loop {
        hook(Point::BeforeStep);
        if cancel.load(Ordering::Acquire) {
            break Some(Failure::Cancelled);
        }
        let rc = handle.step();
        if rc == ffi::SQLITE_DONE {
            committed = true; // SQLite has committed before DONE is returned.
            hook(Point::Committed);
            break None;
        }
        if rc != ffi::SQLITE_OK {
            break Some(classify(rc));
        }
        hook(Point::AfterStep {
            remaining: handle.remaining(),
        });
    };
    let finish = finish_observer(handle.finish());
    if finish != ffi::SQLITE_OK {
        return Outcome::Uncertain(failure.unwrap_or(Failure::Finish(finish)));
    }
    if !committed {
        let failure = failure.unwrap();
        return if matches!(
            failure,
            Failure::Cancelled | Failure::Busy | Failure::Locked
        ) {
            Outcome::Unchanged(failure)
        } else {
            Outcome::Uncertain(failure)
        };
    }
    // Cancellation after commit must not pretend the old destination survived.
    let verified =
        match VerifiedJournal::verify(&mut target.database, Arc::new(AtomicBool::new(false))) {
            Ok(v) => v,
            Err(_) => return Outcome::Uncertain(Failure::Verification),
        };
    let identical = matches!(
        journal_content::compare(source, &verified),
        Ok(Comparison::Identical)
    );
    if verified.finish().is_err() || !identical {
        return Outcome::Uncertain(Failure::Verification);
    }
    Outcome::Completed {
        cancel_requested: cancel.load(Ordering::Acquire),
    }
}
#[cfg(test)]
mod tests;
