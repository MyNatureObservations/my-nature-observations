//! Observations of v2; the separate transition path revalidates them.
//! The retained startup owner excludes coordinated MNO writers. These results
//! are NOT capabilities: any future completion/swap must revalidate and apply
//! its own ownership, close, durability and startup-admission requirements.
use super::{Contract, ImportV2, Target, parse};
use crate::{
    backup::{
        self,
        restore_context::activation::{self, import_protection},
    },
    database_state::recovery_session::RecoverySession,
    open_import::{self, ContentProof},
    startup::established,
};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Blocked {
    Ownership,
    Installation,
    Marker,
    InconsistentProof,
    Legacy,
    Namespace,
    Sidecar,
    Changed,
    Cancelled,
    Io,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Active {
    Missing,
    Invalid,
    Other,
    Old,
    New,
    Both, // Content cannot establish whether a swap happened.
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Missing,
    Invalid,
    WrongProof,
    Verified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protection {
    Missing,
    WrongArchiveHash,
    InvalidOrWrongContent,
    Verified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextCheck {
    Blocked,
    Completion,     // Selected ACTIVE content verified, not startup admission.
    NewReplacement, // Requires a new set of checks; does not authorize swap.
    OldReplacement,
    BuildPrevious, // Requires durable target=old; protection alone cannot swap.
}
#[derive(Debug, PartialEq, Eq)]
struct Assessment {
    target: Target,
    active: Active,
    new_candidate: Candidate,
    previous_candidate: Candidate,
    protection: Protection,
    identical_content: bool,
    previous_available: bool, // Data available, never an action authorization.
    next_check: NextCheck,
}
type Result<T> = std::result::Result<T, Blocked>;
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(Blocked::Cancelled)
    } else {
        Ok(())
    }
}
fn root(session: &RecoverySession<'_, '_>) -> Result<PathBuf> {
    // Running/Maintenance owners are rejected before any generation-lock wait.
    session
        .startup_generation()
        .map_err(|_| Blocked::Ownership)?;
    session.ensure_current().map_err(|_| Blocked::Ownership)?;
    let path = session.database_path().map_err(|_| Blocked::Ownership)?;
    let root = path.parent().ok_or(Blocked::Ownership)?.to_owned();
    let directory = File::open(&root).map_err(|_| Blocked::Installation)?;
    let installation =
        established::RetainedMarker::open_at(&directory).map_err(|_| Blocked::Installation)?;
    installation
        .check(&directory)
        .map_err(|_| Blocked::Installation)?;
    Ok(root)
}
fn directory(path: &Path, optional: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(()),
        _ => Err(Blocked::Namespace),
    }
}
// All checks precede SQLite. Links, directories and special files are not data
// candidates, even if following them could lead to an otherwise valid journal.
fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Blocked::Io),
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() {
                return Err(Blocked::Namespace);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if m.nlink() != 1 {
                    return Err(Blocked::Namespace);
                }
            }
            Ok(true)
        }
    }
}
fn marker(root: &Path) -> Result<ImportV2> {
    let path = root.join("recovery").join(activation::MARKER);
    if !regular(&path)? {
        return Err(Blocked::Marker);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| Blocked::Io)?
        .take(activation::LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Blocked::Io)?;
    match parse(&bytes).map_err(|_| Blocked::Marker)? {
        Contract::ImportV2(marker) => Ok(marker),
        _ => Err(Blocked::Marker), // v1 has its original separate production path.
    }
}
fn paths(root: &Path, m: &ImportV2) -> [PathBuf; 3] {
    [
        root.join("mno.sqlite"),
        root.join("recovery").join(&m.candidate_file),
        root.join("recovery").join(&m.previous_candidate_file),
    ]
}
fn environment(root: &Path, m: &ImportV2) -> Result<()> {
    for path in [
        root.join("pending-restore.json"),
        root.join(".mno-restore-completion-v1.json"),
        root.join("recovery/pending-restore.json"),
    ] {
        if activation::present(&path).map_err(|_| Blocked::Io)? {
            return Err(Blocked::Legacy);
        }
        // The root/recovery scans below also reject unexpected P/Q aliases.
    }
    if crate::startup::artifacts::unexpected_root(root, None).map_err(|_| Blocked::Io)? {
        return Err(Blocked::Namespace);
    }
    for path in paths(root, m) {
        activation::no_sidecars(&path).map_err(|_| Blocked::Sidecar)?;
        regular(&path)?;
    }
    directory(&root.join("backups"), true)?;
    directory(&root.join("backups/pre-restore"), true)?;
    regular(&root.join("backups/pre-restore").join(&m.protection_file))?;
    for entry in fs::read_dir(root.join("recovery")).map_err(|_| Blocked::Io)? {
        let entry = entry.map_err(|_| Blocked::Io)?;
        let name = entry.file_name();
        if name != activation::MARKER
            && name != m.candidate_file.as_str()
            && name != m.previous_candidate_file.as_str()
        {
            return Err(Blocked::Namespace);
        }
        regular(&entry.path())?;
    }
    Ok(())
}
fn proof_version(hash: &str, count: u64, version: u32) -> ContentProof {
    ContentProof {
        version,
        sha256: hash.to_owned(),
        entry_count: count,
    }
}
fn proof(hash: &str, count: u64) -> ContentProof {
    ContentProof {
        version: 1,
        sha256: hash.to_owned(),
        entry_count: count,
    }
}
fn inspect_version(
    path: &Path,
    flag: Arc<AtomicBool>,
    version: u32,
) -> Result<Option<std::result::Result<ContentProof, ()>>> {
    cancelled(&flag)?;
    if !regular(path)? {
        return Ok(None);
    }
    activation::no_sidecars(path).map_err(|_| Blocked::Sidecar)?;
    let mut header = [0; 20];
    if File::open(path)
        .map_err(|_| Blocked::Io)?
        .read_exact(&mut header)
        .is_err()
        || &header[..16] != b"SQLite format 3\0"
        || header[18..20] != [1, 1]
    {
        return Ok(Some(Err(())));
    }
    let mut db = match import_protection::open_readonly(path) {
        Ok(db) => db,
        Err(_) => return Ok(Some(Err(()))),
    };
    let result = (|| {
        backup::restore::harden(&db, flag.clone()).map_err(|_| ())?;
        db.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            crate::open_export::ROW_LIMIT as i32,
        )
        .map_err(|_| ())?;
        open_import::verify_database_version(&mut db, &flag, version).map_err(|_| ())
    })();
    activation::close(db); // Existing checked-close policy; never return a live owner.
    cancelled(&flag)?;
    Ok(Some(result))
}
fn candidate(
    value: Option<std::result::Result<ContentProof, ()>>,
    expected: &ContentProof,
) -> Candidate {
    match value {
        None => Candidate::Missing,
        Some(Err(())) => Candidate::Invalid,
        Some(Ok(actual)) if &actual == expected => Candidate::Verified,
        Some(Ok(_)) => Candidate::WrongProof,
    }
}
fn decide(
    target: Target,
    active: Active,
    new: Candidate,
    previous: Candidate,
    protection: Protection,
) -> NextCheck {
    if protection != Protection::Verified
        || matches!(new, Candidate::Invalid | Candidate::WrongProof)
        || matches!(previous, Candidate::Invalid | Candidate::WrongProof)
    {
        return NextCheck::Blocked;
    }
    match (target, active) {
        (Target::New, Active::New | Active::Both) | (Target::Old, Active::Old | Active::Both) => {
            NextCheck::Completion
        }
        (Target::New, Active::Old) if new == Candidate::Verified => NextCheck::NewReplacement,
        (Target::Old, Active::New | Active::Missing | Active::Invalid) => {
            if previous == Candidate::Verified {
                NextCheck::OldReplacement
            } else {
                NextCheck::BuildPrevious
            }
        }
        _ => NextCheck::Blocked,
    }
}
fn classify(session: &RecoverySession<'_, '_>, flag: Arc<AtomicBool>) -> Result<Assessment> {
    cancelled(&flag)?;
    let root = root(session)?;
    directory(&root.join("recovery"), false)?;
    let m = marker(&root)?;
    if m.old_content.0.sha256 == m.new_content.0.sha256
        && m.old_content.0.entry_count != m.new_content.0.entry_count
    {
        return Err(Blocked::InconsistentProof);
    }
    environment(&root, &m)?;
    let old = proof_version(
        &m.old_content.0.sha256,
        m.old_content.0.entry_count,
        m.content_contract.version,
    );
    let new = proof_version(
        &m.new_content.0.sha256,
        m.new_content.0.entry_count,
        m.content_contract.version,
    );
    let archive = root.join("backups/pre-restore").join(&m.protection_file);
    let protection = if !regular(&archive)? {
        Protection::Missing
    } else if backup::digest_file(&archive).map_err(|_| Blocked::Io)? != m.protection_sha256 {
        Protection::WrongArchiveHash
    } else {
        let staging = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| Blocked::Io)?;
        if import_protection::verify_bound_archive(
            &archive,
            &staging,
            &old,
            &m.protection_sha256,
            flag.clone(),
        )
        .is_ok()
        {
            Protection::Verified
        } else {
            Protection::InvalidOrWrongContent
        }
    };
    let [active_path, new_path, previous_path] = paths(&root, &m);
    let active = match inspect_version(&active_path, flag.clone(), m.content_contract.version)? {
        None => Active::Missing,
        Some(Err(())) => Active::Invalid,
        Some(Ok(actual)) => match (actual == old, actual == new) {
            (true, true) => Active::Both,
            (true, false) => Active::Old,
            (false, true) => Active::New,
            _ => Active::Other,
        },
    };
    let new_candidate = candidate(
        inspect_version(&new_path, flag.clone(), m.content_contract.version)?,
        &new,
    );
    let previous_candidate = candidate(
        inspect_version(&previous_path, flag.clone(), m.content_contract.version)?,
        &old,
    );
    // Recheck the retained owner and immutable selected target before returning
    // observations. No observation can serve as a future durable transition proof.
    session.ensure_current().map_err(|_| Blocked::Ownership)?;
    environment(&root, &m)?;
    if marker(&root)? != m {
        return Err(Blocked::Changed);
    }
    cancelled(&flag)?;
    Ok(Assessment {
        target: m.target,
        active,
        new_candidate,
        previous_candidate,
        protection,
        identical_content: old == new,
        previous_available: protection == Protection::Verified,
        next_check: decide(
            m.target,
            active,
            new_candidate,
            previous_candidate,
            protection,
        ),
    })
}
#[cfg(test)]
mod tests;

