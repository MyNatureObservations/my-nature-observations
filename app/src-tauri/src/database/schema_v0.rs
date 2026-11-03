// Frozen schema-0 definition, also used by the read-only validator's in-memory reference.
// A future creation version must not silently change this legacy reference.
pub(crate) fn initialize_schema(connection: &rusqlite::Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS journal_entries (
                id TEXT PRIMARY KEY,
                entry_date TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                content_json TEXT NOT NULL,
                document_version INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_journal_entries_date
                ON journal_entries(entry_date);
            ",
        )
        .map_err(|error| error.to_string())?;

    Ok(())
}
