//! Observation-list ownership. Internal macOS boundary until schema-5 activation.
use crate::{database, database_state::DatabaseState, journal_search::SearchState};
use rusqlite::{OptionalExtension, params};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct SpeciesCount {
    pub avibase_id: String,
    pub taxonomy_version: String,
    pub count: i64,
    pub comment: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct List {
    pub id: String,
    pub visit_date: String,
    pub place_id: String,
    pub visit_start: Option<String>,
    pub visit_end: Option<String>,
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
    create_with_visit(
        state,
        search,
        generation,
        visit_date,
        place_id,
        rows,
        first_journal_entry,
        None,
        None,
    )
}
pub(crate) fn create_with_visit(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    visit_date: &str,
    place_id: &str,
    rows: &[SpeciesCount],
    first_journal_entry: Option<&str>,
    visit_start: Option<&str>,
    visit_end: Option<&str>,
) -> Result<String, String> {
    create_with_identity(
        state,
        search,
        generation,
        visit_date,
        place_id,
        rows,
        first_journal_entry,
        visit_start,
        visit_end,
        None,
    )
}
pub(crate) fn create_with_identity(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    visit_date: &str,
    place_id: &str,
    rows: &[SpeciesCount],
    first_journal_entry: Option<&str>,
    visit_start: Option<&str>,
    visit_end: Option<&str>,
    request_id: Option<&str>,
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
        let receipts=database::bird_requests::validate_schema(&tx).map_err(|e|e.to_string())?;
        let comments=receipts||database::bird_comments::validate_schema(&tx).map_err(|e|e.to_string())?;
        if rows.iter().any(|r|!database::bird_comments::valid_comment(r.comment.as_deref())||(!comments&&r.comment.is_some())){return Err("bird_comment_invalid".into());}
        let independent=comments||database::bird_visit_identity::validate_schema(&tx).map_err(|e|e.to_string())?;
        let visits=independent||database::bird_visits::validate_schema(&tx).map_err(|e|e.to_string())?;
        if !visits && !database::bird_schema::validate_schema(&tx).map_err(|e| e.to_string())? {
            return Err("bird_list_schema_unsupported".into());
        }
        if (!visits && (visit_start.is_some()||visit_end.is_some())) || !database::bird_visits::valid_time(visit_start)||!database::bird_visits::valid_time(visit_end) {return Err("bird_visit_invalid".into());}
        let id=if let Some(request)=request_id {
            let parsed=uuid::Uuid::parse_str(request).map_err(|_|"bird_request_invalid")?;
            if !independent||parsed.get_version_num()!=4||parsed.get_variant()!=uuid::Variant::RFC4122||parsed.to_string()!=request {return Err("bird_request_invalid".into());}
            let payload=request_digest(visit_date,place_id,rows,first_journal_entry,visit_start,visit_end);
            let receipt=if receipts {tx.query_row("SELECT payload_sha256,completed FROM bird_creation_requests WHERE request_id=?1",[request],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,bool>(1)?))).optional().map_err(|e|e.to_string())?}else{None};
            if receipt.as_ref().and_then(|v|v.0.as_ref()).is_some_and(|hash|hash!=&payload){return Err("bird_request_conflict".into());}
            if receipts&&receipt.is_none(){return Err("bird_request_unreserved".into());}
            if receipts&&receipt.as_ref().is_some_and(|v|v.1){
             let live:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bird_lists WHERE id=?1)",[request],|r|r.get(0)).map_err(|e|e.to_string())?;
             if !live{return Err("bird_request_retired".into());}
             if receipt.as_ref().is_some_and(|v|v.0.is_some()){
              crate::storage_content::snapshot_v10(&tx,&AtomicBool::new(false)).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
              tx.rollback().map_err(|e|e.to_string())?;return Ok(request.to_owned());
             }
             return Err("bird_request_legacy".into());
            }
            let existing=tx.query_row("SELECT visit_date,place_id,visit_start,visit_end FROM bird_lists WHERE id=?1",[request],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?)));
            match existing {
                Ok((date,place,start,end))=>{
                    (if receipts {crate::storage_content::snapshot_v10(&tx,&AtomicBool::new(false))} else if comments {crate::storage_content::snapshot_v9(&tx,&AtomicBool::new(false))}else{crate::storage_content::snapshot_v8(&tx,&AtomicBool::new(false))}).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
                    let stored=tx.prepare(if comments {"SELECT avibase_id,taxonomy_version,count,comment FROM bird_rows WHERE list_id=?1 ORDER BY position"}else{"SELECT avibase_id,taxonomy_version,count,NULL FROM bird_rows WHERE list_id=?1 ORDER BY position"}).map_err(|e|e.to_string())?.query_map([request],|r|Ok(SpeciesCount{avibase_id:r.get(0)?,taxonomy_version:r.get(1)?,count:r.get(2)?,comment:r.get(3)?})).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
                    let linked=if let Some(entry)=first_journal_entry { tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_bird_lists WHERE list_id=?1 AND entry_id=?2)",params![request,entry],|r|r.get::<_,bool>(0)).map_err(|e|e.to_string())? }else{false};
                    if date!=visit_date||place!=place_id||start.as_deref()!=visit_start||end.as_deref()!=visit_end||stored!=rows||!linked {return Err("bird_request_conflict".into());}
                    tx.rollback().map_err(|e|e.to_string())?;
                    return Ok(request.to_owned());
                }
                Err(rusqlite::Error::QueryReturnedNoRows)=>{},
                Err(e)=>return Err(e.to_string()),
            }
            request.to_owned()
        }else{uuid::Uuid::new_v4().to_string()};
        if receipts {
         let hash=request_id.map(|_|request_digest(visit_date,place_id,rows,first_journal_entry,visit_start,visit_end));
         if request_id.is_some(){
          if tx.execute("UPDATE bird_creation_requests SET payload_sha256=?2,completed=1 WHERE request_id=?1 AND completed=0",params![id,hash]).map_err(|e|e.to_string())?!=1{return Err("bird_request_conflict".into());}
         }else{tx.execute("INSERT INTO bird_creation_requests(request_id,payload_sha256,completed) VALUES (?1,NULL,1)",[&id]).map_err(|e|e.to_string())?;}
        }
        tx.execute("INSERT INTO bird_lists(id,visit_date,place_id) VALUES (?1,?2,?3)", params![id,visit_date,place_id]).map_err(|e| e.to_string())?;
        if visits {tx.execute("UPDATE bird_lists SET visit_start=?1,visit_end=?2 WHERE id=?3",params![visit_start,visit_end,id]).map_err(|e|e.to_string())?;}
        for (position, row) in rows.iter().enumerate() {
            tx.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count) VALUES (?1,?2,?3,?4,?5,?6)", params![uuid::Uuid::new_v4().to_string(),id,position as i64,row.avibase_id,row.taxonomy_version,row.count]).map_err(|e| e.to_string())?;
            if comments {tx.execute("UPDATE bird_rows SET comment=?1 WHERE list_id=?2 AND position=?3",params![row.comment,id,position as i64]).map_err(|e|e.to_string())?;}
        }
        if let Some(entry) = first_journal_entry {
            tx.execute("INSERT INTO journal_bird_lists(entry_id,list_id) VALUES (?1,?2)", params![entry,id]).map_err(|e| e.to_string())?;
        }
        // Validate content and foreign keys inside the same transaction: an invalid
        // date, position or identifier cannot leave an otherwise committed list.
        (if receipts {crate::storage_content::snapshot_v10(&tx,&AtomicBool::new(false))} else if comments {crate::storage_content::snapshot_v9(&tx,&AtomicBool::new(false))} else if independent {crate::storage_content::snapshot_v8(&tx,&AtomicBool::new(false))} else if visits {crate::storage_content::snapshot_v7(&tx,&AtomicBool::new(false))} else {crate::storage_content::snapshot_v6(&tx, &AtomicBool::new(false))})
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
    let list = read_list(&tx, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(list)
}
fn read_list(db: &rusqlite::Connection, id: &str) -> Result<List, String> {
    let receipts = database::bird_requests::validate_schema(db).map_err(|e| e.to_string())?;
    let comments =
        receipts || database::bird_comments::validate_schema(db).map_err(|e| e.to_string())?;
    let independent = comments
        || database::bird_visit_identity::validate_schema(db).map_err(|e| e.to_string())?;
    let visits =
        independent || database::bird_visits::validate_schema(db).map_err(|e| e.to_string())?;
    (if receipts {
        crate::storage_content::snapshot_v10(db, &AtomicBool::new(false))
    } else if comments {
        crate::storage_content::snapshot_v9(db, &AtomicBool::new(false))
    } else if independent {
        crate::storage_content::snapshot_v8(db, &AtomicBool::new(false))
    } else if visits {
        crate::storage_content::snapshot_v7(db, &AtomicBool::new(false))
    } else {
        crate::storage_content::snapshot_v6(db, &AtomicBool::new(false))
    })
    .map_err(|e| format!("bird_list_invalid: {e:?}"))?;
    let (visit_date, place_id) = db
        .query_row(
            "SELECT visit_date,place_id FROM bird_lists WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let rows = db.prepare(if comments {"SELECT avibase_id,taxonomy_version,count,comment FROM bird_rows WHERE list_id=?1 ORDER BY position"}else{"SELECT avibase_id,taxonomy_version,count,NULL FROM bird_rows WHERE list_id=?1 ORDER BY position"}).map_err(|e| e.to_string())?.query_map([id], |r| Ok(SpeciesCount {avibase_id:r.get(0)?, taxonomy_version:r.get(1)?,count:r.get(2)?,comment:r.get(3)?})).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    let journal_entries = db.prepare("SELECT entry_id FROM journal_bird_lists WHERE list_id=?1 ORDER BY entry_id COLLATE BINARY").map_err(|e| e.to_string())?.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    let (visit_start, visit_end) = if visits {
        db.query_row(
            "SELECT visit_start,visit_end FROM bird_lists WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?
    } else {
        (None, None)
    };
    Ok(List {
        id: id.into(),
        visit_date,
        place_id,
        visit_start,
        visit_end,
        rows,
        journal_entries,
    })
}

// An optimistic content token rejects stale editors without another schema field.
pub(crate) fn edit_token(list: &List) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(list).expect("serializable list"))
    )
}
pub(crate) fn update(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry: &str,
    expected: &str,
    date: &str,
    place: &str,
    start: Option<&str>,
    end: Option<&str>,
    rows: &[SpeciesCount],
) -> Result<String, String> {
    search.with_write(|| {
  let path=state.ready_database_path()?;
  let _operation=state.operation(Some(generation))?;
  let mut db=database::open_existing(&path)?;
  let tx=db.transaction().map_err(|e|e.to_string())?;
  if !(database::bird_requests::validate_schema(&tx).map_err(|e|e.to_string())?||database::bird_comments::validate_schema(&tx).map_err(|e|e.to_string())?) {return Err("bird_schema_unsupported".into());}
  let previous=read_list(&tx,id)?;
  if !previous.journal_entries.iter().any(|e|e==entry) {return Err("bird_link_missing".into());}
  if edit_token(&previous)!=expected {return Err("bird_edit_stale".into());}
  if rows.is_empty()||rows.len()>11172 {return Err("bird_rows_invalid".into());}
  for row in rows {
   if row.count<=0||!database::bird_comments::valid_comment(row.comment.as_deref()) {return Err("bird_row_invalid".into());}
   // Retain recorded identities, including duplicates/older catalog versions.
   if !previous.rows.iter().any(|old|old.avibase_id==row.avibase_id&&old.taxonomy_version==row.taxonomy_version)
    && (row.taxonomy_version!=crate::species_registry::VERSION||crate::species_registry::get(&row.avibase_id).is_none()) {return Err("bird_species_invalid".into());}
  }
  tx.execute("UPDATE bird_lists SET visit_date=?1,place_id=?2,visit_start=?3,visit_end=?4 WHERE id=?5",params![date,place,start,end,id]).map_err(|e|e.to_string())?;
  tx.execute("DELETE FROM bird_rows WHERE list_id=?1",[id]).map_err(|e|e.to_string())?;
  for (position,row) in rows.iter().enumerate() {
   tx.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count,comment) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![uuid::Uuid::new_v4().to_string(),id,position as i64,row.avibase_id,row.taxonomy_version,row.count,row.comment]).map_err(|e|e.to_string())?;
  }
  let updated=read_list(&tx,id)?; // Full schema/content/reference verification before commit.
  let token=edit_token(&updated);
  tx.commit().map_err(|e|e.to_string())?;
  Ok(token)
 })
}

