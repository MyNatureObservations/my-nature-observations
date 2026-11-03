use super::super::*;
use super::SUFFIXES;
use crate::database_state::DatabaseState;
use std::{
    cell::Cell,
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};
use tempfile::TempDir;
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    File(Vec<u8>),
    Directory,
    Symlink(PathBuf),
    Other,
}
fn tree(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn walk(root: &Path, out: &mut BTreeMap<PathBuf, Entry>) {
        for entry in fs::read_dir(root).unwrap() {
            let p = entry.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            let value = if m.file_type().is_symlink() {
                Entry::Symlink(fs::read_link(&p).unwrap())
            } else if m.is_dir() {
                walk(&p, out);
                Entry::Directory
            } else if m.is_file() {
                Entry::File(fs::read(&p).unwrap())
            } else {
                Entry::Other
            };
            out.insert(p, value);
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}
fn established() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("book");
    let s = DatabaseState::default();
    let path = s.initialize_startup(&root).unwrap();
    drop(s);
    let root = root.canonicalize().unwrap();
    (dir, root, path)
}
fn sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
fn no_open(root: &Path, expected: &str) {
    let called = Cell::new(false);
    let outcome = begin_with(
        root,
        |p| fs::symlink_metadata(p),
        |_, _| {
            called.set(true);
            panic!("SQLite initializer must not be called")
        },
    )
    .unwrap();
    assert!(!called.get());
    let StartupOutcome::Recovery(gate) = outcome else {
        panic!("must remain gated")
    };
    assert!(gate.reason().starts_with(expected), "{}", gate.reason());
    gate.lease().unwrap().ensure_current().unwrap();
    assert!(DirectoryLease::open_existing(root).is_err());
    drop(gate);
}
fn blocked_state(root: &Path, expected: &str) {
    let before = tree(root);
    let s = DatabaseState::default();
    assert!(
        s.initialize_startup(root)
            .unwrap_err()
            .starts_with(expected)
    );
    assert!(s.operation(None).is_err());
    assert!(s.maintenance().is_err());
    assert!(DirectoryLease::open_existing(root).is_err());
    assert_eq!(tree(root), before);
    drop(s);
    assert!(DirectoryLease::open_existing(root).is_ok());
}
#[test]
fn normal_first_and_established_startups_without_sidecars_still_work() {
    let (_dir, root, path) = established();
    let before = tree(&root);
    let s = DatabaseState::default();
    assert_eq!(s.initialize_startup(&root).unwrap(), path);
    assert!(s.operation(None).is_ok());
    assert_eq!(tree(&root), before);
}
#[test]
fn every_sidecar_including_empty_files_blocks_before_any_sqlite_open() {
    for suffix in SUFFIXES {
        for bytes in [b"".as_slice(), b"leftover journal bytes".as_slice()] {
            let (_dir, root, path) = established();
            fs::write(sidecar(&path, suffix), bytes).unwrap();
            let before = tree(&root);
            no_open(&root, "database_sqlite_sidecar_present");
            blocked_state(&root, "database_sqlite_sidecar_present");
            assert_eq!(tree(&root), before);
        }
    }
}
#[test]
fn sidecar_directories_and_symlinks_are_not_followed_or_removed() {
    use std::os::unix::fs::symlink;
    for suffix in SUFFIXES {
        for kind in 0..3 {
            let (dir, root, path) = established();
            let outside = dir.path().join("external");
            fs::write(&outside, b"external original").unwrap();
            let p = sidecar(&path, suffix);
            match kind {
                0 => fs::create_dir(&p).unwrap(),
                1 => symlink(&outside, &p).unwrap(),
                _ => symlink(dir.path().join("missing"), &p).unwrap(),
            }
            let before = tree(&root);
            no_open(&root, "database_sqlite_sidecar_present");
            blocked_state(&root, "database_sqlite_sidecar_present");
            assert_eq!(tree(&root), before);
            assert_eq!(fs::read(outside).unwrap(), b"external original");
        }
    }
}
#[test]
fn metadata_io_errors_are_not_absence_and_keep_the_directory_lease() {
    for suffix in SUFFIXES {
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Other,
            io::ErrorKind::NotADirectory,
        ] {
            let (_dir, root, path) = established();
            let before = tree(&root);
            let fault = sidecar(&path, suffix);
            let called = Cell::new(false);
            let reached = Cell::new(false);
            let result = begin_with(
                &root,
                |p| {
                    if p == fault {
                        reached.set(true);
                        Err(io::Error::from(kind))
                    } else {
                        fs::symlink_metadata(p)
                    }
                },
                |_, _| {
                    called.set(true);
                    Ok(())
                },
            )
            .unwrap();
            assert!(reached.get());
            assert!(!called.get());
            let StartupOutcome::Recovery(gate) = result else {
                panic!()
            };
            assert!(
                gate.reason()
                    .starts_with("database_sqlite_sidecar_check_failed:")
            );
            assert!(DirectoryLease::open_existing(&root).is_err());
            assert_eq!(tree(&root), before);
        }
    }
}
#[test]
fn recovery_artifacts_and_sidecars_remain_blocked_together() {
    for receipt in [false, true] {
        let (_dir, root, path) = established();
        fs::write(sidecar(&path, "-wal"), b"keep wal").unwrap();
        if receipt {
            fs::write(
                root.join(".mno-restore-completion-v1.json"),
                b"unknown receipt",
            )
            .unwrap();
        } else {
            fs::create_dir(root.join("recovery")).unwrap();
            fs::write(root.join("recovery/pending-restore.json"), b"keep pending").unwrap();
        }
        let before = tree(&root);
        no_open(&root, "database_recovery_artifact_present");
        blocked_state(&root, "database_recovery_artifact_present");
        assert_eq!(tree(&root), before);
    }
}
#[test]
fn missing_established_database_with_sidecar_is_never_recreated() {
    for suffix in SUFFIXES {
        let (_dir, root, path) = established();
        fs::remove_file(&path).unwrap();
        fs::write(sidecar(&path, suffix), b"keep sidecar").unwrap();
        let before = tree(&root);
        no_open(&root, "database_sqlite_sidecar_present");
        blocked_state(&root, "database_sqlite_sidecar_present");
        assert!(!path.exists());
        assert_eq!(tree(&root), before);
        assert!(established::read(&root).unwrap());
    }
}
#[test]
fn uninitialized_directory_with_sidecar_never_gets_database_or_marker() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("book");
    fs::create_dir(&root).unwrap();
    let path = database::expected_database_path(&root);
    fs::write(sidecar(&path, "-shm"), b"").unwrap();
    // Acquire the normal lock first, so the snapshot also includes the lockfile.
    drop(DirectoryLease::acquire(&root).unwrap());
    let before = tree(&root);
    no_open(&root, "database_sqlite_sidecar_present");
    blocked_state(&root, "database_sqlite_sidecar_present");
    assert_eq!(tree(&root), before);
    assert!(!path.exists());
    assert!(!root.join(".mno-initialized-v1.json").exists());
}
#[test]
fn second_scan_catches_sidecar_created_after_the_first_scan_for_both_open_paths() {
    for new in [false, true] {
        let (dir, existing, _) = established();
        let root = if new {
            dir.path().canonicalize().unwrap().join("new-book")
        } else {
            existing
        };
        let path = database::expected_database_path(&root);
        let late = sidecar(&path, "-wal");
        let visits = Cell::new(0);
        let called = Cell::new(false);
        let result = begin_with(
            &root,
            |p| {
                if p == sidecar(&path, "-journal") {
                    visits.set(visits.get() + 1);
                    if visits.get() == 2 {
                        fs::write(&late, b"late external sidecar").unwrap();
                    }
                }
                fs::symlink_metadata(p)
            },
            |_, _| {
                called.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(visits.get(), 2);
        assert!(!called.get());
        let StartupOutcome::Recovery(gate) = result else {
            panic!()
        };
        assert_eq!(gate.reason(), "database_sqlite_sidecar_present");
        assert_eq!(fs::read(&late).unwrap(), b"late external sidecar");
        assert!(DirectoryLease::open_existing(&root).is_err());
        if new {
            assert!(!path.exists());
        }
    }
}
#[test]
fn sidecar_lock_probe_child() {
    let Some(root) = std::env::var_os("MNO_SIDECAR_LOCK_PROBE") else {
        return;
    };
    let s = DatabaseState::default();
    assert_eq!(
        s.initialize_startup(Path::new(&root)).unwrap_err(),
        "database_directory_in_use"
    );
    assert!(s.operation(None).is_err());
}
#[test]
fn sidecar_blocked_owner_excludes_a_second_mno_process() {
    let (_dir, root, path) = established();
    fs::write(sidecar(&path, "-journal"), b"keep").unwrap();
    let s = DatabaseState::default();
    assert_eq!(
        s.initialize_startup(&root).unwrap_err(),
        "database_sqlite_sidecar_present"
    );
    let before = tree(&root);
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup::sidecars::tests::sidecar_lock_probe_child",
            "--nocapture",
        ])
        .env("MNO_SIDECAR_LOCK_PROBE", &root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(tree(&root), before);
}
#[test]
fn hot_sidecar_child() {
    let Some(path) = std::env::var_os("MNO_STARTUP_HOT_SIDECAR") else {
        return;
    };
    let path = PathBuf::from(path);
    assert!(
        path.parent()
            .unwrap()
            .join("isolated-sidecar-child")
            .is_file()
    );
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .unwrap();
    db.execute_batch(
        "PRAGMA synchronous=FULL; PRAGMA cache_size=1; PRAGMA cache_spill=ON; BEGIN IMMEDIATE",
    )
    .unwrap();
    db.execute(
        "UPDATE journal_entries SET title=?1",
        ["x".repeat(256 * 1024)],
    )
    .unwrap();
    println!("STARTUP_HOT_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
#[test]
fn real_hot_journal_is_left_untouched_without_attempting_sqlite_recovery() {
    let (_dir, root, path) = established();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO journal_entries VALUES ('one','2026-10-18','old','{}',1,'created','updated')",
        [],
    )
    .unwrap();
    drop(db);
    fs::write(root.join("isolated-sidecar-child"), b"test only").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "startup::sidecars::tests::hot_sidecar_child",
            "--nocapture",
        ])
        .env("MNO_STARTUP_HOT_SIDECAR", &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap().trim() == "STARTUP_HOT_READY" {
                let _ = tx.send(());
                break;
            }
        }
    });
    let ready = rx.recv_timeout(std::time::Duration::from_secs(30));
    let _ = child.kill();
    child.wait().unwrap();
    ready.expect("hot-journal checkpoint not reached");
    let journal = fs::read(sidecar(&path, "-journal")).unwrap();
    assert!(journal.len() > 512);
    assert_eq!(
        &journal[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    let before = tree(&root);
    no_open(&root, "database_sqlite_sidecar_present");
    blocked_state(&root, "database_sqlite_sidecar_present");
    assert_eq!(tree(&root), before);
}

#[test]
fn sidecar_in_root_created_by_this_startup_blocks_before_marker_or_database() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap().join("fresh");
    let path = database::expected_database_path(&root);
    let injected = Cell::new(false);
    let called = Cell::new(false);
    let result = begin_with(
        &root,
        |p| {
            if !injected.replace(true) {
                fs::write(sidecar(&path, "-shm"), b"arrived during initial startup").unwrap();
            }
            fs::symlink_metadata(p)
        },
        |_, _| {
            called.set(true);
            Ok(())
        },
    )
    .unwrap();
    assert!(injected.get());
    assert!(!called.get());
    let StartupOutcome::Recovery(gate) = result else {
        panic!()
    };
    assert_eq!(gate.reason(), "database_sqlite_sidecar_present");
    assert!(!path.exists());
    assert!(!root.join(".mno-initialized-v1.json").exists());
    assert_eq!(
        fs::read(sidecar(&path, "-shm")).unwrap(),
        b"arrived during initial startup"
    );
}
#[test]
fn io_failure_at_last_scan_blocks_before_initializer_and_keeps_data() {
    let (_dir, root, path) = established();
    let before = tree(&root);
    let calls = Cell::new(0);
    let called = Cell::new(false);
    let result = begin_with(
        &root,
        |p| {
            if p == sidecar(&path, "-journal") {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                }
            }
            fs::symlink_metadata(p)
        },
        |_, _| {
            called.set(true);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert!(!called.get());
    let StartupOutcome::Recovery(gate) = result else {
        panic!()
    };
    assert!(
        gate.reason()
            .starts_with("database_sqlite_sidecar_check_failed:")
    );
    assert!(DirectoryLease::open_existing(&root).is_err());
    assert_eq!(tree(&root), before);
}
