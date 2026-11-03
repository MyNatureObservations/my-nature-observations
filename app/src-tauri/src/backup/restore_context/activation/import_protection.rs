//! Lossless protection for import v2 preparation. No marker or activation.
//! Evidence here is NOT PreparedRestore,
//! CompletionProof, or any capability accepted by startup or the restore chain.
use super::*;
use crate::{
    backup::{self, BackupManifest, BackupResult, restore::private},
    open_import::{self, ContentProof},
};
use std::{cell::RefCell, marker::PhantomData, time::Duration};

// Observe the actual native open path used by both archive and classifier
// verification. Production uses the same native open without recording.
#[cfg(test)]
thread_local! {
    static SQLITE_OPEN_ATTEMPTS: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}
pub(super) fn open_readonly(path: &Path) -> rusqlite::Result<Connection> {
    #[cfg(test)]
    SQLITE_OPEN_ATTEMPTS.with(|slot| {
        if let Some(attempts) = slot.borrow_mut().as_mut() {
            attempts.push(path.to_owned());
        }
    });
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
}
#[cfg(test)]
pub(super) fn observe_sqlite_opens<T>(run: impl FnOnce() -> T) -> (T, Vec<PathBuf>) {
    struct Restore(Option<Vec<PathBuf>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SQLITE_OPEN_ATTEMPTS.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let restore = Restore(SQLITE_OPEN_ATTEMPTS.with(|slot| slot.replace(Some(Vec::new()))));
    let result = run();
    let attempts = SQLITE_OPEN_ATTEMPTS.with(|slot| slot.borrow_mut().take().unwrap());
    drop(restore);
    (result, attempts)
}

struct ArchiveEvidence {
    manifest: BackupManifest,
    content: ContentProof,
    archive_sha256: String,
}
pub(super) struct Protection<'m, 's> {
    path: PathBuf,
    evidence: ArchiveEvidence,
    operation_id: uuid::Uuid,
    generation: u32,
    source_identity: private::FileIdentity,
    _maintenance: PhantomData<&'m Maintenance<'s>>,
}

// Own the private files until SQLite is positively closed, including unwind.
// A close failure is an error, never verification; Drop retains the whole owner
// if a retry cannot close it. No active database descriptors are opened here.
struct ExtractedDatabase {
    connection: Option<Connection>,
    directory: Option<private::PrivateDir>,
}
impl ExtractedDatabase {
    fn close(&mut self) -> rusqlite::Result<()> {
        if let Some(db) = self.connection.take() {
            if let Err((db, error)) = db.close() {
                self.connection = Some(db);
                return Err(error);
            }
        }
        Ok(())
    }
}
impl Drop for ExtractedDatabase {
    fn drop(&mut self) {
        if self.close().is_err() {
            std::mem::forget(self.connection.take());
            std::mem::forget(self.directory.take());
        }
    }
}
fn verify_archive(
    archive: &Path,
    parent: &Path,
    expected: &ContentProof,
    cancel_flag: Arc<AtomicBool>,
) -> BackupResult<ArchiveEvidence> {
    verify_with_space(archive, parent, expected, cancel_flag, |p| {
        fs2::available_space(p)
    })
}
fn verify_with_space(
    archive: &Path,
    parent: &Path,
    expected: &ContentProof,
    cancel_flag: Arc<AtomicBool>,
    space: impl Fn(&Path) -> std::io::Result<u64>,
) -> BackupResult<ArchiveEvidence> {
    extract_verified_with_space(archive, parent, expected, cancel_flag, space)
        .map(|(evidence, _directory)| evidence)
}
fn extract_verified_with_space(
    archive: &Path,
    parent: &Path,
    expected: &ContentProof,
    cancel_flag: Arc<AtomicBool>,
    space: impl Fn(&Path) -> std::io::Result<u64>,
) -> BackupResult<(ArchiveEvidence, private::PrivateDir)> {
    if !matches!(expected.version, 1 | 2) {
        return Err("Unsupported import content proof".into());
    }
    cancel(&cancel_flag).map_err(|_| "Protection cancelled")?;
    let (directory, manifest) = backup::restore::extract_for_import(
        File::open(archive)?,
        parent,
        cancel_flag.clone(),
        space,
    )?;
    let path = directory.path().join(backup::DATABASE);
    let db = open_readonly(&path)?;
    let mut owned = ExtractedDatabase {
        connection: Some(db),
        directory: Some(directory),
    };
    let result = (|| -> BackupResult<ContentProof> {
        let db = owned.connection.as_mut().unwrap();
        backup::restore::harden(db, cancel_flag.clone())?;
        // Keep the preservation contract's full row allowance, not the strict
        // restore parser's narrower single-document SQLite length limit.
        db.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            crate::open_export::ROW_LIMIT as i32,
        )?;
        let content = open_import::verify_database_version(db, &cancel_flag, expected.version)
            .map_err(|e| format!("Import protection content validation failed: {e:?}"))?;
        if &content != expected {
            return Err("Import protection content proof differs from source".into());
        }
        Ok(content)
    })();
    // No evidence escapes if either content validation OR checked close failed.
    owned.close()?;
    let content = result?;
    cancel(&cancel_flag).map_err(|_| "Protection cancelled")?;
    let evidence = ArchiveEvidence {
        manifest,
        content,
        archive_sha256: backup::digest_file(archive)?,
    };
    Ok((
        evidence,
        owned.directory.take().ok_or("Missing extraction owner")?,
    ))
}
fn source_proof(active: &mut ActiveDatabase, flag: &AtomicBool) -> BackupResult<ContentProof> {
    active
        .ensure_current()
        .map_err(|_| "Protection source identity changed")?;
    no_sidecars(active.path()).map_err(|_| "Unsafe sidecar beside protection source")?;
    let db = active
        .connection
        .as_mut()
        .ok_or("Protection source is closed")?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    open_import::verify_database_version(db, flag, if version == 0 { 1 } else { 2 })
        .map_err(|e| format!("Protection source validation failed: {e:?}").into())
}
pub(super) fn create<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    active: &mut ActiveDatabase,
    flag: Arc<AtomicBool>,
) -> BackupResult<Protection<'m, 's>> {
    create_with(maintenance, active, flag, |_| Ok(()), durability::sync)
}
fn create_with<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    active: &mut ActiveDatabase,
    flag: Arc<AtomicBool>,
    before_verification: impl FnOnce(&Path) -> BackupResult<()>,
    sync: impl FnMut(SyncPoint, &Path) -> std::io::Result<()>,
) -> BackupResult<Protection<'m, 's>> {
    maintenance.ensure_usable()?;
    create_owned(maintenance, active, flag, before_verification, sync)
}
// This distinct entry cannot turn an arbitrary recovery latch into authority.
// It requires the session held by this exact running Maintenance owner.
pub(super) fn create_for_session<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    session: &RecoverySession<'_, 's>,
    active: &mut ActiveDatabase,
    flag: Arc<AtomicBool>,
) -> BackupResult<Protection<'m, 's>> {
    session.ensure_publication_phase()?;
    if session.maintenance_binding() != Some((maintenance.operation_id(), maintenance.generation()))
        || session.database_path()? != active.path()
    {
        return Err("Protection session binding mismatch".into());
    }
    create_owned(maintenance, active, flag, |_| Ok(()), durability::sync)
}
fn create_owned<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    active: &mut ActiveDatabase,
    flag: Arc<AtomicBool>,
    before_verification: impl FnOnce(&Path) -> BackupResult<()>,
    sync: impl FnMut(SyncPoint, &Path) -> std::io::Result<()>,
) -> BackupResult<Protection<'m, 's>> {
    durability::require_supported().map_err(|_| "Mandatory protection durability unsupported")?;
    let expected = source_proof(active, &flag)?;
    let source_identity = active.identity();
    let root = active
        .path()
        .parent()
        .ok_or("Missing protection root")?
        .to_owned();
    let backups = root.join("backups");
    private::ensure_directory(&backups)?;
    let directory = backups.join("pre-restore");
    private::ensure_directory(&directory)?;
    let path = directory.join(format!(
        "pre-import-{}.mno-backup",
        maintenance.operation_id()
    ));
    let mut evidence = None;
    let sync = RefCell::new(sync);
    let manifest = backup::create_with_publication(
        active.connection(),
        &path,
        backup::BackupKind::Manual,
        Duration::from_secs(30),
        |archive, intended| {
            before_verification(archive)?;
            let verified = verify_archive(archive, &directory, &expected, flag.clone())?;
            if &verified.manifest != intended {
                return Err("Import protection manifest changed".into());
            }
            evidence = Some(verified);
            (sync.borrow_mut())(SyncPoint::VerifiedFile, archive)
                .map_err(durability::SyncFailure)?;
            Ok(())
        },
        |published| {
            let root_parent = root.parent().unwrap_or(Path::new("."));
            for dir in [&directory, &backups, &root, root_parent] {
                (sync.borrow_mut())(SyncPoint::Directory, dir).map_err(durability::SyncFailure)?;
            }
            (sync.borrow_mut())(SyncPoint::FinalFileBarrier, published)
                .map_err(durability::SyncFailure)?;
            Ok(())
        },
    )?;
    // Maintenance drains MNO writes. Also reject any observed source change;
    // a complete archive is preserved even when this late check fails.
    if source_proof(active, &flag)? != expected || active.identity() != source_identity {
        return Err("Protection source changed during snapshot".into());
    }
    let evidence = evidence.ok_or("Protection was not verified")?;
    if evidence.manifest != manifest {
        return Err("Protection manifest mismatch".into());
    }
    Ok(Protection {
        path,
        evidence,
        operation_id: maintenance.operation_id(),
        generation: maintenance.generation(),
        source_identity,
        _maintenance: PhantomData,
    })
}