pub(crate) fn delete(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    expected: &str,
    confirmed: bool,
) -> Result<(), String> {
    delete_with_hook(
        state,
        search,
        generation,
        id,
        expected,
        confirmed,
        || Ok(()),
    )
}

// The hook exercises rollback after the owned rows and links have cascaded.
pub(crate) fn delete_with_hook(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    expected: &str,
    confirmed: bool,
    after_delete: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !confirmed {
        return Err("bird_delete_confirmation_required".into());
    }
    search.with_write(|| {
        let path = state.ready_database_path()?;
        let _operation = state.operation(Some(generation))?;
        let mut db = database::open_existing(&path)?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        if !(database::bird_requests::validate_schema(&tx).map_err(|e| e.to_string())?
            || database::bird_comments::validate_schema(&tx).map_err(|e| e.to_string())?)
        {
            return Err("bird_schema_unsupported".into());
        }
        let list = read_list(&tx, id)?;
        if edit_token(&list) != expected {
            return Err("bird_edit_stale".into());
        }
        let deleted = tx
            .execute("DELETE FROM bird_lists WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        if deleted != 1 {
            return Err("bird_list_missing".into());
        }
        after_delete()?;
        (if database::bird_requests::validate_schema(&tx).map_err(|e| e.to_string())? {
            crate::storage_content::snapshot_v10(&tx, &AtomicBool::new(false))
        } else {
            crate::storage_content::snapshot_v9(&tx, &AtomicBool::new(false))
        })
        .map_err(|e| format!("bird_list_invalid: {e:?}"))?;
        tx.commit().map_err(|e| e.to_string())
    })
}

