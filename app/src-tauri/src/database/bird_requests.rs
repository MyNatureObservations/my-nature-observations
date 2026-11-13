//! Schema 9: durable creation receipts, independent of list deletion.
use rusqlite::Connection;
pub(crate) const DDL: &str = "CREATE TABLE bird_creation_requests (request_id TEXT PRIMARY KEY NOT NULL, payload_sha256 TEXT, completed INTEGER NOT NULL CHECK (completed IN (0,1)));";
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_comments::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_request_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    // Earlier versions did not retain the original payload. Reserve every live ID
    // without inventing a historic digest; legacy replay fails closed because the original payload cannot be proved.
    tx.execute(
        "INSERT INTO bird_creation_requests SELECT id,NULL,1 FROM bird_lists",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 9)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 9 {
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
    reference.execute_batch(DDL)?;
    Ok(super::bird_schema::objects(db)? == super::bird_schema::objects(&reference)?)
}