#[cfg(test)]
mod tests;

// Shared preservation evidence for preparation and the isolated v2 classifier. None of these
// methods mint a strict restore proof or an activation/admission capability.
impl Protection<'_, '_> {
    pub(super) fn is_bound_to(
        &self,
        maintenance: &Maintenance<'_>,
        active: &ActiveDatabase,
    ) -> bool {
        self.operation_id == maintenance.operation_id()
            && self.generation == maintenance.generation()
            && self.source_identity == active.identity()
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn content(&self) -> &ContentProof {
        &self.evidence.content
    }
    pub(super) fn archive_sha256(&self) -> &str {
        &self.evidence.archive_sha256
    }
}
pub(super) fn verify_bound_archive(
    path: &Path,
    parent: &Path,
    expected: &ContentProof,
    expected_archive_sha256: &str,
    flag: Arc<AtomicBool>,
) -> BackupResult<()> {
    let evidence = verify_archive(path, parent, expected, flag)?;
    if evidence.archive_sha256 != expected_archive_sha256 {
        return Err("Protection archive binding mismatch".into());
    }
    Ok(())
}

// Closed, independently verified snapshot only. No strict restore/admission proof.
pub(super) fn extract_bound_archive(
    path: &Path,
    parent: &Path,
    expected: &ContentProof,
    archive_sha256: &str,
    flag: Arc<AtomicBool>,
) -> BackupResult<private::PrivateDir> {
    let (evidence, directory) =
        extract_verified_with_space(path, parent, expected, flag, |p| fs2::available_space(p))?;
    if evidence.archive_sha256 != archive_sha256 {
        return Err("Protection archive binding mismatch".into());
    }
    Ok(directory)
}
