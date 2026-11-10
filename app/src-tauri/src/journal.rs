use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use tauri::AppHandle;

use crate::{
    database,
    database_state::{DatabaseState, Snapshot},
};
use tauri::State;

#[derive(Serialize)]
pub struct SavedJournalEntry {
    id: String,
}

#[tauri::command]
pub fn save_journal_entry(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    search: State<'_, crate::journal_search::SearchState>,
    id: Option<String>,
    new_entry_id: Option<String>,
    entry_date: String,
    title: String,
    content_json: String,
    generation: u32,
) -> Result<SavedJournalEntry, String> {
    let target = match (id, new_entry_id) {
        (Some(id), None) => EntryTarget::Existing(id),
        (None, reserved) => EntryTarget::New(reserved),
        _ => return Err("journal_entry_target_invalid".into()),
    };
    search.with_write(|| {
        save_in_session(
            &state,
            generation,
            || database::open_database(&app),
            target,
            entry_date,
            title,
            content_json,
        )
    })
}

#[tauri::command]
pub(crate) fn delete_journal_entry(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    search: State<'_, crate::journal_search::SearchState>,
    id: String,
    generation: u32,
) -> Result<(), String> {
    search.with_write(|| {
        let _operation = state.operation(Some(generation))?;
        delete_entry(&mut database::open_database(&app)?, &id)
    })
}

pub(crate) fn delete_entry(connection: &mut Connection, id: &str) -> Result<(), String> {
    let tx = connection.transaction().map_err(|error| error.to_string())?;
    // Weather restricts deletion. Photo previews and place links cascade.
    tx.execute("DELETE FROM journal_weather WHERE entry_id = ?1", [id])
        .map_err(|error| error.to_string())?;
    let deleted = tx.execute("DELETE FROM journal_entries WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    if deleted != 1 {
        return Err("journal_entry_not_found".into());
    }
    tx.commit().map_err(|error| error.to_string())
}

pub(crate) enum EntryTarget {
    Existing(String),
    New(Option<String>),
}

pub(crate) fn save_in_session(
    state: &DatabaseState,
    generation: u32,
    open: impl FnOnce() -> Result<Connection, String>,
    target: EntryTarget,
    entry_date: String,
    title: String,
    content_json: String,
) -> Result<SavedJournalEntry, String> {
    let _operation = state.operation(Some(generation))?;
    if matches!(target, EntryTarget::New(_)) && is_definitely_empty_journal_json(&content_json) {
        return Err("journal_empty_entry".into());
    }
    validate_document(&content_json)?;
    let connection = open()?;
    save_journal_entry_with_connection(&connection, target, entry_date, title, content_json)
}

fn validate_document(content_json: &str) -> Result<(), String> {
    if !crate::backup::restore::document_is_editable(content_json) {
        return Err("journal_document_incompatible".into());
    }
    Ok(())
}

