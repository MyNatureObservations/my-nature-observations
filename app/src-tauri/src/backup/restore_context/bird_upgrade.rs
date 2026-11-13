//! macOS startup-only additive 4/5 -> current schema publication under the caller's DirectoryLease.
//! Complete old and new databases exist before the single rename. Abandoned work
//! is retained under backups and never treated as startup authority.
use super::*;
use crate::backup::pre_restore::durability::{self, SyncPoint};
use crate::storage_content::Proof;

type Result<T> = std::result::Result<T, String>;

fn error(e: impl std::fmt::Debug) -> String {
    format!("bird_schema_upgrade_failed: {e:?}")
}
fn close(db: Connection) {
    if db.close().is_err() {
        std::process::abort();
    }
}
#[derive(Default)]
struct Writer(Option<Connection>);
impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(db) = self.0.take() {
            close(db);
        }
    }
}
fn no_sidecars(path: &Path) -> Result<()> {
    if crate::startup::artifacts::sidecars(path, &mut |p| fs::symlink_metadata(p)).map_err(error)? {
        return Err("bird_schema_upgrade_sidecar_present".into());
    }
    Ok(())
}
fn sync_file(path: &Path) -> Result<()> {
    durability::sync(SyncPoint::FinalFileBarrier, path).map_err(error)
}
fn sync_dir(path: &Path) -> Result<()> {
    durability::sync(SyncPoint::Directory, path).map_err(error)
}
fn verify(path: &Path, bird: bool) -> Result<Proof> {
    verify_for(path, if bird { 5 } else { 4 }, 4)
}
fn full(db: &mut Connection, schema: u32) -> Result<Proof> {
    let flag = AtomicBool::new(false);
    (match schema {
        4 => crate::storage_content::verify_v5(db, &flag),
        5 => crate::storage_content::verify_v6(db, &flag),
        6 => crate::storage_content::verify_v7(db, &flag),
        7 => crate::storage_content::verify_v8(db, &flag),
        8 => crate::storage_content::verify_v9(db, &flag),
        9 => crate::storage_content::verify_v10(db, &flag),
        _ => return Err("bird_schema_unsupported".into()),
    })
    .map_err(error)
}
fn preserved(db: &Connection, schema: u32) -> Result<Proof> {
    (if schema == 9 {
        crate::storage_content::verified_snapshot_v10(db, &AtomicBool::new(false))
    } else if schema == 8 {
        crate::storage_content::verified_snapshot_v9(db, &AtomicBool::new(false))
    } else if schema == 4 {
        crate::storage_content::verified_snapshot_v5(db, &AtomicBool::new(false))
    } else if schema == 7 {
        crate::storage_content::verified_snapshot_v8(db, &AtomicBool::new(false))
    } else if schema == 6 {
        crate::storage_content::verified_snapshot_v7(db, &AtomicBool::new(false))
    } else {
        crate::storage_content::verified_snapshot_v6(db, &AtomicBool::new(false))
    })
    .map_err(error)
}
fn verify_for(path: &Path, schema: u32, source: u32) -> Result<Proof> {
    no_sidecars(path)?;
    let mut active = ActiveDatabase::open(path).map_err(error)?;
    let result = (|| {
        let db = active.connection.as_mut().unwrap();
        full(db, schema)?;
        db.execute_batch("BEGIN").map_err(error)?;
        let p = preserved(db, source);
        db.execute_batch("ROLLBACK").map_err(error)?;
        p
    })();
    if active.close_inner().is_err() {
        std::process::abort();
    }
    result
}
// Called only after independent verification and checked SQLite closure.
pub(crate) fn confirm(path: &Path) -> Result<()> {
    no_sidecars(path)?;
    sync_file(path)?;
    sync_dir(path.parent().ok_or("bird_schema_upgrade_parent_missing")?)
}
pub(crate) fn initialize(path: &Path) -> Result<()> {
    initialize_with(path, |point, _| {
        #[cfg(test)]
        if std::env::var("MNO_BIRD_UPGRADE_CRASH").as_deref() == Ok(point) {
            std::process::exit(77);
        }
        let _ = point;
        Ok(())
    })
}
pub(crate) fn initialize_current(path: &Path) -> Result<()> {
    initialize_to(path, 9, |point, _| {
        #[cfg(test)]
        if std::env::var("MNO_BIRD_UPGRADE_CRASH").as_deref() == Ok(point) {
            std::process::exit(77);
        }
        let _ = point;
        Ok(())
    })
}
fn initialize_with(path: &Path, point: impl FnMut(&str, &Path) -> Result<()>) -> Result<()> {
    initialize_to(path, 5, point)
}
fn initialize_to(
    path: &Path,
    target: u32,
    mut point: impl FnMut(&str, &Path) -> Result<()>,
) -> Result<()> {
    no_sidecars(path)?;
    let mut active = ActiveDatabase::open(path).map_err(error)?;
    let canonical = active.path().to_owned();
    let path = canonical.as_path();
    let version = active
        .connection()
        .pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
        .map_err(error);
    if version
        .as_ref()
        .is_ok_and(|v| *v != 4 && !(*v >= 5 && *v < target && *v <= 8))
    {
        if active.close_inner().is_err() {
            std::process::abort();
        }
        // Ordinary current-schema validation also re-establishes publication barriers
        // on retry after rename, while preserving startup error classification.
        version?;
        return crate::database::initialize_database(path);
    }
    // Work owns checked-close ordering even when an intermediate operation fails.
    let mut writer = Writer::default();
    let result = (|| {
        let source_schema = version?;
        let root = path.parent().ok_or("bird_schema_upgrade_parent_missing")?;
        let backups = root.join("backups");
        private::ensure_directory(&backups).map_err(error)?;
        let staging = backups.join("bird-schema-upgrade");
        private::ensure_directory(&staging).map_err(error)?;
        let directory = staging.join(uuid::Uuid::new_v4().to_string());
        private::ensure_directory(&directory).map_err(error)?;
        sync_dir(&backups)?;
        sync_dir(&staging)?;
        sync_dir(root)?;
        let flag = AtomicBool::new(false);
        let source = active.connection.as_mut().unwrap();
        full(source, source_schema)
            .map_err(|e| format!("database_content_validation_failed: {e}"))?;
        source.execute_batch("BEGIN").map_err(error)?;
        let before = preserved(source, source_schema)?;
        let candidate = directory.join("next.sqlite");
        drop(private::create_file(&candidate).map_err(error)?);
        writer.0 = Some(
            Connection::open_with_flags(
                &candidate,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(error)?,
        );
        let next = writer.0.as_mut().unwrap();
        next.execute_batch(
            "PRAGMA synchronous=EXTRA; PRAGMA fullfsync=ON; PRAGMA foreign_keys=ON;",
        )
        .map_err(error)?;
        super::activation::copy(source, next, &flag).map_err(error)?;
        point("copied", &candidate)?;
        if source_schema == 4 {
            crate::database::bird_schema::upgrade(next)?;
        }
        if source_schema < 6 && target >= 6 {
            crate::database::bird_visits::upgrade(next)?;
        }
        if source_schema < 7 && target >= 7 {
            crate::database::bird_visit_identity::upgrade(next)?;
        }
        if source_schema < 8 && target >= 8 {
            crate::database::bird_comments::upgrade(next)?;
        }
        if target >= 9 {
            crate::database::bird_requests::upgrade(next)?;
        }
        point("upgraded", &candidate)?;
        full(next, target)?;
        next.execute_batch("BEGIN").map_err(error)?;
        let after = preserved(next, source_schema)?;
        next.execute_batch("ROLLBACK").map_err(error)?;
        if before != after {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        active.ensure_current().map_err(error)?;
        close(writer.0.take().unwrap());
        if active.close_inner().is_err() {
            std::process::abort();
        }
        if verify_for(path, source_schema, source_schema)? != before
            || verify_for(&candidate, target, source_schema)? != before
        {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        // Preserve an independent byte-identical source, after all SQLite
        // owners close. A retry or a later older-app write cannot mutate it.
        let retained = directory.join(format!("schema{source_schema}.sqlite"));
        {
            let mut input = private::open_retained(path).map_err(error)?;
            let mut output = private::create_file(&retained).map_err(error)?;
            std::io::copy(&mut input, &mut output).map_err(error)?;
            if !private::same_file(&input, path).map_err(error)? {
                return Err("bird_schema_upgrade_source_changed".into());
            }
        }
        if crate::backup::digest_file(&retained).map_err(error)?
            != crate::backup::digest_file(path).map_err(error)?
            || verify_for(&retained, source_schema, source_schema)? != before
        {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        point("source-flush", &retained)?;
        sync_file(&retained)?;
        point("candidate-flush", &candidate)?;
        sync_file(&candidate)?;
        sync_dir(&directory)?;
        point("before-publish", &candidate)?;
        no_sidecars(path)?;
        no_sidecars(&candidate)?;
        if verify_for(path, source_schema, source_schema)? != before
            || verify_for(&candidate, target, source_schema)? != before
        {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        fs::rename(&candidate, path).map_err(error)?;
        point("after-publish", path)?;
        sync_dir(&directory)?;
        point("publication-flush", root)?;
        sync_dir(root)?;
        sync_file(path)?;
        if verify_for(path, target, source_schema)? != before {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        point("verified", path)?;
        Ok(())
    })();
    if let Some(db) = writer.0.take() {
        close(db);
    }
    if active.connection.is_some() && active.close_inner().is_err() {
        std::process::abort();
    }
    result
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn initialize_schema6(path: &Path) -> Result<()> {
    initialize_to(path, 6, |_, _| Ok(()))
}

#[cfg(test)]
pub(crate) fn initialize_schema7(path: &Path) -> Result<()> {
    initialize_to(path, 7, |_, _| Ok(()))
}

#[cfg(test)]
pub(crate) fn initialize_schema8(path: &Path) -> Result<()> {
    initialize_to(path, 8, |_, _| Ok(()))
}
