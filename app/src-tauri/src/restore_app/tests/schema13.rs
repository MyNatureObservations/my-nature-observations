use super::*;
#[test]
fn ordinary_restore_format14_prepares_activates_and_reopens() {
    const CHILD: &str = "MNO_SCHEMA13_RESTORE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::Builder::new()
            .prefix("mno-schema13-restore-")
            .tempdir()
            .unwrap();
        let token = uuid::Uuid::new_v4().to_string();
        fs::write(root.path().join("synthetic-token"), &token).unwrap();
        let output=Command::new(std::env::current_exe().unwrap()).args(["--exact","restore_app::tests::schema13::ordinary_restore_format14_prepares_activates_and_reopens","--nocapture"]).env(CHILD,"yes").env("MNO_ACTIVATION_ROOT",root.path()).env("MNO_ACTIVATION_MODE","").env("MNO_SCHEMA13_TOKEN",token).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }
    let raw = PathBuf::from(std::env::var_os("MNO_ACTIVATION_ROOT").unwrap());
    assert!(!fs::symlink_metadata(&raw).unwrap().file_type().is_symlink());
    let root = fs::canonicalize(raw).unwrap();
    assert_eq!(
        root.parent().unwrap(),
        fs::canonicalize(std::env::temp_dir()).unwrap()
    );
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mno-schema13-restore-")
    );
    assert_eq!(
        fs::read_to_string(root.join("synthetic-token")).unwrap(),
        std::env::var("MNO_SCHEMA13_TOKEN").unwrap()
    );
    let source = DatabaseState::default();
    let path = source
        .initialize_schema13_startup(&root.join("source"))
        .unwrap();
    let mut db = crate::database::open_existing(&path).unwrap();
    db.execute("INSERT INTO journal_entries VALUES('synthetic','2026-11-21','Schema13 Restore','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[]).unwrap();
    let archive = root.join("format14.mno-backup");
    backup::create_backup(&db, &archive, BackupKind::Manual).unwrap();
    let expected = crate::storage_content::verify_v14(&mut db, &AtomicBool::new(false)).unwrap();
    db.close().unwrap();
    drop(source);
    let state = DatabaseState::default();
    let active = state
        .initialize_schema12_startup(&root.join("data"))
        .unwrap();
    assert_eq!(
        Connection::open(&active)
            .unwrap()
            .pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        12
    );
    let choice = validate_selection(&state, archive).unwrap();
    activate_selection(&state, choice).unwrap();
    assert!(state.operation(None).is_err());
    drop(state);
    let recovery = DatabaseState::default();
    assert!(recovery.initialize_startup(&root.join("data")).is_err());
    assert!(
        startup::status(&recovery)
            .unwrap()
            .actions
            .contains(&startup::Action::FinishNew)
    );
    startup::run(&recovery, startup::Action::FinishNew).unwrap();
    drop(recovery);
    let ready = DatabaseState::default();
    ready.initialize_startup(&root.join("data")).unwrap();
    let mut db = crate::database::open_existing(&active).unwrap();
    assert_eq!(
        crate::storage_content::verify_v14(&mut db, &AtomicBool::new(false)).unwrap(),
        expected
    );
    db.close().unwrap();
}
#[test]
fn ordinary_restore_rejects_future_and_structurally_invalid_backups() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let root = tempfile::TempDir::new().unwrap();
    let source = DatabaseState::default();
    let path = source
        .initialize_schema13_startup(&root.path().join("source"))
        .unwrap();
    let db = crate::database::open_existing(&path).unwrap();
    let archive = root.path().join("valid.mno-backup");
    backup::create_backup(&db, &archive, BackupKind::Manual).unwrap();
    db.close().unwrap();
    let state = DatabaseState::default();
    let active = state
        .initialize_startup(&root.path().join("target"))
        .unwrap();
    let before = fs::read(&active).unwrap();
    for mode in ["future", "structural"] {
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
        let mut manifest = String::new();
        zip.by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        let mut bytes = Vec::new();
        zip.by_name("journal.sqlite")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        drop(zip);
        if mode == "future" {
            manifest["format_version"] = 15.into();
            manifest["database_schema_version"] = 14.into();
        } else {
            let bad_db = root.path().join("invalid.sqlite");
            fs::write(&bad_db, &bytes).unwrap();
            let db = Connection::open(&bad_db).unwrap();
            db.execute_batch("CREATE TABLE unexpected(x)").unwrap();
            db.close().unwrap();
            bytes = fs::read(&bad_db).unwrap();
            manifest["database_sha256"] = format!("{:x}", Sha256::digest(&bytes)).into();
        }
        let bad = root.path().join(format!("{mode}.mno-backup"));
        let mut writer = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        writer.start_file("journal.sqlite", options).unwrap();
        writer.write_all(&bytes).unwrap();
        writer.finish().unwrap();
        assert!(matches!(
            validate_selection(&state, bad),
            Err(Error::InvalidBackup)
        ));
        assert_eq!(fs::read(&active).unwrap(), before);
        assert!(state.operation(None).is_ok());
    }
}
