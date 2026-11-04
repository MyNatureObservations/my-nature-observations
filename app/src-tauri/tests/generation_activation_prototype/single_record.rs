//! C4 comparison only: one head, no history/authority witness and no production API.
use super::*;
const NAME: &str = "single-selection.json";
const TEMP: &str = "single-pending-";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Pending,
    Admitted,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    version: u32,
    generation: String,
    phase: Phase,
    candidate_sha256: String,
}
#[derive(Debug, PartialEq, Eq)]
enum State {
    Pending(PathBuf),
    Admitted(PathBuf),
}
impl Lease {
    fn single_record(&self) -> Result<Record> {
        for entry in fs::read_dir(&self.root)? {
            let name = entry?.file_name();
            if name.to_string_lossy().starts_with(TEMP) {
                return reject();
            }
        }
        let path = self.root.join(NAME);
        plain(&path, false)?;
        if fs::metadata(&path)?.len() > 4096 {
            return reject();
        }
        let record: Record = serde_json::from_slice(&fs::read(path)?)?;
        if record.format != "mno-single-selection-prototype"
            || record.version != 1
            || uuid::Uuid::parse_str(&record.generation)?.to_string() != record.generation
            || record.candidate_sha256.len() != 64
            || !record
                .candidate_sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        {
            return reject();
        }
        Ok(record)
    }
    fn single_publish(
        &self,
        record: &Record,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let mut temporary = tempfile::Builder::new()
            .prefix(TEMP)
            .tempfile_in(&self.root)?;
        temporary.write_all(&serde_json::to_vec(record)?)?;
        temporary.flush()?;
        barrier("single-record-file", temporary.path())?;
        hook("before-record-publication")?;
        let published = temporary
            .persist(self.root.join(NAME))
            .map_err(|e| e.error)?;
        hook("after-record-publication")?;
        barrier("single-record-directory", &self.root)?;
        drop(published);
        Ok(())
    }
    fn single_candidate_barriers(
        &self,
        path: &Path,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
    ) -> Result<()> {
        barrier("candidate-file", path)?;
        barrier("generation-directory", path.parent().unwrap())?;
        barrier("generations-directory", &self.root.join("generations"))
    }
    fn single_select(
        &self,
        id: &str,
        digest: &str,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        // Explicit initial selection is allowed; missing selection never authorizes startup.
        if self.root.join(NAME).try_exists()? {
            if self.single_record()?.phase != Phase::Admitted {
                return reject();
            }
        } else {
            for entry in fs::read_dir(&self.root)? {
                if entry?.file_name().to_string_lossy().starts_with(TEMP) {
                    return reject();
                }
            }
        }
        let path = self.verify(id, digest)?;
        self.single_candidate_barriers(&path, barrier)?;
        self.single_publish(
            &Record {
                format: "mno-single-selection-prototype".into(),
                version: 1,
                generation: id.into(),
                phase: Phase::Pending,
                candidate_sha256: digest.into(),
            },
            barrier,
            hook,
        )
    }
    fn single_startup(&self, barrier: &mut impl FnMut(&str, &Path) -> Result<()>) -> Result<State> {
        let record = self.single_record()?;
        let path = match record.phase {
            Phase::Pending => self.verify(&record.generation, &record.candidate_sha256)?,
            Phase::Admitted => {
                let path = self.candidate(&record.generation)?;
                self.verify(&record.generation, &hash(&fs::read(path)?))?
            }
        };
        self.single_candidate_barriers(&path, barrier)?;
        barrier("single-record-file", &self.root.join(NAME))?;
        barrier("single-record-directory", &self.root)?;
        Ok(match record.phase {
            Phase::Pending => State::Pending(path),
            Phase::Admitted => State::Admitted(path),
        })
    }
    fn single_admit(
        &self,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<PathBuf> {
        let path = match self.single_startup(barrier)? {
            State::Pending(path) => path,
            State::Admitted(_) => return reject(),
        };
        let mut record = self.single_record()?;
        self.verify(&record.generation, &record.candidate_sha256)?;
        record.phase = Phase::Admitted;
        self.single_publish(&record, barrier, hook)?;
        Ok(path) // Logical admission and acknowledgement follow required barriers.
    }
}

fn admit(lease: &Lease, id: &str, digest: &str) {
    lease
        .single_select(id, digest, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    lease
        .single_admit(&mut modeled_barrier, &mut no_hook)
        .unwrap();
}
#[test]
fn pending_admission_edit_checked_close_and_repeated_startup() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    {
        let lease = Lease::open(root.path()).unwrap();
        let before = inventory(root.path());
        assert!(lease.single_startup(&mut modeled_barrier).is_err());
        assert_eq!(inventory(root.path()), before);
        lease
            .single_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        assert_eq!(
            lease.single_startup(&mut modeled_barrier).unwrap(),
            State::Pending(path.clone())
        );
        lease
            .single_admit(&mut modeled_barrier, &mut no_hook)
            .unwrap();
    }
    let record = fs::read(root.path().join(NAME)).unwrap();
    {
        let lease = Lease::open(root.path()).unwrap();
        assert_eq!(
            lease.single_startup(&mut modeled_barrier).unwrap(),
            State::Admitted(path.clone())
        );
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        db.execute("UPDATE synthetic SET value='legitimate C4 edit'", [])
            .unwrap();
        db.close().unwrap();
    }
    assert_ne!(hash(&fs::read(&path).unwrap()), digest);
    let after_edit = inventory(root.path());
    for _ in 0..3 {
        let lease = Lease::open(root.path()).unwrap();
        assert_eq!(
            lease.single_startup(&mut modeled_barrier).unwrap(),
            State::Admitted(path.clone())
        );
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM synthetic", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "legitimate C4 edit"
        );
        db.close().unwrap();
    }
    assert_eq!(fs::read(root.path().join(NAME)).unwrap(), record);
    assert_eq!(inventory(root.path()), after_edit);
    assert_eq!(
        fs::read_dir(root.path().join("selections"))
            .unwrap()
            .count(),
        0
    );
    assert!(!root.path().join("authority.json").exists());
}
#[test]
fn changing_pending_candidate_does_not_bypass_original_digest() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    lease
        .single_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute("UPDATE synthetic SET value='unadmitted edit'", [])
        .unwrap();
    db.close().unwrap();
    let before = inventory(root.path());
    assert!(
        lease
            .single_admit(&mut modeled_barrier, &mut no_hook)
            .is_err()
    );
    assert!(lease.single_startup(&mut modeled_barrier).is_err());
    assert_eq!(inventory(root.path()), before);
}
#[test]
fn missing_truncated_corrupt_and_ambiguous_records_fail_closed() {
    for fault in [
        "missing",
        "truncated",
        "json",
        "version",
        "phase",
        "duplicate",
        "pending",
    ] {
        let root = fixture();
        let (id, digest, _) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        admit(&lease, &id, &digest);
        let path = root.path().join(NAME);
        let bytes = fs::read(&path).unwrap();
        match fault {
            "missing" => fs::remove_file(path).unwrap(),
            "truncated" => fs::write(path, &bytes[..bytes.len() / 2]).unwrap(),
            "json" => fs::write(path, b"invalid").unwrap(),
            "duplicate" => {
                let text = String::from_utf8(bytes).unwrap();
                fs::write(path, text.replacen('{', "{\"version\":1,", 1)).unwrap();
            }
            "pending" => {
                fs::write(root.path().join(format!("{TEMP}interrupted")), b"partial").unwrap()
            }
            key => {
                let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                value[key] = if key == "version" {
                    99.into()
                } else {
                    "unknown".into()
                };
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        }
        let before = inventory(root.path());
        for _ in 0..3 {
            assert!(lease.single_startup(&mut modeled_barrier).is_err());
        }
        assert_eq!(inventory(root.path()), before);
    }
}
#[test]
fn missing_invalid_selected_generation_and_sidecars_do_not_create_or_delete_files() {
    for fault in ["missing", "corrupt", "sidecar"] {
        let root = fixture();
        let (id, digest, path) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        admit(&lease, &id, &digest);
        match fault {
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => fs::write(&path, b"damaged").unwrap(),
            _ => fs::write(path.with_extension("sqlite-wal"), b"unaccounted WAL").unwrap(),
        }
        let before = inventory(root.path());
        assert!(lease.single_startup(&mut modeled_barrier).is_err());
        assert_eq!(inventory(root.path()), before);
    }
}
#[test]
fn valid_older_record_replay_is_accepted_and_selects_older_generation() {
    let root = fixture();
    let (old, oh, op) = generation(root.path());
    let (new, nh, np) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    admit(&lease, &old, &oh);
    let older_record = fs::read(root.path().join(NAME)).unwrap();
    admit(&lease, &new, &nh);
    assert_eq!(
        lease.single_startup(&mut modeled_barrier).unwrap(),
        State::Admitted(np)
    );
    let generations = inventory(&root.path().join("generations"));
    fs::write(root.path().join(NAME), older_record).unwrap();
    assert_eq!(
        lease.single_startup(&mut modeled_barrier).unwrap(),
        State::Admitted(op)
    );
    assert_eq!(inventory(&root.path().join("generations")), generations); // Accepted limitation; no witness added.
}
#[test]
fn restoration_preserves_damaged_previous_generation_sidecars_and_backup_bytes() {
    let root = fixture();
    let (old, oh, op) = generation(root.path());
    let (new, nh, np) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    admit(&lease, &old, &oh);
    fs::write(&op, b"damaged old DB").unwrap();
    for suffix in ["sqlite-wal", "sqlite-shm", "sqlite-journal"] {
        fs::write(op.with_extension(suffix), b"preserved sidecar").unwrap();
    }
    let old_before = inventory(op.parent().unwrap());
    // C1 tests real .mno-backup validation. C4 never receives or opens a backup.
    let backup = root.path().join("synthetic-backup.bin");
    fs::write(&backup, b"C4 untouched synthetic archive sentinel").unwrap();
    let backup_before = fs::read(&backup).unwrap();
    admit(&lease, &new, &nh);
    assert_eq!(
        lease.single_startup(&mut modeled_barrier).unwrap(),
        State::Admitted(np)
    );
    assert_eq!(inventory(op.parent().unwrap()), old_before);
    assert_eq!(fs::read(backup).unwrap(), backup_before);
}
#[test]
fn publication_permission_and_sync_failures_preserve_data_and_report_failure() {
    for point in [
        "candidate-file",
        "generation-directory",
        "generations-directory",
        "single-record-file",
        "single-record-directory",
        "permission",
    ] {
        let root = fixture();
        let (old, oh, op) = generation(root.path());
        let (new, nh, np) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        admit(&lease, &old, &oh);
        let generations = inventory(&root.path().join("generations"));
        let original_record = fs::read(root.path().join(NAME)).unwrap();
        let mut barrier = |p: &str, _: &Path| -> Result<()> {
            if p == point {
                Err("injected sync failure".into())
            } else {
                Ok(())
            }
        };
        let mut hook = |p: &str| -> Result<()> {
            if point == "permission" && p == "before-record-publication" {
                Err(io::Error::from(io::ErrorKind::PermissionDenied).into())
            } else {
                Ok(())
            }
        };
        assert!(
            lease
                .single_select(&new, &nh, &mut barrier, &mut hook)
                .is_err()
        );
        assert_eq!(inventory(&root.path().join("generations")), generations);
        if point == "single-record-directory" {
            assert_eq!(
                lease.single_startup(&mut modeled_barrier).unwrap(),
                State::Pending(np)
            );
            assert!(lease.single_startup(&mut barrier).is_err());
        } else {
            assert_eq!(fs::read(root.path().join(NAME)).unwrap(), original_record);
            assert_eq!(
                lease.single_startup(&mut modeled_barrier).unwrap(),
                State::Admitted(op)
            );
        }
    }
}
#[test]
fn real_single_record_barriers_do_not_claim_windows_directory_durability() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    let result = lease.single_select(&id, &digest, &mut real_barrier, &mut no_hook);
    if cfg!(windows) {
        assert_eq!(
            result
                .unwrap_err()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::Unsupported
        );
        assert!(lease.single_startup(&mut real_barrier).is_err());
    } else {
        result.unwrap();
        lease.single_admit(&mut real_barrier, &mut no_hook).unwrap();
        assert_eq!(
            lease.single_startup(&mut real_barrier).unwrap(),
            State::Admitted(path)
        );
    }
}
#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_C4_ROOT") else {
        return;
    };
    let lease = Lease::open(Path::new(&root)).unwrap();
    let id = std::env::var("MNO_C4_ID").unwrap();
    let digest = std::env::var("MNO_C4_HASH").unwrap();
    let stop = std::env::var("MNO_C4_STOP").unwrap();
    if stop.starts_with("admission-") {
        lease
            .single_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        lease
            .single_admit(&mut modeled_barrier, &mut |p| {
                if (stop == "admission-before-publication" && p == "before-record-publication")
                    || (stop == "admission-after-publication" && p == "after-record-publication")
                {
                    std::process::exit(86);
                }
                Ok(())
            })
            .unwrap();
        if stop == "admission-acknowledged" {
            std::process::exit(86);
        }
    } else {
        lease
            .single_select(&id, &digest, &mut modeled_barrier, &mut |p| {
                if p == stop {
                    std::process::exit(86);
                }
                Ok(())
            })
            .unwrap();
    }
    panic!("interruption point not reached");
}
#[test]
fn selection_and_admission_interruptions_have_explicit_repeated_restart_states() {
    for stop in [
        "before-record-publication",
        "after-record-publication",
        "admission-before-publication",
        "admission-after-publication",
        "admission-acknowledged",
    ] {
        let root = fixture();
        let (old, oh, op) = generation(root.path());
        let (new, nh, np) = generation(root.path());
        {
            let lease = Lease::open(root.path()).unwrap();
            admit(&lease, &old, &oh);
        }
        let old_before = fs::read(op).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "single_record::interruption_child",
                "--nocapture",
            ])
            .env("MNO_C4_ROOT", root.path())
            .env("MNO_C4_ID", new)
            .env("MNO_C4_HASH", nh)
            .env("MNO_C4_STOP", stop)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child timed out");
            }
            std::thread::yield_now();
        };
        assert_eq!(status.code(), Some(86));
        let lease = Lease::open(root.path()).unwrap();
        let before = inventory(root.path());
        for _ in 0..3 {
            let result = lease.single_startup(&mut modeled_barrier);
            match stop {
                "after-record-publication" => {
                    assert_eq!(result.unwrap(), State::Pending(np.clone()))
                }
                "admission-after-publication" | "admission-acknowledged" => {
                    assert_eq!(result.unwrap(), State::Admitted(np.clone()))
                }
                _ => assert!(result.is_err()),
            }
        }
        assert_eq!(inventory(root.path()), before);
        assert_eq!(
            fs::read(
                root.path()
                    .join("generations")
                    .join(old)
                    .join("journal.sqlite")
            )
            .unwrap(),
            old_before
        );
    }
}
#[cfg(windows)]
#[test]
fn head_reader_without_delete_sharing_denies_replacement_without_state_changes() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = fixture();
    let (old, oh, op) = generation(root.path());
    let (new, nh, _) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    admit(&lease, &old, &oh);
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(root.path().join(NAME))
        .unwrap();
    let before = inventory(root.path());
    assert!(
        lease
            .single_select(&new, &nh, &mut modeled_barrier, &mut no_hook)
            .is_err()
    );
    assert_eq!(inventory(root.path()), before);
    assert_eq!(
        lease.single_startup(&mut modeled_barrier).unwrap(),
        State::Admitted(op)
    );
    drop(reader);
    lease
        .single_select(&new, &nh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
}

#[test]
fn admission_sync_failure_never_returns_success_or_assumes_old_record_survived() {
    for point in ["single-record-file", "single-record-directory"] {
        let root = fixture();
        let (id, digest, path) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        lease
            .single_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        let before_generations = inventory(&root.path().join("generations"));
        let mut calls = 0;
        let mut barrier = |p: &str, _: &Path| -> Result<()> {
            if p == point {
                calls += 1;
                // Permit the pending-startup reconfirmation, then fail publication
                // and later reconfirmations. This is not a lock retry.
                if calls >= 2 {
                    return Err("injected admission sync failure".into());
                }
            }
            Ok(())
        };
        assert!(lease.single_admit(&mut barrier, &mut no_hook).is_err());
        assert!(lease.single_startup(&mut barrier).is_err());
        let reopened = lease.single_startup(&mut modeled_barrier).unwrap();
        assert_eq!(
            reopened,
            if point == "single-record-file" {
                State::Pending(path)
            } else {
                State::Admitted(path)
            }
        );
        assert_eq!(
            inventory(&root.path().join("generations")),
            before_generations
        );
    }
}
