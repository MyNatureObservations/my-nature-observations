mod about;
mod environment;
mod photographs;
mod startup_error;
// Shared backup primitives; some verification APIs are reserved for later steps.
#[allow(dead_code)]
mod backup;
mod database;
mod database_state;
mod export_app;
mod import_app;
mod journal;
mod journal_history;
mod journal_search;
#[cfg(target_os = "macos")]
mod macos_menu;
mod manual_backup;
// Open-export engine; the app boundary retains its read-only source owner.
#[allow(dead_code)]
mod open_export;
// Open-import engine; activation uses its separate import validation contract.
#[allow(dead_code)]
mod open_import;
mod panic_policy;
mod place_registry;
mod recovery;
mod restore_app;
mod startup;
// Validated weather snapshots, internal persistence and read-only UI; no network.
#[allow(dead_code)]
mod weather;
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn update_native_menu_labels(
    app: tauri::AppHandle,
    labels: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos_menu::update_labels(&app, &labels);
    #[cfg(not(target_os = "macos"))]
    {
        // Native menu localization is macOS-only; other platforms keep existing behavior.
        let _ = (app, labels);
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    panic_policy::install();

    let context = tauri::generate_context!();
    // Validate before plugins can contact another instance or any journal is opened.
    environment::validate_identity(&context.config().identifier, environment::DEVELOPMENT_BUILD)
        .expect("unsafe application environment");

    #[cfg(target_os = "linux")]
    let context = {
        let mut context = context;
        let config = context.config_mut();
        // Tauri creates WebKit storage before setup. Its default on Linux is
        // the journal root, which must remain absent until first initialization.
        // A configured path resolves below local data / window label.
        let webview_data = std::path::Path::new(&config.identifier).join("webview");
        for window in &mut config.app.windows {
            window.data_directory = Some(webview_data.clone());
        }
        context
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(manual_backup::ManualBackupState::default())
        .manage(journal_search::SearchState::default())
        .manage(restore_app::RestoreState::default())
        .manage(import_app::ImportState::default())
        .manage(database_state::DatabaseState::default())
        .manage(weather::historical::saving::PreviewState::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            macos_menu::install_guarded_quit(app)?;

            use tauri::Manager;
            let directory = match database::resolve_data_directory(app.handle()) {
                Ok(directory) => directory,
                Err(error) => {
                    startup_error::show(app.handle(), startup_error::Failure::from_error(&error));
                    return Ok(());
                }
            };
            if app.config().identifier != environment::PRODUCTION_ID {
                if let Some(window) = app.get_webview_window("main") {
                    window.set_title("My Nature Observations — DEV")?;
                }
                #[cfg(debug_assertions)]
                eprintln!("MNO DEV data directory: {}", directory.display());
            }
            let state = app.state::<database_state::DatabaseState>();
            match install_journal_location(&state, &directory, |location| app.manage(location)) {
                Ok(true) => (),
                Ok(false) => {
                    return Err(std::io::Error::other("database_location_already_installed").into());
                }
                Err(failure) => startup_error::show(app.handle(), failure),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            update_native_menu_labels,
            about::open_third_party_notices,
            greet,
            environment::development_mode,
            recovery::database_mode,
            recovery::recovery_status,
            recovery::run_recovery,
            import_app::open_import_available,
            import_app::choose_open_import,
            import_app::cancel_open_import,
            import_app::activate_open_import,
            restore_app::choose_recovery_backup,
            restore_app::confirm_recovery_backup,
            restore_app::restore_available,
            restore_app::choose_restore_backup,
            restore_app::restore_selected_backup,
            restore_app::quit_after_restore,
            restore_app::close_journal_window,
            database_state::new_journal_session,
            manual_backup::create_manual_backup,
            export_app::create_open_export,
            journal_search::search_journal,
            journal_history::journal_history_overview,
            journal_history::journal_history_entries,
            journal::save_journal_entry,
            photographs::prepare_journal_photograph,
            photographs::select_journal_photograph,
            photographs::save_journal_with_photographs,
            photographs::get_journal_photograph,
            journal::load_journal_entry,
            journal::get_journal_entry,
            journal::read_journal_entries,
            weather::view::read_journal_weather,
            weather::view::update_manual_weather,
            weather::historical::saving::preview_historical_weather,
            weather::historical::saving::save_historical_weather,
            place_registry::create_place,
            place_registry::list_places,
            place_registry::get_place,
            place_registry::update_place,
            place_registry::delete_place,
            place_registry::read_place_usage,
            place_registry::read_journal_place,
            place_registry::set_journal_place,
            place_registry::read_weather_default_place,
            place_registry::set_weather_default_place,
            journal::list_journal_entries,
            journal::list_journal_entries_for_date,
            journal::list_journal_dates_for_month
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            use tauri::Manager;
            // The same reservation covers native close/menu quit and explicit exit.
            if app.state::<restore_app::RestoreState>().is_busy() {
                match event {
                    tauri::RunEvent::ExitRequested { api, .. } => api.prevent_exit(),
                    tauri::RunEvent::WindowEvent {
                        event: tauri::WindowEvent::CloseRequested { api, .. },
                        ..
                    } => api.prevent_close(),
                    _ => {}
                }
            }
        });
}

// Isolated database evolution experiment; absent from non-test builds.
#[cfg(test)]
#[path = "database/schema_v1.rs"]
mod schema_v1;

mod storage_content;

// Shared startup-to-state boundary. Only app setup supplies Tauri's state installer.
fn install_journal_location(
    state: &database_state::DatabaseState,
    directory: &std::path::Path,
    install: impl FnOnce(database::DataLocation) -> bool,
) -> Result<bool, startup_error::Failure> {
    match startup_error::initialize(state, directory)? {
        Some(path) => Ok(install(database::DataLocation::new(path))),
        None => Ok(true),
    }
}
