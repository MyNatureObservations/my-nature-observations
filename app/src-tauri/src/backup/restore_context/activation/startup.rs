//! Startup-only staged activation completion. macOS only.
//! No RESET, in-place repair or P/Q cleanup.
use super::*;
use crate::{
    database_state::{DatabaseState, recovery_session::DetachedSession},
    startup::established,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    FinishNew,
    KeepPrevious,
    Continue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    NewInstalled,
    BeforeReplacement,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    Import,
    Restore,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ReplacementView {
    pub target: String,
    pub existing_entry_count: u64,
    pub imported_entry_count: u64,
    // Informational preview binding, never authority without fresh verification.
    pub previous_confirmation: Option<String>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct View {
    pub operation: Operation,
    pub phase: Phase,
    pub actions: Vec<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<ReplacementView>,
}
fn plain_file(path: &Path) -> Outcome<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err(Error::Binding);
    }
    Ok(())
}
fn plain_directory(path: &Path) -> Outcome<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(Error::Binding);
    }
    Ok(())
}
fn namespace(root: &Path, marker: &Marker) -> Outcome<()> {
    if present(&root.join("pending-restore.json"))?
        || present(&root.join(".mno-restore-completion-v1.json"))?
        || crate::startup::artifacts::unexpected_root(root, None)?
    {
        return Err(Error::Binding);
    }
    let directory = root.join("recovery");
    plain_directory(&directory)?;
    for item in fs::read_dir(directory)? {
        let item = item?;
        let name = item.file_name();
        if name != MARKER
            && name != marker.candidate_file.as_str()
            && name != format!("{}-journal", marker.candidate_file).as_str()
        {
            return Err(Error::Binding);
        }
        plain_file(&item.path())?;
    }
    Ok(())
}
fn protection(root: &Path, marker: &Marker) -> Outcome<()> {
    let path = root
        .join("backups/pre-restore")
        .join(&marker.protection_file);
    plain_file(&path)?;
    if crate::backup::digest_file(&path).map_err(|_| Error::Verification)?
        != marker.protection_sha256
    {
        return Err(Error::Verification);
    }
    let candidate =
        super::super::super::restore::prepare_candidate(&path, Arc::new(AtomicBool::new(false)))
            .map_err(Error::Candidate)?;
    let actual = if marker.old_content.version == 6 {
        preserved_fingerprint_v6(&candidate, &AtomicBool::new(false))
    } else if marker.old_content.version == 5 {
        preserved_fingerprint_v5(&candidate, &AtomicBool::new(false))
    } else if marker.old_content.version == 4 {
        preserved_fingerprint_v4(&candidate, &AtomicBool::new(false))
    } else if marker.old_content.version == 3 {
        preserved_fingerprint_v3(&candidate, &AtomicBool::new(false))
    } else if marker.old_content.version == 2 {
        preserved_fingerprint(&candidate, &AtomicBool::new(false))
    } else {
        fingerprint(&candidate, Arc::new(AtomicBool::new(false)))
    };
    match candidate.try_close() {
        Ok(files) => drop(files),
        Err(_original) => std::process::abort(),
    }
    if actual? != marker.old_content {
        return Err(Error::Verification);
    }
    Ok(())
}
fn check_installation(root: &Path, durable: bool) -> Outcome<()> {
    let directory = File::open(root)?;
    let marker = established::RetainedMarker::open_at(&directory).map_err(|_| Error::Binding)?;
    marker.check(&directory).map_err(|_| Error::Binding)?;
    if durable {
        durability::sync_opened(SyncPoint::FinalFileBarrier, marker.retained_file())?;
        durability::sync_opened(SyncPoint::Directory, &directory)?;
    }
    Ok(())
}
fn load(session: &RecoverySession<'_, '_>) -> Outcome<(PathBuf, Marker)> {
    session
        .startup_generation()
        .map_err(|_| Error::Coordination)?;
    session.ensure_current().map_err(|_| Error::Coordination)?;
    let path = session.database_path().map_err(|_| Error::Coordination)?;
    let root = path.parent().ok_or(Error::Binding)?.to_owned();
    // An established installation is required; never publish/repair its marker.
    check_installation(&root, false)?;
    let marker = read_marker(&root)?;
    namespace(&root, &marker)?;
    check_record(&root, &marker)?;
    protection(&root, &marker)?;
    Ok((root, marker))
}
fn assess(root: &Path, marker: &Marker) -> View {
    // Display context only; admission and available actions are unchanged.
    let operation = if marker.is_import() {
        Operation::Import
    } else {
        Operation::Restore
    };
    let active_path = root.join("mno.sqlite");
    let new_matches = verify_new_file(&active_path, marker).is_ok_and(|v| v == marker.new_content);
    if new_matches {
        return View {
            operation,
            replacement: None,
            phase: Phase::NewInstalled,
            actions: vec![Action::FinishNew],
        };
    }
    let previous_matches = if marker.is_import() || matches!(marker.version, 3 | 4 | 5 | 6) {
        verify_file_version(&active_path, marker.old_content.version)
            .is_ok_and(|v| v == marker.old_content)
    } else {
        verify_new_file(&active_path, marker).is_ok_and(|v| v == marker.old_content)
    };
    if previous_matches {
        let mut actions = vec![Action::KeepPrevious];
        if verify_new_file(&root.join("recovery").join(&marker.candidate_file), marker)
            .is_ok_and(|v| v == marker.new_content)
        {
            actions.push(Action::Continue);
        }
        return View {
            operation,
            replacement: None,
            phase: Phase::BeforeReplacement,
            actions,
        };
    }
    View {
        operation,
        replacement: None,
        phase: Phase::Unavailable,
        actions: vec![],
    }
}
// Strict version dispatch under the same exclusive startup owner.
fn v2(session: &RecoverySession<'_, '_>) -> Outcome<bool> {
    session
        .startup_generation()
        .map_err(|_| Error::Coordination)?;
    let path = session.database_path().map_err(|_| Error::Coordination)?;
    let root = path.parent().ok_or(Error::Binding)?;
    check_installation(root, false)?;
    plain_directory(&root.join("recovery"))?;
    let path = root.join("recovery").join(MARKER);
    plain_file(&path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    #[derive(Deserialize)]
    struct Header {
        format: String,
        version: u32,
    }
    if serde_json::from_slice::<Header>(&bytes).is_ok_and(|h| {
        (h.format == "mno-database-activation" && matches!(h.version, 3 | 4 | 5 | 6))
            || (h.format == IMPORT_FORMAT && h.version == 4)
            || (h.format == IMPORT_FORMAT && matches!(h.version, 5 | 6 | 7 | 8))
    }) {
        return Ok(false);
    }
    super::marker_contract::is_v2(&bytes)
}
pub(crate) fn status(state: &DatabaseState) -> Outcome<View> {
    let session = state.recovery_session().map_err(|_| Error::Coordination)?;
    if v2(&session)? {
        let mut view = super::marker_contract::recovery_view(&session)?;
        if !state.permits_legacy_protocol_fixture() {
            view.actions.clear();
        }
        return Ok(view);
    }
    let (root, marker) = load(&session)?;
    let mut view = assess(&root, &marker);
    if !state.permits_legacy_protocol_fixture() {
        view.actions.retain(|action| match action {
            Action::KeepPrevious => matches!(marker.old_content.version, 5 | 6),
            Action::FinishNew | Action::Continue => matches!(marker.new_content.version, 5 | 6),
        });
    }
    Ok(view)
}
// Only complete() can construct this one-use startup owner; no Clone/Serde.
pub(crate) struct VerifiedStartupActivation<'s> {
    session: DetachedSession<'s>,
}
impl VerifiedStartupActivation<'_> {
    pub(crate) fn matches(&self, state: &DatabaseState) -> bool {
        self.session.matches_startup_target(state)
    }
    pub(crate) fn check(&self, state: &DatabaseState, generation: u32) -> Outcome<()> {
        self.session
            .check_startup(state, generation)
            .map_err(|_| Error::Coordination)?;
        // The disk gate was retired only AFTER full content verification and
        // barriers, with no live SQLite connections. No late SQL acquisition or
        // content validation may fail after that gate has disappeared. The
        // original DirectoryLease/session excludes all coordinated writers.
        point("startup-before-admit")?;
        Ok(())
    }
}
fn complete<'s>(
    session: RecoverySession<'_, 's>,
    action: Action,
) -> Outcome<VerifiedStartupActivation<'s>> {
    let (root, marker) = load(&session)?;
    let view = assess(&root, &marker);
    if !view.actions.contains(&action) {
        return Err(Error::Verification);
    }
    point("startup-verified")?;
    let expected = match action {
        Action::KeepPrevious => Content {
            version: marker.old_content.version,
            sha256: marker.old_content.sha256.clone(),
        },
        _ => Content {
            version: marker.new_content.version,
            sha256: marker.new_content.sha256.clone(),
        },
    };
    let target = root.join("mno.sqlite");
    if action == Action::Continue {
        let candidate = root.join("recovery").join(&marker.candidate_file);
        sync_file(&candidate, "startup-sync-candidate")?;
        namespace(&root, &marker)?;
        no_sidecars(&target)?;
        no_sidecars(&candidate)?;
        point("startup-before-replace")?;
        fs::rename(candidate, &target)?;
        point("startup-after-replace")?;
        sync_dir(&root.join("recovery"), "startup-sync-replace-source")?;
        sync_dir(&root, "startup-sync-replace-target")?;
    }
    if (if action == Action::KeepPrevious {
        verify_file_version(&target, marker.old_content.version)?
    } else {
        verify_new_file(&target, &marker)?
    }) != expected
    {
        return Err(Error::Verification);
    }
    sync_file(&target, "startup-sync-active")?;
    sync_dir(&root, "startup-sync-root")?;
    check_installation(&root, true)?;
    session.ensure_current().map_err(|_| Error::Coordination)?;
    namespace(&root, &marker)?;
    check_record(&root, &marker)?;
    if (if action == Action::KeepPrevious {
        verify_file_version(&target, marker.old_content.version)?
    } else {
        verify_new_file(&target, &marker)?
    }) != expected
    {
        return Err(Error::Verification);
    }
    point("startup-final-verified")?;
    // Preserve all staging files/rollback journals unopened. Nothing is erased
    // to bypass a SQLite gate. The archive lives outside the active namespace.
    let archives = root.join("backups/activations");
    match fs::create_dir(&archives) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    plain_directory(&archives)?;
    sync_dir(&root.join("backups"), "startup-sync-archive-parent")?;
    let audit = archives.join(&marker.operation_id);
    if present(&audit)? {
        return Err(Error::Binding);
    }
    point("startup-before-retire")?;
    fs::rename(root.join("recovery"), &audit)?;
    point("startup-after-retire")?;
    sync_dir(&archives, "startup-sync-archive")?;
    sync_dir(&root, "startup-sync-retired-root")?;
    sync_file(&target, "startup-final-barrier")?;
    let binding = session.detach_binding().map_err(|_| Error::Coordination)?;
    Ok(VerifiedStartupActivation {
        session: session.into_detached_session(binding),
    })
}
pub(crate) fn run(state: &DatabaseState, action: Action) -> Outcome<()> {
    run_confirmed(state, action, None)
}
pub(crate) fn run_confirmed(
    state: &DatabaseState,
    action: Action,
    previous_confirmation: Option<&str>,
) -> Outcome<()> {
    let mut session = state.recovery_session().map_err(|_| Error::Coordination)?;
    let legacy = state.permits_legacy_protocol_fixture();
    let old_protocol = v2(&session)?;
    if old_protocol && !legacy {
        // Pre-v0.1 activation protocols cannot establish the current schema-3
        // Ready contract. Leave their marker and all data untouched.
        return Err(Error::Binding);
    }
    let proof = if old_protocol {
        let view = super::marker_contract::recovery_view(&session)?;
        if !view.actions.contains(&action) {
            return Err(Error::Verification);
        }
        if action == Action::KeepPrevious {
            super::marker_contract::select_previous(&mut session, previous_confirmation)?;
        }
        let session = super::marker_contract::complete_v2(session)?;
        let binding = session.detach_binding().map_err(|_| Error::Coordination)?;
        VerifiedStartupActivation {
            session: session.into_detached_session(binding),
        }
    } else {
        let root = session.database_path().map_err(|_| Error::Coordination)?;
        let root = root.parent().ok_or(Error::Binding)?;
        let marker = read_marker(root)?;
        let selected_version = if action == Action::KeepPrevious {
            marker.old_content.version
        } else {
            marker.new_content.version
        };
        if !matches!(selected_version, 5 | 6) && !legacy {
            // Validate the Ready contract before complete() can retire Recovery.
            return Err(Error::Binding);
        }
        complete(session, action)?
    };
    state
        .admit_activation_verified(proof)
        .map_err(|_| Error::Verification)
}
#[cfg(test)]
mod tests;

// Existing interruption fixtures exercise the same production dispatch and
// admission, rather than a separate test executor.
#[cfg(test)]
pub(crate) fn run_v2_for_test(state: &DatabaseState) -> Outcome<()> {
    let view = status(state)?;
    let action = view
        .actions
        .into_iter()
        .find(|a| *a != Action::KeepPrevious)
        .ok_or(Error::Verification)?;
    run(state, action)
}
pub(super) fn check_v2_installation(root: &Path, durable: bool) -> Outcome<()> {
    check_installation(root, durable)
}
