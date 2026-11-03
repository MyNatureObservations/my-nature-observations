//! Additive production migration. Never parses or rewrites journal documents/dates.
use rusqlite::{Connection, TransactionBehavior};
pub(crate) const DDL: &str = "CREATE TABLE journal_weather (
    entry_id TEXT PRIMARY KEY NOT NULL
        REFERENCES journal_entries(id) ON DELETE RESTRICT,
    format_version INTEGER NOT NULL CHECK (format_version = 1),
    manual_json TEXT,
    fetched_json TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1)
);";
pub(crate) fn migrate(db: &mut Connection) -> Result<(), String> {
    migrate_with(db, |_| Ok(()))
}
fn migrate_with(
    db: &mut Connection,
    checkpoint: impl FnOnce(&Connection) -> Result<(), String>,
) -> Result<(), String> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    upgrade_in_transaction(&tx)?;
    checkpoint(&tx)?;
    if !super::validate_supported_schema(&tx).map_err(|e| e.to_string())? {
        return Err("database_schema_incompatible".into());
    }
    tx.commit().map_err(|e| e.to_string())
}
// Only newly reserved databases and private migration candidates may apply
// this DDL. Established active databases are never upgraded by a live edit.
pub(crate) fn upgrade_in_transaction(db: &Connection) -> Result<(), String> {
    if db.is_autocommit() {
        return Err("database_migration_requires_transaction".into());
    }
    if !super::validate_supported_schema(db).map_err(|e| e.to_string())? {
        return Err("database_schema_incompatible".into());
    }
    let version: i64 = db
        .pragma_query_value(Some("main"), "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version == 0 {
        db.execute_batch(DDL).map_err(|e| e.to_string())?;
        db.pragma_update(Some("main"), "user_version", 1)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_preserves_all_legacy_bytes_and_rolls_back() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::database::initialize_schema(&db).unwrap();
        let content = r#" {"unknown":{"a":1,"a":2},"type":"legacy"} "#;
        db.execute(
            "INSERT INTO journal_entries VALUES (?1,?2,?3,?4,1,?5,?6)",
            rusqlite::params![
                "odd / id",
                "2026-02-30",
                "  title  ",
                content,
                "not a timestamp",
                ""
            ],
        )
        .unwrap();
        assert!(migrate_with(&mut db, |_| Err("injected".into())).is_err());
        assert!(crate::database::validate_schema(&db).unwrap());
        migrate(&mut db).unwrap();
        migrate(&mut db).unwrap();
        assert!(crate::database::validate_supported_schema(&db).unwrap());
        let actual: (String,String,String,String,String,String) = db.query_row("SELECT id,entry_date,title,content_json,created_at,updated_at FROM journal_entries",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
        assert_eq!(
            actual,
            (
                "odd / id".into(),
                "2026-02-30".into(),
                "  title  ".into(),
                content.into(),
                "not a timestamp".into(),
                "".into()
            )
        );
    }
}

#[cfg(test)]
pub(crate) fn before_commit_for_test(
    db: &mut Connection,
    checkpoint: impl FnOnce(&Connection) -> Result<(), String>,
) -> Result<(), String> {
    migrate_with(db, checkpoint)
}
