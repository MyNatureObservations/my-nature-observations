use std::collections::HashMap;
use tauri::{
    Emitter, Manager,
    menu::{
        AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
        WINDOW_SUBMENU_ID,
    },
};

const ZOOM_IDS: [&str; 3] = ["mno-zoom-in", "mno-zoom-out", "mno-zoom-reset"];
const QUIT_ID: &str = "mno-guarded-quit";

// Retain handles, never identify menus or commands by their translated text.
struct LocalizedMenu(Vec<(&'static str, MenuItemKind<tauri::Wry>)>);

pub fn install_guarded_quit(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let mut localized = Vec::new();
    // Same menu order, separators and predefined actions as Tauri 2.12.3's default
    // macOS menu. Only Quit remains custom, preserving MNO's close guard.
    macro_rules! native {
        ($key:literal, $item:expr) => {{
            let item = $item?;
            localized.push(($key, MenuItemKind::Predefined(item.clone())));
            item
        }};
    }
    macro_rules! heading {
        ($key:literal, $id:expr, $items:expr) => {{
            // Preserve usable Tauri-default headings before frontend locale initialization.
            let fallback = match $key {
                "file" => "File",
                "edit" => "Edit",
                "view" => "View",
                "window" => "Window",
                "help" => "Help",
                _ => unreachable!(),
            };
            let item = Submenu::with_id_and_items(app, $id, fallback, true, $items)?;
            localized.push(($key, MenuItemKind::Submenu(item.clone())));
            item
        }};
    }
    let package = app.package_info();
    let config = app.config();
    let about = native!(
        "about",
        PredefinedMenuItem::about(
            app,
            None,
            Some(AboutMetadata {
                name: Some(package.name.clone()),
                version: Some(package.version.to_string()),
                copyright: config.bundle.copyright.clone(),
                authors: config.bundle.publisher.clone().map(|p| vec![p]),
                ..Default::default()
            })
        )
    );
    let services = native!("services", PredefinedMenuItem::services(app, None));
    let hide = native!("hide", PredefinedMenuItem::hide(app, None));
    let hide_others = native!("hideOthers", PredefinedMenuItem::hide_others(app, None));
    let quit_title = PredefinedMenuItem::quit(app, None)?.text()?;
    let quit = MenuItem::with_id(app, QUIT_ID, quit_title, true, Some("Cmd+Q"))?;
    localized.push(("quit", MenuItemKind::MenuItem(quit.clone())));
    let app_menu = Submenu::with_id_and_items(
        app,
        "mno-application",
        package.name.clone(),
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(app)?,
            &services,
            &PredefinedMenuItem::separator(app)?,
            &hide,
            &hide_others,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let file_close = native!("close", PredefinedMenuItem::close_window(app, None));
    let file = heading!("file", "mno-file", &[&file_close]);
    let undo = native!("undo", PredefinedMenuItem::undo(app, None));
    let redo = native!("redo", PredefinedMenuItem::redo(app, None));
    let cut = native!("cut", PredefinedMenuItem::cut(app, None));
    let copy = native!("copy", PredefinedMenuItem::copy(app, None));
    let paste = native!("paste", PredefinedMenuItem::paste(app, None));
    let select_all = native!("selectAll", PredefinedMenuItem::select_all(app, None));
    let edit = heading!(
        "edit",
        "mno-edit",
        &[
            &undo,
            &redo,
            &PredefinedMenuItem::separator(app)?,
            &cut,
            &copy,
            &paste,
            &select_all,
        ]
    );
    let fullscreen = native!("fullscreen", PredefinedMenuItem::fullscreen(app, None));
    let view = heading!(
        "view",
        "mno-view",
        &[&fullscreen, &PredefinedMenuItem::separator(app)?]
    );
    for (id, key) in ZOOM_IDS
        .into_iter()
        .zip(["zoomIn", "zoomOut", "actualSize"])
    {
        // The shared frontend controller owns shortcuts, avoiding double dispatch.
        let item = MenuItem::with_id(app, id, "", false, None::<&str>)?;
        view.append(&item)?;
        localized.push((key, MenuItemKind::MenuItem(item)));
    }
    let minimize = native!("minimize", PredefinedMenuItem::minimize(app, None));
    let maximize = native!("maximize", PredefinedMenuItem::maximize(app, None));
    let window_close = native!("close", PredefinedMenuItem::close_window(app, None));
    let window = heading!(
        "window",
        WINDOW_SUBMENU_ID,
        &[
            &minimize,
            &maximize,
            &PredefinedMenuItem::separator(app)?,
            &window_close,
        ]
    );
    let help = heading!("help", HELP_SUBMENU_ID, &[]);
    let menu = Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])?;
    app.set_menu(menu)?;
    if !app.manage(LocalizedMenu(localized)) {
        return Err("Native menu already installed".into());
    }
    app.on_menu_event(|app, event| {
        if let Some(index) = ZOOM_IDS.iter().position(|id| *id == event.id().as_ref()) {
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) =
                    window.emit_to(window.label(), "mno:zoom", ["in", "out", "reset"][index])
                {
                    eprintln!("Could not request application zoom: {error}");
                }
            }
            return;
        }
        if event.id().as_ref() != QUIT_ID {
            return;
        }
        if let Some(window) = app.get_webview_window("main") {
            // close() emits CloseRequested; destroy()/exit() would bypass the guard.
            if let Err(error) = window.close() {
                eprintln!("Could not request journal window close: {error}");
            }
        } else {
            app.exit(0);
        }
    });
    Ok(())
}

pub fn update_labels(
    app: &tauri::AppHandle,
    labels: &HashMap<String, String>,
) -> Result<(), String> {
    let menu = app.state::<LocalizedMenu>();
    // Validate the complete request before mutating any native labels.
    if menu
        .0
        .iter()
        .any(|(key, _)| labels.get(*key).is_none_or(|text| text.trim().is_empty()))
    {
        return Err("Incomplete native menu labels".into());
    }
    for (key, item) in &menu.0 {
        let text = &labels[*key];
        match item {
            MenuItemKind::Submenu(item) => item.set_text(text),
            MenuItemKind::Predefined(item) => item.set_text(text),
            MenuItemKind::MenuItem(item) => item.set_text(text),
            _ => return Err("Unsupported localized menu item".into()),
        }
        .map_err(|error| error.to_string())?;
    }
    // Enable zoom commands only after every label has been updated and the
    // frontend listener is ready. No menus, actions or handlers are recreated.
    for (_, item) in &menu.0 {
        if let MenuItemKind::MenuItem(item) = item {
            if ZOOM_IDS.iter().any(|id| item.id().as_ref() == *id) {
                item.set_enabled(true).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}
