//! Versioned open export using bounded ZIP/JSONL snapshots.
//! Caller supplies a dedicated read-only connection under the normal operation gate.
use rusqlite::{Connection, TransactionBehavior, limits::Limit, types::ValueRef};
use serde::{
    Serialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::{self, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::{NamedTempFile, tempfile_in};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
mod markdown;
#[cfg(test)]
mod tests;
pub(crate) const CONTENT_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const METADATA_LIMIT: usize = 1024 * 1024;
const VALUE_LIMIT: usize = 1_000_000;
const DEPTH_LIMIT: usize = 64;
pub(crate) const ROW_LIMIT: usize = CONTENT_LIMIT + 5 * METADATA_LIMIT + 1024;
const README: &str = "# MNO open export v1\n\nThis is an open ZIP/ZIP64 archive, not an MNO backup. No SQLite database is included.\n\njournal.md is a human-readable view. Unsupported structures appear as labelled original JSON. journal.jsonl is authoritative: each UTF-8 line contains all seven stored fields; content_json is a JSON string whose decoded value is the exact original text. Do not import Markdown as a substitute.\n\nmanifest.json describes version, row count, file byte sizes and SHA-256 checksums. Checksums detect damage; they do not authenticate the sender. Unknown export versions must not be silently imported.\n\nWARNING: this archive is NOT encrypted. Journal text and location references may be private. Store and share it carefully. External images, attachments and original files are NOT included and no referenced paths have been followed.\n\nNo weather data, settings, unsaved editor text or cache are exported by schema 0. Future formats must explicitly account for new persistent data.\n\nLossless reimport of ALL exported document structures, including legacy and unknown JSON objects, is a required acceptance criterion for import step 2. This export engine does not yet implement import.\n";

#[derive(Debug)]
pub(crate) enum Error {
    Sql(rusqlite::Error),
    Io(io::Error),
    Zip(zip::result::ZipError),
    SourceContract,
    IncompatibleSchema,
    InvalidDatabase,
    InvalidStorage,
    InvalidJson,
    ResourceLimit,
    Cancelled,
    InvalidDestination,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Serialize)]
pub(crate) struct FileRecord {
    pub size: u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Manifest {
    pub format: &'static str,
    pub format_version: u32,
    pub database_schema_version: u32,
    pub document_version: u32,
    pub entry_count: u64,
    pub files: BTreeMap<&'static str, FileRecord>,
    pub external_originals_included: bool,
}
#[derive(Serialize)]
pub(crate) struct Entry<'a> {
    id: &'a str,
    entry_date: &'a str,
    title: &'a str,
    content_json: &'a str,
    document_version: i64,
    created_at: &'a str,
    updated_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    weather: Option<crate::weather::storage::WeatherRecord>,
}
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

// Full syntactic validation without a Value tree. Duplicate keys remain legal
// stored JSON; flag them for verbatim fallback rather than choosing a meaning.
struct JsonCheck<'a> {
    values: &'a mut usize,
    depth: usize,
    duplicates: &'a mut bool,
}
impl<'de> DeserializeSeed<'de> for JsonCheck<'_> {
    type Value = bool;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> std::result::Result<bool, D::Error> {
        if *self.values == 0 || self.depth > DEPTH_LIMIT {
            return Err(de::Error::custom("resource_limit"));
        }
        *self.values -= 1;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for JsonCheck<'_> {
    type Value = bool;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<bool, E> {
        Ok(false)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<bool, A::Error> {
        while a
            .next_element_seed(JsonCheck {
                values: self.values,
                depth: self.depth + 1,
                duplicates: self.duplicates,
            })?
            .is_some()
        {}
        Ok(false)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<bool, A::Error> {
        let mut keys = std::collections::HashSet::new();
        while let Some(k) = a.next_key::<String>()? {
            if !keys.insert(k) {
                *self.duplicates = true;
            }
            a.next_value_seed(JsonCheck {
                values: self.values,
                depth: self.depth + 1,
                duplicates: self.duplicates,
            })?;
        }
        Ok(true)
    }
}
pub(crate) fn validate_json(json: &str) -> Result<bool> {
    if json.len() > CONTENT_LIMIT {
        return Err(Error::ResourceLimit);
    }
    let mut values = VALUE_LIMIT;
    let mut duplicates = false;
    let mut d = serde_json::Deserializer::from_str(json);
    let object = JsonCheck {
        values: &mut values,
        depth: 0,
        duplicates: &mut duplicates,
    }
    .deserialize(&mut d)
    .map_err(|e| {
        if e.to_string().contains("resource_limit") {
            Error::ResourceLimit
        } else {
            Error::InvalidJson
        }
    })?;
    d.end().map_err(|_| Error::InvalidJson)?;
    if !object {
        return Err(Error::InvalidJson);
    }
    Ok(duplicates)
}
fn text<'a>(row: &'a rusqlite::Row<'_>, i: usize, limit: usize) -> Result<&'a str> {
    match row.get_ref(i)? {
        ValueRef::Text(b) if b.len() <= limit => {
            std::str::from_utf8(b).map_err(|_| Error::InvalidStorage)
        }
        ValueRef::Text(_) => Err(Error::ResourceLimit),
        _ => Err(Error::InvalidStorage),
    }
}
struct LimitedWrite<'a> {
    file: &'a mut File,
    remaining: u64,
    flag: &'a AtomicBool,
}
impl Write for LimitedWrite<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.flag.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Other, "export cancelled"));
        }
        if b.len() as u64 > self.remaining {
            return Err(io::Error::other("row output resource limit"));
        }
        let n = self.file.write(b)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
