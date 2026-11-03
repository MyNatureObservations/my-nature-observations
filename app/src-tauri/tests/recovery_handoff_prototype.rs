//! Standalone test model: intentionally does not import or alter production APIs.
#[path = "recovery_handoff_prototype/model.rs"]
mod model;
use model::State;

#[test]
fn handoff_ends_all_maintenance_borrows_without_releasing_reservation() {
    let state = State::new();
    let mut maintenance = state.maintenance();
    let session = maintenance.session().unwrap();
    let id = session.id();
    let handoff = session.model_closed_chain().into_handoff();
    maintenance.exclusive_probe();
    assert_eq!(handoff.id(), id);
    assert!(handoff.reserved());
    assert!(handoff.retains_lease(&state));
    assert_eq!(maintenance.session().err(), Some("busy"));
    assert_eq!(handoff.generation(), maintenance.generation());
    assert!(!state.ready());
    handoff.consume();
    assert!(!state.reserved());
    let next = maintenance.session().unwrap();
    assert_ne!(id, next.id());
    drop(next);
    assert_eq!(maintenance.generation(), 1);
    assert!(!state.ready());
}

#[test]
fn dropped_handoff_releases_only_slot_even_during_unwind() {
    let state = State::new();
    let maintenance = state.maintenance();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let handoff = maintenance
            .session()
            .unwrap()
            .model_closed_chain()
            .into_handoff();
        assert!(state.reserved());
        assert!(maintenance.session().is_err());
        let _held = handoff;
        panic!("modeled incomplete handoff");
    }));
    assert!(result.is_err());
    assert!(!state.reserved());
    assert!(!state.ready());
    assert_eq!(maintenance.generation(), 1);
}

#[test]
fn compiler_probes_enforce_handoff_ownership() {
    use std::{fs, process::Command};
    let dir = tempfile::tempdir().unwrap();
    let source = include_str!("recovery_handoff_prototype/model.rs");
    // Strip module doc comments only; compile the actual same model definitions.
    let source = source
        .lines()
        .filter(|line| !line.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let compile = |body: &str| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, format!("#![allow(dead_code)]\nmod model {{ {source} }}\nuse model::*;\nfn main() {{ {body} }}")).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        "let s = State::new(); let mut m = s.maintenance(); let h = m.session().unwrap().model_closed_chain().into_handoff(); m.exclusive_probe(); assert!(h.reserved()); h.consume();",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (body, code) in [
        ("let _ = RunningHandoff {};", "due to private fields"),
        (
            "let s=State::new(); let m=s.maintenance(); let h=m.session().unwrap().model_closed_chain().into_handoff(); let _=h.clone();",
            "E0599",
        ),
        (
            "let s=State::new(); let m=s.maintenance(); let h=m.session().unwrap().model_closed_chain().into_handoff(); h.consume(); h.consume();",
            "E0382",
        ),
        (
            "let h; { let s=State::new(); let m=s.maintenance(); h=m.session().unwrap().model_closed_chain().into_handoff(); } h.consume();",
            "E0597",
        ),
        (
            "let s=State::new(); let mut m=s.maintenance(); let p=m.session().unwrap().model_closed_chain(); m.exclusive_probe(); let _=p.into_handoff();",
            "E0502",
        ),
        (
            "fn extend<'a>(h: RunningHandoff<'a>) -> RunningHandoff<'static> { h }",
            "lifetime may not live long enough",
        ),
        (
            "let s=State::new(); let m=s.maintenance(); let h=m.session().unwrap().model_closed_chain().into_handoff(); let _=h.reservation;",
            "E0616",
        ),
    ] {
        let output = compile(body);
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "unexpected success: {body}");
        assert!(
            diagnostic.contains(code),
            "wrong failure: {body}\n{diagnostic}"
        );
    }
}

#[test]
fn competing_attempt_is_busy_while_transferred_reservation_lives() {
    let state = State::new();
    let mut maintenance = state.maintenance();
    let proof = maintenance.session().unwrap().model_closed_chain();
    let handoff = proof.into_handoff();
    maintenance.exclusive_probe();
    let boundary = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            boundary.wait();
            let result = maintenance.session().err();
            boundary.wait();
            result
        });
        boundary.wait();
        assert!(handoff.reserved());
        boundary.wait();
        assert_eq!(worker.join().unwrap(), Some("busy"));
        assert!(handoff.reserved());
    });
    drop(handoff);
    assert!(maintenance.session().is_ok());
    assert!(!state.ready());
    assert_eq!(maintenance.generation(), 1);
}
