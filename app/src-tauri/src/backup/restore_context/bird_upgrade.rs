//! macOS startup-only additive 4 -> 5 publication under the caller's DirectoryLease.
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
    no_sidecars(path)?;
    let mut active = ActiveDatabase::open(path).map_err(error)?;
    let result = (|| {
        let db = active.connection.as_mut().unwrap();
        if bird {
            crate::storage_content::verify_v6(db, &AtomicBool::new(false)).map_err(error)?;
            db.execute_batch("BEGIN").map_err(error)?;
            let result = crate::storage_content::verified_snapshot_v5(db, &AtomicBool::new(false))
                .map_err(error);
            db.execute_batch("ROLLBACK").map_err(error)?;
            result
        } else {
            crate::storage_content::verify_v5(db, &AtomicBool::new(false)).map_err(error)
        }
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
    initialize_with(path, |_point, _| {
        #[cfg(test)]
        if std::env::var("MNO_BIRD_UPGRADE_CRASH").as_deref() == Ok(_point) {
            std::process::exit(77);
        }
        Ok(())
    })
}
fn initialize_with(path: &Path, mut point: impl FnMut(&str, &Path) -> Result<()>) -> Result<()> {
    no_sidecars(path)?;
    let mut active = ActiveDatabase::open(path).map_err(error)?;
    let canonical = active.path().to_owned();
    let path = canonical.as_path();
    let version = active
        .connection()
        .pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
        .map_err(error);
    if version.as_ref().is_ok_and(|v| *v != 4) {
        if active.close_inner().is_err() {
            std::process::abort();
        }
        // A retry after rename must re-establish barriers before Ready.
        if version? == 5 {
            verify(path, true)?;
            sync_file(path)?;
            sync_dir(path.parent().ok_or("bird_schema_upgrade_parent_missing")?)?;
        }
        return crate::database::initialize_database(path);
    }
    // Work owns checked-close ordering even when an intermediate operation fails.
    let mut writer = Writer::default();
    let result = (|| {
        version?;
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
        crate::storage_content::verify_v5(source, &flag)
            .map_err(|e| format!("database_content_validation_failed: {e:?}"))?;
        source.execute_batch("BEGIN").map_err(error)?;
        let before = crate::storage_content::snapshot_v5(source, &flag).map_err(error)?;
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
        crate::database::bird_schema::upgrade(next)?;
        point("upgraded", &candidate)?;
        crate::storage_content::verify_v6(next, &flag).map_err(error)?;
        next.execute_batch("BEGIN").map_err(error)?;
        let after = crate::storage_content::verified_snapshot_v5(next, &flag).map_err(error)?;
        next.execute_batch("ROLLBACK").map_err(error)?;
        if before != after {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        active.ensure_current().map_err(error)?;
        close(writer.0.take().unwrap());
        if active.close_inner().is_err() {
            std::process::abort();
        }
        if verify(path, false)? != before || verify(&candidate, true)? != before {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        // Preserve an independent byte-identical source, after all SQLite
        // owners close. A retry or a later older-app write cannot mutate it.
        let retained = directory.join("schema4.sqlite");
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
            || verify(&retained, false)? != before
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
        if verify(path, false)? != before || verify(&candidate, true)? != before {
            return Err("bird_schema_upgrade_content_changed".into());
        }
        fs::rename(&candidate, path).map_err(error)?;
        point("after-publish", path)?;
        sync_dir(&directory)?;
        point("publication-flush", root)?;
        sync_dir(root)?;
        sync_file(path)?;
        if verify(path, true)? != before {
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
