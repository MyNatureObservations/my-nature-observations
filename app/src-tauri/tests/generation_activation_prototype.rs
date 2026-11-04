//! C2 exploratory protocol, NOT production startup or a Ready capability.
//! Cooperative processes on a private local test root; no hostile directory replacement.
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn reject<T>() -> Result<T> {
    Err("ambiguous or invalid prototype state".into())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn plain(path: &Path, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return reject();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return reject();
        } // reparse point
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    format: String,
    version: u32,
    sequence: u32,
    generation: String,
    sha256: String,
    previous: Option<String>,
}
struct Lease {
    root: PathBuf,
    _file: File,
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Logical ownership ends here, not when the last descriptor inherited
        // by an unrelated fork/exec child eventually closes. Never delegate it.
        if let Err(error) = FileExt::unlock(&self._file) {
            eprintln!("prototype lease unlock failed: {error}");
            std::process::abort();
        }
    }
}
impl Lease {
    fn open(root: &Path) -> Result<Self> {
        plain(root, true)?;
        plain(&root.join("lease"), false)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("lease"))?;
        file.try_lock_exclusive()?;
        // Establish the RAII owner immediately: validation errors after locking
        // must follow the same explicit-unlock path as successful operations.
        let lease = Self {
            root: root.into(),
            _file: file,
        };
        plain(&root.join("generations"), true)?;
        plain(&root.join("selections"), true)?;
        Ok(lease)
    }
    fn candidate(&self, id: &str) -> Result<PathBuf> {
        if uuid::Uuid::parse_str(id)?.to_string() != id {
            return reject();
        }
        let dir = self.root.join("generations").join(id);
        plain(&dir, true)?;
        // Synthetic standalone DB only: never replay or remove unaccounted sidecars.
        let entries: Vec<_> = fs::read_dir(&dir)?.collect::<std::result::Result<_, _>>()?;
        if entries.len() != 1 || entries[0].file_name() != "journal.sqlite" {
            return reject();
        }
        let path = dir.join("journal.sqlite");
        plain(&path, false)?;
        Ok(path)
    }
    fn verify(&self, id: &str, digest: &str) -> Result<PathBuf> {
        let path = self.candidate(id)?;
        let bytes = fs::read(&path)?;
        if bytes.len() < 20
            || &bytes[..16] != b"SQLite format 3\0"
            || bytes[18..20] != [1, 1]
            || hash(&bytes) != digest
        {
            return reject();
        }
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let result = db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0));
        db.close().map_err(|(_, e)| e)?;
        if result? != "ok" || hash(&fs::read(&path)?) != digest {
            return reject();
        }
        self.candidate(id)?;
        Ok(path)
    }
    fn records(&self) -> Result<Vec<(Selection, Vec<u8>)>> {
        let mut paths = fs::read_dir(self.root.join("selections"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        paths.sort();
        let mut records = Vec::new();
        let mut previous = None;
        for (index, path) in paths.into_iter().enumerate() {
            if index >= 100 {
                return reject();
            } // bounded prototype history; never prune
            let sequence = index as u32 + 1;
            if path.file_name().unwrap() != format!("select-{sequence:08}.json").as_str() {
                return reject();
            }
            plain(&path, false)?;
            if fs::metadata(&path)?.len() > 4096 {
                return reject();
            }
            let bytes = fs::read(&path)?;
            let record: Selection = serde_json::from_slice(&bytes)?;
            if record.format != "mno-selection-prototype"
                || record.version != 1
                || record.sequence != sequence
                || record.previous != previous
                || record.sha256.len() != 64
                || !record.sha256.bytes().all(|c| c.is_ascii_hexdigit())
                || uuid::Uuid::parse_str(&record.generation)?.to_string() != record.generation
            {
                return reject();
            }
            previous = Some(hash(&bytes));
            records.push((record, bytes));
        }
        Ok(records)
    }
    // Empty history is allowed ONLY for an explicit first selection, never startup.
    fn select(
        &self,
        id: &str,
        digest: &str,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let records = self.records()?;
        if records.len() >= 100 {
            return reject();
        }
        let path = self.verify(id, digest)?;
        barrier("candidate-file", &path)?;
        barrier("generation-directory", path.parent().unwrap())?;
        barrier("generations-directory", &self.root.join("generations"))?;
        let record = Selection {
            format: "mno-selection-prototype".into(),
            version: 1,
            sequence: records.len() as u32 + 1,
            generation: id.into(),
            sha256: digest.into(),
            previous: records.last().map(|(_, b)| hash(b)),
        };
        let bytes = serde_json::to_vec(&record)?;
        let dir = self.root.join("selections");
        let mut temporary = tempfile::Builder::new()
            .prefix("pending-")
            .tempfile_in(&dir)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        barrier("record-file", temporary.path())?;
        hook("before-publication")?;
        // Append only. No replacement of an old selection or database.
        let published = temporary
            .persist_noclobber(dir.join(format!("select-{:08}.json", record.sequence)))
            .map_err(|e| e.error)?;
        hook("after-publication")?;
        barrier("selection-directory", &dir)?;
        barrier("root-directory", &self.root)?;
        drop(published);
        Ok(())
    }
    fn startup(&self, barrier: &mut impl FnMut(&str, &Path) -> Result<()>) -> Result<PathBuf> {
        let records = self.records()?;
        let (record, _) = records
            .last()
            .ok_or("missing selection; never create empty journal")?;
        let path = self.verify(&record.generation, &record.sha256)?;
        // Reconfirm barriers after an unacknowledged publication. No disk mutation.
        barrier("candidate-file", &path)?;
        barrier("generation-directory", path.parent().unwrap())?;
        barrier("generations-directory", &self.root.join("generations"))?;
        for (r, _) in &records {
            barrier(
                "record-file",
                &self
                    .root
                    .join("selections")
                    .join(format!("select-{:08}.json", r.sequence)),
            )?;
        }
        barrier("selection-directory", &self.root.join("selections"))?;
        barrier("root-directory", &self.root)?;
        Ok(path)
    }
}
// Real file/directory barriers. Windows directory durability is deliberately unsupported.
fn real_barrier(_point: &str, path: &Path) -> Result<()> {
    #[cfg(windows)]
    if path.is_dir() {
        return Err(io::Error::from(io::ErrorKind::Unsupported).into());
    }
    let file = if cfg!(windows) && path.is_file() {
        // FlushFileBuffers needs a write-capable handle; synthetic files only.
        OpenOptions::new().read(true).write(true).open(path)?
    } else {
        File::open(path)?
    };
    #[cfg(target_os = "macos")]
    if path.is_file() {
        use std::os::fd::AsRawFd;
        loop {
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e.into());
            }
        }
    }
    file.sync_all()?;
    Ok(())
}
// Explicit protocol-only model: NOT evidence of filesystem/power-loss durability.
fn modeled_barrier(_: &str, _: &Path) -> Result<()> {
    Ok(())
}
fn no_hook(_: &str) -> Result<()> {
    Ok(())
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("generations")).unwrap();
    fs::create_dir(dir.path().join("selections")).unwrap();
    File::create(dir.path().join("lease")).unwrap();
    dir
}
fn generation(root: &Path) -> (String, String, PathBuf) {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = root.join("generations").join(&id);
    fs::create_dir(&dir).unwrap();
    let path = dir.join("journal.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE synthetic(value); INSERT INTO synthetic VALUES('C2');")
        .unwrap();
    db.close().unwrap();
    (id, hash(&fs::read(&path).unwrap()), path)
}
fn inventory(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, lease_path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        for e in fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                collect(&p, lease_path, result);
            } else if p == lease_path {
                // The fixture's lease has no payload. Windows denies reads through
                // another handle while LockFileEx holds its exclusive byte range.
                // Assert exact empty content through metadata; never unlock, omit
                // the file, or turn a failed read of any other file into success.
                plain(&p, false).unwrap();
                assert_eq!(fs::metadata(&p).unwrap().len(), 0, "nonempty fixture lease");
                result.push((p, Vec::new()));
            } else {
                let bytes = fs::read(&p).unwrap();
                result.push((p, bytes));
            }
        }
    }
    let mut result = Vec::new();
    collect(root, &root.join("lease"), &mut result);
    result.sort();
    result
}

