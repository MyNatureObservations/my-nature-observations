//! C3 test-only extension. One authoritative head; no production format or Ready token.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    AwaitingAdmission,
    Admitted,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    format: String,
    version: u32,
    sequence: u32,
    selection_sha256: String,
    phase: Phase,
}
#[derive(Debug, PartialEq, Eq)]
enum State {
    AwaitingAdmission(PathBuf),
    Admitted(PathBuf),
}

impl Lease {
    fn authority(&self) -> Result<Authority> {
        for entry in fs::read_dir(&self.root)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .starts_with("authority-pending-")
            {
                return reject();
            }
        }
        let path = self.root.join("authority.json");
        plain(&path, false)?;
        if fs::metadata(&path)?.len() > 4096 {
            return reject();
        }
        let a: Authority = serde_json::from_slice(&fs::read(path)?)?;
        if a.format != "mno-authority-prototype"
            || a.version != 1
            || a.sequence == 0
            || a.selection_sha256.len() != 64
            || !a.selection_sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return reject();
        }
        Ok(a)
    }
    fn bound_head(&self) -> Result<(Authority, Selection)> {
        let a = self.authority()?;
        let mut records = self.records()?;
        let (record, bytes) = records.pop().ok_or("authority without selection")?;
        if record.sequence != a.sequence || hash(&bytes) != a.selection_sha256 {
            return reject();
        }
        Ok((a, record))
    }
    fn publish_authority(
        &self,
        a: &Authority,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let mut temporary = tempfile::Builder::new()
            .prefix("authority-pending-")
            .tempfile_in(&self.root)?;
        temporary.write_all(&serde_json::to_vec(a)?)?;
        temporary.flush()?;
        barrier("authority-file", temporary.path())?;
        hook("before-authority-publication")?;
        // Single replaceable head. No journal/old selection record is replaced.
        let published = temporary
            .persist(self.root.join("authority.json"))
            .map_err(|e| e.error)?;
        hook("after-authority-publication")?;
        barrier("authority-directory", &self.root)?;
        drop(published);
        Ok(())
    }
    fn c3_select(
        &self,
        id: &str,
        digest: &str,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let records = self.records()?;
        if records.is_empty() {
            // Only an explicit first selection can establish authority. Never
            // bootstrap from an existing chain when authority has disappeared.
            if self.root.join("authority.json").try_exists()? {
                return reject();
            }
            for entry in fs::read_dir(&self.root)? {
                if entry?
                    .file_name()
                    .to_string_lossy()
                    .starts_with("authority-pending-")
                {
                    return reject();
                }
            }
        } else {
            let (a, _) = self.bound_head()?;
            if a.phase != Phase::Admitted {
                return reject();
            }
        }
        if records.len() >= 100 {
            return reject();
        }
        self.verify(id, digest)?;
        let selection = Selection {
            format: "mno-selection-prototype".into(),
            version: 1,
            sequence: records.len() as u32 + 1,
            generation: id.into(),
            sha256: digest.into(),
            previous: records.last().map(|(_, b)| hash(b)),
        };
        let authority = Authority {
            format: "mno-authority-prototype".into(),
            version: 1,
            sequence: selection.sequence,
            selection_sha256: hash(&serde_json::to_vec(&selection)?),
            phase: Phase::AwaitingAdmission,
        };
        // Intent is authoritative BEFORE appending the selection: interrupted
        // publication cannot silently authorize the preceding valid prefix.
        self.publish_authority(&authority, barrier, hook)?;
        self.select(id, digest, barrier, hook)
    }
    fn c3_startup(&self, barrier: &mut impl FnMut(&str, &Path) -> Result<()>) -> Result<State> {
        let (a, selection) = self.bound_head()?;
        let path = match a.phase {
            Phase::AwaitingAdmission => self.verify(&selection.generation, &selection.sha256)?,
            Phase::Admitted => {
                // Activation hash remains frozen in history. An admitted journal
                // is mutable: verify its CURRENT synthetic DB, never rewrite history.
                let path = self.candidate(&selection.generation)?;
                self.verify(&selection.generation, &hash(&fs::read(path)?))?
            }
        };
        barrier("candidate-file", &path)?;
        barrier("generation-directory", path.parent().unwrap())?;
        barrier("generations-directory", &self.root.join("generations"))?;
        barrier(
            "record-file",
            &self
                .root
                .join("selections")
                .join(format!("select-{:08}.json", selection.sequence)),
        )?;
        barrier("selection-directory", &self.root.join("selections"))?;
        barrier("authority-file", &self.root.join("authority.json"))?;
        barrier("authority-directory", &self.root)?;
        Ok(match a.phase {
            Phase::AwaitingAdmission => State::AwaitingAdmission(path),
            Phase::Admitted => State::Admitted(path),
        })
    }
    fn c3_admit(
        &self,
        barrier: &mut impl FnMut(&str, &Path) -> Result<()>,
        hook: &mut impl FnMut(&str) -> Result<()>,
    ) -> Result<PathBuf> {
        let path = match self.c3_startup(barrier)? {
            State::AwaitingAdmission(path) => path,
            State::Admitted(_) => return reject(),
        };
        let (mut a, selection) = self.bound_head()?;
        self.verify(&selection.generation, &selection.sha256)?;
        a.phase = Phase::Admitted;
        self.publish_authority(&a, barrier, hook)?;
        Ok(path) // No mutable handle before the admission record's barriers succeed.
    }
}

