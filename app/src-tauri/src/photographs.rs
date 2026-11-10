//! Durable per-entry photographs. External originals are read only at preparation.
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Cursor, Read},
    path::Path,
};

pub(crate) const PREVIEW_LIMIT: usize = 4 * 1024 * 1024;
const SOURCE_LIMIT: u64 = 64 * 1024 * 1024;
const PIXEL_LIMIT: u64 = 60_000_000;
pub(crate) const DDL: &str = "
CREATE TABLE journal_image_assets (
    asset_id TEXT PRIMARY KEY NOT NULL,
    entry_id TEXT NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    mime TEXT NOT NULL CHECK (mime IN ('image/jpeg','image/png')),
    width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 2500),
    height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 2500),
    preview BLOB NOT NULL CHECK (length(preview) BETWEEN 1 AND 4194304)
);
";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Asset {
    pub asset_id: String,
    pub entry_id: String,
    pub original_path: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    #[serde(with = "preview_base64")]
    pub preview: Vec<u8>,
}
mod preview_base64 {
    use super::*;
    pub fn serialize<S: serde::Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        if text.len() > PREVIEW_LIMIT.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("photograph_preview_limit"));
        }
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}
fn id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| {
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == value
    })
}
fn decode(
    bytes: &[u8],
    max_side: u32,
    max_pixels: u64,
) -> Result<(DynamicImage, ImageFormat), String> {
    let format = image::guess_format(bytes).map_err(|_| "photograph_format")?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
        return Err("photograph_format".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > max_pixels {
        return Err("photograph_pixel_limit".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok((image, format))
}

pub(crate) fn prepare(path: &Path, entry_id: String) -> Result<Asset, String> {
    if !path.is_absolute() {
        return Err("photograph_absolute_path_required".into());
    }
    let original_path = path.to_str().ok_or("photograph_path_encoding")?.to_owned();
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("photograph_file_required".into());
    }
    let mut bytes = Vec::new();
    file.take(SOURCE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > SOURCE_LIMIT {
        return Err("photograph_source_limit".into());
    }
    let (image, format) = decode(&bytes, 32768, PIXEL_LIMIT)?;
    let mut preview = image.resize(
        2500.min(image.width()),
        2500.min(image.height()),
        FilterType::Triangle,
    );
    loop {
        let mut encoded = Cursor::new(Vec::new());
        if format == ImageFormat::Jpeg {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
                .encode_image(&preview.to_rgb8())
                .map_err(|e| e.to_string())?;
        } else {
            preview
                .write_to(&mut encoded, ImageFormat::Png)
                .map_err(|e| e.to_string())?;
        }
        let encoded = encoded.into_inner();
        if encoded.len() <= PREVIEW_LIMIT {
            return Ok(Asset {
                asset_id: uuid::Uuid::new_v4().to_string(),
                entry_id,
                original_path,
                mime: if format == ImageFormat::Jpeg {
                    "image/jpeg"
                } else {
                    "image/png"
                }
                .into(),
                width: preview.width(),
                height: preview.height(),
                preview: encoded,
            });
        }
        let side = (preview.width().max(preview.height()) * 3 / 4).max(1);
        if side == preview.width().max(preview.height()) {
            return Err("photograph_preview_limit".into());
        }
        preview = preview.resize(side, side, FilterType::Triangle);
    }
}
impl Asset {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !id(&self.asset_id)
            || self.entry_id.is_empty()
            || self.entry_id.len() > 16384
            || !(Path::new(&self.original_path).is_absolute()
                || self.original_path.starts_with("\\\\")
                || (self.original_path.as_bytes().get(1) == Some(&b':')
                    && matches!(self.original_path.as_bytes().get(2), Some(b'/' | b'\\'))))
            || self.original_path.is_empty()
            || self.original_path.len() > 32768
            || self.original_path.contains('\0')
            || self.preview.is_empty()
            || self.preview.len() > PREVIEW_LIMIT
        {
            return Err("photograph_metadata".into());
        }
        // A saved path is inert metadata and may belong to another platform.
        let (image, format) = decode(&self.preview, 2500, 2500 * 2500)?;
        let mime = if format == ImageFormat::Jpeg {
            "image/jpeg"
        } else {
            "image/png"
        };
        if self.mime != mime || (image.width(), image.height()) != (self.width, self.height) {
            return Err("photograph_metadata".into());
        }
        Ok(())
    }
    pub(crate) fn insert(&self, db: &Connection) -> Result<(), String> {
        self.validate()?;
        let existing = get(db, &self.asset_id)?;
        if let Some(existing) = existing {
            return if existing == *self {
                Ok(())
            } else {
                Err("photograph_asset_conflict".into())
            };
        }
        db.execute(
            "INSERT INTO journal_image_assets VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                self.asset_id,
                self.entry_id,
                self.original_path,
                self.mime,
                self.width,
                self.height,
                self.preview
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub(crate) fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        asset_id: row.get(0)?,
        entry_id: row.get(1)?,
        original_path: row.get(2)?,
        mime: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        preview: row.get(6)?,
    })
}
pub(crate) fn get(db: &Connection, asset_id: &str) -> Result<Option<Asset>, String> {
    use rusqlite::OptionalExtension;
    db.query_row("SELECT asset_id,entry_id,original_path,mime,width,height,preview FROM journal_image_assets WHERE asset_id=?1", [asset_id], row)
        .optional().map_err(|e| e.to_string())
}

pub(crate) fn validate_document(json: &str) -> Result<Vec<String>, String> {
    if json.len() > crate::open_export::CONTENT_LIMIT
        || crate::open_export::validate_json(json).map_err(|_| "journal_document_incompatible")?
    {
        return Err("journal_document_incompatible".into());
    }
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let nodes = value
        .get_mut("content")
        .and_then(|v| v.as_array_mut())
        .ok_or("journal_document_incompatible")?;
    let mut node_ids = HashSet::new();
    let mut assets = Vec::new();
    for node in nodes {
        if node["type"] != "journalImage" {
            continue;
        }
        let object = node.as_object().ok_or("journal_document_incompatible")?;
        if object.len() != 2 {
            return Err("journal_document_incompatible".into());
        }
        let attrs = node["attrs"]
            .as_object()
            .ok_or("journal_document_incompatible")?;
        if attrs.keys().any(|k| {
            !["nodeId", "assetId", "placement", "size", "caption", "alt"].contains(&k.as_str())
        }) {
            return Err("journal_document_incompatible".into());
        }
        let node_id = attrs
            .get("nodeId")
            .and_then(|v| v.as_str())
            .ok_or("photograph_node_id")?;
        let asset = attrs
            .get("assetId")
            .and_then(|v| v.as_str())
            .ok_or("photograph_asset_id")?;
        if !id(node_id)
            || !id(asset)
            || !node_ids.insert(node_id.to_owned())
            || !matches!(
                attrs.get("placement").and_then(|v| v.as_str()),
                Some("left" | "right" | "row")
            )
            || !matches!(
                attrs.get("size").and_then(|v| v.as_str()),
                Some("small" | "medium" | "large")
            )
            || ["caption", "alt"].iter().any(|key| {
                attrs
                    .get(*key)
                    .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 16384))
            })
        {
            return Err("journal_document_incompatible".into());
        }
        assets.push(asset.to_owned());
        // Validate the unchanged text grammar using its frozen, bounded parser.
        *node = serde_json::json!({"type":"paragraph"});
    }
    let text = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    if !crate::backup::restore::document_is_editable(&text) {
        return Err("journal_document_incompatible".into());
    }
    Ok(assets)
}
pub(crate) fn validate_references(db: &Connection, entry: &str, json: &str) -> Result<(), String> {
    for asset in validate_document(json)? {
        let owned: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM journal_image_assets WHERE asset_id=?1 AND entry_id=?2)", params![asset, entry], |row| row.get(0)).map_err(|e| e.to_string())?;
        if !owned {
            return Err("photograph_missing_asset".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

pub(crate) fn create_schema(db: &Connection) -> Result<(), String> {
    db.execute_batch(DDL).map_err(|e| e.to_string())?;
    db.pragma_update(None, "user_version", 4)
        .map_err(|e| e.to_string())
}
pub(crate) fn validate_schema(db: &Connection) -> rusqlite::Result<bool> {
    if db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))? != 4 {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    crate::database::initialize_schema(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    for ddl in [
        crate::database::weather_schema::DDL,
        crate::database::place_schema::DDL,
        crate::database::weather_default_schema::DDL,
        DDL,
    ] {
        reference.execute_batch(ddl)?;
    }
    fn objects(db: &Connection) -> rusqlite::Result<Vec<(String, String, String, Option<String>)>> {
        db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name LIMIT 16")?
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
    Ok(objects(db)? == objects(&reference)?)
}

#[tauri::command]
pub(crate) async fn prepare_journal_photograph(
    app: tauri::AppHandle,
    generation: u32,
    path: String,
    entry_id: String,
) -> Result<Asset, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<crate::database_state::DatabaseState>();
        let _operation = state.operation(Some(generation))?;
        prepare(Path::new(&path), entry_id)
    })
    .await
    .map_err(|_| "photograph_preparation_failed".to_string())?
}

pub(crate) fn document_editable(db: &Connection, entry: &str, version: i64, json: &str) -> bool {
    match version {
        1 => crate::backup::restore::document_is_editable(json),
        2 => validate_references(db, entry, json).is_ok(),
        _ => false,
    }
}

pub(crate) fn save(
    db: &mut Connection,
    entry_id: &str,
    entry_date: &str,
    title: &str,
    json: &str,
    assets: &[Asset],
    create_entry: bool,
) -> Result<(), String> {
    use rusqlite::OptionalExtension;
    if entry_id.is_empty() {
        return Err("photograph_entry_id".into());
    }
    validate_document(json)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let previous: Option<(i64, String)> = tx
        .query_row(
            "SELECT document_version,content_json FROM journal_entries WHERE id=?1",
            [entry_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if create_entry && previous.is_some() {
        return Err("journal_entry_already_exists".into());
    }
    if !create_entry && previous.is_none() {
        return Err("journal_entry_not_found".into());
    }
    if let Some((version, original)) = previous {
        let valid = match version {
            1 => crate::backup::restore::document_is_editable(&original),
            2 => validate_references(&tx, entry_id, &original).is_ok(),
            _ => false,
        };
        if !valid {
            return Err("journal_document_incompatible".into());
        }
    }
    tx.execute("INSERT INTO journal_entries(id,entry_date,title,content_json,document_version,created_at,updated_at)
        VALUES(?1,?2,?3,?4,2,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        ON CONFLICT(id) DO UPDATE SET entry_date=excluded.entry_date,title=excluded.title,
        content_json=excluded.content_json,document_version=2,updated_at=excluded.updated_at",
        params![entry_id,entry_date,title,json]).map_err(|e| e.to_string())?;
    for asset in assets {
        if asset.entry_id != entry_id {
            return Err("photograph_asset_owner".into());
        }
        asset.insert(&tx)?;
    }
    validate_references(&tx, entry_id, json)?;
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_journal_with_photographs(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    search: tauri::State<'_, crate::journal_search::SearchState>,
    generation: u32,
    entry_id: String,
    entry_date: String,
    title: String,
    content_json: String,
    assets: Vec<Asset>,
    create_entry: bool,
) -> Result<(), String> {
    search.with_write(|| {
        let _operation = state.operation(Some(generation))?;
        let mut db = crate::database::open_database(&app)?;
        save(
            &mut db,
            &entry_id,
            &entry_date,
            &title,
            &content_json,
            &assets,
            create_entry,
        )
    })
}

#[tauri::command]
pub(crate) fn get_journal_photograph(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::database_state::DatabaseState>,
    generation: u32,
    entry_id: String,
    asset_id: String,
) -> Result<Option<Asset>, String> {
    let _operation = state.operation(Some(generation))?;
    let db = crate::database::open_database(&app)?;
    let asset = get(&db, &asset_id)?;
    if asset.as_ref().is_some_and(|a| a.entry_id != entry_id) {
        return Err("photograph_asset_owner".into());
    }
    Ok(asset)
}

#[tauri::command]
pub(crate) async fn select_journal_photograph(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .add_filter("JPEG / PNG", &["jpg", "jpeg", "png"])
            .blocking_pick_file()
            .map(|file| {
                file.into_path()
                    .map_err(|_| "photograph_file_required".to_string())?
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "photograph_path_encoding".to_string())
            })
            .transpose()
    })
    .await
    .map_err(|_| "photograph_selection_failed".to_string())?
}
