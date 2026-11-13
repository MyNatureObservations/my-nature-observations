//! Production preparation only: publish a closed v2 episode, never replace or admit.
use super::*;
use crate::backup::restore_context::activation::*;
use crate::database_state::Maintenance;
use crate::open_import::{self, ContentProof};
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

// A shared read operation may coexist with autosave. Use SQLite's own descriptor
// handling here, never an extra raw active-database descriptor/close.
pub(crate) fn journal_proof(path: &Path) -> Outcome<ContentProof> {
    no_sidecars(path)?;
    let mut db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let result = open_import::verify_database_version(
        &mut db,
        &AtomicBool::new(false),
        match version {
            0 => 1,
            1 => 2,
            2 => 3,
            3 => 4,
            4 => 5,
            5 => 6,
            6 => 7,
            7 => 8,
            8 => 9,
            9 => 10,
            _ => return Err(Error::Binding),
        },
    )
    .map_err(|_| Error::Verification);
    close(db);
    result
}
pub(crate) fn prepare_replacement(
    maintenance: &Maintenance<'_>,
    archive: &mut File,
    expected_old: &ContentProof,
    expected_new: &ContentProof,
    flag: Arc<AtomicBool>,
) -> Outcome<()> {
    maintenance
        .ensure_usable()
        .map_err(|_| Error::Coordination)?;
    if expected_old.entry_count == 0 && expected_old.version == 1 {
        return Err(Error::Binding);
    }
    cancel(&flag)?;
    let session = maintenance
        .recovery_session()
        .map_err(|_| Error::Coordination)?;
    let target = session.database_path().map_err(|_| Error::Binding)?;
    let root = target.parent().ok_or(Error::Binding)?.to_owned();
    no_legacy(&root)?;
    let mut work = Work {
        writer: None,
        incoming: None,
        protection: None,
        active: None,
        session: Some(session),
    };
    work.active = Some(ActiveDatabase::open(&target).map_err(|_| Error::Binding)?);
    if open_import::verify_database_version(
        work.active.as_mut().unwrap().connection.as_mut().unwrap(),
        &flag,
        expected_old.version,
    )
    .map_err(|_| Error::Verification)?
        != *expected_old
    {
        return Err(Error::Binding);
    }
    point("replacement-before-protection")?;
    let protection = import_protection::create_for_session(
        maintenance,
        work.session.as_ref().unwrap(),
        work.active.as_mut().unwrap(),
        flag.clone(),
    )
    .map_err(|_| Error::Verification)?;
    if !protection.is_bound_to(maintenance, work.active.as_ref().unwrap())
        || protection.content() != expected_old
    {
        return Err(Error::Binding);
    }
    if expected_old.version != expected_new.version {
        return Err(Error::Binding);
    }
    let m = ImportV2::new_version(
        maintenance.operation_id().to_string(),
        Target::New,
        if expected_old.version == 2 {
            PreviousContent(import_proof_v2(expected_old.clone())?)
        } else {
            PreviousContent::try_from(expected_old.clone())?
        },
        if expected_new.version == 2 {
            ReplacementContent(import_proof_v2(expected_new.clone())?)
        } else {
            ReplacementContent::try_from(expected_new.clone())?
        },
        protection.archive_sha256().to_owned(),
        expected_old.version,
    )?;
    if protection.path().file_name().and_then(|n| n.to_str()) != Some(m.protection_file.as_str()) {
        return Err(Error::Binding);
    }
    let protection_path = protection.path().to_owned();
    drop(protection); // Archive is persistent and independently verified; no SQL owner escapes.
    point("replacement-protected")?;
    let backups = root.join("backups");
    let staging = backups.join("import-staging");
    private::ensure_directory(&staging)?;
    point("replacement-before-build")?;
    let candidate = (if expected_new.version == 2 {
        open_import::build_current(archive, &staging, &flag)
    } else {
        open_import::build(archive, &staging, &flag)
    })
    .map_err(|_| Error::Verification)?;
    if candidate.proof() != expected_new {
        return Err(Error::Verification);
    }
    point("replacement-built")?;
    work.active
        .as_ref()
        .unwrap()
        .ensure_current()
        .map_err(|_| Error::Binding)?;
    if open_import::verify_database_version(
        work.active.as_mut().unwrap().connection.as_mut().unwrap(),
        &flag,
        expected_old.version,
    )
    .map_err(|_| Error::Verification)?
        != *expected_old
    {
        return Err(Error::Binding);
    }
    work.close_all(); // Existing checked-close policy on success, error and unwind.
    point("replacement-closed")?;
    sync_file(candidate.path(), "replacement-sync-candidate")?;
    sync_dir(
        candidate.path().parent().ok_or(Error::Binding)?,
        "replacement-sync-built",
    )?;
    let publication = private::PrivateDir::new(&staging)?;
    let recovery = publication.path().join("recovery");
    private::ensure_directory(&recovery)?;
    let candidate_path = recovery.join(&m.candidate_file);
    // The private destination is freshly owned. No active journal is involved.
    fs::rename(candidate.path(), &candidate_path)?;
    sync_dir(
        candidate.path().parent().ok_or(Error::Binding)?,
        "replacement-sync-source",
    )?;
    let bytes = serde_json::to_vec(&m).map_err(|_| Error::Binding)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    match parse(&bytes)? {
        Contract::ImportV2(encoded) if encoded == m => (),
        _ => return Err(Error::Binding),
    }
    let path = recovery.join(MARKER);
    let mut file = private::create_file(&path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    durability::sync_opened(SyncPoint::VerifiedFile, &file)?;
    sync_dir(&recovery, "replacement-sync-private")?;
    point("replacement-before-publish")?;
    work.session
        .as_ref()
        .unwrap()
        .ensure_publication_phase()
        .map_err(|_| Error::Coordination)?;
    no_legacy(&root)?;
    if crate::backup::digest_file(&protection_path).map_err(|_| Error::Verification)?
        != m.protection_sha256
    {
        return Err(Error::Binding);
    }
    if journal_proof(&candidate_path)? != *expected_new {
        return Err(Error::Verification);
    }
    cancel(&flag)?;
    // Like v1 directory publication, but no-clobber on macOS. No v2 startup or
    // database swap is called here; this process stays latched even on late errors.
    publish_directory(&recovery, &root.join("recovery"))?;
    point("replacement-after-publish")?;
    sync_dir(publication.path(), "replacement-sync-publication-source")?;
    sync_dir(&root, "replacement-sync-root")?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, &file)?;
    point("replacement-prepared")?;
    Ok(())
}
fn publish_directory(source: &Path, target: &Path) -> Outcome<()> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| Error::Binding)?;
        let target = CString::new(target.as_os_str().as_bytes()).map_err(|_| Error::Binding)?;
        // SAFETY: live NUL-terminated paths. Never overwrite an existing episode.
        if unsafe {
            libc::renameatx_np(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_EXCL,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, target);
        Err(Error::Binding)
    }
}
