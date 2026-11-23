//! Server-owned one-use reviews. No persistent IDs are reserved during review.
use crate::{
    bird_module::BirdModule,
    database_state::DatabaseState,
    ebird_persistence::{self as store, Fields, InputVisit, Record},
    journal_search::SearchState,
};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Change {
    pub submission_id: String,
    pub kind: String,
    pub fields: Fields,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Comparison {
    pub operation_id: String,
    pub generation: u32,
    pub changes: Vec<Change>,
}
struct Review {
    input: Vec<InputVisit>,
    before: String,
    generation: u32,
    expires: Instant,
    status: String,
    after: Option<String>,
}
#[derive(Default)]
pub(crate) struct ImportState(Mutex<BTreeMap<String, Review>>);
impl ImportState {
    pub(crate) fn invalidate(&self) -> Result<()> {
        let mut slots = self.0.lock().map_err(|_| "ebird_review_failed")?;
        slots.retain(|_, r| r.status != "ready");
        Ok(())
    }
}
fn sql(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn validate(input: &[InputVisit]) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    let mut rows = 0;
    if input.is_empty() || serde_json::to_vec(input).map_err(sql)?.len() > 16 * 1024 * 1024 {
        return Err("ebird_input_invalid".into());
    }
    for visit in input {
        store::validate_visit(visit)?;
        rows += visit.rows.len();
        if rows > 100000 || !ids.insert(&visit.submission_id) {
            return Err("ebird_input_invalid".into());
        }
    }
    Ok(())
}
fn compare(db: &Connection, input: &[InputVisit]) -> Result<Vec<Change>> {
    let records = store::records(db)?;
    let mut changes = Vec::new();
    for visit in input {
        let current = records.iter().find_map(|r| {
            if let Record::Visit {
                id, submission_id, ..
            } = r
            {
                (submission_id == &visit.submission_id).then_some(id)
            } else {
                None
            }
        });
        let kind = if let Some(id) = current {
            let mut rows = records
                .iter()
                .filter_map(|r| {
                    if let Record::Row {
                        visit_id,
                        position,
                        active: true,
                        source_json,
                        ..
                    } = r
                    {
                        (visit_id == id).then_some((*position, source_json))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            rows.sort_by_key(|r| r.0);
            if rows.iter().map(|r| r.1.as_str()).eq(visit
                .rows
                .iter()
                .map(|f| serde_json::to_string(f).unwrap())
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str))
            {
                "unchanged"
            } else {
                "changed"
            }
        } else {
            "new"
        };
        changes.push(Change {
            submission_id: visit.submission_id.clone(),
            kind: kind.into(),
            fields: visit.rows[0].clone(),
        });
    }
    for record in records {
        if let Record::Visit {
            submission_id,
            source_json,
            ..
        } = record
        {
            if !input.iter().any(|v| v.submission_id == submission_id) {
                changes.push(Change {
                    submission_id,
                    kind: "retained".into(),
                    fields: serde_json::from_str(&source_json).map_err(sql)?,
                });
            }
        }
    }
    Ok(changes)
}
pub(crate) fn prepare(
    state: &DatabaseState,
    module: &BirdModule,
    slots: &ImportState,
    generation: u32,
    input: Vec<InputVisit>,
) -> Result<Comparison> {
    let _module = module.command()?;
    validate(&input)?;
    let path = state.ready_database_path()?;
    let _operation = state.operation(Some(generation))?;
    let mut db = crate::database::open_existing(&path)?;
    let tx = db.transaction().map_err(sql)?;
    crate::storage_content::snapshot_v14(&tx, &AtomicBool::new(false))
        .map_err(|_| "ebird_review_failed")?;
    let before = store::fingerprint(&tx)?;
    let changes = compare(&tx, &input)?;
    tx.commit().map_err(sql)?;
    drop(_operation);
    let operation_id = uuid::Uuid::new_v4().to_string();
    let mut reviews = slots.0.lock().map_err(|_| "ebird_review_failed")?;
    reviews.retain(|_, r| r.expires > Instant::now());
    if reviews.len() >= 32 {
        return Err("ebird_review_capacity".into());
    }
    reviews.insert(
        operation_id.clone(),
        Review {
            input,
            before,
            generation,
            expires: Instant::now() + Duration::from_secs(900),
            status: "ready".into(),
            after: None,
        },
    );
    Ok(Comparison {
        operation_id,
        generation,
        changes,
    })
}
pub(crate) fn confirm(
    state: &DatabaseState,
    module: &BirdModule,
    search: &SearchState,
    slots: &ImportState,
    generation: u32,
    id: &str,
) -> Result<String> {
    {
        let _module = module.command()?;
    }
    let (input, before) = {
        let mut reviews = slots.0.lock().map_err(|_| "ebird_review_failed")?;
        let review = reviews.get_mut(id).ok_or("ebird_review_expired")?;
        if review.generation != generation || review.expires <= Instant::now() {
            return Err("ebird_review_expired".into());
        }
        if review.status != "ready" {
            return Err("ebird_check_status".into());
        }
        review.status = "uncertain".into();
        (review.input.clone(), review.before.clone())
    };
    let mut after = None;
    let result = store::import_bound(
        state,
        search,
        module,
        generation,
        &input,
        Some(&before),
        |db| {
            after = Some(store::fingerprint(db)?);
            Ok(())
        },
    );
    let mut reviews = slots.0.lock().map_err(|_| "ebird_check_status")?;
    if let Some(review) = reviews.get_mut(id) {
        review.after = after;
        if result.is_ok() {
            review.status = "committed".into();
        }
    }
    result.map(|_| "committed".into())
}
pub(crate) fn status(
    state: &DatabaseState,
    module: &BirdModule,
    slots: &ImportState,
    generation: u32,
    id: &str,
) -> Result<String> {
    let _module = module.command()?;
    let path = state.ready_database_path()?;
    let mut slots = slots.0.lock().map_err(|_| "ebird_review_failed")?;
    let _operation = state.operation(Some(generation))?;
    let Some(r) = slots.get_mut(id) else {
        return Ok("unknown".into());
    };
    if r.generation != generation {
        return Err("ebird_review_stale".into());
    }
    if r.status == "uncertain" {
        let mut db = crate::database::open_existing(&path)?;
        let tx = db.transaction().map_err(sql)?;
        crate::storage_content::snapshot_v14(&tx, &AtomicBool::new(false))
            .map_err(|_| "ebird_review_failed")?;
        if r.after
            .as_ref()
            .is_some_and(|hash| store::fingerprint(&tx).as_ref() == Ok(hash))
        {
            r.status = "committed".into();
        }
        tx.commit().map_err(sql)?;
    }
    Ok(r.status.clone())
}
#[tauri::command]
pub(crate) fn review_ebird_import(
    state: tauri::State<'_, DatabaseState>,
    module: tauri::State<'_, BirdModule>,
    slots: tauri::State<'_, ImportState>,
    generation: u32,
    visits: Vec<InputVisit>,
) -> Result<Comparison> {
    prepare(&state, &module, &slots, generation, visits)
}
#[tauri::command]
pub(crate) fn confirm_ebird_import(
    state: tauri::State<'_, DatabaseState>,
    module: tauri::State<'_, BirdModule>,
    search: tauri::State<'_, SearchState>,
    slots: tauri::State<'_, ImportState>,
    generation: u32,
    operation_id: String,
) -> Result<String> {
    confirm(&state, &module, &search, &slots, generation, &operation_id)
}
#[tauri::command]
pub(crate) fn ebird_import_status(
    state: tauri::State<'_, DatabaseState>,
    module: tauri::State<'_, BirdModule>,
    slots: tauri::State<'_, ImportState>,
    generation: u32,
    operation_id: String,
) -> Result<String> {
    status(&state, &module, &slots, generation, &operation_id)
}
#[tauri::command]
pub(crate) fn cancel_ebird_import(
    module: tauri::State<'_, BirdModule>,
    slots: tauri::State<'_, ImportState>,
    operation_id: String,
) -> Result<()> {
    let _module = module.command()?;
    let mut slots = slots.0.lock().map_err(|_| "ebird_review_failed")?;
    if slots
        .get(&operation_id)
        .is_some_and(|r| r.status == "ready")
    {
        slots.remove(&operation_id);
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
