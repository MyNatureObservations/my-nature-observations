//! Verify and admit selected Windows journals under the startup lease.
use super::selection_publication::{Step, publish_record, read_record};
use super::*;
use crate::startup::{
    DirectoryLease,
    generation_selection::{LIMIT, Phase, Record},
    windows_file_information,
};
use sha2::{Digest, Sha256};
use std::io::Seek;

pub(super) const HEAD: &str = "generation-selection.json";
pub(super) const STAGED: &str = ".generation-selection-staged";
pub(super) const INTENT: &str = ".generation-selection-in-progress";
const GENERATIONS: &str = "generations";
const DATABASE: &str = "journal.sqlite";

#[derive(Debug)]
pub(in crate::startup) enum Block {
    Invalid,
    Interrupted,
    Publication,
    Verification,
}

impl Block {
    pub(in crate::startup) fn code(self) -> &'static str {
        match self {
            Self::Invalid => "database_managed_layout_invalid",
            Self::Interrupted => "database_managed_publication_incomplete",
            Self::Publication => "database_managed_admission_publication_failed",
            Self::Verification => "database_managed_verification_failed",
        }
    }
}

fn canonical_entry(root: &Path, name: &str) -> io::Result<bool> {
    let mut found = false;
    for entry in fs::read_dir(root)? {
        let entry = entry?.file_name();
        if entry
            .as_encoded_bytes()
            .eq_ignore_ascii_case(name.as_bytes())
        {
            if entry != name || found {
                return Err(invalid());
            }
            found = true;
        }
    }
    Ok(found)
}

fn open_single_file(path: &Path, sharing: u32, writable: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || !plain(&metadata)
        || windows_file_information(&file)?.nNumberOfLinks != 1
    {
        return Err(invalid());
    }
    Ok(file)
}

pub(in crate::startup) fn pin_database(path: &Path, directory: &File) -> Result<File, String> {
    let pin = open_single_file(path, 3, false).map_err(failure)?;
    if identity(&pin).map_err(failure)?.0 != identity(directory).map_err(failure)?.0 {
        return Err("database_startup_unsafe_database".into());
    }
    Ok(pin)
}

pub(in crate::startup) fn check_database_pin(path: &Path, pin: &File) -> Result<(), String> {
    let current = open_single_file(path, 3, false).map_err(failure)?;
    if identity(&current).map_err(failure)? != identity(pin).map_err(failure)? {
        return Err("database_startup_unsafe_database".into());
    }
    Ok(())
}

pub(in crate::startup) struct Admitted {
    pub(in crate::startup) path: PathBuf,
    pub(in crate::startup) database: File,
    pub(in crate::startup) directories: [File; 2],
}

pub(in crate::startup) fn resolve(lease: &DirectoryLease) -> Result<Option<Admitted>, Block> {
    inspect(lease).map_err(|_| Block::Invalid)?
}

