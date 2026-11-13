//! Schema 7: a list is an independent visit, not a place/date singleton.
use rusqlite::Connection;
const DDL: &str = "
CREATE TABLE bird_lists_next (
 id TEXT PRIMARY KEY NOT NULL,
 visit_date TEXT NOT NULL,
 place_id TEXT NOT NULL REFERENCES places(id) ON DELETE RESTRICT,
 visit_start TEXT,
 visit_end TEXT
);
INSERT INTO bird_lists_next SELECT id,visit_date,place_id,visit_start,visit_end FROM bird_lists;
DROP TABLE bird_lists;
ALTER TABLE bird_lists_next RENAME TO bird_lists;
";
fn rebuild(db: &mut Connection) -> rusqlite::Result<()> {
    if !db.is_autocommit() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let foreign_keys: bool = db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    db.pragma_update(None, "foreign_keys", false)?;
    let result = (|| {
        let tx = db.transaction()?;
        tx.execute_batch(DDL)?;
        if tx
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        tx.pragma_update(None, "user_version", 7)?;
        tx.commit()
    })();
    let restored = db.pragma_update(None, "foreign_keys", foreign_keys);
    result.and(restored)
}
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_visits::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_identity_source_schema_invalid".into());
    }
    rebuild(db).map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 7 {
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
    rebuild(&mut reference)?;
    Ok(super::bird_schema::objects(db)? == super::bird_schema::objects(&reference)?)
}
