//! Test-only integration of real borrowed restore resources with the 5d.3p owner.
use super::*;
use crate::database_state::maintenance_retention::{Owner, assert_exclusive};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) fn outer<'s>(f: &'s Fixture) -> Owner<'s> {
    let maintenance = f.state.maintenance().unwrap();
    let connection =
        Connection::open_with_flags(&f.path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    Owner::new(maintenance, connection, tempfile::TempDir::new().unwrap())
}
pub(super) fn prepared<'m, 's>(
    f: &Fixture,
    m: &'m Maintenance<'s>,
) -> PreparedWriteEpisode<'m, 's> {
    let ctx = context(f, m);
    assert!(std::ptr::eq(ctx.maintenance, m));
    // The short reference ends here; no borrowed view owns or releases M.
    {
        let view = &ctx;
        assert_eq!(view.generation, m.generation());
    }
    assert_exclusive(&f.state, &f.path);
    let durable = ctx.publish_write_preparation().unwrap();
    let session = m.recovery_session().unwrap();
    let id = session.session_id();
    let epoch = durable
        .prepare_write(session)
        .unwrap()
        .close_active()
        .unwrap()
        .into_epoch_inputs();
    assert_eq!(
        epoch.with_verification(|v| (v.session_id(), v.operation_id(), v.generation())),
        Some((id, m.operation_id(), m.generation()))
    );
    let prepared = epoch.prepare_episode().unwrap();
    let p = prepared.inputs.inputs.owner.after.as_ref().unwrap();
    assert_eq!(p.session.as_ref().unwrap().session_id(), id);
    p.binding
        .as_ref()
        .unwrap()
        .check_session(p.session.as_ref().unwrap())
        .unwrap();
    assert!(std::ptr::eq(p.maintenance, m));
    assert_eq!(
        m.recovery_session().err().as_deref(),
        Some("database_recovery_busy")
    );
    prepared
}
#[test]
fn borrowed_child() {
    let Ok(mode) = std::env::var("MNO_5D3R_MODE") else {
        return;
    };
    let parent = PathBuf::from(std::env::var_os("MNO_5D3R_ROOT").unwrap());
    let f = Fixture::new();
    assert!(f._dir.path().starts_with(&parent));
    let main = fs::read(&f.path).unwrap();
    if mode == "outer-unwind" {
        let retained = std::cell::RefCell::new(None);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let outer = outer(&f);
            let prepared = prepared(&f, outer.maintenance());
            let p = prepared.inputs.inputs.owner.after.as_ref().unwrap();
            *retained.borrow_mut() = Some((
                p.sources
                    .live()
                    .unwrap()
                    .incoming
                    .as_ref()
                    .unwrap()
                    .database_path(),
                p.sources
                    .live()
                    .unwrap()
                    .protection
                    .as_ref()
                    .unwrap()
                    .candidate()
                    .database_path(),
                p.pending.as_ref().unwrap().bytes.clone(),
            ));
            let _prepared = prepared;
            panic!("entire borrowed scope unwind");
        }));
        let payload = match caught {
            Err(payload) => payload,
            Ok(_) => panic!("required panic"),
        };
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"entire borrowed scope unwind")
        );
        assert_exclusive(&f.state, &f.path);
        let (b, c, bytes) = retained.into_inner().expect("full real chain reached");
        assert!(b.is_file() && c.is_file());
        assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
    } else {
        let outer = outer(&f);
        assert_exclusive(&f.state, &f.path);
        match mode.as_str() {
            "normal" => {
                let prepared = prepared(&f, outer.maintenance());
                let p = prepared.inputs.inputs.owner.after.as_ref().unwrap();
                let bytes = p.pending.as_ref().unwrap().bytes.clone();
                let b = p
                    .sources
                    .live()
                    .unwrap()
                    .incoming
                    .as_ref()
                    .unwrap()
                    .database_path();
                let c = p
                    .sources
                    .live()
                    .unwrap()
                    .protection
                    .as_ref()
                    .unwrap()
                    .candidate()
                    .database_path();
                drop(prepared);
                assert!(!b.exists() && !c.exists());
                assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
                assert_exclusive(&f.state, &f.path);
            }
            "before" => {
                let intended: std::result::Result<(), &str> = Err("before borrowed construction");
                assert_eq!(intended, Err("before borrowed construction"));
                assert!(!f.root.join(DIRECTORY).join(NAME).exists());
                assert_exclusive(&f.state, &f.path);
            }
            "construction" => {
                let ctx = context(&f, outer.maintenance());
                let paths = staging(&ctx);
                let hit = Cell::new(false);
                let failure = match ctx.publish_write_preparation_inner(&mut |point| {
                    if matches!(point, PreparationPoint::BeforeAuthorization) {
                        hit.set(true);
                        Err(io::Error::other("construction checkpoint"))
                    } else {
                        Ok(())
                    }
                }) {
                    Err(failure) => failure,
                    Ok(_) => panic!("required construction failure"),
                };
                assert!(hit.get());
                assert!(matches!(failure.error(), WritePreparationError::Checkpoint));
                assert_exclusive(&f.state, &f.path);
                drop(failure);
                assert!(paths.iter().all(|p| !p.exists()));
                assert!(!f.root.join(DIRECTORY).join(NAME).exists());
                assert_exclusive(&f.state, &f.path);
            }
            "inner-unwind" => {
                let prepared = prepared(&f, outer.maintenance());
                let p = prepared.inputs.inputs.owner.after.as_ref().unwrap();
                let bytes = p.pending.as_ref().unwrap().bytes.clone();
                let b = p
                    .sources
                    .live()
                    .unwrap()
                    .incoming
                    .as_ref()
                    .unwrap()
                    .database_path();
                let c = p
                    .sources
                    .live()
                    .unwrap()
                    .protection
                    .as_ref()
                    .unwrap()
                    .candidate()
                    .database_path();
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _inner = prepared;
                    panic!("borrowed inner unwind");
                }));
                let payload = match caught {
                    Err(payload) => payload,
                    Ok(_) => panic!("required panic"),
                };
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"borrowed inner unwind")
                );
                assert!(b.is_file() && c.is_file());
                assert_eq!(fs::read(f.root.join(DIRECTORY).join(NAME)).unwrap(), bytes);
                assert_eq!(
                    outer.maintenance().recovery_session().err().as_deref(),
                    Some("database_recovery_busy")
                );
                assert_exclusive(&f.state, &f.path);
                drop(outer);
                assert_exclusive(&f.state, &f.path);
                println!("BORROWED_RETAINED:{mode}");
                assert_eq!(fs::read(&f.path).unwrap(), main);
                std::process::exit(0);
            }
            _ => panic!("unknown integration mode"),
        }
        // All tracked inner borrows have ended. The original, not a replacement,
        // owner consumes its independent read-only connection and releases M.
        let _closed = match outer.close() {
            Ok(closed) => closed,
            Err(_) => panic!("outer close"),
        };
        let next = f.state.maintenance().unwrap();
        assert_eq!(next.generation(), 1);
        if mode == "normal" {
            blocked(&next, &f);
        }
        drop(next);
    }
    assert_eq!(fs::read(&f.path).unwrap(), main);
    println!("BORROWED_RETAINED:{mode}");
    // Leaked inner guards can reference this real fixture/state until OS exit.
    // Never destroy state under deliberately retained borrowed guards.
    std::process::exit(0);
}
fn run(mode: &str) {
    let directory = tempfile::TempDir::new().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["borrow_integration::borrowed_child", "--nocapture"])
        .env("MNO_5D3R_MODE", mode)
        .env("MNO_5D3R_ROOT", directory.path())
        .env("TMPDIR", directory.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("integration child timeout: {mode}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("BORROWED_RETAINED:{mode}")));
}
#[test]
fn outer_outlives_normal_borrowed_chain() {
    run("normal");
}
#[test]
fn outer_survives_borrowed_construction_errors() {
    run("before");
    run("construction");
}
#[test]
fn inner_and_outer_unwind_retain_real_maintenance() {
    run("inner-unwind");
    run("outer-unwind");
}
