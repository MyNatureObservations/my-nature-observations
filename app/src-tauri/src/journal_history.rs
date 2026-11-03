//! Read-only timeline metadata. No document parsing, cache, schema or unlock state.
use crate::{
    database,
    database_state::{DatabaseState, Snapshot},
};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::Manager;
use time::{Date, Month, OffsetDateTime, format_description::well_known::Rfc3339};

const PAGE_SIZE: usize = 50;
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Overview {
    total_entries: u64,
    navigation_visible: bool,
    // Original RFC3339 spelling of the earliest valid instant, not a new stored value.
    earliest_created_at: Option<String>,
    years: Vec<Year>,
    undated_count: u64,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Year {
    year: i32,
    count: u64,
    months: Vec<MonthCount>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct MonthCount {
    month: u8,
    count: u64,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    id: String,
    entry_date: String,
    title: String,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entries {
    entries: Vec<Entry>,
    has_more: bool,
}

fn sql(_: rusqlite::Error) -> String {
    "journal_history_failed".into()
}
fn entry_month(raw: &str) -> Option<(i32, u8)> {
    let b = raw.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return None;
    }
    let year: i32 = raw[..4].parse().ok()?;
    let month: u8 = raw[5..7].parse().ok()?;
    let day: u8 = raw[8..].parse().ok()?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    Some((year, month))
}
fn overview_in_session(
    state: &DatabaseState,
    open: impl FnOnce() -> Result<Connection, String>,
    now: OffsetDateTime,
) -> Result<Snapshot<Overview>, String> {
    let operation = state.operation(None)?;
    let mut connection = open()?;
    let tx = connection.transaction().map_err(sql)?;
    let mut total_entries = 0;
    let mut undated_count = 0;
    let mut earliest: Option<(i128, String)> = None;
    let mut groups: BTreeMap<i32, BTreeMap<u8, u64>> = BTreeMap::new();
    {
        let mut statement = tx
            .prepare("SELECT entry_date, created_at FROM journal_entries")
            .map_err(sql)?;
        let mut rows = statement.query([]).map_err(sql)?;
        while let Some(row) = rows.next().map_err(sql)? {
            let date: String = row.get(0).map_err(sql)?;
            let created: String = row.get(1).map_err(sql)?;
            total_entries += 1;
            if let Some((year, month)) = entry_month(&date) {
                *groups.entry(year).or_default().entry(month).or_default() += 1;
            } else {
                undated_count += 1;
            }
            if let Ok(stamp) = OffsetDateTime::parse(&created, &Rfc3339) {
                // Unix nanoseconds compare absolute UTC instants, including offsets.
                let instant = stamp.unix_timestamp_nanos();
                if earliest.as_ref().is_none_or(|(old, _)| instant < *old) {
                    earliest = Some((instant, created));
                }
            }
        }
    }
    let navigation_visible = total_entries >= 5
        && earliest.as_ref().is_some_and(|(stamp, _)| {
            now.unix_timestamp_nanos() - stamp >= time::Duration::days(30).whole_nanoseconds()
        });
    let years = groups
        .into_iter()
        .rev()
        .map(|(year, months)| Year {
            year,
            count: months.values().sum(),
            months: months
                .into_iter()
                .rev()
                .map(|(month, count)| MonthCount { month, count })
                .collect(),
        })
        .collect();
    tx.rollback().map_err(sql)?;
    Ok(operation.snapshot(Overview {
        total_entries,
        navigation_visible,
        earliest_created_at: earliest.map(|(_, raw)| raw),
        years,
        undated_count,
    }))
}
fn period(year: Option<i32>, month: Option<u8>) -> Result<Option<(i32, u8)>, String> {
    match (year, month) {
        (None, None) => Ok(None),
        (Some(y), Some(m)) if (1..=9999).contains(&y) && (1..=12).contains(&m) => Ok(Some((y, m))),
        _ => Err("journal_history_invalid_period".into()),
    }
}
fn entries_in_session(
    state: &DatabaseState,
    open: impl FnOnce() -> Result<Connection, String>,
    year: Option<i32>,
    month: Option<u8>,
    offset: u32,
) -> Result<Snapshot<Entries>, String> {
    let selected = period(year, month)?;
    let operation = state.operation(None)?;
    let mut connection = open()?;
    let tx = connection.transaction().map_err(sql)?;
    let mut entries = Vec::new();
    let mut skipped = 0;
    {
        // A lexical range uses the existing date index. Calendar validation below
        // excludes legacy strings inside that range without losing them from undated.
        let mut statement = tx.prepare(if selected.is_some() {
            "SELECT id, entry_date, title FROM journal_entries WHERE entry_date >= ?1 AND entry_date < ?2 ORDER BY entry_date DESC, created_at DESC, id ASC"
        } else {
            "SELECT id, entry_date, title FROM journal_entries ORDER BY entry_date DESC, created_at DESC, id ASC"
        }).map_err(sql)?;
        let mut rows = if let Some((year, month)) = selected {
            let lower = format!("{year:04}-{month:02}-01");
            // 9999-13-01 is only an upper SQL bound, never a parsed journal date.
            let upper = if month < 12 || year == 9999 {
                format!("{year:04}-{:02}-01", month + 1)
            } else {
                format!("{:04}-01-01", year + 1)
            };
            statement.query(params![lower, upper]).map_err(sql)?
        } else {
            statement.query([]).map_err(sql)?
        };
        while let Some(row) = rows.next().map_err(sql)? {
            let date: String = row.get(1).map_err(sql)?;
            if entry_month(&date) != selected {
                continue;
            }
            if skipped < offset {
                skipped += 1;
                continue;
            }
            entries.push(Entry {
                id: row.get(0).map_err(sql)?,
                entry_date: date,
                title: row.get(2).map_err(sql)?,
            });
            if entries.len() > PAGE_SIZE {
                break;
            }
        }
    }
    let has_more = entries.len() > PAGE_SIZE;
    entries.truncate(PAGE_SIZE);
    tx.rollback().map_err(sql)?;
    Ok(operation.snapshot(Entries { entries, has_more }))
}
#[tauri::command]
pub(crate) async fn journal_history_overview(
    app: tauri::AppHandle,
) -> Result<Snapshot<Overview>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        overview_in_session(
            &app.state::<DatabaseState>(),
            || database::open_database(&app),
            OffsetDateTime::now_utc(),
        )
    })
    .await
    .map_err(|_| "journal_history_failed".to_owned())?
}
#[tauri::command]
pub(crate) async fn journal_history_entries(
    app: tauri::AppHandle,
    year: Option<i32>,
    month: Option<u8>,
    offset: Option<u32>,
) -> Result<Snapshot<Entries>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        entries_in_session(
            &app.state::<DatabaseState>(),
            || database::open_database(&app),
            year,
            month,
            offset.unwrap_or(0),
        )
    })
    .await
    .map_err(|_| "journal_history_failed".to_owned())?
}
#[cfg(test)]
mod tests;
