//! Internal, synchronous backup primitives. Future callers must run these off the UI thread.
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempDir};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) type BackupResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const FORMAT: &str = "mno-backup";
const FORMAT_VERSION: u32 = 1;
// Backup format v1 currently accepts only schema 0. This is a protocol rule,
// not an alias to the application's current database creation version.
const BACKUP_V1_SCHEMA_VERSION: u32 = 0;
const MANIFEST: &str = "manifest.json";
const DATABASE: &str = "journal.sqlite";
const MANIFEST_LIMIT: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackupKind {
    Manual,
    Automatic,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupManifest {
    pub format: String,
    pub format_version: u32,
    pub backup_id: String,
    /// UTC completion time of the snapshot, not a guarantee of its exact starting instant.
    pub created_at: String,
    pub app_version: String,
    pub database_schema_version: u32,
    /// No stable book identity exists yet. Never invent a new identity per backup.
    pub book_id: Option<String>,
    pub kind: BackupKind,
    pub database_file: String,
    pub database_size: u64,
    pub database_sha256: String,
}

/// Copies committed main-database contents; never alters source or replaces destination.
/// The caller supplies an open source connection and an existing destination directory.
pub(crate) fn create_backup(
    source: &Connection,
    destination: &Path,
    kind: BackupKind,
) -> BackupResult<BackupManifest> {
    create_with_timeout(source, destination, kind, Duration::from_secs(30))
}

fn create_with_timeout(
    source: &Connection,
    destination: &Path,
    kind: BackupKind,
    timeout: Duration,
) -> BackupResult<BackupManifest> {
    create_before_publish(source, destination, kind, timeout, |_, _| Ok(()))
}

// The no-op production callback is optimized away. Tests synchronize or fail at
// the exact publication boundary, after real snapshot/package verification.
fn create_before_publish(
    source: &Connection,
    destination: &Path,
    kind: BackupKind,
    timeout: Duration,
    before_publish: impl FnOnce(&Path, &BackupManifest) -> BackupResult<()>,
) -> BackupResult<BackupManifest> {
    create_with_publication(source, destination, kind, timeout, before_publish, |_| {
        Ok(())
    })
}

// Mandatory protections add a fallible durability barrier AFTER no-clobber publication.
// Errors after publication must never remove the complete published archive.
fn create_with_publication(
    source: &Connection,
    destination: &Path,
    kind: BackupKind,
    timeout: Duration,
    before_publish: impl FnOnce(&Path, &BackupManifest) -> BackupResult<()>,
    after_publish: impl FnOnce(&Path) -> BackupResult<()>,
) -> BackupResult<BackupManifest> {
    if !source.is_autocommit() {
        return Err("Backup requires a source connection without an active transaction".into());
    }
    if destination.extension().and_then(|s| s.to_str()) != Some("mno-backup") {
        return Err("Backup destination must end with .mno-backup".into());
    }
    if destination.try_exists()? {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists).into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = restore::private::PrivateDir::new(parent)?;
    let snapshot_path = staging.path().join(DATABASE);
    drop(restore::private::create_file(&snapshot_path)?);
    let schema_version;
    {
        let mut snapshot = Connection::open(&snapshot_path)?;
        {
            let backup = Backup::new(source, &mut snapshot)?;
            let started = Instant::now();
            loop {
                if started.elapsed() >= timeout {
                    return Err(
                        "Backup exceeded its time limit; source database was not changed".into(),
                    );
                }
                match backup.step(128)? {
                    StepResult::Done => break,
                    StepResult::More | StepResult::Busy | StepResult::Locked => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    _ => return Err("Unsupported SQLite backup step result".into()),
                }
            }
        }
        // A fresh destination uses DELETE mode: the snapshot is a standalone file.
        let mode: String = snapshot.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if mode.eq_ignore_ascii_case("wal") {
            let mode: String =
                snapshot.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
            if !mode.eq_ignore_ascii_case("delete") {
                return Err("Could not make snapshot standalone".into());
            }
        }
        schema_version =
            snapshot.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?;
        check_database(&snapshot)?;
        match schema_version {
            1 => {
                crate::storage_content::verify(
                    &mut snapshot,
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
            }
            2 => {
                crate::storage_content::verify_v3(
                    &mut snapshot,
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
            }
            3 => {
                crate::storage_content::verify_v4(
                    &mut snapshot,
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
            }
            5 => {
                crate::storage_content::verify_v6(
                    &mut snapshot,
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
            }
            4 => {
                crate::storage_content::verify_v5(
                    &mut snapshot,
                    &std::sync::atomic::AtomicBool::new(false),
                )
                .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
            }
            _ => {}
        }
        snapshot.close().map_err(|(_, error)| error)?;
    }
    let manifest = BackupManifest {
        format: FORMAT.into(),
        format_version: schema_version + 1,
        backup_id: uuid::Uuid::new_v4().to_string(),
        created_at: OffsetDateTime::now_utc().format(&Rfc3339)?,
        app_version: env!("CARGO_PKG_VERSION").into(),
        database_schema_version: schema_version,
        book_id: None,
        kind,
        database_file: DATABASE.into(),
        database_size: snapshot_path.metadata()?.len(),
        database_sha256: digest_file(&snapshot_path)?,
    };
    let archive = NamedTempFile::new_in(parent)?;
    {
        let mut zip = ZipWriter::new(archive.as_file());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        zip.start_file(MANIFEST, options)?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        zip.start_file(DATABASE, options)?;
        io::copy(&mut File::open(&snapshot_path)?, &mut zip)?;
        zip.finish()?;
    }
    archive.as_file().sync_all()?;
    // Reopen the actual package and verify its extracted database, not just staging.
    let verified = verify_backup_in(archive.path(), staging.path())?;
    if manifest != verified {
        return Err("Written backup manifest differs from the intended manifest".into());
    }
    before_publish(archive.path(), &manifest)?;
    // Existing files are never overwritten. A later durability failure preserves the published file.
    // Drop the failed temporary file here, rather than storing it in the
    // returned PersistError for as long as a caller retains that error.
    archive
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    after_publish(destination)?;
    Ok(manifest)
}

/// Validates v1 packages without touching a live journal or extracting arbitrary paths.
pub(crate) fn verify_backup(path: &Path) -> BackupResult<BackupManifest> {
    let staging = TempDir::new()?;
    verify_backup_in(path, staging.path())
}

fn verify_backup_in(path: &Path, staging: &Path) -> BackupResult<BackupManifest> {
    let file = File::open(path)?;
    check_archive_entries(&file)?;
    let mut zip = ZipArchive::new(file)?;
    if zip.len() != 2 {
        return Err("Version 1 requires exactly manifest.json and journal.sqlite".into());
    }
    let mut names = std::collections::BTreeSet::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if entry.compression() != CompressionMethod::Stored
            || ![MANIFEST, DATABASE].contains(&entry.name())
            || !names.insert(entry.name().to_owned())
        {
            return Err("Unsupported or duplicate archive entry".into());
        }
    }
    let manifest: BackupManifest = {
        let mut entry = zip.by_name(MANIFEST)?;
        if entry.size() > MANIFEST_LIMIT {
            return Err("Backup manifest is too large".into());
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        serde_json::from_slice(&bytes)?
    };
    if manifest.format != FORMAT
        || !matches!(
            (manifest.format_version, manifest.database_schema_version),
            (1, 0) | (2, 1) | (3, 2) | (4, 3) | (5, 4) | (6, 5)
        )
        || manifest.database_file != DATABASE
        || manifest.book_id.is_some()
        || manifest.app_version.is_empty()
    {
        return Err("Unsupported backup format, schema version or book identity".into());
    }
    uuid::Uuid::parse_str(&manifest.backup_id)?;
    let created_at = OffsetDateTime::parse(&manifest.created_at, &Rfc3339)?;
    if created_at.offset() != time::UtcOffset::UTC {
        return Err("Backup timestamp must be UTC".into());
    }
    let mut extracted = NamedTempFile::new_in(staging)?;
    {
        let mut entry = zip.by_name(DATABASE)?;
        if entry.size() != manifest.database_size {
            return Err("Database size differs from manifest".into());
        }
        let copied = io::copy(&mut entry, &mut extracted)?;
        if copied != manifest.database_size {
            return Err("Truncated backup database".into());
        }
    }
    extracted.flush()?;
    if digest_file(extracted.path())? != manifest.database_sha256 {
        return Err("Database checksum differs from manifest".into());
    }
    let mut database =
        Connection::open_with_flags(extracted.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let actual: u32 = database.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if actual != manifest.database_schema_version {
        return Err("Backup schema differs from manifest".into());
    }
    check_database(&database)?;
    if manifest.format_version == 6 {
        crate::storage_content::verify_v6(
            &mut database,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
    } else if manifest.format_version == 5 {
        crate::storage_content::verify_v5(
            &mut database,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
    } else if manifest.format_version == 2 {
        crate::storage_content::verify(&mut database, &std::sync::atomic::AtomicBool::new(false))
            .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
    } else if manifest.format_version == 3 {
        crate::storage_content::verify_v3(
            &mut database,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
    } else if manifest.format_version == 4 {
        crate::storage_content::verify_v4(
            &mut database,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map_err(|e| format!("Invalid preserved content: {e:?}"))?;
    }
    database.close().map_err(|(_, error)| error)?;
    Ok(manifest)
}

// zip::ZipArchive indexes by name and discards duplicate central-directory
// records. Inspect the actual records first using rawzip's streaming iterator.
fn check_archive_entries(file: &File) -> BackupResult<()> {
    let mut buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
    let archive = rawzip::ZipArchive::from_file(file.try_clone()?, &mut buffer)?;
    let mut entries = archive.entries(&mut buffer);
    let mut names = std::collections::BTreeSet::new();
    while let Some(entry) = entries.next_entry()? {
        let name = entry.file_path().as_bytes();
        if ![MANIFEST.as_bytes(), DATABASE.as_bytes()].contains(&name)
            || !names.insert(name.to_vec())
        {
            return Err("Unexpected or duplicate central-directory entry".into());
        }
    }
    if names.len() != 2 || archive.entries_hint() != 2 {
        return Err("Version 1 requires exactly two actual archive entries".into());
    }
    Ok(())
}

fn check_database(database: &Connection) -> BackupResult<()> {
    let result: String = database.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(format!("SQLite quick_check failed: {result}").into());
    }
    let version: u32 = database.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 5 {
        if !crate::database::bird_schema::validate_schema(database)? {
            return Err("Unsupported database schema".into());
        }
        return Ok(());
    }
    if version == 4 {
        if !crate::photographs::validate_schema(database)? {
            return Err("Unsupported database schema".into());
        }
        return Ok(());
    }
    if version == 3 {
        if !crate::database::weather_default_schema::validate_schema3(database)? {
            return Err("Unsupported database schema".into());
        }
        return Ok(());
    }
    if version == 2 {
        if !crate::database::place_schema::validate_schema2(database)? {
            return Err("Unsupported database schema".into());
        }
        return Ok(());
    }
    if version == 1 {
        if !crate::database::validate_supported_schema(database)? {
            return Err("Unsupported database schema".into());
        }
        return Ok(());
    }
    if version != BACKUP_V1_SCHEMA_VERSION {
        return Err("Unsupported database schema version".into());
    }
    let is_table: bool = database.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='journal_entries')",
        [],
        |row| row.get(0),
    )?;
    if !is_table {
        return Err("Backup has no journal_entries table".into());
    }
    database.prepare("SELECT id, entry_date, title, content_json, document_version, created_at, updated_at FROM journal_entries LIMIT 0")?;
    Ok(())
}

fn digest_file(path: &Path) -> BackupResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) mod journal_content;
pub(crate) mod pre_restore;
pub(crate) mod restore;
pub(crate) mod restore_context;

#[cfg(test)]
mod tests;

// Deliberately absent from all production and development app builds.
#[cfg(test)]
mod isolated_copy;

// Isolated C5a persistence probe, never compiled into application builds.
#[cfg(test)]
mod windows_initialization_probe;
