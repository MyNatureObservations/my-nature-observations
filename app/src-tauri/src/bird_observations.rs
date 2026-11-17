//! Observation-list ownership. Internal macOS boundary until schema-5 activation.
use crate::{database, database_state::DatabaseState, journal_search::SearchState};
use rusqlite::{OptionalExtension, params};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct SpeciesCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<crate::bird_photos::Photo>,
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
        let photos=crate::bird_photos::supported_schema(&tx).map_err(|e|e.to_string())?;
        let receipts=receipt_schema(&tx).map_err(|e|e.to_string())?;
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
              receipt_snapshot(&tx,&AtomicBool::new(false)).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
              tx.rollback().map_err(|e|e.to_string())?;return Ok(request.to_owned());
             }
             return Err("bird_request_legacy".into());
            }
            let existing=tx.query_row("SELECT visit_date,place_id,visit_start,visit_end FROM bird_lists WHERE id=?1",[request],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?)));
            match existing {
                Ok((date,place,start,end))=>{
                    (if receipts {receipt_snapshot(&tx,&AtomicBool::new(false))} else if comments {crate::storage_content::snapshot_v9(&tx,&AtomicBool::new(false))}else{crate::storage_content::snapshot_v8(&tx,&AtomicBool::new(false))}).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
                    let stored=tx.prepare(if comments {"SELECT avibase_id,taxonomy_version,count,comment FROM bird_rows WHERE list_id=?1 ORDER BY position"}else{"SELECT avibase_id,taxonomy_version,count,NULL FROM bird_rows WHERE list_id=?1 ORDER BY position"}).map_err(|e|e.to_string())?.query_map([request],|r|Ok(SpeciesCount{ row_id:None,photo:None,avibase_id:r.get(0)?,taxonomy_version:r.get(1)?,count:r.get(2)?,comment:r.get(3)?})).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
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
            tx.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count) VALUES (?1,?2,?3,?4,?5,?6)", params![new_row_id(&tx,row,photos)?,id,position as i64,row.avibase_id,row.taxonomy_version,row.count]).map_err(|e| e.to_string())?;
            if comments {tx.execute("UPDATE bird_rows SET comment=?1 WHERE list_id=?2 AND position=?3",params![row.comment,id,position as i64]).map_err(|e|e.to_string())?;}
            if let Some(photo)=&row.photo { photo.insert(&tx)?; }
        }
        if let Some(entry) = first_journal_entry {
            tx.execute("INSERT INTO journal_bird_lists(entry_id,list_id) VALUES (?1,?2)", params![entry,id]).map_err(|e| e.to_string())?;
        }
        // Validate content and foreign keys inside the same transaction: an invalid
        // date, position or identifier cannot leave an otherwise committed list.
        (if receipts {receipt_snapshot(&tx,&AtomicBool::new(false))} else if comments {crate::storage_content::snapshot_v9(&tx,&AtomicBool::new(false))} else if independent {crate::storage_content::snapshot_v8(&tx,&AtomicBool::new(false))} else if visits {crate::storage_content::snapshot_v7(&tx,&AtomicBool::new(false))} else {crate::storage_content::snapshot_v6(&tx, &AtomicBool::new(false))})
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
    let receipts = receipt_schema(db).map_err(|e| e.to_string())?;
    let comments =
        receipts || database::bird_comments::validate_schema(db).map_err(|e| e.to_string())?;
    let independent = comments
        || database::bird_visit_identity::validate_schema(db).map_err(|e| e.to_string())?;
    let visits =
        independent || database::bird_visits::validate_schema(db).map_err(|e| e.to_string())?;
    (if receipts {
        receipt_snapshot(db, &AtomicBool::new(false))
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
    let mut rows = db.prepare(if comments {"SELECT avibase_id,taxonomy_version,count,comment FROM bird_rows WHERE list_id=?1 ORDER BY position"}else{"SELECT avibase_id,taxonomy_version,count,NULL FROM bird_rows WHERE list_id=?1 ORDER BY position"}).map_err(|e| e.to_string())?.query_map([id], |r| Ok(SpeciesCount { row_id:None,photo:None,avibase_id:r.get(0)?, taxonomy_version:r.get(1)?,count:r.get(2)?,comment:r.get(3)?})).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    if crate::bird_photos::supported_schema(db).map_err(|e| e.to_string())? {
        let ids = db
            .prepare("SELECT id FROM bird_rows WHERE list_id=?1 ORDER BY position")
            .map_err(|e| e.to_string())?
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        for (row, row_id) in rows.iter_mut().zip(ids) {
            row.photo = crate::bird_photos::get(db, &row_id)?;
            row.row_id = Some(row_id);
        }
    }
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
    update_in_context(
        state,
        search,
        generation,
        id,
        Some(entry),
        expected,
        date,
        place,
        start,
        end,
        rows,
    )
}

pub(crate) fn update_in_context(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry: Option<&str>,
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
  if !(receipt_schema(&tx).map_err(|e|e.to_string())?||database::bird_comments::validate_schema(&tx).map_err(|e|e.to_string())?) {return Err("bird_schema_unsupported".into());}
  let previous=read_list(&tx,id)?;
  if entry.is_some_and(|entry|!previous.journal_entries.iter().any(|e|e==entry)) {return Err("bird_link_missing".into());}
  if edit_token(&previous)!=expected {return Err("bird_edit_stale".into());}
  if rows.is_empty()||rows.len()>11172 {return Err("bird_rows_invalid".into());}
  for row in rows {
   if row.count<=0||!database::bird_comments::valid_comment(row.comment.as_deref()) {return Err("bird_row_invalid".into());}
   // Retain recorded identities, including duplicates/older catalog versions.
   if !previous.rows.iter().any(|old|old.avibase_id==row.avibase_id&&old.taxonomy_version==row.taxonomy_version)
    && (row.taxonomy_version!=crate::species_registry::VERSION||crate::species_registry::get(&row.avibase_id).is_none()) {return Err("bird_species_invalid".into());}
  }
  tx.execute("UPDATE bird_lists SET visit_date=?1,place_id=?2,visit_start=?3,visit_end=?4 WHERE id=?5",params![date,place,start,end,id]).map_err(|e|e.to_string())?;
  let photos=crate::bird_photos::supported_schema(&tx).map_err(|e|e.to_string())?;
  let mut used=std::collections::HashSet::new();
  let mut row_ids=Vec::new();
  for (position,row) in rows.iter().enumerate(){
   let row_id=if let Some(row_id)=&row.row_id {
    if !photos{return Err("bird_photo_schema_unsupported".into());}
    if let Some(owner)=tx.query_row("SELECT list_id FROM bird_rows WHERE id=?1",[row_id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())? {
     if owner!=id{return Err("bird_row_owner_invalid".into());}
     let old=previous.rows.iter().find(|r|r.row_id.as_ref()==Some(row_id)).ok_or("bird_row_owner_invalid")?;
     if old.avibase_id!=row.avibase_id||old.taxonomy_version!=row.taxonomy_version{return Err("bird_row_identity_invalid".into());}
    }else{valid_row_id(row_id)?;}
    row_id.clone()
   }else{previous.rows.get(position).filter(|r|r.avibase_id==row.avibase_id&&r.taxonomy_version==row.taxonomy_version).and_then(|r|r.row_id.clone()).unwrap_or_else(||uuid::Uuid::new_v4().to_string())};
   if !used.insert(row_id.clone()){return Err("bird_row_identity_invalid".into());}
   if let Some(photo)=&row.photo {if !photos||photo.row_id!=row_id{return Err("bird_photo_owner_invalid".into());}photo.validate()?;}
   row_ids.push(row_id);
  }
  tx.execute("DELETE FROM bird_rows WHERE list_id=?1",[id]).map_err(|e|e.to_string())?;
  for (position,row) in rows.iter().enumerate() {
   tx.execute("INSERT INTO bird_rows(id,list_id,position,avibase_id,taxonomy_version,count,comment) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![row_ids[position],id,position as i64,row.avibase_id,row.taxonomy_version,row.count,row.comment]).map_err(|e|e.to_string())?;
  }
  for row in rows {if let Some(photo)=&row.photo{photo.insert(&tx)?;}}
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
        if !(receipt_schema(&tx).map_err(|e| e.to_string())?
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
        (if receipt_schema(&tx).map_err(|e| e.to_string())? {
            receipt_snapshot(&tx, &AtomicBool::new(false))
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
    (if receipt_schema(&tx).map_err(|e| e.to_string())? {
        receipt_snapshot(&tx, &AtomicBool::new(false))
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
    // A generated storage row ID is not new observation content. Photo ownership
    // remains part of the photo payload, preserving strict attachment replay checks.
    #[derive(serde::Serialize)]
    struct PayloadRow<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        photo: &'a Option<crate::bird_photos::Photo>,
        avibase_id: &'a str,
        taxonomy_version: &'a str,
        count: i64,
        comment: &'a Option<String>,
    }
    let normalized: Vec<_> = rows
        .iter()
        .map(|row| PayloadRow {
            photo: &row.photo,
            avibase_id: &row.avibase_id,
            taxonomy_version: &row.taxonomy_version,
            count: row.count,
            comment: &row.comment,
        })
        .collect();
    let bytes = serde_json::to_vec(&(date, place, normalized, entry, start, end))
        .expect("serializable request");
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
  receipt_snapshot(&tx,&AtomicBool::new(false)).map_err(|e|format!("bird_list_invalid: {e:?}"))?;
  let parsed=uuid::Uuid::parse_str(id).map_err(|_|"bird_request_invalid")?;
  if parsed.get_version_num()!=4||parsed.get_variant()!=uuid::Variant::RFC4122||parsed.to_string()!=id{return Err("bird_request_invalid".into());}
  tx.execute("INSERT INTO bird_creation_requests(request_id,payload_sha256,completed) VALUES (?1,NULL,0)",[id]).map_err(|e|e.to_string())?;
  tx.commit().map_err(|e|e.to_string())
 })
}

// Links belong to the observation list; unlinking never deletes either owner.
pub(crate) fn set_link(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry: &str,
    linked: bool,
) -> Result<(), String> {
    set_link_with_hook(state, search, generation, id, entry, linked, || Ok(()))
}
fn set_link_with_hook(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry: &str,
    linked: bool,
    after: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    search.with_write(||{
        let path=state.ready_database_path()?;
        let _permit=state.operation(Some(generation))?;
        let mut db=database::open_existing(&path)?;
        let tx=db.transaction().map_err(|e|e.to_string())?;
        if !receipt_schema(&tx).map_err(|e|e.to_string())? {return Err("bird_schema_unsupported".into());}
        read_list(&tx,id)?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=?1)",[entry],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !exists{return Err("bird_entry_missing".into());}
        if linked{
            tx.execute("INSERT INTO journal_bird_lists(entry_id,list_id) VALUES (?1,?2) ON CONFLICT(entry_id,list_id) DO NOTHING",params![entry,id]).map_err(|e|e.to_string())?;
        }else{
            tx.execute("DELETE FROM journal_bird_lists WHERE entry_id=?1 AND list_id=?2",params![entry,id]).map_err(|e|e.to_string())?;
        }
        after()?;
        read_list(&tx,id)?;
        tx.commit().map_err(|e|e.to_string())
    })
}

#[cfg(test)]
mod link_tests {
    use super::*;
    #[test]
    fn links_rollback_reject_stale_or_missing_owners_and_never_recreate_deleted_lists() {
        let root = tempfile::TempDir::new().unwrap();
        let state = DatabaseState::default();
        let path = state
            .initialize_startup(&root.path().join("journal"))
            .unwrap();
        let search = SearchState::default();
        let db = database::open_existing(&path).unwrap();
        db.execute(
            "INSERT INTO places VALUES ('11111111-1111-4111-8111-111111111111','Lake',59,18,1)",
            [],
        )
        .unwrap();
        for id in ["a", "b"] {
            db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-13','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'created','updated')",[id]).unwrap();
        }
        db.close().unwrap();
        let id = create(
            &state,
            &search,
            1,
            "2026-11-13",
            "11111111-1111-4111-8111-111111111111",
            &[SpeciesCount {
                row_id: None,
                photo: None,
                avibase_id: "avibase-FBBBA943".into(),
                taxonomy_version: "AviList v2026".into(),
                count: 1,
                comment: None,
            }],
            None,
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(
            set_link_with_hook(
                &state,
                &search,
                1,
                &id,
                "a",
                true,
                || Err("injected".into())
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(set_link(&state, &search, 999, &id, "a", true).is_err());
        assert!(set_link(&state, &search, 1, &id, "missing", true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        set_link(&state, &search, 1, &id, "a", true).unwrap();
        set_link(&state, &search, 1, &id, "b", true).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(
            set_link_with_hook(&state, &search, 1, &id, "a", false, || Err(
                "injected".into()
            ))
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        set_link(&state, &search, 1, &id, "a", false).unwrap();
        let list = reopen(&state, 1, &id).unwrap();
        assert_eq!(list.journal_entries, vec!["b"]);
        delete(&state, &search, 1, &id, &edit_token(&list), true).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(set_link(&state, &search, 1, &id, "a", true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

fn receipt_schema(db: &rusqlite::Connection) -> rusqlite::Result<bool> {
    Ok(crate::bird_photos::supported_schema(db)? || database::bird_requests::validate_schema(db)?)
}
fn receipt_snapshot(
    db: &rusqlite::Connection,
    flag: &AtomicBool,
) -> Result<crate::storage_content::Proof, crate::storage_content::Error> {
    if database::bird_captions::validate_schema(db)? {
        crate::storage_content::snapshot_v12(db, flag)
    } else if crate::bird_photos::supported_schema(db)? {
        crate::storage_content::snapshot_v11(db, flag)
    } else {
        crate::storage_content::snapshot_v10(db, flag)
    }
}
fn valid_row_id(id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "bird_row_identity_invalid")?;
    if parsed.get_version_num() != 4 || parsed.to_string() != id {
        return Err("bird_row_identity_invalid".into());
    }
    Ok(())
}
fn new_row_id(
    db: &rusqlite::Connection,
    row: &SpeciesCount,
    photos: bool,
) -> Result<String, String> {
    if !photos && (row.row_id.is_some() || row.photo.is_some()) {
        return Err("bird_photo_schema_unsupported".into());
    }
    let id = row
        .row_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    valid_row_id(&id)?;
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bird_rows WHERE id=?1)",
            [&id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        return Err("bird_row_identity_invalid".into());
    }
    if let Some(photo) = &row.photo {
        if photo.row_id != id {
            return Err("bird_photo_owner_invalid".into());
        }
        photo.validate()?;
    }
    Ok(id)
}
