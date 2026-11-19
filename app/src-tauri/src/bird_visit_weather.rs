//! List-owned weather. No Tauri commands, network calls or journal-weather writes.
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, format_description, format_description::well_known::Rfc3339};
const LIMIT: usize = crate::weather::storage::PART_LIMIT;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
    pub date: String,
    pub place_id: String,
    pub place_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub start: Option<String>,
    pub end: Option<String>,
}
impl Context {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
        if self.date.len() != 10
            || Date::parse(&self.date, &format)
                .map_err(|_| "visit_weather_context_invalid")?
                .to_string()
                != self.date
            || self.place_id.is_empty()
            || self.place_id.len() > 128
            || self.place_name.trim().is_empty()
            || self.place_name.len() > 512
            || !self.latitude.is_finite()
            || !self.longitude.is_finite()
            || self.latitude.abs() > 90.0
            || self.longitude.abs() > 180.0
            || !crate::database::bird_visits::valid_time(self.start.as_deref())
            || !crate::database::bird_visits::valid_time(self.end.as_deref())
        {
            return Err("visit_weather_context_invalid".into());
        }
        Ok(())
    }
}
pub(crate) fn context(db: &Connection, id: &str) -> Result<Context, String> {
    db.query_row("SELECT b.visit_date,p.id,p.name,p.latitude,p.longitude,b.visit_start,b.visit_end FROM bird_lists b JOIN places p ON p.id=b.place_id WHERE b.id=?1",[id],|r|Ok(Context{date:r.get(0)?,place_id:r.get(1)?,place_name:r.get(2)?,latitude:r.get(3)?,longitude:r.get(4)?,start:r.get(5)?,end:r.get(6)?})).map_err(|_|"bird_list_missing".into())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub list_id: String,
    pub kind: String,
    pub context_json: String,
    pub payload_json: String,
    pub stale: bool,
}
impl Record {
    pub(crate) fn new(
        list_id: &str,
        kind: &str,
        context: &Context,
        payload: &str,
    ) -> Result<Self, String> {
        let r = Self {
            list_id: list_id.into(),
            kind: kind.into(),
            context_json: serde_json::to_string(context).map_err(|e| e.to_string())?,
            payload_json: payload.into(),
            stale: false,
        };
        r.validate()?;
        Ok(r)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.list_id.is_empty()
            || self.list_id.len() > 512
            || self.context_json.len() > 4096
            || self.payload_json.len() > LIMIT
        {
            return Err("visit_weather_invalid".into());
        }
        let c: Context = serde_json::from_str(&self.context_json)
            .map_err(|_| "visit_weather_context_invalid")?;
        c.validate()?;
        match self.kind.as_str() {
            "manual" => {
                crate::weather::validate_stored_parts(&self.list_id, Some(&self.payload_json), None)
            }
            "fetched" => {
                crate::weather::validate_stored_parts(&self.list_id, None, Some(&self.payload_json))
            }
            _ => return Err("visit_weather_kind_invalid".into()),
        }
        .map_err(|e| e.to_string())?;
        let p: serde_json::Value =
            serde_json::from_str(&self.payload_json).map_err(|_| "visit_weather_invalid")?;
        if p["period"] != serde_json::json!({"kind":"day","date":c.date}) {
            return Err("visit_weather_period_invalid".into());
        }
        if let Some(location) = p.get("location").filter(|v| !v.is_null()) {
            // The shared validator has checked numeric types, ranges and finiteness.
            // Compare the same f64 values as Context, not JSON number representations.
            if location.get("coordinates").is_some_and(|v| {
                !v.is_null()
                    && (v["latitude"].as_f64() != Some(c.latitude)
                        || v["longitude"].as_f64() != Some(c.longitude))
            }) {
                return Err("visit_weather_location_invalid".into());
            }
        }
        if self.kind == "fetched" {
            let source = &p["source"];
            let forecast =
                source["dataKind"] == "forecast" && source["product"] == "forecast-daily";
            let history =
                source["dataKind"] == "reanalysis" && source["product"] == "historical-weather";
            if !forecast && !history {
                return Err("visit_weather_daily_source_invalid".into());
            }
            // The existing ERA5 route has a publication lag. Never allow today's
            // or future data to masquerade as completed historical observations.
            if history {
                let retrieved = OffsetDateTime::parse(
                    p["retrievedAt"].as_str().ok_or("visit_weather_invalid")?,
                    &Rfc3339,
                )
                .map_err(|_| "visit_weather_invalid")?;
                if c.date >= (retrieved.date() - time::Duration::days(1)).to_string() {
                    return Err("visit_weather_daily_source_invalid".into());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn is_forecast(&self) -> bool {
        self.kind == "fetched"
            && serde_json::from_str::<serde_json::Value>(&self.payload_json)
                .is_ok_and(|v| v["source"]["dataKind"] == "forecast")
    }
    pub(crate) fn resolution(&self) -> &str {
        if self.kind == "fetched" {
            "day"
        } else {
            "visit"
        }
    }
    pub(crate) fn insert(&self, db: &Connection) -> Result<(), String> {
        self.validate()?;
        db.execute(
            "INSERT INTO bird_visit_weather VALUES (?1,?2,?3,?4,?5)",
            params![
                self.list_id,
                self.kind,
                self.context_json,
                self.payload_json,
                self.stale
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub(crate) fn read_all(db: &Connection) -> Result<Vec<Record>, String> {
    let mut q=db.prepare("SELECT list_id,kind,context_json,payload_json,stale FROM bird_visit_weather ORDER BY list_id COLLATE BINARY,kind COLLATE BINARY").map_err(|e|e.to_string())?;
    q.query_map([], |r| {
        Ok(Record {
            list_id: r.get(0)?,
            kind: r.get(1)?,
            context_json: r.get(2)?,
            payload_json: r.get(3)?,
            stale: r.get(4)?,
        })
    })
    .map_err(|e| e.to_string())?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(|e| e.to_string())
}
pub(crate) fn read(db: &Connection, id: &str) -> Result<Vec<Record>, String> {
    let current = context(db, id)?;
    read_all(db)?
        .into_iter()
        .filter(|r| r.list_id == id)
        .map(|mut r| {
            r.validate()?;
            let saved: Context = serde_json::from_str(&r.context_json)
                .map_err(|_| "visit_weather_context_invalid")?;
            r.stale |= saved != current;
            Ok(r)
        })
        .collect()
}
pub(crate) fn current(records: &[Record]) -> impl Iterator<Item = &Record> {
    records.iter().filter(|r| !r.stale)
}
/// Sticky invalidation: changing a context back never silently reactivates weather.
pub(crate) fn invalidate(db: &Connection, id: &str) -> Result<(), String> {
    for r in read(db, id)? {
        if r.stale {
            db.execute(
                "UPDATE bird_visit_weather SET stale=1 WHERE list_id=?1 AND kind=?2",
                params![id, r.kind],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[derive(Clone)]
pub(crate) enum Change {
    Set { record: Record, confirmed: bool },
    Remove(String),
    ReuseManual { confirmed: bool },
}
/// Called only inside guarded list creation/update transactions, before full proof.
pub(crate) fn apply(db: &Connection, id: &str, changes: &[Change]) -> Result<(), String> {
    if !crate::database::bird_visit_weather::validate_schema(db).map_err(|e| e.to_string())? {
        return Err("visit_weather_schema_unsupported".into());
    }
    let current = context(db, id)?;
    let mut seen = std::collections::HashSet::new();
    for change in changes {
        let kind = match change {
            Change::Set { record, .. } => record.kind.as_str(),
            Change::Remove(k) => k.as_str(),
            Change::ReuseManual { .. } => "manual",
        };
        if !matches!(kind, "manual" | "fetched") || !seen.insert(kind) {
            return Err("visit_weather_change_invalid".into());
        }
        match change {
            Change::Set {
                record: r,
                confirmed,
            } => {
                r.validate()?;
                let captured: Context = serde_json::from_str(&r.context_json)
                    .map_err(|_| "visit_weather_context_invalid")?;
                if r.list_id != id || r.stale || captured != current {
                    return Err("visit_weather_context_conflict".into());
                }
                if kind == "manual"
                    && !confirmed
                    && read(db, id)?
                        .iter()
                        .any(|old| old.kind == "manual" && old.stale)
                {
                    return Err("visit_weather_confirmation_required".into());
                }
                db.execute(
                    "DELETE FROM bird_visit_weather WHERE list_id=?1 AND kind=?2",
                    params![id, kind],
                )
                .map_err(|e| e.to_string())?;
                r.insert(db)?;
            }
            Change::Remove(_) => {
                db.execute(
                    "DELETE FROM bird_visit_weather WHERE list_id=?1 AND kind=?2",
                    params![id, kind],
                )
                .map_err(|e| e.to_string())?;
            }
            Change::ReuseManual { confirmed } => {
                if !confirmed {
                    return Err("visit_weather_confirmation_required".into());
                }
                let mut old = read(db, id)?
                    .into_iter()
                    .find(|r| r.kind == "manual")
                    .ok_or("visit_weather_manual_missing")?;
                let mut p: serde_json::Value =
                    serde_json::from_str(&old.payload_json).map_err(|_| "visit_weather_invalid")?;
                p["period"] = serde_json::json!({"kind":"day","date":current.date});
                if p.get("location").is_some_and(|v| !v.is_null()) {
                    p["location"] = serde_json::json!({"name":current.place_name,"coordinates":{"latitude":current.latitude,"longitude":current.longitude}});
                }
                old.payload_json = serde_json::to_string(&p).map_err(|e| e.to_string())?;
                old.context_json = serde_json::to_string(&current).map_err(|e| e.to_string())?;
                old.stale = false;
                old.validate()?;
                db.execute(
                    "DELETE FROM bird_visit_weather WHERE list_id=?1 AND kind='manual'",
                    [id],
                )
                .map_err(|e| e.to_string())?;
                old.insert(db)?;
            }
        }
    }
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
