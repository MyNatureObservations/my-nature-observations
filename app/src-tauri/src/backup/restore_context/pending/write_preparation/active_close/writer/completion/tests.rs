use super::*;
use crate::backup::pre_restore::durability;
use crate::backup::restore_context::pending::tests::Fixture;
use crate::backup::restore_context::pending::write_preparation::tests::context;
use crate::backup::{self, BackupKind};
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    time::{Duration, Instant},
};
fn event(root: &Path, name: &str) {
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{name}"
    )
    .unwrap();
}
#[test]
fn completion_child() {
    let Some(root) = std::env::var_os("MNO_COMPLETION_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_COMPLETION_MODE").unwrap();
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
    let source = Connection::open(root.join("source.sqlite")).unwrap();
    crate::database::initialize_schema(&source).unwrap();
    source.execute("INSERT INTO journal_entries VALUES('new-entry','2026-10-01','Ny',?1,1,'created','updated')", [r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Nytt innehåll åäö"}]}]}"#]).unwrap();
    let archive = root.join("incoming.mno-backup");
    backup::create_backup(&source, &archive, BackupKind::Manual).unwrap();
    source.close().unwrap();
    let incoming =
        backup::restore::prepare_candidate(&archive, Arc::new(AtomicBool::new(false))).unwrap();
    let hook_mode = mode.clone();
    let hook_root = root.clone();
    let sync_mode = mode.clone();
    let sync_root = root.clone();
    classification::result::receipt::with_episode_probe(
        move |name| {
            event(&hook_root, name);
            if hook_mode == format!("crash:{name}") {
                std::process::exit(88);
            }
            if hook_mode == format!("fail:{name}") {
                return Err(io::Error::other("injected checkpoint failure"));
            }
            Ok(())
        },
        move |name, point, file| {
            event(&sync_root, &format!("sync:{name}"));
            if sync_mode == format!("sync:{name}") {
                return Err(io::Error::other("injected sync failure"));
            }
            durability::sync_opened(point, file)
        },
        || {
            let mut scope = RestoreMaintenanceScope {
                maintenance: f.state.maintenance().unwrap(),
                terminal_policy: TerminalPolicy::new(),
            };
            let m = scope.maintenance();
            let mut ctx = context(&f, m);
            if mode == "same" {
                drop(incoming);
            } else {
                ctx.incoming = incoming;
            }
            ctx.terminal_policy = Some(&scope.terminal_policy);
            let cancel = Arc::new(AtomicBool::new(false));
            let mut ep = ctx
                .publish_write_preparation()
                .unwrap()
                .prepare_write(m.recovery_session().unwrap())
                .unwrap()
                .close_active()
                .unwrap()
                .into_epoch_inputs()
                .prepare_episode()
                .unwrap();
            let protection = ep
                .inputs
                .inputs
                .owner
                .after
                .as_ref()
                .unwrap()
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
            crate::panic_policy::install();
            ep = ep.open_writer(&cancel).unwrap();
            if mode != "reject-uncopied" {
                ep = ep.copy_candidate(&cancel).unwrap();
            }
            if mode == "writer-busy" {
                let parts = ep.inputs.inputs.owner.after.as_ref().unwrap();
                let WriterState::Owned {
                    connection: Some(db),
                    ..
                } = &parts.writer
                else {
                    unreachable!()
                };
                std::mem::forget(db.prepare("SELECT 1").unwrap());
                let failed = ep.close_writer().err().expect("real BUSY close");
                assert!(matches!(failed.error(), ActiveCloseError::WriterClose));
                event(&root, "writer-busy-retained");
                drop(failed);
                std::process::exit(76);
            }
            if mode != "reject-writer-live" {
                ep = ep.close_writer().unwrap();
            }
            if !["reject-writer-live", "reject-sources-live"].contains(&mode.as_str()) {
                ep = ep.close_b().unwrap().close_c().unwrap();
            }
            event(&root, "prepared");
            if mode == "cancel-proof" {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
            let completed = ep.complete(cancel);
            event(&root, "detached");
            assert!(scope.maintenance.ensure_usable().is_err());
            assert_eq!(scope.maintenance.generation(), 1);
            if mode == "abandon-handoff" {
                drop(completed);
                std::process::exit(76);
            }
            if mode == "negative" {
                std::process::exit(77);
            }
            if mode == "wrong-scope" {
                let other = Fixture::new();
                let mut other_scope = RestoreMaintenanceScope {
                    maintenance: other.state.maintenance().unwrap(),
                    terminal_policy: TerminalPolicy::new(),
                };
                event(&root, "wrong-scope");
                other_scope.finish_episode(completed);
                std::process::exit(76);
            }
            scope.finish_episode(completed);
            assert!(scope.terminal_policy.phase.get() == TerminalPhase::Finished as u8);
            assert!(scope.maintenance.ensure_usable().is_ok());
            assert_eq!(scope.maintenance.generation(), 2);
            event(&root, "ready");
            drop(scope); // Outer terminal policy must also be Finished, not just local Drop.
            assert!(f.state.operation(Some(1)).is_err());
            drop(f.state.operation(Some(2)).unwrap());
            assert_eq!(
                fs::read(&protection).unwrap(),
                fs::read(root.join("protection-before")).unwrap()
            );
            event(&root, "normal-return");
        },
    );
    drop(f.state); // Release the real DirectoryLease before parent startup.
    std::process::exit(0); // Preserve fixture on disk, not its TempDir cleanup.
}
#[test]
fn complete_restore_subprocesses() {
    let mut modes = vec![
        "ok".to_string(),
        "same".into(),
        "reject-uncopied".into(),
        "reject-writer-live".into(),
        "reject-sources-live".into(),
        "writer-busy".into(),
        "cancel-proof".into(),
        "abandon-handoff".into(),
        "negative".into(),
        "wrong-scope".into(),
    ];
    for name in [
        "episode-before-transfer",
        "episode-after-transfer",
        "episode-before-proof",
        "before-temporary",
        "partial-write",
        "published",
        "cleanup-before-p-unlink",
        "cleanup-p-unlinked",
        "cleanup-after-root-sync",
        "q-before-unlink",
        "q-after-unlink",
        "disk-after-final-barriers",
        "handoff-unwrapped",
        "handoff-context",
        "handoff-files",
        "handoff-detached",
        "episode-handoff-complete",
        "episode-before-admission",
        "admission-after-open",
        "admission-before-ready",
        "episode-finished",
    ] {
        modes.push(format!("fail:{name}"));
        modes.push(format!("crash:{name}"));
    }
    for name in [
        "journal-file",
        "journal-directory",
        "receipt-file",
        "receipt-directory",
        "receipt-final",
        "cleanup-p-directory",
        "cleanup-root",
        "final-journal-file",
        "disk-root-directory",
        "disk-journal-final",
    ] {
        modes.push(format!("sync:{name}"));
    }
    for mode in modes {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["completion::tests::completion_child", "--nocapture"])
            .env("MNO_COMPLETION_ROOT", root.path())
            .env("MNO_COMPLETION_MODE", &mode)
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
        let happy = mode == "ok" || mode == "same";
        if happy {
            assert!(status.success(), "{mode}: {status}: {events}");
            assert!(events.ends_with("ready\nnormal-return\n"));
        } else {
            assert!(
                !events
                    .lines()
                    .any(|line| line == "ready" || line == "normal-return"),
                "{mode}: premature success"
            );
            if mode.starts_with("crash:") {
                assert_eq!(status.code(), Some(88), "{mode}: {events}");
            } else if mode == "negative" {
                assert_eq!(status.code(), Some(77));
                assert!(!status.success());
            } else {
                assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {events}");
            }
            let expected = mode
                .strip_prefix("crash:")
                .or_else(|| mode.strip_prefix("fail:"))
                .map(str::to_owned)
                .or_else(|| mode.starts_with("sync:").then(|| mode.clone()));
            if let Some(expected) = expected {
                assert_eq!(events.lines().last(), Some(expected.as_str()), "{mode}");
            } else {
                assert_eq!(
                    events.lines().last(),
                    Some(match mode.as_str() {
                        "writer-busy" => "writer-busy-retained",
                        "cancel-proof" => "episode-before-proof",
                        "wrong-scope" => "wrong-scope",
                        "abandon-handoff" | "negative" => "detached",
                        _ => "prepared",
                    }),
                    "{mode}"
                );
            }
        }
        let data = PathBuf::from(fs::read_to_string(root.path().join("data-root")).unwrap());
        let pending = data.join(DIRECTORY).join(NAME);
        let receipt = data.join(".mno-restore-completion-v1.json");
        let has_pending = pending.exists();
        let has_receipt = receipt.exists();
        // Independent expected disk state per injected boundary: never derive
        // correctness merely from whichever artifacts the implementation left.
        let name = mode
            .strip_prefix("fail:")
            .or_else(|| mode.strip_prefix("crash:"))
            .unwrap_or(&mode);
        let before_q = matches!(
            name,
            "episode-before-transfer"
                | "episode-after-transfer"
                | "episode-before-proof"
                | "before-temporary"
                | "partial-write"
                | "sync:journal-file"
                | "sync:journal-directory"
                | "sync:receipt-file"
                | "reject-uncopied"
                | "reject-writer-live"
                | "reject-sources-live"
                | "writer-busy"
                | "cancel-proof"
        );
        let before_p_unlink = matches!(
            name,
            "published"
                | "cleanup-before-p-unlink"
                | "sync:receipt-directory"
                | "sync:receipt-final"
        );
        let q_only = matches!(
            name,
            "cleanup-p-unlinked"
                | "cleanup-after-root-sync"
                | "q-before-unlink"
                | "sync:cleanup-p-directory"
                | "sync:cleanup-root"
                | "sync:final-journal-file"
        );
        assert_eq!(
            (has_pending, has_receipt),
            if before_q {
                (true, false)
            } else if before_p_unlink {
                (true, true)
            } else if q_only {
                (false, true)
            } else {
                (false, false)
            },
            "{mode}: premature artifact cleanup"
        );
        if events.lines().any(|p| p == "cleanup-p-unlinked") {
            let q_synced = events
                .lines()
                .position(|p| p == "sync:receipt-final")
                .expect("Q barrier before deleting P");
            let p_unlinked = events
                .lines()
                .position(|p| p == "cleanup-p-unlinked")
                .unwrap();
            assert!(q_synced < p_unlinked);
        }
        let before = fs::read(data.join("mno.sqlite")).unwrap();
        let p_before = fs::read(&pending).ok();
        let q_before = fs::read(&receipt).ok();
        let state = crate::database_state::DatabaseState::default();
        let startup = state.initialize_legacy_startup(&data);
        if has_pending || has_receipt || data.join("recovery").exists() {
            assert!(
                startup.is_err(),
                "{mode}: incomplete disk state reached Ready"
            );
            assert!(state.operation(None).is_err());
        } else {
            let path = startup.unwrap();
            let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            assert_eq!(
                db.query_row("SELECT title FROM journal_entries", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                if mode == "same" { "Gammal" } else { "Ny" }
            );
            db.close().unwrap();
        }
        assert_eq!(
            fs::read(data.join("mno.sqlite")).unwrap(),
            before,
            "{mode}: startup changed data"
        );
        assert_eq!(fs::read(&pending).ok(), p_before);
        assert_eq!(fs::read(&receipt).ok(), q_before);
        if happy {
            assert!(!has_pending && !has_receipt && !data.join("recovery").exists());
        }
        let protection =
            PathBuf::from(fs::read_to_string(root.path().join("protection-path")).unwrap());
        assert_eq!(
            fs::read(&protection).unwrap(),
            fs::read(root.path().join("protection-before")).unwrap()
        );
    }
}
