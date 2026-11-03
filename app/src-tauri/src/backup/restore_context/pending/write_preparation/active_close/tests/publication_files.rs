//! Real P-publication calls. Every alias/loss control is in a disposable child.
use super::*;
use crate::backup::restore::close_results;
use crate::backup::restore_context::{RestoreMaintenanceScope, TerminalPolicy};
use std::{
    cell::RefCell,
    io::{Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileExt, MetadataExt},
            process::ExitStatusExt,
        },
    },
    time::{Duration, Instant},
};

fn event(root: &Path, value: &str) {
    use std::io::Write;
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("events"))
            .unwrap(),
        "{value}"
    )
    .unwrap();
}
fn lock_state(paths: &[PathBuf; 2], lost: Option<usize>) {
    for (index, path) in paths.iter().enumerate() {
        super::b_close_outcome::check_b_lock(path, lost != Some(index));
    }
}
fn identities(ctx: &RestoreContext<'_, '_>) -> Vec<((i32, u64, u64, u64, bool), usize)> {
    [&ctx.incoming, ctx.protection.as_ref().unwrap().candidate()]
        .into_iter()
        .map(|b| (b.retained_test_identity().unwrap(), b.sqlite_test_handle()))
        .collect()
}
#[derive(Default)]
struct Observation {
    temporary: Option<PathBuf>,
    original: Option<(i32, u64, u64)>,
    substituted: Option<PathBuf>,
    injected: bool,
}
fn substitute(
    observation: &Rc<RefCell<Observation>>,
    root: &Path,
    path: &Path,
    paths: &[PathBuf; 2],
    target: usize,
    stage: &str,
    negative: bool,
) {
    lock_state(paths, None);
    let saved = root.join("saved-original-p");
    fs::rename(path, &saved).unwrap();
    fs::hard_link(&paths[target], path).unwrap();
    let named = fs::symlink_metadata(path).unwrap();
    let main = fs::symlink_metadata(&paths[target]).unwrap();
    assert_eq!((named.dev(), named.ino()), (main.dev(), main.ino()));
    let mut seen = observation.borrow_mut();
    assert!(!seen.injected);
    seen.injected = true;
    seen.substituted = Some(path.to_owned());
    drop(seen);
    event(root, &format!("alias:{stage}"));
    if negative {
        // Intentional OLD unsafe primitive, only against this child's fixtures.
        drop(File::open(path).unwrap());
        event(root, "negative-close");
    }
}

