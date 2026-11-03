//! App boundary for open import. Preview has no RecoverySession; activation is
//! deliberately latched until a separate startup. No SQLite candidate is retained.
use crate::{database_state::DatabaseState, open_import, restore_app::RestoreState};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub(crate) struct ImportState {
    selected: Mutex<Option<Choice>>,
}
struct Choice {
    token: String,
    original: File,
    // Anonymous private archive, not a SQLite candidate. External edits cannot
    // change the bytes that the user confirmed and activation will consume.
    archive: File,
    hash: String,
    generation: u32,
    entry_count: u64,
    existing_place_count: u64,
    imported_place_count: u64,
    existing_setting_count: u64,
    imported_setting_count: u64,
    old_content: open_import::ContentProof,
    new_content: open_import::ContentProof,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Error {
    Busy,
    InvalidPackage,
    Changed,
    Failed,
    #[cfg(not(target_os = "macos"))]
    Unsupported,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Ready,
    Recovery,
    Blocked,
    Unknown,
}
#[derive(Debug, Serialize)]
pub(crate) struct Selection {
    token: String,
    format_version: u32,
    entry_count: u64,
    existing_entry_count: u64,
    existing_place_count: u64,
    imported_place_count: u64,
    existing_setting_count: u64,
    imported_setting_count: u64,
}
#[derive(Debug, Serialize)]
pub(crate) struct ActivationResult {
    activated: bool,
    prepared: bool,
    error: Option<Error>,
    mode: Mode,
}
fn mode(state: &DatabaseState) -> Mode {
    if state.startup_recovery_pending() {
        Mode::Recovery
    } else if state.ready_database_path().is_ok() {
        Mode::Ready
    } else {
        Mode::Blocked
    }
}
fn digest(file: &mut File) -> Result<String, Error> {
    file.seek(SeekFrom::Start(0)).map_err(|_| Error::Failed)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|_| Error::Failed)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn place_count(path: &Path, private_candidate: bool) -> Result<u64, Error> {
    // The private candidate is created by this process under a TempDir. The
    // active path remains NOFOLLOW; tempdir's macOS /var alias need not be.
    let flags = if private_candidate {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW
    };
    let db = Connection::open_with_flags(path, flags).map_err(|_| Error::Failed)?;
    let count: i64 = db
        .query_row("SELECT count(*) FROM main.places", [], |r| r.get(0))
        .map_err(|_| Error::Failed)?;
    db.close().map_err(|_| Error::Failed)?;
    count.try_into().map_err(|_| Error::Failed)
}
fn setting_count(path: &Path, private_candidate: bool) -> Result<u64, Error> {
    let flags = if private_candidate {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW
    };
    let db = Connection::open_with_flags(path, flags).map_err(|_| Error::Failed)?;
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM main.weather_default_place",
            [],
            |row| row.get(0),
        )
        .map_err(|_| Error::Failed)?;
    db.close().map_err(|_| Error::Failed)?;
    count.try_into().map_err(|_| Error::Failed)
}
fn prepare(state: &DatabaseState, path: &Path) -> Result<Choice, Error> {
    let active = state.ready_database_path().map_err(|_| Error::Busy)?;
    let permit = state.operation(None).map_err(|_| Error::Busy)?;
    let generation = permit.snapshot(()).generation;
    // This is only a preview. The authoritative check remains under Maintenance.
    let old_content = crate::backup::restore_context::activation::journal_proof(&active)
        .map_err(|_| Error::Failed)?;
    let mut original = File::open(path).map_err(|_| Error::InvalidPackage)?;
    if !original
        .metadata()
        .map_err(|_| Error::InvalidPackage)?
        .is_file()
    {
        return Err(Error::InvalidPackage);
    }
    let mut archive = tempfile::tempfile().map_err(|_| Error::Failed)?;
    std::io::copy(&mut original, &mut archive).map_err(|_| Error::Failed)?;
    archive.flush().map_err(|_| Error::Failed)?;
    let hash = digest(&mut archive)?;
    if digest(&mut original)? != hash {
        return Err(Error::Changed);
    }
    let staging = tempfile::tempdir().map_err(|_| Error::Failed)?;
    let format = if open_import::is_v5(&mut archive).map_err(|_| Error::InvalidPackage)? {
        5
    } else if open_import::is_v4(&mut archive).map_err(|_| Error::InvalidPackage)? {
        4
    } else if open_import::is_v3(&mut archive).map_err(|_| Error::InvalidPackage)? {
        3
    } else {
        0
    };
    if format != 5 && !state.permits_legacy_protocol_fixture() {
        return Err(Error::InvalidPackage);
    }
    let candidate = (if format == 5 {
        open_import::build_v5(&mut archive, staging.path(), &AtomicBool::new(false))
    } else if format == 4 {
        open_import::build_v4(&mut archive, staging.path(), &AtomicBool::new(false))
    } else if format == 3 {
        open_import::build_v3(&mut archive, staging.path(), &AtomicBool::new(false))
    } else if old_content.version == 2 {
        open_import::build_current(&mut archive, staging.path(), &AtomicBool::new(false))
    } else {
        open_import::build(&mut archive, staging.path(), &AtomicBool::new(false))
    })
    .map_err(|_| Error::InvalidPackage)?;
    let entry_count = candidate.entry_count;
    let new_content = candidate.proof().clone();
    let existing_place_count = if old_content.version >= 3 {
        place_count(&active, false)?
    } else {
        0
    };
    let imported_place_count = if new_content.version >= 3 {
        place_count(candidate.path(), true)?
    } else {
        0
    };
    let existing_setting_count = if old_content.version >= 4 {
        setting_count(&active, false)?
    } else {
        0
    };
    let imported_setting_count = if new_content.version >= 4 {
        setting_count(candidate.path(), true)?
    } else {
        0
    };
    drop(candidate); // No prepared SQLite candidate survives the preview.
    if digest(&mut original)? != hash {
        return Err(Error::Changed);
    }
    Ok(Choice {
        token: uuid::Uuid::new_v4().to_string(),
        original,
        archive,
        hash,
        generation,
        entry_count,
        existing_place_count,
        imported_place_count,
        existing_setting_count,
        imported_setting_count,
        old_content,
        new_content,
    })
}
impl ImportState {
    fn clear(&self) -> Result<(), Error> {
        self.selected.lock().map_err(|_| Error::Failed)?.take();
        Ok(())
    }
    fn cancel(&self, token: &str) -> Result<(), Error> {
        let mut selected = self.selected.lock().map_err(|_| Error::Failed)?;
        // Late cancellation belongs only to its own selection. Unknown and
        // already consumed tokens are harmless, idempotent no-ops.
        if selected
            .as_ref()
            .is_some_and(|choice| choice.token == token)
        {
            selected.take();
        }
        Ok(())
    }
    fn take(&self, token: &str) -> Result<Choice, Error> {
        let mut selected = self.selected.lock().map_err(|_| Error::Failed)?;
        if !selected
            .as_ref()
            .is_some_and(|choice| choice.token == token)
        {
            return Err(Error::Changed);
        }
        selected.take().ok_or(Error::Changed)
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Success {
    Activated,
    Prepared,
}
#[cfg(target_os = "macos")]
fn activate(state: &DatabaseState, mut choice: Choice) -> Result<Success, Error> {
    let active = state.ready_database_path().map_err(|_| Error::Busy)?;
    let maintenance = state.maintenance().map_err(|_| Error::Busy)?;
    if maintenance.generation() != choice.generation {
        return Err(Error::Changed);
    }
    // Full byte comparison by SHA-256, before RecoverySession. Neither a pathname
    // nor metadata stands in for content identity. No descriptor clone/reopen.
    if digest(&mut choice.original)? != choice.hash || digest(&mut choice.archive)? != choice.hash {
        return Err(Error::Changed);
    }
    if choice.new_content.version == 5 {
        if crate::backup::restore_context::activation::journal_proof(&active)
            .map_err(|_| Error::Failed)?
            != choice.old_content
        {
            return Err(Error::Changed);
        }
        let activated = crate::backup::restore_context::activation::importing::activate_v5(
            &maintenance,
            &mut choice.archive,
            &choice.old_content,
            &choice.new_content,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| Error::Failed)?;
        drop(activated);
        return Ok(Success::Activated);
    }
    if choice.new_content.version == 4 {
        if crate::backup::restore_context::activation::journal_proof(&active)
            .map_err(|_| Error::Failed)?
            != choice.old_content
        {
            return Err(Error::Changed);
        }
        let activated = crate::backup::restore_context::activation::importing::activate_v4(
            &maintenance,
            &mut choice.archive,
            &choice.old_content,
            &choice.new_content,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| Error::Failed)?;
        drop(activated);
        return Ok(Success::Activated);
    }
    if choice.new_content.version == 3 {
        if crate::backup::restore_context::activation::journal_proof(&active)
            .map_err(|_| Error::Failed)?
            != choice.old_content
        {
            return Err(Error::Changed);
        }
        let activated = crate::backup::restore_context::activation::importing::activate_v3(
            &maintenance,
            &mut choice.archive,
            &choice.old_content,
            &choice.new_content,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| Error::Failed)?;
        drop(activated);
        return Ok(Success::Activated);
    }
    if choice.old_content.entry_count > 0 || choice.old_content.version == 2 {
        if crate::backup::restore_context::activation::journal_proof(&active)
            .map_err(|_| Error::Failed)?
            != choice.old_content
        {
            return Err(Error::Changed);
        }
        crate::backup::restore_context::activation::prepare_replacement(
            &maintenance,
            &mut choice.archive,
            &choice.old_content,
            &choice.new_content,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| Error::Failed)?;
        return Ok(Success::Prepared);
    }
    let activated = crate::backup::restore_context::activation::importing::activate(
        &maintenance,
        &mut choice.archive,
        Arc::new(AtomicBool::new(false)),
    )
    .map_err(|_| Error::Failed)?;
    drop(activated);
    Ok(Success::Activated) // V1 disk activation only. Never Ready in this process.
}
fn report(
    state: &DatabaseState,
    result: Result<Success, Error>,
    emit: impl FnOnce(),
) -> ActivationResult {
    let mode = mode(state);
    if mode != Mode::Ready {
        emit();
    }
    ActivationResult {
        activated: matches!(&result, Ok(Success::Activated)),
        prepared: matches!(&result, Ok(Success::Prepared)),
        error: result.err(),
        mode,
    }
}
#[tauri::command]
pub(crate) async fn choose_open_import(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    dialog_title: String,
) -> Result<Option<Selection>, Error> {
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<RestoreState>();
        let _busy = shared.reserve().map_err(|_| Error::Busy)?;
        let imports = app.state::<ImportState>();
        // Clearing is only for starting a new picker while holding the shared
        // reservation. No newer selection can be installed until it returns.
        imports.clear()?;
        #[cfg(target_os = "macos")]
        {
            let Some(file) = app
                .dialog()
                .file()
                .set_parent(&window)
                .set_title(&dialog_title)
                .add_filter(&dialog_title, &["mno-export"])
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            let path = file.into_path().map_err(|_| Error::InvalidPackage)?;
            let choice = prepare(&app.state::<DatabaseState>(), &path)?;
            let selection = Selection {
                token: choice.token.clone(),
                format_version: choice.new_content.version,
                entry_count: choice.entry_count,
                existing_entry_count: choice.old_content.entry_count,
                existing_place_count: choice.existing_place_count,
                imported_place_count: choice.imported_place_count,
                existing_setting_count: choice.existing_setting_count,
                imported_setting_count: choice.imported_setting_count,
            };
            *imports.selected.lock().map_err(|_| Error::Failed)? = Some(choice);
            Ok(Some(selection))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window, dialog_title);
            Err(Error::Unsupported)
        }
    })
    .await
    .map_err(|_| Error::Failed)?
}
#[tauri::command]
pub(crate) fn cancel_open_import(app: tauri::AppHandle, token: String) -> Result<(), Error> {
    let shared = app.state::<RestoreState>();
    let _busy = shared.reserve().map_err(|_| Error::Busy)?;
    app.state::<ImportState>().cancel(&token)
}
fn confirm(
    state: &DatabaseState,
    shared: &RestoreState,
    imports: &ImportState,
    token: &str,
    emit: impl FnOnce(),
) -> ActivationResult {
    // Querying mode while another activation holds Maintenance would block.
    let Ok(_busy) = shared.reserve() else {
        return ActivationResult {
            activated: false,
            prepared: false,
            error: Some(Error::Busy),
            mode: Mode::Unknown,
        };
    };
    let result = imports.take(token).and_then(|choice| {
        #[cfg(target_os = "macos")]
        {
            activate(state, choice)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = choice;
            Err(Error::Unsupported)
        }
    });
    report(state, result, emit)
}
#[tauri::command]
pub(crate) async fn activate_open_import(app: tauri::AppHandle, token: String) -> ActivationResult {
    tauri::async_runtime::spawn_blocking(move || {
        confirm(
            &app.state::<DatabaseState>(),
            &app.state::<RestoreState>(),
            &app.state::<ImportState>(),
            &token,
            || {
                let _ = app.emit("mno:database-mode-changed", ());
            },
        )
    })
    .await
    .unwrap_or(ActivationResult {
        activated: false,
        prepared: false,
        error: Some(Error::Failed),
        mode: Mode::Unknown,
    })
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
