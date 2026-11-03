#![cfg(all(test, target_os = "macos"))]
#[path = "managed_target_probe/model.rs"]
mod model;
use model::*;
use rusqlite::ffi;
use std::fs;

#[test]
fn main_descriptor_identity_and_native_locks() {
    let f = Fixture::new();
    let bytes = fs::read(f.path()).unwrap();
    let mut a = Target::open(&f).unwrap();
    assert_eq!(a.lock_state().unwrap(), ffi::SQLITE_LOCK_NONE);
    a.lock(ffi::SQLITE_LOCK_SHARED).unwrap();
    assert!(!a.reserved().unwrap());
    a.lock(ffi::SQLITE_LOCK_RESERVED).unwrap();
    assert!(a.reserved().unwrap());
    let mut b = Target::open(&f).unwrap();
    b.lock(ffi::SQLITE_LOCK_SHARED).unwrap();
    assert!(b.reserved().unwrap());
    assert_eq!(
        b.lock(ffi::SQLITE_LOCK_RESERVED),
        Err(Error::Sqlite(ffi::SQLITE_BUSY))
    );
    assert_eq!(
        a.lock(ffi::SQLITE_LOCK_EXCLUSIVE),
        Err(Error::Sqlite(ffi::SQLITE_BUSY))
    );
    b.unlock(ffi::SQLITE_LOCK_NONE).unwrap();
    b.close().unwrap();
    a.lock(ffi::SQLITE_LOCK_EXCLUSIVE).unwrap();
    assert_eq!(a.lock_state().unwrap(), ffi::SQLITE_LOCK_EXCLUSIVE);
    a.unlock(ffi::SQLITE_LOCK_SHARED).unwrap();
    a.unlock(ffi::SQLITE_LOCK_NONE).unwrap();
    a.close().unwrap();
    assert_eq!(fs::read(f.path()).unwrap(), bytes);
}

#[test]
fn competing_process_child() {
    let Ok(path) = std::env::var("MNO_5D3A_PROBE_CHILD") else {
        return;
    };
    let c =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .unwrap();
    c.busy_timeout(std::time::Duration::ZERO).unwrap();
    let e = c.execute_batch("BEGIN IMMEDIATE").unwrap_err();
    assert!(
        matches!(e,rusqlite::Error::SqliteFailure(e,_) if e.code==rusqlite::ErrorCode::DatabaseBusy)
    );
    c.close().unwrap();
}
#[test]
fn reserved_lock_blocks_real_sqlite_in_another_process() {
    let f = Fixture::new();
    let bytes = fs::read(f.path()).unwrap();
    let mut a = Target::open(&f).unwrap();
    a.lock(ffi::SQLITE_LOCK_SHARED).unwrap();
    a.lock(ffi::SQLITE_LOCK_RESERVED).unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "competing_process_child", "--nocapture"])
        .env("MNO_5D3A_PROBE_CHILD", f.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"));
    a.unlock(ffi::SQLITE_LOCK_NONE).unwrap();
    a.close().unwrap();
    let c = rusqlite::Connection::open_with_flags(
        f.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    c.execute_batch("BEGIN IMMEDIATE; ROLLBACK").unwrap();
    c.close().unwrap();
    assert_eq!(fs::read(f.path()).unwrap(), bytes);
}
#[test]
fn replaced_symlink_hardlink_and_sidecars_are_rejected() {
    for mode in 0..7 {
        let f = Fixture::new();
        let original = fs::read(f.path()).unwrap();
        match mode {
            0 => {
                fs::rename(f.path(), f.sandbox.path().join("old")).unwrap();
                fs::write(f.path(), &original).unwrap();
            }
            1 => {
                fs::rename(f.path(), f.sandbox.path().join("old")).unwrap();
                std::os::unix::fs::symlink("old", f.path()).unwrap();
            }
            2 => fs::hard_link(f.path(), f.sandbox.path().join("alias")).unwrap(),
            3 => fs::write(f.sandbox.path().join("mno.sqlite-journal"), b"foreign").unwrap(),
            4 => fs::write(f.sandbox.path().join("MNO.SQLITE-WAL"), b"").unwrap(),
            5 => fs::create_dir(f.sandbox.path().join("mno.sqlite-shm")).unwrap(),
            _ => {
                fs::remove_file(f.path()).unwrap();
            }
        }
        assert!(Target::open(&f).is_err(), "mode {mode}");
        if mode == 3 {
            assert_eq!(
                fs::read(f.sandbox.path().join("mno.sqlite-journal")).unwrap(),
                b"foreign"
            )
        }
    }
}
#[test]
fn late_replacement_and_unwind_do_not_grant_writes() {
    let f = Fixture::new();
    let bytes = fs::read(f.path()).unwrap();
    let mut a = Target::open(&f).unwrap();
    fs::rename(f.path(), f.sandbox.path().join("old")).unwrap();
    fs::write(f.path(), b"foreign").unwrap();
    assert_eq!(a.lock(ffi::SQLITE_LOCK_SHARED), Err(Error::Identity));
    a.close().unwrap();
    assert_eq!(fs::read(f.path()).unwrap(), b"foreign");
    assert_eq!(fs::read(f.sandbox.path().join("old")).unwrap(), bytes);
    let g = Fixture::new();
    let bytes = fs::read(g.path()).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut t = Target::open(&g).unwrap();
            t.lock(ffi::SQLITE_LOCK_SHARED).unwrap();
            panic!("probe unwind")
        }))
        .is_err()
    );
    let c = rusqlite::Connection::open_with_flags(
        g.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    c.execute_batch("BEGIN EXCLUSIVE; ROLLBACK").unwrap();
    c.close().unwrap();
    assert_eq!(fs::read(g.path()).unwrap(), bytes);
}

// Required external-process control: same-process inode bookkeeping can mask
// kernel locks lost by closing an additional non-SQLite descriptor.
#[test]
fn reserved_lock_must_survive_closing_another_descriptor_bound_target() {
    let f = Fixture::new();
    let mut a = Target::open(&f).unwrap();
    a.lock(ffi::SQLITE_LOCK_SHARED).unwrap();
    a.lock(ffi::SQLITE_LOCK_RESERVED).unwrap();
    let b = Target::open(&f).unwrap();
    b.close().unwrap();
    // SQLite's in-process state still claims RESERVED. The child must verify
    // that a real kernel lock, not only this bookkeeping, still protects us.
    assert_eq!(a.lock_state().unwrap(), ffi::SQLITE_LOCK_RESERVED);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "competing_process_child", "--nocapture"])
        .env("MNO_5D3A_PROBE_CHILD", f.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "reservation lost after closing independent fd: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    a.unlock(ffi::SQLITE_LOCK_NONE).unwrap();
    a.close().unwrap();
}
