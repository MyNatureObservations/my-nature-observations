use super::tests::{fixture, owner, save, tree};
use super::*;
use activation::startup::{self, Action};
use rusqlite::{Connection, OpenFlags, types::Value};

fn rows(path: &Path) -> Vec<Vec<Value>> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows = {
        let mut stmt = db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
        stmt.query_map([], |r| {
            (0..7)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<Value>>>()
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    db.close().unwrap();
    rows
}
#[test]
fn status_confirmation_is_fresh_operation_bound_and_never_changes_files() {
    let dir = tempfile::tempdir_in("/private/tmp").unwrap();
    let root = dir.path();
    let mut m = fixture(root, false);
    let state = owner(root);
    let before = tree(root);
    let key = startup::status(&state)
        .unwrap()
        .replacement
        .unwrap()
        .previous_confirmation
        .unwrap();
    assert_eq!(tree(root), before);
    for key in [None, Some("wrong")] {
        assert!(startup::run_confirmed(&state, Action::KeepPrevious, key).is_err());
        assert_eq!(tree(root), before);
        assert!(state.operation(None).is_err());
    }
    // A different VALID operation with identical journal content must require a
    // new confirmation; no missing candidate/archive can mask this assertion.
    let id = uuid::Uuid::new_v4().to_string();
    let candidate = format!("next-{id}.sqlite");
    let protection = format!("pre-import-{id}.mno-backup");
    fs::rename(
        root.join("data/recovery").join(&m.candidate_file),
        root.join("data/recovery").join(&candidate),
    )
    .unwrap();
    fs::rename(
        root.join("data/backups/pre-restore")
            .join(&m.protection_file),
        root.join("data/backups/pre-restore").join(&protection),
    )
    .unwrap();
    m.operation_id = id.clone();
    m.candidate_file = candidate;
    m.protection_file = protection;
    m.previous_candidate_file = format!("previous-{id}.sqlite");
    save(root, &m);
    let fresh_key = startup::status(&state)
        .unwrap()
        .replacement
        .unwrap()
        .previous_confirmation
        .unwrap();
    assert_ne!(key, fresh_key);
    let changed = tree(root);
    assert!(matches!(
        startup::run_confirmed(&state, Action::KeepPrevious, Some(&key)),
        Err(activation::Error::Binding)
    ));
    assert_eq!(tree(root), changed);
    assert!(state.operation(None).is_err());
}
#[test]
fn changed_active_journal_rejects_prior_confirmation_without_publishing_old() {
    let dir = tempfile::tempdir_in("/private/tmp").unwrap();
    let root = dir.path();
    fixture(root, false);
    let state = owner(root);
    let key = startup::status(&state)
        .unwrap()
        .replacement
        .unwrap()
        .previous_confirmation
        .unwrap();
    let db = Connection::open(root.join("data/mno.sqlite")).unwrap();
    db.execute(
        "UPDATE journal_entries SET title='changed after preview'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let changed = tree(root);
    assert!(startup::run_confirmed(&state, Action::KeepPrevious, Some(&key)).is_err());
    assert_eq!(tree(root), changed);
    assert_eq!(marker(&root.join("data")).unwrap().target, Target::New);
    assert!(state.operation(None).is_err());
}
#[test]
fn durable_old_has_no_reverse_action_and_completes_with_exact_previous_rows() {
    let dir = tempfile::tempdir_in("/private/tmp").unwrap();
    let root = dir.path();
    let mut m = fixture(root, false);
    fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    m.target = Target::Old;
    save(root, &m);
    let state = owner(root);
    let view = startup::status(&state).unwrap();
    assert_eq!(view.actions, vec![Action::Continue]);
    assert_eq!(view.replacement.unwrap().previous_confirmation, None);
    assert!(startup::run(&state, Action::FinishNew).is_err());
    startup::run(&state, Action::Continue).unwrap();
    assert!(state.operation(None).is_ok());
    assert_eq!(
        rows(&root.join("data/mno.sqlite")),
        rows(&root.join("old.sqlite"))
    );
    assert!(startup::status(&state).is_err());
    assert!(startup::run(&state, Action::KeepPrevious).is_err());
}

#[test]
fn actual_generation_change_invalidates_status_confirmation() {
    let dir = tempfile::tempdir_in("/private/tmp").unwrap();
    let root = dir.path();
    fixture(root, false);
    let state = owner(root);
    let key = startup::status(&state)
        .unwrap()
        .replacement
        .unwrap()
        .previous_confirmation
        .unwrap();
    state.set_generation_for_test(17);
    let before = tree(root);
    assert_ne!(
        startup::status(&state)
            .unwrap()
            .replacement
            .unwrap()
            .previous_confirmation
            .as_deref(),
        Some(key.as_str())
    );
    assert!(matches!(
        startup::run_confirmed(&state, Action::KeepPrevious, Some(&key)),
        Err(activation::Error::Binding)
    ));
    assert_eq!(tree(root), before);
    assert_eq!(marker(&root.join("data")).unwrap().target, Target::New);
    assert!(state.operation(None).is_err());
}

#[test]
fn confirmed_previous_after_new_installation_rebuilds_old_and_preserves_new() {
    let dir = tempfile::tempdir_in("/private/tmp").unwrap();
    let root = dir.path();
    let m = fixture(root, false);
    // Simulate the durable state after installing new but before retirement.
    fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    fs::remove_file(root.join("data/recovery").join(&m.candidate_file)).unwrap();
    let archive = root
        .join("data/backups/pre-restore")
        .join(&m.protection_file);
    let protected = fs::read(&archive).unwrap();
    let state = owner(root);
    let view = startup::status(&state).unwrap();
    assert!(view.actions.contains(&Action::KeepPrevious));
    let key = view.replacement.unwrap().previous_confirmation.unwrap();
    startup::run_confirmed(&state, Action::KeepPrevious, Some(&key)).unwrap();
    assert!(state.operation(None).is_ok());
    assert_eq!(
        rows(&root.join("data/mno.sqlite")),
        rows(&root.join("old.sqlite"))
    );
    assert_eq!(
        rows(
            &root
                .join("data/backups/activations")
                .join(&m.operation_id)
                .join(&m.candidate_file)
        ),
        rows(&root.join("new.sqlite"))
    );
    assert_eq!(fs::read(archive).unwrap(), protected);
    assert!(startup::run_confirmed(&state, Action::KeepPrevious, Some(&key)).is_err());
}