#[test]
fn publication_files_child() {
    let Some(root) = std::env::var_os("MNO_P_FILES_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("MNO_P_FILES_MODE").unwrap();
    let negative = mode.starts_with("negative-");
    let scenario = mode.strip_prefix("negative-").unwrap_or(&mode);
    let target = usize::from(scenario.ends_with("-c"));
    let stage = scenario
        .strip_suffix("-b")
        .or_else(|| scenario.strip_suffix("-c"))
        .unwrap_or(scenario)
        .to_owned();
    let f = Fixture::new();
    assert!(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let scope = RestoreMaintenanceScope {
        maintenance: f.state.maintenance().unwrap(),
        terminal_policy: TerminalPolicy::new(),
    };
    let m = scope.maintenance();
    let mut ctx = context(&f, m);
    ctx.terminal_policy = Some(&scope.terminal_policy);
    let paths = staging(&ctx);
    let original_sources = identities(&ctx);
    let lease = f.state.test_startup_owner();
    let named = f.root.join(DIRECTORY).join(NAME);
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    close_results::take();
    lock_state(&paths, None);
    event(&root, "B-C-live");
    let observation = Rc::new(RefCell::new(Observation::default()));
    let probe_observation = observation.clone();
    let probe_root = root.clone();
    let probe_paths = paths.clone();
    let probe_stage = stage.clone();
    let result = retained_probe::with_file(
        move |point, path, file, bytes| {
            use retained_probe::FilePoint;
            let metadata = file.metadata().unwrap();
            let id = (file.as_raw_fd(), metadata.dev(), metadata.ino());
            let mut cursor = file;
            if point == FilePoint::TemporaryValidation {
                let mut seen = probe_observation.borrow_mut();
                assert!(seen.original.is_none());
                seen.original = Some(id);
                seen.temporary = Some(path.to_owned());
                drop(seen);
                cursor.seek(SeekFrom::Start(7)).unwrap();
                fs::write(probe_root.join("expected-p"), bytes).unwrap();
                event(&probe_root, "original-P-observed");
            }
            assert_eq!(Some(id), probe_observation.borrow().original);
            assert_eq!(cursor.stream_position().unwrap(), 7);
            let mut actual = vec![0; bytes.len()];
            file.read_exact_at(&mut actual, 0).unwrap();
            assert_eq!(actual, bytes);
            let inject = match point {
                FilePoint::TemporaryValidation => probe_stage == "initial",
                FilePoint::BeforeSync(Point::FileSync) => {
                    matches!(probe_stage.as_str(), "file-race" | "aba-file")
                }
                FilePoint::BeforeSync(Point::FinalSync) => {
                    matches!(probe_stage.as_str(), "final-race" | "aba-final")
                }
                _ => false,
            };
            if inject {
                substitute(
                    &probe_observation,
                    &probe_root,
                    path,
                    &probe_paths,
                    target,
                    &probe_stage,
                    negative,
                );
            }
            if let FilePoint::AfterSync(barrier) = point {
                event(&probe_root, &format!("sync:{barrier:?}"));
                // Still before the postcheck and any owner's error/Drop cleanup.
                lock_state(
                    &probe_paths,
                    if negative && probe_observation.borrow().injected {
                        Some(target)
                    } else {
                        None
                    },
                );
                if (barrier == Point::FileSync && probe_stage == "file-after")
                    || (barrier == Point::FinalSync && probe_stage == "final-after")
                {
                    substitute(
                        &probe_observation,
                        &probe_root,
                        path,
                        &probe_paths,
                        target,
                        &probe_stage,
                        negative,
                    );
                }
                if (barrier == Point::FileSync && probe_stage == "aba-file")
                    || (barrier == Point::FinalSync && probe_stage == "aba-final")
                {
                    fs::remove_file(path).unwrap();
                    fs::rename(probe_root.join("saved-original-p"), path).unwrap();
                    probe_observation.borrow_mut().substituted = None;
                    event(&probe_root, "name-restored-after-sync");
                }
            }
        },
        || {
            ctx.publish_write_preparation_inner(&mut |point| {
                if let PreparationPoint::Publication(point) = point {
                    event(&root, &format!("hook:{point:?}"));
                    if (point == Point::FileSync && stage == "file-hook")
                        || (point == Point::FinalSync && stage == "final-hook")
                    {
                        let path = if point == Point::FileSync {
                            observation.borrow().temporary.clone().unwrap()
                        } else {
                            named.clone()
                        };
                        substitute(&observation, &root, &path, &paths, target, &stage, negative);
                    }
                    if (point == Point::FileSync && stage == "error-file")
                        || (point == Point::FinalSync && stage == "error-final")
                    {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                }
                Ok(())
            })
        },
    );
    let success = stage == "valid" || stage.starts_with("aba-");
    let publication_reached = success || stage.starts_with("final-") || stage == "error-final";
    assert_eq!(named.exists(), publication_reached);
    let (success_owner, failure_owner) = if success {
        let preparation = result.unwrap();
        let parts = preparation.owner.0.as_ref().unwrap();
        let pending = parts.pending.as_ref().unwrap();
        assert_eq!(identities(&parts.context), original_sources);
        assert_eq!(
            Some((
                pending.marker_file.as_raw_fd(),
                pending.marker_file.metadata().unwrap().dev(),
                pending.marker_file.metadata().unwrap().ino()
            )),
            observation.borrow().original
        );
        let mut cursor = &pending.marker_file;
        assert_eq!(cursor.stream_position().unwrap(), 7);
        assert_eq!(pending.bytes, fs::read(root.join("expected-p")).unwrap());
        let session = m.recovery_session().unwrap();
        let session_id = session.session_id();
        let reservation = session.test_reservation_slot();
        let bound = preparation.prepare_write(session).unwrap();
        let held = bound.owner.0.as_ref().unwrap();
        assert_eq!(held.session.as_ref().unwrap().session_id(), session_id);
        assert_eq!(
            held.session.as_ref().unwrap().test_reservation_slot(),
            reservation
        );
        lock_state(&paths, None);
        event(&root, "publication-and-session-retained");
        (Some(bound), None)
    } else {
        let failure = result.unwrap_err();
        if stage.starts_with("error-") {
            assert!(
                matches!(failure.error(), WritePreparationError::Marker(MarkerError::Io(e)) if e.kind() == io::ErrorKind::PermissionDenied)
            );
        } else {
            assert!(matches!(
                failure.error(),
                WritePreparationError::Marker(MarkerError::Binding)
            ));
        }
        let parts = failure.owner.0.as_ref().unwrap();
        assert!(parts.pending.is_none() && parts.session.is_none() && parts.binding.is_none());
        assert_eq!(identities(&parts.context), original_sources);
        assert!(std::ptr::eq(parts.context.maintenance, m));
        assert!(std::ptr::eq(
            parts.context.terminal_policy.unwrap(),
            &scope.terminal_policy
        ));
        lock_state(&paths, if negative { Some(target) } else { None });
        event(
            &root,
            if negative {
                "rejected-with-detected-lock-loss"
            } else {
                "rejected-with-locks"
            },
        );
        (None, Some(failure))
    };
    assert_eq!(success_owner.is_some(), success);
    assert_eq!(failure_owner.is_some(), !success);
    assert!(m.ensure_usable().is_err());
    crate::database_state::maintenance_retention::assert_exclusive(&f.state, &f.path);
    assert_eq!(f.state.test_startup_owner(), lease);
    assert!(scope.terminal_policy.is_never_armed());
    assert!(close_results::take().is_empty());
    assert!(f.root.join(DIRECTORY).is_dir());
    if let Some(path) = observation.borrow().substituted.as_ref() {
        // Tempfile error cleanup may already have unlinked our alias. No regular open.
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => panic!("{e}"),
        }
        fs::rename(root.join("saved-original-p"), path).unwrap();
        fs::write(
            root.join("restored-p-path"),
            path.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
    } else if publication_reached {
        fs::write(
            root.join("restored-p-path"),
            named.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
    }
    event(&root, "exit-before-owner-drop");
    std::process::exit(if negative { 78 } else { 0 });
}

#[test]
fn publication_file_handles_preserve_live_sqlite_locks() {
    let mut cases = vec![
        "valid".to_owned(),
        "error-file".to_owned(),
        "error-final".to_owned(),
    ];
    for stage in [
        "initial",
        "file-hook",
        "file-race",
        "file-after",
        "final-hook",
        "final-race",
        "final-after",
    ] {
        for target in ["b", "c"] {
            cases.push(format!("{stage}-{target}"));
            cases.push(format!("negative-{stage}-{target}"));
        }
    }
    for stage in ["aba-file", "aba-final"] {
        for target in ["b", "c"] {
            cases.push(format!("{stage}-{target}"));
        }
    }
    for mode in cases {
        let root = tempfile::TempDir::new().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["publication_files::publication_files_child", "--nocapture"])
            .env("MNO_P_FILES_ROOT", root.path())
            .env("MNO_P_FILES_MODE", &mode)
            .env("TMPDIR", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("timeout {mode}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            status.signal(),
            None,
            "unexpected signal {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let negative = mode.starts_with("negative-");
        assert_eq!(
            status.code(),
            Some(if negative { 78 } else { 0 }),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let scenario = mode.strip_prefix("negative-").unwrap_or(&mode);
        let stage = scenario
            .strip_suffix("-b")
            .or_else(|| scenario.strip_suffix("-c"))
            .unwrap_or(scenario);
        let data = PathBuf::from(
            String::from_utf8(fs::read(root.path().join("data-root")).unwrap()).unwrap(),
        );
        assert!(
            data.canonicalize()
                .unwrap()
                .starts_with(root.path().canonicalize().unwrap())
        );
        let mut events = vec!["B-C-live".to_owned(), "original-P-observed".to_owned()];
        let alias_events = |events: &mut Vec<String>| {
            events.push(format!("alias:{stage}"));
            if negative {
                events.push("negative-close".to_owned());
            }
        };
        if stage == "initial" {
            alias_events(&mut events);
        } else {
            events.push("hook:FileSync".to_owned());
            if stage == "file-hook" {
                alias_events(&mut events);
            } else if stage != "error-file" {
                if stage == "file-race" || stage == "aba-file" {
                    alias_events(&mut events);
                }
                events.push("sync:FileSync".to_owned());
                if stage == "file-after" {
                    alias_events(&mut events);
                }
                if stage == "aba-file" {
                    events.push("name-restored-after-sync".to_owned());
                }
                if !stage.starts_with("file-") {
                    events.push("hook:Published".to_owned());
                    events.extend(
                        data.join(DIRECTORY)
                            .ancestors()
                            .map(|_| "hook:DirectorySync".to_owned()),
                    );
                    events.push("hook:FinalSync".to_owned());
                    if stage == "final-hook" {
                        alias_events(&mut events);
                    } else if stage != "error-final" {
                        if stage == "final-race" || stage == "aba-final" {
                            alias_events(&mut events);
                        }
                        events.push("sync:FinalSync".to_owned());
                        if stage == "final-after" {
                            alias_events(&mut events);
                        }
                        if stage == "aba-final" {
                            events.push("name-restored-after-sync".to_owned());
                        }
                    }
                }
            }
        }
        events.push(
            if stage == "valid" || stage.starts_with("aba-") {
                "publication-and-session-retained"
            } else if negative {
                "rejected-with-detected-lock-loss"
            } else {
                "rejected-with-locks"
            }
            .to_owned(),
        );
        events.push("exit-before-owner-drop".to_owned());
        assert_eq!(
            fs::read_to_string(root.path().join("events")).unwrap(),
            events.join("\n") + "\n",
            "{mode}"
        );
        if let Ok(path) = fs::read(root.path().join("restored-p-path")) {
            let path = PathBuf::from(String::from_utf8(path).unwrap());
            assert!(
                path.canonicalize()
                    .unwrap()
                    .starts_with(root.path().canonicalize().unwrap())
            );
            assert_eq!(
                fs::read(path).unwrap(),
                fs::read(root.path().join("expected-p")).unwrap(),
                "{mode}"
            );
        } else {
            assert_eq!(stage, "error-file");
            assert!(!data.join(DIRECTORY).join(NAME).exists());
        }
        match crate::startup::begin(&data).unwrap() {
            crate::startup::StartupOutcome::Recovery(gate) => {
                assert_eq!(gate.reason(), "database_recovery_artifact_present")
            }
            crate::startup::StartupOutcome::Ready(_) => panic!("must remain Recovery {mode}"),
        }
    }
}
