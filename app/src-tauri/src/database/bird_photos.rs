//! Schema 10: one photo owned by a bird row, never by a journal entry.
use rusqlite::Connection;
pub(crate) const DDL: &str = "CREATE TABLE bird_row_photos (row_id TEXT PRIMARY KEY NOT NULL REFERENCES bird_rows(id) ON DELETE CASCADE, asset_id TEXT NOT NULL UNIQUE, original_path TEXT NOT NULL, mime TEXT NOT NULL CHECK (mime IN ('image/jpeg','image/png')), width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 2500), height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 2500), preview BLOB NOT NULL CHECK (length(preview) BETWEEN 1 AND 4194304));";
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_requests::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_photo_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 10)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 10 {
        return Ok(false);
    }
    let mut reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        crate::database::weather_schema::DDL,
        crate::database::place_schema::DDL,
        crate::database::weather_default_schema::DDL,
        crate::photographs::DDL,
        super::bird_schema::DDL,
        super::bird_visits::DDL,
    ] {
        reference.execute_batch(ddl)?;
    }
    reference.pragma_update(None, "user_version", 6)?;
    super::bird_visit_identity::upgrade(&mut reference)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    reference.execute_batch(super::bird_comments::DDL)?;
    reference.execute_batch(super::bird_requests::DDL)?;
    reference.execute_batch(DDL)?;
    Ok(objects(db)? == objects(&reference)?)
}

fn objects(db: &Connection) -> rusqlite::Result<Vec<(String, String, String, Option<String>)>> {
    db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?
                    .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect()
}
