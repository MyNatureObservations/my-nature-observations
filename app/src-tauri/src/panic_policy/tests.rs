use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn mark(value: &str) {
    writeln!(std::io::stderr(), "{value}").unwrap();
}

struct Witness;
impl Drop for Witness {
    fn drop(&mut self) {
        mark("DROP");
    }
}

#[test]
fn child() {
    let Ok(mode) = std::env::var("MNO_PANIC_POLICY_CHILD") else {
        return;
    };
    if mode != "baseline" {
        super::install();
    }
    if mode == "result" {
        let result: Result<(), &str> = Err("ordinary-error");
        assert_eq!(result.unwrap_err(), "ordinary-error");
        mark("RESULT-HANDLED");
        return;
    }
    mark("BEFORE");
    let _ = std::panic::catch_unwind(|| {
        let _witness = Witness;
        panic!("intentional policy probe");
    });
    mark("CAUGHT");
}

fn run(mode: &str) -> (std::process::ExitStatus, String) {
    let child_test = format!("{}::child", module_path!().split_once("::").unwrap().1);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_test, "--nocapture"])
        .env("MNO_PANIC_POLICY_CHILD", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("policy child timed out: {mode}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    (output.status, String::from_utf8(output.stderr).unwrap())
}

#[test]
fn panic_aborts_before_unwind_and_catch_return() {
    let (status, stderr) = run("panic");
    assert_eq!(status.signal(), Some(6), "{status}: {stderr}");
    assert!(
        stderr.starts_with("BEFORE\nMNO startup panic: panicked at"),
        "{stderr}"
    );
    assert!(stderr.contains("intentional policy probe"), "{stderr}");
    assert!(!stderr.contains("DROP\nCAUGHT"));

    // Same unwind harness without installation must expose the witnesses.
    let (status, stderr) = run("baseline");
    assert!(status.success(), "{status}: {stderr}");
    assert!(stderr.starts_with("BEFORE\n"), "{stderr}");
    assert!(stderr.ends_with("DROP\nCAUGHT\n"), "{stderr}");
}

#[test]
fn ordinary_result_does_not_abort() {
    let (status, stderr) = run("result");
    assert!(status.success(), "{status}: {stderr}");
    assert_eq!(stderr, "RESULT-HANDLED\n");
}

#[test]
fn app_installs_policy_before_initialization() {
    let app = include_str!("../lib.rs");
    let body = app.split_once("pub fn run() {").unwrap().1;
    assert!(body.trim_start().starts_with("panic_policy::install();"));
    assert!(include_str!("../main.rs").contains("app_lib::run()"));
    let manifest = include_str!("../../Cargo.toml");
    let release = manifest.split_once("[profile.release]").unwrap().1;
    assert!(
        release
            .lines()
            .any(|line| line.trim() == r#"panic = "abort""#)
    );
}
