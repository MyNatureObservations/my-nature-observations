//! Current open export contract. Builds a private schema-2 database only.
use super::*;
use serde::Deserialize;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

const NAMES_V3: [&str; 6] = [
    "manifest.json",
    "journal.jsonl",
    "journal.md",
    "README.md",
    "places.jsonl",
    "journal_places.jsonl",
];
const NAMES_V4: [&str; 7] = [
    "manifest.json",
    "journal.jsonl",
    "journal.md",
    "README.md",
    "places.jsonl",
    "journal_places.jsonl",
    "weather_default_place.jsonl",
];
const NAMES_V5: [&str; 8] = [
    "manifest.json",
    "journal.jsonl",
    "journal.md",
    "README.md",
    "places.jsonl",
    "journal_places.jsonl",
    "weather_default_place.jsonl",
    "images.jsonl",
];
// Snapshots are preserved values, not editor input. Match proof-v3's bounded
// SQLite row allowance, including historical values created outside the UI.
const PLACE_LINE_LIMIT: usize = crate::storage_content::ROW_LIMIT;
const MAX_PLACE_PAYLOAD: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV3 {
    size: u64,
    sha256: String,
    #[serde(default)]
    row_count: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV3 {
    format: String,
    format_version: u32,
    database_schema_version: u32,
    document_version: u32,
    entry_count: u64,
    #[serde(deserialize_with = "unique_files_v3")]
    files: BTreeMap<String, FileV3>,
    external_originals_included: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Place {
    id: String,
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    entry_id: String,
    place_id: Option<String>,
    place_name_snapshot: Option<String>,
    latitude_snapshot: Option<f64>,
    longitude_snapshot: Option<f64>,
    place_revision_snapshot: Option<i64>,
    revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeatherDefaultPlace {
    singleton: i64,
    place_id: Option<String>,
    revision: i64,
}

fn unique_files_v3<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, FileV3>, D::Error> {
    struct Files;
    impl<'de> serde::de::Visitor<'de> for Files {
        type Value = BTreeMap<String, FileV3>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("five unique export payloads")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut files = BTreeMap::new();
            while let Some((name, record)) = map.next_entry::<String, FileV3>()? {
                if !NAMES_V5[1..].contains(&name.as_str()) || files.insert(name, record).is_some() {
                    return Err(serde::de::Error::custom("unexpected or duplicate payload"));
                }
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

pub(crate) fn is_v3(archive: &mut File) -> Result<bool> {
    is_version(archive, 3, &NAMES_V3)
}
pub(crate) fn is_v4(archive: &mut File) -> Result<bool> {
    is_version(archive, 4, &NAMES_V4)
}
pub(crate) fn is_v5(archive: &mut File) -> Result<bool> {
    is_version(archive, 5, &NAMES_V5)
}
fn is_version(archive: &mut File, version: u32, names: &[&str]) -> Result<bool> {
    archive.seek(SeekFrom::Start(0))?;
    // Bound ZIP directory indexing before zip::ZipArchive builds its name map.
    let mut raw_buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
    let raw = rawzip::ZipArchive::from_seekable(&mut *archive, &mut raw_buffer)
        .map_err(|_| Error::Archive)?;
    if raw.entries_hint() != names.len() as u64 {
        archive.seek(SeekFrom::Start(0))?;
        return Ok(false);
    }
    let mut entries = raw.entries(&mut raw_buffer);
    let mut seen = 0;
    while entries.next_entry().map_err(|_| Error::Archive)?.is_some() {
        seen += 1;
        if seen > names.len() {
            return Err(Error::Archive);
        }
    }
    if seen != names.len() {
        return Err(Error::Archive);
    }
    archive.seek(SeekFrom::Start(0))?;
    let result = (|| {
        let mut zip = zip::ZipArchive::new(&mut *archive).map_err(|_| Error::Archive)?;
        let mut manifest = zip.by_name("manifest.json").map_err(|_| Error::Archive)?;
        if manifest.size() > MANIFEST_LIMIT {
            return Err(Error::ResourceLimit);
        }
        let mut bytes = Vec::new();
        (&mut manifest)
            .take(MANIFEST_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        let h: ManifestV3 = serde_json::from_slice(&bytes).map_err(|_| Error::Manifest)?;
        Ok(h.format == "mno-export" && h.format_version == version)
    })();
    archive.seek(SeekFrom::Start(0))?;
    result
}

pub(crate) fn build_v3(archive: &mut File, parent: &Path, flag: &AtomicBool) -> Result<Candidate> {
    build_version(archive, parent, flag, 3, &NAMES_V3)
}
pub(crate) fn build_v4(archive: &mut File, parent: &Path, flag: &AtomicBool) -> Result<Candidate> {
    build_version(archive, parent, flag, 4, &NAMES_V4)
}
pub(crate) fn build_v5(archive: &mut File, parent: &Path, flag: &AtomicBool) -> Result<Candidate> {
    build_version(archive, parent, flag, 5, &NAMES_V5)
}
fn build_version(
    archive: &mut File,
    parent: &Path,
    flag: &AtomicBool,
    version: u32,
    expected_names: &[&str],
) -> Result<Candidate> {
    cancel(flag)?;
    if !archive.metadata()?.is_file() {
        return Err(Error::Archive);
    }
    archive.seek(SeekFrom::Start(0))?;
    let mut raw_buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
    let raw = rawzip::ZipArchive::from_seekable(&mut *archive, &mut raw_buffer)
        .map_err(|_| Error::Archive)?;
    if raw.entries_hint() != expected_names.len() as u64 {
        return Err(Error::Archive);
    }
    let mut entries = raw.entries(&mut raw_buffer);
    let mut found = BTreeSet::new();
    while let Some(e) = entries.next_entry().map_err(|_| Error::Archive)? {
        let name = e.file_path().as_bytes();
        if !expected_names.iter().any(|n| n.as_bytes() == name) || !found.insert(name.to_vec()) {
            return Err(Error::Archive);
        }
    }
    if found.len() != expected_names.len() {
        return Err(Error::Archive);
    }
    archive.seek(SeekFrom::Start(0))?;
    let mut zip = zip::ZipArchive::new(&mut *archive).map_err(|_| Error::Archive)?;
    if zip.len() != expected_names.len() {
        return Err(Error::Archive);
    }
    for i in 0..expected_names.len() {
        let e = zip.by_index(i).map_err(|_| Error::Archive)?;
        if !expected_names.iter().any(|n| n.as_bytes() == e.name_raw())
            || e.encrypted()
            || e.compression() != zip::CompressionMethod::Stored
            || e.compressed_size() != e.size()
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
        serde_json::from_slice::<ManifestV3>(&bytes).map_err(|_| Error::Manifest)?
    };
    if m.format != "mno-export"
        || m.format_version != version
        || m.database_schema_version != version - 1
        || m.document_version != if version == 5 { 2 } else { 1 }
        || m.external_originals_included
        || m.files.len() != expected_names.len() - 1
        || m.files
            .keys()
            .any(|n| !expected_names[1..].contains(&n.as_str()))
    {
        return Err(Error::Manifest);
    }
    for name in ["journal.jsonl", "places.jsonl", "journal_places.jsonl"] {
        if m.files[name].row_count.is_none() {
            return Err(Error::Manifest);
        }
    }
    if version >= 4 && m.files["weather_default_place.jsonl"].row_count.is_none() {
        return Err(Error::Manifest);
    }
    if m.files["journal.jsonl"].row_count != Some(m.entry_count) {
        return Err(Error::Manifest);
    }
    let directory = tempfile::Builder::new()
        .prefix(".mno-import-v3-")
        .tempdir_in(parent)?;
    for name in expected_names[1..].iter() {
        cancel(flag)?;
        let expected = &m.files[*name];
        if [
            "places.jsonl",
            "journal_places.jsonl",
            "weather_default_place.jsonl",
        ]
        .contains(name)
            && expected.size > MAX_PLACE_PAYLOAD
        {
            return Err(Error::ResourceLimit);
        }
        let mut entry = zip.by_name(name).map_err(|_| Error::Archive)?;
        if entry.size() != expected.size {
            return Err(Error::Checksum);
        }
        let mut output = File::create(directory.path().join(name))?;
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut block = [0u8; 65536];
        loop {
            cancel(flag)?;
            let n = entry.read(&mut block)?;
            if n == 0 {
                break;
            }
            size = size.checked_add(n as u64).ok_or(Error::ResourceLimit)?;
            if size > expected.size {
                return Err(Error::Checksum);
            }
            hash.update(&block[..n]);
            output.write_all(&block[..n])?;
        }
        if size != expected.size || format!("{:x}", hash.finalize()) != expected.sha256 {
            return Err(Error::Checksum);
        }
    }
    drop(zip);
    let path = directory.path().join("candidate.sqlite");
    crate::database::create_legacy_database(&path).map_err(|_| Error::Verification)?;
    let mut db = Database(Some(Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?));
    if version >= 3 {
        crate::database::weather_schema::migrate(db.get()).map_err(|_| Error::Verification)?;
        crate::database::place_schema::migrate_candidate(db.get())
            .map_err(|_| Error::Verification)?;
    }
    if version >= 4 {
        let tx = db.get().transaction()?;
        crate::database::weather_default_schema::create_in_transaction(&tx)
            .map_err(|_| Error::Verification)?;
        if version == 5 {
            crate::photographs::create_schema(&tx).map_err(|_| Error::Verification)?;
        }
        tx.commit()?;
    }
    let tx = db.get().transaction()?;
    let mut count = 0u64;
    let journal = File::open(directory.path().join("journal.jsonl"))?;
    let mut input = BufReader::new(journal);
    let mut previous: Option<String> = None;
    while let Some((r, weather)) = next_record_version(&mut input, flag, version)? {
        if previous.as_ref().is_some_and(|v| v >= &r.id) {
            return Err(Error::Record);
        }
        tx.execute("INSERT INTO journal_entries(id,entry_date,title,content_json,document_version,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![r.id,r.entry_date,r.title,r.content_json,r.document_version,r.created_at,r.updated_at])?;
        if let Some(w) = weather {
            validate_weather(&r.id, &w)?;
            tx.execute(
                "INSERT INTO journal_weather VALUES(?1,?2,?3,?4,?5)",
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
    }
    if count != m.entry_count {
        return Err(Error::Manifest);
    }
    let mut previous = None;
    let place_count = for_each_line::<Place>(
        directory.path().join("places.jsonl"),
        PLACE_LINE_LIMIT,
        flag,
        |p| {
            if previous.as_ref().is_some_and(|id: &String| id >= &p.id) {
                return Err(Error::Record);
            }
            previous = Some(p.id.clone());
            tx.execute(
                "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,?2,?3,?4,?5)",
                params![p.id, p.name, p.latitude, p.longitude, p.revision],
            )?;
            Ok(())
        },
    )?;
    if place_count != m.files["places.jsonl"].row_count.unwrap() {
        return Err(Error::Manifest);
    }
    let mut previous = None;
    let link_count = for_each_line::<Link>(
        directory.path().join("journal_places.jsonl"),
        PLACE_LINE_LIMIT,
        flag,
        |l| {
            if previous
                .as_ref()
                .is_some_and(|id: &String| id >= &l.entry_id)
            {
                return Err(Error::Record);
            }
            previous = Some(l.entry_id.clone());
            tx.execute("INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![l.entry_id,l.place_id,l.place_name_snapshot,l.latitude_snapshot,l.longitude_snapshot,l.place_revision_snapshot,l.revision])?;
            Ok(())
        },
    )?;
    if link_count != m.files["journal_places.jsonl"].row_count.unwrap() {
        return Err(Error::Manifest);
    }
    if version >= 4 {
        let setting_count = for_each_line::<WeatherDefaultPlace>(
            directory.path().join("weather_default_place.jsonl"),
            PLACE_LINE_LIMIT,
            flag,
            |setting| {
                tx.execute(
                    "INSERT INTO weather_default_place(singleton,place_id,revision) VALUES(?1,?2,?3)",
                    params![setting.singleton, setting.place_id, setting.revision],
                )?;
                Ok(())
            },
        )?;
        if setting_count > 1
            || setting_count != m.files["weather_default_place.jsonl"].row_count.unwrap()
        {
            return Err(Error::Manifest);
        }
    }
    if version == 5 {
        let mut previous: Option<String> = None;
        let count = for_each_line::<crate::photographs::Asset>(
            directory.path().join("images.jsonl"),
            6 * 1024 * 1024,
            flag,
            |asset| {
                if previous.as_ref().is_some_and(|id| id >= &asset.asset_id) {
                    return Err(Error::Record);
                }
                previous = Some(asset.asset_id.clone());
                asset.insert(&tx).map_err(|_| Error::Record)
            },
        )?;
        if m.files["images.jsonl"].row_count != Some(count) {
            return Err(Error::Manifest);
        }
    }
    tx.commit()?;
    drop(db);
    let mut db = Database(Some(Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?));
    let proof = verify_database_version(db.get(), flag, version)?;
    if proof.entry_count != count {
        return Err(Error::Verification);
    }
    drop(db);
    cancel(flag)?;
    Ok(Candidate {
        path,
        _directory: directory,
        entry_count: count,
        proof,
    })
}
fn for_each_line<T: for<'de> Deserialize<'de>>(
    path: PathBuf,
    limit: usize,
    flag: &AtomicBool,
    mut visit: impl FnMut(T) -> Result<()>,
) -> Result<u64> {
    let mut input = BufReader::new(File::open(path)?);
    let mut count = 0u64;
    loop {
        cancel(flag)?;
        let mut line = Vec::new();
        input
            .by_ref()
            .take(limit as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if line.is_empty() {
            break;
        }
        if line.len() > limit || line.last() != Some(&b'\n') {
            return Err(Error::Record);
        }
        visit(serde_json::from_slice(&line).map_err(|_| Error::Record)?)?;
        count = count.checked_add(1).ok_or(Error::ResourceLimit)?;
    }
    Ok(count)
}
