//! One volatile preview, never serialized to disk. The client cannot supply saved weather.
use super::*;
use crate::{place_registry as places, weather::storage};
use rusqlite::{Connection, TransactionBehavior, params};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum PlaceSource {
    Journal,
    Default,
    Saved { id: String },
}

#[derive(PartialEq)]
struct Binding {
    date: String,
    journal_place: places::JournalPlace,
    source: serde_json::Value,
    weather_revision: i64,
}

struct Receipt {
    entry: String,
    generation: u32,
    source: PlaceSource,
    binding: Binding,
    fetched_json: String,
}

#[derive(Default)]
pub(crate) struct PreviewState(Mutex<Option<(String, Option<Receipt>)>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoundPreview {
    preview_id: String,
    entry_id: String,
    weather_revision: String,
    fetched: FetchedWeather,
}

fn binding(
    db: &Connection,
    entry: &str,
    source: &PlaceSource,
) -> Result<(Binding, PreviewRequest), String> {
    let weather = storage::read(db, entry)?;
    let (date, document, version): (String, String, i64) = db
        .query_row(
            "SELECT entry_date,content_json,document_version FROM journal_entries WHERE id=?1",
            [entry],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "weather_entry_missing")?;
    if !crate::photographs::document_editable(db, entry, version, &document) {
        return Err("journal_document_incompatible".into());
    }
    let link = places::read_link(db, entry)?;
    let (name, latitude, longitude, value) = match source {
        PlaceSource::Journal => {
            let p = link
                .place
                .as_ref()
                .ok_or("historical_weather_place_missing")?;
            // Also bind the register revision; historical snapshots themselves stay unchanged.
            let current = places::read_place(db, &p.id)?;
            (
                p.name.clone(),
                p.latitude,
                p.longitude,
                serde_json::to_value(current),
            )
        }
        PlaceSource::Default => {
            let setting = places::read_weather_default_row(db)?;
            let p = setting
                .place
                .as_ref()
                .ok_or("historical_weather_place_missing")?;
            let current = places::read_place(db, &p.id)?;
            (
                p.name.clone(),
                Some(p.latitude),
                Some(p.longitude),
                serde_json::to_value((&setting, current)),
            )
        }
        PlaceSource::Saved { id } => {
            let p = places::read_place(db, id)?;
            (
                p.name.clone(),
                p.latitude,
                p.longitude,
                serde_json::to_value(p),
            )
        }
    };
    let request = PreviewRequest {
        date: date.clone(),
        place_name: name,
        latitude: latitude.ok_or("historical_weather_coordinates_missing")?,
        longitude: longitude.ok_or("historical_weather_coordinates_missing")?,
    };
    Ok((
        Binding {
            date,
            journal_place: link,
            source: value.map_err(|_| "historical_weather_invalid_place")?,
            weather_revision: weather.revision,
        },
        request,
    ))
}

fn preview_bound(
    state: &DatabaseState,
    previews: &PreviewState,
    generation: u32,
    mut open: impl FnMut() -> Result<Connection, String>,
    entry: &str,
    source: PlaceSource,
    now: OffsetDateTime,
    transport: impl FnMut(reqwest::Url) -> Result<(u16, Vec<u8>), Error>,
) -> Result<Snapshot<BoundPreview>, String> {
    let id = uuid::Uuid::new_v4().to_string();
    *previews
        .0
        .lock()
        .map_err(|_| "historical_weather_preview_unavailable")? = Some((id.clone(), None));
    let (original, request) = {
        let _operation = state.operation(Some(generation))?;
        let mut db = open()?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        binding(&tx, entry, &source)?
    };
    let preview = super::recent::preview(&request, now, transport).map_err(|e| {
        serde_json::to_value(e)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    })?;
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    if binding(&tx, entry, &source)?.0 != original {
        return Err("historical_weather_context_conflict".into());
    }
    drop(tx);
    drop(db);
    let mut slot = previews
        .0
        .lock()
        .map_err(|_| "historical_weather_preview_unavailable")?;
    if slot.as_ref().map(|s| s.0.as_str()) != Some(id.as_str()) {
        return Err("historical_weather_preview_expired".into());
    }
    let fetched_json = serde_json::to_string(&preview.fetched).map_err(|_| "invalid_response")?;
    let revision = original.weather_revision;
    *slot = Some((
        id.clone(),
        Some(Receipt {
            entry: entry.into(),
            generation,
            source,
            binding: original,
            fetched_json,
        }),
    ));
    Ok(operation.snapshot(BoundPreview {
        preview_id: id,
        entry_id: entry.into(),
        weather_revision: revision.to_string(),
        fetched: preview.fetched,
    }))
}

fn save(
    state: &DatabaseState,
    previews: &PreviewState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry: &str,
    preview_id: &str,
    expected_revision: i64,
    replace_confirmed: bool,
) -> Result<Snapshot<storage::StoredWeather>, String> {
    let operation = state.operation(Some(generation))?;
    let mut slot = previews
        .0
        .lock()
        .map_err(|_| "historical_weather_preview_unavailable")?;
    let (id, receipt) = slot.as_ref().ok_or("historical_weather_preview_expired")?;
    let receipt = receipt
        .as_ref()
        .ok_or("historical_weather_preview_expired")?;
    if id != preview_id || receipt.entry != entry || receipt.generation != generation {
        return Err("historical_weather_preview_conflict".into());
    }
    let mut db = open()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let current = binding(&tx, entry, &receipt.source)?.0;
    if current.weather_revision != expected_revision
        || expected_revision != receipt.binding.weather_revision
    {
        return Err("weather_revision_conflict".into());
    }
    if current != receipt.binding {
        return Err("historical_weather_context_conflict".into());
    }
    let mut weather = storage::read(&tx, entry)?;
    if weather.fetched_json.is_some() && !replace_confirmed {
        return Err("historical_weather_replace_confirmation_required".into());
    }
    storage::validate(
        entry,
        weather.manual_json.as_deref(),
        Some(&receipt.fetched_json),
    )?;
    let fetched: FetchedWeather = serde_json::from_str(&receipt.fetched_json)
        .map_err(|_| "historical_weather_invalid_preview")?;
    if fetched.temperature_mean_c.is_none()
        && fetched.temperature_min_c.is_none()
        && fetched.temperature_max_c.is_none()
        && fetched.precipitation_sum_mm.is_none()
        && fetched.wind_speed_max_ms.is_none()
        && fetched.weather_code.is_none()
        && fetched.temperature_c.is_none()
        && fetched.wind_speed_ms.is_none()
        && fetched.precipitation_mm.is_none()
        && fetched.relative_humidity_percent.is_none()
    {
        return Err("historical_weather_empty_preview".into());
    }
    weather.revision = weather
        .revision
        .checked_add(1)
        .ok_or("weather_revision_conflict")?;
    tx.execute("INSERT INTO journal_weather(entry_id,manual_json,fetched_json,revision,format_version) VALUES (?1,?2,?3,?4,1)
        ON CONFLICT(entry_id) DO UPDATE SET fetched_json=excluded.fetched_json,revision=excluded.revision",
        params![entry, weather.manual_json, receipt.fetched_json, weather.revision]).map_err(|e| e.to_string())?;
    weather.fetched_json = Some(receipt.fetched_json.clone());
    tx.commit().map_err(|e| e.to_string())?;
    *slot = None;
    Ok(operation.snapshot(weather))
}

#[tauri::command]
pub(crate) async fn preview_historical_weather(
    app: tauri::AppHandle,
    generation: u32,
    entry_id: String,
    source: PlaceSource,
) -> Result<Snapshot<BoundPreview>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        preview_bound(
            &app.state::<DatabaseState>(),
            &app.state::<PreviewState>(),
            generation,
            || {
                Connection::open_with_flags(
                    crate::database::database_path(&app)?,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .map_err(|e| e.to_string())
            },
            &entry_id,
            source,
            OffsetDateTime::now_utc(),
            |url| get(&client(Duration::from_secs(15))?, url),
        )
    })
    .await
    .map_err(|_| "historical_weather_preview_unavailable".to_owned())?
}

#[tauri::command]
pub(crate) async fn save_historical_weather(
    app: tauri::AppHandle,
    generation: u32,
    entry_id: String,
    preview_id: String,
    expected_revision: String,
    replace_confirmed: bool,
) -> Result<Snapshot<SavedWeather>, String> {
    use tauri::Manager;
    let revision = super::super::view::parse_revision(&expected_revision)?;
    tauri::async_runtime::spawn_blocking(move || {
        let saved = save(
            &app.state::<DatabaseState>(),
            &app.state::<PreviewState>(),
            generation,
            || crate::database::open_database(&app),
            &entry_id,
            &preview_id,
            revision,
            replace_confirmed,
        )?;
        // No fallible post-write session check: commit success is definitive.
        Ok(Snapshot {
            generation: saved.generation,
            data: SavedWeather {
                revision: saved.data.revision.to_string(),
            },
        })
    })
    .await
    .map_err(|_| "historical_weather_save_unavailable".to_owned())?
}

#[derive(Serialize)]
pub(crate) struct SavedWeather {
    revision: String,
}

#[cfg(test)]
mod tests;
