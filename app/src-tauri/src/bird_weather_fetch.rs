//! Volatile, server-owned daily previews. No command here writes observation data.
use crate::{
    bird_visit_weather::{Context, Record},
    database_state::{DatabaseState, Snapshot},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(10 * 60);
const CAPACITY: usize = 16;
#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub owner_id: String,
    pub expected_token: Option<String>,
    pub context: Context,
}
#[derive(Clone)]
pub(crate) struct Receipt {
    pub record: Record,
    request: Request,
    generation: u32,
    place_revision: String,
    expires: Instant,
    today_forecast: bool,
}
#[derive(Default)]
struct Slots {
    pending: HashMap<(u32, String), String>,
    ready: HashMap<String, Receipt>,
    cancelled: VecDeque<String>,
}
#[derive(Default)]
pub(crate) struct PreviewState(Mutex<Slots>);
impl PreviewState {
    pub(crate) fn invalidate(&self) -> Result<(), String> {
        *self.0.lock().map_err(|_| "bird_weather_preview_expired")? = Slots::default();
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preview {
    preview_id: String,
    pub record: Record,
    today_forecast: bool,
}
const CONFLICT: &str = "bird_weather_context_conflict";
const EXPIRED: &str = "bird_weather_preview_expired";

#[cfg(target_os = "macos")]
fn binding(db: &rusqlite::Connection, request: &Request) -> Result<String, String> {
    request.context.validate()?;
    if !crate::database::bird_visit_weather::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("bird_schema_unsupported".into());
    }
    let place = crate::place_registry::read_place(db, &request.context.place_id)?;
    if place.name != request.context.place_name
        || place.latitude != Some(request.context.latitude)
        || place.longitude != Some(request.context.longitude)
    {
        return Err(CONFLICT.into());
    }
    if let Some(expected) = &request.expected_token {
        let list = crate::bird_observations::read_list(db, &request.owner_id)?;
        if crate::bird_observations::edit_token(&list) != *expected {
            return Err(CONFLICT.into());
        }
    } else {
        let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM bird_creation_requests WHERE request_id=?1 AND completed=0)",[&request.owner_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !pending {
            return Err(CONFLICT.into());
        }
    }
    Ok(place.revision)
}
impl Receipt {
    /// Runs again inside the normal list transaction, before changing its owner.
    #[cfg(target_os = "macos")]
    pub(crate) fn check(&self, db: &rusqlite::Connection) -> Result<(), String> {
        if Instant::now() >= self.expires {
            return Err(EXPIRED.into());
        }
        if binding(db, &self.request)? != self.place_revision {
            return Err(CONFLICT.into());
        }
        Ok(())
    }
}
impl PreviewState {
    pub(crate) fn resolve(
        &self,
        id: &str,
        generation: u32,
        owner: &str,
        expected: Option<&str>,
        date: &str,
        place: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<Receipt, String> {
        let slots = self.0.lock().map_err(|_| EXPIRED)?;
        let r = slots.ready.get(id).ok_or(EXPIRED)?;
        let c = &r.request.context;
        if r.generation != generation
            || r.request.owner_id != owner
            || r.request.expected_token.as_deref() != expected
            || c.date != date
            || c.place_id != place
            || c.start.as_deref() != start
            || c.end.as_deref() != end
        {
            return Err(CONFLICT.into());
        }
        // Expiry is checked in the write transaction. A completed creation retry
        // can still resolve the original payload for its durable receipt check.
        Ok(r.clone())
    }
    fn cancel(&self, id: &str) -> Result<(), String> {
        let mut slots = self.0.lock().map_err(|_| EXPIRED)?;
        slots.pending.retain(|_, v| v != id);
        slots.ready.remove(id);
        // Cancellation may reach the synchronous handler before spawn_blocking
        // starts the request. Retain bounded volatile nonce tombstones for that race.
        if !slots.cancelled.iter().any(|v| v == id) {
            if slots.cancelled.len() >= CAPACITY * 4 {
                slots.cancelled.pop_front();
            }
            slots.cancelled.push_back(id.to_owned());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn preview_with(
    state: &DatabaseState,
    previews: &PreviewState,
    generation: u32,
    id: String,
    request: Request,
    now: time::OffsetDateTime,
    transport: impl FnMut(reqwest::Url) -> Result<(u16, Vec<u8>), crate::weather::historical::Error>,
) -> Result<Snapshot<Preview>, String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|_| EXPIRED)?;
    if uuid.get_version_num() != 4 || uuid.to_string() != id {
        return Err(EXPIRED.into());
    }
    let path = state.ready_database_path()?;
    let key = (generation, request.owner_id.clone());
    let revision = {
        let _permit = state.operation(Some(generation))?;
        let mut db = crate::database::open_existing(&path)?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        binding(&tx, &request)?
    };
    {
        let mut slots = previews.0.lock().map_err(|_| EXPIRED)?;
        if slots.ready.contains_key(&id)
            || slots.pending.values().any(|v| v == &id)
            || slots.cancelled.contains(&id)
        {
            return Err(EXPIRED.into());
        }
        if slots.pending.len() >= CAPACITY {
            return Err(EXPIRED.into());
        }
        slots.pending.insert(key.clone(), id.clone());
    }
    let expires = Instant::now() + TTL;
    let result = (|| {
        let (payload, today) =
            crate::weather::historical::daily_visit_with(&request.context, now, transport)
                .map_err(|e| {
                    serde_json::to_value(e)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })?;
        let permit = state.operation(Some(generation))?;
        let mut db = crate::database::open_existing(&path)?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        if binding(&tx, &request)? != revision {
            return Err(CONFLICT.into());
        }
        let record = Record::new(&request.owner_id, "fetched", &request.context, &payload)?;
        let mut slots = previews.0.lock().map_err(|_| EXPIRED)?;
        if Instant::now() >= expires || slots.pending.get(&key) != Some(&id) {
            return Err(EXPIRED.into());
        }
        if slots.ready.len() >= CAPACITY {
            let oldest = slots
                .ready
                .iter()
                .min_by_key(|(_, v)| v.expires)
                .map(|(k, _)| k.clone())
                .unwrap();
            slots.ready.remove(&oldest);
        }
        slots.ready.insert(
            id.clone(),
            Receipt {
                record: record.clone(),
                request: request.clone(),
                generation,
                place_revision: revision,
                expires,
                today_forecast: today,
            },
        );
        Ok(permit.snapshot(Preview {
            preview_id: id.clone(),
            record,
            today_forecast: today,
        }))
    })();
    if let Ok(mut slots) = previews.0.lock() {
        if slots.pending.get(&key) == Some(&id) {
            slots.pending.remove(&key);
        }
    }
    result
}

#[tauri::command]
pub(crate) async fn preview_bird_visit_weather(
    app: tauri::AppHandle,
    generation: u32,
    preview_id: String,
    request: Request,
) -> Result<Snapshot<Preview>, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn_blocking(move || {
            let module = app.state::<crate::bird_module::BirdModule>();
            let _module_permit = module.command()?;
            preview_with(
                &app.state::<DatabaseState>(),
                &app.state::<PreviewState>(),
                generation,
                preview_id,
                request,
                time::OffsetDateTime::now_utc(),
                crate::weather::historical::visit_transport,
            )
        })
        .await
        .map_err(|_| EXPIRED.to_owned())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, generation, preview_id, request);
        Err("bird_platform_unsupported".into())
    }
}
#[tauri::command]
pub(crate) fn review_bird_visit_weather(
    module: tauri::State<'_, crate::bird_module::BirdModule>,
    state: tauri::State<'_, DatabaseState>,
    previews: tauri::State<'_, PreviewState>,
    generation: u32,
    preview_id: String,
    request: Request,
) -> Result<Snapshot<Preview>, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        review(&state, &previews, generation, &preview_id, &request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, previews, generation, preview_id, request);
        Err("bird_platform_unsupported".into())
    }
}
#[cfg(target_os = "macos")]
pub(crate) fn review(
    state: &DatabaseState,
    previews: &PreviewState,
    generation: u32,
    id: &str,
    request: &Request,
) -> Result<Snapshot<Preview>, String> {
    let path = state.ready_database_path()?;
    let permit = state.operation(Some(generation))?;
    let receipt = previews.resolve(
        id,
        generation,
        &request.owner_id,
        request.expected_token.as_deref(),
        &request.context.date,
        &request.context.place_id,
        request.context.start.as_deref(),
        request.context.end.as_deref(),
    )?;
    if receipt.request != *request {
        return Err(CONFLICT.into());
    }
    let mut db = crate::database::open_existing(&path)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    receipt.check(&tx)?;
    Ok(permit.snapshot(Preview {
        preview_id: id.into(),
        record: receipt.record,
        today_forecast: receipt.today_forecast,
    }))
}
#[tauri::command]
pub(crate) fn cancel_bird_visit_weather(
    module: tauri::State<'_, crate::bird_module::BirdModule>,
    previews: tauri::State<'_, PreviewState>,
    preview_id: String,
) -> Result<(), String> {
    let _module_permit = module.command()?;
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    previews.cancel(&preview_id)
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
