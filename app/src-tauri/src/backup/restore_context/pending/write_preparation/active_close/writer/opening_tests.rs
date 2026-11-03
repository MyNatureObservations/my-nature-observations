//! Actual opener, real native SQLite, only isolated subprocess fixtures.
use super::tests::event;
use super::*;
use crate::backup::restore_context::pending::write_preparation::tests::context;
use crate::backup::restore_context::{RestoreMaintenanceScope, pending::tests::Fixture};
use std::{
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};

#[test]
fn hot_child() {
    let Some(path) = std::env::var_os("MNO_OPEN_HOT") else {
        return;
    };
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    db.execute_batch("PRAGMA cache_size=2; PRAGMA cache_spill=ON; PRAGMA synchronous=FULL; BEGIN IMMEDIATE; UPDATE journal_entries SET title=printf('%.*c',200000,'x')").unwrap();
    std::process::exit(88); // Real uncommitted spill, no Rust/SQLite cleanup.
}
#[test]
fn locker_child() {
    let Some(path) = std::env::var_os("MNO_OPEN_LOCK") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("MNO_OPEN_LOCK_ROOT").unwrap());
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    db.busy_timeout(Duration::ZERO).unwrap();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    fs::write(root.join("locked"), b"ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join("release").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    db.execute_batch("ROLLBACK").unwrap();
    db.close().unwrap();
}
#[test]
fn opening_child() {
    let Some(root) = std::env::var_os("MNO_OPEN_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_OPEN_MODE").unwrap();
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
    let scope = RestoreMaintenanceScope {
        maintenance: f.state.maintenance().unwrap(),
        terminal_policy: TerminalPolicy::new(),
    };
    let m = scope.maintenance();
    let mut ctx = context(&f, m);
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
    let original = episode.inputs.inputs.owner.after.as_ref().unwrap();
    let id = original.session.as_ref().unwrap().session_id();
    let b = original
        .sources
        .live()
        .unwrap()
        .incoming
        .as_ref()
        .unwrap()
        .retained_test_identity()
        .unwrap();
    let c = original
        .sources
        .live()
        .unwrap()
        .protection
        .as_ref()
        .unwrap()
        .candidate()
        .retained_test_identity()
        .unwrap();
    fs::write(
        root.join("before-p"),
        &original.pending.as_ref().unwrap().bytes,
    )
    .unwrap();
    let lease = f.state.test_startup_owner();
    let cancel = Arc::new(AtomicBool::new(mode == "cancel-before"));
    let mut locker = if mode == "locked" {
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
    crate::panic_policy::install();
    let hook_mode = mode.clone();
    let hook_path = f.path.clone();
    let hook_root = root.clone();
    let hook_cancel = cancel.clone();
    let result = opening_probe::with(
        move |point| {
            if point == OpeningPoint::Armed {
                assert!(!hook_root.join("anchor-started").exists());
                event(&hook_root, "armed");
                if hook_mode == "identity" || hook_mode == "missing" {
                    fs::rename(&hook_path, hook_root.join("saved-main")).unwrap();
                    if hook_mode == "identity" {
                        fs::copy(hook_root.join("before-db"), &hook_path).unwrap();
                    }
                }
                if hook_mode == "cancel-armed" {
                    hook_cancel.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            if point == OpeningPoint::Anchor {
                event(&hook_root, "anchor");
                if hook_mode == "cancel-anchor" {
                    hook_cancel.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            if point == OpeningPoint::BeforeSqlite {
                event(&hook_root, "before-sqlite");
                if hook_mode == "readonly-fallback" {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o400)).unwrap();
                }
                if hook_mode == "sqlite-error" {
                    fs::rename(&hook_path, hook_root.join("saved-main")).unwrap();
                }
                if hook_mode == "hot" || hook_mode == "hot-negative" {
                    let status = std::process::Command::new(std::env::current_exe().unwrap())
                        .args(["opening_tests::hot_child", "--nocapture"])
                        .env("MNO_OPEN_HOT", &hook_path)
                        .status()
                        .unwrap();
                    assert_eq!(status.code(), Some(88));
                    let journal = fs::read(hook_path.with_file_name("mno.sqlite-journal")).unwrap();
                    assert!(journal.len() > 512 && journal[..8] != [0; 8]);
                    fs::write(hook_root.join("hot-journal"), journal).unwrap();
                    fs::write(hook_root.join("hot-db"), fs::read(&hook_path).unwrap()).unwrap();
                }
            }
            if point == OpeningPoint::Sqlite {
                event(&hook_root, "sqlite");
                if hook_mode == "post-identity" {
                    fs::rename(&hook_path, hook_root.join("saved-main")).unwrap();
                    fs::copy(hook_root.join("before-db"), &hook_path).unwrap();
                }
                if hook_mode == "cancel-sqlite" {
                    hook_cancel.store(true, std::sync::atomic::Ordering::Release);
                }
            }
        },
        || episode.open_writer(&cancel),
    );
    let owner = match result {
        Ok(episode) => {
            assert!(mode == "ok" || mode == "locked" || mode == "busy-close");
            let parts = episode.inputs.inputs.owner.after.as_ref().unwrap();
            let WriterState::Owned {
                connection: Some(db),
                anchor,
                outcome: None,
            } = &parts.writer
            else {
                panic!("owned open")
            };
            let ActiveState::Closed(active) = &parts.active else {
                unreachable!()
            };
            assert_eq!(private::file_identity(anchor).unwrap(), active.identity);
            if mode == "locked" {
                // Opening is lazy, not proof of acquiring a SQLite read/write lock.
                let error = db
                    .query_row("SELECT count(*) FROM journal_entries", [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap_err();
                assert!(
                    matches!(error,rusqlite::Error::SqliteFailure(e,_) if e.code==rusqlite::ErrorCode::DatabaseBusy)
                );
                fs::write(root.join("release"), b"release").unwrap();
                assert!(locker.as_mut().unwrap().wait().unwrap().success());
                event(&root, "busy-observed");
            }
            let opened_handle = unsafe { db.handle() } as usize;
            if mode == "busy-close" {
                std::mem::forget(db.prepare("SELECT 1").unwrap());
            }
            let result = episode.close_writer();
            if mode == "busy-close" {
                let failure = result.err().expect("real busy close");
                assert!(matches!(failure.error(), ActiveCloseError::WriterClose));
                let WriterState::Owned {
                    outcome: Some(Err((db, error))),
                    ..
                } = &failure.owner.after.as_ref().unwrap().writer
                else {
                    panic!("retained BUSY")
                };
                assert!(
                    matches!(error,rusqlite::Error::SqliteFailure(e,_) if e.code==rusqlite::ErrorCode::DatabaseBusy)
                );
                assert_eq!(unsafe { db.handle() } as usize, opened_handle);
                event(&root, "close-busy");
                failure.owner
            } else {
                let episode = result.unwrap();
                assert!(matches!(
                    episode.inputs.inputs.owner.after.as_ref().unwrap().writer,
                    WriterState::Closed { .. }
                ));
                event(&root, "closed");
                episode.inputs.inputs.owner
            }
        }
        Err(failure) => {
            match mode.as_str() {
                "identity" | "post-identity" | "readonly-fallback" | "hot" | "hot-negative" => {
                    assert!(matches!(failure.error(), ActiveCloseError::WriterIdentity))
                }
                "missing" => assert!(
                    matches!(failure.error(),ActiveCloseError::WriterOpenIo(e) if e.kind()==io::ErrorKind::NotFound)
                ),
                "sqlite-error" => assert!(
                    matches!(failure.error(),ActiveCloseError::WriterOpenSqlite(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::CannotOpen)
                ),
                "cancel-before" | "cancel-armed" | "cancel-anchor" | "cancel-sqlite" => {
                    assert!(matches!(failure.error(), ActiveCloseError::WriterCancelled))
                }
                _ => panic!("unexpected open failure: {:?}", failure.error()),
            }
            if mode == "sqlite-error" || mode == "missing" {
                assert!(!f.path.exists(), "no CREATE");
            }
            if mode == "hot" || mode == "hot-negative" {
                // Observe through the already-held A descriptor; no new main fd.
                let parts = failure.owner.after.as_ref().unwrap();
                let ActiveState::Closed(active) = &parts.active else {
                    unreachable!()
                };
                use std::os::unix::fs::FileExt;
                let mut bytes = vec![0; active.file.metadata().unwrap().len() as usize];
                active.file.read_exact_at(&mut bytes, 0).unwrap();
                assert_eq!(bytes, fs::read(root.join("hot-db")).unwrap());
                assert_eq!(
                    fs::read(f.path.with_file_name("mno.sqlite-journal")).unwrap(),
                    fs::read(root.join("hot-journal")).unwrap()
                );
                if mode == "hot-negative" {
                    let WriterState::Owned {
                        connection: Some(db),
                        ..
                    } = &parts.writer
                    else {
                        panic!("real opened connection")
                    };
                    let title: String = db
                        .query_row("SELECT title FROM journal_entries", [], |r| r.get(0))
                        .unwrap();
                    assert_eq!(title, "Gammal");
                    assert!(!f.path.with_file_name("mno.sqlite-journal").exists());
                    event(&root, "recovery-detected");
                    std::process::exit(79); // Explicitly forbidden read, negative control ONLY.
                }
            }
            event(&root, "rejected");
            failure.owner
        }
    };
    let parts = owner.after.as_ref().unwrap();
    let live = parts.sources.live().unwrap();
    assert_eq!(
        live.incoming
            .as_ref()
            .unwrap()
            .retained_test_identity()
            .unwrap(),
        b
    );
    assert_eq!(
        live.protection
            .as_ref()
            .unwrap()
            .candidate()
            .retained_test_identity()
            .unwrap(),
        c
    );
    assert_eq!(parts.session.as_ref().unwrap().session_id(), id);
    assert!(std::ptr::eq(parts.maintenance, m));
    assert_eq!(f.state.test_startup_owner(), lease);
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    if mode == "cancel-before" {
        assert!(scope.terminal_policy.is_never_armed());
        event(&root, "unarmed-drop");
        drop(owner);
        std::process::exit(0);
    }
    assert!(scope.terminal_policy.is_armed());
    if mode != "ok" && mode != "locked" {
        let episode = PreparedWriteEpisode {
            inputs: WriteEpochInputs {
                inputs: WriteInputs { owner },
            },
        };
        let failure = episode
            .close_b()
            .err()
            .expect("unsafe/open-failed writer blocks B");
        assert!(matches!(failure.error(), ActiveCloseError::Binding));
        event(&root, "B-blocked");
        event(&root, "retained");
        drop(failure);
    } else {
        event(&root, "retained");
        drop(owner);
    }
    std::process::exit(76);
}
#[test]
fn native_opening_subprocesses() {
    for mode in [
        "ok",
        "identity",
        "missing",
        "sqlite-error",
        "post-identity",
        "readonly-fallback",
        "cancel-before",
        "cancel-armed",
        "cancel-anchor",
        "cancel-sqlite",
        "locked",
        "busy-close",
        "hot",
        "hot-negative",
    ] {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["opening_tests::opening_child", "--nocapture"])
            .env("MNO_OPEN_ROOT", root.path())
            .env("MNO_OPEN_MODE", mode)
            .env("TMPDIR", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            assert!(Instant::now() < deadline, "timeout {mode}");
            std::thread::sleep(Duration::from_millis(10));
        };
        let events = fs::read_to_string(root.path().join("events")).unwrap_or_default();
        let expected = match mode {
            "ok" => "armed\nanchor\nbefore-sqlite\nsqlite\nclosed\nretained\n",
            "locked" => "armed\nanchor\nbefore-sqlite\nsqlite\nbusy-observed\nclosed\nretained\n",
            "busy-close" => {
                "armed\nanchor\nbefore-sqlite\nsqlite\nclose-busy\nB-blocked\nretained\n"
            }
            "identity" | "missing" => "armed\nrejected\nB-blocked\nretained\n",
            "sqlite-error" => "armed\nanchor\nbefore-sqlite\nrejected\nB-blocked\nretained\n",
            "cancel-before" => "rejected\nunarmed-drop\n",
            "cancel-armed" => "armed\nrejected\nB-blocked\nretained\n",
            "cancel-anchor" => "armed\nanchor\nrejected\nB-blocked\nretained\n",
            "cancel-sqlite" | "post-identity" | "readonly-fallback" | "hot" => {
                "armed\nanchor\nbefore-sqlite\nsqlite\nrejected\nB-blocked\nretained\n"
            }
            "hot-negative" => "armed\nanchor\nbefore-sqlite\nsqlite\nrecovery-detected\n",
            _ => unreachable!(),
        };
        assert_eq!(events, expected, "{mode}: {status}");
        if mode == "cancel-before" {
            assert!(status.success());
        } else if mode == "hot-negative" {
            assert_eq!(status.code(), Some(79));
        } else {
            assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}");
        }
        let data = PathBuf::from(fs::read_to_string(root.path().join("data-root")).unwrap());
        let actual = if mode == "missing" || mode == "sqlite-error" {
            root.path().join("saved-main")
        } else {
            data.join("mno.sqlite")
        };
        let expected = if mode == "hot" { "hot-db" } else { "before-db" };
        assert_eq!(
            fs::read(actual).unwrap(),
            fs::read(root.path().join(expected)).unwrap(),
            "{mode}: database bytes"
        );
        for suffix in ["-wal", "-shm", "-journal"] {
            if mode != "hot" || suffix != "-journal" {
                assert!(
                    !data.join(format!("mno.sqlite{suffix}")).exists(),
                    "{mode}: unexpected sidecar {suffix}"
                );
            }
        }
        if mode == "hot" {
            assert_eq!(
                fs::read(data.join("mno.sqlite-journal")).unwrap(),
                fs::read(root.path().join("hot-journal")).unwrap()
            );
        }
        assert_eq!(
            fs::read(data.join(DIRECTORY).join(NAME)).unwrap(),
            fs::read(root.path().join("before-p")).unwrap()
        );
    }
}
