//! Real episode, native target and distinct schema-0 source; subprocesses only.
use super::super::tests::event;
use super::*;
use crate::backup::restore_context::pending::write_preparation::tests::context;
use crate::backup::restore_context::{RestoreMaintenanceScope, pending::tests::Fixture};
use crate::backup::{self, BackupKind};
use std::{
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};

#[test]
fn copy_child() {
    let Some(root) = std::env::var_os("MNO_COPY_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_COPY_MODE").unwrap();
    let f = Fixture::new();
    assert!(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    fs::write(root.join("before-db"), fs::read(&f.path).unwrap()).unwrap();
    let source_path = root.join("source.sqlite");
    let source = Connection::open(&source_path).unwrap();
    crate::database::initialize_schema(&source).unwrap();
    let document = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"å".repeat(200_000)}]}]}).to_string();
    source.execute("INSERT INTO journal_entries VALUES('new-entry','2026-09-12','Ny',?1,1,'source-created','source-updated')", [&document]).unwrap();
    let archive = root.join("source.mno-backup");
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    let incoming =
        backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
    let scope = RestoreMaintenanceScope {
        maintenance: f.state.maintenance().unwrap(),
        terminal_policy: TerminalPolicy::new(),
    };
    let m = scope.maintenance();
    let mut ctx = context(&f, m);
    ctx.incoming = incoming;
    ctx.terminal_policy = Some(&scope.terminal_policy);
    let episode = ctx
        .publish_write_preparation()
        .unwrap()
        .prepare_write(m.recovery_session().unwrap())
        .unwrap()
        .close_active()
        .unwrap()
        .into_epoch_inputs()
        .prepare_episode()
        .unwrap();
    let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
    let protection = parts
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .path()
        .to_owned();
    fs::write(
        root.join("protection-path"),
        protection.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    fs::write(
        root.join("protection-before"),
        fs::read(&protection).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("p-before"),
        &parts.pending.as_ref().unwrap().bytes,
    )
    .unwrap();
    let session_id = parts.session.as_ref().unwrap().session_id();
    let b = parts
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .retained_test_identity()
        .unwrap();
    let c = parts
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .retained_test_identity()
        .unwrap();
    crate::panic_policy::install();
    let cancel = Arc::new(AtomicBool::new(false));
    let episode = episode.open_writer(&cancel).unwrap();
    if mode == "full" {
        let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
        let WriterState::Owned {
            connection: Some(db),
            ..
        } = &parts.writer
        else {
            unreachable!()
        };
        let pages: i64 = db
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        db.pragma_update(None, "max_page_count", pages).unwrap();
    }
    if ["cancel-mid", "crash-mid"].contains(&mode.as_str()) {
        let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
        let WriterState::Owned {
            connection: Some(db),
            ..
        } = &parts.writer
        else {
            unreachable!()
        };
        db.execute_batch("PRAGMA cache_size=2; PRAGMA cache_spill=ON;")
            .unwrap();
    }
    let original_writer_handle = {
        let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
        let WriterState::Owned {
            connection: Some(db),
            ..
        } = &parts.writer
        else {
            unreachable!()
        };
        unsafe { db.handle() as usize }
    };
    event(&root, "opened");
    // Separate process holds the real native exclusive lock, not a fake status.
    let mut locker = if mode == "busy" {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["opening_tests::locker_child", "--nocapture"])
            .env("MNO_OPEN_LOCK", &f.path)
            .env("MNO_OPEN_LOCK_ROOT", &root)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("locked").exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        Some(child)
    } else {
        None
    };
    if mode == "cancel-before" {
        cancel.store(true, Ordering::Release);
    }
    // Real SQL IOERR via an authorizer is not claimed: finish error below is
    // explicitly a post-real-finish injected status, not physical disk failure.
    let hook_mode = mode.clone();
    let hook_cancel = cancel.clone();
    let hook_root = root.clone();
    let mut steps = 0;
    let result = probe::with(
        move |point| {
            if point == Point::BeforeStep && hook_mode == "crash-before" {
                event(&hook_root, "crash-before");
                std::process::exit(88);
            }
            if point == Point::AfterStep {
                steps += 1;
            }
            if point == Point::AfterStep && steps == 32 {
                if hook_mode == "cancel-mid" {
                    hook_cancel.store(true, Ordering::Release);
                }
                if hook_mode == "crash-mid" {
                    let data =
                        PathBuf::from(fs::read_to_string(hook_root.join("data-root")).unwrap());
                    let journal = fs::read(data.join("mno.sqlite-journal")).unwrap();
                    assert!(journal.len() > 512 && journal[..8].iter().any(|b| *b != 0));
                    event(&hook_root, "crash-mid");
                    std::process::exit(89);
                }
            }
            if point == Point::Done {
                if hook_mode == "crash-done" {
                    event(&hook_root, "crash-done");
                    std::process::exit(90);
                }
                if hook_mode == "cancel-late" {
                    hook_cancel.store(true, Ordering::Release);
                }
            }
        },
        mode == "finish-error",
        match mode.as_str() {
            "ioerr" => rusqlite::ffi::SQLITE_IOERR,
            "incomplete" => rusqlite::ffi::SQLITE_DONE,
            _ => 0,
        },
        || episode.copy_candidate(&cancel),
    );
    let (mut episode, result) = match result {
        Ok(episode) => (episode, Ok(())),
        Err(failure) => {
            let ActiveCloseFailure { owner, error } = *failure;
            (
                PreparedWriteEpisode {
                    inputs: WriteEpochInputs {
                        inputs: WriteInputs { owner },
                    },
                },
                Err(error),
            )
        }
    };
    if let Some(ref mut child) = locker {
        fs::write(root.join("release"), b"go").unwrap();
        assert!(child.wait().unwrap().success());
    }
    let failed = matches!(
        mode.as_str(),
        "busy" | "cancel-before" | "cancel-mid" | "finish-error" | "ioerr" | "incomplete" | "full"
    );
    if failed {
        match mode.as_str() {
            "busy" => assert!(
                matches!(result, Err(ActiveCloseError::WriterOpenSqlite(rusqlite::Error::SqliteFailure(e,_))) if e.code == rusqlite::ErrorCode::DatabaseBusy)
                    || matches!(result, Err(ActiveCloseError::CopyStep(rc)) if rc & 255 == rusqlite::ffi::SQLITE_BUSY)
            ),
            "ioerr" => assert!(matches!(
                result,
                Err(ActiveCloseError::CopyStep(rusqlite::ffi::SQLITE_IOERR))
            )),
            "incomplete" => assert!(matches!(result, Err(ActiveCloseError::CopyVerification))),
            "full" => assert!(
                matches!(result, Err(ActiveCloseError::CopyStep(rc) | ActiveCloseError::CopyFinish(rc)) if rc & 255 == rusqlite::ffi::SQLITE_FULL)
            ),
            "finish-error" => assert!(matches!(
                result,
                Err(ActiveCloseError::CopyFinish(rusqlite::ffi::SQLITE_IOERR))
            )),
            _ => assert!(matches!(result, Err(ActiveCloseError::WriterCancelled))),
        }
        assert!(episode.inputs.inputs.owner.after.as_ref().unwrap().copy == CopyState::Failed);
        assert!(matches!(
            episode.close_writer_inner(),
            Err(ActiveCloseError::Binding)
        ));
    } else {
        result.unwrap();
        assert!(episode.inputs.inputs.owner.after.as_ref().unwrap().copy == CopyState::Verified);
    }
    // Never retry, even after a fully verified copy or cancellation before step.
    assert!(matches!(
        episode.copy_candidate_inner(&AtomicBool::new(false)),
        Err(ActiveCloseError::Binding)
    ));
    let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
    let WriterState::Owned {
        connection: Some(db),
        outcome: None,
        ..
    } = &parts.writer
    else {
        panic!("original writer retained")
    };
    assert_eq!(unsafe { db.handle() as usize }, original_writer_handle);
    assert!(db.is_autocommit());
    // Observe BEFORE any diagnostic SELECT could recover an incomplete journal.
    assert!(!f.path.with_file_name("mno.sqlite-journal").exists());
    if failed && mode != "finish-error" {
        use std::os::unix::fs::FileExt;
        let ActiveState::Closed(active) = &parts.active else {
            unreachable!()
        };
        let mut bytes = vec![0; active.file.metadata().unwrap().len() as usize];
        active.file.read_exact_at(&mut bytes, 0).unwrap();
        assert_eq!(bytes, fs::read(root.join("before-db")).unwrap());
    }
    let title: String = db
        .query_row("SELECT title FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        title,
        if failed && mode != "finish-error" {
            "Gammal"
        } else {
            "Ny"
        }
    );
    if title == "Ny" {
        let row: (String,String,String,i64,String,String) = db.query_row("SELECT id,entry_date,content_json,document_version,created_at,updated_at FROM journal_entries", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
        assert_eq!(
            row,
            (
                "new-entry".into(),
                "2026-09-12".into(),
                document,
                1,
                "source-created".into(),
                "source-updated".into()
            )
        );
    }
    assert_eq!(parts.session.as_ref().unwrap().session_id(), session_id);
    assert!(std::ptr::eq(parts.maintenance, m));
    assert_eq!(
        parts
            .sources
            .live()
            .unwrap()
            .incoming
            .as_ref()
            .unwrap()
            .retained_test_identity()
            .unwrap(),
        b
    );
    assert_eq!(
        parts
            .sources
            .live()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .candidate()
            .retained_test_identity()
            .unwrap(),
        c
    );
    // External native SQLite must still observe each original B/C read lock.
    // The unlocked source fixture is a sensitive control (24 versus BUSY 23).
    let lock_status = |path: &Path| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["writer::tests::lock_child", "--nocapture"])
            .env("MNO_WRITER_LOCK_PATH", path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .code()
    };
    assert_eq!(lock_status(&source_path), Some(24));
    assert_eq!(
        lock_status(
            &parts
                .sources
                .live()
                .unwrap()
                .incoming
                .as_ref()
                .unwrap()
                .database_path()
        ),
        Some(23)
    );
    assert_eq!(
        lock_status(
            &parts
                .sources
                .live()
                .unwrap()
                .protection
                .as_ref()
                .unwrap()
                .candidate()
                .database_path()
        ),
        Some(23)
    );
    assert!(m.recovery_session().is_err());
    assert_eq!(
        fs::read(&protection).unwrap(),
        fs::read(root.join("protection-before")).unwrap()
    );
    assert_eq!(
        fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(),
        fs::read(root.join("p-before")).unwrap()
    );
    assert!(!f.path.with_file_name("mno.sqlite-journal").exists());
    event(&root, "checked");
    if !failed {
        episode = episode.close_writer().unwrap();
        event(&root, "closed");
    }
    // Failures must not enter B/C close; a later abort cannot mask this check.
    if mode == "close-chain" {
        episode = episode.close_b().unwrap().close_c().unwrap();
        assert!(matches!(
            episode.inputs.inputs.owner.after.as_ref().unwrap().sources,
            SourcePhase::Closed { .. }
        ));
        event(&root, "abc-closed");
    }
    let owner = if failed {
        let e = episode.close_b().err().expect("B blocked");
        assert!(matches!(e.error(), ActiveCloseError::Binding));
        e.owner
    } else {
        episode.inputs.inputs.owner
    };
    event(&root, "dropping");
    if mode == "negative" {
        std::process::exit(77);
    } // same markers, absent terminal Drop
    drop(owner);
    std::process::exit(76);
}