// Decision validation only; no persistence or transition authority.
mod decision;

// Candidate construction; no active replacement or admission.
mod previous_candidate;

pub(super) mod transition;

// These are observations only. The transition and publisher revalidate all files.
pub(in crate::backup::restore_context::activation) fn recovery_view(
    session: &RecoverySession<'_, '_>,
) -> activation::Outcome<activation::startup::View> {
    use activation::startup::{Action, Operation, Phase, ReplacementView, View};
    let a = classify(session, Arc::new(AtomicBool::new(false)))
        .map_err(|_| activation::Error::Verification)?;
    if a.protection != Protection::Verified {
        return Err(activation::Error::Verification);
    }
    let root = root(session).map_err(|_| activation::Error::Binding)?;
    let m = marker(&root).map_err(|_| activation::Error::Binding)?;
    let can_complete = transition::available(session).is_ok();
    let mut actions = vec![];
    if can_complete {
        actions.push(
            if a.target == Target::New && matches!(a.active, Active::New | Active::Both) {
                Action::FinishNew
            } else {
                Action::Continue
            },
        );
    }
    let previous_confirmation = decision::confirmation_key(session);
    if previous_confirmation.is_some() {
        actions.push(Action::KeepPrevious);
    }
    Ok(View {
        operation: Operation::Import,
        phase: if !can_complete {
            Phase::Unavailable
        } else if a.target == Target::New && matches!(a.active, Active::New | Active::Both) {
            Phase::NewInstalled
        } else {
            Phase::BeforeReplacement
        },
        actions,
        replacement: Some(ReplacementView {
            target: if m.target == Target::New {
                "new"
            } else {
                "old"
            }
            .into(),
            existing_entry_count: m.old_content.0.entry_count,
            imported_entry_count: m.new_content.0.entry_count,
            previous_confirmation,
        }),
    })
}
pub(in crate::backup::restore_context::activation) fn select_previous(
    session: &mut RecoverySession<'_, '_>,
    confirmation: Option<&str>,
) -> activation::Outcome<()> {
    decision::publish_confirmed(session, confirmation)
}

#[cfg(test)]
mod production_tests;

#[cfg(test)]
fn inspect(
    path: &Path,
    flag: Arc<AtomicBool>,
) -> Result<Option<std::result::Result<ContentProof, ()>>> {
    inspect_version(path, flag, 1)
}
