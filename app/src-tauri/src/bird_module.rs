//! Account/installation preference, deliberately outside journal/archive content.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{RwLock, RwLockReadGuard},
};
use tauri::Manager;

// On macOS app_config_dir and app_data_dir can coincide. Keep preferences in
// a separate sibling namespace so journal admission and replacement never see them.
pub(crate) fn preference_path(config: &std::path::Path, identifier: &str) -> PathBuf {
    config
        .join("org.mynatureobservations.preferences")
        .join(identifier)
        .join("bird-module-v1.json")
}
const DISABLED: &str = "bird_module_disabled";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    version: u8,
    enabled: bool,
}
pub(crate) struct BirdModule {
    path: PathBuf,
    enabled: RwLock<bool>,
}
impl BirdModule {
    pub(crate) fn load(path: PathBuf) -> Self {
        let enabled = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Preference>(&b).ok())
            .filter(|p| p.version == 1)
            .is_some_and(|p| p.enabled);
        Self {
            path,
            enabled: RwLock::new(enabled),
        }
    }
    pub(crate) fn enabled(&self) -> bool {
        self.enabled.read().map(|v| *v).unwrap_or(false)
    }
    // Kept for the entire command, including its transaction. Disabling waits for
    // admitted work; no journal lock or archive proof depends on this preference.
    pub(crate) fn operation(&self) -> Result<RwLockReadGuard<'_, bool>, String> {
        let permit = self.enabled.read().map_err(|_| DISABLED.to_owned())?;
        if !*permit {
            return Err(DISABLED.into());
        }
        Ok(permit)
    }
    pub(crate) fn command(&self) -> Result<RwLockReadGuard<'_, bool>, String> {
        if !cfg!(target_os = "macos") {
            return Err("bird_platform_unsupported".into());
        }
        self.operation()
    }
    pub(crate) fn set(
        &self,
        enabled: bool,
        invalidate: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut current = self
            .enabled
            .write()
            .map_err(|_| "bird_module_preference_failed")?;
        invalidate()?;
        let parent = self.path.parent().ok_or("bird_module_preference_failed")?;
        fs::create_dir_all(parent).map_err(|_| "bird_module_preference_failed")?;
        let temp = parent.join(format!(".bird-module-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&serde_json::to_vec(&Preference {
                version: 1,
                enabled,
            })?)?;
            file.sync_all()?;
            fs::rename(&temp, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
            return Err("bird_module_preference_failed".into());
        }
        *current = enabled;
        Ok(())
    }
}
#[tauri::command]
pub(crate) fn bird_module_supported() -> bool {
    cfg!(target_os = "macos")
}
#[tauri::command]
pub(crate) fn bird_module_enabled(module: tauri::State<'_, BirdModule>) -> bool {
    cfg!(target_os = "macos") && module.enabled()
}
#[tauri::command]
pub(crate) async fn set_bird_module_enabled(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<BirdModule>().set(enabled, || {
            app.state::<crate::ebird_app::ImportState>().invalidate()?;
            app.state::<crate::bird_weather_fetch::PreviewState>()
                .invalidate()
        })
    })
    .await
    .map_err(|_| "bird_module_preference_failed".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_are_outside_both_journal_roots() {
        let base = PathBuf::from("/synthetic/config");
        for id in [
            "org.mynatureobservations.app",
            "org.mynatureobservations.app.dev",
        ] {
            assert!(!preference_path(&base, id).starts_with(base.join(id)));
        }
        assert_ne!(
            preference_path(&base, "org.mynatureobservations.app"),
            preference_path(&base, "org.mynatureobservations.app.dev")
        );
    }
    #[test]
    fn default_off_persistent_and_profile_isolated() {
        let root = std::env::temp_dir().join(format!("mno-module-{}", uuid::Uuid::new_v4()));
        let prod = root.join("org.mynatureobservations.app/bird-module.json");
        let dev = root.join("org.mynatureobservations.app.dev/bird-module.json");
        let module = BirdModule::load(prod.clone());
        assert!(!module.enabled());
        assert!(module.operation().is_err());
        module.set(true, || Ok(())).unwrap();
        assert!(BirdModule::load(prod.clone()).enabled());
        assert!(!BirdModule::load(dev).enabled());
        module.set(false, || Ok(())).unwrap();
        assert!(!BirdModule::load(prod.clone()).enabled());
        fs::write(&prod, br#"{"version":2,"enabled":true}"#).unwrap();
        assert!(!BirdModule::load(prod).enabled());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn disable_waits_for_admitted_operation_and_invalidates_before_completion() {
        let root = std::env::temp_dir().join(format!("mno-module-{}", uuid::Uuid::new_v4()));
        let module = std::sync::Arc::new(BirdModule::load(root.join("bird-module.json")));
        module.set(true, || Ok(())).unwrap();
        let permit = module.operation().unwrap();
        let (started, ready) = std::sync::mpsc::channel();
        let copy = module.clone();
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            copy.set(false, || Ok(())).unwrap();
        });
        ready.recv().unwrap();
        assert!(module.enabled.try_write().is_err());
        drop(permit);
        worker.join().unwrap();
        assert!(module.operation().is_err());
        module.set(true, || Ok(())).unwrap();
        assert!(module.operation().is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn catalog_commands_reject_disabled_and_work_after_enable() {
        let off = super::test_module(false);
        assert_eq!(
            crate::bird_app::search_bird_species_with_module(&off, "Skata".into()).unwrap_err(),
            DISABLED
        );
        assert_eq!(
            crate::bird_app::match_bird_paste_names_with_module(&off, vec!["Skata".into()])
                .unwrap_err(),
            DISABLED
        );
        let on = super::test_module(true);
        assert!(
            !crate::bird_app::search_bird_species_with_module(&on, "Skata".into())
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn failed_invalidation_leaves_choice_unchanged() {
        let module = BirdModule::load(PathBuf::from("unused"));
        assert!(module.set(true, || Err("failed".into())).is_err());
        assert!(!module.enabled());
    }
}

#[cfg(test)]
pub(crate) fn test_module(enabled: bool) -> BirdModule {
    BirdModule {
        path: PathBuf::from("unused-test-preference"),
        enabled: RwLock::new(enabled),
    }
}
