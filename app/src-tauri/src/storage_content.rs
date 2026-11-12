//! Preservation proof v2: exact stored values, independent of editor compatibility.
//! Explicit, frozen table sections. Schema 0 has an empty weather section. Future
//! persistent modules extend this single inventory under a NEW proof version;
//! filesystem publication, recovery ownership and archive security stay unchanged.
use rusqlite::{Connection, types::ValueRef};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Debug)]
pub(crate) enum Error {
    Sql(rusqlite::Error),
    Invalid,
    Limit,
    Cancelled,
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}
// Physical metadata retains v1's 64 MiB value capacity. Keep the existing
// v2 total row budget (formerly derived from portable export) unchanged.
// Portable export keeps its separate, narrower per-field metadata bounds.
const METADATA_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const ROW_LIMIT: usize = 69 * 1024 * 1024 + 1024;
pub(crate) type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Proof {
    pub sha256: String,
    pub entry_count: u64,
}
fn check(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn text(v: ValueRef<'_>, limit: usize) -> Result<&str> {
    match v {
        ValueRef::Text(b) if b.len() <= limit => std::str::from_utf8(b).map_err(|_| Error::Invalid),
        ValueRef::Text(_) => Err(Error::Limit),
        _ => Err(Error::Invalid),
    }
}
fn field(hash: &mut Sha256, value: ValueRef<'_>) -> Result<()> {
    match value {
        ValueRef::Null => hash.update([0]),
        ValueRef::Integer(n) => {
            hash.update([1]);
            hash.update(n.to_be_bytes());
        }
        ValueRef::Text(b) => {
            hash.update([3]);
            hash.update((b.len() as u64).to_be_bytes());
            hash.update(b);
        }
        _ => return Err(Error::Invalid),
    }
    Ok(())
}
// Proof v3 adds SQLite REAL values without changing the frozen v1/v2 encoding.
fn field_v3(hash: &mut Sha256, value: ValueRef<'_>) -> Result<()> {
    match value {
        ValueRef::Real(n) if n.is_finite() => {
            hash.update([2]);
            hash.update(n.to_bits().to_be_bytes());
        }
        other => field(hash, other)?,
    }
    Ok(())
}
// Caller owns the transaction. This function never releases or replaces it.
pub(crate) fn snapshot(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() || !crate::database::validate_supported_schema(db)? {
        return Err(Error::Invalid);
    }
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version == 1 {
        crate::weather::storage::validate_metadata(db, None).map_err(|e| {
            if e == "weather_resource_limit" {
                Error::Limit
            } else {
                Error::Invalid
            }
        })?;
    }
    let integrity: String = db.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::Invalid);
    }
    let mut fk = db.prepare("PRAGMA main.foreign_key_check")?;
    if fk.query([])?.next()?.is_some() {
        return Err(Error::Invalid);
    }
    verified_snapshot(db, flag)
}
// For an already verified, immutable PreparedRestore snapshot only. No PRAGMA
// or transaction-control SQL: its existing authorizer remains unchanged.
pub(crate) fn verified_snapshot(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() {
        return Err(Error::Invalid);
    }
    // Bound payload materialization using the caller's SQLite limit (ROW_LIMIT).
    let mut hash = Sha256::new().chain_update(b"MNO-preserved-content\0\x00\x00\x00\x02");
    hash.update(b"journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0");
    let mut statement=db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY")?;
    let mut rows = statement.query([])?;
    let mut count = 0u64;
    let mut previous: Option<String> = None;
    while let Some(row) = rows.next()? {
        check(flag)?;
        let id = text(row.get_ref(0)?, METADATA_LIMIT)?;
        if previous.as_deref().is_some_and(|p| p >= id) {
            return Err(Error::Invalid);
        }
        for i in [1, 2, 5, 6] {
            text(row.get_ref(i)?, METADATA_LIMIT)?;
        }
        if !matches!(row.get_ref(4)?, ValueRef::Integer(1)) {
            return Err(Error::Invalid);
        }
        let json = text(row.get_ref(3)?, crate::open_export::CONTENT_LIMIT)?;
        crate::open_export::validate_json(json).map_err(|e| match e {
            crate::open_export::Error::ResourceLimit => Error::Limit,
            _ => Error::Invalid,
        })?;
        hash.update([0x52]);
        for i in 0..7 {
            field(&mut hash, row.get_ref(i)?)?;
        }
        previous = Some(id.to_owned());
        count = count.checked_add(1).ok_or(Error::Limit)?;
    }
    hash.update([0x45]);
    hash.update(count.to_be_bytes());
    hash.update(
        b"journal_weather\0entry_id\0format_version\0manual_json\0fetched_json\0revision\0",
    );
    let has_weather:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE type='table' AND name='journal_weather')",[],|r|r.get(0))?;
    let mut weather_count = 0u64;
    if has_weather {
        let mut statement=db.prepare("SELECT entry_id,format_version,manual_json,fetched_json,revision FROM main.journal_weather ORDER BY entry_id COLLATE BINARY")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            check(flag)?;
            let entry = text(row.get_ref(0)?, crate::open_export::METADATA_LIMIT)?;
            let part = |i| -> Result<Option<&str>> {
                match row.get_ref(i)? {
                    ValueRef::Null => Ok(None),
                    v => text(v, crate::weather::storage::PART_LIMIT).map(Some),
                }
            };
            if !matches!(row.get_ref(1)?, ValueRef::Integer(1))
                || !matches!(row.get_ref(4)?,ValueRef::Integer(n) if n>=1)
            {
                return Err(Error::Invalid);
            }
            crate::weather::storage::validate(entry, part(2)?, part(3)?)
                .map_err(|_| Error::Invalid)?;
            hash.update([0x52]);
            for i in 0..5 {
                field(&mut hash, row.get_ref(i)?)?;
            }
            weather_count = weather_count.checked_add(1).ok_or(Error::Limit)?;
        }
    }
    hash.update([0x45]);
    hash.update(weather_count.to_be_bytes());
    check(flag)?;
    Ok(Proof {
        sha256: format!("{:x}", hash.finalize()),
        entry_count: count,
    })
}
pub(crate) fn verify(db: &mut Connection, flag: &AtomicBool) -> Result<Proof> {
    use rusqlite::limits::Limit;
    check(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = (|| {
        let tx = db.transaction()?;
        let proof = snapshot(&tx, flag)?;
        tx.commit()?;
        Ok(proof)
    })();
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(p), Ok(_)) => Ok(p),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}

