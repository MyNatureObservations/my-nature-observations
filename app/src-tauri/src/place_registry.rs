//! Schema-3 place-register operations shared by journals, settings and future modules.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

const NAME_LIMIT: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Place {
    pub id: String,
    pub name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Decimal string keeps all SQLite i64 revisions exact in JavaScript.
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlaceSnapshot {
    pub id: String,
    pub name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub place_revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct JournalPlace {
    pub place: Option<PlaceSnapshot>,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WeatherDefaultPlaceValue {
    pub id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WeatherDefaultPlace {
    pub place: Option<WeatherDefaultPlaceValue>,
    /// Decimal string keeps all SQLite i64 revisions exact in JavaScript.
    pub revision: String,
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > NAME_LIMIT {
        return Err("place_name_invalid".into());
    }
    Ok(())
}

fn validate_coordinates(latitude: Option<f64>, longitude: Option<f64>) -> Result<(), String> {
    match (latitude, longitude) {
        (None, None) => Ok(()),
        (Some(lat), Some(lon))
            if lat.is_finite()
                && lon.is_finite()
                && (-90.0..=90.0).contains(&lat)
                && (-180.0..=180.0).contains(&lon) =>
        {
            Ok(())
        }
        _ => Err("place_coordinates_invalid".into()),
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::from_str(id).map_err(|_| "place_id_invalid")?;
    if parsed.to_string() != id || parsed.get_variant() != uuid::Variant::RFC4122 {
        return Err("place_id_invalid".into());
    }
    Ok(())
}

fn parse_revision(value: &str, allow_zero: bool) -> Result<i64, String> {
    if value.is_empty()
        || value.len() > 19
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err("place_revision_invalid".into());
    }
    let revision = value.parse::<i64>().map_err(|_| "place_revision_invalid")?;
    if revision < if allow_zero { 0 } else { 1 } {
        return Err("place_revision_invalid".into());
    }
    Ok(revision)
}

fn require_schema3(db: &Connection) -> Result<(), String> {
    let version: i64 = db
        .pragma_query_value(Some("main"), "user_version", |r| r.get(0))
        .map_err(|_| "place_schema_unavailable")?;
    if !matches!(version, 3 | 4 | 5) {
        return Err("place_schema_unavailable".into());
    }
    if !(if version == 5 {
        crate::database::bird_schema::validate_schema(db)
    } else if version == 4 {
        crate::photographs::validate_schema(db)
    } else {
        crate::database::weather_default_schema::validate_schema3(db)
    })
    .map_err(|_| "place_schema_invalid")?
    {
        return Err("place_schema_invalid".into());
    }
    let foreign_keys: i64 = db
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .map_err(|_| "place_integrity_unavailable")?;
    if foreign_keys != 1 {
        return Err("place_integrity_unavailable".into());
    }
    Ok(())
}

pub(crate) fn read_place(db: &Connection, id: &str) -> Result<Place, String> {
    let row = db
        .query_row(
            "SELECT id,name,latitude,longitude,revision FROM main.places WHERE id=?1",
            [id],
            |r| {
                Ok(Place {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    latitude: r.get(2)?,
                    longitude: r.get(3)?,
                    revision: r.get::<_, i64>(4)?.to_string(),
                })
            },
        )
        .optional()
        .map_err(|_| "place_read_failed")?
        .ok_or("place_not_found")?;
    validate_id(&row.id)?;
    validate_name(&row.name)?;
    validate_coordinates(row.latitude, row.longitude)?;
    if parse_revision(&row.revision, false).is_err() {
        return Err("place_data_invalid".into());
    }
    Ok(row)
}

pub(crate) fn read_weather_default_row(db: &Connection) -> Result<WeatherDefaultPlace, String> {
    let row: Option<(Option<String>, i64)> = db
        .query_row(
            "SELECT place_id,revision FROM main.weather_default_place WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "weather_default_place_read_failed")?;
    let Some((place_id, revision)) = row else {
        return Ok(WeatherDefaultPlace {
            place: None,
            revision: "0".into(),
        });
    };
    if revision < 1 {
        return Err("weather_default_place_data_invalid".into());
    }
    let place = if let Some(id) = place_id {
        validate_id(&id).map_err(|_| "weather_default_place_data_invalid")?;
        let current = read_place(db, &id).map_err(|_| "weather_default_place_data_invalid")?;
        let (Some(latitude), Some(longitude)) = (current.latitude, current.longitude) else {
            return Err("weather_default_place_data_invalid".into());
        };
        Some(WeatherDefaultPlaceValue {
            id: current.id,
            name: current.name,
            latitude,
            longitude,
        })
    } else {
        None
    };
    Ok(WeatherDefaultPlace {
        place,
        revision: revision.to_string(),
    })
}

pub(crate) fn read_weather_default_place_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
) -> Result<crate::database_state::Snapshot<WeatherDefaultPlace>, String> {
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction()
        .map_err(|_| "weather_default_place_read_failed")?;
    let setting = read_weather_default_row(&tx)?;
    tx.commit()
        .map_err(|_| "weather_default_place_read_failed")?;
    Ok(operation.snapshot(setting))
}

pub(crate) fn set_weather_default_place_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    expected_revision: i64,
    place_id: Option<&str>,
) -> Result<crate::database_state::Snapshot<WeatherDefaultPlace>, String> {
    if expected_revision < 0 {
        return Err("place_revision_invalid".into());
    }
    if let Some(id) = place_id {
        validate_id(id)?;
    }
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "weather_default_place_write_failed")?;
    let current = read_weather_default_row(&tx)?;
    let revision = parse_revision(&current.revision, true)?;
    if revision != expected_revision {
        return Err("place_revision_conflict".into());
    }
    if let Some(id) = place_id {
        let place = read_place(&tx, id)?;
        if place.latitude.is_none() || place.longitude.is_none() {
            return Err("weather_default_place_coordinates_required".into());
        }
    }
    let next = revision.checked_add(1).ok_or("place_revision_overflow")?;
    tx.execute(
        "INSERT INTO main.weather_default_place(singleton,place_id,revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET place_id=excluded.place_id,revision=excluded.revision",
        params![place_id, next],
    )
    .map_err(|_| "weather_default_place_write_failed")?;
    let saved = read_weather_default_row(&tx)?;
    tx.commit()
        .map_err(|_| "weather_default_place_write_failed")?;
    Ok(operation.snapshot(saved))
}

pub(crate) fn create_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    validate_name(&name)?;
    validate_coordinates(latitude, longitude)?;
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "place_write_failed")?;
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO main.places(id,name,latitude,longitude,revision) VALUES (?1,?2,?3,?4,1)",
        params![id, name, latitude, longitude],
    )
    .map_err(|_| "place_write_failed")?;
    let place = read_place(&tx, &id)?;
    tx.commit().map_err(|_| "place_write_failed")?;
    Ok(operation.snapshot(place))
}

