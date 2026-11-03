//! Native save dialog and app coordination for the existing open-export engine.
use crate::{database, database_state::DatabaseState, open_export, restore_app::RestoreState};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Error {
    Busy,
    AlreadyExists,
    InvalidDestination,
    InvalidContent,
    Failed,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Cancelled,
    Created,
}

fn close_source(source: Connection) {
    if let Err((connection, _)) = source.close() {
        // An uncertain close is terminal. Keep the original owner until abort.
        std::mem::forget(connection);
        std::process::abort();
    }
}
fn map_error(error: open_export::Error) -> Error {
    match error {
        open_export::Error::Io(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Error::AlreadyExists
        }
        open_export::Error::InvalidDestination => Error::InvalidDestination,
        open_export::Error::InvalidDatabase
        | open_export::Error::IncompatibleSchema
        | open_export::Error::InvalidStorage
        | open_export::Error::InvalidJson
        | open_export::Error::ResourceLimit => Error::InvalidContent,
        open_export::Error::Sql(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Error::Busy
        }
        _ => Error::Failed,
    }
}
fn run(
    state: &DatabaseState,
    shared: &RestoreState,
    choose: impl FnOnce() -> Result<Option<PathBuf>, Error>,
    source_path: impl FnOnce() -> Result<PathBuf, Error>,
) -> Result<Outcome, Error> {
    // One existing reservation covers the dialog, worker, normal exit and other
    // import/restore/export requests, including when the requesting view leaves.
    let _busy = shared.reserve().map_err(|_| Error::Busy)?;
    let Some(mut destination) = choose()? else {
        return Ok(Outcome::Cancelled);
    };
    if destination.extension().is_none() {
        destination.set_extension("mno-export");
    }
    if destination.extension().and_then(|e| e.to_str()) != Some("mno-export") {
        return Err(Error::InvalidDestination);
    }
    if destination
        .try_exists()
        .map_err(|_| Error::InvalidDestination)?
    {
        return Err(Error::AlreadyExists);
    }
    if !destination.parent().is_some_and(|parent| parent.is_dir()) {
        return Err(Error::InvalidDestination);
    }
    // No database permit or SQLite connection is held while the user chooses.
    let _operation = state.operation(None).map_err(|_| Error::Busy)?;
    let mut source = Connection::open_with_flags(source_path()?, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| Error::Failed)?;
    if source.busy_timeout(Duration::from_secs(5)).is_err() {
        close_source(source);
        return Err(Error::Failed);
    }
    let result = open_export::export(&mut source, &destination, &AtomicBool::new(false));
    // Close before returning any result and before releasing the operation guard.
    close_source(source);
    result.map_err(map_error)?;
    Ok(Outcome::Created)
}
#[tauri::command]
pub(crate) async fn create_open_export(
    app: AppHandle,
    window: WebviewWindow,
    suggested_name: String,
    dialog_title: String,
) -> Result<Outcome, Error> {
    tauri::async_runtime::spawn_blocking(move || {
        run(
            &app.state::<DatabaseState>(),
            &app.state::<RestoreState>(),
            || {
                app.dialog()
                    .file()
                    .set_parent(&window)
                    .set_title(&dialog_title)
                    .set_file_name(&suggested_name)
                    .add_filter(&dialog_title, &["mno-export"])
                    .blocking_save_file()
                    .map(|file| file.into_path().map_err(|_| Error::InvalidDestination))
                    .transpose()
            },
            || database::database_path(&app).map_err(|_| Error::Failed),
        )
    })
    .await
    .map_err(|_| Error::Failed)?
}
#[cfg(test)]
mod tests;
