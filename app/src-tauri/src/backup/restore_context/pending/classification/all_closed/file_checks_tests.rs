use super::*;
use crate::{
    backup::restore_context::pending::classification::{
        self,
        tests::{Fixture, flag, tree},
    },
    database_state::DatabaseState,
};
use std::{fs, sync::atomic::Ordering};
fn run(action: impl FnOnce(&Fixture, &AllConnectionsClosed<'_, '_>, &DatabaseState)) {
    let f = Fixture::new();
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&f.root).is_err());
    let all = classification::classify_session(state.recovery_session().unwrap(), flag())
        .into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap()
        .remove_pending()
        .unwrap()
        .close_resources()
        .unwrap()
        .confirm_startup()
        .unwrap();
    action(&f, &all, &state);
    assert_eq!(all.generation, 1);
    assert!(state.operation(Some(1)).is_err());
    assert!(state.recovery_session().is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}
#[test]
fn unchanged_files_and_unrelated_files_are_accepted_without_mutation() {
    run(|f, all, _| {
        fs::write(f.root.join("user-notes.txt"), b"unrelated").unwrap();
        let before = tree(&f.root);
        let checks = all.file_checks().unwrap();
        checks.check().unwrap();
        assert_eq!(tree(&f.root), before);
    });
}
#[test]
fn changed_bytes_and_replaced_files_are_rejected_without_cleanup() {
    for which in 0..4 {
        for replace in [false, true] {
            run(|f, all, _| {
                let checks = all.file_checks().unwrap();
                let path = match which {
                    0 => f.path.clone(),
                    1 => f.protection.clone(),
                    2 => f.root.join(".mno-restore-completion-v1.json"),
                    _ => f.root.join(".mno-initialized-v1.json"),
                };
                if replace {
                    let bytes = fs::read(&path).unwrap();
                    fs::rename(&path, path.with_extension("held-original")).unwrap();
                    fs::write(&path, bytes).unwrap();
                } else {
                    use std::io::{Seek, SeekFrom, Write};
                    let mut out = fs::OpenOptions::new().write(true).open(&path).unwrap();
                    out.seek(SeekFrom::End(0)).unwrap();
                    out.write_all(b"changed").unwrap();
                }
                let before = tree(&f.root);
                assert!(
                    checks.check().is_err(),
                    "case {which}, replacement {replace}"
                );
                assert_eq!(tree(&f.root), before);
            });
        }
    }
}
#[test]
fn symlinks_and_hardlinks_to_each_security_file_are_rejected() {
    for which in 0..4 {
        for symlink in [false, true] {
            run(|f, all, _| {
                let checks = all.file_checks().unwrap();
                let path = match which {
                    0 => f.path.clone(),
                    1 => f.protection.clone(),
                    2 => f.root.join(".mno-restore-completion-v1.json"),
                    _ => f.root.join(".mno-initialized-v1.json"),
                };
                if symlink {
                    let held = path.with_extension("held");
                    fs::rename(&path, &held).unwrap();
                    std::os::unix::fs::symlink(&held, &path).unwrap();
                } else {
                    fs::hard_link(&path, path.with_extension("linked")).unwrap();
                }
                let before = tree(&f.root);
                assert!(checks.check().is_err());
                assert_eq!(tree(&f.root), before);
            });
        }
    }
}
#[test]
fn absent_invalid_or_unsafe_establishment_is_never_repaired() {
    for mode in 0..4 {
        run(|f, all, _| {
            let path = f.root.join(".mno-initialized-v1.json");
            fs::remove_file(&path).unwrap();
            match mode {
                0 => (),
                1 => fs::write(&path, b"{}").unwrap(),
                2 => fs::create_dir(&path).unwrap(),
                _ => std::os::unix::fs::symlink("missing", &path).unwrap(),
            }
            let before = tree(&f.root);
            assert!(all.file_checks().is_err());
            assert_eq!(tree(&f.root), before);
        });
    }
}
#[test]
fn returning_recovery_sidecars_and_reserved_names_block_but_are_preserved() {
    for name in [
        "recovery",
        "pending-restore.json",
        "mno.sqlite-journal",
        "mno.sqlite-wal",
        "mno.sqlite-shm",
        ".mno-completion-orphan",
        ".mno-init-orphan",
        ".pending-orphan",
        ".mno-restore-completion-v2.json",
    ] {
        run(|f, all, _| {
            let checks = all.file_checks().unwrap();
            if name == "recovery" {
                fs::create_dir(f.root.join(name)).unwrap();
            } else {
                fs::write(f.root.join(name), b"").unwrap();
            }
            let before = tree(&f.root);
            assert!(checks.check().is_err(), "{name}");
            assert!(fs::symlink_metadata(f.root.join(name)).is_ok());
            assert_eq!(tree(&f.root), before);
        });
    }
}
#[test]
fn cancellation_occurs_after_a_real_hash_block_and_preserves_guards() {
    run(|f, all, state| {
        let checks = all.file_checks().unwrap();
        let cancel = all.held().resources().closed().view().cancel.clone();
        let hit = std::rc::Rc::new(std::cell::Cell::new(false));
        let reached = hit.clone();
        use std::os::unix::fs::MetadataExt;
        let id = fs::metadata(&f.path).unwrap().ino();
        let before = tree(&f.root);
        let result = classification::hash_blocks::with_observer(
            move |file, bytes| {
                if file.metadata().unwrap().ino() == id {
                    assert!(bytes > 0);
                    reached.set(true);
                    cancel.store(true, Ordering::Release);
                }
            },
            || checks.check(),
        );
        assert!(hit.get());
        assert!(matches!(
            result,
            Err(FileCheckError::Files(Reason::Cancelled))
        ));
        assert_eq!(tree(&f.root), before);
        assert!(state.recovery_session().is_err());
    });
}
#[test]
fn panic_during_hashing_does_not_cleanup_or_unlock() {
    run(|f, all, state| {
        let checks = all.file_checks().unwrap();
        let before = tree(&f.root);
        let hit = std::rc::Rc::new(std::cell::Cell::new(false));
        let reached = hit.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            classification::hash_blocks::with_observer(
                move |_, bytes| {
                    assert!(bytes > 0);
                    reached.set(true);
                    panic!("during file hash");
                },
                || checks.check(),
            )
        }));
        assert!(result.is_err());
        assert!(hit.get());
        drop(checks);
        assert_eq!(tree(&f.root), before);
        assert!(state.recovery_session().is_err());
        assert!(state.operation(Some(1)).is_err());
    });
}

#[test]
fn ascii_case_reserved_artifacts_are_rejected_by_closed_file_checks() {
    for name in [
        ".MNO-COMPLETION-orphan",
        ".Mno-Completion-orphan",
        ".MNO-INIT-orphan",
        ".PENDING-orphan",
        ".MNO-RESTORE-COMPLETION-v2.json",
        "MNO.SQLITE-WAL",
    ] {
        run(|f, all, state| {
            let checks = all.file_checks().unwrap();
            fs::write(f.root.join(name), b"partial").unwrap();
            let before = tree(&f.root);
            assert!(checks.check().is_err(), "{name}");
            assert_eq!(tree(&f.root), before);
            assert!(state.recovery_session().is_err());
        });
    }
}