pub(crate) fn list_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
) -> Result<crate::database_state::Snapshot<Vec<Place>>, String> {
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db.transaction().map_err(|_| "place_read_failed")?;
    let ids = {
        let mut statement = tx
            .prepare("SELECT id FROM main.places ORDER BY name COLLATE NOCASE,id COLLATE BINARY")
            .map_err(|_| "place_read_failed")?;
        statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| "place_read_failed")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| "place_read_failed")?
    };
    let mut places = Vec::with_capacity(ids.len());
    for id in ids {
        places.push(read_place(&tx, &id)?);
    }
    tx.commit().map_err(|_| "place_read_failed")?;
    Ok(operation.snapshot(places))
}

pub(crate) fn get_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    id: &str,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    validate_id(id)?;
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db.transaction().map_err(|_| "place_read_failed")?;
    let place = read_place(&tx, id)?;
    tx.commit().map_err(|_| "place_read_failed")?;
    Ok(operation.snapshot(place))
}

pub(crate) fn update_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    id: &str,
    expected_revision: i64,
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    validate_id(id)?;
    validate_name(&name)?;
    validate_coordinates(latitude, longitude)?;
    if expected_revision < 1 {
        return Err("place_revision_invalid".into());
    }
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "place_write_failed")?;
    let current = read_place(&tx, id)?;
    let revision = parse_revision(&current.revision, false)?;
    if revision != expected_revision {
        return Err("place_revision_conflict".into());
    }
    if latitude.is_none() {
        let is_default: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM main.weather_default_place WHERE singleton=1 AND place_id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|_| "place_read_failed")?;
        if is_default {
            return Err("place_coordinates_required_by_weather_default".into());
        }
    }
    let next = revision.checked_add(1).ok_or("place_revision_overflow")?;
    let changed = tx
        .execute(
            "UPDATE main.places SET name=?1,latitude=?2,longitude=?3,revision=?4 WHERE id=?5 AND revision=?6",
            params![name, latitude, longitude, next, id, revision],
        )
        .map_err(|_| "place_write_failed")?;
    if changed != 1 {
        return Err("place_revision_conflict".into());
    }
    let saved = read_place(&tx, id)?;
    tx.commit().map_err(|_| "place_write_failed")?;
    Ok(operation.snapshot(saved))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaceUsageEntry {
    pub id: String,
    pub entry_date: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaceUsage {
    pub place_id: String,
    pub is_weather_default: bool,
    pub total_count: usize,
    pub entries: Vec<PlaceUsageEntry>,
}

pub(crate) fn read_place_usage_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    place_id: &str,
) -> Result<crate::database_state::Snapshot<PlaceUsage>, String> {
    validate_id(place_id)?;
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db.transaction().map_err(|_| "place_usage_read_failed")?;
    read_place(&tx, place_id)?;
    let entries = {
        let mut statement = tx.prepare(
            "SELECT j.id,j.entry_date,j.title FROM main.journal_places p JOIN main.journal_entries j ON j.id=p.entry_id WHERE p.place_id=?1 ORDER BY j.entry_date DESC,j.created_at DESC,j.id DESC"
        ).map_err(|_| "place_usage_read_failed")?;
        statement
            .query_map([place_id], |row| {
                Ok(PlaceUsageEntry {
                    id: row.get(0)?,
                    entry_date: row.get(1)?,
                    title: row.get(2)?,
                })
            })
            .map_err(|_| "place_usage_read_failed")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "place_usage_read_failed")?
    };
    let is_weather_default = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.weather_default_place WHERE singleton=1 AND place_id=?1)",
        [place_id], |row| row.get(0),
    ).map_err(|_| "place_usage_read_failed")?;
    let value = PlaceUsage {
        place_id: place_id.into(),
        is_weather_default,
        total_count: entries.len(),
        entries,
    };
    tx.commit().map_err(|_| "place_usage_read_failed")?;
    Ok(operation.snapshot(value))
}