fn terminal_outcome(status: &std::process::ExitStatus, events: &str, expected: &str) -> bool {
    status.signal() == Some(libc::SIGABRT) && events == expected
}
#[test]
fn backup_copy_subprocesses() {
    for mode in [
        "ok",
        "close-chain",
        "cancel-before",
        "cancel-mid",
        "cancel-late",
        "busy",
        "finish-error",
        "ioerr",
        "incomplete",
        "full",
        "crash-before",
        "crash-mid",
        "crash-done",
        "negative",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["copying::tests::copy_child", "--nocapture"])
            .env("MNO_COPY_ROOT", root.path())
            .env("MNO_COPY_MODE", mode)
            .env("TMPDIR", root.path())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(40);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("timeout {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let events = fs::read_to_string(root.path().join("events")).unwrap();
        let crash = match mode {
            "crash-before" => Some(88),
            "crash-mid" => Some(89),
            "crash-done" => Some(90),
            _ => None,
        };
        if let Some(code) = crash {
            assert_eq!(status.code(), Some(code), "{mode}");
            assert_eq!(events, format!("opened\n{mode}\n"));
        } else if mode == "negative" {
            assert_eq!(status.code(), Some(77));
            assert_eq!(events, "opened\nchecked\nclosed\ndropping\n");
            assert!(!terminal_outcome(
                &status,
                &events,
                "opened\nchecked\nclosed\ndropping\n"
            ));
        } else {
            assert!(
                terminal_outcome(
                    &status,
                    &events,
                    if mode == "close-chain" {
                        "opened\nchecked\nclosed\nabc-closed\ndropping\n"
                    } else if ["ok", "cancel-late"].contains(&mode) {
                        "opened\nchecked\nclosed\ndropping\n"
                    } else {
                        "opened\nchecked\ndropping\n"
                    }
                ),
                "{mode}: {status}: {events}"
            );
        }
        let data = PathBuf::from(fs::read_to_string(root.path().join("data-root")).unwrap());
        let p = data.join(DIRECTORY).join(NAME);
        assert_eq!(
            fs::read(&p).unwrap(),
            fs::read(root.path().join("p-before")).unwrap()
        );
        let protection =
            PathBuf::from(fs::read_to_string(root.path().join("protection-path")).unwrap());
        assert_eq!(
            fs::read(protection).unwrap(),
            fs::read(root.path().join("protection-before")).unwrap()
        );
        // Inspect actual startup BEFORE any SQLite connection can recover a journal.
        let before = fs::read(data.join("mno.sqlite")).unwrap();
        let journal = data.join("mno.sqlite-journal");
        let journal_before = fs::read(&journal).ok();
        let state = crate::database_state::DatabaseState::default();
        assert_eq!(
            state.initialize_legacy_startup(&data).unwrap_err(),
            "database_recovery_artifact_present",
            "P must block Ready: {mode}"
        );
        assert!(state.operation(None).is_err());
        assert!(state.maintenance().is_err());
        assert_eq!(fs::read(data.join("mno.sqlite")).unwrap(), before);
        assert_eq!(fs::read(&journal).ok(), journal_before);
        assert!(p.exists());
        // Explicit diagnostic opening AFTER blocked startup, isolated data only.
        let db =
            Connection::open_with_flags(data.join("mno.sqlite"), OpenFlags::SQLITE_OPEN_READ_WRITE)
                .unwrap();
        let title: String = db
            .query_row("SELECT title FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            title,
            if [
                "ok",
                "close-chain",
                "cancel-late",
                "finish-error",
                "crash-done",
                "negative"
            ]
            .contains(&mode)
            {
                "Ny"
            } else {
                "Gammal"
            },
            "{mode}"
        );
        db.close().unwrap();
    }
}