// Deliberately narrower than validation: uncertain/future content is not empty.
fn is_definitely_empty_journal_json(content: &str) -> bool {
    fn keys(value: &serde_json::Map<String, serde_json::Value>, allowed: &[&str]) -> bool {
        value.keys().all(|key| allowed.contains(&key.as_str()))
    }
    fn integer(value: &serde_json::Value, max: f64) -> bool {
        value
            .as_f64()
            .is_some_and(|n| n >= 1.0 && n <= max && n.fract() == 0.0)
    }
    fn attrs(node: &serde_json::Map<String, serde_json::Value>, kind: &str) -> bool {
        let Some(value) = node.get("attrs") else {
            return true;
        };
        let Some(a) = value.as_object() else {
            return false;
        };
        match kind {
            "heading" => keys(a, &["level"]) && a.get("level").is_none_or(|v| integer(v, 6.0)),
            "orderedList" => {
                keys(a, &["start", "type"])
                    && a.get("start")
                        .is_none_or(|v| integer(v, 9_007_199_254_740_991.0))
                    && a.get("type").is_none_or(|v| {
                        v.is_null()
                            || v.as_str()
                                .is_some_and(|v| ["1", "a", "A", "i", "I"].contains(&v))
                    })
            }
            "codeBlock" => {
                keys(a, &["language"])
                    && a.get("language")
                        .is_none_or(|v| v.is_null() || v.is_string())
            }
            _ => keys(a, &[]),
        }
    }
    fn empty(value: &serde_json::Value, allowed: &[&str], depth: usize) -> bool {
        const BLOCKS: &[&str] = &[
            "paragraph",
            "heading",
            "blockquote",
            "bulletList",
            "orderedList",
            "codeBlock",
            "horizontalRule",
        ];
        let Some(node) = value.as_object() else {
            return false;
        };
        let Some(kind) = node.get("type").and_then(|v| v.as_str()) else {
            return false;
        };
        if depth > 64 || !allowed.contains(&kind) {
            return false;
        }
        if kind == "text" {
            let Some(text) = node.get("text").and_then(|v| v.as_str()) else {
                return false;
            };
            if !keys(node, &["type", "text", "marks"])
                || !text.chars().all(|c| {
                    matches!(
                        c,
                        '\t' | '\n'
                            | '\u{000b}'
                            | '\u{000c}'
                            | '\r'
                            | ' '
                            | '\u{00a0}'
                            | '\u{1680}'
                            | '\u{2000}'
                            ..='\u{200a}'
                                | '\u{2028}'
                                | '\u{2029}'
                                | '\u{202f}'
                                | '\u{205f}'
                                | '\u{3000}'
                                | '\u{feff}'
                    )
                })
            {
                return false;
            }
            return node.get("marks").is_none_or(|v| {
                v.as_array().is_some_and(|marks| {
                    marks.iter().all(|mark| {
                        let Some(m) = mark.as_object() else {
                            return false;
                        };
                        keys(m, &["type", "attrs"])
                            && m.get("type").and_then(|v| v.as_str()).is_some_and(|v| {
                                ["bold", "italic", "strike", "underline", "code"].contains(&v)
                            })
                            && m.get("attrs")
                                .is_none_or(|v| v.as_object().is_some_and(|a| keys(a, &[])))
                    })
                })
            });
        }
        if kind == "hardBreak" {
            return keys(node, &["type"]);
        }
        if kind == "horizontalRule"
            || !keys(node, &["type", "content", "attrs"])
            || !attrs(node, kind)
        {
            return false;
        }
        let children = match node.get("content") {
            None => &[][..],
            Some(v) => match v.as_array() {
                Some(a) => a.as_slice(),
                None => return false,
            },
        };
        if ["bulletList", "orderedList", "blockquote", "listItem"].contains(&kind)
            && children.is_empty()
        {
            return false;
        }
        if kind == "listItem"
            && children
                .first()
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str())
                != Some("paragraph")
        {
            return false;
        }
        let allowed = match kind {
            "paragraph" | "heading" => &["text", "hardBreak"][..],
            "codeBlock" => &["text"][..],
            "bulletList" | "orderedList" => &["listItem"][..],
            _ => BLOCKS,
        };
        children
            .iter()
            .all(|child| empty(child, allowed, depth + 1))
    }
    serde_json::from_str::<serde_json::Value>(content).is_ok_and(|value| empty(&value, &["doc"], 0))
}