#[tauri::command]
pub(crate) fn read_place_usage(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    place_id: String,
) -> Result<crate::database_state::Snapshot<PlaceUsage>, String> {
    read_place_usage_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &place_id,
    )
}

pub(crate) fn delete_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    id: &str,
) -> Result<crate::database_state::Snapshot<()>, String> {
    validate_id(id)?;
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "place_write_failed")?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.places WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "place_read_failed")?;
    if !exists {
        return Err("place_not_found".into());
    }
    let referenced: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.journal_places WHERE place_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "place_read_failed")?;
    if referenced {
        return Err("place_in_use".into());
    }
    let weather_default: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.weather_default_place WHERE singleton=1 AND place_id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| "place_read_failed")?;
    if weather_default {
        return Err("place_in_use".into());
    }
    let changed = tx
        .execute("DELETE FROM main.places WHERE id=?1", [id])
        .map_err(|_| "place_write_failed")?;
    if changed != 1 {
        return Err("place_not_found".into());
    }
    tx.commit().map_err(|_| "place_write_failed")?;
    Ok(operation.snapshot(()))
}

pub(crate) fn read_link(tx: &Connection, entry_id: &str) -> Result<JournalPlace, String> {
    let row: Option<(Option<String>, Option<String>, Option<f64>, Option<f64>, Option<i64>, i64)> =
        tx.query_row(
            "SELECT place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision FROM main.journal_places WHERE entry_id=?1",
            [entry_id],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)),
        )
        .optional()
        .map_err(|_| "journal_place_read_failed")?;
    let Some((place_id, name, latitude, longitude, place_revision, revision)) = row else {
        return Ok(JournalPlace {
            place: None,
            revision: "0".into(),
        });
    };
    if revision < 1 {
        return Err("journal_place_data_invalid".into());
    }
    let place = match (place_id, name, place_revision) {
        (None, None, None) if latitude.is_none() && longitude.is_none() => None,
        (Some(id), Some(name), Some(place_revision)) => {
            validate_id(&id)?;
            validate_name(&name)?;
            validate_coordinates(latitude, longitude)?;
            if place_revision < 1 {
                return Err("journal_place_data_invalid".into());
            }
            Some(PlaceSnapshot {
                id,
                name,
                latitude,
                longitude,
                place_revision: place_revision.to_string(),
            })
        }
        _ => return Err("journal_place_data_invalid".into()),
    };
    Ok(JournalPlace {
        place,
        revision: revision.to_string(),
    })
}

pub(crate) fn read_journal_place_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry_id: &str,
) -> Result<crate::database_state::Snapshot<JournalPlace>, String> {
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db.transaction().map_err(|_| "journal_place_read_failed")?;
    let found: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.journal_entries WHERE id=?1)",
            [entry_id],
            |r| r.get(0),
        )
        .map_err(|_| "journal_place_read_failed")?;
    if !found {
        return Err("journal_entry_not_found".into());
    }
    let link = read_link(&tx, entry_id)?;
    tx.commit().map_err(|_| "journal_place_read_failed")?;
    Ok(operation.snapshot(link))
}

