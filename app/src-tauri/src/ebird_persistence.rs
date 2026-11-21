//! Internal source-owned import boundary. No Tauri command or UI activation.
//! Exact rows match first; bounded corrections preserve source-row continuity.
//! Other changed/absent rows remain for review, never guessed taxon identity.
use crate::{database, database_state::DatabaseState, journal_search::SearchState};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};
type Result<T> = std::result::Result<T, String>;
pub(crate) type Fields = BTreeMap<String, String>;
const META: &[&str] = &[
    "Submission ID",
    "Location ID",
    "Location",
    "Latitude",
    "Longitude",
    "Date",
    "Time",
    "State/Province",
    "County",
    "Protocol",
    "Duration (Min)",
    "Distance Traveled (km)",
    "Area Covered (ha)",
    "Number of Observers",
    "All Obs Reported",
    "Checklist Comments",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputVisit {
    pub submission_id: String,
    pub rows: Vec<Fields>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", deny_unknown_fields)]
pub(crate) enum Record {
    Visit {
        id: String,
        submission_id: String,
        source_json: String,
    },
    Row {
        id: String,
        visit_id: String,
        position: i64,
        active: bool,
        source_json: String,
    },
    Addition {
        id: String,
        visit_id: String,
        row_id: Option<String>,
        content: String,
    },
    Link {
        entry_id: String,
        visit_id: String,
    },
}
fn sql(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn invalid<T>() -> Result<T> {
    Err("ebird_input_invalid".into())
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|u| {
        u.to_string() == s && u.get_version_num() == 4 && u.get_variant() == uuid::Variant::RFC4122
    })
}
fn source_id(s: &str, prefix: char) -> bool {
    s.starts_with(prefix)
        && s[1..].bytes().next().is_some_and(|c| c != b'0')
        && s[1..].bytes().all(|c| c.is_ascii_digit())
}
fn get<'a>(f: &'a Fields, k: &str) -> &'a str {
    f.get(k).map(String::as_str).unwrap_or("")
}
fn clock(s: &str) -> Option<u16> {
    let (time, period) = s.split_once(' ')?;
    let (h, m) = time.split_once(':')?;
    if !(1..=2).contains(&h.len())
        || m.len() != 2
        || !h.bytes().chain(m.bytes()).all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let h = h.parse::<u16>().ok()?;
    let m = m.parse::<u16>().ok()?;
    if !(1..=12).contains(&h) || m > 59 || !matches!(period, "AM" | "PM") {
        return None;
    }
    Some((h % 12 + if period == "PM" { 12 } else { 0 }) * 60 + m)
}
fn decimal(s: &str, signed: bool) -> Option<f64> {
    let value = if signed {
        s.strip_prefix('-').unwrap_or(s)
    } else {
        s
    };
    if value.is_empty()
        || value.chars().filter(|c| *c == '.').count() > 1
        || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return None;
    }
    let n = s.parse::<f64>().ok()?;
    n.is_finite().then_some(n)
}
fn metadata(f: &Fields) -> Fields {
    META.iter()
        .map(|k| ((*k).into(), get(f, k).into()))
        .collect()
}
fn equivalent(k: &str, a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if k == "Time" {
        return clock(a).is_some() && clock(a) == clock(b);
    }
    if matches!(
        k,
        "Latitude"
            | "Longitude"
            | "Duration (Min)"
            | "Distance Traveled (km)"
            | "Area Covered (ha)"
            | "Number of Observers"
    ) {
        // Compare exact decimal representations, without rounding at f64 precision.
        fn canonical(s: &str) -> String {
            let neg = s.starts_with('-');
            let s = s.trim_start_matches('-');
            let (w, p) = s.split_once('.').unwrap_or((s, ""));
            let w = w.trim_start_matches('0');
            let p = p.trim_end_matches('0');
            format!(
                "{}{}{}",
                if neg && (!w.is_empty() || !p.is_empty()) {
                    "-"
                } else {
                    ""
                },
                if w.is_empty() { "0" } else { w },
                if p.is_empty() {
                    String::new()
                } else {
                    format!(".{p}")
                }
            )
        }
        return !a.is_empty()
            && !b.is_empty()
            && decimal(a, true).is_some()
            && decimal(b, true).is_some()
            && canonical(a) == canonical(b);
    }
    false
}
fn validate_fields(f: &Fields) -> Result<()> {
    if f.len() > 128
        || f.iter()
            .any(|(k, v)| k.is_empty() || k.len() > 256 || v.len() > 65536 || v.contains('\0'))
    {
        return invalid();
    }
    for k in [
        "Submission ID",
        "Common Name",
        "Scientific Name",
        "Count",
        "Location ID",
        "Location",
        "Latitude",
        "Longitude",
        "Date",
        "Time",
    ] {
        if !f.contains_key(k) || (k != "Time" && get(f, k).trim().is_empty()) {
            return invalid();
        }
    }
    if !source_id(get(f, "Submission ID"), 'S') || !source_id(get(f, "Location ID"), 'L') {
        return invalid();
    }
    let d = get(f, "Date");
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
    if d.len() != 10
        || time::Date::parse(d, &format).map_err(sql)?.to_string() != d
        || d.starts_with("0000")
    {
        return invalid();
    }
    if !get(f, "Time").is_empty() && clock(get(f, "Time")).is_none() {
        return invalid();
    }
    for (k, max) in [("Latitude", 90.), ("Longitude", 180.)] {
        if !decimal(get(f, k), true).is_some_and(|n| n.abs() <= max) {
            return invalid();
        }
    }
    let count = get(f, "Count");
    if count != "X"
        && (!count.bytes().all(|c| c.is_ascii_digit())
            || count.starts_with('0')
            || !count.parse::<i64>().is_ok_and(|n| n > 0))
    {
        return invalid();
    }
    for k in [
        "Duration (Min)",
        "Distance Traveled (km)",
        "Area Covered (ha)",
        "Taxonomic Order",
    ] {
        if !get(f, k).is_empty() && decimal(get(f, k), false).is_none() {
            return invalid();
        }
    }
    let observers = get(f, "Number of Observers");
    if !observers.is_empty()
        && (!observers.bytes().all(|c| c.is_ascii_digit())
            || observers.starts_with('0')
            || !observers.parse::<i64>().is_ok_and(|n| n > 0))
    {
        return invalid();
    }
    if !matches!(get(f, "All Obs Reported"), "" | "0" | "1") {
        return invalid();
    }
    if get(f, "ML Catalog Numbers")
        .split_whitespace()
        .any(|id| id.starts_with('0') || !id.bytes().all(|b| b.is_ascii_digit()))
    {
        return invalid();
    }
    Ok(())
}
fn validate_visit(v: &InputVisit) -> Result<Fields> {
    if !source_id(&v.submission_id, 'S') || v.rows.is_empty() || v.rows.len() > 100000 {
        return invalid();
    }
    let meta = metadata(&v.rows[0]);
    let mut taxa = BTreeSet::new();
    for row in &v.rows {
        validate_fields(row)?;
        if get(row, "Submission ID") != v.submission_id
            || META
                .iter()
                .any(|k| !equivalent(k, get(&meta, k), get(row, k)))
            || !taxa.insert(get(row, "Scientific Name").to_lowercase())
        {
            return invalid();
        }
    }
    Ok(meta)
}
// Only these source fields may change for bounded source-row continuity.
const CORRECTION_FIELDS: &[&str] = &["Count", "Observation Details", "ML Catalog Numbers"];
fn continuity_key(fields: &Fields) -> Fields {
    fields
        .iter()
        .filter(|(k, _)| !CORRECTION_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
// Returns old indices for new rows. Each old row can be consumed once. Exact
// matching precedes correction matching; neither phase resolves ambiguity by order.
fn match_rows(old: &[Fields], new: &[Fields]) -> Vec<Option<usize>> {
    let mut matched = vec![None; new.len()];
    let mut used = BTreeSet::new();
    for (i, row) in new.iter().enumerate() {
        let candidates = old
            .iter()
            .enumerate()
            .filter(|(_, saved)| *saved == row)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if candidates.len() == 1 && new.iter().filter(|other| *other == row).count() == 1 {
            matched[i] = Some(candidates[0]);
            used.insert(candidates[0]);
        }
    }
    let old_keys = old.iter().map(continuity_key).collect::<Vec<_>>();
    let new_keys = new.iter().map(continuity_key).collect::<Vec<_>>();
    for i in 0..new.len() {
        if matched[i].is_some() {
            continue;
        }
        let candidates = old_keys
            .iter()
            .enumerate()
            .filter(|(j, key)| !used.contains(j) && *key == &new_keys[i])
            .map(|(j, _)| j)
            .collect::<Vec<_>>();
        let incoming = (0..new.len())
            .filter(|j| matched[*j].is_none() && new_keys[*j] == new_keys[i])
            .count();
        if candidates.len() == 1 && incoming == 1 {
            matched[i] = Some(candidates[0]);
            used.insert(candidates[0]);
        }
    }
    matched
}
// Full atomic batch. This function is deliberately NOT registered as an IPC command.
// Caller must already own the directory/startup lifecycle; the generation permit
// prevents a request prepared for another journal from crossing replacement.
pub(crate) fn import(
    state: &DatabaseState,
    search: &SearchState,
    module: &crate::bird_module::BirdModule,
    generation: u32,
    visits: &[InputVisit],
) -> Result<()> {
    import_checked(state, search, module, generation, visits, |_| Ok(()))
}
fn import_checked(
    state: &DatabaseState,
    search: &SearchState,
    module: &crate::bird_module::BirdModule,
    generation: u32,
    visits: &[InputVisit],
    before_commit: impl FnOnce(&Connection) -> Result<()>,
) -> Result<()> {
    let _module = module.command()?;
    search.with_write(||{
  let path=state.ready_database_path()?;let _operation=state.operation(Some(generation))?;
  let mut db=database::open_existing(&path)?;let tx=db.transaction().map_err(sql)?;
  if !database::ebird_schema::validate_schema(&tx).map_err(sql)?{return Err("ebird_schema_unsupported".into());}
  let mut seen=BTreeSet::new();let mut rows=0usize;let mut bytes=0usize;
  for visit in visits{
   let meta=validate_visit(visit)?;rows+=visit.rows.len();bytes+=serde_json::to_vec(visit).map_err(sql)?.len();
   if !seen.insert(&visit.submission_id)||rows>100000||bytes>16*1024*1024{return invalid();}
   let source_json=serde_json::to_string(&meta).map_err(sql)?;
   let existing:Option<(String,String)>=tx.query_row("SELECT id,source_json FROM ebird_visits WHERE submission_id=?1",[&visit.submission_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(sql)?;
   let id=if let Some((id,old))=existing{if old!=source_json{tx.execute("UPDATE ebird_visits SET source_json=?2 WHERE id=?1",params![id,source_json]).map_err(sql)?;}id}else{let id=uuid::Uuid::new_v4().to_string();tx.execute("INSERT INTO ebird_visits VALUES(?1,?2,?3)",params![id,visit.submission_id,source_json]).map_err(sql)?;id};
   let current:Vec<(String,i64,String)>=tx.prepare("SELECT id,position,source_json FROM ebird_rows WHERE visit_id=?1 AND active=1 ORDER BY position").map_err(sql)?.query_map([&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?.collect::<rusqlite::Result<_>>().map_err(sql)?;
   let incoming:Vec<String>=visit.rows.iter().map(serde_json::to_string).collect::<std::result::Result<_,_>>().map_err(sql)?;
   if current.iter().map(|r|&r.2).eq(incoming.iter()){continue;}
   let old_fields=current.iter().map(|row|serde_json::from_str::<Fields>(&row.2).map_err(sql)).collect::<Result<Vec<_>>>()?;
   let continuity=match_rows(&old_fields,&visit.rows);
   tx.execute("UPDATE ebird_rows SET active=0 WHERE visit_id=?1 AND active=1",[&id]).map_err(sql)?;
   for(position,source)in incoming.iter().enumerate(){
    if let Some(previous)=continuity[position]{let row=&current[previous].0;tx.execute("UPDATE ebird_rows SET active=1,position=?2,source_json=?3 WHERE id=?1",params![row,position as i64,source]).map_err(sql)?;}
    else{tx.execute("INSERT INTO ebird_rows VALUES(?1,?2,?3,1,?4)",params![uuid::Uuid::new_v4().to_string(),id,position as i64,source]).map_err(sql)?;}
   }
  }
  crate::storage_content::snapshot_v14(&tx,&AtomicBool::new(false)).map_err(|e|format!("ebird_content_invalid: {e:?}"))?;
  before_commit(&tx)?;
  tx.commit().map_err(sql)
 })
}
// Exact archive inventory, including inactive rows and their local additions.
pub(crate) fn records(db: &Connection) -> Result<Vec<Record>> {
    let mut records = Vec::new();
    let mut s = db
        .prepare("SELECT id,submission_id,source_json FROM ebird_visits ORDER BY id COLLATE BINARY")
        .map_err(sql)?;
    for r in s
        .query_map([], |r| {
            Ok(Record::Visit {
                id: r.get(0)?,
                submission_id: r.get(1)?,
                source_json: r.get(2)?,
            })
        })
        .map_err(sql)?
    {
        records.push(r.map_err(sql)?);
    }
    let mut s=db.prepare("SELECT id,visit_id,position,active,source_json FROM ebird_rows ORDER BY id COLLATE BINARY").map_err(sql)?;
    for r in s
        .query_map([], |r| {
            Ok(Record::Row {
                id: r.get(0)?,
                visit_id: r.get(1)?,
                position: r.get(2)?,
                active: r.get(3)?,
                source_json: r.get(4)?,
            })
        })
        .map_err(sql)?
    {
        records.push(r.map_err(sql)?);
    }
    let mut s = db
        .prepare(
            "SELECT id,visit_id,row_id,content FROM ebird_additions ORDER BY id COLLATE BINARY",
        )
        .map_err(sql)?;
    for r in s
        .query_map([], |r| {
            Ok(Record::Addition {
                id: r.get(0)?,
                visit_id: r.get(1)?,
                row_id: r.get(2)?,
                content: r.get(3)?,
            })
        })
        .map_err(sql)?
    {
        records.push(r.map_err(sql)?);
    }
    let mut s=db.prepare("SELECT entry_id,visit_id FROM journal_ebird_visits ORDER BY entry_id COLLATE BINARY,visit_id COLLATE BINARY").map_err(sql)?;
    for r in s
        .query_map([], |r| {
            Ok(Record::Link {
                entry_id: r.get(0)?,
                visit_id: r.get(1)?,
            })
        })
        .map_err(sql)?
    {
        records.push(r.map_err(sql)?);
    }
    Ok(records)
}
pub(crate) fn insert(db: &Connection, r: &Record) -> Result<()> {
    match r {
        Record::Visit {
            id,
            submission_id,
            source_json,
        } => {
            db.execute(
                "INSERT INTO ebird_visits VALUES(?1,?2,?3)",
                params![id, submission_id, source_json],
            )
            .map_err(sql)?;
        }
        Record::Row {
            id,
            visit_id,
            position,
            active,
            source_json,
        } => {
            db.execute(
                "INSERT INTO ebird_rows VALUES(?1,?2,?3,?4,?5)",
                params![id, visit_id, position, active, source_json],
            )
            .map_err(sql)?;
        }
        Record::Addition {
            id,
            visit_id,
            row_id,
            content,
        } => {
            db.execute(
                "INSERT INTO ebird_additions VALUES(?1,?2,?3,?4)",
                params![id, visit_id, row_id, content],
            )
            .map_err(sql)?;
        }
        Record::Link { entry_id, visit_id } => {
            db.execute(
                "INSERT INTO journal_ebird_visits VALUES(?1,?2)",
                params![entry_id, visit_id],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}
pub(crate) fn validate(db: &Connection) -> Result<()> {
    let records = records(db)?;
    if records.len() > 400000 {
        return invalid();
    }
    let mut visits = BTreeMap::new();
    let mut rows = BTreeMap::new();
    for record in &records {
        match record {
            Record::Visit {
                id,
                submission_id,
                source_json,
            } => {
                if !uuid(id) || !source_id(submission_id, 'S') || source_json.len() > 1048576 {
                    return invalid();
                }
                let f: Fields = serde_json::from_str(source_json).map_err(sql)?;
                if serde_json::to_string(&f).map_err(sql)? != *source_json {
                    return invalid();
                }
                if metadata(&f) != f || get(&f, "Submission ID") != submission_id {
                    return invalid();
                }
                visits.insert(id, (submission_id, f));
            }
            Record::Row {
                id,
                visit_id,
                position,
                active,
                source_json,
            } => {
                if !uuid(id) || *position < 0 || source_json.len() > 1048576 {
                    return invalid();
                }
                let f: Fields = serde_json::from_str(source_json).map_err(sql)?;
                if serde_json::to_string(&f).map_err(sql)? != *source_json {
                    return invalid();
                }
                validate_fields(&f)?;
                rows.insert(id, (visit_id, *position, *active, f));
            }
            Record::Addition {
                id,
                visit_id,
                row_id,
                content,
            } => {
                if !uuid(id)
                    || !uuid(visit_id)
                    || row_id.as_ref().is_some_and(|s| !uuid(s))
                    || content.len() > 65536
                    || content.contains('\0')
                {
                    return invalid();
                }
            }
            Record::Link { .. } => {}
        }
    }
    for (id, (submission, meta)) in &visits {
        let mut active = rows
            .values()
            .filter(|r| r.0 == *id && r.2)
            .collect::<Vec<_>>();
        active.sort_by_key(|r| r.1);
        if active.iter().enumerate().any(|(i, r)| r.1 != i as i64) {
            return invalid();
        }
        let input = InputVisit {
            submission_id: (*submission).clone(),
            rows: active.iter().map(|r| r.3.clone()).collect(),
        };
        if validate_visit(&input)? != *meta {
            return invalid();
        }
    }
    for (visit, _, _, f) in rows.values() {
        if !visits
            .get(visit)
            .is_some_and(|(submission, _)| get(f, "Submission ID") == submission.as_str())
        {
            return invalid();
        }
    }
    for record in &records {
        if let Record::Addition {
            visit_id,
            row_id: Some(row),
            ..
        } = record
        {
            if !rows.get(row).is_some_and(|r| r.0 == visit_id) {
                return invalid();
            }
        }
    }
    Ok(())
}
#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests;
