//! Schema 11: optional caption owned by its bird-row photo.
use rusqlite::Connection;
pub(crate) const DDL: &str = "ALTER TABLE bird_row_photos ADD COLUMN caption TEXT;";
pub(crate) fn upgrade(db: &mut Connection) -> Result<(), String> {
    if !super::bird_photos::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_caption_source_schema_invalid".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(DDL).map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", 11)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))? != 11 {
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
    reference.execute_batch(super::bird_photos::DDL)?;
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

pub(crate) fn valid_caption(value: Option<&str>) -> bool {
    value.is_none_or(|s| !s.contains('\0') && s.len() <= 16384 && s.encode_utf16().count() <= 4000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_photo_records_and_required_caption_contract() {
        let old = serde_json::json!({"row_id":"row","asset_id":"asset","original_path":"/synthetic","mime":"image/png","width":1,"height":1,"preview":"AQID"});
        assert!(serde_json::from_value::<crate::bird_photos::LegacyPhoto>(old.clone()).is_ok());
        assert!(serde_json::from_value::<crate::bird_photos::CaptionedPhoto>(old.clone()).is_err());
        let mut new = old;
        new["caption"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<crate::bird_photos::CaptionedPhoto>(new.clone()).is_ok());
        assert!(serde_json::from_value::<crate::bird_photos::LegacyPhoto>(new).is_err());
        assert!(valid_caption(Some("Bildtext")));
        assert!(!valid_caption(Some(&"a".repeat(4001))));
        assert!(!valid_caption(Some("bad\0")));
    }
}
