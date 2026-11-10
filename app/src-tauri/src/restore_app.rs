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
    #[cfg(windows)]
    recovery: Mutex<Option<RecoveryChoice>>,
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
    #[cfg(windows)]
    PreparationFailed,
    Changed,
    Failed,
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}
#[derive(Serialize)]
pub(crate) struct Selection {
    token: String,
    #[cfg(windows)]
    preserves_previous: bool,
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
    cfg!(any(target_os = "macos", windows))
}

fn require_restore_available() -> Result<(), Error> {
    #[cfg(not(any(target_os = "macos", windows)))]
    if !restore_available() {
        return Err(Error::Unsupported);
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn pick_backup(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    title: &str,
) -> Result<Option<PathBuf>, Error> {
    app.dialog()
        .file()
        .set_parent(window)
        .set_title(title)
        .add_filter(title, &["mno-backup"])
        .blocking_pick_file()
        .map(|file| file.into_path().map_err(|_| Error::InvalidBackup))
        .transpose()
}

#[cfg(windows)]
struct RecoveryChoice {
    token: String,
    path: PathBuf,
    digest: String,
}

#[tauri::command]
pub(crate) async fn choose_recovery_backup(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    dialog_title: String,
) -> Result<Option<Selection>, Error> {
    #[cfg(not(windows))]
    {
        let _ = (app, window, dialog_title);
        Err(Error::Failed)
    }
    #[cfg(windows)]
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<RestoreState>();
        let _busy = shared.reserve()?;
        let state = app.state::<DatabaseState>();
        *shared.recovery.lock().map_err(|_| Error::Failed)? = None;
        if !state.windows_recovery_available() {
            return Err(Error::Unsupported);
        }
        let Some(path) = pick_backup(&app, &window, &dialog_title)? else {
            return Ok(None);
        };
        let (_, digest) = crate::backup::restore::prepare_windows_backup(&path, None)
            .map_err(|_| Error::InvalidBackup)?;
        let token = uuid::Uuid::new_v4().to_string();
        *shared.recovery.lock().map_err(|_| Error::Failed)? = Some(RecoveryChoice {
            token: token.clone(),
            path,
            digest,
        });
        Ok(Some(Selection {
            token,
            preserves_previous: true,
        }))
    })
    .await
    .map_err(|_| Error::Failed)?
}

#[tauri::command]
pub(crate) async fn confirm_recovery_backup(
    app: tauri::AppHandle,
    token: String,
) -> Result<(), Error> {
    #[cfg(not(windows))]
    {
        let _ = (app, token);
        Err(Error::Failed)
    }
    #[cfg(windows)]
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<RestoreState>();
        let _busy = shared.reserve()?;
        confirm_recovery_choice(&app.state::<DatabaseState>(), &shared, &token)
    })
    .await
    .map_err(|_| Error::Failed)?
}

