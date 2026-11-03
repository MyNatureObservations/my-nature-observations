//! Disposable FTS index. Lock order is search cache, then normal operation permit.
//! Maintenance never acquires this cache. No journal writes or persistent search files.
use crate::{
    database,
    database_state::{DatabaseState, Snapshot},
};
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Default)]
pub(crate) struct SearchState(Mutex<Option<Index>>);
struct Index {
    generation: u32,
    database: Connection,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Hit {
    pub id: String,
    pub entry_date: String,
    pub title: String,
    pub excerpt: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct Results {
    pub entries: Vec<Hit>,
    pub more: bool,
}
impl SearchState {
    // Keep the lock through the real save/commit and invalidation. Failed saves
    // do not claim success; a cache failure prevents a write, never loses one.
    pub(crate) fn with_write<T>(
        &self,
        save: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut cache = self.0.lock().map_err(|_| "journal_search_unavailable")?;
        let result = save();
        if result.is_ok() {
            *cache = None;
        }
        result
    }
    fn search(
        &self,
        state: &DatabaseState,
        open: impl FnOnce() -> Result<Connection, String>,
        query: &str,
        offset: u32,
    ) -> Result<Snapshot<Results>, String> {
        let expression = expression(query)?;
        let mut cache = self.0.lock().map_err(|_| "journal_search_unavailable")?;
        let operation = state.operation(None)?;
        let generation = operation.snapshot(()).generation;
        if expression.is_empty() {
            return Ok(operation.snapshot(Results {
                entries: vec![],
                more: false,
            }));
        }
        if cache.as_ref().is_none_or(|i| i.generation != generation) {
            // Publish only after the entire consistent snapshot has been indexed.
            *cache = Some(Index {
                generation,
                database: build_index(open()?)?,
            });
        }
        let index = &cache.as_ref().unwrap().database;
        let mut statement = index.prepare("SELECT id, entry_date, title, snippet(entries, 3, '', '', '…', 24) FROM entries WHERE entries MATCH ?1 ORDER BY bm25(entries, 0, 0, 5, 1), entry_date DESC, id ASC LIMIT 51 OFFSET ?2").map_err(sql)?;
        let mut entries = statement
            .query_map(params![expression, offset], |row| {
                Ok(Hit {
                    id: row.get(0)?,
                    entry_date: row.get(1)?,
                    title: row.get(2)?,
                    excerpt: row.get(3)?,
                })
            })
            .map_err(sql)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql)?;
        let more = entries.len() > 50;
        entries.truncate(50);
        Ok(operation.snapshot(Results { entries, more }))
    }
}
fn sql(_: rusqlite::Error) -> String {
    "journal_search_failed".into()
}
fn build_index(mut source: Connection) -> Result<Connection, String> {
    let tx = source.transaction().map_err(sql)?;
    let mut index = Connection::open_in_memory().map_err(sql)?;
    index.execute_batch("CREATE VIRTUAL TABLE entries USING fts5(id UNINDEXED, entry_date UNINDEXED, title, body, tokenize='unicode61 remove_diacritics 0');").map_err(sql)?;
    {
        let index_tx = index.transaction().map_err(sql)?;
        let mut insert = index_tx
            .prepare("INSERT INTO entries(id,entry_date,title,body) VALUES (?1,?2,?3,?4)")
            .map_err(sql)?;
        let mut rows = tx
            .prepare("SELECT id,entry_date,title,content_json FROM journal_entries ORDER BY id")
            .map_err(sql)?;
        let mut cursor = rows.query([]).map_err(sql)?;
        while let Some(row) = cursor.next().map_err(sql)? {
            let raw: String = row.get(3).map_err(sql)?;
            // Invalid/unknown structures remain stored untouched and title-searchable.
            let body = document_text(&raw);
            insert
                .execute(params![
                    row.get::<_, String>(0).map_err(sql)?,
                    row.get::<_, String>(1).map_err(sql)?,
                    row.get::<_, String>(2).map_err(sql)?,
                    body
                ])
                .map_err(sql)?;
        }
        drop(insert);
        index_tx.commit().map_err(sql)?;
    }
    tx.rollback().map_err(sql)?;
    Ok(index)
}
fn document_text(raw: &str) -> String {
    fn visit(node: &Value, out: &mut String, depth: usize) {
        if depth > 64 {
            return;
        }
        let kind = node.get("type").and_then(Value::as_str);
        if kind == Some("image") {
            return;
        }
        if kind == Some("text") || kind.is_none() {
            if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
        if kind == Some("hardBreak") {
            out.push('\n');
        }
        if let Some(children) = node.get("content").and_then(Value::as_array) {
            for child in children {
                visit(child, out, depth + 1);
            }
        }
        if !matches!(kind, Some("text" | "hardBreak" | "image")) {
            out.push('\n');
        }
    }
    let mut out = String::new();
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        visit(&value, &mut out, 0);
    }
    out
}
// Only quoted literal tokens/phrases reach MATCH, never user FTS operators.
fn expression(input: &str) -> Result<String, String> {
    if input.len() > 512 {
        return Err("journal_search_query_too_long".into());
    }
    let mut clauses = vec![];
    let mut phrase = false;
    for part in input.split('"') {
        let words: Vec<_> = part
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .collect();
        if phrase && !words.is_empty() {
            clauses.push(format!("\"{}\"", words.join(" ")));
        } else {
            clauses.extend(words.iter().map(|s| format!("\"{s}\"*")));
        }
        phrase = !phrase;
    }
    if clauses.len() > 32 {
        return Err("journal_search_query_too_long".into());
    }
    Ok(clauses.join(" AND "))
}
#[tauri::command]
pub(crate) async fn search_journal(
    app: tauri::AppHandle,
    query: String,
    offset: Option<u32>,
) -> Result<Snapshot<Results>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<SearchState>().search(
            &app.state::<DatabaseState>(),
            || database::open_database(&app),
            &query,
            offset.unwrap_or(0),
        )
    })
    .await
    .map_err(|_| "journal_search_failed".to_owned())?
}
#[cfg(test)]
mod tests;
