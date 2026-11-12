use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    backup::{self, BackupKind},
    database,
    database_state::DatabaseState,
};

#[derive(Default, Clone)]
pub(crate) struct ManualBackupState(Arc<AtomicBool>);

struct RunningBackup(Arc<AtomicBool>);

impl ManualBackupState {
    fn try_start(&self) -> Result<RunningBackup, ManualBackupError> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ManualBackupError::Busy)?;
        Ok(RunningBackup(Arc::clone(&self.0)))
    }
}

impl Drop for RunningBackup {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManualBackupError {
    Busy,
    AlreadyExists,
    InvalidDestination,
    Failed,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManualBackupOutcome {
    Cancelled,
    Created,
}

#[tauri::command]
pub(crate) async fn create_manual_backup(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, ManualBackupState>,
    suggested_name: String,
    dialog_title: String,
) -> Result<ManualBackupOutcome, ManualBackupError> {
    let running = state.try_start()?;
    // The permit stays with the worker even if the requesting frontend disappears.
    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        let coordination = app.state::<DatabaseState>();
        run_manual_backup_coordinated(
            || {
                app.dialog()
                    .file()
                    .set_parent(&window)
                    .set_title(dialog_title)
                    .set_file_name(suggested_name)
                    .add_filter("MNO backup", &["mno-backup"])
                    .blocking_save_file()
                    .map(|file| {
                        file.into_path()
                            .map_err(|_| ManualBackupError::InvalidDestination)
                    })
                    .transpose()
            },
            || {
                let path = database::database_path(&app).map_err(|_| ManualBackupError::Failed)?;
                open_source(&path)
            },
            &coordination,
        )
    })
    .await
    .map_err(|_| ManualBackupError::Failed)?
}

fn open_source(path: &Path) -> Result<Connection, ManualBackupError> {
    // Never create a missing source database or initialize/change its schema.
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| ManualBackupError::Failed)?;
    source
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| ManualBackupError::Failed)?;
    Ok(source)
}

fn run_manual_backup_coordinated(
    choose_destination: impl FnOnce() -> Result<Option<PathBuf>, ManualBackupError>,
    open_database: impl FnOnce() -> Result<Connection, ManualBackupError>,
    state: &DatabaseState,
) -> Result<ManualBackupOutcome, ManualBackupError> {
    let Some(mut destination) = choose_destination()? else {
        return Ok(ManualBackupOutcome::Cancelled);
    };
    // Some native dialogs append the filter extension, others leave it to us.
    if destination.extension().is_none() {
        destination.set_extension("mno-backup");
    }
    if destination.extension().and_then(|ext| ext.to_str()) != Some("mno-backup") {
        return Err(ManualBackupError::InvalidDestination);
    }
    if destination
        .try_exists()
        .map_err(|_| ManualBackupError::InvalidDestination)?
    {
        return Err(ManualBackupError::AlreadyExists);
    }
    if !destination.parent().unwrap_or(Path::new(".")).is_dir() {
        return Err(ManualBackupError::InvalidDestination);
    }
    let _operation = state.operation(None).map_err(|_| ManualBackupError::Busy)?;
    let source = open_database()?;
    backup::create_backup(&source, &destination, BackupKind::Manual).map_err(|error| {
        // Preserve no-clobber behavior even if the target appears after our check.
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists)
        {
            ManualBackupError::AlreadyExists
        } else {
            ManualBackupError::Failed
        }
    })?;
    Ok(ManualBackupOutcome::Created)
}