#[cfg(windows)]
fn confirm_recovery_choice(
    state: &DatabaseState,
    shared: &RestoreState,
    token: &str,
) -> Result<(), Error> {
    let choice = shared
        .recovery
        .lock()
        .map_err(|_| Error::Failed)?
        .take()
        .ok_or(Error::Changed)?;
    if choice.token != token {
        return Err(Error::Changed);
    }
    if !state.windows_recovery_available() {
        return Err(Error::Unsupported);
    }
    state
        .confirm_windows_recovery(&choice.path, Some(&choice.digest))
        .map_err(|error| match error.as_str() {
            "CandidateChanged" => Error::Changed,
            "DamagedArchive"
            | "UnsupportedFormat"
            | "InvalidDatabase"
            | "IncompatibleSchema"
            | "IncompatibleDocument" => Error::InvalidBackup,
            _ => Error::Failed,
        })
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
            #[cfg(windows)]
            app.state::<DatabaseState>()
                .ready_database_path()
                .map_err(|_| Error::Busy)?;
            let Some(path) = pick_backup(&app, &window, &dialog_title)? else {
                return Ok(None);
            };
            let choice = validate_selection(&app.state::<DatabaseState>(), path)?;
            let result = Selection {
                token: choice.token.clone(),
                #[cfg(windows)]
                preserves_previous: true,
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
        #[cfg(target_os = "macos")]
        let result = activate_selection(&app.state::<DatabaseState>(), choice);
        #[cfg(windows)]
        let result = {
            let state = app.state::<DatabaseState>();
            activate_selection(&state, choice).map_err(|error| {
                if state.ready_database_path().is_ok() {
                    Error::PreparationFailed
                } else {
                    error
                }
            })
        };
        #[cfg(not(any(target_os = "macos", windows)))]
        let result = Err(Error::Unsupported);
        #[cfg(windows)]
        drop(_busy);
        #[cfg(windows)]
        if result == Err(Error::PreparationFailed) {
            // The current journal is still Ready. A needless refresh could
            // strand the user in the blocked shell if IPC fails.
            return result;
        }
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

#[cfg(all(test, any(target_os = "macos", windows)))]
#[test]
fn ordinary_restore_is_available_on_supported_platforms() {
    assert!(restore_available());
    assert_eq!(require_restore_available(), Ok(()));
}

#[cfg(all(test, not(any(target_os = "macos", windows))))]
#[test]
fn ordinary_restore_is_unavailable_on_unsupported_platforms() {
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
    let db = rusqlite::Connection::open(&database).unwrap();
    create_backup(&db, &archive, BackupKind::Manual).unwrap();
    db.close().unwrap();
    let shared = RestoreState::default();
    let invalid = parent.path().join("invalid.mno-backup");
    std::fs::write(&invalid, b"invalid archive").unwrap();
    let original_database = std::fs::read(&database).unwrap();
    assert!(matches!(
        validate_selection(&state, invalid),
        Err(Error::InvalidBackup)
    ));
    assert_eq!(std::fs::read(&database).unwrap(), original_database);
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
    assert_eq!(state.windows_restore_outcome(), None);
    assert_eq!(std::fs::read(&database).unwrap(), original_database);
    assert!(!root.join("generation-selection.json").exists());
    {
        let active = state.ready_database_path().unwrap();
        let _permit = state.operation(None).unwrap();
        let db = crate::database::open_existing(&active).unwrap();
        db.execute(
            "INSERT INTO journal_entries VALUES ('after-rejection','2026-10-25','Still writable',?1,1,'','')",
            [r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Synthetic entry"}]}]}"#],
        ).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT title FROM journal_entries WHERE id = 'after-rejection'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "Still writable"
        );
        db.close().unwrap();
    }

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

#[cfg(all(test, windows))]
#[test]
fn blocked_windows_confirmation_consumes_token_and_rejects_changed_archive() {
    use crate::backup::{BackupKind, create_backup};
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("app");
    let ready = DatabaseState::default();
    let database = ready.initialize_startup(&root).unwrap();
    let archive = parent.path().join("backup.mno-backup");
    let db = rusqlite::Connection::open(&database).unwrap();
    create_backup(&db, &archive, BackupKind::Manual).unwrap();
    db.close().unwrap();
    assert!(!ready.windows_recovery_available());
    drop(ready);

    let head = root.join("generation-selection.json");
    std::fs::write(&head, b"head without managed namespace").unwrap();
    let unsupported = DatabaseState::default();
    assert!(unsupported.initialize_startup(&root).is_err());
    assert!(!unsupported.windows_recovery_available());
    drop(unsupported);
    std::fs::remove_file(&head).unwrap();

    std::fs::create_dir(root.join("generations")).unwrap();
    let blocked = DatabaseState::default();
    assert!(blocked.initialize_startup(&root).is_err());
    assert!(blocked.windows_recovery_available());
    let contender = DatabaseState::default();
    assert!(contender.initialize_startup(&root).is_err());
    assert!(!contender.windows_recovery_available());
    let (_, digest) = crate::backup::restore::prepare_windows_backup(&archive, None).unwrap();
    let shared = RestoreState::default();
    *shared.recovery.lock().unwrap() = Some(RecoveryChoice {
        token: "confirmed".into(),
        path: archive.clone(),
        digest,
    });
    let original = std::fs::read(&archive).unwrap();
    std::fs::write(&archive, b"changed").unwrap();
    assert_eq!(
        confirm_recovery_choice(&blocked, &shared, "confirmed"),
        Err(Error::Changed)
    );
    assert_eq!(
        confirm_recovery_choice(&blocked, &shared, "confirmed"),
        Err(Error::Changed)
    );
    assert!(shared.recovery.lock().unwrap().is_none());
    assert!(!root.join("generation-selection.json").exists());
    assert!(blocked.operation(None).is_err());

    std::fs::write(&archive, &original).unwrap();
    let (_, digest) = crate::backup::restore::prepare_windows_backup(&archive, None).unwrap();
    *shared.recovery.lock().unwrap() = Some(RecoveryChoice {
        token: "retry".into(),
        path: archive,
        digest,
    });
    confirm_recovery_choice(&blocked, &shared, "retry").unwrap();
    assert!(!blocked.windows_recovery_available());
    assert!(blocked.operation(None).is_err());
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(head).unwrap()).unwrap();
    assert_eq!(record["phase"].as_str(), Some("pending"));
    let mut quit = false;
    request_quit(&blocked, &shared, || quit = true).unwrap();
    assert!(quit);
}