#[test]
fn first_admission_checked_edit_and_repeated_mutable_startup() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    {
        let lease = Lease::open(root.path()).unwrap();
        let before = inventory(root.path());
        assert!(lease.c3_startup(&mut modeled_barrier).is_err());
        assert_eq!(inventory(root.path()), before);
        lease
            .c3_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        assert_eq!(
            lease.c3_startup(&mut modeled_barrier).unwrap(),
            State::AwaitingAdmission(path.clone())
        );
        lease.c3_admit(&mut modeled_barrier, &mut no_hook).unwrap();
    }
    let record_before = inventory(&root.path().join("selections"));
    let authority_before = fs::read(root.path().join("authority.json")).unwrap();
    {
        let lease = Lease::open(root.path()).unwrap();
        assert_eq!(
            lease.c3_startup(&mut modeled_barrier).unwrap(),
            State::Admitted(path.clone())
        );
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        db.execute("UPDATE synthetic SET value='legitimate edit'", [])
            .unwrap();
        db.close().unwrap();
    }
    assert_ne!(hash(&fs::read(&path).unwrap()), digest);
    for _ in 0..3 {
        let lease = Lease::open(root.path()).unwrap();
        assert_eq!(
            lease.c3_startup(&mut modeled_barrier).unwrap(),
            State::Admitted(path.clone())
        );
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM synthetic", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "legitimate edit"
        );
        db.close().unwrap();
    }
    assert_eq!(inventory(&root.path().join("selections")), record_before);
    assert_eq!(
        fs::read(root.path().join("authority.json")).unwrap(),
        authority_before
    );
}

#[test]
fn unadmitted_candidate_edits_are_rejected_without_history_changes() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    lease
        .c3_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute("UPDATE synthetic SET value='not admitted'", [])
        .unwrap();
    db.close().unwrap();
    let before = inventory(root.path());
    assert!(lease.c3_admit(&mut modeled_barrier, &mut no_hook).is_err());
    assert!(lease.c3_startup(&mut modeled_barrier).is_err());
    assert_eq!(inventory(root.path()), before);
}