fn inspect(lease: &DirectoryLease) -> io::Result<Result<Option<Admitted>, Block>> {
    lease.boundary.check()?;
    let root = &lease.root;
    if canonical_entry(root, INTENT)? || canonical_entry(root, STAGED)? {
        return Ok(Err(Block::Interrupted));
    }
    let head = canonical_entry(root, HEAD)?;
    let generations = canonical_entry(root, GENERATIONS)?;
    if !head && !generations {
        return Ok(Ok(None));
    }
    if !head
        || !generations
        || crate::startup::checkpoint(root, |path| fs::symlink_metadata(path))
            .map_err(io::Error::other)?
        || !crate::startup::established::read(root).map_err(io::Error::other)?
    {
        return Ok(Err(Block::Invalid));
    }

    let mut bytes = Vec::new();
    let record_pin = open_single_file(&root.join(HEAD), 1, false)?;
    (&record_pin)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    let record = Record::decode(&bytes)?;
    let generations = root.join(GENERATIONS);
    let parent = directory_handle(&generations)?;
    require_ntfs(&generations, &parent)?;
    if !canonical_entry(&generations, &record.generation)? {
        return Ok(Err(Block::Invalid));
    }
    let selected = generations.join(&record.generation);
    let directory = directory_handle(&selected)?;
    require_ntfs(&selected, &directory)?;
    if !canonical_entry(&selected, DATABASE)? {
        return Ok(Err(Block::Invalid));
    }
    let path = selected.join(DATABASE);
    let pending = record.phase == Phase::Pending;
    let database = open_single_file(&path, 1, pending)?;
    let volume = identity(&lease.directory)?.0;
    if [&parent, &directory, &database]
        .iter()
        .any(|file| identity(file).map_or(true, |id| id.0 != volume))
        || crate::startup::sidecars::check(&path, &mut |p| fs::symlink_metadata(p)).is_err()
    {
        return Ok(Err(Block::Invalid));
    }
    if pending {
        admission_point(AdmissionPoint::BeforeVerification)?;
        let mut digest = Sha256::new();
        let mut reader = &database;
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let count = reader.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            digest.update(&chunk[..count]);
            admission_point(AdmissionPoint::Digest)?;
        }
        if format!("{:x}", digest.finalize()) != record.candidate_sha256 {
            return Ok(Err(Block::Verification));
        }
        reader.rewind()?;
        admission_point(AdmissionPoint::SqliteVerification)?;
    }
    let mut header = [0u8; 20];
    (&database).read_exact(&mut header)?;
    if &header[..16] != b"SQLite format 3\0" || header[18..20] != [1, 1] {
        return Ok(Err(Block::Verification));
    }
    if confirm(root, &lease.boundary).is_err()
        || crate::database::initialize_database(&path).is_err()
    {
        return Ok(Err(Block::Verification));
    }
    let bridge = open_single_file(&path, 3, false)?;
    if identity(&bridge)? != identity(&database)?
        || crate::startup::sidecars::check(&path, &mut |p| fs::symlink_metadata(p)).is_err()
    {
        return Ok(Err(Block::Invalid));
    }
    lease.ensure_current().map_err(io::Error::other)?;
    lease.boundary.check()?;
    if pending {
        // The pending pin grants flush access but denies competing writers.
        // Establish the candidate and its directory entries before selecting it.
        flush(&database)?;
        directory.sync_all()?;
        parent.sync_all()?;
        admission_point(AdmissionPoint::BeforePublication)?;
        // MoveFileExW needs delete sharing on the old head. Keep the lease
        // and candidate pins, and recheck the head after releasing its pin.
        drop(record_pin);
        if read_record(&root.join(HEAD))? != bytes {
            return Ok(Err(Block::Invalid));
        }
        let mut admitted = record;
        admitted.phase = Phase::Admitted;
        if publish_record(lease, &admitted, true, |step| match step {
            Step::Replaced => admission_point(AdmissionPoint::Replaced),
            Step::Flushed => admission_point(AdmissionPoint::BeforeAcknowledgement),
            _ => Ok(()),
        })
        .is_err()
        {
            return Ok(Err(Block::Publication));
        }
        admission_point(AdmissionPoint::Acknowledged)?;
    }
    drop(database);
    // Confirm a usable mutable open before exposing Ready. The bridge holds
    // identity across both opens and does not request any SQLite byte locks.
    let writable = match crate::database::open_existing(&path) {
        Ok(connection) => connection,
        Err(_) => return Ok(Err(Block::Verification)),
    };
    let opening = admission_point(AdmissionPoint::WritableOpen);
    let readonly = writable.is_readonly(rusqlite::MAIN_DB);
    if writable.close().is_err() || !matches!(readonly, Ok(false)) {
        return Ok(Err(Block::Verification));
    }
    opening?;
    check_database_pin(&path, &bridge).map_err(io::Error::other)?;
    if crate::startup::sidecars::check(&path, &mut |p| fs::symlink_metadata(p)).is_err() {
        return Ok(Err(Block::Invalid));
    }
    Ok(Ok(Some(Admitted {
        path,
        database: bridge,
        directories: [directory, parent],
    })))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::startup) enum AdmissionPoint {
    BeforeVerification,
    Digest,
    SqliteVerification,
    BeforePublication,
    Replaced,
    BeforeAcknowledgement,
    Acknowledged,
    WritableOpen,
}

#[cfg(test)]
thread_local! {
    static ADMISSION_FAULT: std::cell::Cell<Option<AdmissionPoint>> = const { std::cell::Cell::new(None) };
}

fn admission_point(point: AdmissionPoint) -> io::Result<()> {
    #[cfg(test)]
    if ADMISSION_FAULT.get() == Some(point) {
        return Err(io::Error::other("injected admission interruption"));
    }
    let _ = point;
    Ok(())
}

#[cfg(test)]
pub(in crate::startup) fn with_admission_fault<T>(
    point: AdmissionPoint,
    action: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<AdmissionPoint>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ADMISSION_FAULT.set(self.0);
        }
    }
    let _reset = Reset(ADMISSION_FAULT.replace(Some(point)));
    action()
}
