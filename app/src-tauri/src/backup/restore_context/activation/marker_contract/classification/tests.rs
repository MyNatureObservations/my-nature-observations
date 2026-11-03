use super::super::{PreviousContent, ReplacementContent};
use super::*;
use crate::{backup::restore_context::ActiveDatabase, database, database_state::DatabaseState};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;

const OLD: &str = "{ \"text\":\"gammal ö 🌿\", \"text\":\"duplicate\" }";
fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn database_proof(path: &Path) -> ContentProof {
    let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let proof = open_import::verify_database(&mut db, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    proof
}
pub(super) fn fixture(root: &Path, same: bool) -> ImportV2 {
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root.join("data")).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES('old','legacy date','old title',?1,1,'created','updated')",[OLD]).unwrap();
    db.close().unwrap();
    fs::copy(&path, root.join("old.sqlite")).unwrap();
    let next = root.join("new.sqlite");
    if same {
        fs::copy(&path, &next).unwrap();
    } else {
        let db = Connection::open(&next).unwrap();
        database::initialize_schema(&db).unwrap();
        db.execute("INSERT INTO journal_entries VALUES('new','2026-10-25','new title','{}',1,'new created','new updated')",[]).unwrap();
        db.close().unwrap();
    }
    let new = database_proof(&next);
    let mut active = ActiveDatabase::open(&path).unwrap();
    let maintenance = state.maintenance().unwrap();
    let protection = import_protection::create(&maintenance, &mut active, flag()).unwrap();
    let marker = ImportV2::new(
        maintenance.operation_id().to_string(),
        Target::New,
        PreviousContent::try_from(protection.content().clone()).unwrap(),
        ReplacementContent::try_from(new).unwrap(),
        protection.archive_sha256().to_owned(),
    )
    .unwrap();
    assert_eq!(
        protection.path().file_name().unwrap(),
        marker.protection_file.as_str()
    );
    drop(protection);
    drop(active);
    drop(maintenance);
    drop(state);
    fs::create_dir(root.join("data/recovery")).unwrap();
    fs::copy(
        next,
        root.join("data/recovery").join(&marker.candidate_file),
    )
    .unwrap();
    save(root, &marker);
    marker
}
pub(super) fn save(root: &Path, m: &ImportV2) {
    fs::write(
        root.join("data/recovery").join(activation::MARKER),
        serde_json::to_vec(m).unwrap(),
    )
    .unwrap();
}
fn archive(root: &Path, m: &ImportV2) -> PathBuf {
    root.join("data/backups/pre-restore")
        .join(&m.protection_file)
}
fn next(root: &Path, m: &ImportV2) -> PathBuf {
    root.join("data/recovery").join(&m.candidate_file)
}
fn previous(root: &Path, m: &ImportV2) -> PathBuf {
    root.join("data/recovery").join(&m.previous_candidate_file)
}
pub(super) fn owner(root: &Path) -> DatabaseState {
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&root.join("data")).is_err());
    assert!(state.operation(None).is_err());
    state
}
// Snapshot names, identities, lengths, modification times and bytes. Never follow
// symlinks or open special files. Access times are deliberately not state evidence.
pub(super) fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<PathBuf, String>) {
        let m = fs::symlink_metadata(path).unwrap();
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (m.dev(), m.ino(), m.nlink())
        };
        #[cfg(not(unix))]
        let identity = (0u64, 0u64, 0u64);
        let kind = if m.file_type().is_symlink() {
            format!("link {:?}", fs::read_link(path).unwrap())
        } else if m.is_dir() {
            "directory".into()
        } else if m.is_file() {
            format!("file {:?}", fs::read(path).unwrap())
        } else {
            "special".into()
        };
        out.insert(
            path.strip_prefix(base).unwrap().to_owned(),
            format!(
                "{identity:?} {:?} {} {kind}",
                m.modified().unwrap(),
                m.len()
            ),
        );
        if m.is_dir() && !m.file_type().is_symlink() {
            for item in fs::read_dir(path).unwrap() {
                visit(base, &item.unwrap().path(), out);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn classify_observed(
    session: &RecoverySession<'_, '_>,
    flag: Arc<AtomicBool>,
) -> Result<Assessment> {
    let (result, attempts) = import_protection::observe_sqlite_opens(|| classify(session, flag));
    if result.is_err() {
        assert!(
            attempts.is_empty(),
            "pre-open rejection attempted SQLite: {attempts:?}"
        );
    }
    result
}
fn observe_unchanged(root: &Path) -> Result<Assessment> {
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    let first = classify_observed(&session, flag());
    let second = classify_observed(&session, flag());
    assert_eq!(first, second);
    assert_eq!(tree(root), before);
    assert!(state.operation(None).is_err());
    match activation::read_marker(&root.join("data")) {
        Err(activation::Error::Binding) => {}
        Err(activation::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => panic!("unexpected production acceptance or marker error"),
    }
    first
}
#[test]
fn both_targets_with_old_and_new_active_content_keep_the_persisted_choice() {
    for target in [Target::New, Target::Old] {
        for active_new in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let mut m = fixture(root, false);
            m.target = target;
            if active_new {
                fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
            }
            save(root, &m);
            let observed = observe_unchanged(root).unwrap();
            assert_eq!(observed.target, target);
            assert_eq!(
                observed.active,
                if active_new { Active::New } else { Active::Old }
            );
            assert_eq!(observed.protection, Protection::Verified);
            assert!(observed.previous_available);
            assert_eq!(observed.new_candidate, Candidate::Verified);
            assert_eq!(observed.previous_candidate, Candidate::Missing);
            assert!(!observed.identical_content);
            assert_eq!(
                observed.next_check,
                match (target, active_new) {
                    (Target::New, false) => NextCheck::NewReplacement,
                    (Target::New, true) | (Target::Old, false) => NextCheck::Completion,
                    (Target::Old, true) => NextCheck::BuildPrevious,
                }
            );
            if active_new {
                fs::remove_file(next(root, &m)).unwrap();
                assert_eq!(
                    observe_unchanged(root).unwrap().next_check,
                    observed.next_check
                );
                if target == Target::Old {
                    fs::copy(root.join("old.sqlite"), previous(root, &m)).unwrap();
                    assert_eq!(
                        observe_unchanged(root).unwrap().next_check,
                        NextCheck::OldReplacement
                    );
                }
            }
        }
    }
}
#[test]
fn identical_proofs_are_both_not_a_claim_of_swap_history() {
    for target in [Target::New, Target::Old] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut m = fixture(root, true);
        m.target = target;
        save(root, &m);
        let observed = observe_unchanged(root).unwrap();
        assert_eq!(observed.target, target);
        assert_eq!(observed.active, Active::Both);
        assert!(observed.identical_content);
        assert_eq!(observed.next_check, NextCheck::Completion);
        fs::remove_file(next(root, &m)).unwrap();
        assert_eq!(
            observe_unchanged(root).unwrap().next_check,
            NextCheck::Completion
        );
    }
}
#[test]
fn missing_invalid_and_unrecognized_active_never_offer_completion() {
    for target in [Target::New, Target::Old] {
        for mode in ["missing", "corrupt", "other"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let mut m = fixture(root, false);
            m.target = target;
            save(root, &m);
            let active = root.join("data/mno.sqlite");
            match mode {
                "missing" => fs::remove_file(&active).unwrap(),
                "corrupt" => fs::write(&active, b"damaged").unwrap(),
                _ => {
                    let db = Connection::open(&active).unwrap();
                    db.execute("UPDATE journal_entries SET title='different journal'", [])
                        .unwrap();
                    db.close().unwrap();
                }
            }
            let observed = observe_unchanged(root).unwrap();
            assert_eq!(
                observed.active,
                match mode {
                    "missing" => Active::Missing,
                    "corrupt" => Active::Invalid,
                    _ => Active::Other,
                }
            );
            assert_eq!(observed.target, target);
            assert!(observed.previous_available);
            assert_eq!(
                observed.next_check,
                if target == Target::Old && mode != "other" {
                    NextCheck::BuildPrevious
                } else {
                    NextCheck::Blocked
                }
            );
            if target == Target::Old && mode != "other" {
                fs::copy(root.join("old.sqlite"), previous(root, &m)).unwrap();
                assert_eq!(
                    observe_unchanged(root).unwrap().next_check,
                    NextCheck::OldReplacement
                );
            }
        }
    }
}
#[test]
fn missing_corrupt_or_unbound_protection_blocks_even_selected_active_content() {
    for target in [Target::New, Target::Old] {
        for mode in [
            "missing",
            "corrupt",
            "archive-hash",
            "content-hash",
            "content-count",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let mut m = fixture(root, false);
            m.target = target;
            if target == Target::New {
                fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
            }
            let path = archive(root, &m);
            match mode {
                "missing" => fs::remove_file(&path).unwrap(),
                "corrupt" => {
                    fs::write(&path, b"corrupt ZIP").unwrap();
                    m.protection_sha256 = backup::digest_file(&path).unwrap();
                }
                "archive-hash" => m.protection_sha256 = "0".repeat(64),
                "content-hash" => m.old_content.0.sha256 = "0".repeat(64),
                _ => m.old_content.0.entry_count += 1,
            }
            save(root, &m);
            let observed = observe_unchanged(root).unwrap();
            assert_eq!(
                observed.protection,
                match mode {
                    "missing" => Protection::Missing,
                    "archive-hash" => Protection::WrongArchiveHash,
                    _ => Protection::InvalidOrWrongContent,
                }
            );
            assert!(!observed.previous_available);
            assert_eq!(observed.next_check, NextCheck::Blocked);
            assert_eq!(observed.target, target);
        }
    }
}
#[test]
fn candidates_are_checked_against_their_own_content_roles_including_counts() {
    for target in [Target::New, Target::Old] {
        for role in ["new", "previous"] {
            for mode in ["missing", "corrupt", "wrong"] {
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path();
                let mut m = fixture(root, false);
                m.target = target;
                fs::copy(root.join("old.sqlite"), previous(root, &m)).unwrap();
                let path = if role == "new" {
                    next(root, &m)
                } else {
                    previous(root, &m)
                };
                match mode {
                    "missing" => fs::remove_file(&path).unwrap(),
                    "corrupt" => fs::write(&path, b"bad").unwrap(),
                    _ => {
                        fs::copy(
                            root.join(if role == "new" {
                                "old.sqlite"
                            } else {
                                "new.sqlite"
                            }),
                            &path,
                        )
                        .unwrap();
                    }
                }
                save(root, &m);
                let observed = observe_unchanged(root).unwrap();
                assert_eq!(
                    if role == "new" {
                        observed.new_candidate
                    } else {
                        observed.previous_candidate
                    },
                    match mode {
                        "missing" => Candidate::Missing,
                        "corrupt" => Candidate::Invalid,
                        _ => Candidate::WrongProof,
                    }
                );
                if mode != "missing" {
                    assert_eq!(observed.next_check, NextCheck::Blocked);
                }
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut m = fixture(root, false);
    m.new_content.0.entry_count += 1;
    save(root, &m);
    assert_eq!(
        observe_unchanged(root).unwrap().new_candidate,
        Candidate::WrongProof
    );
}
#[test]
fn standalone_headers_and_sql_storage_are_validated_not_just_file_presence() {
    for sql in [
        "CREATE TABLE extra(x)",
        "UPDATE journal_entries SET title=CAST('blob' AS BLOB)",
        "UPDATE journal_entries SET content_json='broken'",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = fixture(root, false);
        let db = Connection::open(next(root, &m)).unwrap();
        db.execute_batch(sql).unwrap();
        db.close().unwrap();
        let observed = observe_unchanged(root).unwrap();
        assert_eq!(observed.new_candidate, Candidate::Invalid);
        assert_eq!(observed.next_check, NextCheck::Blocked);
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let m = fixture(root, false);
    let mut bytes = fs::read(next(root, &m)).unwrap();
    bytes[18] = 2;
    bytes[19] = 2;
    fs::write(next(root, &m), bytes).unwrap();
    assert_eq!(
        observe_unchanged(root).unwrap().new_candidate,
        Candidate::Invalid
    );
}
#[test]
fn sidecars_at_every_database_location_block_without_recovery_or_cleanup() {
    for role in ["active", "new", "previous"] {
        for suffix in ["-journal", "-wal", "-shm", "-WaL"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let m = fixture(root, false);
            let path = match role {
                "active" => root.join("data/mno.sqlite"),
                "new" => next(root, &m),
                _ => previous(root, &m),
            };
            fs::write(
                PathBuf::from(format!("{}{suffix}", path.display())),
                b"blocking evidence",
            )
            .unwrap();
            assert_eq!(observe_unchanged(root), Err(Blocked::Sidecar));
        }
    }
}
#[test]
fn unexpected_files_links_and_legacy_pq_remain_blocked() {
    for mode in [
        "extra",
        "p",
        "q",
        "root-p",
        "alias-q",
        "candidate-directory",
        "marker-link",
        "candidate-link",
        "protection-link",
        "hardlink",
        "backup-directory-link",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = fixture(root, false);
        match mode {
            "extra" => fs::write(root.join("data/recovery/unexpected"), b"extra").unwrap(),
            "p" => fs::write(root.join("data/recovery/pending-restore.json"), b"P").unwrap(),
            "q" => fs::write(root.join("data/.mno-restore-completion-v1.json"), b"Q").unwrap(),
            "root-p" => fs::write(root.join("data/pending-restore.json"), b"P").unwrap(),
            "alias-q" => {
                fs::write(root.join("data/.MNO-RESTORE-COMPLETION-v1.json"), b"Q").unwrap()
            }
            "candidate-directory" => {
                fs::remove_file(next(root, &m)).unwrap();
                fs::create_dir(next(root, &m)).unwrap();
            }
            "hardlink" => {
                fs::remove_file(next(root, &m)).unwrap();
                fs::hard_link(root.join("data/mno.sqlite"), next(root, &m)).unwrap();
            }
            _ => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::symlink;
                    let path = match mode {
                        "marker-link" => root.join("data/recovery").join(activation::MARKER),
                        "candidate-link" => next(root, &m),
                        "protection-link" => archive(root, &m),
                        _ => root.join("data/backups/pre-restore"),
                    };
                    let outside = root.join("outside");
                    fs::rename(&path, &outside).unwrap();
                    symlink(&outside, &path).unwrap();
                }
            }
        }
        assert!(matches!(
            observe_unchanged(root),
            Err(Blocked::Namespace | Blocked::Legacy)
        ));
    }
}
#[test]
fn malformed_v1_unknown_and_inconsistent_markers_are_never_v2_observations() {
    for mode in [
        "missing",
        "bad",
        "v1",
        "unknown",
        "target-object",
        "inconsistent",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = fixture(root, false);
        let path = root.join("data/recovery").join(activation::MARKER);
        match mode {
            "missing" => fs::remove_file(&path).unwrap(),
            "bad" => fs::write(&path, b"{}").unwrap(),
            _ => {
                let mut value = serde_json::to_value(&m).unwrap();
                match mode {
                    "v1" => value["version"] = serde_json::json!(1),
                    "unknown" => value["version"] = serde_json::json!(3),
                    "target-object" => value["target"] = serde_json::json!({"new":null}),
                    _ => {
                        value["new_content"]["sha256"] = value["old_content"]["sha256"].clone();
                        value["new_content"]["entry_count"] = serde_json::json!(99);
                    }
                }
                fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        }
        assert_eq!(
            observe_unchanged(root),
            Err(if mode == "inconsistent" {
                Blocked::InconsistentProof
            } else {
                Blocked::Marker
            })
        );
    }
}
#[test]
fn invalid_owner_and_installation_never_start_sqlite_verification() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _m = fixture(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    let other = DatabaseState::default();
    assert!(other.initialize_legacy_startup(&root.join("data")).is_err());
    assert!(other.recovery_session().is_err());
    assert_eq!(tree(root), before); // The actual OS lease excludes a second owner.
    fs::rename(root.join("data"), root.join("moved-data")).unwrap();
    fs::create_dir(root.join("data")).unwrap();
    let before = tree(root);
    assert_eq!(classify_observed(&session, flag()), Err(Blocked::Ownership));
    assert_eq!(tree(root), before);

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _m = fixture(root, false);
    fs::write(
        root.join("data/.mno-initialized-v1.json"),
        b"bad installation",
    )
    .unwrap();
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    assert_eq!(
        classify_observed(&session, flag()),
        Err(Blocked::Installation)
    );
    assert_eq!(tree(root), before);
}
#[test]
fn running_owner_and_cancellation_cannot_supply_startup_classification() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&root.join("data")).unwrap();
    let maintenance = state.maintenance().unwrap();
    let session = maintenance.recovery_session().unwrap();
    let before = tree(root);
    assert_eq!(classify_observed(&session, flag()), Err(Blocked::Ownership));
    assert_eq!(tree(root), before);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fixture(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let cancelled = flag();
    cancelled.store(true, Ordering::Release);
    let before = tree(root);
    assert_eq!(
        classify_observed(&session, cancelled),
        Err(Blocked::Cancelled)
    );
    assert_eq!(tree(root), before);
}

#[test]
fn structurally_valid_v1_contracts_remain_v1_and_are_not_classified_as_v2() {
    for import in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = fixture(root, false);
        let mut value = serde_json::to_value(&m).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("target");
        object.remove("content_contract");
        object.remove("previous_candidate_file");
        value["version"] = serde_json::json!(1);
        value["format"] = serde_json::json!(if import {
            activation::IMPORT_FORMAT
        } else {
            "mno-database-activation"
        });
        value["old_content"] = serde_json::json!({"version":1,"sha256": if import {
            activation::importing::EMPTY_RESTORE_SHA256.to_owned()
        } else { m.old_content.0.sha256.clone() }});
        value["new_content"] = serde_json::json!({"version":1,"sha256":m.new_content.0.sha256});
        value["protection_file"] = serde_json::json!("pre-restore-original.mno-backup");
        fs::write(
            root.join("data/recovery").join(activation::MARKER),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(
            activation::read_marker(&root.join("data"))
                .unwrap()
                .is_import(),
            import
        );
        let state = owner(root);
        let session = state.recovery_session().unwrap();
        let before = tree(root);
        assert_eq!(classify_observed(&session, flag()), Err(Blocked::Marker));
        assert_eq!(tree(root), before);
        assert!(state.operation(None).is_err());
    }
}

#[test]
fn valid_classification_observes_archive_active_and_both_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let canonical = dir.path().canonicalize().unwrap();
    let root = canonical.as_path();
    let m = fixture(root, false);
    fs::copy(root.join("old.sqlite"), previous(root, &m)).unwrap();
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    let (result, attempts) = import_protection::observe_sqlite_opens(|| classify(&session, flag()));
    let observed = result.unwrap();
    assert_eq!(observed.active, Active::Old);
    assert_eq!(observed.protection, Protection::Verified);
    assert_eq!(observed.new_candidate, Candidate::Verified);
    assert_eq!(observed.previous_candidate, Candidate::Verified);
    assert_eq!(
        attempts.len(),
        4,
        "all four actual verification opens must be observed"
    );
    for path in [
        root.join("data/mno.sqlite"),
        next(root, &m),
        previous(root, &m),
    ] {
        assert_eq!(attempts.iter().filter(|actual| **actual == path).count(), 1);
    }
    assert_eq!(attempts[0].file_name().unwrap(), backup::DATABASE);
    assert_eq!(tree(root), before);
}

#[test]
fn sqlite_open_observation_is_scoped_nested_and_unwind_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.sqlite");
    let (_, outer) = import_protection::observe_sqlite_opens(|| {
        let failure = std::panic::catch_unwind(|| {
            import_protection::observe_sqlite_opens(|| {
                assert!(import_protection::open_readonly(&path).is_err());
                panic!("observer unwind");
            });
        });
        assert!(failure.is_err());
        assert!(import_protection::open_readonly(&path).is_err());
    });
    assert_eq!(outer, vec![path.clone()]);
    let (_, fresh) = import_protection::observe_sqlite_opens(|| {});
    assert!(fresh.is_empty());
    assert!(!path.exists());
}
