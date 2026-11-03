//! Import-specific proof and entry; shared activation and startup own completion.
use super::*;
use crate::open_import;
pub(super) const EMPTY_RESTORE_SHA256: &str =
    "f5ad7170c0e4e8c0c7b0815b743411bee06f8344285b5cd66a057894d7a58b31";
fn content(proof: &open_import::ContentProof) -> Content {
    Content {
        version: proof.version,
        sha256: proof.sha256.clone(),
    }
}

// Format-3 import owns a complete private schema-2 candidate. The confirmed
// import command is the only caller; preview never publishes a marker or swaps.
pub(crate) fn activate_v3<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &mut File,
    expected_old: &open_import::ContentProof,
    expected_new: &open_import::ContentProof,
    flag: Arc<AtomicBool>,
) -> Outcome<DiskActivated<'m, 's>> {
    activate_version(maintenance, archive, expected_old, expected_new, flag, 3)
}
pub(crate) fn activate_v4<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &mut File,
    expected_old: &open_import::ContentProof,
    expected_new: &open_import::ContentProof,
    flag: Arc<AtomicBool>,
) -> Outcome<DiskActivated<'m, 's>> {
    activate_version(maintenance, archive, expected_old, expected_new, flag, 4)
}
pub(crate) fn activate_v5<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &mut File,
    expected_old: &open_import::ContentProof,
    expected_new: &open_import::ContentProof,
    flag: Arc<AtomicBool>,
) -> Outcome<DiskActivated<'m, 's>> {
    activate_version(maintenance, archive, expected_old, expected_new, flag, 5)
}
fn activate_version<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &mut File,
    expected_old: &open_import::ContentProof,
    expected_new: &open_import::ContentProof,
    flag: Arc<AtomicBool>,
    version: u32,
) -> Outcome<DiskActivated<'m, 's>> {
    maintenance
        .ensure_usable()
        .map_err(|_| Error::Coordination)?;
    if expected_new.version != version
        || !(if version >= 4 {
            expected_old.version == version
        } else {
            matches!(expected_old.version, 2 | 3)
        })
    {
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
    let old_schema: u32 = work
        .active
        .as_ref()
        .unwrap()
        .connection()
        .pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(if version >= 4 {
        (old_schema, expected_old.version) == (version - 1, version)
    } else {
        matches!((old_schema, expected_old.version), (1, 2) | (2, 3))
    }) {
        return Err(Error::Binding);
    }
    let actual_old = open_import::verify_database_version(
        work.active.as_mut().unwrap().connection.as_mut().unwrap(),
        &flag,
        expected_old.version,
    )
    .map_err(|_| Error::Verification)?;
    if actual_old != *expected_old {
        return Err(Error::Binding);
    }
    let protection =
        pre_restore::create_bound(maintenance, work.active.as_ref().unwrap(), flag.clone())
            .map_err(Error::Protection)?;
    let protected = if old_schema == 4 {
        preserved_fingerprint_v5(protection.candidate(), &flag)?
    } else if old_schema == 3 {
        preserved_fingerprint_v4(protection.candidate(), &flag)?
    } else if old_schema == 2 {
        preserved_fingerprint_v3(protection.candidate(), &flag)?
    } else {
        preserved_fingerprint(protection.candidate(), &flag)?
    };
    if protected.sha256 != expected_old.sha256 {
        return Err(Error::Binding);
    }
    let protection_file = protection
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error::Binding)?
        .to_owned();
    let protection_sha256 =
        crate::backup::digest_file(protection.path()).map_err(|_| Error::Verification)?;
    let (source, _record) = protection.into_parts();
    work.protection = Some(source);
    point("import-v3-protected")?;
    let staging = root.join("backups/import-staging");
    private::ensure_directory(&staging)?;
    let candidate = (if version == 5 {
        open_import::build_v5(archive, &staging, &flag)
    } else if version == 4 {
        open_import::build_v4(archive, &staging, &flag)
    } else {
        open_import::build_v3(archive, &staging, &flag)
    })
    .map_err(|_| Error::Verification)?;
    if candidate.proof() != expected_new {
        return Err(Error::Verification);
    }
    point("import-v3-built")?;
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
    work.close_all();
    sync_file(candidate.path(), "import-v3-sync-candidate")?;
    sync_dir(
        candidate.path().parent().ok_or(Error::Binding)?,
        "import-v3-sync-built",
    )?;
    let marker = Marker {
        format: IMPORT_FORMAT.into(),
        version: version + 1,
        operation_id: maintenance.operation_id().to_string(),
        schema_version: version - 1,
        old_schema_version: Some(old_schema),
        backup_format_version: None,
        export_format_version: Some(version),
        old_content: Content {
            version: expected_old.version,
            sha256: expected_old.sha256.clone(),
        },
        new_content: Content {
            version,
            sha256: expected_new.sha256.clone(),
        },
        protection_file,
        protection_sha256,
        candidate_file: format!("next-{}.sqlite", maintenance.operation_id()),
    };
    let publication = tempfile::Builder::new()
        .prefix(".import-v3-publication-")
        .tempdir_in(&staging)?;
    publish(publication.path(), &marker)?;
    let private_recovery = publication.path().join("recovery");
    fs::rename(
        candidate.path(),
        private_recovery.join(&marker.candidate_file),
    )?;
    sync_dir(&private_recovery, "import-v3-sync-publication")?;
    point("import-v3-before-marker-publish")?;
    cancel(&flag)?;
    fs::rename(&private_recovery, root.join("recovery"))?;
    point("import-v3-after-marker-publish")?;
    sync_dir(publication.path(), "import-v3-sync-publication-source")?;
    sync_dir(&root, "import-v3-sync-root")?;
    sync_file(&root.join("recovery").join(MARKER), "import-v3-sync-marker")?;
    point("import-v3-marker-published")?;
    let staged = root.join("recovery").join(&marker.candidate_file);
    replace_closed(&root, &target, &staged, &marker, &flag)?;
    Ok(DiskActivated {
        marker,
        _session: work.session.take().unwrap(),
    })
}
pub(super) fn verify_file(path: &Path) -> Outcome<Content> {
    no_sidecars(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::Binding);
    }
    let mut header = [0; 20];
    File::open(path)?.read_exact(&mut header)?;
    if &header[..16] != b"SQLite format 3\0" || header[18..20] != [1, 1] {
        return Err(Error::Verification);
    }
    let mut db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let result = open_import::verify_database(&mut db, &AtomicBool::new(false))
        .map_err(|_| Error::Verification);
    close(db);
    result.map(|p| content(&p))
}
// Borrow the already-owned export descriptor; no acquisition or UI is added.
// The RecoverySession latches this process before work; only a new verified
// startup can release it, including ordinary errors after the latch is set.
pub(crate) fn activate<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &mut File,
    flag: Arc<AtomicBool>,
) -> Outcome<DiskActivated<'m, 's>> {
    maintenance
        .ensure_usable()
        .map_err(|_| Error::Coordination)?;
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
    // Includes every stored row, even if the UI hides it. Maintenance has
    // drained all normal operations and prevents new ones throughout.
    let occupied: bool = work.active.as_ref().unwrap().connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM main.journal_entries)",
        [],
        |r| r.get(0),
    )?;
    if occupied {
        return Err(Error::Verification);
    }
    let backups = root.join("backups");
    private::ensure_directory(&backups)?;
    let staging = backups.join("import-staging");
    private::ensure_directory(&staging)?;
    point("import-before-build")?;
    let candidate =
        open_import::build(archive, &staging, &flag).map_err(|_| Error::Verification)?;
    point("import-built")?;
    let expected = content(candidate.proof());
    let protection =
        pre_restore::create_bound(maintenance, work.active.as_ref().unwrap(), flag.clone())
            .map_err(Error::Protection)?;
    let old_content = fingerprint(protection.candidate(), flag.clone())?;
    if old_content.sha256 != EMPTY_RESTORE_SHA256 {
        return Err(Error::Verification);
    }
    let protection_file = protection
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(Error::Binding)?
        .to_owned();
    let protection_sha256 =
        crate::backup::digest_file(protection.path()).map_err(|_| Error::Verification)?;
    let (source, _record) = protection.into_parts();
    work.protection = Some(source);
    point("protection-durable")?;
    work.active
        .as_ref()
        .unwrap()
        .ensure_current()
        .map_err(|_| Error::Binding)?;
    work.close_all();
    point("all-closed")?;
    if verify_file(candidate.path())? != expected {
        return Err(Error::Verification);
    }
    sync_file(candidate.path(), "import-sync-built-file")?;
    sync_dir(
        candidate.path().parent().ok_or(Error::Binding)?,
        "import-sync-built-directory",
    )?;
    let marker = Marker {
        old_schema_version: None,
        backup_format_version: None,
        export_format_version: None,
        format: IMPORT_FORMAT.into(),
        version: 1,
        operation_id: maintenance.operation_id().to_string(),
        schema_version: 0,
        old_content,
        new_content: expected,
        protection_file,
        protection_sha256,
        candidate_file: format!("next-{}.sqlite", maintenance.operation_id()),
    };
    // Do not expose an empty/partial recovery namespace at the active root.
    // Reuse the existing durable publisher privately, then publish the complete
    // directory (marker + verified candidate) in one same-volume rename.
    let publication = tempfile::Builder::new()
        .prefix(".import-publication-")
        .tempdir_in(&staging)?;
    publish(publication.path(), &marker)?;
    cancel(&flag)?;
    let private_recovery = publication.path().join("recovery");
    fs::rename(
        candidate.path(),
        private_recovery.join(&marker.candidate_file),
    )?;
    point("import-staged")?;
    sync_dir(
        candidate.path().parent().ok_or(Error::Binding)?,
        "import-sync-staging-source",
    )?;
    sync_dir(&private_recovery, "import-sync-publication")?;
    point("import-before-marker-publish")?;
    cancel(&flag)?;
    fs::rename(&private_recovery, root.join("recovery"))?;
    point("import-after-marker-rename")?;
    sync_dir(publication.path(), "import-sync-publication-source")?;
    sync_dir(&root, "import-sync-marker-root")?;
    sync_file(
        &root.join("recovery").join(MARKER),
        "import-sync-marker-final",
    )?;
    point("import-marker-published")?;
    let staged = root.join("recovery").join(&marker.candidate_file);
    replace_closed(&root, &target, &staged, &marker, &flag)?;
    Ok(DiskActivated {
        marker,
        _session: work.session.take().unwrap(),
    })
}
#[cfg(test)]
mod tests;