#[cfg(test)]
fn run_manual_backup(
    picker: impl FnOnce() -> Result<Option<PathBuf>, ManualBackupError>,
    source: impl FnOnce() -> Result<Connection, ManualBackupError>,
) -> Result<ManualBackupOutcome, ManualBackupError> {
    run_manual_backup_coordinated(picker, source, &DatabaseState::ready_for_test())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn source(path: &Path) {
        let connection = Connection::open(path).unwrap();
        database::initialize_schema(&connection).unwrap();
        connection.execute("INSERT INTO journal_entries VALUES ('entry', '2026-10-17', 'Test', '{}', 1, 'created', 'updated')", []).unwrap();
    }

    #[test]
    fn manual_backup_is_verified_and_source_is_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("source.sqlite");
        source(&path);
        let original = fs::read(&path).unwrap();
        let destination = dir.path().join("chosen.mno-backup");
        assert_eq!(
            run_manual_backup(|| Ok(Some(destination.clone())), || open_source(&path)),
            Ok(ManualBackupOutcome::Created)
        );
        assert_eq!(
            backup::verify_backup(&destination).unwrap().kind,
            BackupKind::Manual
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn cancelled_dialog_does_not_open_source_or_create_files() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            run_manual_backup(|| Ok(None), || panic!("cancel must not open source")),
            Ok(ManualBackupOutcome::Cancelled)
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn existing_file_and_invalid_destination_are_rejected_before_source_access() {
        let dir = TempDir::new().unwrap();
        let existing = dir.path().join("existing.mno-backup");
        fs::write(&existing, "keep original").unwrap();
        assert_eq!(
            run_manual_backup(
                || Ok(Some(existing.clone())),
                || panic!("must not open source")
            ),
            Err(ManualBackupError::AlreadyExists)
        );
        assert_eq!(fs::read_to_string(existing).unwrap(), "keep original");
        for destination in [
            dir.path().join("missing/file.mno-backup"),
            dir.path().join("bad.txt"),
        ] {
            assert_eq!(
                run_manual_backup(|| Ok(Some(destination)), || panic!("must not open source")),
                Err(ManualBackupError::InvalidDestination)
            );
        }
    }

    #[test]
    fn missing_source_is_not_created_and_failure_leaves_no_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.sqlite");
        let destination = dir.path().join("backup.mno-backup");
        assert_eq!(
            run_manual_backup(|| Ok(Some(destination.clone())), || open_source(&path)),
            Err(ManualBackupError::Failed)
        );
        assert!(!path.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn duplicate_starts_are_rejected_and_permit_released_after_cancel_or_error() {
        let state = ManualBackupState::default();
        let other_request = state.clone();
        {
            let _permit = state.try_start().unwrap();
            assert!(matches!(
                other_request.try_start(),
                Err(ManualBackupError::Busy)
            ));
            assert_eq!(
                run_manual_backup(|| Ok(None), || panic!("cancel")),
                Ok(ManualBackupOutcome::Cancelled)
            );
        }
        {
            let _permit = other_request.try_start().unwrap();
            assert_eq!(
                run_manual_backup(
                    || Err(ManualBackupError::InvalidDestination),
                    || panic!("error")
                ),
                Err(ManualBackupError::InvalidDestination)
            );
        }
        assert!(state.try_start().is_ok());
    }

    #[test]
    fn filename_without_extension_gets_backup_extension() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("source.sqlite");
        source(&path);
        assert_eq!(
            run_manual_backup(
                || Ok(Some(dir.path().join("chosen"))),
                || open_source(&path)
            ),
            Ok(ManualBackupOutcome::Created)
        );
        backup::verify_backup(&dir.path().join("chosen.mno-backup")).unwrap();
    }
    #[test]
    fn picker_and_cancellation_hold_no_database_permit_during_maintenance() {
        let state = DatabaseState::ready_for_test();
        assert_eq!(
            run_manual_backup_coordinated(
                || {
                    let mut maintenance = state.maintenance().unwrap();
                    maintenance.advance_generation().unwrap();
                    Ok(None)
                },
                || panic!("cancelled"),
                &state
            ),
            Ok(ManualBackupOutcome::Cancelled)
        );
        let dir = TempDir::new().unwrap();
        // Use a real maintenance guard on another thread for the exclusive phase.
        let state = std::sync::Arc::new(state);
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let other = state.clone();
        let worker = std::thread::spawn(move || {
            let _maintenance = other.maintenance().unwrap();
            acquired_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Dialog cancellation remains possible even during maintenance.
        assert_eq!(
            run_manual_backup_coordinated(|| Ok(None), || panic!("cancelled"), &state),
            Ok(ManualBackupOutcome::Cancelled)
        );
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(!dir.path().join("cancelled.mno-backup").exists());
    }
}

#[cfg(test)]
mod startup_gate_tests {
    use super::*;
    #[test]
    fn startup_blocks_backup_before_source_open_or_file_creation() {
        let dir = tempfile::TempDir::new().unwrap();
        let destination = dir.path().join("blocked.mno-backup");
        let state = DatabaseState::default();
        let result = run_manual_backup_coordinated(
            || Ok(Some(destination.clone())),
            || panic!("source must not open"),
            &state,
        );
        assert!(matches!(result, Err(ManualBackupError::Busy)));
        assert!(!destination.exists());
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod established_startup_tests {
    use super::*;
    #[test]
    fn real_current_startup_creates_verified_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("data");
        let state = DatabaseState::default();
        let path = state.initialize_startup(&root).unwrap();
        crate::journal::save_started_fixture(&state, &path).unwrap();
        let destination = dir.path().join("manual.mno-backup");
        assert_eq!(
            run_manual_backup_coordinated(
                || Ok(Some(destination.clone())),
                || open_source(&path),
                &state
            ),
            Ok(ManualBackupOutcome::Created)
        );
        let manifest = backup::verify_backup(&destination).unwrap();
        assert_eq!(
            (manifest.format_version, manifest.database_schema_version),
            if cfg!(target_os = "macos") {
                (6, 5)
            } else {
                (5, 4)
            }
        );
        assert_eq!(
            Connection::open(&path)
                .unwrap()
                .query_row("SELECT title FROM journal_entries", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "Saved after startup"
        );
    }
}
