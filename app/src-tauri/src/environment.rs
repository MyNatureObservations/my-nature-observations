//! Application identity and data-root policy. Resolving paths never opens SQLite.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tauri::Manager;

pub(crate) const PRODUCTION_ID: &str = "org.mynatureobservations.app";
pub(crate) const DEVELOPMENT_BUILD: bool = cfg!(debug_assertions) || cfg!(dev);

pub(crate) fn validate_identity(identifier: &str, development_build: bool) -> Result<(), String> {
    if development_build && identifier == PRODUCTION_ID {
        return Err(
            "development_requires_separate_application_identity: use npm run tauri dev".into(),
        );
    }
    Ok(())
}

// Resolve existing ancestors too, so a symlink cannot disguise an overlapping root.
fn location(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(
            "environment_data_directory_requires_absolute_path_without_parent_components".into(),
        );
    }
    match fs::symlink_metadata(path) {
        Ok(_) => fs::canonicalize(path).map_err(|e| format!("environment_data_directory: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or("environment_data_directory_invalid")?;
            let name = path
                .file_name()
                .ok_or("environment_data_directory_invalid")?;
            Ok(location(parent)?.join(name))
        }
        Err(e) => Err(format!("environment_data_directory: {e}")),
    }
}

pub(crate) fn resolve_directory(
    base: &Path,
    identifier: &str,
    development_build: bool,
    debug_build: bool,
    test_directory: Option<&Path>,
) -> Result<PathBuf, String> {
    validate_identity(identifier, development_build)?;
    let production = base.join(PRODUCTION_ID);
    if identifier == PRODUCTION_ID {
        // Production ignores test overrides and retains its original location.
        return Ok(production);
    }
    let directory = if debug_build {
        test_directory.map(Path::to_path_buf)
    } else {
        None
    }
    .unwrap_or_else(|| base.join(identifier));
    let actual = location(&directory)?;
    let protected = location(&production)?;
    // Reserved app IDs must also stay distinct on case-insensitive filesystems.
    let comparable = |path: &Path| {
        if cfg!(any(target_os = "macos", windows)) {
            PathBuf::from(path.to_string_lossy().to_lowercase())
        } else {
            path.to_path_buf()
        }
    };
    let actual = comparable(&actual);
    let protected = comparable(&protected);
    if actual.starts_with(&protected) || protected.starts_with(&actual) {
        return Err("development_data_directory_overlaps_production".into());
    }
    Ok(directory)
}

pub(crate) fn data_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app.path().data_dir().map_err(|e| e.to_string())?;
    let test_directory = if cfg!(debug_assertions) {
        std::env::var_os("MNO_TEST_DATA_DIR").map(PathBuf::from)
    } else {
        None
    };
    resolve_directory(
        &base,
        &app.config().identifier,
        DEVELOPMENT_BUILD,
        cfg!(debug_assertions),
        test_directory.as_deref(),
    )
}

