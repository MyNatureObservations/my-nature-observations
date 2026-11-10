//! App boundary for staged startup recovery. Never calls the abandoned reset engine.
#[cfg(target_os = "macos")]
use crate::backup::restore_context::activation::startup as engine;
use crate::{database, database_state::DatabaseState};
#[cfg(target_os = "macos")]
use engine::{Action, View};
use serde::Serialize;
use tauri::{Emitter, Manager};
#[cfg(not(target_os = "macos"))]
#[derive(serde::Deserialize)]
pub(crate) enum Action {}
#[cfg(not(target_os = "macos"))]
#[derive(Serialize)]
pub(crate) struct View {}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Ready,
    Recovery,
    Blocked,
    #[cfg(windows)]
    WindowsRecovery,
    #[cfg(windows)]
    WindowsRestorePending,
    #[cfg(windows)]
    WindowsRestoreUncertain,
}
#[tauri::command]
pub(crate) fn database_mode(state: tauri::State<'_, DatabaseState>) -> Mode {
    #[cfg(windows)]
    if let Some(acknowledged) = state.windows_restore_outcome() {
        return if acknowledged {
            Mode::WindowsRestorePending
        } else {
            Mode::WindowsRestoreUncertain
        };
    }
    #[cfg(windows)]
    if state.windows_recovery_available() {
        return Mode::WindowsRecovery;
    }
    if state.startup_recovery_pending() {
        Mode::Recovery
    } else if state.ready_database_path().is_ok() {
        Mode::Ready
    } else {
        Mode::Blocked
    }
}
#[tauri::command]
pub(crate) async fn recovery_status(app: tauri::AppHandle) -> Result<View, String> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || {
        engine::status(&app.state::<DatabaseState>()).map_err(|_| "restore_blocked".to_owned())
    })
    .await
    .map_err(|_| "restore_blocked".to_owned())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("restore_unsupported".into())
    }
}
// Retain the existing app-work reservation through DataLocation/event publication.
#[cfg(target_os = "macos")]
pub(crate) fn run_owned<'a>(
    state: &DatabaseState,
    shared: &'a crate::restore_app::RestoreState,
    action: Action,
    previous_confirmation: Option<&str>,
) -> Result<crate::restore_app::Busy<'a>, String> {
    let busy = shared.reserve().map_err(|_| "restore_busy".to_owned())?;
    engine::run_confirmed(state, action, previous_confirmation)
        .map_err(|_| "restore_blocked".to_owned())?;
    Ok(busy)
}
#[tauri::command]
pub(crate) async fn run_recovery(
    app: tauri::AppHandle,
    action: Action,
    previous_confirmation: Option<String>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<crate::restore_app::RestoreState>();
        let state = app.state::<DatabaseState>();
        let _busy = run_owned(&state, &shared, action, previous_confirmation.as_deref())?;
        let path = state
            .ready_database_path()
            .map_err(|_| "restore_blocked".to_owned())?;
        if !app.manage(database::DataLocation::new(path)) {
            std::process::abort();
        }
        let _ = app.emit("mno:database-mode-changed", ());
        Ok(())
    })
    .await
    .map_err(|_| "restore_blocked".to_owned())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, action, previous_confirmation);
        Err("restore_unsupported".into())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn work_reservation_is_checked_before_startup_and_released_on_error() {
        let state = DatabaseState::default();
        let shared = crate::restore_app::RestoreState::default();
        let busy = shared.reserve().unwrap();
        assert!(
            matches!(run_owned(&state, &shared, Action::Continue, None), Err(e) if e == "restore_busy")
        );
        assert!(shared.is_busy());
        drop(busy);
        assert!(
            matches!(run_owned(&state, &shared, Action::Continue, None), Err(e) if e == "restore_blocked")
        );
        assert!(!shared.is_busy());
    }
}