fn record(file: &mut File, flag: &AtomicBool) -> Result<FileRecord> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        cancelled(flag)?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        size = size.checked_add(n as u64).ok_or(Error::ResourceLimit)?;
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(FileRecord {
        size,
        sha256: format!("{:x}", hash.finalize()),
        row_count: None,
    })
}
// Internal only. The caller retains ownership of the dedicated SQLite handle and
// must close it normally after this call (also on failure). No reopen/dup occurs.
pub(crate) fn export(
    source: &mut Connection,
    destination: &Path,
    flag: &AtomicBool,
) -> Result<Manifest> {
    export_with(source, destination, flag, |_| Ok(()))
}
fn export_with(
    source: &mut Connection,
    destination: &Path,
    flag: &AtomicBool,
    mut checkpoint: impl FnMut(&str) -> Result<()>,
) -> Result<Manifest> {
    cancelled(flag)?;
    if !source.is_autocommit() || !source.is_readonly("main")? {
        return Err(Error::SourceContract);
    }
    if destination.extension().and_then(|s| s.to_str()) != Some("mno-export") {
        return Err(Error::InvalidDestination);
    }
    if destination.try_exists()? {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists).into());
    }
    // Dedicated connection settings bound SQLite value/row materialization too.
    let old_limit = source.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    source.set_limit(Limit::SQLITE_LIMIT_LENGTH, old_limit.min(ROW_LIMIT as i32))?;
    let result = export_snapshot(source, destination, flag, &mut checkpoint);
    let reset = source.set_limit(Limit::SQLITE_LIMIT_LENGTH, old_limit);
    match (result, reset) {
        (Ok(m), Ok(_)) => Ok(m),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}