/// Schema-2 preservation proof. Version 1/2 implementations above remain frozen.
pub(crate) fn verify_v3(db: &mut Connection, flag: &AtomicBool) -> Result<Proof> {
    use rusqlite::limits::Limit;
    check(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = (|| {
        let tx = db.transaction()?;
        let proof = snapshot_v3(&tx, flag)?;
        tx.commit()?;
        Ok(proof)
    })();
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(p), Ok(_)) => Ok(p),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}

pub(crate) fn snapshot_v3(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() || !crate::database::place_schema::validate_schema2(db)? {
        return Err(Error::Invalid);
    }
    let integrity: String = db.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::Invalid);
    }
    let mut fk = db.prepare("PRAGMA main.foreign_key_check")?;
    if fk.query([])?.next()?.is_some() {
        return Err(Error::Invalid);
    }
    verified_snapshot_v3(db, flag)
}

/// For a previously verified immutable Restore candidate with its SQL authorizer installed.
pub(crate) fn verified_snapshot_v3(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    verified_snapshot_v3_inner(db, flag, false)
}
fn verified_snapshot_v3_inner(
    db: &Connection,
    flag: &AtomicBool,
    photographs: bool,
) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() {
        return Err(Error::Invalid);
    }
    // Preserve exact v2 domain and encoding as a prefix, then append explicitly
    // separated schema-2 domains. Typed field encoding distinguishes NULL.
    let mut hash = Sha256::new().chain_update(b"MNO-preserved-content\0\x00\x00\x00\x03");
    hash.update(b"journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0");
    let mut entries = 0u64;
    let mut q=db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY")?;
    let mut rows = q.query([])?;
    let mut previous: Option<String> = None;
    while let Some(row) = rows.next()? {
        check(flag)?;
        let id = text(row.get_ref(0)?, METADATA_LIMIT)?;
        if previous.as_deref().is_some_and(|p| p >= id) {
            return Err(Error::Invalid);
        }
        for i in [1, 2, 5, 6] {
            text(row.get_ref(i)?, METADATA_LIMIT)?;
        }
        if !matches!(row.get_ref(4)?, ValueRef::Integer(1))
            && !(photographs && matches!(row.get_ref(4)?, ValueRef::Integer(2)))
        {
            return Err(Error::Invalid);
        }
        let json = text(row.get_ref(3)?, crate::open_export::CONTENT_LIMIT)?;
        if photographs && matches!(row.get_ref(4)?, ValueRef::Integer(2)) {
            crate::photographs::validate_references(db, id, json).map_err(|_| Error::Invalid)?;
        }
        crate::open_export::validate_json(json).map_err(|e| match e {
            crate::open_export::Error::ResourceLimit => Error::Limit,
            _ => Error::Invalid,
        })?;
        hash.update([0x52]);
        for i in 0..7 {
            field(&mut hash, row.get_ref(i)?)?;
        }
        previous = Some(id.to_owned());
        entries = entries.checked_add(1).ok_or(Error::Limit)?;
    }
    hash.update([0x45]);
    hash.update(entries.to_be_bytes());
    hash.update(
        b"journal_weather\0entry_id\0format_version\0manual_json\0fetched_json\0revision\0",
    );
    let mut weather = 0u64;
    let mut q=db.prepare("SELECT entry_id,format_version,manual_json,fetched_json,revision FROM main.journal_weather ORDER BY entry_id COLLATE BINARY")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        check(flag)?;
        let entry = text(row.get_ref(0)?, crate::open_export::METADATA_LIMIT)?;
        let part = |i| -> Result<Option<&str>> {
            match row.get_ref(i)? {
                ValueRef::Null => Ok(None),
                v => text(v, crate::weather::storage::PART_LIMIT).map(Some),
            }
        };
        if !matches!(row.get_ref(1)?, ValueRef::Integer(1))
            || !matches!(row.get_ref(4)?,ValueRef::Integer(n) if n>=1)
        {
            return Err(Error::Invalid);
        }
        crate::weather::storage::validate(entry, part(2)?, part(3)?).map_err(|_| Error::Invalid)?;
        hash.update([0x52]);
        for i in 0..5 {
            field_v3(&mut hash, row.get_ref(i)?)?;
        }
        weather = weather.checked_add(1).ok_or(Error::Limit)?;
    }
    hash.update([0x45]);
    hash.update(weather.to_be_bytes());
    hash.update(b"places\0id\0name\0latitude\0longitude\0revision\0");
    let mut places = 0u64;
    let mut q = db.prepare(
        "SELECT id,name,latitude,longitude,revision FROM main.places ORDER BY id COLLATE BINARY",
    )?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        check(flag)?;
        let id = text(row.get_ref(0)?, 128)?;
        let name = text(row.get_ref(1)?, 512)?;
        let parsed = uuid::Uuid::parse_str(id).map_err(|_| Error::Invalid)?;
        if parsed.to_string() != id
            || parsed.get_variant() != uuid::Variant::RFC4122
            || name.trim().is_empty()
        {
            return Err(Error::Invalid);
        }
        let coord = |i| -> Result<Option<f64>> {
            match row.get_ref(i)? {
                ValueRef::Null => Ok(None),
                ValueRef::Integer(n) => Ok(Some(n as f64)),
                ValueRef::Real(n) if n.is_finite() => Ok(Some(n)),
                _ => Err(Error::Invalid),
            }
        };
        let (lat, lon) = (coord(2)?, coord(3)?);
        match (lat, lon) {
            (None, None) => {}
            (Some(a), Some(b)) if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&b) => {}
            _ => return Err(Error::Invalid),
        }
        if !matches!(row.get_ref(4)?,ValueRef::Integer(n) if n>=1) {
            return Err(Error::Invalid);
        }
        hash.update([0x52]);
        for i in 0..5 {
            field_v3(&mut hash, row.get_ref(i)?)?;
        }
        places += 1;
    }
    hash.update([0x45]);
    hash.update(places.to_be_bytes());
    hash.update(b"journal_places\0entry_id\0place_id\0place_name_snapshot\0latitude_snapshot\0longitude_snapshot\0place_revision_snapshot\0revision\0");
    let mut links = 0u64;
    let mut q=db.prepare("SELECT entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision FROM main.journal_places ORDER BY entry_id COLLATE BINARY")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        check(flag)?;
        let entry = text(row.get_ref(0)?, crate::open_export::METADATA_LIMIT)?;
        let place = match row.get_ref(1)? {
            ValueRef::Null => None,
            v => Some(text(v, 128)?),
        };
        let tombstone = place.is_none();
        if tombstone {
            for i in 2..=5 {
                if !matches!(row.get_ref(i)?, ValueRef::Null) {
                    return Err(Error::Invalid);
                }
            }
        } else {
            let pid = place.unwrap();
            let id = uuid::Uuid::parse_str(pid).map_err(|_| Error::Invalid)?;
            if id.to_string() != pid || id.get_variant() != uuid::Variant::RFC4122 {
                return Err(Error::Invalid);
            }
            let n = text(row.get_ref(2)?, 512)?;
            if n.trim().is_empty() {
                return Err(Error::Invalid);
            }
            let lat = match row.get_ref(3)? {
                ValueRef::Null => None,
                ValueRef::Integer(n) => Some(n as f64),
                ValueRef::Real(n) if n.is_finite() => Some(n),
                _ => return Err(Error::Invalid),
            };
            let lon = match row.get_ref(4)? {
                ValueRef::Null => None,
                ValueRef::Integer(n) => Some(n as f64),
                ValueRef::Real(n) if n.is_finite() => Some(n),
                _ => return Err(Error::Invalid),
            };
            match (lat, lon) {
                (None, None) => {}
                (Some(a), Some(b))
                    if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&b) => {}
                _ => return Err(Error::Invalid),
            }
            if !matches!(row.get_ref(5)?,ValueRef::Integer(n) if n>=1) {
                return Err(Error::Invalid);
            }
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM main.places WHERE id=?1)",
                [pid],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::Invalid);
            }
        }
        if !matches!(row.get_ref(6)?,ValueRef::Integer(n) if n>=1) {
            return Err(Error::Invalid);
        }
        let entry_exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM main.journal_entries WHERE id=?1)",
            [entry],
            |r| r.get(0),
        )?;
        if !entry_exists {
            return Err(Error::Invalid);
        }
        hash.update([0x52]);
        for i in 0..7 {
            field_v3(&mut hash, row.get_ref(i)?)?;
        }
        links += 1;
    }
    hash.update([0x45]);
    hash.update(links.to_be_bytes());
    check(flag)?;
    Ok(Proof {
        sha256: format!("{:x}", hash.finalize()),
        entry_count: entries,
    })
}

