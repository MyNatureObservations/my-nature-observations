//! DEV-only external CSV read. No journal/database state or creation reservations.
use std::io::Read;
use tauri::Manager;
const LIMIT: u64 = 16 * 1024 * 1024;
fn allowed(macos: bool, development: bool, enabled: bool) -> bool {
    macos && development && enabled
}
#[tauri::command]
pub(crate) fn ebird_preview_available(
    module: tauri::State<'_, crate::bird_module::BirdModule>,
) -> bool {
    allowed(
        cfg!(target_os = "macos"),
        crate::environment::DEVELOPMENT_BUILD,
        module.enabled(),
    )
}
fn read_csv(path: &std::path::Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|_| "ebird_unreadable")?;
    if !file.metadata().map_err(|_| "ebird_unreadable")?.is_file() {
        return Err("ebird_unreadable".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "ebird_unreadable")?;
    if bytes.len() as u64 > LIMIT {
        return Err("ebird_too_large".into());
    }
    String::from_utf8(bytes).map_err(|_| "ebird_encoding".into())
}
#[tauri::command]
pub(crate) async fn preview_ebird_export(
    app: tauri::AppHandle,
    locale: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let module = app.state::<crate::bird_module::BirdModule>();
        if !allowed(
            cfg!(target_os = "macos"),
            crate::environment::DEVELOPMENT_BUILD,
            module.enabled(),
        ) {
            return Err("ebird_preview_unavailable".into());
        }
        let path = app
            .dialog()
            .file()
            .set_title(if locale == "sv" {
                "Välj personlig eBird-export"
            } else {
                "Choose personal eBird export"
            })
            .add_filter("CSV", &["csv"])
            .blocking_pick_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let _permit = module.command()?;
        let path = path.into_path().map_err(|_| "ebird_unreadable")?;
        read_csv(&path).map(Some)
    })
    .await
    .map_err(|_| "ebird_unreadable".to_owned())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_requires_all_three_gates() {
        for macos in [false, true] {
            for dev in [false, true] {
                for enabled in [false, true] {
                    assert_eq!(allowed(macos, dev, enabled), macos && dev && enabled);
                }
            }
        }
    }
    #[test]
    fn selected_file_is_unchanged_and_utf8_errors_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.csv");
        let data = b"\xef\xbb\xbfSubmission ID,Count\r\nS1,X\r\n";
        std::fs::write(&path, data).unwrap();
        assert!(read_csv(&path).unwrap().starts_with('\u{feff}'));
        assert_eq!(std::fs::read(&path).unwrap(), data);
        std::fs::write(&path, [255]).unwrap();
        assert_eq!(read_csv(&path).unwrap_err(), "ebird_encoding");
        assert_eq!(read_csv(dir.path()).unwrap_err(), "ebird_unreadable");
    }
    #[test]
    fn oversized_and_missing_files_are_rejected_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.csv");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(LIMIT + 1).unwrap();
        drop(file);
        assert_eq!(read_csv(&path).unwrap_err(), "ebird_too_large");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), LIMIT + 1);
        assert_eq!(
            read_csv(&dir.path().join("missing.csv")).unwrap_err(),
            "ebird_unreadable"
        );
    }
}
