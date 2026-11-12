//! Separate schema-0 database + atomic local activation (macOS only).
//! The marker deliberately remains: disk replacement is NOT Ready admission.
//! Cooperative local-process model: Maintenance and DirectoryLease exclude MNO
//! operations; pathname metadata is not protection against an active same-user
//! substituter. The app installs its abort hook before entering this module.
//! Tests call destructive scenarios only in isolated subprocesses.
use super::*;
use crate::backup::{
    journal_content::{self, VerifiedJournal},
    pre_restore::durability::{self, SyncPoint},
};
use crate::database_state::recovery_session::RecoverySession;
use rusqlite::ffi;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    sync::atomic::Ordering,
};
const MARKER: &str = "activation-v1.json";
const IMPORT_FORMAT: &str = "mno-open-import-activation";
const LIMIT: u64 = 16 * 1024;
type Outcome<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub(crate) enum Error {
    Coordination,
    Candidate(PrepareError),
    Protection(ProtectionError),
    Binding,
    Cancelled,
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Verification,
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Content {
    version: u32,
    sha256: String,
}
impl From<journal_content::ContentFingerprint> for Content {
    fn from(value: journal_content::ContentFingerprint) -> Self {
        Self {
            version: value.version,
            sha256: value.sha256,
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Marker {
    format: String,
    version: u32,
    operation_id: String,
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    old_schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_format_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    export_format_version: Option<u32>,
    old_content: Content,
    new_content: Content,
    protection_file: String,
    protection_sha256: String,
    candidate_file: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMarker {
    format: String,
    version: u32,
    operation_id: String,
    schema_version: u32,
    #[serde(default)]
    old_schema_version: Option<u32>,
    #[serde(default)]
    backup_format_version: Option<u32>,
    old_content: Content,
    new_content: Content,
    protection_file: String,
    protection_sha256: String,
    candidate_file: String,
}
impl From<LegacyMarker> for Marker {
    fn from(m: LegacyMarker) -> Self {
        Self {
            format: m.format,
            version: m.version,
            operation_id: m.operation_id,
            schema_version: m.schema_version,
            old_schema_version: m.old_schema_version,
            backup_format_version: m.backup_format_version,
            export_format_version: None,
            old_content: m.old_content,
            new_content: m.new_content,
            protection_file: m.protection_file,
            protection_sha256: m.protection_sha256,
            candidate_file: m.candidate_file,
        }
    }
}
impl Marker {
    fn is_import(&self) -> bool {
        self.format == IMPORT_FORMAT && matches!(self.version, 1 | 4 | 5 | 6 | 7)
    }
}
// Inert disk observation, not a completion/admission capability.
#[derive(Debug, PartialEq, Eq)]
enum Observed {
    Old,
    New,
    Same,
    Other,
}
pub(crate) struct DiskActivated<'m, 's> {
    marker: Marker,
    _session: RecoverySession<'m, 's>,
}
fn cancel(flag: &AtomicBool) -> Outcome<()> {
    if flag.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn point(_name: &str) -> Outcome<()> {
    #[cfg(test)]
    return tests::point(_name);
    #[cfg(not(test))]
    Ok(())
}
fn sync_file(path: &Path, name: &str) -> Outcome<()> {
    point(name)?;
    // Called only after checked SQLite-close, never during a same-inode lock episode.
    let file = File::open(path)?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, &file)?;
    Ok(())
}
fn sync_dir(path: &Path, name: &str) -> Outcome<()> {
    point(name)?;
    durability::sync(SyncPoint::Directory, path)?;
    Ok(())
}
fn no_sidecars(path: &Path) -> Outcome<()> {
    if crate::startup::artifacts::sidecars(path, &mut |p| fs::symlink_metadata(p))? {
        Err(Error::Binding)
    } else {
        Ok(())
    }
}
fn present(path: &Path) -> Outcome<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn no_legacy(root: &Path) -> Outcome<()> {
    for name in [
        "recovery",
        "pending-restore.json",
        ".mno-restore-completion-v1.json",
    ] {
        if present(&root.join(name))? {
            return Err(Error::Binding);
        }
    }
    if crate::startup::artifacts::unexpected_root(root, None)? {
        return Err(Error::Binding);
    }
    no_sidecars(&root.join("mno.sqlite"))
}
fn fingerprint(candidate: &PreparedRestore, flag: Arc<AtomicBool>) -> Outcome<Content> {
    let snapshot =
        VerifiedJournal::from_candidate(candidate, flag).map_err(|_| Error::Verification)?;
    let content = journal_content::fingerprint(&snapshot).map_err(|_| Error::Verification)?;
    snapshot.finish().map_err(|_| Error::Verification)?;
    Ok(content.into())
}
fn close(db: Connection) {
    #[cfg(test)]
    let result = tests::close(db);
    #[cfg(not(test))]
    let result = db.close();
    if let Err(_original_connection_and_error) = result {
        std::process::abort();
    }
}
fn verify_file(path: &Path) -> Outcome<Content> {
    no_sidecars(path)?;
    // Startup may encounter a damaged/replaced header. Reject WAL before native
    // SQLite can create SHM. All episode connections are already closed here.
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
    let result = (|| {
        let snapshot = VerifiedJournal::verify(&mut db, Arc::new(AtomicBool::new(false)))
            .map_err(|_| Error::Verification)?;
        let content = journal_content::fingerprint(&snapshot).map_err(|_| Error::Verification)?;
        snapshot.finish().map_err(|_| Error::Verification)?;
        Ok(content.into())
    })();
    close(db);
    result
}
// One local owner, with checked closes on EVERY exit. No live connection can
// outlive the Maintenance/session and filesystem resources it depends upon.
struct Work<'m, 's> {
    writer: Option<Connection>,
    incoming: Option<PreparedRestore>,
    protection: Option<PreparedRestore>,
    active: Option<ActiveDatabase>,
    session: Option<RecoverySession<'m, 's>>,
}
impl Work<'_, '_> {
    fn close_all(&mut self) {
        if let Some(db) = self.writer.take() {
            close(db);
        }
        if let Some(active) = self.active.as_mut() {
            if active.connection.is_some() {
                match active.close_inner() {
                    Ok(files) => drop(files),
                    Err(_) => std::process::abort(),
                }
            }
        }
        drop(self.active.take());
        for source in [&mut self.incoming, &mut self.protection] {
            if let Some(candidate) = source.take() {
                match candidate.try_close() {
                    Ok(files) => drop(files),
                    Err(_retained) => std::process::abort(),
                }
            }
        }
    }
}
impl Drop for Work<'_, '_> {
    fn drop(&mut self) {
        self.close_all();
    }
}
fn publish(root: &Path, marker: &Marker) -> Outcome<()> {
    use std::os::unix::fs::DirBuilderExt;
    let directory = root.join("recovery");
    fs::DirBuilder::new().mode(0o700).create(&directory)?; // No existing P/Q directory is reused.
    sync_dir(root, "sync-recovery-created")?;
    point("before-marker")?;
    let mut file = tempfile::Builder::new()
        .prefix(".activation-")
        .tempfile_in(&directory)?;
    let bytes = serde_json::to_vec(marker).map_err(|_| Error::Binding)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    file.write_all(&bytes)?;
    file.flush()?;
    point("sync-marker-file")?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, file.as_file())?;
    let published = file
        .persist_noclobber(directory.join(MARKER))
        .map_err(|e| Error::Io(e.error))?;
    sync_dir(&directory, "sync-marker-directory")?;
    sync_dir(root, "sync-marker-root")?;
    durability::sync_opened(SyncPoint::FinalFileBarrier, &published)?;
    point("marker-durable")
}
fn read_marker(root: &Path) -> Outcome<Marker> {
    read_marker_file(&root.join("recovery").join(MARKER))
}
fn read_marker_file(path: &Path) -> Outcome<Marker> {
    let mut bytes = Vec::new();
    File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    parse_marker(&bytes)
}
// Frozen v1 reader. New contracts dispatch separately; parsing never grants
// activation or startup admission.
fn parse_v1_marker(bytes: &[u8]) -> Outcome<Marker> {
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    let marker: Marker = serde_json::from_slice::<LegacyMarker>(bytes)
        .map_err(|_| Error::Binding)?
        .into();
    let id = uuid::Uuid::parse_str(&marker.operation_id).map_err(|_| Error::Binding)?;
    let hash = |v: &str| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if (marker.format != "mno-database-activation" && marker.format != IMPORT_FORMAT)
        || marker.version != 1
        || marker.schema_version != 0
        || marker.old_schema_version.is_some()
        || marker.backup_format_version.is_some()
        || marker.export_format_version.is_some()
        || id.get_version_num() != 4
        || id.to_string() != marker.operation_id
        || marker.candidate_file != format!("next-{id}.sqlite")
        || !marker.protection_file.starts_with("pre-restore-")
        || !marker.protection_file.ends_with(".mno-backup")
        || !marker
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || marker.old_content.version != 1
        || marker.new_content.version != 1
        || (marker.is_import() && marker.old_content.sha256 != importing::EMPTY_RESTORE_SHA256)
        || !hash(&marker.old_content.sha256)
        || !hash(&marker.new_content.sha256)
        || !hash(&marker.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(marker)
}
// v2 restore is explicit: both sides use preservation proof v2. Physical
// schema-0 sources are verified under their original archive contract first.
fn parse_marker(bytes: &[u8]) -> Outcome<Marker> {
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Binding);
    }
    #[derive(Deserialize)]
    struct Header {
        format: String,
        version: u32,
    }
    let h: Header = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    if h.format == "mno-database-activation" && matches!(h.version, 5 | 6) {
        return parse_v5_restore_marker(bytes);
    }
    if h.format == IMPORT_FORMAT && matches!(h.version, 6 | 7) {
        return parse_v6_import_marker(bytes);
    }
    if h.format != "mno-database-activation" || h.version != 2 {
        if h.format == "mno-database-activation" && h.version == 3 {
            return parse_v3_marker(bytes);
        }
        if h.format == "mno-database-activation" && h.version == 4 {
            return parse_v4_restore_marker(bytes);
        }
        if h.format == IMPORT_FORMAT && h.version == 4 {
            return parse_v4_import_marker(bytes);
        }
        if h.format == IMPORT_FORMAT && h.version == 5 {
            return parse_v5_import_marker(bytes);
        }
        return parse_v1_marker(bytes);
    }
    let m: Marker = serde_json::from_slice::<LegacyMarker>(bytes)
        .map_err(|_| Error::Binding)?
        .into();
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |v: &str| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if m.schema_version != 1
        || m.old_schema_version.is_some()
        || m.backup_format_version.is_some()
        || m.export_format_version.is_some()
        || m.old_content.version != 2
        || m.new_content.version != 2
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}
fn parse_v4_restore_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if m.format != "mno-database-activation"
        || m.version != 4
        || m.schema_version != 3
        || m.old_schema_version != Some(3)
        || m.backup_format_version != Some(4)
        || m.export_format_version.is_some()
        || m.old_content.version != 4
        || m.new_content.version != 4
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}
fn parse_v3_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice::<LegacyMarker>(bytes)
        .map_err(|_| Error::Binding)?
        .into();
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |v: &str| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if m.format != "mno-database-activation"
        || m.version != 3
        || m.schema_version != 2
        || m.backup_format_version != Some(3)
        || m.export_format_version.is_some()
        || !matches!(
            (m.old_schema_version, m.old_content.version),
            (Some(1), 2) | (Some(2), 3)
        )
        || m.new_content.version != 3
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}
fn parse_v4_import_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if m.format != IMPORT_FORMAT
        || m.version != 4
        || m.schema_version != 2
        || m.backup_format_version.is_some()
        || m.export_format_version != Some(3)
        || !matches!(
            (m.old_schema_version, m.old_content.version),
            (Some(1), 2) | (Some(2), 3)
        )
        || m.new_content.version != 3
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}
fn parse_v5_import_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if m.format != IMPORT_FORMAT
        || m.version != 5
        || m.schema_version != 3
        || m.old_schema_version != Some(3)
        || m.backup_format_version.is_some()
        || m.export_format_version != Some(4)
        || m.old_content.version != 4
        || m.new_content.version != 4
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}
fn preserved_fingerprint(candidate: &PreparedRestore, flag: &AtomicBool) -> Outcome<Content> {
    let db = candidate.source().map_err(Error::Candidate)?;
    let p = crate::storage_content::verified_snapshot(db, flag).map_err(|_| Error::Verification)?;
    Ok(Content {
        version: 2,
        sha256: p.sha256,
    })
}
fn preserved_fingerprint_v3(candidate: &PreparedRestore, flag: &AtomicBool) -> Outcome<Content> {
    let db = candidate.source().map_err(Error::Candidate)?;
    let p =
        crate::storage_content::verified_snapshot_v3(db, flag).map_err(|_| Error::Verification)?;
    Ok(Content {
        version: 3,
        sha256: p.sha256,
    })
}
fn preserved_fingerprint_v4(candidate: &PreparedRestore, flag: &AtomicBool) -> Outcome<Content> {
    let db = candidate.source().map_err(Error::Candidate)?;
    let p =
        crate::storage_content::verified_snapshot_v4(db, flag).map_err(|_| Error::Verification)?;
    Ok(Content {
        version: 4,
        sha256: p.sha256,
    })
}
fn verify_file_version(path: &Path, version: u32) -> Outcome<Content> {
    if version == 1 {
        return verify_file(path);
    }
    if !matches!(version, 2 | 3 | 4 | 5 | 6) {
        return Err(Error::Binding);
    }
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
    let result = (|| {
        crate::backup::restore::harden_open_connection(&db, Arc::new(AtomicBool::new(false)))
            .map_err(Error::Candidate)?;
        (if version == 6 {
            crate::storage_content::verify_v6(&mut db, &AtomicBool::new(false))
        } else if version == 5 {
            crate::storage_content::verify_v5(&mut db, &AtomicBool::new(false))
        } else if version == 4 {
            crate::storage_content::verify_v4(&mut db, &AtomicBool::new(false))
        } else if version == 3 {
            crate::storage_content::verify_v3(&mut db, &AtomicBool::new(false))
        } else {
            crate::storage_content::verify(&mut db, &AtomicBool::new(false))
        })
        .map(|p| Content {
            version,
            sha256: p.sha256,
        })
        .map_err(|_| Error::Verification)
    })();
    close(db);
    result
}
fn check_record(root: &Path, marker: &Marker) -> Outcome<()> {
    if read_marker(root)? != *marker
        || present(&root.join(".mno-restore-completion-v1.json"))?
        || present(&root.join("pending-restore.json"))?
        || present(&root.join("recovery/pending-restore.json"))?
    {
        return Err(Error::Binding);
    }
    if crate::backup::digest_file(
        &root
            .join("backups/pre-restore")
            .join(&marker.protection_file),
    )
    .map_err(|_| Error::Verification)?
        != marker.protection_sha256
    {
        return Err(Error::Verification);
    }
    Ok(())
}
// It always leaves the activation marker in place; no phase reset, generation
// advancement, P/Q cleanup, CompletionProof or Ready is produced here.
pub(crate) fn activate<'m, 's>(
    maintenance: &'m Maintenance<'s>,
    archive: &Path,
    flag: Arc<AtomicBool>,
) -> Outcome<DiskActivated<'m, 's>> {
    maintenance
        .ensure_usable()
        .map_err(|_| Error::Coordination)?;
    cancel(&flag)?;
    point("before-input")?;
    let incoming = super::super::restore::prepare_candidate(archive, flag.clone())
        .map_err(Error::Candidate)?;
    point("input-verified")?;
    let session = maintenance
        .recovery_session()
        .map_err(|_| Error::Coordination)?;
    let target = session.database_path().map_err(|_| Error::Binding)?;
    let root = target.parent().ok_or(Error::Binding)?.to_owned();
    no_legacy(&root)?;
    let mut work = Work {
        writer: None,
        incoming: Some(incoming),
        protection: None,
        active: None,
        session: Some(session),
    };
    work.active = Some(ActiveDatabase::open(&target).map_err(|_| Error::Binding)?);
    let protection =
        pre_restore::create_bound(maintenance, work.active.as_ref().unwrap(), flag.clone())
            .map_err(Error::Protection)?;
    let new_format = work.incoming.as_ref().unwrap().manifest().format_version;
    let old_schema = protection.candidate().manifest().database_schema_version;
    if old_schema == 2 && new_format != 3 {
        // An older archive has no place registry. Never let the v2 proof,
        // which intentionally does not cover places, authorize its replacement.
        return Err(Error::Binding);
    }
    if old_schema == 3 && new_format != 4 {
        // Schema 3 adds the durable weather-default reference. Only proof v4
        // covers it, so an older archive contract cannot replace it.
        return Err(Error::Binding);
    }
    if matches!(old_schema, 4 | 5) && !matches!(new_format, 5 | 6) {
        return Err(Error::Binding);
    }
    let current = protection.candidate().manifest().database_schema_version == 1
        || work
            .incoming
            .as_ref()
            .unwrap()
            .manifest()
            .database_schema_version
            == 1;
    let old_content = if matches!(new_format, 5 | 6) {
        match old_schema {
            4 => preserved_fingerprint_v5(protection.candidate(), &flag)?,
            5 => preserved_fingerprint_v6(protection.candidate(), &flag)?,
            _ => return Err(Error::Binding),
        }
    } else if new_format == 4 {
        if old_schema != 3 {
            return Err(Error::Binding);
        }
        preserved_fingerprint_v4(protection.candidate(), &flag)?
    } else if new_format == 3 {
        match old_schema {
            1 => preserved_fingerprint(protection.candidate(), &flag)?,
            2 => preserved_fingerprint_v3(protection.candidate(), &flag)?,
            _ => return Err(Error::Binding),
        }
    } else if current {
        preserved_fingerprint(protection.candidate(), &flag)?
    } else {
        fingerprint(protection.candidate(), flag.clone())?
    };
    let new_content = if new_format == 6 {
        preserved_fingerprint_v6(work.incoming.as_ref().unwrap(), &flag)?
    } else if new_format == 5 {
        preserved_fingerprint_v5(work.incoming.as_ref().unwrap(), &flag)?
    } else if new_format == 4 {
        preserved_fingerprint_v4(work.incoming.as_ref().unwrap(), &flag)?
    } else if new_format == 3 {
        preserved_fingerprint_v3(work.incoming.as_ref().unwrap(), &flag)?
    } else if current {
        preserved_fingerprint(work.incoming.as_ref().unwrap(), &flag)?
    } else {
        fingerprint(work.incoming.as_ref().unwrap(), flag.clone())?
    };
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
    cancel(&flag)?;
    let marker = Marker {
        format: "mno-database-activation".into(),
        version: if new_format == 6 || old_schema == 5 {
            6
        } else if new_format == 5 {
            5
        } else if new_format == 4 {
            4
        } else if new_format == 3 {
            3
        } else if current {
            2
        } else {
            1
        },
        operation_id: maintenance.operation_id().to_string(),
        schema_version: if new_format == 6 {
            5
        } else if new_format == 5 {
            4
        } else if new_format == 4 {
            3
        } else if new_format == 3 {
            2
        } else if current {
            1
        } else {
            0
        },
        old_schema_version: if new_format >= 3 {
            Some(old_schema)
        } else {
            None
        },
        backup_format_version: match new_format {
            6 => Some(6),
            5 => Some(5),
            4 => Some(4),
            3 => Some(3),
            _ => None,
        },
        export_format_version: None,
        old_content,
        new_content,
        protection_file,
        protection_sha256,
        candidate_file: format!("next-{}.sqlite", maintenance.operation_id()),
    };
    publish(&root, &marker)?;
    cancel(&flag)?;
    let staged = root.join("recovery").join(&marker.candidate_file);
    // Reserve a genuinely new, local inode; no CREATE on SQLite's subsequent open.
    drop(private::create_file(&staged)?);
    work.writer = Some(Connection::open_with_flags(
        &staged,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?);
    let writer = work.writer.as_mut().unwrap();
    writer.busy_timeout(std::time::Duration::ZERO)?;
    writer.execute_batch("PRAGMA synchronous=EXTRA; PRAGMA fullfsync=ON;")?;
    #[cfg(test)]
    tests::configure(writer)?;
    point("before-copy")?;
    copy(
        work.incoming
            .as_ref()
            .unwrap()
            .source()
            .map_err(Error::Candidate)?,
        writer,
        &flag,
    )?;
    point("copy-done")?;
    if current && new_format != 3 {
        crate::database::weather_schema::migrate(writer).map_err(|_| Error::Verification)?;
    }
    let actual: Content = if new_format == 6 {
        let p =
            crate::storage_content::verify_v6(writer, &flag).map_err(|_| Error::Verification)?;
        Content {
            version: 6,
            sha256: p.sha256,
        }
    } else if new_format == 5 {
        let p =
            crate::storage_content::verify_v5(writer, &flag).map_err(|_| Error::Verification)?;
        Content {
            version: 5,
            sha256: p.sha256,
        }
    } else if new_format == 4 {
        let p =
            crate::storage_content::verify_v4(writer, &flag).map_err(|_| Error::Verification)?;
        Content {
            version: 4,
            sha256: p.sha256,
        }
    } else if new_format == 3 {
        let p =
            crate::storage_content::verify_v3(writer, &flag).map_err(|_| Error::Verification)?;
        Content {
            version: 3,
            sha256: p.sha256,
        }
    } else if current {
        let p = crate::storage_content::verify(writer, &flag).map_err(|_| Error::Verification)?;
        Content {
            version: 2,
            sha256: p.sha256,
        }
    } else {
        let snapshot =
            VerifiedJournal::verify(writer, flag.clone()).map_err(|_| Error::Verification)?;
        let p = journal_content::fingerprint(&snapshot).map_err(|_| Error::Verification)?;
        snapshot.finish().map_err(|_| Error::Verification)?;
        p.into()
    };
    if actual != marker.new_content {
        return Err(Error::Verification);
    }
    point("before-close")?;
    #[cfg(test)]
    tests::configure_close(work.writer.as_ref().unwrap());
    work.active
        .as_ref()
        .unwrap()
        .ensure_current()
        .map_err(|_| Error::Binding)?;
    work.close_all(); // ALL old/new/source SQLite handles close before fsync/replacement.
    point("all-closed")?;
    replace_closed(&root, &target, &staged, &marker, &flag)?;
    Ok(DiskActivated {
        marker,
        _session: work.session.take().unwrap(),
    })
}
// Both operations enter only after checked closure of all episode SQLite owners.
fn replace_closed(
    root: &Path,
    target: &Path,
    staged: &Path,
    marker: &Marker,
    flag: &AtomicBool,
) -> Outcome<()> {
    if verify_new_file(staged, marker)? != marker.new_content
        || verify_file_version(&target, marker.old_content.version)? != marker.old_content
    {
        return Err(Error::Verification);
    }
    no_sidecars(&staged)?;
    no_sidecars(&target)?;
    sync_file(&staged, "sync-candidate-file")?;
    sync_dir(&root.join("recovery"), "sync-candidate-directory")?;
    check_record(&root, &marker)?;
    point("before-replace")?;
    cancel(&flag)?;
    // macOS local filesystem, same directory tree/volume, no live SQLite handles.
    fs::rename(&staged, &target)?;
    point("after-replace")?;
    // After rename errors mean uncertain/changed target, NEVER original survived.
    sync_dir(&root.join("recovery"), "sync-source-directory")?;
    sync_dir(&root, "sync-active-directory")?;
    sync_file(&target, "sync-active-final")?;
    if verify_new_file(target, marker)? != marker.new_content {
        return Err(Error::Verification);
    }
    check_record(&root, &marker)?;
    point("disk-verified")?;
    Ok(())
}
fn verify_new_file(path: &Path, marker: &Marker) -> Outcome<Content> {
    if marker.format == IMPORT_FORMAT && marker.version == 1 {
        importing::verify_file(path)
    } else {
        verify_file_version(path, marker.new_content.version)
    }
}
fn inspect(session: &RecoverySession<'_, '_>) -> Outcome<Observed> {
    session
        .ensure_publication_phase()
        .map_err(|_| Error::Coordination)?;
    let path = session.database_path().map_err(|_| Error::Coordination)?;
    let root = path.parent().ok_or(Error::Binding)?;
    let marker = read_marker(root)?;
    check_record(root, &marker)?;
    let content = if matches!(marker.version, 3 | 4 | 5 | 6 | 7) {
        let old = verify_file_version(&path, marker.old_content.version)
            .is_ok_and(|v| v == marker.old_content);
        let new = verify_new_file(&path, &marker).is_ok_and(|v| v == marker.new_content);
        return Ok(match (old, new) {
            (true, true) => Observed::Same,
            (true, false) => Observed::Old,
            (false, true) => Observed::New,
            _ => Observed::Other,
        });
    } else {
        match verify_new_file(&path, &marker) {
            Ok(v) => v,
            Err(_) => return Ok(Observed::Other),
        }
    };
    Ok(
        match (
            if marker.is_import() {
                verify_file_version(&path, marker.old_content.version)
                    .is_ok_and(|v| v == marker.old_content)
            } else {
                content == marker.old_content
            },
            content == marker.new_content,
        ) {
            (true, true) => Observed::Same,
            (true, false) => Observed::Old,
            (false, true) => Observed::New,
            _ => Observed::Other,
        },
    )
}
pub(super) fn copy(source: &Connection, target: &mut Connection, flag: &AtomicBool) -> Outcome<()> {
    struct Backup(*mut ffi::sqlite3_backup);
    impl Drop for Backup {
        fn drop(&mut self) {
            if !self.0.is_null() {
                std::process::abort();
            }
        }
    }
    let raw = unsafe {
        ffi::sqlite3_backup_init(
            target.handle(),
            c"main".as_ptr(),
            source.handle(),
            c"main".as_ptr(),
        )
    };
    if raw.is_null() {
        return Err(Error::Sqlite(rusqlite::Error::SqliteFailure(
            ffi::Error::new(unsafe { ffi::sqlite3_errcode(target.handle()) }),
            None,
        )));
    }
    let mut handle = Backup(raw);
    let result = loop {
        if let Err(e) = cancel(flag).and_then(|_| point("before-copy-step")) {
            break Err(e);
        }
        let rc = unsafe { ffi::sqlite3_backup_step(handle.0, 1) };
        #[cfg(test)]
        let rc = tests::step(rc);
        if rc == ffi::SQLITE_DONE {
            break Ok(());
        }
        if rc != ffi::SQLITE_OK {
            break Err(Error::Sqlite(rusqlite::Error::SqliteFailure(
                ffi::Error::new(rc),
                None,
            )));
        }
        if let Err(e) = point("copy-step") {
            break Err(e);
        }
    };
    let rc = unsafe { ffi::sqlite3_backup_finish(handle.0) };
    handle.0 = std::ptr::null_mut();
    #[cfg(test)]
    let rc = tests::finish(rc);
    if rc != ffi::SQLITE_OK {
        return Err(Error::Sqlite(rusqlite::Error::SqliteFailure(
            ffi::Error::new(rc),
            None,
        )));
    }
    result
}
#[cfg(test)]
mod tests;

pub(crate) mod startup;

// Internal import integration; no Tauri command or UI entry.
pub(crate) mod importing;

// Read-only preview for the app confirmation, with the same strict validator.
// No PreparedRestore or descriptor ownership escapes this backup boundary.
pub(crate) fn validate_archive(path: &Path) -> Outcome<String> {
    let candidate =
        super::super::restore::prepare_candidate(path, Arc::new(AtomicBool::new(false)))
            .map_err(Error::Candidate)?;
    match candidate.try_close() {
        Ok(files) => drop(files),
        Err(_original) => std::process::abort(),
    }
    crate::backup::digest_file(path).map_err(|_| Error::Verification)
}

// Separate v2 preparation and startup contracts; all admission uses startup.
mod marker_contract;

// Preservation protection is separate from strict restore validation.
mod import_protection;

pub(crate) use marker_contract::preparation::{journal_proof, prepare_replacement};

fn parse_v5_restore_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if m.format != "mno-database-activation"
        || !(if m.version == 5 {
            m.schema_version == 4
                && m.old_schema_version == Some(4)
                && m.backup_format_version == Some(5)
                && m.old_content.version == 5
                && m.new_content.version == 5
        } else if m.version == 6 {
            matches!(m.schema_version, 4 | 5)
                && matches!(m.old_schema_version, Some(4) | Some(5))
                && (m.schema_version == 5 || m.old_schema_version == Some(5))
                && m.backup_format_version == Some(m.schema_version + 1)
                && m.old_content.version == m.old_schema_version.unwrap_or(0) + 1
                && m.new_content.version == m.schema_version + 1
        } else {
            false
        })
        || m.export_format_version.is_some()
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}

