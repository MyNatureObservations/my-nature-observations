use super::super::tests::{fixture, owner, save, tree};
use super::*;
use rusqlite::Connection;

fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
fn setup(root: &Path, same: bool) -> ImportV2 {
    let m = fixture(root, same);
    fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    m
}
#[test]
fn explicit_return_is_repeatable_read_only_for_old_new_and_identical_content() {
    for mode in ["old", "new", "same"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = setup(root, mode == "same");
        if mode == "old" {
            fs::copy(root.join("old.sqlite"), root.join("data/mno.sqlite")).unwrap();
        }
        let state = owner(root);
        let session = state.recovery_session().unwrap();
        let before = tree(root);
        let confirmation = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        assert_eq!(confirmation.binding.marker, m);
        assert_eq!(
            confirmation.binding.active,
            match mode {
                "old" => Active::Old,
                "same" => Active::Both,
                _ => Active::New,
            }
        );
        assert_eq!(
            validate(&session, Some(&confirmation), flag()).unwrap(),
            ValidatedDecision(&confirmation)
        );
        assert_eq!(
            validate(&session, Some(&confirmation), flag()).unwrap(),
            ValidatedDecision(&confirmation)
        );
        assert_eq!(tree(root), before);
        assert_eq!(marker(&root.join("data")).unwrap().target, Target::New);
        assert!(state.operation(None).is_err());
        assert!(matches!(
            activation::read_marker(&root.join("data")),
            Err(activation::Error::Binding)
        ));
    }
}
#[test]
fn missing_cancelled_and_new_choices_are_not_return_consent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    assert_eq!(
        validate(&session, None, flag()),
        Err(Error::MissingDecision)
    );
    for choice in [UserChoice::Cancel, UserChoice::KeepNew] {
        let c = preview(&session, flag()).unwrap().confirm(choice);
        assert_eq!(
            validate(&session, Some(&c), flag()),
            Err(Error::NotConfirmed)
        );
    }
    assert_eq!(tree(root), before);
}
#[test]
fn operation_both_proofs_and_owner_binding_are_all_checked() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    for field in [
        "operation",
        "old-hash",
        "new-hash",
        "old-count",
        "new-count",
        "session",
        "directory",
        "generation",
        "protection",
    ] {
        let mut c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        match field {
            "operation" => c.binding.marker.operation_id = uuid::Uuid::new_v4().to_string(),
            "old-hash" => c.binding.marker.old_content.0.sha256 = "0".repeat(64),
            "new-hash" => c.binding.marker.new_content.0.sha256 = "0".repeat(64),
            "old-count" => c.binding.marker.old_content.0.entry_count += 1,
            "new-count" => c.binding.marker.new_content.0.entry_count += 1,
            "session" => c.binding.session_id = uuid::Uuid::new_v4(),
            "directory" => c.binding.directory_identity.1 += 1,
            "generation" => c.binding.generation += 1,
            _ => c.binding.marker.protection_sha256 = "0".repeat(64),
        }
        assert_eq!(
            validate(&session, Some(&c), flag()),
            Err(Error::Stale),
            "{field}"
        );
    }
    assert_eq!(tree(root), before);
}
#[test]
fn changed_active_content_and_changed_verified_marker_invalidate_consent() {
    for rebind in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut m = setup(root, false);
        let state = owner(root);
        let session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        // Deliberate fixture mutation, not a decision operation. No SQLite owner
        // from preview remains alive, so this does not close a locked descriptor.
        let mut db = Connection::open(root.join("data/mno.sqlite")).unwrap();
        db.execute(
            "UPDATE journal_entries SET title='changed after preview'",
            [],
        )
        .unwrap();
        if rebind {
            let proof = open_import::verify_database(&mut db, &AtomicBool::new(false)).unwrap();
            m.new_content.0.sha256 = proof.sha256;
            m.new_content.0.entry_count = proof.entry_count;
        }
        db.close().unwrap();
        if rebind {
            fs::remove_file(root.join("data/recovery").join(&m.candidate_file)).unwrap();
            save(root, &m);
        }
        let before = tree(root);
        assert_eq!(
            validate(&session, Some(&c), flag()),
            Err(if rebind {
                Error::Stale
            } else {
                Error::UnsafeState
            })
        );
        assert_eq!(tree(root), before);
    }
}
#[test]
fn old_target_and_reverse_direction_are_never_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut m = setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let mut c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    m.target = Target::Old;
    save(root, &m);
    let before = tree(root);
    assert!(matches!(preview(&session, flag()), Err(Error::NotNew)));
    assert_eq!(validate(&session, Some(&c), flag()), Err(Error::NotNew));
    c.choice = UserChoice::KeepNew;
    assert_eq!(
        validate(&session, Some(&c), flag()),
        Err(Error::NotConfirmed)
    );
    assert_eq!(tree(root), before);
}
#[test]
fn malformed_or_missing_live_marker_never_uses_the_preview_instead() {
    for missing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        setup(root, false);
        let state = owner(root);
        let session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        let path = root.join("data/recovery").join(activation::MARKER);
        if missing {
            fs::remove_file(path).unwrap();
        } else {
            fs::write(path, b"{}").unwrap();
        }
        let before = tree(root);
        assert_eq!(
            validate(&session, Some(&c), flag()),
            Err(Error::Observation(Blocked::Marker))
        );
        assert_eq!(tree(root), before);
    }
}
#[test]
fn archived_episode_is_rejected_even_if_a_live_marker_reappears() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let m = setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    let archives = root.join("data/backups/activations");
    fs::create_dir(&archives).unwrap();
    fs::rename(root.join("data/recovery"), archives.join(&m.operation_id)).unwrap();
    let before = tree(root);
    assert_eq!(
        validate(&session, Some(&c), flag()),
        Err(Error::Observation(Blocked::Namespace))
    );
    assert_eq!(tree(root), before);
    fs::create_dir(root.join("data/recovery")).unwrap();
    save(root, &m);
    let before = tree(root);
    assert_eq!(
        validate(&session, Some(&c), flag()),
        Err(Error::ClosedEpisode)
    );
    assert!(matches!(
        preview(&session, flag()),
        Err(Error::ClosedEpisode)
    ));
    assert_eq!(tree(root), before);
}
#[test]
fn new_session_and_different_valid_operation_cannot_reuse_consent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut m = setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    drop(session);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    assert_eq!(validate(&session, Some(&c), flag()), Err(Error::Stale));
    assert_eq!(tree(root), before);
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    let new_id = uuid::Uuid::new_v4().to_string();
    let old_archive = root
        .join("data/backups/pre-restore")
        .join(&m.protection_file);
    let old_candidate = root.join("data/recovery").join(&m.candidate_file);
    m.operation_id = new_id.clone();
    m.protection_file = format!("pre-import-{new_id}.mno-backup");
    m.candidate_file = format!("next-{new_id}.sqlite");
    m.previous_candidate_file = format!("previous-{new_id}.sqlite");
    fs::rename(
        old_archive,
        root.join("data/backups/pre-restore")
            .join(&m.protection_file),
    )
    .unwrap();
    fs::rename(
        old_candidate,
        root.join("data/recovery").join(&m.candidate_file),
    )
    .unwrap();
    save(root, &m);
    let before = tree(root);
    assert!(preview(&session, flag()).is_ok());
    assert_eq!(validate(&session, Some(&c), flag()), Err(Error::Stale));
    assert_eq!(tree(root), before);
}
#[test]
fn unsafe_active_backup_candidate_and_sidecars_reject_return() {
    for mode in [
        "missing-active",
        "bad-active",
        "missing-protection",
        "candidate",
        "sidecar",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let m = setup(root, false);
        let state = owner(root);
        let session = state.recovery_session().unwrap();
        let c = preview(&session, flag())
            .unwrap()
            .confirm(UserChoice::ReturnPrevious);
        match mode {
            "missing-active" => fs::remove_file(root.join("data/mno.sqlite")).unwrap(),
            "bad-active" => fs::write(root.join("data/mno.sqlite"), b"broken").unwrap(),
            "missing-protection" => fs::remove_file(
                root.join("data/backups/pre-restore")
                    .join(&m.protection_file),
            )
            .unwrap(),
            "candidate" => fs::write(
                root.join("data/recovery").join(&m.candidate_file),
                b"broken",
            )
            .unwrap(),
            _ => fs::write(root.join("data/mno.sqlite-journal"), b"blocked").unwrap(),
        }
        let before = tree(root);
        assert_eq!(
            validate(&session, Some(&c), flag()),
            Err(if mode == "sidecar" {
                Error::Observation(Blocked::Sidecar)
            } else {
                Error::UnsafeState
            })
        );
        assert_eq!(tree(root), before);
    }
}