#[tauri::command]
pub(crate) fn development_mode(app: tauri::AppHandle) -> bool {
    app.config().identifier != PRODUCTION_ID
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEVELOPMENT_ID: &str = "org.mynatureobservations.app.dev";

    #[test]
    fn identities_and_default_directories_are_distinct_and_production_is_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let prod = resolve_directory(root.path(), PRODUCTION_ID, false, false, None).unwrap();
        let dev = resolve_directory(root.path(), DEVELOPMENT_ID, true, true, None).unwrap();
        assert_eq!(prod, root.path().join("org.mynatureobservations.app"));
        assert_eq!(dev, root.path().join("org.mynatureobservations.app.dev"));
        assert_ne!(prod, dev);
        assert!(!dev.exists());
        assert!(!prod.exists());
        assert_eq!(
            resolve_directory(
                root.path(),
                PRODUCTION_ID,
                false,
                false,
                Some(Path::new("/ignored"))
            )
            .unwrap(),
            prod
        );
        assert!(validate_identity(PRODUCTION_ID, true).is_err());
        assert!(
            resolve_directory(root.path(), PRODUCTION_ID, true, true, Some(root.path())).is_err()
        );
    }

    #[test]
    fn explicit_isolated_profiles_are_debug_only_and_must_not_overlap_production() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("isolated");
        assert_eq!(
            resolve_directory(root.path(), DEVELOPMENT_ID, true, true, Some(&profile)).unwrap(),
            profile
        );
        assert_eq!(
            resolve_directory(root.path(), "org.mno.synthetic", true, true, Some(&profile))
                .unwrap(),
            profile
        );
        assert_eq!(
            resolve_directory(root.path(), DEVELOPMENT_ID, true, false, Some(&profile)).unwrap(),
            root.path().join(DEVELOPMENT_ID)
        );
        let prod = root.path().join(PRODUCTION_ID);
        for candidate in [
            prod.clone(),
            prod.join("nested"),
            root.path().to_path_buf(),
            PathBuf::from("relative"),
            prod.join("../alias"),
        ] {
            assert!(
                resolve_directory(root.path(), DEVELOPMENT_ID, true, true, Some(&candidate))
                    .is_err(),
                "{candidate:?}"
            );
        }
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn case_alias_is_rejected_even_before_production_exists() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            resolve_directory(
                root.path(),
                DEVELOPMENT_ID,
                true,
                true,
                Some(&root.path().join("ORG.MYNATUREOBSERVATIONS.APP"))
            )
            .is_err()
        );
        assert!(!root.path().join(PRODUCTION_ID).exists());
    }

    #[cfg(unix)]
    #[test]
    fn aliases_of_production_and_dev_aliases_are_rejected_without_touching_files() {
        let root = tempfile::tempdir().unwrap();
        let prod = root.path().join(PRODUCTION_ID);
        fs::create_dir(&prod).unwrap();
        fs::write(prod.join("mno.sqlite"), b"untouched production sentinel").unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&prod, &alias).unwrap();
        assert!(
            resolve_directory(
                root.path(),
                DEVELOPMENT_ID,
                true,
                true,
                Some(&alias.join("child"))
            )
            .is_err()
        );
        std::os::unix::fs::symlink(&prod, root.path().join(DEVELOPMENT_ID)).unwrap();
        assert!(resolve_directory(root.path(), DEVELOPMENT_ID, true, true, None).is_err());
        assert_eq!(
            fs::read(prod.join("mno.sqlite")).unwrap(),
            b"untouched production sentinel"
        );
    }

    #[test]
    fn synthetic_dev_and_production_can_hold_locks_and_persist_independently() {
        let root = tempfile::tempdir().unwrap();
        let prod = resolve_directory(root.path(), PRODUCTION_ID, false, false, None).unwrap();
        let dev = resolve_directory(root.path(), DEVELOPMENT_ID, true, true, None).unwrap();
        // Both are synthetic roots. Never use the operating system's real data root.
        let production = crate::database_state::DatabaseState::default();
        let development = crate::database_state::DatabaseState::default();
        let prod_file = production.initialize_startup(&prod).unwrap();
        let dev_file = development.initialize_startup(&dev).unwrap();
        let json = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"DEV only"}]}]}"#;
        let mut db = rusqlite::Connection::open(&dev_file).unwrap();
        crate::photographs::save(&mut db, "dev-entry", "2026-10-31", "DEV", json, &[], true).unwrap();
        drop(db);
        assert_eq!(
            rusqlite::Connection::open(&prod_file)
                .unwrap()
                .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(development);
        drop(production);
        let reopened = crate::database_state::DatabaseState::default();
        let file = reopened.initialize_startup(&dev).unwrap();
        assert_eq!(
            rusqlite::Connection::open(file)
                .unwrap()
                .query_row("SELECT content_json FROM journal_entries", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            json
        );
    }
}