fn parse_v6_import_marker(bytes: &[u8]) -> Outcome<Marker> {
    let m: Marker = serde_json::from_slice(bytes).map_err(|_| Error::Binding)?;
    let id = uuid::Uuid::parse_str(&m.operation_id).map_err(|_| Error::Binding)?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if m.format != IMPORT_FORMAT
        || !matches!((m.version, m.schema_version), (6, 4) | (7, 5))
        || m.old_schema_version != Some(m.schema_version)
        || m.backup_format_version.is_some()
        || m.export_format_version != Some(m.schema_version + 1)
        || m.old_content.version != m.schema_version + 1
        || m.new_content.version != m.schema_version + 1
        || id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != m.operation_id
        || m.candidate_file != format!("next-{id}.sqlite")
        || !m.protection_file.starts_with("pre-restore-")
        || !m.protection_file.ends_with(".mno-backup")
        || !m
            .protection_file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !hash(&m.old_content.sha256)
        || !hash(&m.new_content.sha256)
        || !hash(&m.protection_sha256)
    {
        return Err(Error::Binding);
    }
    Ok(m)
}

fn preserved_fingerprint_v5(candidate: &PreparedRestore, flag: &AtomicBool) -> Outcome<Content> {
    let db = candidate.source().map_err(Error::Candidate)?;
    let p =
        crate::storage_content::verified_snapshot_v5(db, flag).map_err(|_| Error::Verification)?;
    Ok(Content {
        version: 5,
        sha256: p.sha256,
    })
}

fn preserved_fingerprint_v6(candidate: &PreparedRestore, flag: &AtomicBool) -> Outcome<Content> {
    let db = candidate.source().map_err(Error::Candidate)?;
    let p =
        crate::storage_content::verified_snapshot_v6(db, flag).map_err(|_| Error::Verification)?;
    Ok(Content {
        version: 6,
        sha256: p.sha256,
    })
}
