use super::*;
use crate::{database_state::DatabaseState, storage_content};
use std::sync::atomic::AtomicBool;
fn version(path: &Path) -> u32 {
    let db = database::open_existing(path).unwrap();
    let v = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    db.close().unwrap();
    v
}
fn schema12_copies(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join("backups/bird-schema-upgrade"))
        .unwrap()
        .map(|e| e.unwrap().path().join("schema12.sqlite"))
        .filter(|p| p.exists())
        .collect()
}
#[test]
fn ordinary_schema13_startup_preserves_rich_schema12_and_reopens_without_repeating_upgrade() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("journal");
    assert!(!root.exists());
    let (state, path) = crate::ebird_persistence::tests::fixture_at_schema(&root, 12);
    let mut db = database::open_existing(&path).unwrap();
    let old = storage_content::verify_v13(&mut db, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    drop(state);
    let bytes = fs::read(&path).unwrap();
    let ready = DatabaseState::default();
    assert_eq!(ready.initialize_startup(&root).unwrap(), path);
    assert_eq!(version(&path), 13);
    let mut db = database::open_existing(&path).unwrap();
    storage_content::verify_v14(&mut db, &AtomicBool::new(false)).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        storage_content::verified_snapshot_v13(&tx, &AtomicBool::new(false)).unwrap(),
        old
    );
    tx.commit().unwrap();
    db.close().unwrap();
    let copies = schema12_copies(&root);
    assert_eq!(copies.len(), 1);
    assert_eq!(fs::read(&copies[0]).unwrap(), bytes);
    drop(ready);
    let after = fs::read(&path).unwrap();
    let reopened = DatabaseState::default();
    reopened.initialize_startup(&root).unwrap();
    assert_eq!(fs::read(&path).unwrap(), after);
    assert_eq!(schema12_copies(&root), copies);
}
#[test]
fn ordinary_fresh_startup_reaches13_without_module_activation() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("absent-journal");
    assert!(!root.exists());
    let state = DatabaseState::default();
    let path = state.initialize_startup(&root).unwrap();
    assert_eq!(version(&path), 13);
    assert!(!crate::bird_module::test_module(false).enabled());
    let db = database::open_existing(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM ebird_visits", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
    db.close().unwrap();
    drop(state);
    let bytes = fs::read(&path).unwrap();
    DatabaseState::default().initialize_startup(&root).unwrap();
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn ordinary_schema13_upgrade_interruption_retries_without_opening_during_recovery() {
    const CHILD: &str = "MNO_SCHEMA13_UPGRADE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        for boundary in ["before-publish", "after-publish"] {
            let temp = tempfile::Builder::new()
                .prefix("mno-schema13-upgrade-")
                .tempdir()
                .unwrap();
            let root = temp.path().join("journal");
            let (state, path) = crate::ebird_persistence::tests::fixture_at_schema(&root, 12);
            drop(state);
            let before = fs::read(&path).unwrap();
            let token = uuid::Uuid::new_v4().to_string();
            fs::write(temp.path().join("synthetic-token"), &token).unwrap();
            let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","startup::ebird_tests::ordinary_schema13_upgrade_interruption_retries_without_opening_during_recovery","--nocapture"]).env(CHILD,temp.path()).env("MNO_SCHEMA13_TOKEN",token).env("MNO_BIRD_UPGRADE_CRASH",boundary).output().unwrap();
            assert_eq!(output.status.code(), Some(77), "{output:?}");
            if boundary == "before-publish" {
                assert_eq!(fs::read(&path).unwrap(), before);
            }
            let state = DatabaseState::default();
            state.initialize_startup(&root).unwrap();
            assert_eq!(version(&path), 13);
            assert!(
                schema12_copies(&root)
                    .iter()
                    .any(|p| fs::read(p).unwrap() == before)
            );
        }
        return;
    }
    let raw = PathBuf::from(std::env::var_os(CHILD).unwrap());
    assert!(!fs::symlink_metadata(&raw).unwrap().file_type().is_symlink());
    let temp = fs::canonicalize(raw).unwrap();
    assert_eq!(
        temp.parent().unwrap(),
        fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    assert!(
        temp.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mno-schema13-upgrade-")
    );
    assert_eq!(
        fs::read_to_string(temp.join("synthetic-token")).unwrap(),
        std::env::var("MNO_SCHEMA13_TOKEN").unwrap()
    );
    DatabaseState::default()
        .initialize_startup(&temp.join("journal"))
        .unwrap();
    panic!("expected injected interruption");
}
#[test]
fn schema12_pending_recovery_blocks_before_upgrade() {
    for artifact in ["recovery", "pending-restore.json"] {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("journal");
        let (state, path) = crate::ebird_persistence::tests::fixture_at_schema(&root, 12);
        drop(state);
        let bytes = fs::read(&path).unwrap();
        if artifact == "recovery" {
            fs::create_dir(root.join(artifact)).unwrap();
        } else {
            fs::write(root.join(artifact), b"synthetic unfinished operation").unwrap();
        }
        let state = DatabaseState::default();
        assert!(state.initialize_startup(&root).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(schema12_copies(&root).is_empty());
    }
}
