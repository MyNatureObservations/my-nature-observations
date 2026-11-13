//! Schema 6 adds only optional local visit times to existing bird lists.
use rusqlite::Connection;
pub(crate) const DDL: &str = "ALTER TABLE bird_lists ADD COLUMN visit_start TEXT; ALTER TABLE bird_lists ADD COLUMN visit_end TEXT;";
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 6 {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        crate::database::weather_schema::DDL,
        crate::database::place_schema::DDL,
        crate::database::weather_default_schema::DDL,
        crate::photographs::DDL,
        super::bird_schema::DDL,
        DDL,
    ] {
        reference.execute_batch(ddl)?;
    }
    Ok(super::bird_schema::objects(db)? == super::bird_schema::objects(&reference)?)
}
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_schema::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_visit_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 6)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn valid_time(value: Option<&str>) -> bool {
    value.is_none_or(|s| {
        let b = s.as_bytes();
        b.len() == 5
            && b[2] == b':'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 2 || c.is_ascii_digit())
            && &s[..2] < "24"
            && &s[3..] < "60"
    })
}
