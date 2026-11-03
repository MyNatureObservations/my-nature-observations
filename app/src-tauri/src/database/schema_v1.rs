//! Test-only schema definition/validator and explicit 0 -> 1 migration.
//! No path acquisition, admission token, content proof or production entry point.
use rusqlite::{Connection, TransactionBehavior, types::ValueRef};

const JOURNAL_DDL: &str = "
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
            ";

const WEATHER_DDL: &str = "CREATE TABLE journal_weather (
    entry_id TEXT PRIMARY KEY NOT NULL
        REFERENCES journal_entries(id) ON DELETE RESTRICT,
    format_version INTEGER NOT NULL CHECK (format_version = 1),
    manual_json TEXT,
    fetched_json TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1)
);";
const WEATHER_PART_LIMIT: usize = 64 * 1024;
const JOURNAL_JSON_LIMIT: usize = crate::backup::restore::SCHEMA_TEST_DOCUMENT_LIMIT as usize;
const ENTRY_ID_LIMIT: usize = 16 * 1024;

#[derive(Debug)]
enum Error {
    Sql(rusqlite::Error),
    Schema,
    Content,
    ResourceLimit,
    ForeignKeysDisabled,
    UnsupportedEncoding,
    UnsafeMetadataPlan,
}
type Result<T> = std::result::Result<T, Error>;
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

// Independent frozen schema-1 definition, including the unchanged journal DDL.
fn define_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(JOURNAL_DDL)?;
    connection.execute_batch(WEATHER_DDL)?;
    connection.pragma_update(Some("main"), "user_version", 1)?;
    Ok(())
}

type Object = (String, String, String, Option<String>);
fn objects(connection: &Connection) -> Result<Vec<Object>> {
    // Exactly five objects, including both PK autoindices; sixth rejects extras.
    let mut statement =
        connection.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema LIMIT 6")?;
    let mut result = statement
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?
                    .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    result.sort();
    Ok(result)
}
fn text(value: ValueRef<'_>, limit: usize) -> Result<&str> {
    match value {
        ValueRef::Text(bytes) if bytes.len() > limit => Err(Error::ResourceLimit),
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).map_err(|_| Error::Content),
        _ => Err(Error::Content),
    }
}
fn part(value: ValueRef<'_>) -> Result<Option<&str>> {
    match value {
        ValueRef::Null => Ok(None),
        value => text(value, WEATHER_PART_LIMIT).map(Some),
    }
}

// Each expression is a direct column argument: SQLite's OP_Column uses
// TYPEOFARG (0x80) or BYTELENARG (0xc0), without reading overflow payloads.
// No CAST, concatenation, text length(), expression or full value in this pass.
const JOURNAL_METADATA: &str = "SELECT typeof(id),octet_length(id),typeof(entry_date),octet_length(entry_date),typeof(title),octet_length(title),typeof(content_json),octet_length(content_json),typeof(document_version),octet_length(document_version),typeof(created_at),octet_length(created_at),typeof(updated_at),octet_length(updated_at) FROM main.journal_entries";
const WEATHER_METADATA: &str = "SELECT typeof(entry_id),octet_length(entry_id),typeof(format_version),octet_length(format_version),typeof(manual_json),octet_length(manual_json),typeof(fetched_json),octet_length(fetched_json),typeof(revision),octet_length(revision) FROM main.journal_weather";

fn require_utf8(connection: &Connection) -> Result<()> {
    let encoding: String = connection.pragma_query_value(Some("main"), "encoding", |r| r.get(0))?;
    if encoding != "UTF-8" {
        return Err(Error::UnsupportedEncoding);
    }
    // Fails closed if the bundled function is unavailable or not byte-counting.
    let bytes: i64 = connection.query_row("SELECT octet_length('é🌿')", [], |r| r.get(0))?;
    if bytes != 6 {
        return Err(Error::UnsafeMetadataPlan);
    }
    Ok(())
}
fn require_metadata_plan(connection: &Connection, query: &str) -> Result<()> {
    let mut statement = connection.prepare(&format!("EXPLAIN {query}"))?;
    let mut rows = statement.query([])?;
    let mut columns = 0;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == "Column" {
            let flags: i64 = row.get(6)?;
            if !matches!(flags & 0xc0, 0x80 | 0xc0) {
                return Err(Error::UnsafeMetadataPlan);
            }
            columns += 1;
        }
    }
    if columns == 0 {
        return Err(Error::UnsafeMetadataPlan);
    }
    Ok(())
}
fn check_metadata(
    connection: &Connection,
    query: &str,
    columns: &[(&str, usize, bool)],
) -> Result<()> {
    require_metadata_plan(connection, query)?;
    let mut statement = connection.prepare(query)?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        for (column, (expected, limit, nullable)) in columns.iter().enumerate() {
            let kind: String = row.get(2 * column)?;
            if *nullable && kind == "null" {
                if row.get::<_, Option<i64>>(2 * column + 1)?.is_some() {
                    return Err(Error::Content);
                }
                continue;
            }
            if kind != *expected {
                return Err(Error::Content);
            }
            let bytes: i64 = row.get(2 * column + 1)?;
            if bytes < 0 || bytes as u64 > *limit as u64 {
                return Err(Error::ResourceLimit);
            }
        }
    }
    Ok(())
}
fn precheck_payloads(connection: &Connection) -> Result<()> {
    require_utf8(connection)?;
    check_metadata(
        connection,
        JOURNAL_METADATA,
        &[
            ("text", ENTRY_ID_LIMIT, false),
            ("text", JOURNAL_JSON_LIMIT, false),
            ("text", JOURNAL_JSON_LIMIT, false),
            ("text", JOURNAL_JSON_LIMIT, false),
            ("integer", 20, false),
            ("text", JOURNAL_JSON_LIMIT, false),
            ("text", JOURNAL_JSON_LIMIT, false),
        ],
    )?;
    check_metadata(
        connection,
        WEATHER_METADATA,
        &[
            ("text", ENTRY_ID_LIMIT, false),
            ("integer", 20, false),
            ("text", WEATHER_PART_LIMIT, true),
            ("text", WEATHER_PART_LIMIT, true),
            ("integer", 20, false),
        ],
    )
}