/// Current schema-3 proof. Proof v3 above remains byte-for-byte unchanged and
/// is committed as a complete sub-proof before the new settings domain.
pub(crate) fn verify_v4(db: &mut Connection, flag: &AtomicBool) -> Result<Proof> {
    use rusqlite::limits::Limit;
    check(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = (|| {
        let tx = db.transaction()?;
        let proof = snapshot_v4(&tx, flag)?;
        tx.commit()?;
        Ok(proof)
    })();
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(p), Ok(_)) => Ok(p),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}

pub(crate) fn snapshot_v4(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() || !crate::database::weather_default_schema::validate_schema3(db)? {
        return Err(Error::Invalid);
    }
    let integrity: String = db.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::Invalid);
    }
    let mut fk = db.prepare("PRAGMA main.foreign_key_check")?;
    if fk.query([])?.next()?.is_some() {
        return Err(Error::Invalid);
    }
    verified_snapshot_v4(db, flag)
}

pub(crate) fn verified_snapshot_v4(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    verified_snapshot_v4_inner(db, flag, false)
}
fn verified_snapshot_v4_inner(
    db: &Connection,
    flag: &AtomicBool,
    photographs: bool,
) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let previous = verified_snapshot_v3_inner(db, flag, photographs)?;
    let mut hash = Sha256::new().chain_update(b"MNO-preserved-content\0\x00\x00\x00\x04");
    hash.update(b"schema-2-proof-v3\0sha256\0entry_count\0");
    hash.update([0x52]);
    field_v3(&mut hash, ValueRef::Text(previous.sha256.as_bytes()))?;
    let entry_count = i64::try_from(previous.entry_count).map_err(|_| Error::Limit)?;
    field_v3(&mut hash, ValueRef::Integer(entry_count))?;
    hash.update([0x45]);
    hash.update(1u64.to_be_bytes());

    hash.update(b"weather_default_place\0singleton\0place_id\0revision\0");
    let mut rows = db.prepare(
        "SELECT singleton,place_id,revision FROM main.weather_default_place ORDER BY singleton",
    )?;
    let mut rows = rows.query([])?;
    let mut count = 0u64;
    while let Some(row) = rows.next()? {
        check(flag)?;
        if count != 0
            || !matches!(row.get_ref(0)?, ValueRef::Integer(1))
            || !matches!(row.get_ref(2)?, ValueRef::Integer(n) if n >= 1)
        {
            return Err(Error::Invalid);
        }
        match row.get_ref(1)? {
            ValueRef::Null => {}
            value => {
                let place_id = text(value, 128)?;
                let parsed = uuid::Uuid::parse_str(place_id).map_err(|_| Error::Invalid)?;
                if parsed.to_string() != place_id || parsed.get_variant() != uuid::Variant::RFC4122
                {
                    return Err(Error::Invalid);
                }
                let coordinates = db.query_row(
                    "SELECT latitude,longitude FROM main.places WHERE id=?1",
                    [place_id],
                    |r| Ok((r.get::<_, Option<f64>>(0)?, r.get::<_, Option<f64>>(1)?)),
                );
                let (Some(latitude), Some(longitude)) = coordinates.map_err(|_| Error::Invalid)?
                else {
                    return Err(Error::Invalid);
                };
                if !latitude.is_finite()
                    || !longitude.is_finite()
                    || !(-90.0..=90.0).contains(&latitude)
                    || !(-180.0..=180.0).contains(&longitude)
                {
                    return Err(Error::Invalid);
                }
            }
        }
        hash.update([0x52]);
        for index in 0..3 {
            field_v3(&mut hash, row.get_ref(index)?)?;
        }
        count += 1;
    }
    hash.update([0x45]);
    hash.update(count.to_be_bytes());
    check(flag)?;
    Ok(Proof {
        sha256: format!("{:x}", hash.finalize()),
        entry_count: previous.entry_count,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    #[test]
    fn frozen_empty_proof_v2_is_identical_across_additive_migration() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::initialize_schema(&db).unwrap();
        let flag = std::sync::atomic::AtomicBool::new(false);
        let legacy = super::verify(&mut db, &flag).unwrap();
        assert_eq!(legacy.entry_count, 0);
        assert_eq!(
            legacy.sha256,
            "01bd0ab2859c83d087dc7eb30f5340ccba278fc41ec82583b8e0351acdb72914"
        );
        crate::database::weather_schema::migrate(&mut db).unwrap();
        assert_eq!(super::verify(&mut db, &flag).unwrap(), legacy);
    }

    #[test]
    fn v3_covers_places_links_tombstones_and_rejects_bad_references() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::initialize_schema(&db).unwrap();
        crate::database::weather_schema::migrate(&mut db).unwrap();
        crate::database::place_schema::migrate_candidate(&mut db).unwrap();
        let flag = AtomicBool::new(false);
        let empty = super::verify_v3(&mut db, &flag).unwrap();
        let p1 = uuid::Uuid::new_v4().to_string();
        let p2 = uuid::Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO places(id,name,revision) VALUES(?1,'Meadow',1)",
            [&p1],
        )
        .unwrap();
        let one = super::verify_v3(&mut db, &flag).unwrap();
        assert_ne!(one, empty);
        db.execute(
            "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Meadow',0,0,1)",
            [&p2],
        )
        .unwrap();
        let two = super::verify_v3(&mut db, &flag).unwrap();
        assert_ne!(two, one);
        db.execute("INSERT INTO journal_entries(id,entry_date,title,content_json,document_version,created_at,updated_at) VALUES('e','2024-01-01','t','{}',1,'c','u')",[]).unwrap();
        db.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES('e',?1,'Meadow',1,1)",[&p1]).unwrap();
        let linked = super::verify_v3(&mut db, &flag).unwrap();
        assert_ne!(linked, two);
        db.execute(
            "UPDATE places SET name='Changed',revision=2 WHERE id=?1",
            [&p1],
        )
        .unwrap();
        let changed = super::verify_v3(&mut db, &flag).unwrap();
        assert_ne!(changed, linked);
        let frozen: String = db
            .query_row(
                "SELECT place_name_snapshot FROM journal_places WHERE entry_id='e'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(frozen, "Meadow");
        db.execute("UPDATE journal_places SET place_id=NULL,place_name_snapshot=NULL,latitude_snapshot=NULL,longitude_snapshot=NULL,place_revision_snapshot=NULL,revision=2 WHERE entry_id='e'",[]).unwrap();
        assert_ne!(super::verify_v3(&mut db, &flag).unwrap(), changed);
        db.execute("UPDATE places SET latitude=999 WHERE id=?1", [&p2])
            .unwrap_err();
    }

    fn schema2() -> rusqlite::Connection {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::initialize_schema(&db).unwrap();
        crate::database::weather_schema::migrate(&mut db).unwrap();
        crate::database::place_schema::migrate_candidate(&mut db).unwrap();
        db
    }

    fn schema3() -> rusqlite::Connection {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        crate::database::initialize_schema(&tx).unwrap();
        crate::database::weather_schema::upgrade_in_transaction(&tx).unwrap();
        crate::database::place_schema::upgrade_in_transaction(&tx).unwrap();
        crate::database::weather_default_schema::create_in_transaction(&tx).unwrap();
        tx.commit().unwrap();
        db
    }

    #[test]
    fn v4_distinguishes_unset_active_tombstone_reference_and_revision() {
        let mut db = schema3();
        let flag = AtomicBool::new(false);
        let unset = super::verify_v4(&mut db, &flag).unwrap();
        let place = "00000000-0000-4000-8000-000000000001";
        db.execute(
            "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Home',59.0,18.0,1)",
            [place],
        )
        .unwrap();
        let place_only = super::verify_v4(&mut db, &flag).unwrap();
        assert_ne!(place_only, unset);
        db.execute("INSERT INTO weather_default_place VALUES(1,?1,1)", [place])
            .unwrap();
        let active = super::verify_v4(&mut db, &flag).unwrap();
        assert_ne!(active, place_only);
        let other = "00000000-0000-4000-8000-000000000002";
        db.execute(
            "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Other',60.0,19.0,1)",
            [other],
        )
        .unwrap();
        db.execute("UPDATE weather_default_place SET place_id=?1", [other])
            .unwrap();
        let changed_reference = super::verify_v4(&mut db, &flag).unwrap();
        assert_ne!(changed_reference, active);
        db.execute("UPDATE weather_default_place SET revision=2", [])
            .unwrap();
        let revised = super::verify_v4(&mut db, &flag).unwrap();
        assert_ne!(revised, changed_reference);
        db.execute(
            "UPDATE weather_default_place SET place_id=NULL,revision=3",
            [],
        )
        .unwrap();
        let tombstone = super::verify_v4(&mut db, &flag).unwrap();
        assert_ne!(tombstone, revised);
        assert_ne!(tombstone, unset);
    }

    #[test]
    fn v4_is_deterministic_and_rejects_missing_or_coordinate_less_default() {
        let flag = AtomicBool::new(false);
        let mut a = schema3();
        let mut b = schema3();
        let ids = [
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000002",
        ];
        for (db, order) in [(&mut a, [0, 1]), (&mut b, [1, 0])] {
            for index in order {
                db.execute(
                    "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,?2,?3,?4,1)",
                    rusqlite::params![
                        ids[index],
                        format!("p{index}"),
                        if index == 0 { None } else { Some(0.0) },
                        if index == 0 { None } else { Some(0.0) }
                    ],
                )
                .unwrap();
            }
            db.execute("INSERT INTO weather_default_place VALUES(1,?1,1)", [ids[1]])
                .unwrap();
        }
        assert_eq!(
            super::verify_v4(&mut a, &flag).unwrap(),
            super::verify_v4(&mut b, &flag).unwrap()
        );
        a.execute("UPDATE weather_default_place SET place_id=?1", [ids[0]])
            .unwrap();
        assert!(super::verify_v4(&mut a, &flag).is_err());
        a.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        a.execute(
            "UPDATE weather_default_place SET place_id='00000000-0000-4000-8000-000000000099'",
            [],
        )
        .unwrap();
        assert!(super::verify_v4(&mut a, &flag).is_err());
    }

    #[test]
    fn v3_is_order_independent_and_distinguishes_null_and_float_bits() {
        let mut a = schema2();
        let mut b = schema2();
        let flag = AtomicBool::new(false);
        let ids = [
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000002",
        ];
        for (db, order) in [(&mut a, [0, 1]), (&mut b, [1, 0])] {
            for i in order {
                db.execute(
                    "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,?2,?3,?4,1)",
                    rusqlite::params![
                        ids[i],
                        format!("p{i}"),
                        if i == 0 { None } else { Some(0.0) },
                        if i == 0 { None } else { Some(0.0) }
                    ],
                )
                .unwrap();
            }
        }
        assert_eq!(
            super::verify_v3(&mut a, &flag).unwrap(),
            super::verify_v3(&mut b, &flag).unwrap()
        );
        a.execute(
            "UPDATE places SET latitude=-0.0,longitude=-0.0 WHERE id=?1",
            [ids[1]],
        )
        .unwrap();
        // SQLite normalizes signed zero in REAL storage; prove stored values.
        assert_eq!(
            super::verify_v3(&mut a, &flag).unwrap(),
            super::verify_v3(&mut b, &flag).unwrap()
        );
        // Finite boundaries are accepted; NaN/infinity are rejected by storage validation.
        a.execute(
            "UPDATE places SET latitude=90.0,longitude=180.0 WHERE id=?1",
            [ids[1]],
        )
        .unwrap();
        assert!(super::verify_v3(&mut a, &flag).is_ok());
        a.execute_batch("PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        a.execute(
            "UPDATE places SET latitude=1,longitude=NULL WHERE id=?1",
            [ids[1]],
        )
        .unwrap();
        assert!(super::verify_v3(&mut a, &flag).is_err());
    }

    #[test]
    fn v3_covers_shared_references_and_rejects_invalid_snapshot_and_tombstone() {
        let mut db = schema2();
        let flag = AtomicBool::new(false);
        let pid = "00000000-0000-4000-8000-000000000001";
        db.execute(
            "INSERT INTO places(id,name,revision) VALUES(?1,'Field',1)",
            [pid],
        )
        .unwrap();
        for e in ["e1", "e2"] {
            db.execute("INSERT INTO journal_entries(id,entry_date,title,content_json,document_version,created_at,updated_at) VALUES(?1,'2024-01-01','t','{}',1,'c','u')",[e]).unwrap();
            db.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,place_revision_snapshot,revision) VALUES(?1,?2,'Field',1,1)",rusqlite::params![e,pid]).unwrap();
        }
        let base = super::verify_v3(&mut db, &flag).unwrap();
        db.execute(
            "UPDATE journal_places SET place_name_snapshot='Other' WHERE entry_id='e1'",
            [],
        )
        .unwrap();
        assert_ne!(super::verify_v3(&mut db, &flag).unwrap(), base);
        db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        db.execute("UPDATE journal_places SET place_id='00000000-0000-4000-8000-000000000099' WHERE entry_id='e1'",[]).unwrap();
        assert!(super::verify_v3(&mut db, &flag).is_err());
        db.execute_batch("PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        db.execute("UPDATE journal_places SET place_id=NULL,place_name_snapshot='orphan' WHERE entry_id='e1'",[]).unwrap();
        assert!(super::verify_v3(&mut db, &flag).is_err());
    }
}