#[test]
fn missing_latest_truncated_or_conflicting_history_never_selects_old_prefix() {
    for fault in [
        "latest",
        "history",
        "truncated-record",
        "authority",
        "corrupt",
        "conflict",
    ] {
        let root = fixture();
        let (old, oh, op) = generation(root.path());
        let (new, nh, _) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        lease
            .c3_select(&old, &oh, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        lease.c3_admit(&mut modeled_barrier, &mut no_hook).unwrap();
        fs::write(&op, b"damaged previous").unwrap();
        fs::write(op.with_extension("sqlite-wal"), b"retained sidecar").unwrap();
        let old_before = inventory(op.parent().unwrap());
        lease
            .c3_select(&new, &nh, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        lease.c3_admit(&mut modeled_barrier, &mut no_hook).unwrap();
        let selections = root.path().join("selections");
        match fault {
            "latest" => fs::remove_file(selections.join("select-00000002.json")).unwrap(),
            "history" => {
                fs::remove_file(selections.join("select-00000001.json")).unwrap();
            }
            "truncated-record" => {
                let path = selections.join("select-00000002.json");
                let bytes = fs::read(&path).unwrap();
                fs::write(path, &bytes[..bytes.len() / 2]).unwrap();
            }
            "authority" => fs::remove_file(root.path().join("authority.json")).unwrap(),
            "corrupt" => fs::write(root.path().join("authority.json"), b"broken").unwrap(),
            _ => {
                let mut a = lease.authority().unwrap();
                a.selection_sha256 = "0".repeat(64);
                fs::write(
                    root.path().join("authority.json"),
                    serde_json::to_vec(&a).unwrap(),
                )
                .unwrap();
            }
        }
        let before = inventory(root.path());
        for _ in 0..3 {
            assert!(lease.c3_startup(&mut modeled_barrier).is_err());
        }
        assert_eq!(inventory(root.path()), before);
        assert_eq!(inventory(op.parent().unwrap()), old_before);
    }
}

#[test]
fn authority_sync_failures_do_not_grant_mutable_admission() {
    for point in ["authority-file", "authority-directory"] {
        let root = fixture();
        let (id, digest, path) = generation(root.path());
        let lease = Lease::open(root.path()).unwrap();
        lease
            .c3_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        let mut calls = 0;
        let mut barrier = |p: &str, _: &Path| -> Result<()> {
            if p == point {
                calls += 1;
                // Let startup reconfirm the pending state, then fail the
                // admission publication's barrier and subsequent reconfirmations.
                if calls >= 2 {
                    return Err("injected authority sync failure".into());
                }
            }
            Ok(())
        };
        assert!(lease.c3_admit(&mut barrier, &mut no_hook).is_err());
        assert!(lease.c3_startup(&mut barrier).is_err());
        // A published admission may survive a failed acknowledgement. Reopening
        // requires fresh successful barriers; do not pretend the old head survived.
        let state = lease.c3_startup(&mut modeled_barrier).unwrap();
        assert_eq!(
            state,
            if point == "authority-file" {
                State::AwaitingAdmission(path)
            } else {
                State::Admitted(path)
            }
        );
    }
}

#[test]
fn interruption_child() {
    let Some(root) = std::env::var_os("MNO_C3_ROOT") else {
        return;
    };
    let lease = Lease::open(Path::new(&root)).unwrap();
    let id = std::env::var("MNO_C3_ID").unwrap();
    let digest = std::env::var("MNO_C3_HASH").unwrap();
    let stop = std::env::var("MNO_C3_STOP").unwrap();
    if stop == "admission-published" {
        lease
            .c3_select(&id, &digest, &mut modeled_barrier, &mut no_hook)
            .unwrap();
        lease
            .c3_admit(&mut modeled_barrier, &mut |p| {
                if p == "after-authority-publication" {
                    std::process::exit(86);
                }
                Ok(())
            })
            .unwrap();
    } else {
        lease
            .c3_select(&id, &digest, &mut modeled_barrier, &mut |p| {
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
fn interrupted_intent_selection_and_admission_have_explicit_restart_states() {
    for stop in [
        "before-authority-publication",
        "after-authority-publication",
        "after-publication",
        "admission-published",
    ] {
        let root = fixture();
        let (id, digest, path) = generation(root.path());
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "admission::interruption_child", "--nocapture"])
            .env("MNO_C3_ROOT", root.path())
            .env("MNO_C3_ID", &id)
            .env("MNO_C3_HASH", digest)
            .env("MNO_C3_STOP", stop)
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
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(86));
        let lease = Lease::open(root.path()).unwrap();
        let before = inventory(root.path());
        for _ in 0..3 {
            let result = lease.c3_startup(&mut modeled_barrier);
            match stop {
                "after-publication" => {
                    assert_eq!(result.unwrap(), State::AwaitingAdmission(path.clone()))
                }
                "admission-published" => assert_eq!(result.unwrap(), State::Admitted(path.clone())),
                _ => assert!(result.is_err()),
            }
        }
        assert_eq!(inventory(root.path()), before);
    }
}

#[test]
fn real_admission_barriers_remain_explicit_about_windows_support() {
    let root = fixture();
    let (id, digest, path) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    let result = lease.c3_select(&id, &digest, &mut real_barrier, &mut no_hook);
    if cfg!(windows) {
        let error = result.unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::Unsupported
        );
        assert!(lease.c3_startup(&mut real_barrier).is_err());
    } else {
        result.unwrap();
        lease.c3_admit(&mut real_barrier, &mut no_hook).unwrap();
        assert_eq!(
            lease.c3_startup(&mut real_barrier).unwrap(),
            State::Admitted(path)
        );
    }
}

#[cfg(windows)]
#[test]
fn authority_replacement_denied_by_reader_fails_without_changing_selection() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = fixture();
    let (old, oh, op) = generation(root.path());
    let (new, nh, _) = generation(root.path());
    let lease = Lease::open(root.path()).unwrap();
    lease
        .c3_select(&old, &oh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
    lease.c3_admit(&mut modeled_barrier, &mut no_hook).unwrap();
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(root.path().join("authority.json"))
        .unwrap();
    let before = inventory(root.path());
    assert!(
        lease
            .c3_select(&new, &nh, &mut modeled_barrier, &mut no_hook)
            .is_err()
    );
    assert_eq!(inventory(root.path()), before);
    assert_eq!(
        lease.c3_startup(&mut modeled_barrier).unwrap(),
        State::Admitted(op)
    );
    drop(reader);
    lease
        .c3_select(&new, &nh, &mut modeled_barrier, &mut no_hook)
        .unwrap();
}
