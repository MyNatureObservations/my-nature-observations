use super::*;
use std::{cell::Cell, collections::BTreeMap};
use tempfile::TempDir;
fn established() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("book");
    let state = crate::database_state::DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    drop(state);
    (dir, root)
}
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for e in fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        let m = fs::symlink_metadata(&p).unwrap();
        if m.file_type().is_symlink() {
            out.insert(
                p.clone(),
                fs::read_link(&p)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            );
        } else if m.is_dir() {
            out.insert(p.clone(), b"directory".to_vec());
            out.extend(tree(&p));
        } else if m.is_file() {
            out.insert(p.clone(), fs::read(&p).unwrap());
        }
    }
    out
}
fn gated(root: &Path) {
    let before = tree(root);
    let called = Cell::new(false);
    let result = begin_with(
        root,
        |p| fs::symlink_metadata(p),
        |_, _| {
            called.set(true);
            Ok(())
        },
    )
    .unwrap();
    assert!(!called.get());
    let StartupOutcome::Recovery(gate) = result else {
        panic!("must gate")
    };
    assert!(DirectoryLease::open_existing(root).is_err());
    assert_eq!(tree(root), before);
    drop(gate);
    let state = crate::database_state::DatabaseState::default();
    assert!(state.initialize_startup(root).is_err());
    assert!(state.operation(Some(1)).is_err());
    assert!(state.maintenance().is_err());
    assert!(DirectoryLease::open_existing(root).is_err());
    assert_eq!(tree(root), before);
}
#[test]
fn first_start_and_existing_journal_accept_unrelated_files() {
    let (_dir, root) = established();
    fs::write(root.join("user-notes.txt"), b"notes").unwrap();
    let before = tree(&root);
    let state = crate::database_state::DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    assert!(state.operation(Some(1)).is_ok());
    assert_eq!(tree(&root), before);
}
#[test]
fn all_reserved_names_block_without_sqlite_or_cleanup() {
    for name in [
        "recovery",
        "pending-restore.json",
        ".mno-restore-completion-v1.json",
        "mno.sqlite-journal",
        "mno.sqlite-wal",
        "mno.sqlite-shm",
        ".mno-completion-orphan",
        ".mno-init-orphan",
        ".pending-orphan",
        ".mno-restore-completion-v2.json",
    ] {
        let (_dir, root) = established();
        fs::write(root.join(name), b"partial").unwrap();
        gated(&root);
    }
}
#[test]
fn unsafe_types_on_reserved_names_block_without_following() {
    for name in [
        "recovery",
        "pending-restore.json",
        ".mno-restore-completion-v1.json",
        ".mno-completion-orphan",
        "mno.sqlite-wal",
    ] {
        for link in [false, true] {
            let (dir, root) = established();
            let target = dir.path().join("outside");
            fs::write(&target, b"external original").unwrap();
            if link {
                std::os::unix::fs::symlink(&target, root.join(name)).unwrap();
            } else {
                fs::create_dir(root.join(name)).unwrap();
            }
            gated(&root);
            assert_eq!(fs::read(&target).unwrap(), b"external original");
        }
    }
}
#[test]
fn missing_journal_with_artifact_never_creates_replacement_or_marker() {
    for marked in [false, true] {
        for name in [
            "pending-restore.json",
            ".mno-completion-orphan",
            ".mno-init-orphan",
        ] {
            let (_dir, root) = established();
            fs::remove_file(root.join("mno.sqlite")).unwrap();
            if !marked {
                fs::remove_file(root.join(".mno-initialized-v1.json")).unwrap();
            }
            fs::write(root.join(name), b"partial").unwrap();
            gated(&root);
            assert!(!root.join("mno.sqlite").exists());
            assert_eq!(root.join(".mno-initialized-v1.json").exists(), marked);
        }
    }
}
#[test]
fn artifact_metadata_errors_keep_lease_and_never_call_initializer() {
    for name in [artifacts::RECOVERY, artifacts::PENDING, artifacts::RECEIPT] {
        for error in [io::ErrorKind::PermissionDenied, io::ErrorKind::Other] {
            let (_dir, root) = established();
            let before = tree(&root);
            let reached = Cell::new(false);
            let called = Cell::new(false);
            let outcome = begin_with(
                &root,
                |p| {
                    if p.file_name().is_some_and(|n| n == name) {
                        reached.set(true);
                        Err(io::Error::from(error))
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
            let StartupOutcome::Recovery(gate) = outcome else {
                panic!()
            };
            assert!(gate.reason().starts_with("database_startup_io:"));
            assert!(DirectoryLease::open_existing(&root).is_err());
            assert_eq!(tree(&root), before);
        }
    }
}
#[test]
fn late_temporary_is_caught_before_both_database_open_paths() {
    for existing in [false, true] {
        let (dir, root) = established();
        let root = if existing {
            root
        } else {
            dir.path().join("brand-new")
        };
        let calls = Cell::new(0);
        let called = Cell::new(false);
        let result = begin_with(
            &root,
            |p| {
                if p.file_name().is_some_and(|n| n == artifacts::RECEIPT) {
                    calls.set(calls.get() + 1);
                    if calls.get() == 2 {
                        fs::write(root.join(".mno-completion-late"), b"late").unwrap();
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
        let StartupOutcome::Recovery(_gate) = result else {
            panic!()
        };
        assert!(DirectoryLease::open_existing(&root).is_err());
        assert_eq!(
            fs::read(root.join(".mno-completion-late")).unwrap(),
            b"late"
        );
        assert_eq!(root.join("mno.sqlite").exists(), existing);
    }
}
#[test]
fn namespace_scan_io_error_is_not_absence() {
    let dir = TempDir::new().unwrap();
    let not_directory = dir.path().join("file");
    fs::write(&not_directory, b"data").unwrap();
    assert!(artifacts::unexpected_root(&not_directory, None).is_err());
    assert!(artifacts::unexpected_root(&dir.path().join("missing"), None).is_err());
}
#[test]
fn non_utf8_suffix_in_reserved_namespace_is_rejected() {
    use std::os::unix::ffi::OsStringExt;
    let mut name = b".mno-completion-".to_vec();
    name.push(0xff);
    // Some macOS filesystems reject such names themselves (EILSEQ). Test the
    // byte-based policy directly instead of depending on filesystem support.
    let name = std::ffi::OsString::from_vec(name);
    assert!(artifacts::unexpected_root_name(&name, None));
}

#[test]
fn ascii_case_variants_gate_early_before_establishment_or_sqlite() {
    for name in [
        ".MNO-COMPLETION-orphan",
        ".Mno-Completion-orphan",
        ".MNO-INIT-orphan",
        ".Mno-Init-orphan",
        ".PENDING-orphan",
        ".MNO-RESTORE-COMPLETION-v2.json",
        ".Mno-Restore-Completion-v2.json",
        "RECOVERY",
        "Pending-Restore.json",
        ".MNO-RESTORE-COMPLETION-V1.JSON",
        "MNO.SQLITE-JOURNAL",
        "Mno.Sqlite-Wal",
        "MNO.SQLITE-SHM",
    ] {
        let (_dir, root) = established();
        fs::remove_file(root.join(".mno-initialized-v1.json")).unwrap();
        fs::write(root.join(name), b"partial").unwrap();
        gated(&root);
        assert!(!root.join(".mno-initialized-v1.json").exists());
    }
}
#[test]
fn ascii_case_variants_gate_at_the_last_checkpoint_on_both_open_paths() {
    for existing in [false, true] {
        for name in [
            ".MNO-COMPLETION-late",
            ".Mno-Init-late",
            ".MNO-RESTORE-COMPLETION-v2.json",
            "MNO.SQLITE-WAL",
        ] {
            let (dir, old) = established();
            let root = if existing {
                old
            } else {
                dir.path().join("new-book")
            };
            let calls = Cell::new(0);
            let called = Cell::new(false);
            let result = begin_with(
                &root,
                |p| {
                    if p.file_name().is_some_and(|n| n == artifacts::RECEIPT) {
                        calls.set(calls.get() + 1);
                        if calls.get() == 2 {
                            fs::write(root.join(name), b"late").unwrap();
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
            let StartupOutcome::Recovery(_gate) = result else {
                panic!()
            };
            assert!(DirectoryLease::open_existing(&root).is_err());
            assert_eq!(fs::read(root.join(name)).unwrap(), b"late");
            assert_eq!(root.join("mno.sqlite").exists(), existing);
        }
    }
}
#[test]
fn ascii_matching_does_not_fold_unicode_or_cross_prefix_boundaries() {
    let (_dir, root) = established();
    for name in [
        ".mno-completionnotes",
        ".mno-initiation",
        ".pendingnotes",
        "recovery-notes",
        "pending-restore.json.notes",
        "mno.sqlite-wal-notes",
        ".mnö-completion-note",
        ".mno-completión-note",
        ".mno-İNİT-note",
        ".mno-reſtore-completion-note",
    ] {
        assert!(
            !artifacts::unexpected_root_name(std::ffi::OsStr::new(name), None),
            "{name}"
        );
        fs::write(root.join(name), b"unrelated").unwrap();
    }
    let before = tree(&root);
    let state = crate::database_state::DatabaseState::default();
    state.initialize_startup(&root).unwrap();
    assert!(state.operation(Some(1)).is_ok());
    assert_eq!(tree(&root), before);
}
#[test]
fn temporary_exception_requires_exact_encoded_name_even_for_case_aliases() {
    use std::ffi::OsStr;
    let dir = TempDir::new().unwrap();
    let name = OsStr::new(".mno-completion-current");
    fs::write(dir.path().join(name), b"temporary").unwrap();
    assert!(!artifacts::unexpected_root(dir.path(), Some(name)).unwrap());
    assert!(artifacts::unexpected_root_name(
        OsStr::new(".MNO-COMPLETION-current"),
        Some(name)
    ));
    assert!(artifacts::unexpected_root_name(
        OsStr::new(".Mno-Completion-current"),
        Some(name)
    ));
    // Rename works on both case-sensitive and case-insensitive filesystems.
    fs::rename(
        dir.path().join(name),
        dir.path().join(".MNO-COMPLETION-current"),
    )
    .unwrap();
    assert!(artifacts::unexpected_root(dir.path(), Some(name)).unwrap());
    assert_eq!(
        fs::read(dir.path().join(".MNO-COMPLETION-current")).unwrap(),
        b"temporary"
    );
}
