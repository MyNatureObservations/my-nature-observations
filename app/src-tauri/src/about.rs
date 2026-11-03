//! Read-only access to the bundled notice document, independent of journal state.
use std::{fs::File, io::Read, path::Path};
use tauri::{Manager, path::BaseDirectory};
use tauri_plugin_opener::OpenerExt;

const NOTICES: &str = "notices/THIRD-PARTY-NOTICES.html";

fn open_notice(path: &Path, open: impl FnOnce(&Path) -> Result<(), ()>) -> Result<(), ()> {
    // Detached OS openers may not report a missing or unreadable file.
    let mut file = File::open(path).map_err(|_| ())?;
    if !file.metadata().map_err(|_| ())?.is_file() {
        return Err(());
    }
    file.read_exact(&mut [0u8; 1]).map_err(|_| ())?;
    open(path)
}

#[tauri::command]
pub(crate) fn open_third_party_notices(app: tauri::AppHandle) -> Result<(), &'static str> {
    let path = app
        .path()
        .resolve(NOTICES, BaseDirectory::Resource)
        .map_err(|_| "notices_unavailable")?;
    open_notice(&path, |path| {
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|_| ())
    })
    .map_err(|_| "notices_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn opens_readable_bundled_html_and_propagates_opener_failure() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("THIRD-PARTY-NOTICES.html");
        std::fs::write(&file, "<!doctype html><title>Synthetic notices</title>").unwrap();
        let opened = Cell::new(false);
        assert!(
            open_notice(&file, |path| {
                assert_eq!(path, file);
                opened.set(true);
                Ok(())
            })
            .is_ok()
        );
        assert!(opened.get());
        assert!(open_notice(&file, |_| Err(())).is_err());
    }

    #[test]
    fn missing_empty_and_directory_resources_never_reach_the_opener() {
        let root = tempfile::tempdir().unwrap();
        let empty = root.path().join("empty.html");
        std::fs::write(&empty, "").unwrap();
        for file in [
            root.path().join("missing.html"),
            empty,
            root.path().to_owned(),
        ] {
            assert!(open_notice(&file, |_| panic!("must not open invalid resources")).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_resource_never_reaches_the_opener() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("unreadable.html");
        std::fs::write(&file, "synthetic notices").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0)).unwrap();
        let result = open_notice(&file, |_| panic!("must not open unreadable resources"));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(result.is_err());
    }
}