pub(crate) fn verify_v5(db: &mut Connection, flag: &AtomicBool) -> Result<Proof> {
    use rusqlite::limits::Limit;
    check(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = (|| {
        let tx = db.transaction()?;
        let proof = snapshot_v5(&tx, flag)?;
        tx.commit()?;
        Ok(proof)
    })();
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(p), Ok(_)) => Ok(p),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}

pub(crate) fn snapshot_v5(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    if db.is_autocommit() || !crate::photographs::validate_schema(db)? {
        return Err(Error::Invalid);
    }
    let integrity: String = db.query_row("PRAGMA integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok"
        || db
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(Error::Invalid);
    }
    verified_snapshot_v5(db, flag)
}
pub(crate) fn verified_snapshot_v5(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    let previous = verified_snapshot_v4_inner(db, flag, true)?;
    let mut hash = Sha256::new().chain_update(b"MNO-preserved-content\0\x00\x00\x00\x05");
    hash.update(previous.sha256.as_bytes());
    hash.update(
        b"journal_image_assets\0asset_id,entry_id,original_path,mime,width,height,preview\0",
    );
    let mut q = db.prepare("SELECT asset_id,entry_id,original_path,mime,width,height,preview FROM journal_image_assets ORDER BY asset_id COLLATE BINARY")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        check(flag)?;
        crate::photographs::row(row)?
            .validate()
            .map_err(|_| Error::Invalid)?;
        hash.update([0x52]);
        for i in 0..7 {
            match row.get_ref(i)? {
                ValueRef::Blob(bytes) => {
                    hash.update([4]);
                    hash.update((bytes.len() as u64).to_be_bytes());
                    hash.update(bytes);
                }
                value => field_v3(&mut hash, value)?,
            }
        }
    }
    hash.update([0x45]);
    Ok(Proof {
        sha256: format!("{:x}", hash.finalize()),
        entry_count: previous.entry_count,
    })
}

