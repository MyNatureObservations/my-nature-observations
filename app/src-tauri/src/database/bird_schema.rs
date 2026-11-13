//! Schema 5: the first bird-list persistence boundary. No UI commands.
use rusqlite::Connection;

pub(crate) const DDL: &str = "
CREATE TABLE bird_lists (
    id TEXT PRIMARY KEY NOT NULL,
    visit_date TEXT NOT NULL,
    place_id TEXT NOT NULL REFERENCES places(id) ON DELETE RESTRICT,
    UNIQUE(place_id, visit_date)
);
CREATE TABLE bird_rows (
    id TEXT PRIMARY KEY NOT NULL,
    list_id TEXT NOT NULL REFERENCES bird_lists(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    avibase_id TEXT NOT NULL,
    taxonomy_version TEXT NOT NULL,
    count INTEGER NOT NULL CHECK (count > 0),
    UNIQUE(list_id, position)
);
CREATE TABLE journal_bird_lists (
    entry_id TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    list_id TEXT NOT NULL REFERENCES bird_lists(id) ON DELETE CASCADE,
    PRIMARY KEY(entry_id, list_id)
);
";

type Object = (String, String, String, Option<String>);
pub(super) fn objects(db: &Connection) -> rusqlite::Result<Vec<Object>> {
    db.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema ORDER BY type,name LIMIT 32")?
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, Option<String>>(3)?
                    .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect()
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 5 {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        crate::database::weather_schema::DDL,
        crate::database::place_schema::DDL,
        crate::database::weather_default_schema::DDL,
        crate::photographs::DDL,
        DDL,
    ] {
        reference.execute_batch(ddl)?;
    }
    Ok(objects(db)? == objects(&reference)?)
}
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !crate::photographs::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_upgrade_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 5)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema5_is_distinct_from_the_frozen_schema4_contract() {
        let root = tempfile::TempDir::new().unwrap();
        let path = root.path().join("journal.sqlite");
        crate::database::create_current_database(&path).unwrap();
        let mut db = crate::database::open_existing(&path).unwrap();
        upgrade(&mut db).unwrap();
        assert!(validate_schema(&db).unwrap());
        assert!(!crate::photographs::validate_schema(&db).unwrap());
        crate::storage_content::verify_v6(&mut db, &std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        db.close().unwrap();
        #[cfg(not(target_os = "macos"))]
        {
            let before = std::fs::read(&path).unwrap();
            assert_eq!(
                crate::database::initialize_database(&path).unwrap_err(),
                "database_schema_platform_unsupported"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}
