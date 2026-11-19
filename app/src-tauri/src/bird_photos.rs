//! Bird-row ownership; shared image processing, with no original-file writes.
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Photo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    pub row_id: String,
    pub asset_id: String,
    pub original_path: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    #[serde(with = "crate::photographs::preview_base64")]
    pub preview: Vec<u8>,
}
impl Photo {
    pub(crate) fn from_asset(a: crate::photographs::Asset) -> Self {
        Self {
            caption: None,
            row_id: a.entry_id,
            asset_id: a.asset_id,
            original_path: a.original_path,
            mime: a.mime,
            width: a.width,
            height: a.height,
            preview: a.preview,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !crate::database::bird_captions::valid_caption(self.caption.as_deref()) {
            return Err("bird_photo_caption_invalid".into());
        }
        if self.row_id.trim().is_empty() || self.row_id.len() > 512 {
            return Err("bird_photo_row_invalid".into());
        }
        crate::photographs::Asset {
            entry_id: self.row_id.clone(),
            asset_id: self.asset_id.clone(),
            original_path: self.original_path.clone(),
            mime: self.mime.clone(),
            width: self.width,
            height: self.height,
            preview: self.preview.clone(),
        }
        .validate()
    }
    pub(crate) fn insert(&self, db: &Connection) -> Result<(), String> {
        self.validate()?;
        let captions = crate::database::bird_visit_weather::validate_schema(db)
            .map_err(|e| e.to_string())?
            || crate::database::bird_captions::validate_schema(db).map_err(|e| e.to_string())?;
        if !captions && self.caption.is_some() {
            return Err("bird_photo_caption_schema_unsupported".into());
        }
        if captions {
            db.execute(
                "INSERT INTO bird_row_photos VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    self.row_id,
                    self.asset_id,
                    self.original_path,
                    self.mime,
                    self.width,
                    self.height,
                    self.preview,
                    self.caption
                ],
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }
        db.execute(
            "INSERT INTO bird_row_photos VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                self.row_id,
                self.asset_id,
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
pub(crate) fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Photo> {
    Ok(Photo {
        caption: if r.as_ref().column_count() > 7 {
            r.get(7)?
        } else {
            None
        },
        row_id: r.get(0)?,
        asset_id: r.get(1)?,
        original_path: r.get(2)?,
        mime: r.get(3)?,
        width: r.get(4)?,
        height: r.get(5)?,
        preview: r.get(6)?,
    })
}
pub(crate) fn get(db: &Connection, id: &str) -> Result<Option<Photo>, String> {
    use rusqlite::OptionalExtension;
    db.query_row(if crate::database::bird_visit_weather::validate_schema(db).map_err(|e|e.to_string())? || crate::database::bird_captions::validate_schema(db).map_err(|e|e.to_string())? {"SELECT row_id,asset_id,original_path,mime,width,height,preview,caption FROM bird_row_photos WHERE row_id=?1"}else{"SELECT row_id,asset_id,original_path,mime,width,height,preview FROM bird_row_photos WHERE row_id=?1"},[id],row).optional().map_err(|e|e.to_string())
}
#[tauri::command]
pub(crate) async fn prepare_bird_photograph(
    app: tauri::AppHandle,
    generation: u32,
    path: String,
    row_id: String,
) -> Result<Photo, String> {
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<crate::database_state::DatabaseState>();
        let _permit = state.operation(Some(generation))?;
        let photo = Photo::from_asset(crate::photographs::prepare(
            std::path::Path::new(&path),
            row_id,
        )?);
        photo.validate()?;
        Ok(photo)
    })
    .await
    .map_err(|_| "photograph_preparation_failed".to_string())?
}

#[cfg(all(test, target_os = "macos"))]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyPhoto {
    row_id: String,
    asset_id: String,
    original_path: String,
    mime: String,
    width: u32,
    height: u32,
    #[serde(with = "crate::photographs::preview_base64")]
    preview: Vec<u8>,
}
impl LegacyPhoto {
    pub(crate) fn into_photo(self) -> Photo {
        Photo {
            caption: None,
            row_id: self.row_id,
            asset_id: self.asset_id,
            original_path: self.original_path,
            mime: self.mime,
            width: self.width,
            height: self.height,
            preview: self.preview,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CaptionedPhoto {
    row_id: String,
    asset_id: String,
    original_path: String,
    mime: String,
    width: u32,
    height: u32,
    #[serde(with = "crate::photographs::preview_base64")]
    preview: Vec<u8>,
    #[serde(deserialize_with = "required_caption")]
    caption: Option<String>,
}
fn required_caption<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
impl CaptionedPhoto {
    pub(crate) fn into_photo(self) -> Photo {
        Photo {
            caption: self.caption,
            row_id: self.row_id,
            asset_id: self.asset_id,
            original_path: self.original_path,
            mime: self.mime,
            width: self.width,
            height: self.height,
            preview: self.preview,
        }
    }
}
pub(crate) fn supported_schema(db: &Connection) -> rusqlite::Result<bool> {
    Ok(crate::database::bird_visit_weather::validate_schema(db)?
        || crate::database::bird_captions::validate_schema(db)?
        || crate::database::bird_photos::validate_schema(db)?)
}
