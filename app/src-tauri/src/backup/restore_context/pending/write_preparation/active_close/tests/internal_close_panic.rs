//! Subprocess-only observation of rusqlite 0.40.2 hook destruction in close.
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct PanicOnRemoval {
    events: PathBuf,
    calls: Arc<AtomicUsize>,
}
impl Drop for PanicOnRemoval {
    fn drop(&mut self) {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        // No unwrap/second intentional panic during a possible Drop retry.
        let _ = fs::write(&self.events, format!("hook-destructor:{call}"));
        if call == 0 {
            panic!("internal rollback-hook destructor");
        }
    }
}

// Shared only by the sibling subprocess probe; never exposes a resource.
pub(super) fn install_internal_panic(connection: &Connection, events: PathBuf) -> Arc<AtomicUsize> {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = PanicOnRemoval {
        events,
        calls: calls.clone(),
    };
    connection
        .rollback_hook(Some(move || {
            let _ = &captured;
        }))
        .unwrap();
    calls
}

#[test]
fn internal_close_child() {
    let Some(root) = std::env::var_os("MNO_INTERNAL_CLOSE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let f = Fixture::new();
    assert!(f._dir.path().starts_with(&root));
    let m = f.state.maintenance().unwrap();
    let durable = context(&f, &m).publish_write_preparation().unwrap();
    // Real durable publication, not a fabricated marker. Keep its whole owner.
    let marker = f.root.join(DIRECTORY).join(NAME);
    let bytes = fs::read(&marker).unwrap();
    fs::write(
        root.join("data-root"),
        f.root.as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    fs::write(root.join("expected-p"), &bytes).unwrap();
    blocked(&m, &f);
    let connection = Connection::open_in_memory().unwrap();
    let calls = install_internal_panic(&connection, root.join("destructor"));
    fs::write(root.join("before-close"), b"entered consuming close next").unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| connection.close()));
    match result {
        Err(payload) => {
            // Only this exact injected panic qualifies. Never reuse Connection.
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"internal rollback-hook destructor")
            );
            assert!(calls.load(Ordering::SeqCst) >= 1);
            fs::write(root.join("caught"), b"no Connection returned; fail-stop").unwrap();
            // Do not drop payload, durable owner or M; no post-panic DB operation.
            // OS exit is the terminal test policy, not a production implementation.
            std::process::exit(73);
        }
        Ok(_) => {
            fs::write(root.join("unexpected-return"), b"close returned").unwrap();
            drop(durable);
            std::process::exit(74);
        }
    }
}

#[test]
fn internal_hook_destructor_panic_fail_stops_and_preserves_durable_p() {
    let directory = tempfile::TempDir::new().unwrap();
    let root = directory.path();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["internal_close_panic::internal_close_child", "--nocapture"])
        .env("MNO_INTERNAL_CLOSE_ROOT", root)
        .env("TMPDIR", root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("internal close child timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        status.code(),
        Some(73),
        "expected caught internal panic followed by fail-stop: {status}"
    );
    assert!(root.join("before-close").is_file());
    assert!(root.join("destructor").is_file());
    assert_eq!(
        fs::read(root.join("caught")).unwrap(),
        b"no Connection returned; fail-stop"
    );
    assert!(!root.join("unexpected-return").exists());
    let data = PathBuf::from(String::from_utf8(fs::read(root.join("data-root")).unwrap()).unwrap());
    assert!(
        data.canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert_eq!(
        fs::read(data.join(DIRECTORY).join(NAME)).unwrap(),
        fs::read(root.join("expected-p")).unwrap()
    );
    match crate::startup::begin(&data).unwrap() {
        crate::startup::StartupOutcome::Recovery(gate) => {
            assert_eq!(gate.reason(), "database_recovery_artifact_present")
        }
        crate::startup::StartupOutcome::Ready(_) => panic!("durable P must block startup"),
    }
}
