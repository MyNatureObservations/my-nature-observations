//! Schema 13: source-owned eBird snapshots, separate from manual bird lists.
use rusqlite::Connection;
pub(crate) const DDL: &str = "
CREATE TABLE ebird_visits (id TEXT PRIMARY KEY NOT NULL, submission_id TEXT NOT NULL UNIQUE, source_json TEXT NOT NULL);
CREATE TABLE ebird_rows (id TEXT PRIMARY KEY NOT NULL, visit_id TEXT NOT NULL REFERENCES ebird_visits(id) ON DELETE CASCADE, position INTEGER NOT NULL CHECK(position>=0), active INTEGER NOT NULL CHECK(active IN (0,1)), source_json TEXT NOT NULL);
CREATE UNIQUE INDEX ebird_active_position ON ebird_rows(visit_id,position) WHERE active=1;
CREATE TABLE ebird_additions (id TEXT PRIMARY KEY NOT NULL, visit_id TEXT NOT NULL REFERENCES ebird_visits(id) ON DELETE CASCADE, row_id TEXT REFERENCES ebird_rows(id) ON DELETE RESTRICT, content TEXT NOT NULL);
CREATE TABLE journal_ebird_visits (entry_id TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE, visit_id TEXT NOT NULL REFERENCES ebird_visits(id) ON DELETE CASCADE, PRIMARY KEY(entry_id,visit_id));
";
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_visit_weather::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("ebird_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 13)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 13 {
        return Ok(false);
    }
    let mut reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        super::weather_schema::DDL,
        super::place_schema::DDL,
        super::weather_default_schema::DDL,
        crate::photographs::DDL,
        super::bird_schema::DDL,
        super::bird_visits::DDL,
    ] {
        reference.execute_batch(ddl)?;
    }
    reference.pragma_update(None, "user_version", 6)?;
    super::bird_visit_identity::upgrade(&mut reference)
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        super::bird_comments::DDL,
        super::bird_requests::DDL,
        super::bird_photos::DDL,
        super::bird_captions::DDL,
        super::bird_visit_weather::DDL,
        DDL,
    ] {
        reference.execute_batch(ddl)?;
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
    Ok(objects(db)? == objects(&reference)?)
}

pub(crate) fn weather_schema(db: &Connection) -> rusqlite::Result<bool> {
    Ok(validate_schema(db)? || super::bird_visit_weather::validate_schema(db)?)
}
