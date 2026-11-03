use super::*;
use crate::{
    backup::{self, BackupKind},
    database_state::{DatabaseState, GENERATION_CONFLICT, RECOVERY_REQUIRED},
};
use publishing::{Point, publish};
use std::os::unix::fs::{PermissionsExt, symlink};
struct Fixture {
    state: DatabaseState,
    root: PathBuf,
    database: Vec<u8>,
    marker: Vec<u8>,
    protection: PathBuf,
    protection_bytes: Vec<u8>,
    bytes: Vec<u8>,
    _dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let state = DatabaseState::default();
        let path = state
            .initialize_legacy_startup(&dir.path().join("book"))
            .unwrap();
        let root = path.parent().unwrap().to_owned();
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES('one','2026-10-18','Original',?1,1,'created','updated')", [r#"{"type":"doc","content":[{"type":"paragraph"}]}"#]).unwrap();
        let archive = dir.path().join("incoming.mno-backup");
        backup::create_backup(&db, &archive, BackupKind::Manual).unwrap();
        drop(db);
        let candidate =
            backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
        let protection = crate::backup::restore_context::with_active(
            &state,
            &path,
            candidate,
            Arc::new(AtomicBool::new(false)),
            |mut ctx| {
                let auth = ctx.authorize()?;
                let marker = auth.publish_pending().unwrap();
                drop(marker);
                Ok(auth.protection_path().to_owned())
            },
        )
        .unwrap();
        let marker = fs::read(root.join(DIRECTORY).join(NAME)).unwrap();
        // Preserve whitespace and exact byte representation, not just semantic JSON.
        let mut exact = b" \n".to_vec();
        exact.extend_from_slice(&marker);
        exact.push(b'\n');
        fs::write(root.join(DIRECTORY).join(NAME), &exact).unwrap();
        let bytes = serde_json::to_vec(&CompletionReceipt {
            format: "mno-restore-completion".into(),
            version: 1,
            pending_json: String::from_utf8(exact.clone()).unwrap(),
            verified_outcome: Outcome::Old,
        })
        .unwrap();
        Self {
            database: fs::read(path).unwrap(),
            protection_bytes: fs::read(&protection).unwrap(),
            state,
            root,
            marker: exact,
            protection,
            bytes,
            _dir: dir,
        }
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.root.join("mno.sqlite")).unwrap(),
            self.database
        );
        assert_eq!(
            fs::read(self.root.join(DIRECTORY).join(NAME)).unwrap(),
            self.marker
        );
        assert_eq!(fs::read(&self.protection).unwrap(), self.protection_bytes);
        assert!(self.root.join(DIRECTORY).is_dir());
        assert_eq!(
            self.state.operation(Some(1)).err().unwrap(),
            RECOVERY_REQUIRED
        );
        assert_eq!(
            self.state.operation(Some(2)).err().unwrap(),
            GENERATION_CONFLICT
        );
        assert!(!fs::read_dir(&self.root).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".mno-completion-")
        }));
    }
    fn install(&self, bytes: &[u8]) {
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root).unwrap();
        temporary.write_all(bytes).unwrap();
        temporary
            .persist_noclobber(self.root.join(RECEIPT))
            .unwrap();
    }
}
#[test]
fn valid_receipt_preserves_exact_pending_and_stays_metadata_only() {
    let f = Fixture::new();
    assert!(read(&f.root).unwrap().is_none());
    for outcome in ["old", "new"] {
        let mut value: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
        value["verified_outcome"] = outcome.into();
        assert_eq!(
            parse_receipt(&serde_json::to_vec(&value).unwrap())
                .unwrap()
                .pending_json
                .as_bytes(),
            f.marker
        );
    }
    let loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
    assert_eq!(loaded.bytes, f.bytes);
    assert_eq!(loaded.receipt.pending_json.as_bytes(), f.marker);
    assert_eq!(
        identity(&loaded.root).unwrap(),
        parse(&f.marker).unwrap().directory_identity
    );
    assert_eq!(
        loaded.file.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(read(&f.root).unwrap().unwrap().bytes, f.bytes);
    drop(loaded);
    assert_eq!(fs::read(f.root.join(RECEIPT)).unwrap(), f.bytes);
    f.unchanged();
}
#[test]
fn strict_envelope_rejects_unknown_missing_duplicate_fields_and_invalid_values() {
    let f = Fixture::new();
    let original: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
    for (key, value) in [
        ("format", serde_json::json!("wrong")),
        ("version", serde_json::json!(2)),
        ("verified_outcome", serde_json::json!("uncertain")),
        ("verified_outcome", serde_json::json!(true)),
        ("unexpected", serde_json::json!(0)),
    ] {
        let mut changed = original.clone();
        changed[key] = value;
        assert!(parse_receipt(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    for key in ["format", "version", "pending_json", "verified_outcome"] {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(parse_receipt(&serde_json::to_vec(&missing).unwrap()).is_err());
        let mut duplicate = String::from_utf8(f.bytes.clone()).unwrap();
        duplicate.pop();
        duplicate.push_str(&format!(
            ",{}:{}}}",
            serde_json::to_string(key).unwrap(),
            original[key]
        ));
        assert!(parse_receipt(duplicate.as_bytes()).is_err());
    }
    f.unchanged();
}
#[test]
fn embedded_marker_and_metadata_limits_are_strict_and_bounded() {
    let f = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
    for pending in [
        "{}".to_string(),
        "not JSON".to_string(),
        " ".repeat(LIMIT as usize + 1),
        String::from_utf8(f.marker.clone())
            .unwrap()
            .replacen("{", "{\"version\":1,", 1),
    ] {
        value["pending_json"] = pending.into();
        assert!(parse_receipt(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    assert!(matches!(
        parse_receipt(&vec![b' '; RECEIPT_LIMIT as usize + 1]),
        Err(ReceiptError::TooLarge)
    ));
    let file = tempfile::NamedTempFile::new_in(&f.root).unwrap();
    file.as_file().set_len(RECEIPT_LIMIT + 1).unwrap();
    assert!(matches!(
        read_receipt_bytes(file.as_file()),
        Err(ReceiptError::TooLarge)
    ));
    f.unchanged();
}
#[test]
fn exact_marker_conflict_is_rejected_even_for_equivalent_json() {
    let f = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
    value["pending_json"] = serde_json::to_string(&parse(&f.marker).unwrap())
        .unwrap()
        .into();
    let changed = serde_json::to_vec(&value).unwrap();
    assert!(matches!(
        publish(&f.root, &changed, |_| Ok(())),
        Err(ReceiptError::Conflict)
    ));
    f.install(&changed);
    assert!(matches!(read(&f.root), Err(ReceiptError::Conflict)));
    f.unchanged();
}
#[test]
fn invalid_root_database_and_receipt_identities_are_rejected() {
    for key in ["directory_identity", "database_identity"] {
        let f = Fixture::new();
        let mut receipt: CompletionReceipt = parse_receipt(&f.bytes).unwrap();
        let mut pending: serde_json::Value = serde_json::from_slice(&f.marker).unwrap();
        pending[key]["file"] = "0000000000000000".into();
        receipt.pending_json = serde_json::to_string(&pending).unwrap();
        f.install(&serde_json::to_vec(&receipt).unwrap());
        assert!(matches!(read(&f.root), Err(ReceiptError::Binding)));
        f.unchanged();
    }
    let f = Fixture::new();
    let result = publish(&f.root, &f.bytes, |point| {
        if point == Point::Published {
            fs::rename(f.root.join(RECEIPT), f.root.join("saved-receipt"))?;
            fs::write(f.root.join(RECEIPT), &f.bytes)?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(ReceiptError::Binding)));
    assert_eq!(fs::read(f.root.join("saved-receipt")).unwrap(), f.bytes);
    f.unchanged();
}
#[test]
fn existing_destination_races_never_overwrite_a_winner() {
    for late in [false, true] {
        let f = Fixture::new();
        if !late {
            fs::write(f.root.join(RECEIPT), b"existing").unwrap();
        }
        let result = publish(&f.root, &f.bytes, |point| {
            if late && point == Point::BeforePublish {
                fs::write(f.root.join(RECEIPT), b"winner")?;
            }
            Ok(())
        });
        assert!(matches!(result, Err(ReceiptError::AlreadyExists)));
        assert_eq!(
            fs::read(f.root.join(RECEIPT)).unwrap(),
            if late {
                b"winner".as_slice()
            } else {
                b"existing".as_slice()
            }
        );
        f.unchanged();
    }
}
#[test]
fn unsafe_receipt_types_and_hardlinks_are_rejected_without_changes() {
    for kind in [0, 1, 2, 3] {
        let f = Fixture::new();
        let path = f.root.join(RECEIPT);
        match kind {
            0 => symlink(f.root.join("mno.sqlite"), &path).unwrap(),
            1 => fs::create_dir(&path).unwrap(),
            2 => fs::hard_link(f.root.join(DIRECTORY).join(NAME), &path).unwrap(),
            _ => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
        }
        assert!(read(&f.root).is_err());
        assert!(matches!(
            publish(&f.root, &f.bytes, |_| Ok(())),
            Err(ReceiptError::AlreadyExists) | Err(ReceiptError::Binding)
        ));
        // Hardlink is test-created; preserve it and compare original bytes directly.
        f.unchanged();
    }
}
#[test]
fn all_publication_faults_preserve_inputs_and_late_complete_receipts() {
    for at in [
        Point::Temporary,
        Point::PartialWrite,
        Point::FileSync,
        Point::BeforePublish,
        Point::Published,
        Point::DirectorySync,
        Point::FinalSync,
        Point::FinalCheck,
    ] {
        let f = Fixture::new();
        let result = publish(&f.root, &f.bytes, |point| {
            if point == at {
                return Err(io::Error::other("injected operation/sync failure"));
            }
            Ok(())
        });
        assert!(result.is_err(), "{at:?}");
        let after_publication = matches!(
            at,
            Point::Published | Point::DirectorySync | Point::FinalSync | Point::FinalCheck
        );
        assert_eq!(f.root.join(RECEIPT).exists(), after_publication, "{at:?}");
        if after_publication {
            assert_eq!(read(&f.root).unwrap().unwrap().bytes, f.bytes);
        }
        f.unchanged();
    }
}
#[test]
fn ordering_requires_verified_bytes_before_publish_and_all_barriers_before_success() {
    let f = Fixture::new();
    let mut order = Vec::new();
    publish(&f.root, &f.bytes, |point| {
        order.push(point);
        if matches!(point, Point::FileSync | Point::BeforePublish) {
            let temporary = fs::read_dir(&f.root)?
                .map(|e| e.unwrap().path())
                .find(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".mno-completion-")
                })
                .unwrap();
            assert_eq!(fs::read(temporary)?, f.bytes);
            assert!(!f.root.join(RECEIPT).exists());
        }
        if matches!(
            point,
            Point::Published | Point::DirectorySync | Point::FinalSync | Point::FinalCheck
        ) {
            assert_eq!(fs::read(f.root.join(RECEIPT))?, f.bytes);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        order,
        [
            Point::Temporary,
            Point::PartialWrite,
            Point::FileSync,
            Point::BeforePublish,
            Point::Published,
            Point::DirectorySync,
            Point::FinalSync,
            Point::FinalCheck
        ]
    );
    f.unchanged();
}
#[test]
fn root_swap_cannot_redirect_creation_or_publication() {
    let f = Fixture::new();
    let moved = f.root.with_file_name("moved");
    let result = publish(&f.root, &f.bytes, |point| {
        if point == Point::BeforePublish {
            fs::rename(&f.root, &moved)?;
            fs::create_dir(&f.root)?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(ReceiptError::Binding)));
    assert_eq!(fs::read_dir(&f.root).unwrap().count(), 0);
    fs::remove_dir(&f.root).unwrap();
    fs::rename(moved, &f.root).unwrap();
    assert!(!f.root.join(RECEIPT).exists());
    f.unchanged();
}

#[test]
fn simultaneous_publishers_leave_exactly_one_readable_receipt() {
    let f = Fixture::new();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            publish(&f.root, &f.bytes, |point| {
                if point == Point::BeforePublish {
                    barrier.wait();
                }
                Ok(())
            })
            .map(|_| ())
            .map_err(|error| matches!(error, ReceiptError::AlreadyExists))
        });
        let second = scope.spawn(|| {
            publish(&f.root, &f.bytes, |point| {
                if point == Point::BeforePublish {
                    barrier.wait();
                }
                Ok(())
            })
            .map(|_| ())
            .map_err(|error| matches!(error, ReceiptError::AlreadyExists))
        });
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|r| matches!(r, Err(true))).count(), 1);
    assert_eq!(read(&f.root).unwrap().unwrap().bytes, f.bytes);
    f.unchanged();
}

#[test]
fn changed_temporary_bytes_are_rejected_before_the_final_name_appears() {
    let f = Fixture::new();
    let result = publish(&f.root, &f.bytes, |point| {
        if point == Point::BeforePublish {
            let temporary = fs::read_dir(&f.root)?
                .map(|e| e.unwrap().path())
                .find(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".mno-completion-")
                })
                .unwrap();
            fs::write(temporary, b"incomplete or changed receipt")?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(ReceiptError::Invalid)));
    assert!(!f.root.join(RECEIPT).exists());
    f.unchanged();
}

#[test]
fn outcome_is_exclusively_an_old_or_new_json_string() {
    let f = Fixture::new();
    let original: serde_json::Value = serde_json::from_slice(&f.bytes).unwrap();
    for (text, expected) in [("old", Outcome::Old), ("new", Outcome::New)] {
        let mut value = original.clone();
        value["verified_outcome"] = serde_json::json!(text);
        let parsed = parse_receipt(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.verified_outcome, expected);
        assert_eq!(parsed.pending_json.as_bytes(), f.marker);
    }
    for invalid in [
        serde_json::json!({"old":null}),
        serde_json::json!({"new":null}),
        serde_json::Value::Null,
        serde_json::json!(true),
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!([]),
        serde_json::json!(["old"]),
        serde_json::json!({}),
        serde_json::json!("unknown"),
        serde_json::json!("Old"),
        serde_json::json!(""),
    ] {
        let mut value = original.clone();
        value["verified_outcome"] = invalid.clone();
        assert!(
            matches!(
                parse_receipt(&serde_json::to_vec(&value).unwrap()),
                Err(ReceiptError::Invalid)
            ),
            "accepted {invalid}"
        );
    }
    f.unchanged();
}

#[test]
fn errors_from_each_sync_function_propagate_and_preserve_published_receipts() {
    for (at, expected_calls) in [
        (SyncPoint::VerifiedFile, vec![SyncPoint::VerifiedFile]),
        (
            SyncPoint::Directory,
            vec![SyncPoint::VerifiedFile, SyncPoint::Directory],
        ),
        (
            SyncPoint::FinalFileBarrier,
            vec![
                SyncPoint::VerifiedFile,
                SyncPoint::Directory,
                SyncPoint::FinalFileBarrier,
            ],
        ),
    ] {
        let f = Fixture::new();
        let mut calls = Vec::new();
        let result = publishing::publish_with_sync(
            &f.root,
            &f.bytes,
            |_| Ok(()),
            |point, file| {
                calls.push(point);
                if point == SyncPoint::Directory {
                    assert!(file.metadata()?.is_dir());
                } else {
                    assert_eq!(read_receipt_bytes(file).unwrap(), f.bytes);
                }
                if point == at {
                    return Err(io::Error::from_raw_os_error(libc::EIO));
                }
                durability::sync_opened(point, file)
            },
        );
        assert!(
            matches!(result, Err(ReceiptError::Io(ref error)) if error.raw_os_error() == Some(libc::EIO)),
            "sync failure {at:?} was lost"
        );
        assert_eq!(calls, expected_calls);
        assert_eq!(f.root.join(RECEIPT).exists(), at != SyncPoint::VerifiedFile);
        if at != SyncPoint::VerifiedFile {
            let loaded = read(&f.root).unwrap().unwrap();
            assert_eq!(loaded.bytes, f.bytes);
            drop(loaded);
            assert_eq!(fs::read(f.root.join(RECEIPT)).unwrap(), f.bytes);
        }
        f.unchanged();
    }
}

#[test]
fn retained_receipt_checks_both_handle_and_fixed_name() {
    let f = Fixture::new();
    let loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
    loaded.check_handle().unwrap();
    loaded.check_name(&f.root).unwrap();
    loaded.check(&f.root).unwrap();
    f.unchanged();
}

#[test]
fn retained_receipt_rejects_changed_bytes_and_embedded_pending() {
    for change in 0..3 {
        let f = Fixture::new();
        let loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
        let mut changed = f.bytes.clone();
        match change {
            0 => changed.push(b' '), // Valid equivalent JSON is still different bytes.
            1 => changed = b"{}".to_vec(),
            _ => {
                let mut receipt = parse_receipt(&f.bytes).unwrap();
                receipt.pending_json.push(' '); // Valid pending, wrong exact binding.
                changed = serde_json::to_vec(&receipt).unwrap();
            }
        }
        fs::write(f.root.join(RECEIPT), &changed).unwrap();
        assert!(loaded.check_handle().is_err());
        assert!(loaded.check_name(&f.root).is_err());
        assert!(loaded.check(&f.root).is_err());
        assert_eq!(fs::read(f.root.join(RECEIPT)).unwrap(), changed);
        f.unchanged();
    }
}

#[test]
fn retained_receipt_handle_survives_name_loss_but_named_verification_never_does() {
    for replacement in 0..5 {
        let f = Fixture::new();
        let loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
        let path = f.root.join(RECEIPT);
        // Test-only name mutation, never a production removal capability.
        fs::remove_file(&path).unwrap();
        match replacement {
            0 => (),
            1 => fs::write(&path, &f.bytes).unwrap(), // Same bytes, different inode.
            2 => symlink(f.root.join("mno.sqlite"), &path).unwrap(),
            3 => fs::create_dir(&path).unwrap(),
            _ => {
                use std::os::unix::ffi::OsStrExt;
                let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
        }
        // Only the retained object's properties are proven by this observation.
        loaded.check_handle().unwrap();
        assert_eq!(read_receipt_bytes(&loaded.file).unwrap(), f.bytes);
        assert!(loaded.check_name(&f.root).is_err());
        assert!(loaded.check(&f.root).is_err());
        f.unchanged();
    }
}

#[test]
fn retained_receipt_rejects_hardlinks_and_wrong_handle_identity() {
    let f = Fixture::new();
    let mut loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
    let alias = f.root.join("unrelated-hardlink");
    fs::hard_link(f.root.join(RECEIPT), &alias).unwrap();
    assert!(loaded.check_handle().is_err());
    assert!(loaded.check_name(&f.root).is_err());
    assert!(loaded.check(&f.root).is_err());
    fs::remove_file(alias).unwrap();
    loaded.check(&f.root).unwrap();
    loaded.file_identity = Identity::from_parts(0, 0);
    assert!(loaded.check_handle().is_err());
    assert!(loaded.check_name(&f.root).is_err());
    f.unchanged();
}

#[test]
fn retained_receipt_preserves_directory_and_external_pending_binding() {
    let f = Fixture::new();
    let mut loaded = publish(&f.root, &f.bytes, |_| Ok(())).unwrap();
    let other = tempfile::TempDir::new().unwrap();
    assert!(loaded.check_name(other.path()).is_err());
    assert!(loaded.check(other.path()).is_err());
    loaded.check_handle().unwrap();
    let mut changed_pending = f.marker.clone();
    changed_pending.push(b' ');
    fs::write(f.root.join(DIRECTORY).join(NAME), &changed_pending).unwrap();
    // A retained Q is unchanged; production check must still reject changed P.
    loaded.check_handle().unwrap();
    assert!(matches!(loaded.check(&f.root), Err(ReceiptError::Conflict)));
    fs::write(f.root.join(DIRECTORY).join(NAME), &f.marker).unwrap();
    loaded.check(&f.root).unwrap();
    loaded.root = open_plain(other.path(), true).unwrap();
    assert!(loaded.check_handle().is_err());
    assert!(loaded.check(&f.root).is_err());
    f.unchanged();
}

#[test]
fn versioning_completion_v1_preserves_frozen_p_bytes_and_rejects_new_combinations() {
    let pending = crate::backup::restore_context::pending::tests::PENDING_V1_FIXTURE;
    let text = std::str::from_utf8(pending).unwrap();
    let escaped = serde_json::to_string(text).unwrap();
    for (outcome, typed) in [("old", Outcome::Old), ("new", Outcome::New)] {
        let expected = format!(
            r#"{{"format":"mno-restore-completion","version":1,"pending_json":{escaped},"verified_outcome":"{outcome}"}}"#
        );
        let bytes = receipt_bytes(pending, typed).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        assert_eq!(
            parse_receipt(&bytes).unwrap().pending_json.as_bytes(),
            pending
        );
        let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for (field, version) in [
            ("version", 2),
            ("schema_version", 1),
            ("old_content", 2),
            ("new_content", 2),
        ] {
            let mut changed = original.clone();
            if field == "version" {
                changed["version"] = version.into();
            } else {
                let mut nested: serde_json::Value = serde_json::from_str(text).unwrap();
                if field == "schema_version" {
                    nested[field] = version.into();
                } else {
                    nested[field]["version"] = version.into();
                }
                changed["pending_json"] = serde_json::to_string(&nested).unwrap().into();
            }
            assert!(parse_receipt(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
    }
}
