//! Isolated format 2 / schema 1 experiment. Never an admission capability.
use super::{Error as ValidationError, validate_snapshot};
use rusqlite::{Connection, TransactionBehavior, types::ValueRef};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

const CONTENT_VERSION: u32 = 2;
const SCHEMA_VERSION: u32 = 1;
const DOMAIN: &[u8] = b"MNO-database-content\0";
const JOURNAL_FIELDS: &[&str] = &[
    "id",
    "entry_date",
    "title",
    "content_json",
    "document_version",
    "created_at",
    "updated_at",
];
const WEATHER_FIELDS: &[&str] = &[
    "entry_id",
    "format_version",
    "manual_json",
    "fetched_json",
    "revision",
];
const JOURNAL_ROWS: &str = "SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries INDEXED BY sqlite_autoindex_journal_entries_1 ORDER BY id COLLATE BINARY ASC";
const WEATHER_ROWS: &str = "SELECT entry_id,format_version,manual_json,fetched_json,revision FROM main.journal_weather INDEXED BY sqlite_autoindex_journal_weather_1 ORDER BY entry_id COLLATE BINARY ASC";

#[derive(Debug)]
enum Error {
    Validation(ValidationError),
    Sql(rusqlite::Error),
    ExistingTransaction,
    Cancelled,
    Overflow,
    UnsafeSort,
    SnapshotLost,
}
type Result<T> = std::result::Result<T, Error>;
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}
#[derive(Debug, PartialEq, Eq)]
struct ExperimentalFingerprint {
    content_version: u32,
    schema_version: u32,
    digest: [u8; 32],
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn lp32(hash: &mut Sha256, value: &str) -> Result<()> {
    hash.update(
        u32::try_from(value.len())
            .map_err(|_| Error::Overflow)?
            .to_be_bytes(),
    );
    hash.update(value.as_bytes());
    Ok(())
}
fn bytes(hash: &mut Sha256, value: &[u8], cancel: &AtomicBool) -> Result<()> {
    hash.update(
        u64::try_from(value.len())
            .map_err(|_| Error::Overflow)?
            .to_be_bytes(),
    );
    for chunk in value.chunks(64 * 1024) {
        check(cancel)?;
        hash.update(chunk);
    }
    Ok(())
}
fn encode(hash: &mut Sha256, value: ValueRef<'_>, cancel: &AtomicBool) -> Result<()> {
    check(cancel)?;
    match value {
        ValueRef::Null => hash.update([0]),
        ValueRef::Integer(n) => {
            hash.update([1]);
            hash.update(n.to_be_bytes());
        }
        ValueRef::Real(n) => {
            hash.update([2]);
            hash.update(n.to_bits().to_be_bytes());
        }
        ValueRef::Text(value) => {
            hash.update([3]);
            bytes(hash, value, cancel)?;
        }
        ValueRef::Blob(value) => {
            hash.update([4]);
            bytes(hash, value, cancel)?;
        }
    }
    Ok(())
}
// Fixed PK-index scans, checked against the actual SQLite bytecode. Do not sort
// or copy complete JSON rows into a temporary b-tree if the planner changes.
fn require_streaming_order(connection: &Connection, query: &str) -> Result<()> {
    let mut statement = connection.prepare(&format!("EXPLAIN {query}"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let opcode: String = row.get(1)?;
        if matches!(
            opcode.as_str(),
            "SorterOpen" | "SorterInsert" | "OpenEphemeral" | "Sort" | "SorterSort"
        ) {
            return Err(Error::UnsafeSort);
        }
    }
    Ok(())
}
fn table(
    connection: &Connection,
    hash: &mut Sha256,
    name: &str,
    fields: &[&str],
    query: &str,
    cancel: &AtomicBool,
) -> Result<()> {
    require_streaming_order(connection, query)?;
    hash.update([0x54]);
    lp32(hash, name)?;
    hash.update(
        u32::try_from(fields.len())
            .map_err(|_| Error::Overflow)?
            .to_be_bytes(),
    );
    for field in fields {
        lp32(hash, field)?;
    }
    let mut statement = connection.prepare(query)?;
    let mut rows = statement.query([])?;
    let mut count = 0u64;
    while let Some(row) = rows.next()? {
        check(cancel)?;
        hash.update([0x52]);
        for index in 0..fields.len() {
            encode(hash, row.get_ref(index)?, cancel)?;
        }
        count = count.checked_add(1).ok_or(Error::Overflow)?;
    }
    hash.update([0x45]);
    hash.update(count.to_be_bytes());
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Validated,
    JournalHashed,
    Hashed,
}
fn fingerprint(
    connection: &mut Connection,
    cancel: &AtomicBool,
) -> Result<ExperimentalFingerprint> {
    fingerprint_with(connection, cancel, |_, _| Ok(()))
}
// Private diagnostic checkpoints are confined to this already test-only module.
fn fingerprint_with(
    connection: &mut Connection,
    cancel: &AtomicBool,
    mut checkpoint: impl FnMut(Point, &Connection) -> Result<()>,
) -> Result<ExperimentalFingerprint> {
    if !connection.is_autocommit() {
        return Err(Error::ExistingTransaction);
    }
    check(cancel)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let outcome = (|| {
        validate_snapshot(&transaction).map_err(Error::Validation)?;
        check(cancel)?;
        checkpoint(Point::Validated, &transaction)?;
        if transaction.is_autocommit() {
            return Err(Error::SnapshotLost);
        }
        let mut hash = Sha256::new();
        hash.update(DOMAIN);
        hash.update(CONTENT_VERSION.to_be_bytes());
        hash.update(SCHEMA_VERSION.to_be_bytes());
        hash.update(2u32.to_be_bytes());
        table(
            &transaction,
            &mut hash,
            "journal_entries",
            JOURNAL_FIELDS,
            JOURNAL_ROWS,
            cancel,
        )?;
        checkpoint(Point::JournalHashed, &transaction)?;
        if transaction.is_autocommit() {
            return Err(Error::SnapshotLost);
        }
        table(
            &transaction,
            &mut hash,
            "journal_weather",
            WEATHER_FIELDS,
            WEATHER_ROWS,
            cancel,
        )?;
        hash.update([0x5a]);
        checkpoint(Point::Hashed, &transaction)?;
        if transaction.is_autocommit() {
            return Err(Error::SnapshotLost);
        }
        check(cancel)?;
        Ok(ExperimentalFingerprint {
            content_version: CONTENT_VERSION,
            schema_version: SCHEMA_VERSION,
            digest: hash.finalize().into(),
        })
    })();
    // No result escapes until explicit read-transaction termination succeeds.
    // On unwind rusqlite's Transaction defaults to rollback; no commit path.
    // Persistent I/O failures or a hostile caller-owned SQLite callback can
    // prevent even rollback. The error is not a reusable-connection guarantee;
    // no fingerprint escapes in that case. We do not replace caller callbacks.
    transaction.rollback()?;
    check(cancel)?;
    outcome
}

#[path = "content/tests.rs"]
mod tests;