fn save_journal_entry_with_connection(
    connection: &Connection,
    target: EntryTarget,
    entry_date: String,
    title: String,
    content_json: String,
) -> Result<SavedJournalEntry, String> {
    if matches!(target, EntryTarget::New(_)) && is_definitely_empty_journal_json(&content_json) {
        return Err("journal_empty_entry".into());
    }
    validate_document(&content_json)?;
    // Keep the source check and overwrite in one SQLite transaction.
    let transaction = if connection.is_autocommit() {
        Some(connection.unchecked_transaction().map_err(|e| e.to_string())?)
    } else {
        None
    };
    let id = match target {
        EntryTarget::Existing(id) => {
            let (original, version): (String, i64) = connection
                .query_row(
                    "SELECT content_json, document_version FROM journal_entries WHERE id = ?1",
                    [&id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or("journal_entry_not_found")?;
            if version != 1 {
                return Err("journal_document_incompatible".into());
            }
            validate_document(&original)?;
            connection.execute(
                "UPDATE journal_entries SET entry_date = ?2, title = ?3, content_json = ?4,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1",
                params![id, entry_date, title, content_json],
            ).map_err(|e| e.to_string())?;
            id
        }
        EntryTarget::New(reserved) => {
            let id = reserved.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            connection.execute(
                "INSERT INTO journal_entries (id, entry_date, title, content_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, entry_date, title, content_json],
            ).map_err(|e| e.to_string())?;
            id
        }
    };
    if let Some(transaction) = transaction {
        transaction.commit().map_err(|e| e.to_string())?;
    }
    Ok(SavedJournalEntry { id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    id: String,
    entry_date: String,
    title: String,
    content_json: String,
    document_version: i64,
    editable: bool,
}

fn read_entry(row: &rusqlite::Row<'_>, connection: &Connection) -> rusqlite::Result<JournalEntry> {
    let content_json: String = row.get(3)?;
    let document_version: i64 = row.get(4)?;
    let editable = crate::photographs::document_editable(
        connection,
        &row.get::<_, String>(0)?,
        document_version,
        &content_json,
    );
    Ok(JournalEntry {
        id: row.get(0)?,
        entry_date: row.get(1)?,
        title: row.get(2)?,
        content_json,
        document_version,
        editable,
    })
}

#[tauri::command]
pub fn load_journal_entry(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    entry_date: String,
) -> Result<Snapshot<Option<JournalEntry>>, String> {
    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;

    let data = connection
        .query_row(
            "
            SELECT id, entry_date, title, content_json, document_version
            FROM journal_entries
            WHERE entry_date = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            ",
            params![entry_date],
            |row| read_entry(row, &connection),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(operation.snapshot(data))
}

#[tauri::command]
pub fn get_journal_entry(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    id: String,
) -> Result<Snapshot<Option<JournalEntry>>, String> {
    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;

    let data = connection
        .query_row(
            "
            SELECT id, entry_date, title, content_json, document_version
            FROM journal_entries
            WHERE id = ?1
            ",
            params![id],
            |row| read_entry(row, &connection),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(operation.snapshot(data))
}

/// Full entries for chronological reading; existing summary lists keep their order.
#[tauri::command]
pub fn read_journal_entries(
    app: AppHandle,
    state: State<'_, DatabaseState>,
) -> Result<Snapshot<Vec<JournalEntry>>, String> {
    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;
    let entries = read_journal_entries_with_connection(&connection)?;
    Ok(operation.snapshot(entries))
}

pub(crate) fn read_journal_entries_with_connection(
    connection: &Connection,
) -> Result<Vec<JournalEntry>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, entry_date, title, content_json, document_version
            FROM journal_entries
            ORDER BY entry_date ASC, created_at ASC, id ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    // Preserve the stored document string exactly; reading is not a migration.
    statement
        .query_map([], |row| read_entry(row, connection))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntrySummary {
    id: String,
    entry_date: String,
    title: String,
}

#[tauri::command]
pub fn list_journal_entries(
    app: AppHandle,
    state: State<'_, DatabaseState>,
) -> Result<Snapshot<Vec<JournalEntrySummary>>, String> {
    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, entry_date, title
            FROM journal_entries
            ORDER BY entry_date DESC, created_at DESC, id DESC
            ",
        )
        .map_err(|error| error.to_string())?;

    let entries = statement
        .query_map([], |row| {
            Ok(JournalEntrySummary {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                title: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(operation.snapshot(entries))
}

#[tauri::command]
pub fn list_journal_entries_for_date(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    entry_date: String,
) -> Result<Snapshot<Vec<JournalEntrySummary>>, String> {
    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, entry_date, title
            FROM journal_entries
            WHERE entry_date = ?1
            ORDER BY created_at DESC, id DESC
            ",
        )
        .map_err(|error| error.to_string())?;

    let entries = statement
        .query_map([entry_date], |row| {
            Ok(JournalEntrySummary {
                id: row.get(0)?,
                entry_date: row.get(1)?,
                title: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(operation.snapshot(entries))
}

#[tauri::command]
pub fn list_journal_dates_for_month(
    app: AppHandle,
    state: State<'_, DatabaseState>,
    year: i32,
    month: u32,
) -> Result<Snapshot<Vec<String>>, String> {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return Err("Invalid year or month".to_string());
    }

    let operation = state.operation(None)?;
    let connection = database::open_database(&app)?;

    let month_prefix = format!("{year:04}-{month:02}-%");

    let mut statement = connection
        .prepare(
            "
            SELECT DISTINCT entry_date
            FROM journal_entries
            WHERE entry_date LIKE ?1
            ORDER BY entry_date
            ",
        )
        .map_err(|error| error.to_string())?;

    let dates = statement
        .query_map([month_prefix], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(operation.snapshot(dates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        database::initialize_schema(&connection).unwrap();
        connection
    }

    fn save(
        connection: &Connection,
        id: Option<String>,
        content: &str,
    ) -> Result<SavedJournalEntry, String> {
        save_journal_entry_with_connection(
            connection,
            id.map_or(EntryTarget::New(None), EntryTarget::Existing),
            "2026-10-16".into(),
            "Testanteckning".into(),
            content.into(),
        )
    }

    fn content(connection: &Connection, id: &str) -> String {
        connection
            .query_row(
                "SELECT content_json FROM journal_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn insert_read_entry(
        connection: &Connection,
        id: &str,
        date: &str,
        created: &str,
        content: &str,
    ) {
        connection
            .execute(
                "INSERT INTO journal_entries
                 (id, entry_date, title, content_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, '2026-10-22T12:00:00.000Z')",
                params![id, date, format!("Title {id}"), content, created],
            )
            .unwrap();
    }

    #[test]
    fn read_entries_empty_journal() {
        let connection = test_database();
        assert!(
            read_journal_entries_with_connection(&connection)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn read_entries_orders_by_experience_date_before_creation_date() {
        let connection = test_database();
        for (id, date, created) in [
            ("new", "2026-10-22", "2020-01-01T00:00:00.000Z"),
            ("old", "2020-01-01", "2026-10-22T00:00:00.000Z"),
            ("middle", "2025-06-01", "2021-01-01T00:00:00.000Z"),
        ] {
            insert_read_entry(&connection, id, date, created, "{}");
        }
        let entries = read_journal_entries_with_connection(&connection).unwrap();
        let ids: Vec<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["old", "middle", "new"]);
    }

    #[test]
    fn read_entries_same_date_orders_by_creation_then_id() {
        let connection = test_database();
        for (id, created) in [
            ("b", "2026-10-22T12:00:00.000Z"),
            ("a", "2026-10-22T12:00:00.000Z"),
            ("z", "2026-10-22T11:00:00.000Z"),
        ] {
            insert_read_entry(&connection, id, "2026-10-21", created, "{}");
        }
        for _ in 0..2 {
            let entries = read_journal_entries_with_connection(&connection).unwrap();
            let ids: Vec<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
            assert_eq!(ids, ["z", "a", "b"]);
        }
    }

    #[test]
    fn read_entries_preserves_full_content_and_stored_data() {
        let connection = test_database();
        let document = r#"{
  "type": "doc", "content": [
    { "type": "paragraph", "content": [
      { "type": "text", "text": "Räven 🦊 & <spår>", "marks": [{"type":"bold"}] }
    ] }
  ], "extra": { "number": 1.00, "text": "\u00e5" }
}"#;
        insert_read_entry(
            &connection,
            "entry",
            "2026-10-21",
            "2026-10-20T09:00:00.000Z",
            document,
        );
        let stored = || {
            connection
                .query_row(
                    "SELECT id, entry_date, title, content_json, created_at,
                            updated_at, document_version FROM journal_entries",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .unwrap()
        };
        let before = stored();
        let changes = connection.total_changes();
        connection.execute_batch("PRAGMA query_only=ON").unwrap();
        let entries = read_journal_entries_with_connection(&connection).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.id, "entry");
        assert_eq!(entry.entry_date, "2026-10-21");
        assert_eq!(entry.title, "Title entry");
        assert_eq!(entry.content_json.as_bytes(), document.as_bytes());
        assert_eq!(stored(), before);
        assert_eq!(connection.total_changes(), changes);
    }

    #[test]
    fn empty_document_contract_matches_shared_fixtures() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/journal-empty-documents.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let json = case
                .get("json")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| serde_json::to_string(&case["document"]).unwrap());
            assert_eq!(
                is_definitely_empty_journal_json(&json),
                case["empty"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn empty_new_entries_with_generated_or_missing_explicit_id_never_insert() {
        let connection = test_database();
        let empty = r#"{"type":"doc","content":[{"type":"paragraph"}]}"#;
        assert_eq!(
            save(&connection, None, empty).err().as_deref(),
            Some("journal_empty_entry")
        );
        assert_eq!(
            save(&connection, Some("missing".into()), empty).err().as_deref(),
            Some("journal_entry_not_found")
        );
        let count: i64 = connection
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(connection.total_changes(), 0);
    }

    #[test]
    fn existing_entry_can_be_cleared_without_deletion_and_reads_keep_original_bytes() {
        let connection = test_database();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        let empty = r#"{ "type": "doc", "content": [{ "type": "paragraph" }] }"#;
        save(&connection, Some(saved.id.clone()), empty).unwrap();
        let entries = read_journal_entries_with_connection(&connection).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, saved.id);
        assert_eq!(entries[0].content_json, empty);
    }

    #[test]
    fn future_and_uncertain_content_cannot_be_created_by_the_editor() {
        let connection = test_database();
        for content in [
            r#"{"type":"doc","content":[{"type":"image","attrs":{"src":"photo.jpg"}}]}"#,
            r#"{"type":"doc","content":[{"type":"future"}]}"#,
            "{}",
        ] {
            assert_eq!(
                save(&connection, None, content).err().as_deref(),
                Some("journal_document_incompatible")
            );
        }
        assert_eq!(connection.total_changes(), 0);
    }

    #[test]
    fn empty_new_session_write_is_rejected_before_opening_database() {
        let state = DatabaseState::ready_for_test();
        let result = save_in_session(
            &state,
            1,
            || panic!("must not open"),
            EntryTarget::New(None),
            "2026-10-21".into(),
            String::new(),
            r#"{"type":"doc","content":[]}"#.into(),
        );
        assert_eq!(result.err().as_deref(), Some("journal_empty_entry"));
    }

    #[test]
    fn saves_and_updates_same_entry() {
        let connection = test_database();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        uuid::Uuid::parse_str(&saved.id).unwrap();
        let created: String = connection
            .query_row(
                "SELECT created_at FROM journal_entries WHERE id = ?1",
                [&saved.id],
                |row| row.get(0),
            )
            .unwrap();
        let updated = save(&connection, Some(saved.id.clone()), r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Updated"}]}]}"#).unwrap();
        assert_eq!(saved.id, updated.id);
        assert_eq!(
            content(&connection, &saved.id),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Updated"}]}]}"#
        );
        let (count, created_after, version): (i64, String, i64) = connection
            .query_row(
                "SELECT count(*), created_at, document_version FROM journal_entries",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(created, created_after);
        assert_eq!(version, 1);
    }

    #[test]
    fn invalid_content_preserves_saved_entry() {
        let connection = test_database();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        for invalid in ["invalid JSON", "[]", "null", "42", "\"text\""] {
            assert!(save(&connection, Some(saved.id.clone()), invalid).is_err());
            assert_eq!(
                content(&connection, &saved.id),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#
            );
        }
    }

    #[test]
    fn failed_write_preserves_saved_entry() {
        let connection = test_database();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_update BEFORE UPDATE ON journal_entries
             BEGIN SELECT RAISE(ABORT, 'test write failure'); END;",
            )
            .unwrap();
        let error = save(
            &connection,
            Some(saved.id.clone()),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Rejected"}]}]}"#,
        )
        .err()
        .unwrap();
        assert!(error.contains("test write failure"));
        assert_eq!(
            content(&connection, &saved.id),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#
        );
    }

    #[test]
    fn unfinished_save_transaction_preserves_saved_entry() {
        let mut connection = test_database();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        {
            let transaction = connection.transaction().unwrap();
            save(
                &transaction,
                Some(saved.id.clone()),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Uncommitted"}]}]}"#,
            )
            .unwrap();
            assert_eq!(
                content(&transaction, &saved.id),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Uncommitted"}]}]}"#
            );
        }
        assert_eq!(
            content(&connection, &saved.id),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#
        );
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
    #[test]
    fn session_writes_reject_old_generations_without_opening_or_changing_database() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("journal.sqlite");
        let state = DatabaseState::ready_for_test();
        let generation = state.operation(None).unwrap().snapshot(()).generation;
        let connection = Connection::open(&path).unwrap();
        database::initialize_schema(&connection).unwrap();
        drop(connection);
        let saved = save_in_session(
            &state,
            generation,
            || Connection::open(&path).map_err(|e| e.to_string()),
            EntryTarget::New(None),
            "2026-10-17".into(),
            "Test".into(),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        state.maintenance().unwrap().advance_generation().unwrap();
        for _ in 0..3 {
            let result = save_in_session(
                &state,
                generation,
                || panic!("stale save must not open database"),
                EntryTarget::Existing(saved.id.clone()),
                "2026-10-17".into(),
                "Old session".into(),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
            );
            assert_eq!(
                result.err().unwrap(),
                crate::database_state::GENERATION_CONFLICT
            );
        }
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let operation = state.operation(None).unwrap();
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            content(&connection, &saved.id),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#
        );
        assert_eq!(operation.snapshot(()).generation, generation + 1);
        drop(connection);
        drop(operation);
        let updated = save_in_session(
            &state,
            generation + 1,
            || Connection::open(&path).map_err(|e| e.to_string()),
            EntryTarget::Existing(saved.id.clone()),
            "2026-10-17".into(),
            "Current session".into(),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"new"}]}]}"#.into(),
        )
        .unwrap();
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            content(&connection, &updated.id),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"new"}]}]}"#
        );
    }
    #[test]
    fn queued_real_save_never_opens_database_after_generation_change() {
        let state = std::sync::Arc::new(DatabaseState::ready_for_test());
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("journal.sqlite");
        let connection = Connection::open(&path).unwrap();
        database::initialize_schema(&connection).unwrap();
        let saved = save(&connection, None, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#).unwrap();
        drop(connection);
        let original = std::fs::read(&path).unwrap();
        let mut maintenance = state.maintenance().unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        state.on_blocked(move || ready_tx.send(()).unwrap());
        let other = state.clone();
        let worker = std::thread::spawn(move || {
            save_in_session(
                &other,
                1,
                || panic!("queued stale save must not open database"),
                EntryTarget::Existing(saved.id),
                "2026-10-17".into(),
                "Stale".into(),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
            )
            .err()
            .unwrap()
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        maintenance.advance_generation().unwrap();
        drop(maintenance);
        assert_eq!(
            worker.join().unwrap(),
            crate::database_state::GENERATION_CONFLICT
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[cfg(test)]
mod startup_gate_tests {
    use super::*;
    #[test]
    fn startup_blocks_real_save_before_database_open() {
        let state = DatabaseState::default();
        let result = save_in_session(
            &state,
            1,
            || panic!("database must not open"),
            EntryTarget::New(None),
            "2026-10-17".into(),
            "title".into(),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
        );
        assert!(matches!(result,Err(ref e) if e=="database_startup_blocked"));
    }
}

// Uses the real generation/operation permit and save path; only the fixture
// location is supplied by the isolated startup+backup regression test.
#[cfg(test)]
pub(crate) fn save_started_fixture(
    state: &DatabaseState,
    path: &std::path::Path,
) -> Result<(), String> {
    let generation = state.operation(None)?.snapshot(()).generation;
    save_in_session(state,generation,||Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|e|e.to_string()),EntryTarget::New(None),"2026-10-17".into(),"Saved after startup".into(),r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Säker journal"}]}]}"#.into()).map(|_|())
}

#[cfg(test)]
mod normal_open_contract_tests {
    use super::*;
    use std::cell::Cell;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, DatabaseState) {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("normal.sqlite");
        let connection = Connection::open(&path).unwrap();
        database::initialize_schema(&connection).unwrap();
        connection.close().unwrap();
        (directory, path, DatabaseState::ready_for_test())
    }

    #[test]
    fn normal_open_has_operation_before_and_during_real_sqlite_use() {
        let (_directory, path, state) = fixture();
        let hit = Cell::new(false);
        let saved = save_in_session(
            &state,
            1,
            || {
                state.assert_normal_operation_held_for_test();
                let connection =
                    Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
                        .map_err(|e| e.to_string())?;
                assert_eq!(
                    connection
                        .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
                state.assert_normal_operation_held_for_test();
                hit.set(true);
                Ok(connection)
            },
            EntryTarget::New(None),
            "2026-10-20".into(),
            "Normal".into(),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
        )
        .unwrap();
        assert!(hit.get());
        // The actual save consumed the returned connection locally; its permit
        // is released when save_in_session returns, so Maintenance can acquire.
        let maintenance = state.maintenance().unwrap();
        assert_eq!(maintenance.generation(), 1);
        let connection =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT title FROM journal_entries WHERE id=?1",
                    [&saved.id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "Normal"
        );
        connection.close().unwrap();
        drop(maintenance);
    }

    #[test]
    fn normal_open_failure_releases_operation_without_creating_database() {
        let (directory, _path, state) = fixture();
        let missing = directory.path().join("missing.sqlite");
        let hit = Cell::new(false);
        let result = save_in_session(
            &state,
            1,
            || {
                state.assert_normal_operation_held_for_test();
                hit.set(true);
                Connection::open_with_flags(&missing, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
                    .map_err(|e| {
                        assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::CannotOpen));
                        "expected normal open failure".to_owned()
                    })
            },
            EntryTarget::New(None),
            "2026-10-20".into(),
            "Normal".into(),
            r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
        );
        assert!(hit.get());
        assert_eq!(
            result.err().as_deref(),
            Some("expected normal open failure")
        );
        assert!(!missing.exists());
        assert_eq!(state.maintenance().unwrap().generation(), 1);
    }

    #[test]
    fn normal_open_panic_after_real_open_unwinds_operation() {
        let (_directory, path, state) = fixture();
        let hit = Cell::new(false);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            save_in_session(
                &state,
                1,
                || {
                    state.assert_normal_operation_held_for_test();
                    let connection = Connection::open_with_flags(
                        &path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                    )
                    .unwrap();
                    assert_eq!(
                        connection
                            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                                .get::<_, i64>(0))
                            .unwrap(),
                        0
                    );
                    state.assert_normal_operation_held_for_test();
                    hit.set(true);
                    panic!("normal open unwind");
                },
                EntryTarget::New(None),
                "2026-10-20".into(),
                "Normal".into(),
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original"}]}]}"#.into(),
            )
        }));
        assert!(hit.get());
        let payload = match caught {
            Err(payload) => payload,
            Ok(_) => panic!("expected normal open panic"),
        };
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"normal open unwind"));
        assert_eq!(state.maintenance().unwrap().generation(), 1);
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    const VALID: &str = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Original åäö"}]}]}"#;
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        database::initialize_schema(&db).unwrap();
        db
    }
    #[test]
    fn incompatible_sources_cannot_be_overwritten_even_by_valid_direct_save() {
        for (version, original) in [
            (1, "{}"),
            (
                1,
                r#"{"type":"doc","content":[{"type":"image","attrs":{"src":"external.jpg"}}]}"#,
            ),
            (
                1,
                r#"{"type":"doc","content":[{"type":"paragraph","attrs":{"sourceNote":"keep"}}]}"#,
            ),
            (
                1,
                r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"first","text":"second"}]}]}"#,
            ),
            (3, VALID),
        ] {
            let db = database();
            db.execute(
                "INSERT INTO journal_entries VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    "protected",
                    "2026-10-25",
                    "Keep title",
                    original,
                    version,
                    "created",
                    "updated"
                ],
            )
            .unwrap();
            let changes = db.total_changes();
            let entries = read_journal_entries_with_connection(&db).unwrap();
            assert!(!entries[0].editable);
            assert_eq!(entries[0].document_version, version);
            assert_eq!(entries[0].content_json.as_bytes(), original.as_bytes());
            for replacement in [VALID, original] {
                let error = save_journal_entry_with_connection(
                    &db,
                    EntryTarget::Existing("protected".into()),
                    "2000-01-01".into(),
                    "Changed title".into(),
                    replacement.into(),
                )
                .err()
                .unwrap();
                assert_eq!(error, "journal_document_incompatible");
                assert!(db.is_autocommit());
                assert_eq!(db.total_changes(), changes);
                let row: (String, String, String, i64, String, String) = db.query_row(
                    "SELECT entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries WHERE id='protected'", [],
                    |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
                assert_eq!(
                    row,
                    (
                        "2026-10-25".into(),
                        "Keep title".into(),
                        original.into(),
                        version,
                        "created".into(),
                        "updated".into()
                    )
                );
            }
        }
    }
    #[test]
    fn current_documents_and_default_attributes_remain_editable() {
        let db = database();
        let id = save_journal_entry_with_connection(
            &db,
            EntryTarget::New(None),
            "2026-10-25".into(),
            "Title".into(),
            VALID.into(),
        )
        .unwrap()
        .id;
        assert!(read_journal_entries_with_connection(&db).unwrap()[0].editable);
        let changed = r#"{ "type":"doc", "content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"New"}]}]}"#;
        save_journal_entry_with_connection(
            &db,
            EntryTarget::Existing(id),
            "2026-10-26".into(),
            "New".into(),
            changed.into(),
        )
        .unwrap();
        let entry = read_journal_entries_with_connection(&db).unwrap().remove(0);
        assert!(entry.editable);
        assert_eq!(entry.content_json, changed);
        assert_eq!(entry.entry_date, "2026-10-26");
    }
}
