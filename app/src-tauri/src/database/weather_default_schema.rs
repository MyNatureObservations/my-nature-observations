//! Current schema-3 extension for the optional weather default place.
use rusqlite::Connection;

pub(crate) const DDL: &str = "
CREATE TABLE weather_default_place (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    place_id TEXT REFERENCES places(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL CHECK (revision >= 1)
);
";

type SchemaObject = (String, String, String, Option<String>);

fn schema_objects(db: &Connection) -> rusqlite::Result<Vec<SchemaObject>> {
    let mut statement =
        db.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema LIMIT 13")?;
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

pub(crate) fn validate_schema3(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<i64, _>(Some("main"), "user_version", |r| r.get(0))? != 3 {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    reference.execute_batch(crate::database::weather_schema::DDL)?;
    reference.execute_batch(crate::database::place_schema::DDL)?;
    reference.execute_batch(DDL)?;
    reference.pragma_update(Some("main"), "user_version", 3)?;
    Ok(schema_objects(db)? == schema_objects(&reference)?)
}

/// Extend only the transaction that creates a brand-new current database.
/// There is deliberately no established schema-2 migration entry point.
pub(crate) fn create_in_transaction(db: &Connection) -> Result<(), String> {
    if db.is_autocommit() {
        return Err("database_creation_requires_transaction".into());
    }
    let version: i64 = db
        .pragma_query_value(Some("main"), "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version != 2
        || !crate::database::place_schema::validate_schema2(db).map_err(|e| e.to_string())?
    {
        return Err("database_schema_incompatible".into());
    }
    db.execute_batch(DDL).map_err(|e| e.to_string())?;
    db.pragma_update(Some("main"), "user_version", 3)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "weather_default_schema/tests.rs"]
mod tests;