fn export_snapshot(
    source: &mut Connection,
    destination: &Path,
    flag: &AtomicBool,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<Manifest> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut jsonl = tempfile_in(parent)?;
    let mut md = tempfile_in(parent)?;
    let tx = source.transaction_with_behavior(TransactionBehavior::Deferred)?;
    // Establish one main snapshot before checking schema and streaming both views.
    let _: i64 = tx.query_row("SELECT count(*) FROM main.sqlite_schema", [], |r| r.get(0))?;
    let schema_version: u32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(if schema_version == 9 {
        crate::database::bird_requests::validate_schema(&tx)?
    } else if matches!(schema_version, 8 | 9) {
        crate::database::bird_comments::validate_schema(&tx)?
    } else if schema_version == 7 {
        crate::database::bird_visit_identity::validate_schema(&tx)?
    } else if schema_version == 6 {
        crate::database::bird_visits::validate_schema(&tx)?
    } else if schema_version == 5 {
        crate::database::bird_schema::validate_schema(&tx)?
    } else if schema_version == 4 {
        crate::photographs::validate_schema(&tx)?
    } else if schema_version >= 3 {
        crate::database::weather_default_schema::validate_schema3(&tx)?
    } else if schema_version == 2 {
        crate::database::place_schema::validate_schema2(&tx)?
    } else {
        crate::database::validate_supported_schema(&tx)?
    }) {
        return Err(Error::IncompatibleSchema);
    }
    let integrity: String = tx.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::InvalidDatabase);
    }
    if schema_version == 1 {
        crate::storage_content::snapshot(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    } else if schema_version == 2 {
        crate::storage_content::snapshot_v3(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    } else if schema_version == 3 {
        crate::storage_content::snapshot_v4(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 9 {
        crate::storage_content::snapshot_v10(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 8 {
        crate::storage_content::snapshot_v9(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 7 {
        crate::storage_content::snapshot_v8(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 6 {
        crate::storage_content::snapshot_v7(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 5 {
        crate::storage_content::snapshot_v6(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    if schema_version == 4 {
        crate::storage_content::snapshot_v5(&tx, flag).map_err(|_| Error::InvalidDatabase)?;
    }
    cancelled(flag)?;
    checkpoint("snapshot")?;
    md.write_all(b"# MNO Journal\n\nJSONL preserves the exact stored data; this Markdown is a reading view.\n\n")?;
    let mut entry_count = 0u64;
    {
        let mut statement=tx.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            cancelled(flag)?;
            let version = match row.get_ref(4)? {
                ValueRef::Integer(v)
                    if v == 1 || (matches!(schema_version, 4 | 5 | 6 | 7 | 8 | 9) && v == 2) =>
                {
                    v
                }
                _ => return Err(Error::InvalidStorage),
            };
            let e = Entry {
                id: text(row, 0, METADATA_LIMIT)?,
                entry_date: text(row, 1, METADATA_LIMIT)?,
                title: text(row, 2, METADATA_LIMIT)?,
                content_json: text(row, 3, CONTENT_LIMIT)?,
                document_version: version,
                created_at: text(row, 5, METADATA_LIMIT)?,
                updated_at: text(row, 6, METADATA_LIMIT)?,
                weather: if schema_version >= 1 {
                    crate::weather::storage::export_record(&tx, text(row, 0, METADATA_LIMIT)?)
                        .map_err(|_| Error::InvalidStorage)?
                } else {
                    None
                },
            };
            let duplicates = validate_json(e.content_json)?;
            let mut output = BufWriter::new(LimitedWrite {
                file: &mut jsonl,
                remaining: if schema_version == 0 {
                    (ROW_LIMIT as u64) * 6 + 1024
                } else {
                    ((ROW_LIMIT + 2 * crate::weather::storage::PART_LIMIT) as u64) * 6 + 2048
                },
                flag,
            });
            serde_json::to_writer(&mut output, &e).map_err(|e| Error::Io(io::Error::other(e)))?;
            output.write_all(b"\n")?;
            output.flush()?;
            drop(output);
            let mut output = BufWriter::new(LimitedWrite {
                file: &mut md,
                remaining: (ROW_LIMIT as u64) * 8 + 4096,
                flag,
            });
            markdown::entry(&mut output, &e, duplicates)?;
            if let Some(weather) = &e.weather {
                markdown::weather(&mut output, weather)?;
            }
            output.flush()?;
            drop(output);
            entry_count = entry_count.checked_add(1).ok_or(Error::ResourceLimit)?;
            checkpoint("row")?;
        }
    }
    let mut places_file = tempfile_in(parent)?;
    let mut links_file = tempfile_in(parent)?;
    let mut weather_default_file = tempfile_in(parent)?;
    let mut place_count = 0u64;
    let mut link_count = 0u64;
    let mut weather_default_count = 0u64;
    if schema_version >= 2 {
        let mut q=tx.prepare("SELECT id,name,latitude,longitude,revision FROM main.places ORDER BY id COLLATE BINARY")?;
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            cancelled(flag)?;
            let wire = serde_json::json!({"id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"latitude":row.get::<_,Option<f64>>(2)?,"longitude":row.get::<_,Option<f64>>(3)?,"revision":row.get::<_,i64>(4)?});
            serde_json::to_writer(&mut places_file, &wire)
                .map_err(|e| Error::Io(io::Error::other(e)))?;
            places_file.write_all(b"\n")?;
            place_count = place_count.checked_add(1).ok_or(Error::ResourceLimit)?;
        }
        let mut q=tx.prepare("SELECT entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision FROM main.journal_places ORDER BY entry_id COLLATE BINARY")?;
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            cancelled(flag)?;
            let wire = serde_json::json!({"entry_id":row.get::<_,String>(0)?,"place_id":row.get::<_,Option<String>>(1)?,"place_name_snapshot":row.get::<_,Option<String>>(2)?,"latitude_snapshot":row.get::<_,Option<f64>>(3)?,"longitude_snapshot":row.get::<_,Option<f64>>(4)?,"place_revision_snapshot":row.get::<_,Option<i64>>(5)?,"revision":row.get::<_,i64>(6)?});
            serde_json::to_writer(&mut links_file, &wire)
                .map_err(|e| Error::Io(io::Error::other(e)))?;
            links_file.write_all(b"\n")?;
            link_count = link_count.checked_add(1).ok_or(Error::ResourceLimit)?;
        }
        if schema_version >= 3 {
            let mut q = tx.prepare(
                "SELECT singleton,place_id,revision FROM main.weather_default_place ORDER BY singleton",
            )?;
            let mut rows = q.query([])?;
            while let Some(row) = rows.next()? {
                cancelled(flag)?;
                let wire = serde_json::json!({
                    "singleton": row.get::<_,i64>(0)?,
                    "place_id": row.get::<_,Option<String>>(1)?,
                    "revision": row.get::<_,i64>(2)?,
                });
                serde_json::to_writer(&mut weather_default_file, &wire)
                    .map_err(|e| Error::Io(io::Error::other(e)))?;
                weather_default_file.write_all(b"\n")?;
                weather_default_count = weather_default_count
                    .checked_add(1)
                    .ok_or(Error::ResourceLimit)?;
            }
        }
    }
    let mut images_file = tempfile_in(parent)?;
    let mut image_count = 0u64;
    if matches!(schema_version, 4 | 5 | 6 | 7 | 8 | 9) {
        let mut q = tx.prepare("SELECT asset_id,entry_id,original_path,mime,width,height,preview FROM journal_image_assets ORDER BY asset_id COLLATE BINARY")?;
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            cancelled(flag)?;
            let asset = crate::photographs::row(row)?;
            serde_json::to_writer(&mut images_file, &asset)
                .map_err(|e| Error::Io(io::Error::other(e)))?;
            images_file.write_all(b"\n")?;
            image_count += 1;
        }
    }
    let mut bird_files = Vec::new();
    let mut bird_counts = BTreeMap::new();
    if matches!(schema_version, 5 | 6 | 7 | 8 | 9) {
        for (name, sql, fields) in [
            (
                "bird_lists.jsonl",
                if matches!(schema_version, 6 | 7 | 8 | 9) {
                    "SELECT id,visit_date,place_id,visit_start,visit_end FROM bird_lists ORDER BY id COLLATE BINARY"
                } else {
                    "SELECT id,visit_date,place_id FROM bird_lists ORDER BY id COLLATE BINARY"
                },
                if matches!(schema_version, 6 | 7 | 8 | 9) {
                    &["id", "visit_date", "place_id", "visit_start", "visit_end"][..]
                } else {
                    &["id", "visit_date", "place_id"][..]
                },
            ),
            (
                "bird_rows.jsonl",
                if matches!(schema_version, 8 | 9) {
                    "SELECT id,list_id,position,avibase_id,taxonomy_version,count,comment FROM bird_rows ORDER BY id COLLATE BINARY"
                } else {
                    "SELECT id,list_id,position,avibase_id,taxonomy_version,count FROM bird_rows ORDER BY id COLLATE BINARY"
                },
                if matches!(schema_version, 8 | 9) {
                    &[
                        "id",
                        "list_id",
                        "position",
                        "avibase_id",
                        "taxonomy_version",
                        "count",
                        "comment",
                    ][..]
                } else {
                    &[
                        "id",
                        "list_id",
                        "position",
                        "avibase_id",
                        "taxonomy_version",
                        "count",
                    ][..]
                },
            ),
            (
                "journal_bird_lists.jsonl",
                "SELECT entry_id,list_id FROM journal_bird_lists ORDER BY entry_id COLLATE BINARY,list_id COLLATE BINARY",
                &["entry_id", "list_id"][..],
            ),
        ] {
            let mut file = tempfile_in(parent)?;
            let mut query = tx.prepare(sql)?;
            let mut rows = query.query([])?;
            let mut count = 0u64;
            while let Some(row) = rows.next()? {
                cancelled(flag)?;
                let mut record = serde_json::Map::new();
                for (i, key) in fields.iter().enumerate() {
                    let value = match row.get_ref(i)? {
                        ValueRef::Text(_) => serde_json::Value::String(row.get(i)?),
                        ValueRef::Integer(n) => serde_json::Value::from(n),
                        ValueRef::Null => serde_json::Value::Null,
                        _ => return Err(Error::InvalidStorage),
                    };
                    record.insert((*key).into(), value);
                }
                serde_json::to_writer(&mut file, &record)
                    .map_err(|e| Error::Io(io::Error::other(e)))?;
                file.write_all(b"\n")?;
                count = count.checked_add(1).ok_or(Error::ResourceLimit)?;
            }
            bird_counts.insert(name, count);
            bird_files.push((name, file));
        }
    }
    if schema_version == 9 {
        let mut file = tempfile_in(parent)?;
        let mut count = 0;
        let mut q=tx.prepare("SELECT request_id,payload_sha256,completed FROM bird_creation_requests ORDER BY request_id COLLATE BINARY")?;
        let mut rows = q.query([])?;
        while let Some(row) = rows.next()? {
            cancelled(flag)?;
            let record = serde_json::json!({"request_id":row.get::<_,String>(0)?,"payload_sha256":row.get::<_,Option<String>>(1)?,"completed":row.get::<_,i64>(2)?});
            serde_json::to_writer(&mut file, &record)
                .map_err(|e| Error::Io(io::Error::other(e)))?;
            file.write_all(b"\n")?;
            count += 1;
        }
        file.flush()?;
        file.sync_all()?;
        bird_counts.insert("bird_creation_requests.jsonl", count);
        bird_files.push(("bird_creation_requests.jsonl", file));
    }
    cancelled(flag)?;
    tx.commit()?; // Never publish before successful snapshot completion.
    let mut readme = tempfile_in(parent)?;
    if schema_version == 0 {
        readme.write_all(README.as_bytes())?;
    } else if schema_version == 9 {
        readme.write_all(b"# MNO open export v10\n\nPreserves all content plus durable bird_creation_requests receipts, including deleted-list IDs. Receipts must not be discarded; checksums and counts are in manifest.json. Original files are not included or followed.\n")?;
    } else if schema_version == 8 {
        readme.write_all(b"# MNO open export v9\n\nAuthoritative JSONL preserves all observation visits and rows, including nullable plain-text row comments, optional visit times, namespaced species identifiers, versions, ordering, links, places, weather, journal documents and stored image previews. Original files are not included or followed. The archive is not encrypted. Checksums and row counts are in manifest.json.\n")?;
    } else if schema_version == 7 {
        readme.write_all(b"# MNO open export v8\n\nEach bird list is an independent visit, including lists with the same place/date. JSONL preserves IDs, rows, links, optional local visit times, journal, images, places and weather. Checksums and row counts are in manifest.json. Original photo files are not included or followed. The archive is not encrypted. journal.md is a reading view.\n")?;
    } else if schema_version == 6 {
        readme.write_all(b"# MNO open export v7\n\nJSONL preserves journal, images, places, weather, all bird lists, species rows, links and optional local visit_start/visit_end times (HH:MM or null). IDs and taxonomy versions are preserved. Original photo files are not included or followed. The archive is not encrypted. journal.md is a reading view.\n")?;
    } else if schema_version == 5 {
        readme.write_all(b"# MNO open export v6\n\nJSONL files are authoritative. bird_lists.jsonl, bird_rows.jsonl and journal_bird_lists.jsonl preserve all observation lists, including unlinked lists, ordered species rows and journal links. Existing journal, weather, places and images payloads are retained. images.jsonl includes preview bytes and original-file references; original files are never included or followed. This archive is not encrypted. journal.md is a reading view.\n")?;
    } else if schema_version == 4 {
        readme.write_all(b"# MNO open export v5\n\nJSONL files are authoritative. images.jsonl contains per-entry metadata, complete original paths and base64-encoded JPEG/PNG preview bytes, including unused assets. Original files are never included or followed. This archive is not encrypted; paths may be private. journal.md is a text reading view; image rendering is not implemented.\n")?;
    } else if schema_version >= 3 {
        readme.write_all(b"# MNO open export v4\n\nThe authoritative journal.jsonl, places.jsonl, journal_places.jsonl and weather_default_place.jsonl preserve journal, weather, place data and the optional weather-default setting. journal.md is a reading view. Checksums and row counts are in manifest.json. This archive is not encrypted. External files are not included.\n")?;
    } else if schema_version == 2 {
        readme.write_all(b"# MNO open export v3\n\nThe authoritative journal.jsonl, places.jsonl and journal_places.jsonl preserve journal, weather and place data. journal.md is a reading view. Checksums and row counts are in manifest.json. This archive is not encrypted.\n")?;
    } else {
        readme.write_all(b"# MNO open export v2\n\nThe authoritative journal.jsonl preserves all seven journal fields and optional weather records, including independent manual/fetched JSON snapshots and their revision. Missing weather means no record. Canonical units are Celsius, metres/second, millimetres and percentages. No settings, caches, external files or unsaved editor content are included. journal.md is a reading view. Each payload has a size and SHA-256 checksum in manifest.json. This archive is not encrypted.\n")?;
    }
    let mut payloads = vec![
        ("journal.jsonl", jsonl),
        ("journal.md", md),
        ("README.md", readme),
    ];
    if schema_version >= 2 {
        payloads.push(("places.jsonl", places_file));
        payloads.push(("journal_places.jsonl", links_file));
    }
    if schema_version >= 3 {
        payloads.push(("weather_default_place.jsonl", weather_default_file));
    }
    if matches!(schema_version, 4 | 5 | 6 | 7 | 8 | 9) {
        payloads.push(("images.jsonl", images_file));
    }
    payloads.extend(bird_files);
    let mut files = BTreeMap::new();
    for (name, file) in &mut payloads {
        let mut r = record(file, flag)?;
        if schema_version >= 2 {
            if matches!(
                *name,
                "places.jsonl"
                    | "journal_places.jsonl"
                    | "weather_default_place.jsonl"
                    | "bird_lists.jsonl"
                    | "bird_rows.jsonl"
                    | "journal_bird_lists.jsonl"
                    | "bird_creation_requests.jsonl"
            ) && r.size > 512 * 1024 * 1024
            {
                return Err(Error::ResourceLimit);
            }
            r.row_count = match *name {
                "images.jsonl" => Some(image_count),
                "journal.jsonl" => Some(entry_count),
                "places.jsonl" => Some(place_count),
                "journal_places.jsonl" => Some(link_count),
                "weather_default_place.jsonl" => Some(weather_default_count),
                name => bird_counts.get(name).copied(),
            };
        }
        files.insert(*name, r);
    }
    let manifest = Manifest {
        format: "mno-export",
        format_version: schema_version + 1,
        database_schema_version: schema_version,
        document_version: if matches!(schema_version, 4 | 5 | 6 | 7 | 8 | 9) {
            2
        } else {
            1
        },
        entry_count,
        files,
        external_originals_included: false,
    };
    let mut archive = NamedTempFile::new_in(parent)?;
    {
        let mut zip = ZipWriter::new(archive.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        zip.start_file("manifest.json", options)?;
        serde_json::to_writer_pretty(&mut zip, &manifest)
            .map_err(|e| Error::Io(io::Error::other(e)))?;
        for (name, file) in &mut payloads {
            cancelled(flag)?;
            zip.start_file(name, options)?;
            let mut buf = [0u8; 65536];
            loop {
                cancelled(flag)?;
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                zip.write_all(&buf[..n])?;
                checkpoint("archive-block")?;
            }
        }
        zip.finish()?;
    }
    archive.as_file().sync_all()?;
    checkpoint("before-publish")?;
    cancelled(flag)?;
    // Only this fully finished archive can acquire the requested final name.
    // A collision never replaces an existing file. No pathname reacquisition.
    archive
        .persist_noclobber(destination)
        .map_err(|e| Error::Io(e.error))?;
    checkpoint("published")?;
    Ok(manifest)
}