pub(crate) fn exists(state: &DatabaseState, generation: u32, id: &str) -> Result<bool, String> {
    let path = state.ready_database_path()?;
    let _operation = state.operation(Some(generation))?;
    let mut db = database::open_existing(&path)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    (if database::bird_requests::validate_schema(&tx).map_err(|e| e.to_string())? {
        crate::storage_content::snapshot_v10(&tx, &AtomicBool::new(false))
    } else {
        crate::storage_content::snapshot_v9(&tx, &AtomicBool::new(false))
    })
    .map_err(|e| format!("bird_list_invalid: {e:?}"))?;
    let exists = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bird_lists WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(exists)
}

fn request_digest(
    date: &str,
    place: &str,
    rows: &[SpeciesCount],
    entry: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};
    let bytes =
        serde_json::to_vec(&(date, place, rows, entry, start, end)).expect("serializable request");
    format!(
        "{:x}",
        Sha256::new()
            .chain_update(b"MNO-bird-creation-request-v1\0")
            .chain_update(bytes)
            .finalize()
    )
}

// Only this backend boundary issues new IDs. Missing IDs are never auto-created
// by create_with_identity, including after importing an older archive.
pub(crate) fn reserve(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    reserve_id(state, search, generation, &id)?;
    Ok(id)
}
pub(crate) fn reserve_id(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
) -> Result<(), String> {
    search.with_write(||{
  let path=state.ready_database_path()?;let _permit=state.operation(Some(generation))?;
  let mut db=database::open_existing(&path)?;let tx=db.transaction().map_err(|e|e.to_string())?;
  crate::storage_content::snapshot_v10(&tx,&AtomicBool::new(false)).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
  let parsed=uuid::Uuid::parse_str(id).map_err(|_|"bird_request_invalid")?;
  if parsed.get_version_num()!=4||parsed.get_variant()!=uuid::Variant::RFC4122||parsed.to_string()!=id{return Err("bird_request_invalid".into());}
  tx.execute("INSERT INTO bird_creation_requests(request_id,payload_sha256,completed) VALUES (?1,NULL,0)",[id]).map_err(|e|e.to_string())?;
  tx.commit().map_err(|e|e.to_string())
 })
}