// Caller must hold a transaction; all observations use the same SQLite snapshot.
fn validate_snapshot(connection: &Connection) -> Result<()> {
    if connection.is_autocommit()
        || connection.pragma_query_value(Some("main"), "user_version", |r| r.get::<_, i64>(0))? != 1
    {
        return Err(Error::Schema);
    }
    require_utf8(connection)?;
    let reference = Connection::open_in_memory()?;
    define_schema(&reference)?;
    if objects(connection)? != objects(&reference)? {
        return Err(Error::Schema);
    }
    // Finish metadata-only type/size scans before integrity checks or payload reads.
    precheck_payloads(connection)?;
    // Comparing the complete normalized CREATE statements above freezes columns,
    // constraints, FK actions and indices. Integrity/FK scans also inspect data.
    let mut check = connection.prepare("PRAGMA main.integrity_check")?;
    let results = check
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if results != ["ok"] {
        return Err(Error::Content);
    }
    let mut foreign = connection.prepare("PRAGMA main.foreign_key_check")?;
    if foreign.query([])?.next()?.is_some() {
        return Err(Error::Content);
    }

    let mut statement = connection.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = text(row.get_ref(0)?, ENTRY_ID_LIMIT)?;
        for column in [1, 2, 5, 6] {
            text(row.get_ref(column)?, JOURNAL_JSON_LIMIT)?;
        }
        if !matches!(row.get_ref(4)?, ValueRef::Integer(1)) {
            return Err(Error::Content);
        }
        let json = text(row.get_ref(3)?, JOURNAL_JSON_LIMIT)?;
        let date = text(row.get_ref(1)?, JOURNAL_JSON_LIMIT)?;
        crate::backup::restore::validate_journal_for_schema_test(id, date, json).map_err(
            |error| {
                if error == crate::backup::restore::PrepareError::ResourceLimit {
                    Error::ResourceLimit
                } else {
                    Error::Content
                }
            },
        )?;
    }
    // The legacy nullable TEXT PRIMARY KEY permits NULL; validate IDs explicitly
    // and check uniqueness independently of the implicit index.
    if connection.query_row(
        "SELECT EXISTS(SELECT id FROM main.journal_entries GROUP BY id HAVING COUNT(*) > 1)",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::Content);
    }
    let mut statement = connection.prepare("SELECT entry_id,format_version,manual_json,fetched_json,revision FROM main.journal_weather")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let entry = text(row.get_ref(0)?, ENTRY_ID_LIMIT)?;
        if !matches!(row.get_ref(1)?, ValueRef::Integer(1))
            || !matches!(row.get_ref(4)?, ValueRef::Integer(n) if n >= 1)
        {
            return Err(Error::Content);
        }
        crate::weather::validate_stored_parts(
            entry,
            part(row.get_ref(2)?)?,
            part(row.get_ref(3)?)?,
        )
        .map_err(|_| Error::Content)?;
    }
    Ok(())
}

// A read transaction is rolled back, never committed or used to repair anything.
fn validate(connection: &mut Connection) -> Result<()> {
    let snapshot = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let result = validate_snapshot(&snapshot);
    snapshot.rollback()?;
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    AfterBegin,
    AfterTable,
    AfterVersion,
    BeforeCommit,
}
fn migrate(connection: &mut Connection) -> Result<()> {
    migrate_with(connection, |_, _| Ok(()))
}
fn migrate_with(
    connection: &mut Connection,
    mut checkpoint: impl FnMut(Point, &Connection) -> Result<()>,
) -> Result<()> {
    if !connection.is_autocommit() {
        return Err(Error::Schema);
    }
    if connection.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i64>(0))? != 1 {
        return Err(Error::ForeignKeysDisabled);
    }
    require_utf8(connection)?;
    if !crate::database::validate_schema(connection)? {
        return Err(Error::Schema);
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcome = (|| {
        checkpoint(Point::AfterBegin, &tx)?;
        if !crate::database::validate_schema(&tx)? {
            return Err(Error::Schema);
        }
        tx.execute_batch(WEATHER_DDL)?;
        checkpoint(Point::AfterTable, &tx)?;
        tx.pragma_update(Some("main"), "user_version", 1)?;
        checkpoint(Point::AfterVersion, &tx)?;
        validate_snapshot(&tx)?;
        checkpoint(Point::BeforeCommit, &tx)?;
        // Diagnostics cannot silently invalidate the final checked state.
        validate_snapshot(&tx)?;
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            tx.commit()?;
            Ok(())
        }
        Err(error) => {
            tx.rollback()?;
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "schema_v1/tests.rs"]
mod tests;

// Experimental fingerprint has no production registration or authority.
#[cfg(test)]
#[path = "schema_v1/content.rs"]
mod content;
