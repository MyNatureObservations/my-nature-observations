//! Observation-list ownership. Internal macOS boundary until schema-5 activation.
use crate::{database, database_state::DatabaseState, journal_search::SearchState};
use rusqlite::params;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpeciesCount {
    pub avibase_id: String,
    pub taxonomy_version: String,
    pub count: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct List {
    pub id: String,
    pub visit_date: String,
    pub place_id: String,
    pub rows: Vec<SpeciesCount>,
    pub journal_entries: Vec<String>,
}

// Creation only: never an upsert. Place ownership remains with the shared registry.
pub(crate) fn create(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    visit_date: &str,
    place_id: &str,
    rows: &[SpeciesCount],
    first_journal_entry: Option<&str>,
) -> Result<String, String> {
    search.with_write(|| {
        // Resolve the immutable startup-owned location before taking the long-lived
        // permit (ready_database_path itself takes a short permit).
        let path = state.ready_database_path()?;
        let _operation = state.operation(Some(generation))?;
        if rows.is_empty() {
            return Err("bird_list_rows_required".into());
        }
        let mut db = database::open_existing(&path)?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        if !database::bird_schema::validate_schema(&tx).map_err(|e| e.to_string())? {
            return Err("bird_list_schema_unsupported".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO bird_lists(id,visit_date,place_id) VALUES (?1,?2,?3)", params![id,visit_date,place_id]).map_err(|e| e.to_string())?;
        for (position, row) in rows.iter().enumerate() {
            tx.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count) VALUES (?1,?2,?3,?4,?5,?6)", params![uuid::Uuid::new_v4().to_string(),id,position as i64,row.avibase_id,row.taxonomy_version,row.count]).map_err(|e| e.to_string())?;
        }
        if let Some(entry) = first_journal_entry {
            tx.execute("INSERT INTO journal_bird_lists(entry_id,list_id) VALUES (?1,?2)", params![entry,id]).map_err(|e| e.to_string())?;
        }
        // Validate content and foreign keys inside the same transaction: an invalid
        // date, position or identifier cannot leave an otherwise committed list.
        crate::storage_content::snapshot_v6(&tx, &AtomicBool::new(false))
            .map_err(|e| format!("bird_list_invalid: {e:?}"))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(id)
    })
}

pub(crate) fn reopen(state: &DatabaseState, generation: u32, id: &str) -> Result<List, String> {
    let path = state.ready_database_path()?;
    let _operation = state.operation(Some(generation))?;
    let mut db = database::open_existing(&path)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    crate::storage_content::snapshot_v6(&tx, &AtomicBool::new(false))
        .map_err(|e| format!("bird_list_invalid: {e:?}"))?;
    let (visit_date, place_id) = tx
        .query_row(
            "SELECT visit_date,place_id FROM bird_lists WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let rows = tx.prepare("SELECT avibase_id,taxonomy_version,count FROM bird_rows WHERE list_id=?1 ORDER BY position").map_err(|e| e.to_string())?.query_map([id], |r| Ok(SpeciesCount {avibase_id:r.get(0)?, taxonomy_version:r.get(1)?,count:r.get(2)?})).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    let journal_entries = tx.prepare("SELECT entry_id FROM journal_bird_lists WHERE list_id=?1 ORDER BY entry_id COLLATE BINARY").map_err(|e| e.to_string())?.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(List {
        id: id.into(),
        visit_date,
        place_id,
        rows,
        journal_entries,
    })
}
