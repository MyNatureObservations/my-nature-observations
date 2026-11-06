//! Expected startup failures get a native message; programming panics still abort.
use crate::database_state::DatabaseState;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    InUse,
    Invalid,
    Unavailable,
}
impl Failure {
    pub(crate) fn from_error(error: &str) -> Self {
        if error == "database_directory_in_use" {
            Self::InUse
        } else if error.starts_with("database_schema_")
            || error.starts_with("database_content_validation_failed")
        {
            Self::Invalid
        } else {
            Self::Unavailable
        }
    }
    fn message(&self) -> String {
        let reason = match self {
            Self::InUse => {
                "Journalens datakatalog används redan.\nThe journal data directory is already in use."
            }
            Self::Invalid => {
                "Journalen kunde inte godkännas vid kontrollen.\nThe journal could not pass validation."
            }
            Self::Unavailable => {
                "Ett problem uppstod när journalen skulle öppnas.\nA problem occurred while opening the journal."
            }
        };
        format!(
            "{reason}\n\nJournalen har inte öppnats. Behåll dina befintliga filer och säkerhetskopior tills problemet har undersökts.\n\nThe journal has not been opened. Keep your existing files and backups until the problem has been investigated."
        )
    }
}

pub(crate) fn initialize(
    state: &DatabaseState,
    directory: &Path,
) -> Result<Option<PathBuf>, Failure> {
    match state.initialize_startup(directory) {
        Ok(path) => Ok(Some(path)),
        Err(_) if state.startup_recovery_pending() => Ok(None),
        #[cfg(windows)]
        Err(_) if state.windows_recovery_available() => Ok(None),
        Err(error) => Err(Failure::from_error(&error)),
    }
}

pub(crate) fn show(app: &tauri::AppHandle, failure: Failure) {
    // No DataLocation is installed. Keep the normal journal window inaccessible.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let exit = app.clone();
    app.dialog()
        .message(failure.message())
        .title("MNO kunde inte öppna journalen / MNO could not open the journal")
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::OkCustom(
            "Avsluta MNO / Exit MNO".into(),
        ))
        // Nonblocking: setup runs on the main thread. Closing always exits.
        .show(move |_| exit.exit(1));
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    fn blocked(state: &DatabaseState) {
        assert!(state.ready_database_path().is_err());
        assert!(state.operation(None).is_err());
        assert!(!state.startup_recovery_pending());
    }
    #[test]
    fn inaccessible_directory_is_reported_and_never_admitted() {
        use std::os::unix::fs::PermissionsExt;
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("denied");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
        let state = DatabaseState::default();
        let result = initialize(&state, &root);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result, Err(Failure::Unavailable));
        blocked(&state);
        assert!(!root.join("mno.sqlite").exists());
    }
    #[test]
    fn unreadable_database_is_not_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("mno.sqlite");
        fs::write(&path, b"synthetic inaccessible journal").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let state = DatabaseState::default();
        let result = initialize(&state, root.path());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(result, Err(Failure::Unavailable));
        blocked(&state);
        assert_eq!(fs::read(path).unwrap(), b"synthetic inaccessible journal");
    }
    #[test]
    fn invalid_sqlite_is_not_repaired_or_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("mno.sqlite");
        fs::write(&path, b"not a SQLite database").unwrap();
        let state = DatabaseState::default();
        assert_eq!(initialize(&state, root.path()), Err(Failure::Unavailable));
        blocked(&state);
        assert_eq!(fs::read(path).unwrap(), b"not a SQLite database");
    }
    #[test]
    fn locked_directory_is_reported_without_admission() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data");
        let first = DatabaseState::default();
        initialize(&first, &path).unwrap();
        let second = DatabaseState::default();
        assert_eq!(initialize(&second, &path), Err(Failure::InUse));
        blocked(&second);
    }
    #[test]
    fn rejected_schema_is_reported_without_rewriting_database() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("data");
        let first = DatabaseState::default();
        let path = initialize(&first, &root).unwrap().unwrap();
        drop(first);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", 999).unwrap();
        drop(db);
        let before = fs::read(&path).unwrap();
        let state = DatabaseState::default();
        assert_eq!(initialize(&state, &root), Err(Failure::Invalid));
        blocked(&state);
        assert_eq!(fs::read(path).unwrap(), before);
    }
    #[test]
    fn recovery_gate_is_retained_without_normal_journal_access() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("recovery")).unwrap();
        let state = DatabaseState::default();
        assert_eq!(initialize(&state, root.path()), Ok(None));
        assert!(state.startup_recovery_pending());
        assert!(state.ready_database_path().is_err());
        assert!(state.operation(None).is_err());
    }
    #[test]
    fn messages_are_bilingual_and_do_not_include_raw_errors() {
        for error in [
            "database_directory_in_use",
            "database_schema_incompatible",
            "database_content_validation_failed: secret",
            "/private/sensitive/path: cannot open",
        ] {
            let text = Failure::from_error(error).message();
            assert!(text.contains("Behåll dina befintliga filer"));
            assert!(text.contains("Keep your existing files and backups"));
            assert!(!text.contains("secret"));
            assert!(!text.contains("/private/"));
        }
        let source = include_str!("startup_error.rs")
            .split("#[cfg(all(test, unix))]")
            .next()
            .unwrap();
        assert!(source.contains(".show(move |_| exit.exit(1))"));
        assert!(!source.contains(".blocking_show("));
    }
}
