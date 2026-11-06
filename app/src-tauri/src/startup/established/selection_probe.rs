//! Isolated C7.1 publication under the existing directory lease on local NTFS.
use super::*;
use crate::startup::{
    DirectoryLease,
    generation_selection::{LIMIT, LayoutEvidence, Phase, Record, Selection, classify},
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING;

const HEAD: &str = "generation-selection.json";
const STAGED: &str = ".generation-selection-staged";
const INTENT: &str = ".generation-selection-in-progress";

fn present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn read_record(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !plain(&metadata) {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    Record::decode(&bytes)?;
    Ok(bytes)
}

fn reopen(lease: &DirectoryLease, evidence: LayoutEvidence) -> io::Result<Selection> {
    lease.ensure_current().map_err(io::Error::other)?;
    lease.boundary.check()?;
    let root = &lease.root;
    let interrupted = present(&root.join(INTENT))? || present(&root.join(STAGED))?;
    let head = root.join(HEAD);
    let bytes = present(&head)?.then(|| read_record(&head)).transpose()?;
    classify(evidence, bytes.as_deref(), interrupted)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Intent,
    Prepared,
    Replaced,
    Flushed,
}

fn publish(
    lease: &DirectoryLease,
    evidence: LayoutEvidence,
    record: &Record,
    mut hook: impl FnMut(Step) -> io::Result<()>,
) -> io::Result<()> {
    let previous = match reopen(lease, evidence)? {
        Selection::Managed(old) => Some(old),
        Selection::Legacy => None,
        Selection::Fresh => return Err(invalid()),
    };
    if !record.follows(previous.as_ref()) {
        return Err(invalid());
    }
    let bytes = record.encode()?;
    let root = &lease.root;
    let intent = root.join(INTENT);
    let staged = root.join(STAGED);
    let head = root.join(HEAD);
    let create = |path: &Path| OpenOptions::new().write(true).create_new(true).open(path);
    // Keep evidence across replacement and every fallible persistence barrier.
    let marker = create(&intent)?;
    flush(&marker)?;
    drop(marker);
    lease.boundary.sync()?;
    hook(Step::Intent)?;

    let mut output = create(&staged)?;
    output.write_all(&bytes)?;
    flush(&output)?;
    drop(output);
    if read_record(&staged)? != bytes {
        return Err(invalid());
    }
    lease.boundary.sync()?;
    hook(Step::Prepared)?;
    lease.ensure_current().map_err(io::Error::other)?;
    lease.boundary.check()?;
    let flags = MOVEFILE_WRITE_THROUGH
        | if previous.is_some() {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(wide(&staged).as_ptr(), wide(&head).as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    hook(Step::Replaced)?;
    let published = opened_file(&head)?;
    if read_record(&head)? != bytes {
        return Err(invalid());
    }
    flush(&published)?;
    lease.boundary.sync()?;
    hook(Step::Flushed)?;
    // A stale marker after a crash blocks reopening. The record and its
    // directory entry have passed all barriers before marker removal.
    fs::remove_file(intent)
}

fn fixture() -> (tempfile::TempDir, DirectoryLease) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let lease = DirectoryLease::acquire(&root).unwrap();
    fs::write(root.join("mno.sqlite"), b"legacy journal sentinel").unwrap();
    fs::write(root.join("backup.mno-backup"), b"backup sentinel").unwrap();
    (temp, lease)
}

fn pending() -> Record {
    Record::new(Uuid::new_v4(), Phase::Pending, "a".repeat(64))
}

#[test]
fn repeated_ntfs_publication_and_reopen_preserve_legacy() {
    let (_temp, mut lease) = fixture();
    let root = lease.root.clone();
    let mut evidence = LayoutEvidence::Legacy;
    for _ in 0..3 {
        let record = pending();
        publish(&lease, evidence, &record, |_| Ok(())).unwrap();
        drop(lease);
        lease = DirectoryLease::open_existing(&root).unwrap();
        assert_eq!(
            reopen(&lease, LayoutEvidence::Managed).unwrap(),
            Selection::Managed(record.clone())
        );
        let admitted = Record::new(
            record.generation.parse().unwrap(),
            Phase::Admitted,
            record.candidate_sha256.clone(),
        );
        publish(&lease, LayoutEvidence::Managed, &admitted, |_| Ok(())).unwrap();
        assert_eq!(
            reopen(&lease, LayoutEvidence::Managed).unwrap(),
            Selection::Managed(admitted)
        );
        evidence = LayoutEvidence::Managed;
    }
    fs::remove_file(root.join(HEAD)).unwrap();
    assert!(reopen(&lease, LayoutEvidence::Managed).is_err());
    assert_eq!(
        fs::read(root.join("mno.sqlite")).unwrap(),
        b"legacy journal sentinel"
    );
}

#[test]
fn interrupted_replacement_blocks_even_with_a_valid_head() {
    for failure in [Step::Intent, Step::Prepared, Step::Replaced, Step::Flushed] {
        let (_temp, lease) = fixture();
        let record = pending();
        publish(&lease, LayoutEvidence::Legacy, &record, |_| Ok(())).unwrap();
        let admitted = Record::new(
            record.generation.parse().unwrap(),
            Phase::Admitted,
            record.candidate_sha256.clone(),
        );
        assert!(
            publish(&lease, LayoutEvidence::Managed, &admitted, |step| {
                if step == failure {
                    Err(io::Error::other("injected interruption"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        let root = lease.root.clone();
        drop(lease);
        let lease = DirectoryLease::open_existing(&root).unwrap();
        assert!(root.join(INTENT).is_file());
        let visible = Record::decode(&read_record(&root.join(HEAD)).unwrap()).unwrap();
        assert_eq!(
            visible,
            if matches!(failure, Step::Replaced | Step::Flushed) {
                admitted
            } else {
                record
            }
        );
        assert_eq!(
            fs::read(root.join("backup.mno-backup")).unwrap(),
            b"backup sentinel"
        );
        assert!(reopen(&lease, LayoutEvidence::Managed).is_err());
        assert_eq!(
            fs::read(root.join("mno.sqlite")).unwrap(),
            b"legacy journal sentinel"
        );
    }
}

#[test]
fn sharing_denial_preserves_previous_record_and_evidence() {
    let (_temp, lease) = fixture();
    let record = pending();
    publish(&lease, LayoutEvidence::Legacy, &record, |_| Ok(())).unwrap();
    let head = lease.root.join(HEAD);
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&head)
        .unwrap();
    let admitted = Record::new(
        record.generation.parse().unwrap(),
        Phase::Admitted,
        record.candidate_sha256.clone(),
    );
    assert!(publish(&lease, LayoutEvidence::Managed, &admitted, |_| Ok(())).is_err());
    drop(reader);
    assert_eq!(read_record(&head).unwrap(), record.encode().unwrap());
    assert!(lease.root.join(STAGED).is_file());
    assert!(reopen(&lease, LayoutEvidence::Managed).is_err());
}

#[test]
fn corrupt_managed_record_blocks_publication_without_falling_back() {
    let (_temp, lease) = fixture();
    let head = lease.root.join(HEAD);
    fs::write(&head, b"{partial").unwrap();
    assert!(reopen(&lease, LayoutEvidence::Managed).is_err());
    assert!(publish(&lease, LayoutEvidence::Managed, &pending(), |_| Ok(())).is_err());
    assert_eq!(fs::read(head).unwrap(), b"{partial");
    assert!(!lease.root.join(INTENT).exists());
}
