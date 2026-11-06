//! Native picker -> verified one-use selection -> Maintenance -> activation -> explicit quit.
//! Ready is granted only by the existing startup completion, in the next process.
use crate::database_state::DatabaseState;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
#[derive(Default)]
pub(crate) struct RestoreState {
    busy: AtomicBool,
    selected: Mutex<Option<Choice>>,
}
struct Choice {
    token: String,
    path: PathBuf,
    hash: String,
    generation: u32,
}
pub(crate) struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl RestoreState {
    pub(crate) fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
    pub(crate) fn reserve(&self) -> Result<Busy<'_>, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        Ok(Busy(&self.busy))
    }
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Error {
    Busy,
    InvalidBackup,
    Changed,
    Failed,
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}
#[derive(Serialize)]
pub(crate) struct Selection {
    token: String,
}
#[cfg(target_os = "macos")]
fn validate_selection(state: &DatabaseState, path: PathBuf) -> Result<Choice, Error> {
    let permit = state.operation(None).map_err(|_| Error::Busy)?;
    let active = state.ready_database_path().map_err(|_| Error::Busy)?;
    let active = rusqlite::Connection::open_with_flags(
        active,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| Error::Failed)?;
    let active_version: u32 = active
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| Error::Failed)?;
    active.close().map_err(|_| Error::Failed)?;
    let generation = permit.snapshot(()).generation;
    let manifest = crate::backup::verify_backup(&path).map_err(|_| Error::InvalidBackup)?;
    if active_version != crate::database::SCHEMA_VERSION
        || manifest.format_version != 5
        || manifest.database_schema_version != 4
    {
        return Err(Error::InvalidBackup);
    }
    let hash = crate::backup::restore_context::activation::validate_archive(&path)
        .map_err(|_| Error::InvalidBackup)?;
    Ok(Choice {
        token: uuid::Uuid::new_v4().to_string(),
        path,
        hash,
        generation,
    })
}
#[cfg(target_os = "macos")]
fn activate_selection(state: &DatabaseState, choice: Choice) -> Result<(), Error> {
    let maintenance = state.maintenance().map_err(|_| Error::Busy)?;
    if maintenance.generation() != choice.generation {
        return Err(Error::Changed);
    }
    if crate::backup::restore_context::activation::validate_archive(&choice.path)
        .map_err(|_| Error::InvalidBackup)?
        != choice.hash
    {
        return Err(Error::Changed);
    }
    // activate revalidates the complete archive before any activation writes.
    // Its RecoverySession latches the process closed; there is no running disarm.
    let activated = crate::backup::restore_context::activation::activate(
        &maintenance,
        &choice.path,
        std::sync::Arc::new(AtomicBool::new(false)),
    )
    .map_err(|_| Error::Failed)?;
    drop(activated);
    Ok(()) // Disk activated, NOT Ready. All session/SQLite owners close before reply.
}
#[cfg(windows)]
fn validate_selection(state: &DatabaseState, path: PathBuf) -> Result<Choice, Error> {
    let permit = state.operation(None).map_err(|_| Error::Busy)?;
    let generation = permit.snapshot(()).generation;
    let (_, hash) = crate::backup::restore::prepare_windows_backup(&path, None)
        .map_err(|_| Error::InvalidBackup)?;
    Ok(Choice {
        token: uuid::Uuid::new_v4().to_string(),
        path,
        hash,
        generation,
    })
}

#[cfg(windows)]
fn activate_selection(state: &DatabaseState, choice: Choice) -> Result<(), Error> {
    let maintenance = state.maintenance().map_err(|_| Error::Busy)?;
    if maintenance.generation() != choice.generation {
        return Err(Error::Changed);
    }
    let (candidate, _) =
        crate::backup::restore::prepare_windows_backup(&choice.path, Some(&choice.hash))
            .map_err(|_| Error::InvalidBackup)?;
    maintenance
        .publish_windows_restore(&candidate)
        .map_err(|_| Error::Failed)
}

#[tauri::command]
pub(crate) fn restore_available() -> bool {
    // Windows stays unavailable in every build until blocked-startup Recovery is accepted.
    cfg!(target_os = "macos")
}

