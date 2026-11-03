//! Startup-only schema 0 -> 1. The caller holds DirectoryLease before any open.
//! Only a private candidate is written. A single same-volume rename publishes
//! a closed, verified database; either side of a crash is a complete schema.
//! Abandoned candidates/journals stay under backups, never in the active gate.
use super::*;
use crate::backup::pre_restore::durability::{self, SyncPoint};
use crate::storage_content::Proof;

#[derive(Debug)]
pub(crate) enum Error {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Verification,
    Binding,
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
type Outcome<T> = std::result::Result<T, Error>;

// The same explicit checked-close rule as activation::Work, with only the
// owners needed for migration. No maintenance/session or new Recovery authority.
struct Work {
    writer: Option<Connection>,
    protection: Option<PreparedRestore>,
    active: Option<ActiveDatabase>,
}
impl Work {
    fn close_all(&mut self) {
        if let Some(db) = self.writer.take() {
            checked_close(db);
        }
        if let Some(mut active) = self.active.take() {
            match active.close_inner() {
                Ok(files) => drop(files),
                Err(_) => std::process::abort(),
            }
        }
        if let Some(protection) = self.protection.take() {
            match protection.try_close() {
                Ok(files) => drop(files),
                Err(_) => std::process::abort(),
            }
        }
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        self.close_all();
    }
}
fn checked_close(db: Connection) {
    if db.close().is_err() {
        std::process::abort();
    }
}
fn no_sidecars(path: &Path) -> Outcome<()> {
    if crate::startup::artifacts::sidecars(path, &mut |p| fs::symlink_metadata(p))? {
        return Err(Error::Binding);
    }
    Ok(())
}
fn no_legacy(root: &Path) -> Outcome<()> {
    for name in [
        crate::startup::artifacts::RECOVERY,
        crate::startup::artifacts::PENDING,
        crate::startup::artifacts::RECEIPT,
    ] {
        match fs::symlink_metadata(root.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
            Ok(_) => return Err(Error::Binding),
        }
    }
    if crate::startup::artifacts::unexpected_root(root, None)? {
        return Err(Error::Binding);
    }
    Ok(())
}
fn sync_file(path: &Path, _point: &str) -> Outcome<()> {
    durability::sync(SyncPoint::FinalFileBarrier, path)?;
    Ok(())
}
fn sync_dir(path: &Path, _point: &str) -> Outcome<()> {
    durability::sync(SyncPoint::Directory, path)?;
    Ok(())
}
fn verify_file(path: &Path) -> Outcome<Proof> {
    no_sidecars(path)?;
    let mut work = Work {
        writer: None,
        protection: None,
        active: Some(ActiveDatabase::open(path).map_err(|_| Error::Binding)?),
    };
    let db = work.active.as_mut().unwrap().connection.as_mut().unwrap();
    // Apply existing hardening, then v2's unchanged row budget (v1 is frozen).
    super::super::restore::harden_open_connection(db, Arc::new(AtomicBool::new(false)))
        .map_err(|_| Error::Verification)?;
    db.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        crate::storage_content::ROW_LIMIT as i32,
    )?;
    let proof = crate::storage_content::verify(db, &AtomicBool::new(false))
        .map_err(|_| Error::Verification)?;
    work.close_all();
    Ok(proof)
}

