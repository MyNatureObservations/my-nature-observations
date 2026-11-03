//! Shared strict verification plus exact, layout-independent journal content.
use super::restore::{self, PrepareError, PreparedRestore};
use rusqlite::{Connection, TransactionState, types::ValueRef};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentError {
    Database(PrepareError),
    WriteTransaction,
    SnapshotLost,
}
type Result<T> = std::result::Result<T, ContentError>;
impl From<PrepareError> for ContentError {
    fn from(e: PrepareError) -> Self {
        Self::Database(e)
    }
}
fn sql(e: rusqlite::Error) -> ContentError {
    restore::sql_error(e).into()
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(PrepareError::Cancelled.into())
    } else {
        Ok(())
    }
}

// Dedicated connections: this verifier owns their temporary progress handler and
// tightens resource/configuration settings. It never changes their authorizer.
// An exclusive Rust borrow prevents the caller from writing/releasing the snapshot.
pub(crate) struct VerifiedJournal<'a> {
    database: &'a Connection,
    own_transaction: bool,
    own_handler: bool,
    candidate: Option<&'a PreparedRestore>,
    cancel: Arc<AtomicBool>,
}
impl<'a> VerifiedJournal<'a> {
    pub fn verify(database: &'a mut Connection, cancel: Arc<AtomicBool>) -> Result<Self> {
        cancelled(&cancel)?;
        if database.transaction_state(None::<&str>).map_err(sql)? == TransactionState::Write {
            return Err(ContentError::WriteTransaction);
        }
        let mut snapshot = Self {
            database,
            own_transaction: false,
            own_handler: true,
            candidate: None,
            cancel,
        };
        restore::harden_open_connection(snapshot.database, snapshot.cancel.clone())?;
        if snapshot.database.is_autocommit() {
            snapshot
                .database
                .execute_batch("BEGIN DEFERRED")
                .map_err(sql)?;
            snapshot.own_transaction = true;
        }
        // integrity_check establishes the main-database snapshot before schema/row validation.
        restore::validate_database(snapshot.database, &snapshot.cancel)?;
        snapshot.ensure()?;
        Ok(snapshot)
    }
    pub fn from_candidate(candidate: &'a PreparedRestore, cancel: Arc<AtomicBool>) -> Result<Self> {
        cancelled(&cancel)?;
        let database = candidate.source()?;
        let snapshot = Self {
            database,
            own_transaction: false,
            own_handler: false,
            candidate: Some(candidate),
            cancel,
        };
        snapshot.ensure()?;
        Ok(snapshot)
    }
    fn ensure(&self) -> Result<()> {
        cancelled(&self.cancel)?;
        if self.database.is_autocommit() {
            return Err(ContentError::SnapshotLost);
        }
        if let Some(candidate) = self.candidate {
            candidate.source()?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<()> {
        if self.own_handler {
            self.database
                .progress_handler(0, None::<fn() -> bool>)
                .map_err(sql)?;
            self.own_handler = false;
        }
        if self.own_transaction {
            self.database.execute_batch("ROLLBACK").map_err(sql)?;
            self.own_transaction = false;
        }
        Ok(())
    }
}
impl Drop for VerifiedJournal<'_> {
    fn drop(&mut self) {
        if self.own_handler {
            let _ = self.database.progress_handler(0, None::<fn() -> bool>);
        }
        if self.own_transaction {
            let _ = self.database.execute_batch("ROLLBACK");
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comparison {
    Identical,
    Different,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ContentFingerprint {
    pub version: u32,
    pub sha256: String,
}

// Fixed protocol identity; DOMAIN and its independently encoded version bytes stay frozen.
pub(super) const CONTENT_FINGERPRINT_VERSION_V1: u32 = 1;

// Strict verification requires unique, non-NULL TEXT IDs; BINARY UTF-8 order is stable.
// TEMP tables must never shadow the verified main database.
const ORDERED_ROWS: &str = "SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY";
const DOMAIN:&[u8]=b"MNO-journal-content\0\x00\x00\x00\x01journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0";

pub(crate) fn compare(
    left: &VerifiedJournal<'_>,
    right: &VerifiedJournal<'_>,
) -> Result<Comparison> {
    left.ensure()?;
    right.ensure()?;
    let result = compare_rows(left.database, right.database, &left.cancel, &right.cancel);
    cancelled(&left.cancel)?;
    cancelled(&right.cancel)?;
    let result = result?;
    left.ensure()?;
    right.ensure()?;
    Ok(result)
}
pub(crate) fn fingerprint(snapshot: &VerifiedJournal<'_>) -> Result<ContentFingerprint> {
    snapshot.ensure()?;
    let sha256 = fingerprint_rows(snapshot.database, &snapshot.cancel);
    cancelled(&snapshot.cancel)?;
    let sha256 = sha256?;
    snapshot.ensure()?;
    Ok(ContentFingerprint {
        version: CONTENT_FINGERPRINT_VERSION_V1,
        sha256,
    })
}
fn compare_rows(
    left: &Connection,
    right: &Connection,
    left_cancel: &AtomicBool,
    right_cancel: &AtomicBool,
) -> Result<Comparison> {
    let mut a = left.prepare(ORDERED_ROWS).map_err(sql)?;
    let mut b = right.prepare(ORDERED_ROWS).map_err(sql)?;
    let mut a = a.query([]).map_err(sql)?;
    let mut b = b.query([]).map_err(sql)?;
    loop {
        cancelled(left_cancel)?;
        cancelled(right_cancel)?;
        match (a.next().map_err(sql)?, b.next().map_err(sql)?) {
            (None, None) => return Ok(Comparison::Identical),
            (Some(a), Some(b)) => {
                for column in 0..7 {
                    if !same_value(
                        a.get_ref(column).map_err(sql)?,
                        b.get_ref(column).map_err(sql)?,
                    ) {
                        return Ok(Comparison::Different);
                    }
                }
            }
            _ => return Ok(Comparison::Different),
        }
    }
}
fn same_value(a: ValueRef<'_>, b: ValueRef<'_>) -> bool {
    match (a, b) {
        (ValueRef::Null, ValueRef::Null) => true,
        (ValueRef::Integer(a), ValueRef::Integer(b)) => a == b,
        (ValueRef::Real(a), ValueRef::Real(b)) => a.to_bits() == b.to_bits(),
        (ValueRef::Text(a), ValueRef::Text(b)) | (ValueRef::Blob(a), ValueRef::Blob(b)) => a == b,
        _ => false,
    }
}
fn fingerprint_rows(database: &Connection, cancel: &AtomicBool) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    let mut statement = database.prepare(ORDERED_ROWS).map_err(sql)?;
    let mut rows = statement.query([]).map_err(sql)?;
    let mut count = 0u64;
    while let Some(row) = rows.next().map_err(sql)? {
        cancelled(cancel)?;
        hash.update([0x52]);
        for column in 0..7 {
            encode(&mut hash, row.get_ref(column).map_err(sql)?);
        }
        count = count
            .checked_add(1)
            .ok_or(ContentError::Database(PrepareError::ResourceLimit))?;
    }
    cancelled(cancel)?;
    hash.update([0x45]);
    hash.update(count.to_be_bytes());
    Ok(format!("{:x}", hash.finalize()))
}
fn encode(hash: &mut Sha256, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => hash.update([0]),
        ValueRef::Integer(value) => {
            hash.update([1]);
            hash.update(value.to_be_bytes());
        }
        ValueRef::Real(value) => {
            hash.update([2]);
            hash.update(value.to_bits().to_be_bytes());
        }
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            hash.update([if matches!(value, ValueRef::Text(_)) {
                3
            } else {
                4
            }]);
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod copy_handle;
#[cfg(test)]
pub(super) use copy_handle::CopyStartError;
