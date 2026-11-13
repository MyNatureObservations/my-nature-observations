//! Schema 8 adds optional plain-text row comments without changing saved rows.
use rusqlite::Connection;
pub(crate) const DDL: &str = "ALTER TABLE bird_rows ADD COLUMN comment TEXT;";
pub(crate) const LIMIT: usize = 8192;
pub(crate) fn valid_comment(value: Option<&str>) -> bool {
    value.is_none_or(|s| s.len() <= LIMIT && !s.contains('\0'))
}
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_visit_identity::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_comment_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 8)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 8 {
        return Ok(false);
    }
    let mut r = Connection::open_in_memory()?;
    crate::database::initialize_schema(&r).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        crate::database::weather_schema::DDL,
        crate::database::place_schema::DDL,
        crate::database::weather_default_schema::DDL,
        crate::photographs::DDL,
        super::bird_schema::DDL,
        super::bird_visits::DDL,
    ] {
        r.execute_batch(ddl)?;
    }
    r.pragma_update(None, "user_version", 6)?;
    super::bird_visit_identity::upgrade(&mut r).map_err(|_| rusqlite::Error::InvalidQuery)?;
    r.execute_batch(DDL)?;
    Ok(super::bird_schema::objects(db)? == super::bird_schema::objects(&r)?)
}