fn require_restore_available() -> Result<(), Error> {
    #[cfg(not(target_os = "macos"))]
    if !restore_available() {
        return Err(Error::Unsupported);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn choose_restore_backup(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    dialog_title: String,
) -> Result<Option<Selection>, Error> {
    require_restore_available()?;
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<RestoreState>();
        let _busy = shared.reserve()?;
        *shared.selected.lock().map_err(|_| Error::Failed)? = None;
        #[cfg(any(target_os = "macos", windows))]
        {
            let Some(file) = app
                .dialog()
                .file()
                .set_parent(&window)
                .set_title(&dialog_title)
                .add_filter(&dialog_title, &["mno-backup"])
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            let path = file.into_path().map_err(|_| Error::InvalidBackup)?;
            let choice = validate_selection(&app.state::<DatabaseState>(), path)?;
            let result = Selection {
                token: choice.token.clone(),
            };
            *shared.selected.lock().map_err(|_| Error::Failed)? = Some(choice);
            Ok(Some(result))
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = (window, dialog_title);
            Err(Error::Unsupported)
        }
    })
    .await
    .map_err(|_| Error::Failed)?
}
#[tauri::command]
pub(crate) async fn restore_selected_backup(
    app: tauri::AppHandle,
    token: String,
) -> Result<(), Error> {
    require_restore_available()?;
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<RestoreState>();
        let _busy = shared.reserve()?;
        let choice = shared
            .selected
            .lock()
            .map_err(|_| Error::Failed)?
            .take()
            .ok_or(Error::Changed)?;
        if choice.token != token {
            return Err(Error::Changed);
        }
        #[cfg(any(target_os = "macos", windows))]
        let result = activate_selection(&app.state::<DatabaseState>(), choice);
        #[cfg(not(any(target_os = "macos", windows)))]
        let result = Err(Error::Unsupported);
        // On failure after arming, hide all journal views too. The backend gate
        // already denies operations, including stale/queued editor writes.
        let _ = app.emit("mno:database-mode-changed", ());
        result
    })
    .await
    .map_err(|_| Error::Failed)?
}
// Synchronous Tauri command: the reservation is held while destroy is queued.
// Tauri/Wry processes that queued event after this main-thread command
// returns, so our own reservation cannot suppress the final ExitRequested.
#[tauri::command]
pub(crate) fn close_journal_window(
    window: tauri::WebviewWindow,
    shared: tauri::State<'_, RestoreState>,
) -> Result<(), Error> {
    request_window_close(&shared, || window.destroy().map_err(|_| Error::Failed))
}
fn request_window_close(
    shared: &RestoreState,
    close: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    let _busy = shared.reserve()?;
    close()
}
#[tauri::command]
pub(crate) fn quit_after_restore(app: tauri::AppHandle) -> Result<(), Error> {
    request_quit(
        &app.state::<DatabaseState>(),
        &app.state::<RestoreState>(),
        || {
            // Normal ExitRequested/Exit path, including single-instance cleanup.
            // No relaunch: the user opens MNO after this process has terminated.
            app.exit(0);
        },
    )
}
fn request_quit(
    state: &DatabaseState,
    shared: &RestoreState,
    quit: impl FnOnce(),
) -> Result<(), Error> {
    let _busy = shared.reserve()?;
    // A normal Ready editor must use its existing unsaved-changes/close guard.
    // This command is only for the blocked restore shell.
    if state.ready_database_path().is_ok() {
        return Err(Error::Changed);
    }
    // The database gate is already closed and no activation owns the reservation.
    // Release this short command reservation before dispatching ExitRequested;
    // otherwise the shared event guard could reject our own normal exit.
    drop(_busy);
    quit();
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

#[cfg(all(test, windows))]
#[test]
fn windows_restore_commands_are_unavailable_even_in_test_builds() {
    assert!(!restore_available());
    assert_eq!(require_restore_available(), Err(Error::Unsupported));
}

#[cfg(all(test, windows))]
#[test]
fn windows_restore_requires_an_unchanged_archive_and_quit_waits_for_publication() {
    use crate::backup::{BackupKind, create_backup};
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("app");
    let state = DatabaseState::default();
    let database = state.initialize_startup(&root).unwrap();
    let archive = parent.path().join("restore.mno-backup");
    let db = rusqlite::Connection::open(database).unwrap();
    create_backup(&db, &archive, BackupKind::Manual).unwrap();
    db.close().unwrap();
    let shared = RestoreState::default();
    let choice = validate_selection(&state, archive.clone()).unwrap();
    assert!(!root.join("generations").exists());
    assert_eq!(
        request_quit(&state, &shared, || panic!("Ready quit")),
        Err(Error::Changed)
    );
    let original = std::fs::read(&archive).unwrap();
    std::fs::write(&archive, b"changed archive").unwrap();
    assert_eq!(
        activate_selection(&state, choice),
        Err(Error::InvalidBackup)
    );
    assert!(!root.join("generations").exists());
    assert!(state.operation(None).is_ok());

    std::fs::write(&archive, &original).unwrap();
    let choice = validate_selection(&state, archive.clone()).unwrap();
    activate_selection(&state, choice).unwrap();
    let reservation = shared.reserve().unwrap();
    assert_eq!(
        request_quit(&state, &shared, || panic!("busy quit")),
        Err(Error::Busy)
    );
    drop(reservation);
    let mut quit = false;
    request_quit(&state, &shared, || quit = true).unwrap();
    assert!(quit);
    assert!(state.operation(None).is_err());
    assert_eq!(std::fs::read(archive).unwrap(), original);
}