#[test]
fn changing_between_verified_active_roles_requires_fresh_consent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    fs::copy(root.join("old.sqlite"), root.join("data/mno.sqlite")).unwrap();
    let before = tree(root);
    let fresh = preview(&session, flag()).unwrap();
    assert_eq!(fresh.0.active, Active::Old);
    assert_eq!(validate(&session, Some(&c), flag()), Err(Error::Stale));
    let c = fresh.confirm(UserChoice::ReturnPrevious);
    assert!(validate(&session, Some(&c), flag()).is_ok());
    assert_eq!(tree(root), before);
}

#[test]
fn actual_database_state_generation_change_invalidates_previous_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root, false);
    let state = owner(root);
    let session = state.recovery_session().unwrap();
    let before = tree(root);
    let initial_generation = session.startup_generation().unwrap();
    let c = preview(&session, flag())
        .unwrap()
        .confirm(UserChoice::ReturnPrevious);
    assert_eq!(c.binding.generation, initial_generation);
    assert_eq!(
        validate(&session, Some(&c), flag()).unwrap(),
        ValidatedDecision(&c)
    );

    let next_generation = initial_generation.checked_add(1).unwrap();
    state.set_generation_for_test(next_generation);
    assert_eq!(session.startup_generation().unwrap(), next_generation);
    // Leave the original confirmation intact: only the actual owner's generation
    // changed. Returning Ok here would prove the fresh read was bypassed.
    assert_eq!(validate(&session, Some(&c), flag()), Err(Error::Stale));

    let fresh = preview(&session, flag()).unwrap();
    assert_eq!(fresh.0.generation, next_generation);
    let fresh = fresh.confirm(UserChoice::ReturnPrevious);
    assert_eq!(
        validate(&session, Some(&fresh), flag()).unwrap(),
        ValidatedDecision(&fresh)
    );
    assert_eq!(tree(root), before);
    assert!(state.operation(None).is_err());
}