pub(crate) fn set_journal_place_with(
    state: &crate::database_state::DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    entry_id: &str,
    expected_revision: i64,
    place_id: Option<&str>,
) -> Result<crate::database_state::Snapshot<JournalPlace>, String> {
    if expected_revision < 0 {
        return Err("place_revision_invalid".into());
    }
    if let Some(id) = place_id {
        validate_id(id)?;
    }
    let operation = state.operation(Some(generation))?;
    let mut db = open()?;
    require_schema3(&db)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "journal_place_write_failed")?;
    let entry_exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.journal_entries WHERE id=?1)",
            [entry_id],
            |r| r.get(0),
        )
        .map_err(|_| "journal_place_read_failed")?;
    if !entry_exists {
        return Err("journal_entry_not_found".into());
    }
    let current = read_link(&tx, entry_id)?;
    let current_revision = parse_revision(&current.revision, true)?;
    if current_revision != expected_revision {
        return Err("place_revision_conflict".into());
    }
    let next = current_revision
        .checked_add(1)
        .ok_or("place_revision_overflow")?;
    let snapshot = if let Some(id) = place_id {
        let place = read_place(&tx, id)?;
        Some(PlaceSnapshot {
            id: place.id,
            name: place.name,
            latitude: place.latitude,
            longitude: place.longitude,
            place_revision: place.revision,
        })
    } else {
        None
    };
    let (id, name, latitude, longitude, place_revision) = match &snapshot {
        Some(value) => (
            Some(value.id.as_str()),
            Some(value.name.as_str()),
            value.latitude,
            value.longitude,
            Some(parse_revision(&value.place_revision, false)?),
        ),
        None => (None, None, None, None, None),
    };
    tx.execute(
        "INSERT INTO main.journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(entry_id) DO UPDATE SET place_id=excluded.place_id,place_name_snapshot=excluded.place_name_snapshot,latitude_snapshot=excluded.latitude_snapshot,longitude_snapshot=excluded.longitude_snapshot,place_revision_snapshot=excluded.place_revision_snapshot,revision=excluded.revision",
        params![entry_id,id,name,latitude,longitude,place_revision,next],
    )
    .map_err(|_| "journal_place_write_failed")?;
    let saved = read_link(&tx, entry_id)?;
    tx.commit().map_err(|_| "journal_place_write_failed")?;
    Ok(operation.snapshot(saved))
}

#[tauri::command]
pub(crate) fn create_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    create_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        name,
        latitude,
        longitude,
    )
}

#[tauri::command]
pub(crate) fn list_places(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
) -> Result<crate::database_state::Snapshot<Vec<Place>>, String> {
    list_with(&state, generation, || crate::database::open_database(&app))
}

#[tauri::command]
pub(crate) fn get_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    id: String,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    get_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &id,
    )
}

#[tauri::command]
pub(crate) fn update_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    id: String,
    expected_revision: String,
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<crate::database_state::Snapshot<Place>, String> {
    let revision = parse_revision(&expected_revision, false)?;
    update_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &id,
        revision,
        name,
        latitude,
        longitude,
    )
}

#[tauri::command]
pub(crate) fn delete_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    id: String,
) -> Result<crate::database_state::Snapshot<()>, String> {
    delete_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &id,
    )
}

#[tauri::command]
pub(crate) fn read_journal_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    entry_id: String,
) -> Result<crate::database_state::Snapshot<JournalPlace>, String> {
    read_journal_place_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &entry_id,
    )
}

#[tauri::command]
pub(crate) fn set_journal_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    entry_id: String,
    expected_revision: String,
    place_id: Option<String>,
) -> Result<crate::database_state::Snapshot<JournalPlace>, String> {
    let revision = parse_revision(&expected_revision, true)?;
    set_journal_place_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        &entry_id,
        revision,
        place_id.as_deref(),
    )
}

#[tauri::command]
pub(crate) fn read_weather_default_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
) -> Result<crate::database_state::Snapshot<WeatherDefaultPlace>, String> {
    read_weather_default_place_with(&state, generation, || crate::database::open_database(&app))
}

#[tauri::command]
pub(crate) fn set_weather_default_place(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    expected_revision: String,
    place_id: Option<String>,
) -> Result<crate::database_state::Snapshot<WeatherDefaultPlace>, String> {
    let revision = parse_revision(&expected_revision, true)?;
    set_weather_default_place_with(
        &state,
        generation,
        || crate::database::open_database(&app),
        revision,
        place_id.as_deref(),
    )
}

#[cfg(test)]
mod tests;
