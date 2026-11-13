//! Open-export v1 reader. Builds ONLY a private, separate schema-0 candidate.
//! No active database path, startup authority or activation capability is accepted.
use crate::open_export::{CONTENT_LIMIT, METADATA_LIMIT, ROW_LIMIT, validate_json};
use rusqlite::{Connection, OpenFlags, params, types::ValueRef};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
const NAMES: [&str; 4] = ["manifest.json", "journal.jsonl", "journal.md", "README.md"];
const MANIFEST_LIMIT: u64 = 16 * 1024;
// Includes worst-case JSON string escaping, as in the export writer.
const LINE_LIMIT: usize = ROW_LIMIT * 6 + 1024;
const LINE_LIMIT_V2: usize = (ROW_LIMIT + 2 * crate::weather::storage::PART_LIMIT) * 6 + 2048;
mod v3;
pub(crate) use v3::{
    build_v3, build_v4, build_v5, build_v6, build_v7, build_v8, build_v9, build_v10, is_v3, is_v4,
    is_v5, is_v6, is_v7, is_v8, is_v9, is_v10,
};
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub(crate) enum Error {
    Io(io::Error),
    Sql(rusqlite::Error),
    Archive,
    Manifest,
    Record,
    ResourceLimit,
    Checksum,
    Verification,
    Cancelled,
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRecord {
    size: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    format_version: u32,
    database_schema_version: u32,
    document_version: u32,
    entry_count: u64,
    #[serde(deserialize_with = "unique_files")]
    files: BTreeMap<String, FileRecord>,
    external_originals_included: bool,
}
// serde's default map accepts duplicate keys; a manifest must not be ambiguous.
fn unique_files<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<BTreeMap<String, FileRecord>, D::Error> {
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = BTreeMap<String, FileRecord>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("three unique payload records")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut a: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut files = BTreeMap::new();
            while let Some((name, value)) = a.next_entry::<String, FileRecord>()? {
                if !["journal.jsonl", "journal.md", "README.md"].contains(&name.as_str())
                    || files.insert(name, value).is_some()
                {
                    return Err(serde::de::Error::custom(
                        "unexpected or duplicate payload record",
                    ));
                }
            }
            Ok(files)
        }
    }
    d.deserialize_map(Files)
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    entry_date: String,
    title: String,
    content_json: String,
    document_version: i64,
    created_at: String,
    updated_at: String,
}
// No connection escapes. The owned private directory keeps the verified file alive.
// This is NOT PreparedRestore and grants no activation/Ready authority.
#[derive(Debug)]
pub(crate) struct Candidate {
    path: PathBuf,
    _directory: tempfile::TempDir,
    pub(crate) entry_count: u64,
    proof: ContentProof,
}
impl Candidate {
    pub(crate) fn proof(&self) -> &ContentProof {
        &self.proof
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
// Only these two source contracts can produce a schema-5 import candidate.
// Both preview and confirmed activation use this exact path.
pub(crate) fn build_for_schema5(
    archive: &mut File,
    parent: &Path,
    flag: &AtomicBool,
    source_format: u32,
) -> Result<Candidate> {
    match source_format {
        5 => normalize_v5_candidate(build_v5(archive, parent, flag)?, flag),
        6 => build_v6(archive, parent, flag),
        _ => Err(Error::Manifest),
    }
}
fn normalize_v5_candidate(mut candidate: Candidate, flag: &AtomicBool) -> Result<Candidate> {
    cancel(flag)?;
    let original = candidate.proof.clone();
    if original.version != 5 {
        return Err(Error::Verification);
    }
    let mut db = Database(Some(Connection::open_with_flags(
        candidate.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    // Recheck the stored private candidate against its frozen source proof.
    if verify_database_version(db.get(), flag, 5)? != original {
        return Err(Error::Verification);
    }
    crate::database::bird_schema::upgrade(db.get()).map_err(|_| Error::Verification)?;
    if !crate::database::bird_schema::validate_schema(db.get())? {
        return Err(Error::Verification);
    }
    let tx = db.get().transaction()?;
    let preserved =
        crate::storage_content::verified_snapshot_v5(&tx, flag).map_err(|_| Error::Verification)?;
    if preserved.sha256 != original.sha256 || preserved.entry_count != original.entry_count {
        return Err(Error::Verification);
    }
    tx.commit()?;
    drop(db); // Checked close before independent readback and the new proof.
    let mut reopened = Database(Some(Connection::open_with_flags(
        candidate.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    candidate.proof = verify_database_version(reopened.get(), flag, 6)?;
    if candidate.proof.entry_count != candidate.entry_count {
        return Err(Error::Verification);
    }
    drop(reopened);
    cancel(flag)?;
    Ok(candidate)
}
pub(crate) fn build_for_schema6(
    a: &mut File,
    p: &Path,
    flag: &AtomicBool,
    source: u32,
) -> Result<Candidate> {
    if source == 7 {
        return build_v7(a, p, flag);
    }
    let mut c = build_for_schema5(a, p, flag, source)?;
    let old = c.proof.clone();
    let mut db = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    if verify_database_version(db.get(), flag, 6)? != old {
        return Err(Error::Verification);
    }
    crate::database::bird_visits::upgrade(db.get()).map_err(|_| Error::Verification)?;
    let tx = db.get().transaction()?;
    let preserved =
        crate::storage_content::verified_snapshot_v6(&tx, flag).map_err(|_| Error::Verification)?;
    if preserved.sha256 != old.sha256 || preserved.entry_count != old.entry_count {
        return Err(Error::Verification);
    }
    tx.commit()?;
    drop(db);
    let mut read = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    c.proof = verify_database_version(read.get(), flag, 7)?;
    drop(read);
    Ok(c)
}
pub(crate) fn build_for_schema7(
    a: &mut File,
    p: &Path,
    flag: &AtomicBool,
    source: u32,
) -> Result<Candidate> {
    if source == 8 {
        return build_v8(a, p, flag);
    }
    let mut c = build_for_schema6(a, p, flag, source)?;
    let old = c.proof.clone();
    let mut db = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    if verify_database_version(db.get(), flag, 7)? != old {
        return Err(Error::Verification);
    }
    crate::database::bird_visit_identity::upgrade(db.get()).map_err(|_| Error::Verification)?;
    let tx = db.get().transaction()?;
    let preserved =
        crate::storage_content::verified_snapshot_v7(&tx, flag).map_err(|_| Error::Verification)?;
    if preserved.sha256 != old.sha256 || preserved.entry_count != old.entry_count {
        return Err(Error::Verification);
    }
    tx.commit()?;
    drop(db);
    let mut read = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    c.proof = verify_database_version(read.get(), flag, 8)?;
    drop(read);
    Ok(c)
}
pub(crate) fn build_for_schema8(
    a: &mut File,
    p: &Path,
    f: &AtomicBool,
    source: u32,
) -> Result<Candidate> {
    if source == 9 {
        return build_v9(a, p, f);
    }
    let mut c = build_for_schema7(a, p, f, source)?;
    let old = c.proof.clone();
    let mut db = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    if verify_database_version(db.get(), f, 8)? != old {
        return Err(Error::Verification);
    }
    crate::database::bird_comments::upgrade(db.get()).map_err(|_| Error::Verification)?;
    let tx = db.get().transaction()?;
    let preserved =
        crate::storage_content::verified_snapshot_v8(&tx, f).map_err(|_| Error::Verification)?;
    if preserved.sha256 != old.sha256 || preserved.entry_count != old.entry_count {
        return Err(Error::Verification);
    }
    tx.commit()?;
    drop(db);
    let mut read = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    c.proof = verify_database_version(read.get(), f, 9)?;
    drop(read);
    Ok(c)
}
pub(crate) fn build_for_schema9(
    a: &mut File,
    p: &Path,
    f: &AtomicBool,
    source: u32,
) -> Result<Candidate> {
    if source == 10 {
        return build_v10(a, p, f);
    }
    let mut c = build_for_schema8(a, p, f, source)?;
    let old = c.proof.clone();
    let mut db = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    if verify_database_version(db.get(), f, 9)? != old {
        return Err(Error::Verification);
    }
    crate::database::bird_requests::upgrade(db.get()).map_err(|_| Error::Verification)?;
    let tx = db.get().transaction()?;
    let preserved =
        crate::storage_content::verified_snapshot_v9(&tx, f).map_err(|_| Error::Verification)?;
    if preserved.sha256 != old.sha256 || preserved.entry_count != old.entry_count {
        return Err(Error::Verification);
    }
    tx.commit()?;
    drop(db);
    let mut read = Database(Some(Connection::open_with_flags(
        c.path(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    c.proof = verify_database_version(read.get(), f, 10)?;
    drop(read);
    Ok(c)
}
// A close failure must not release directory resources ahead of the original handle.
struct Database(Option<Connection>);
impl Database {
    fn get(&mut self) -> &mut Connection {
        self.0.as_mut().unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        if let Some(db) = self.0.take() {
            if let Err(_original_and_error) = db.close() {
                std::process::abort();
            }
        }
    }
}
fn cancel(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
// Borrow the caller's archive descriptor: no pathname reacquisition, clone or dup.
// The caller is responsible for acquiring/closing a genuine archive safely.
pub(crate) fn build(archive: &mut File, parent: &Path, flag: &AtomicBool) -> Result<Candidate> {
    build_with(archive, parent, flag, |_| Ok(()))
}
fn check_directory(archive: &mut File) -> Result<()> {
    let mut buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
    let zip =
        rawzip::ZipArchive::from_seekable(archive, &mut buffer).map_err(|_| Error::Archive)?;
    if zip.entries_hint() != 4 {
        return Err(Error::Archive);
    }
    let mut entries = zip.entries(&mut buffer);
    let mut names = BTreeSet::new();
    while let Some(entry) = entries.next_entry().map_err(|_| Error::Archive)? {
        let name = entry.file_path().as_bytes();
        if !NAMES.iter().any(|n| n.as_bytes() == name) || !names.insert(name.to_vec()) {
            return Err(Error::Archive);
        }
    }
    if names.len() != 4 {
        return Err(Error::Archive);
    }
    Ok(())
}
fn manifest(bytes: &[u8]) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(bytes).map_err(|_| Error::Manifest)?;
    if m.format != "mno-export"
        || !matches!(
            (m.format_version, m.database_schema_version),
            (1, 0) | (2, 1)
        )
        || m.document_version != 1
        || m.external_originals_included
        || m.files.keys().map(String::as_str).collect::<Vec<_>>()
            != ["README.md", "journal.jsonl", "journal.md"]
    {
        return Err(Error::Manifest);
    }
    for record in m.files.values() {
        if record.sha256.len() != 64
            || !record
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Manifest);
        }
    }
    Ok(m)
}
fn next_record(reader: &mut impl BufRead, flag: &AtomicBool) -> Result<Option<Record>> {
    cancel(flag)?;
    let mut line = Vec::new();
    reader
        .take(LINE_LIMIT as u64 + 1)
        .read_until(b'\n', &mut line)?;
    if line.is_empty() {
        return Ok(None);
    }
    if line.len() > LINE_LIMIT {
        return Err(Error::ResourceLimit);
    }
    if line.last() != Some(&b'\n') {
        return Err(Error::Record);
    }
    let r: Record = serde_json::from_slice(&line).map_err(|_| Error::Record)?;
    validate_record(&r)?;
    Ok(Some(r))
}
fn validate_record(r: &Record) -> Result<()> {
    if r.document_version != 1 {
        return Err(Error::Record);
    }
    for text in [&r.id, &r.entry_date, &r.title, &r.created_at, &r.updated_at] {
        if text.len() > METADATA_LIMIT {
            return Err(Error::ResourceLimit);
        }
    }
    if r.content_json.len() > CONTENT_LIMIT {
        return Err(Error::ResourceLimit);
    }
    validate_json(&r.content_json).map_err(|e| match e {
        crate::open_export::Error::ResourceLimit => Error::ResourceLimit,
        _ => Error::Record,
    })?;
    Ok(())
}

fn build_with(
    archive: &mut File,
    parent: &Path,
    flag: &AtomicBool,
    mut point: impl FnMut(&str) -> Result<()>,
) -> Result<Candidate> {
    cancel(flag)?;
    if !archive.metadata()?.is_file() {
        return Err(Error::Archive);
    }
    check_directory(archive)?; // Streaming scan before zip's allocating name index.
    let mut zip = zip::ZipArchive::new(archive).map_err(|_| Error::Archive)?;
    if zip.len() != 4 {
        return Err(Error::Archive);
    }
    for i in 0..zip.len() {
        let e = zip.by_index(i).map_err(|_| Error::Archive)?;
        if !NAMES.iter().any(|n| n.as_bytes() == e.name_raw())
            || e.encrypted()
            || e.compression() != zip::CompressionMethod::Stored
            || e.compressed_size() != e.size()
            || e.unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
        {
            return Err(Error::Archive);
        }
    }
    let m = {
        let mut entry = zip.by_name("manifest.json").map_err(|_| Error::Archive)?;
        if entry.size() > MANIFEST_LIMIT {
            return Err(Error::ResourceLimit);
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MANIFEST_LIMIT {
            return Err(Error::ResourceLimit);
        }
        manifest(&bytes)?
    };
    // Private per-operation directory. Never extract an archive-controlled filename.
    let directory = tempfile::Builder::new()
        .prefix(".mno-import-")
        .tempdir_in(parent)?;
    let mut journal = tempfile::tempfile_in(directory.path())?;
    for name in ["journal.jsonl", "journal.md", "README.md"] {
        cancel(flag)?;
        let expected = &m.files[name];
        let mut entry = zip.by_name(name).map_err(|_| Error::Archive)?;
        if entry.size() != expected.size {
            return Err(Error::Checksum);
        }
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut block = [0u8; 65536];
        loop {
            cancel(flag)?;
            let n = entry.read(&mut block)?; // Reads through EOF, including ZIP CRC verification.
            if n == 0 {
                break;
            }
            size = size.checked_add(n as u64).ok_or(Error::ResourceLimit)?;
            if size > expected.size {
                return Err(Error::Checksum);
            }
            hash.update(&block[..n]);
            if name == "journal.jsonl" {
                journal.write_all(&block[..n])?;
            }
        }
        if size != expected.size || format!("{:x}", hash.finalize()) != expected.sha256 {
            return Err(Error::Checksum);
        }
    }
    point("package-verified")?;
    cancel(flag)?;
    let path = directory.path().join("candidate.sqlite");
    // v1 reader preserves its original schema-0 candidate semantics.
    crate::database::create_legacy_database(&path).map_err(|_| Error::Verification)?;
    if m.format_version == 2 {
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        crate::database::weather_schema::migrate(&mut db).map_err(|_| Error::Verification)?;
    }
    let mut db = Database(Some(Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    let count = {
        let tx = db.get().transaction()?;
        journal.seek(SeekFrom::Start(0))?;
        let mut input = BufReader::new(&mut journal);
        let mut count = 0u64;
        let mut previous: Option<String> = None;
        while let Some((r, weather)) = next_record_version(&mut input, flag, m.format_version)? {
            // Export format specifies BINARY ID order. This also rejects duplicate IDs
            // without a journal-sized in-memory set; SQLite's PK independently enforces it.
            if previous.as_ref().is_some_and(|id| id >= &r.id) {
                return Err(Error::Record);
            }
            point("before-insert")?;
            tx.execute("INSERT INTO main.journal_entries (id,entry_date,title,content_json,document_version,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![r.id,r.entry_date,r.title,r.content_json,r.document_version,r.created_at,r.updated_at])?;
            if let Some(w) = weather {
                validate_weather(&r.id, &w)?;
                tx.execute(
                    "INSERT INTO main.journal_weather VALUES (?1,?2,?3,?4,?5)",
                    params![
                        r.id,
                        w.format_version,
                        w.manual_json,
                        w.fetched_json,
                        w.revision
                    ],
                )?;
            }
            previous = Some(r.id);
            count = count.checked_add(1).ok_or(Error::ResourceLimit)?;
            point("row")?;
        }
        if count != m.entry_count {
            return Err(Error::Manifest);
        }
        cancel(flag)?;
        point("before-commit")?;
        tx.commit()?;
        count
    };
    point("committed")?;
    drop(db); // Checked close before read-only post-commit verification.
    let mut db = Database(Some(Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    let mut proof = ProofHash::new();
    {
        let tx = db.get().transaction()?;
        if !(if m.format_version == 1 {
            crate::database::validate_schema(&tx)?
        } else {
            crate::database::validate_supported_schema(&tx)?
        }) {
            return Err(Error::Verification);
        }
        let integrity: String = tx.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(Error::Verification);
        }
        journal.seek(SeekFrom::Start(0))?;
        let mut input = BufReader::new(&mut journal);
        let mut stmt = tx.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY")?;
        let mut rows = stmt.query([])?;
        while let Some((expected, weather)) =
            next_record_version(&mut input, flag, m.format_version)?
        {
            let row = rows.next()?.ok_or(Error::Verification)?;
            proof.row(&expected)?;
            if m.format_version == 2 {
                let actual = crate::weather::storage::export_record(&tx, &expected.id)
                    .map_err(|_| Error::Verification)?;
                if actual != weather {
                    return Err(Error::Verification);
                }
            }
            for (i, text) in [
                (0, &expected.id),
                (1, &expected.entry_date),
                (2, &expected.title),
                (3, &expected.content_json),
                (5, &expected.created_at),
                (6, &expected.updated_at),
            ] {
                if !matches!(row.get_ref(i)?, ValueRef::Text(b) if b == text.as_bytes()) {
                    return Err(Error::Verification);
                }
            }
            if !matches!(row.get_ref(4)?, ValueRef::Integer(v) if v == expected.document_version) {
                return Err(Error::Verification);
            }
        }
        if rows.next()?.is_some() {
            return Err(Error::Verification);
        }
        drop(rows);
        drop(stmt);
        tx.commit()?;
    }
    let current_proof = if m.format_version == 2 {
        Some(verify_database_version(db.get(), flag, 2)?)
    } else {
        None
    };
    drop(db);
    cancel(flag)?;
    point("verified")?;
    cancel(flag)?;
    Ok(Candidate {
        path,
        _directory: directory,
        entry_count: count,
        proof: current_proof.unwrap_or_else(|| proof.finish()),
    })
}
// A separate import contract, never the strict restore fingerprint v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContentProof {
    pub(crate) version: u32,
    pub(crate) sha256: String,
    pub(crate) entry_count: u64,
}
// Frozen import proof v1: domain, row 0x52, six TEXTs tagged 3 + BE u64
// byte length + original UTF-8, INTEGER tagged 1 + BE i64 in column order;
// final 0x45 + BE u64 row count. No JSON normalization.
const PROOF_DOMAIN: &[u8] = b"MNO-open-import-content\0\x00\x00\x00\x01schema-0\0journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0";
struct ProofHash {
    hash: Sha256,
    count: u64,
}
impl ProofHash {
    fn new() -> Self {
        Self {
            hash: Sha256::new().chain_update(PROOF_DOMAIN),
            count: 0,
        }
    }
    fn row(&mut self, r: &Record) -> Result<()> {
        self.hash.update([0x52]);
        for text in [&r.id, &r.entry_date, &r.title, &r.content_json] {
            self.text(text);
        }
        self.hash.update([1]);
        self.hash.update(r.document_version.to_be_bytes());
        for text in [&r.created_at, &r.updated_at] {
            self.text(text);
        }
        self.count = self.count.checked_add(1).ok_or(Error::ResourceLimit)?;
        Ok(())
    }
    fn text(&mut self, s: &str) {
        self.hash.update([3]);
        self.hash.update((s.len() as u64).to_be_bytes());
        self.hash.update(s.as_bytes());
    }
    fn finish(mut self) -> ContentProof {
        self.hash.update([0x45]);
        self.hash.update(self.count.to_be_bytes());
        ContentProof {
            version: 1,
            sha256: format!("{:x}", self.hash.finalize()),
            entry_count: self.count,
        }
    }
}
// Import-only validation within one owned read snapshot. Does not mint a strict
// VerifiedJournal or any activation/startup capability.
pub(crate) fn verify_database(db: &mut Connection, flag: &AtomicBool) -> Result<ContentProof> {
    use rusqlite::limits::Limit;
    cancel(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Verification);
    }
    // Bound SQLite materialization too, before reading an untrusted restart file.
    let old = db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old.min(ROW_LIMIT as i32))?;
    let result = verify_snapshot(db, flag);
    let reset = db.set_limit(Limit::SQLITE_LIMIT_LENGTH, old);
    match (result, reset) {
        (Ok(proof), Ok(_)) => Ok(proof),
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(e.into()),
    }
}
fn verify_snapshot(db: &mut Connection, flag: &AtomicBool) -> Result<ContentProof> {
    cancel(flag)?;
    if !db.is_autocommit() {
        return Err(Error::Verification);
    }
    let tx = db.transaction()?;
    if !crate::database::validate_schema(&tx)? {
        return Err(Error::Verification);
    }
    let integrity: String = tx.query_row("PRAGMA main.integrity_check(1)", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::Verification);
    }
    let mut proof = ProofHash::new();
    {
        let mut stmt=tx.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM main.journal_entries ORDER BY id COLLATE BINARY")?;
        let mut rows = stmt.query([])?;
        let mut previous: Option<String> = None;
        while let Some(row) = rows.next()? {
            cancel(flag)?;
            let text = |i, limit| -> Result<String> {
                match row.get_ref(i)? {
                    ValueRef::Text(b) if b.len() <= limit => Ok(std::str::from_utf8(b)
                        .map_err(|_| Error::Record)?
                        .to_owned()),
                    ValueRef::Text(_) => Err(Error::ResourceLimit),
                    _ => Err(Error::Record),
                }
            };
            let version = match row.get_ref(4)? {
                ValueRef::Integer(v) => v,
                _ => return Err(Error::Record),
            };
            let r = Record {
                id: text(0, METADATA_LIMIT)?,
                entry_date: text(1, METADATA_LIMIT)?,
                title: text(2, METADATA_LIMIT)?,
                content_json: text(3, CONTENT_LIMIT)?,
                document_version: version,
                created_at: text(5, METADATA_LIMIT)?,
                updated_at: text(6, METADATA_LIMIT)?,
            };
            validate_record(&r)?;
            if previous.as_ref().is_some_and(|id| id >= &r.id) {
                return Err(Error::Record);
            }
            proof.row(&r)?;
            previous = Some(r.id);
        }
    }
    cancel(flag)?;
    tx.commit()?;
    Ok(proof.finish())
}
#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordV2 {
    id: String,
    entry_date: String,
    title: String,
    content_json: String,
    document_version: i64,
    created_at: String,
    updated_at: String,
    weather: Option<crate::weather::storage::WeatherRecord>,
}
fn validate_weather(id: &str, w: &crate::weather::storage::WeatherRecord) -> Result<()> {
    if w.format_version != 1 || w.revision < 1 {
        return Err(Error::Record);
    }
    crate::weather::storage::validate(id, w.manual_json.as_deref(), w.fetched_json.as_deref())
        .map_err(|_| Error::Record)
}
fn next_record_version(
    reader: &mut impl BufRead,
    flag: &AtomicBool,
    version: u32,
) -> Result<Option<(Record, Option<crate::weather::storage::WeatherRecord>)>> {
    if version == 1 {
        return next_record(reader, flag).map(|r| r.map(|r| (r, None)));
    }
    cancel(flag)?;
    let mut line = Vec::new();
    reader
        .take(LINE_LIMIT_V2 as u64 + 1)
        .read_until(b'\n', &mut line)?;
    if line.is_empty() {
        return Ok(None);
    }
    if line.len() > LINE_LIMIT_V2 {
        return Err(Error::ResourceLimit);
    }
    if line.last() != Some(&b'\n') {
        return Err(Error::Record);
    }
    let wire: RecordV2 = serde_json::from_slice(&line).map_err(|_| Error::Record)?;
    let r = Record {
        id: wire.id,
        entry_date: wire.entry_date,
        title: wire.title,
        content_json: wire.content_json,
        document_version: wire.document_version,
        created_at: wire.created_at,
        updated_at: wire.updated_at,
    };
    if matches!(version, 5 | 6 | 7 | 8 | 9 | 10) && r.document_version == 2 {
        let mut legacy = r.clone();
        legacy.document_version = 1;
        validate_record(&legacy)?;
        crate::photographs::validate_document(&r.content_json).map_err(|_| Error::Record)?;
    } else {
        validate_record(&r)?;
    }
    if let Some(w) = &wire.weather {
        validate_weather(&r.id, w)?;
    }
    Ok(Some((r, wire.weather)))
}
// Upgrade only private candidates, after the original archive proof succeeded.
// No PreparedRestore binding or active journal is changed here.
pub(crate) fn build_current(
    archive: &mut File,
    parent: &Path,
    flag: &AtomicBool,
) -> Result<Candidate> {
    let mut candidate = build(archive, parent, flag)?;
    let mut db = Database(Some(Connection::open_with_flags(
        candidate.path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    crate::database::weather_schema::migrate(db.get()).map_err(|_| Error::Verification)?;
    candidate.proof = verify_database_version(db.get(), flag, 2)?;
    drop(db);
    cancel(flag)?;
    Ok(candidate)
}
pub(crate) fn verify_database_version(
    db: &mut Connection,
    flag: &AtomicBool,
    version: u32,
) -> Result<ContentProof> {
    match version {
        1 => verify_database(db, flag),
        2 => crate::storage_content::verify(db, flag)
            .map(|p| ContentProof {
                version: 2,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|e| match e {
                crate::storage_content::Error::Sql(e) => Error::Sql(e),
                crate::storage_content::Error::Cancelled => Error::Cancelled,
                crate::storage_content::Error::Limit => Error::ResourceLimit,
                _ => Error::Verification,
            }),
        3 => crate::storage_content::verify_v3(db, flag)
            .map(|p| ContentProof {
                version: 3,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|e| match e {
                crate::storage_content::Error::Sql(e) => Error::Sql(e),
                crate::storage_content::Error::Cancelled => Error::Cancelled,
                crate::storage_content::Error::Limit => Error::ResourceLimit,
                _ => Error::Verification,
            }),
        4 => crate::storage_content::verify_v4(db, flag)
            .map(|p| ContentProof {
                version: 4,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|e| match e {
                crate::storage_content::Error::Sql(e) => Error::Sql(e),
                crate::storage_content::Error::Cancelled => Error::Cancelled,
                crate::storage_content::Error::Limit => Error::ResourceLimit,
                _ => Error::Verification,
            }),
        5 => crate::storage_content::verify_v5(db, flag)
            .map(|p| ContentProof {
                version: 5,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|e| match e {
                crate::storage_content::Error::Sql(e) => Error::Sql(e),
                crate::storage_content::Error::Cancelled => Error::Cancelled,
                crate::storage_content::Error::Limit => Error::ResourceLimit,
                _ => Error::Verification,
            }),
        6 => crate::storage_content::verify_v6(db, flag)
            .map(|p| ContentProof {
                version: 6,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|e| match e {
                crate::storage_content::Error::Sql(e) => Error::Sql(e),
                crate::storage_content::Error::Cancelled => Error::Cancelled,
                crate::storage_content::Error::Limit => Error::ResourceLimit,
                _ => Error::Verification,
            }),
        10 => crate::storage_content::verify_v10(db, flag)
            .map(|p| ContentProof {
                version: 10,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|_| Error::Verification),
        9 => crate::storage_content::verify_v9(db, flag)
            .map(|p| ContentProof {
                version: 9,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|_| Error::Verification),
        8 => crate::storage_content::verify_v8(db, flag)
            .map(|p| ContentProof {
                version: 8,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|_| Error::Verification),
        7 => crate::storage_content::verify_v7(db, flag)
            .map(|p| ContentProof {
                version: 7,
                sha256: p.sha256,
                entry_count: p.entry_count,
            })
            .map_err(|_| Error::Verification),
        _ => Err(Error::Verification),
    }
}