pub(crate) fn initialize(target: &Path) -> Outcome<()> {
    no_sidecars(target)?;
    let mut work = Work {
        writer: None,
        protection: None,
        active: Some(ActiveDatabase::open(target).map_err(|_| Error::Binding)?),
    };
    // Use the already checked canonical source parent (/var may alias /private/var).
    let target_path = work.active.as_ref().unwrap().path().to_owned();
    let target = target_path.as_path();
    let root = target.parent().ok_or(Error::Binding)?;
    no_legacy(root)?;
    let flag = AtomicBool::new(false);
    // The source is read-only. Proof includes unusual preserved journal values.
    let source = work.active.as_ref().unwrap().connection();
    source.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        crate::storage_content::ROW_LIMIT as i32,
    )?;
    source.execute_batch("BEGIN")?;
    let before =
        crate::storage_content::snapshot(source, &flag).map_err(|_| Error::Verification)?;
    let version: u32 = source.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != 0 {
        return Err(Error::Binding);
    }
    let backups = root.join("backups");
    private::ensure_directory(&backups)?;
    let staging = backups.join("schema-migration");
    private::ensure_directory(&staging)?;
    // No RAII cleanup: interrupted work is retained as evidence, never replayed.
    let directory = staging.join(uuid::Uuid::new_v4().to_string());
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
    }
    sync_dir(&backups, "migration-sync-backups")?;
    sync_dir(&staging, "migration-sync-staging")?;
    sync_dir(root, "migration-sync-backups-parent")?;
    let candidate = directory.join("next.sqlite");
    drop(private::create_file(&candidate)?);
    work.writer = Some(Connection::open_with_flags(
        &candidate,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?);
    let writer = work.writer.as_mut().unwrap();
    writer.execute_batch("PRAGMA synchronous=EXTRA; PRAGMA fullfsync=ON;")?;
    // Same SQLite backup API used by physical backup; source stays read-only.
    {
        let backup =
            rusqlite::backup::Backup::new(work.active.as_ref().unwrap().connection(), writer)?;
        backup.run_to_completion(128, std::time::Duration::from_millis(5), None)?;
    }
    crash_point("copied");
    #[cfg(test)]
    crate::database::weather_schema::before_commit_for_test(writer, |_| {
        crash_point("transaction");
        Ok(())
    })
    .map_err(|_| Error::Verification)?;
    #[cfg(not(test))]
    crate::database::weather_schema::migrate(writer).map_err(|_| Error::Verification)?;
    crash_point("migrated");
    if crate::storage_content::verify(writer, &flag).map_err(|_| Error::Verification)? != before {
        return Err(Error::Verification);
    }
    // Persist and strictly re-read a v2 backup before replacing the original.
    // The migrated schema adds no content; the proof binds all original values.
    let archive = directory.join("before-activation.mno-backup");
    crate::backup::create_backup(writer, &archive, crate::backup::BackupKind::Automatic)
        .map_err(|_| Error::Verification)?;
    sync_file(&archive, "migration-sync-protection")?;
    sync_dir(&directory, "migration-sync-protection-directory")?;
    work.protection = Some(
        super::super::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false)))
            .map_err(|_| Error::Verification)?,
    );
    if crate::storage_content::verified_snapshot(
        work.protection
            .as_ref()
            .unwrap()
            .source()
            .map_err(|_| Error::Verification)?,
        &flag,
    )
    .map_err(|_| Error::Verification)?
        != before
    {
        return Err(Error::Verification);
    }
    work.active
        .as_ref()
        .unwrap()
        .ensure_current()
        .map_err(|_| Error::Binding)?;
    work.close_all(); // Reuse checked-close policy before every filesystem barrier/swap.
    crash_point("closed");
    let expected = before;
    if verify_file(&candidate)? != expected || verify_file(target)? != expected {
        return Err(Error::Verification);
    }
    sync_file(&candidate, "migration-sync-candidate")?;
    sync_dir(&directory, "migration-sync-candidate-directory")?;
    no_legacy(root)?;
    no_sidecars(target)?;
    no_sidecars(&candidate)?;
    crash_point("before-replace");
    // Same filesystem and closed SQLite owners, as in activation::replace_closed().
    fs::rename(&candidate, target)?;
    crash_point("after-replace");
    sync_dir(&directory, "migration-sync-source")?;
    sync_dir(root, "migration-sync-root")?;
    sync_file(target, "migration-sync-active")?;
    if verify_file(target)? != expected {
        return Err(Error::Verification);
    }
    crash_point("verified");
    Ok(())
}
fn crash_point(_name: &str) {
    #[cfg(test)]
    if std::env::var("MNO_MIGRATION_CRASH").as_deref() == Ok(_name) {
        std::process::exit(89);
    }
}
