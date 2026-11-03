//! Schema-2 definition and additive schema-1 candidate migration.
use rusqlite::{Connection, TransactionBehavior};

pub(crate) const DDL: &str = "
CREATE TABLE places (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0 AND length(name) <= 512),
    latitude REAL,
    longitude REAL,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    CHECK ((latitude IS NULL) = (longitude IS NULL)),
    CHECK (latitude IS NULL OR (latitude >= -90.0 AND latitude <= 90.0)),
    CHECK (longitude IS NULL OR (longitude >= -180.0 AND longitude <= 180.0))
);

CREATE TABLE journal_places (
    entry_id TEXT PRIMARY KEY NOT NULL
        REFERENCES journal_entries(id) ON DELETE CASCADE,
    place_id TEXT REFERENCES places(id) ON DELETE RESTRICT,
    place_name_snapshot TEXT,
    latitude_snapshot REAL,
    longitude_snapshot REAL,
    place_revision_snapshot INTEGER,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    CHECK (
        (place_id IS NULL AND place_name_snapshot IS NULL
            AND latitude_snapshot IS NULL AND longitude_snapshot IS NULL
            AND place_revision_snapshot IS NULL)
        OR
        (place_id IS NOT NULL AND place_name_snapshot IS NOT NULL
            AND length(trim(place_name_snapshot)) > 0
            AND place_revision_snapshot >= 1
            AND ((latitude_snapshot IS NULL) = (longitude_snapshot IS NULL))
            AND (latitude_snapshot IS NULL OR
                (latitude_snapshot >= -90.0 AND latitude_snapshot <= 90.0))
            AND (longitude_snapshot IS NULL OR
                (longitude_snapshot >= -180.0 AND longitude_snapshot <= 180.0)))
    )
);

CREATE INDEX idx_journal_places_place_id ON journal_places(place_id);
";

type SchemaObject = (String, String, String, Option<String>);

fn schema_objects(db: &Connection) -> rusqlite::Result<Vec<SchemaObject>> {
    let mut statement =
        db.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema LIMIT 12")?;
    let mut objects = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, Option<String>>(3)?
                    .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    objects.sort();
    Ok(objects)
}

pub(crate) fn validate_schema2(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<i64, _>(Some("main"), "user_version", |r| r.get(0))? != 2 {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    reference.execute_batch(crate::database::weather_schema::DDL)?;
    reference.execute_batch(DDL)?;
    reference.pragma_update(Some("main"), "user_version", 2)?;
    Ok(schema_objects(db)? == schema_objects(&reference)?)
}

/// Upgrade only a caller-owned private candidate. Ordinary startup creates
/// schema 2 directly and never rewrites an older established database.
pub(crate) fn migrate_candidate(db: &mut Connection) -> Result<(), String> {
    migrate_candidate_with(db, |_| Ok(()))
}

fn migrate_candidate_with(
    db: &mut Connection,
    checkpoint: impl FnOnce(&Connection) -> Result<(), String>,
) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    upgrade_in_transaction(&tx)?;
    checkpoint(&tx)?;
    if !validate_schema2(&tx).map_err(|e| e.to_string())? {
        return Err("database_schema_incompatible".into());
    }
    tx.commit().map_err(|e| e.to_string())
}

pub(crate) fn upgrade_in_transaction(db: &Connection) -> Result<(), String> {
    if db.is_autocommit() {
        return Err("database_migration_requires_transaction".into());
    }
    let version: i64 = db
        .pragma_query_value(Some("main"), "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version == 2 {
        return if validate_schema2(db).map_err(|e| e.to_string())? {
            Ok(())
        } else {
            Err("database_schema_incompatible".into())
        };
    }
    if version != 1 || !crate::database::validate_supported_schema(db).map_err(|e| e.to_string())? {
        return Err("database_schema_incompatible".into());
    }
    db.execute_batch(DDL).map_err(|e| e.to_string())?;
    db.pragma_update(Some("main"), "user_version", 2)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn before_commit_for_test(
    db: &mut Connection,
    checkpoint: impl FnOnce(&Connection) -> Result<(), String>,
) -> Result<(), String> {
    migrate_candidate_with(db, checkpoint)
}

#[cfg(test)]
#[path = "place_schema/tests.rs"]
mod tests;