// Proof v6 extends the frozen schema-4 inventory with all bird lists, including
// unlinked ones. A source-preservation check can still use verified_snapshot_v5
// inside an already validated schema-5 snapshot.
pub(crate) fn verify_v6(db: &mut Connection, flag: &AtomicBool) -> Result<Proof> {
    use rusqlite::limits::Limit;
    if !db.is_autocommit() {
        return Err(Error::Invalid);
    }
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = (|| {
        let tx = db.transaction()?;
        let proof = snapshot_v6(&tx, flag)?;
        tx.commit()?;
        Ok(proof)
    })();
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(p), Ok(_)) => Ok(p),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}
pub(crate) fn snapshot_v6(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    check(flag)?;
    if db.is_autocommit() || !crate::database::bird_schema::validate_schema(db)? {
        return Err(Error::Invalid);
    }
    let integrity: String = db.query_row("PRAGMA integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok"
        || db
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(Error::Invalid);
    }
    verified_snapshot_v6(db, flag)
}
pub(crate) fn verified_snapshot_v6(db: &Connection, flag: &AtomicBool) -> Result<Proof> {
    let previous = verified_snapshot_v5(db, flag)?;
    let mut hash = Sha256::new().chain_update(b"MNO-preserved-content\0\x00\x00\x00\x06");
    hash.update(previous.sha256.as_bytes());
    for (section, sql, columns) in [
        (
            "bird_lists:id,visit_date,place_id",
            "SELECT id,visit_date,place_id FROM bird_lists ORDER BY id COLLATE BINARY",
            3,
        ),
        (
            "bird_rows:id,list_id,position,avibase_id,taxonomy_version,count",
            "SELECT id,list_id,position,avibase_id,taxonomy_version,count FROM bird_rows ORDER BY id COLLATE BINARY",
            6,
        ),
        (
            "journal_bird_lists:entry_id,list_id",
            "SELECT entry_id,list_id FROM journal_bird_lists ORDER BY entry_id COLLATE BINARY,list_id COLLATE BINARY",
            2,
        ),
    ] {
        hash.update(section.as_bytes());
        hash.update([0]);
        let mut query = db.prepare(sql)?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            check(flag)?;
            for i in 0..columns {
                let value = row.get_ref(i)?;
                if let ValueRef::Text(_) = value {
                    text(value, 512)?;
                }
                match (section, i) {
                    ("bird_lists:id,visit_date,place_id", 1) => {
                        let date = text(value, 10)?;
                        let parsed = time::Date::parse(
                            date,
                            &time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
                                .map_err(|_| Error::Invalid)?,
                        )
                        .map_err(|_| Error::Invalid)?;
                        if parsed.to_string() != date {
                            return Err(Error::Invalid);
                        }
                    }
                    ("bird_rows:id,list_id,position,avibase_id,taxonomy_version,count", 2) => {
                        if !matches!(value, ValueRef::Integer(n) if n >= 0) {
                            return Err(Error::Invalid);
                        }
                    }
                    ("bird_rows:id,list_id,position,avibase_id,taxonomy_version,count", 5) => {
                        if !matches!(value, ValueRef::Integer(n) if n > 0) {
                            return Err(Error::Invalid);
                        }
                    }
                    _ => {
                        if text(value, 512)?.trim().is_empty() {
                            return Err(Error::Invalid);
                        }
                    }
                }
                field(&mut hash, value)?;
            }
            hash.update([0x52]);
        }
        hash.update([0x45]);
    }
    // A bird list requires an actual named position, unlike optional journal places.
    let invalid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM bird_lists b JOIN places p ON p.id=b.place_id WHERE p.latitude IS NULL OR p.longitude IS NULL)", [], |r| r.get(0))?;
    if invalid {
        return Err(Error::Invalid);
    }
    Ok(Proof {
        sha256: format!("{:x}", hash.finalize()),
        entry_count: previous.entry_count,
    })
}