#[test]
fn initial_selection_restart_retention_and_damaged_previous_generation() {
    let root = fixture();
    let (old, old_hash, old_path) = generation(root.path());
    let (new, new_hash, new_path) = generation(root.path());
    {
        let lease = Lease::open(root.path()).unwrap();
        assert!(lease.startup(&mut modeled_barrier).is_err());
        lease
            .select(&old, &old_hash, &mut modeled_barrier, &mut no_hook)
            .unwrap();
    }
    assert_eq!(
        Lease::open(root.path())
            .unwrap()
            .startup(&mut modeled_barrier)
            .unwrap(),
        old_path
    );
    fs::write(&old_path, b"synthetic damaged previous database").unwrap();
    fs::write(
        old_path.with_extension("sqlite-wal"),
        b"preserved synthetic WAL",
    )
    .unwrap();
    let old_before = inventory(old_path.parent().unwrap());
    {
        let lease = Lease::open(root.path()).unwrap();
        lease
            .select(&new, &new_hash, &mut modeled_barrier, &mut no_hook)
            .unwrap();
    }
    let all = inventory(root.path());
    for _ in 0..3 {
        assert_eq!(
            Lease::open(root.path())
                .unwrap()
                .startup(&mut modeled_barrier)
                .unwrap(),
            new_path
        );
    }
    assert_eq!(inventory(root.path()), all);
    assert_eq!(inventory(old_path.parent().unwrap()), old_before);
}
#[test]
fn invalid_missing_candidates_and_ambiguous_records_fail_without_changes() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    for (id, digest) in [
        (uuid::Uuid::new_v4().to_string(), digest.clone()),
        (id.clone(), "0".repeat(64)),
        ("../escape".into(), digest.clone()),
    ] {
        let before = inventory(root.path());
        assert!(
            lease
                .select(&id, &digest, &mut modeled_barrier, &mut no_hook)
                .is_err()
        );
        assert_eq!(inventory(root.path()), before);
    }
    lease
        .select(&id, &digest, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    let record = root.path().join("selections/select-00000001.json");
    let original = fs::read(&record).unwrap();
    for invalid in [
        b"bad JSON".to_vec(),
        original
            .iter()
            .copied()
            .chain(b" trailing".iter().copied())
            .collect(),
    ] {
        fs::write(&record, invalid).unwrap();
        let before = inventory(root.path());
        assert!(lease.startup(&mut modeled_barrier).is_err());
        assert_eq!(inventory(root.path()), before);
    }
    fs::write(&record, &original).unwrap();
    fs::write(
        root.path().join("selections/select-00000003.json"),
        &original,
    )
    .unwrap();
    assert!(lease.startup(&mut modeled_barrier).is_err());
    fs::remove_file(root.path().join("selections/select-00000003.json")).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(lease.startup(&mut modeled_barrier).is_err());
    assert!(!path.exists());
}
#[test]
fn sync_failures_and_publication_errors_are_not_success() {
    for point in [
        "candidate-file",
        "generation-directory",
        "generations-directory",
        "record-file",
        "selection-directory",
        "root-directory",
    ] {
        let root = fixture();
        let (old, oh, op) = generation(root.path());
        let (new, nh, np) = generation(root.path());
        {
            let lease = Lease::open(root.path()).unwrap();
            lease
                .select(&old, &oh, &mut modeled_barrier, &mut no_hook)
                .unwrap();
            let mut barrier = |p: &str, _: &Path| -> Result<()> {
                if p == point {
                    Err("injected sync failure".into())
                } else {
                    Ok(())
                }
            };
            assert!(lease.select(&new, &nh, &mut barrier, &mut no_hook).is_err());
            assert!(lease.startup(&mut barrier).is_err());
        }
        let expected = if matches!(point, "selection-directory" | "root-directory") {
            np
        } else {
            op
        };
        assert_eq!(
            Lease::open(root.path())
                .unwrap()
                .startup(&mut modeled_barrier)
                .unwrap(),
            expected
        );
    }
}
#[test]
fn real_barriers_are_supported_or_explicitly_rejected() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    let result = lease.select(&id, &digest, &mut real_barrier, &mut no_hook);
    if cfg!(windows) {
        let error = result.unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::Unsupported
        );
        assert!(lease.startup(&mut real_barrier).is_err());
        assert!(
            fs::read_dir(root.path().join("selections"))
                .unwrap()
                .next()
                .is_none()
        );
    } else {
        result.unwrap();
        assert_eq!(lease.startup(&mut real_barrier).unwrap(), path);
    }
}
#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_C2_ROOT") else {
        return;
    };
    let lease = Lease::open(Path::new(&root)).unwrap();
    let id = std::env::var("MNO_C2_ID").unwrap();
    let digest = std::env::var("MNO_C2_HASH").unwrap();
    let stop = std::env::var("MNO_C2_STOP").unwrap();
    lease
        .select(&id, &digest, &mut modeled_barrier, &mut |p| {
            if p == stop {
                std::process::exit(86);
            }
            Ok(())
        })
        .unwrap();
    panic!("did not interrupt");
}
#[test]
fn process_interruption_before_and_after_publication() {
    for point in ["before-publication", "after-publication"] {
        let root = fixture();
        let (old, oh, op) = generation(root.path());
        let (new, nh, np) = generation(root.path());
        {
            Lease::open(root.path())
                .unwrap()
                .select(&old, &oh, &mut modeled_barrier, &mut no_hook)
                .unwrap();
        }
        let before_old = fs::read(op).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "interruption_child", "--nocapture"])
            .env("MNO_C2_ROOT", root.path())
            .env("MNO_C2_ID", new)
            .env("MNO_C2_HASH", nh)
            .env("MNO_C2_STOP", point)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child timed out");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(86));
        let lease = Lease::open(root.path()).unwrap();
        let result = lease.startup(&mut modeled_barrier);
        if point == "before-publication" {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), np);
        }
        assert_eq!(
            fs::read(
                root.path()
                    .join("generations")
                    .join(old)
                    .join("journal.sqlite")
            )
            .unwrap(),
            before_old
        );
    }
}
#[cfg(windows)]
#[test]
fn windows_readers_do_not_require_replacing_old_records_or_databases() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = fixture();
    let (old, oh, op) = generation(root.path());
    let (new, nh, np) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    lease
        .select(&old, &oh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    let _database = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(op)
        .unwrap();
    let _record = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(root.path().join("selections/select-00000001.json"))
        .unwrap();
    lease
        .select(&new, &nh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    assert_eq!(lease.startup(&mut modeled_barrier).unwrap(), np);
}

#[test]
fn lease_exclusion_and_corrupt_chain_fail_closed() {
    let root = fixture();
    let (old, oh, _) = generation(root.path());
    let (new, nh, _) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    assert!(Lease::open(root.path()).is_err());
    lease
        .select(&old, &oh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    lease
        .select(&new, &nh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    let path = root.path().join("selections/select-00000002.json");
    let original = fs::read(&path).unwrap();
    let mut record: Selection = serde_json::from_slice(&original).unwrap();
    record.previous = Some("0".repeat(64));
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let before = inventory(root.path());
    assert!(lease.startup(&mut modeled_barrier).is_err());
    assert_eq!(inventory(root.path()), before);
    fs::write(&path, original).unwrap();
    fs::remove_file(root.path().join("selections/select-00000001.json")).unwrap();
    assert!(lease.startup(&mut modeled_barrier).is_err());
}

#[test]
fn inventory_preserves_exclusive_lease_and_includes_empty_lock_file() {
    let root = fixture();
    generation(root.path());
    let before = inventory(root.path());
    let lease = Lease::open(root.path()).unwrap();
    assert!(Lease::open(root.path()).is_err());
    #[cfg(windows)]
    assert_eq!(
        fs::read(root.path().join("lease"))
            .unwrap_err()
            .raw_os_error(),
        Some(33)
    );
    assert_eq!(inventory(root.path()), before);
    assert!(
        before
            .iter()
            .any(|(p, bytes)| p == &root.path().join("lease") && bytes.is_empty())
    );
    assert!(Lease::open(root.path()).is_err());
    drop(lease);
    assert_eq!(
        fs::read(root.path().join("lease")).unwrap(),
        Vec::<u8>::new()
    );
    assert_eq!(inventory(root.path()), before);
    // Positive control: exclusion ends only when the owner actually closes.
    let _next = Lease::open(root.path()).unwrap();
}

#[path = "generation_activation_prototype/admission.rs"]
mod admission;

#[cfg(unix)]
#[path = "generation_activation_prototype/lease_lifecycle.rs"]
mod lease_lifecycle;
