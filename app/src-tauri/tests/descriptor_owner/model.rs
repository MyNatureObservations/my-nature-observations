//! Integration-test-only close ownership; no VFS, mutation or admission API.
use rusqlite::{Connection, OpenFlags};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Identity,
    Io,
    Sqlite(i32),
    Active,
}
fn identity(f: &File) -> Result<(u64, u64), Error> {
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file() || m.nlink() != 1 {
        return Err(Error::Identity);
    }
    Ok((m.dev(), m.ino()))
}
fn sql(e: rusqlite::Error) -> Error {
    Error::Sqlite(e.sqlite_error().map_or(-1, |e| e.extended_code))
}
// Prepared is not Clone; no fd accessor. All opens precede the first lock.
pub(super) struct Prepared {
    state: Box<State>,
}
struct State {
    // Explicit close empties these before extra File owners may be destroyed.
    connections: Vec<Option<Connection>>,
    extras: Vec<File>,
    root: File,
    expected: (u64, u64),
    directory: (u64, u64),
    path: PathBuf,
    sandbox: tempfile::TempDir,
}
#[derive(Debug)]
pub(super) struct Closed {
    _sandbox: tempfile::TempDir,
}
pub(super) struct Domain {
    state: Option<Box<State>>,
}
pub(super) struct Episode<'a> {
    domain: &'a mut Domain,
}
pub(super) struct Failure {
    pub(super) error: Error,
    domain: Domain,
}
impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Failure").field(&self.error).finish()
    }
}
impl State {
    fn check(&self) -> Result<(), Error> {
        let root = fs::symlink_metadata(self.sandbox.path()).map_err(|_| Error::Io)?;
        let held = self.root.metadata().map_err(|_| Error::Io)?;
        if !root.is_dir()
            || root.file_type().is_symlink()
            || (root.dev(), root.ino()) != self.directory
            || (held.dev(), held.ino()) != self.directory
        {
            return Err(Error::Identity);
        }
        let named = fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?;
        if !named.is_file()
            || named.file_type().is_symlink()
            || named.nlink() != 1
            || (named.dev(), named.ino()) != self.expected
        {
            return Err(Error::Identity);
        }
        for file in &self.extras {
            if identity(file)? != self.expected {
                return Err(Error::Identity);
            }
        }
        Ok(())
    }
}
impl Prepared {
    pub(super) fn fixture(extra: usize, readers: usize) -> Result<Self, Error> {
        let sandbox = tempfile::tempdir().map_err(|_| Error::Io)?;
        let path = sandbox.path().join("mno.sqlite");
        let c = Connection::open(&path).map_err(sql)?;
        c.execute_batch("CREATE TABLE sample(value); INSERT INTO sample VALUES(7)")
            .map_err(sql)?;
        c.close().map_err(|(_, e)| sql(e))?;
        let path = path.canonicalize().map_err(|_| Error::Io)?;
        let root = File::open(sandbox.path()).map_err(|_| Error::Io)?;
        let m = root.metadata().map_err(|_| Error::Io)?;
        let directory = (m.dev(), m.ino());
        let anchor = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|_| Error::Io)?;
        let expected = identity(&anchor)?;
        let mut state = Box::new(State {
            connections: vec![],
            extras: vec![anchor],
            root,
            expected,
            directory,
            path,
            sandbox,
        });
        // No locks/transactions exist yet. All connection and fd creation is
        // confined to this fixture factory, before Domain's episode is issued.
        for _ in 0..extra {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&state.path)
                .map_err(|_| Error::Io)?;
            if identity(&file)? != expected {
                return Err(Error::Identity);
            }
            state.extras.push(file);
        }
        for _ in 0..=readers {
            let c = Connection::open_with_flags(
                &state.path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(sql)?;
            c.busy_timeout(std::time::Duration::ZERO).map_err(sql)?;
            state.connections.push(Some(c));
        }
        state.check()?;
        Ok(Self { state })
    }
    pub(super) fn path(&self) -> PathBuf {
        self.state.path.clone()
    }
    pub(super) fn bind(self) -> Domain {
        Domain {
            state: Some(self.state),
        }
    }
}
impl Domain {
    pub(super) fn begin(&mut self) -> Result<Episode<'_>, Error> {
        let state = self.state.as_mut().ok_or(Error::Active)?;
        state.check()?;
        state.connections[0]
            .as_ref()
            .unwrap()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(sql)?;
        for c in state.connections.iter().skip(1).flatten() {
            c.execute_batch("BEGIN; SELECT * FROM sample")
                .map_err(sql)?;
        }
        Ok(Episode { domain: self })
    }
    pub(super) fn request_extra_close(&mut self) -> Result<(), Error> {
        Err(Error::Active)
    }
    pub(super) fn request_new_open(&mut self) -> Result<(), Error> {
        Err(Error::Active)
    }
    pub(super) fn finish(self) -> Result<Closed, Failure> {
        self.finish_inner(&mut |_, _| {})
    }
    // Test-only observation inside the SAME close sequence, after confirmed
    // native close and before the remaining connections/extra files are dropped.
    fn finish_inner(
        mut self,
        checkpoint: &mut impl FnMut(usize, &State),
    ) -> Result<Closed, Failure> {
        let state = self.state.as_mut().unwrap();
        // Readers close FIRST, reserved writer LAST. No extra fd closes until
        // every Connection::close has returned OK, including that writer.
        for i in (0..state.connections.len()).rev() {
            if let Some(c) = state.connections[i].take() {
                if let Err((c, e)) = c.close() {
                    state.connections[i] = Some(c);
                    return Err(Failure {
                        error: sql(e),
                        domain: self,
                    });
                }
                checkpoint(i, state);
            }
        }
        let State {
            connections,
            extras,
            root,
            expected: _,
            directory: _,
            path: _,
            sandbox,
        } = *self.state.take().unwrap();
        drop(connections);
        drop(extras);
        drop(root);
        Ok(Closed { _sandbox: sandbox })
    }
}
impl Episode<'_> {
    pub(super) fn check(&self) -> Result<(), Error> {
        self.domain.state.as_ref().unwrap().check()
    }
    pub(super) fn request_extra_close(&mut self) -> Result<(), Error> {
        self.domain.request_extra_close()
    }
    pub(super) fn request_new_open(&mut self) -> Result<(), Error> {
        self.domain.request_new_open()
    }
}
// An abandoned episode does not close anything. Its owner still holds the whole
// state. If THAT owner is dropped without confirmed close, retain ONE coherent
// Box including connections, extra fds and TempDir. No raw fd escape/forget.
// Tests that exercise retention run in a subprocess, bounding it by process exit.
impl Drop for Domain {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            let _ = Box::leak(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn child(path: &std::path::Path, busy: bool) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "model::tests::sqlite_child", "--nocapture"])
            .env("MNO_OWNER_CHILD", path)
            .env("MNO_OWNER_BUSY", if busy { "1" } else { "0" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
    #[test]
    fn sqlite_child() {
        let Ok(path) = std::env::var("MNO_OWNER_CHILD") else {
            return;
        };
        let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        c.busy_timeout(std::time::Duration::ZERO).unwrap();
        let result = c.execute_batch("BEGIN IMMEDIATE");
        if std::env::var("MNO_OWNER_BUSY").unwrap() == "1" {
            assert!(
                matches!(result,Err(e) if e.sqlite_error().is_some_and(|e|e.extended_code==rusqlite::ffi::SQLITE_BUSY))
            )
        } else {
            result.unwrap();
            c.execute_batch("ROLLBACK").unwrap();
        }
        c.close().unwrap();
    }
    #[test]
    fn retained_extras_and_multiple_connections_preserve_kernel_lock() {
        for _ in 0..8 {
            let p = Prepared::fixture(4, 2).unwrap();
            let path = p.path();
            let bytes = fs::read(&path).unwrap();
            let mut d = p.bind();
            {
                let mut e = d.begin().unwrap();
                e.check().unwrap();
                assert_eq!(e.request_extra_close(), Err(Error::Active));
                assert_eq!(e.request_new_open(), Err(Error::Active));
                child(&path, true);
            }
            child(&path, true); // Episode Drop alone cannot release anything.
            drop(d.finish().unwrap());
            // TempDir cleanup follows all native closes. Preserve file for the
            // post-close child in a separate test below; here cleanup is expected.
            assert!(!path.exists());
            let _ = bytes;
        }
    }
    #[test]
    fn close_all_then_external_writer_succeeds() {
        let p = Prepared::fixture(3, 2).unwrap();
        let path = p.path();
        let bytes = fs::read(&path).unwrap();
        let mut d = p.bind();
        {
            let _e = d.begin().unwrap();
            child(&path, true);
        }
        let closed = d.finish().unwrap();
        child(&path, false);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        drop(closed);
        assert!(!path.exists());
    }
    #[test]
    fn retained_drop_panic_busy_child() {
        let Ok(mode) = std::env::var("MNO_OWNER_RETAIN") else {
            return;
        };
        let p = Prepared::fixture(3, 2).unwrap();
        let path = p.path();
        let bytes = fs::read(&path).unwrap();
        let mut d = p.bind();
        if mode == "begin_error" {
            use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            let hits = Arc::new(AtomicUsize::new(0));
            let seen = hits.clone();
            d.state.as_ref().unwrap().connections[2]
                .as_ref()
                .unwrap()
                .authorizer(Some(move |ctx: AuthContext<'_>| {
                    if matches!(ctx.action, AuthAction::Select) {
                        seen.fetch_add(1, Ordering::SeqCst);
                        Authorization::Deny
                    } else {
                        Authorization::Allow
                    }
                }))
                .unwrap();
            assert!(matches!(
                d.begin(),
                Err(Error::Sqlite(rusqlite::ffi::SQLITE_AUTH))
            ));
            assert!(
                hits.load(Ordering::SeqCst) > 0,
                "intended SELECT denial must run"
            );
            let state = d.state.as_ref().unwrap();
            assert!(!state.connections[0].as_ref().unwrap().is_autocommit());
            assert!(!state.connections[1].as_ref().unwrap().is_autocommit());
            assert_eq!(state.extras.len(), 4);
            child(&path, true);
        } else {
            let _e = d.begin().unwrap();
        }
        match mode.as_str() {
            "drop" | "begin_error" => drop(d),
            "finish_panic" => {
                let hit = std::cell::Cell::new(false);
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    d.finish_inner(&mut |index, state| {
                        assert_eq!(index, 2, "first real reader-close must finish");
                        assert!(state.connections[2].is_none());
                        assert!(state.connections[0].is_some());
                        assert!(state.connections[1].is_some());
                        assert_eq!(state.extras.len(), 4);
                        assert!(state.sandbox.path().is_dir());
                        child(&path, true);
                        hit.set(true);
                        panic!("after first confirmed close");
                    })
                }));
                let payload = caught.unwrap_err();
                assert!(hit.get());
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"after first confirmed close")
                );
            }
            "panic" => {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _held = d;
                        panic!("episode unwind")
                    }))
                    .is_err()
                );
            }
            "busy" | "busy_writer" => {
                // One intentionally retained real statement makes sqlite3_close
                // on the reader return BUSY. The complete failed Domain is retained
                // with it, in this subprocess only, never a detached live database.
                let index = if mode == "busy_writer" { 0 } else { 1 };
                let c = d.state.as_ref().unwrap().connections[index]
                    .as_ref()
                    .unwrap();
                let _ = Box::leak(Box::new(c.prepare("SELECT * FROM sample").unwrap()));
                let failure = d.finish().unwrap_err();
                assert_eq!(failure.error, Error::Sqlite(rusqlite::ffi::SQLITE_BUSY));
                assert!(
                    failure.domain.state.as_ref().unwrap().connections[2].is_none(),
                    "later reader already closed before BUSY"
                );
                child(&path, true);
                assert!(failure.domain.state.as_ref().unwrap().connections[0].is_some());
                drop(failure);
            }
            _ => unreachable!(),
        }
        child(&path, true);
        // Do NOT fs::read(main) here: its temporary fd-close would itself
        // release the process locks. Parent verifies bytes after this exit.
        use sha2::Digest;
        println!("EXPECTED_HASH={:x}", sha2::Sha256::digest(&bytes));
        // No TempDir cleanup before retained SQLite/descriptor owners stop using it.
        // This is the only externally advertised cleanup path for retained tests:
        // after this process exits, the parent removes the now-unused directory.
        println!("RETAINED_ROOT={}", path.parent().unwrap().display());
    }
    fn retained_case(mode: &str) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "model::tests::retained_drop_panic_busy_child",
                "--nocapture",
            ])
            .env("MNO_OWNER_RETAIN", mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        let root = text
            .lines()
            .find_map(|l| l.strip_prefix("RETAINED_ROOT="))
            .unwrap();
        let path = std::path::Path::new(root);
        assert!(path.join("mno.sqlite").is_file());
        let expected = text
            .lines()
            .find_map(|l| l.strip_prefix("EXPECTED_HASH="))
            .unwrap();
        use sha2::Digest;
        assert_eq!(
            format!(
                "{:x}",
                sha2::Sha256::digest(fs::read(path.join("mno.sqlite")).unwrap())
            ),
            expected
        );
        child(&path.join("mno.sqlite"), false); // exited child owns no locks now
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn error_drop_panic_and_real_busy_retain_entire_episode() {
        for mode in ["drop", "panic", "busy", "busy_writer"] {
            retained_case(mode);
        }
    }
    #[test]
    fn partial_begin_select_error_retains_kernel_lock_and_all_extras() {
        retained_case("begin_error");
    }
    #[test]
    fn panic_after_first_real_close_retains_remaining_connections_and_kernel_lock() {
        retained_case("finish_panic");
    }
    #[test]
    fn late_inode_symlink_and_stale_metadata_reject_without_closing_extras() {
        for symlink in [false, true] {
            let p = Prepared::fixture(2, 1).unwrap();
            let path = p.path();
            let mut d = p.bind();
            {
                let _e = d.begin().unwrap();
            }
            let old = path.with_file_name("old.sqlite");
            fs::rename(&path, &old).unwrap();
            if symlink {
                std::os::unix::fs::symlink(&old, &path).unwrap();
            } else {
                fs::write(&path, b"foreign").unwrap();
            }
            assert_eq!(d.state.as_ref().unwrap().check(), Err(Error::Identity));
            child(&old, true); // actual old inode remains protected despite rejection
            drop(d.finish().unwrap());
        }
    }
    #[test]
    fn failure_before_locks_and_wrong_extra_identity_are_rejected() {
        let p = Prepared::fixture(2, 1).unwrap();
        let path = p.path();
        let foreign = tempfile::tempfile().unwrap();
        let mut d = p.bind();
        d.state.as_mut().unwrap().extras.push(foreign);
        assert!(matches!(d.begin(), Err(Error::Identity)));
        child(&path, false);
        drop(d.finish().unwrap());
        assert!(!path.exists());
    }
    #[test]
    fn failed_validation_during_reserved_keeps_descriptors_until_close() {
        let p = Prepared::fixture(2, 1).unwrap();
        let path = p.path();
        let mut d = p.bind();
        {
            let _e = d.begin().unwrap();
        }
        let expected = d.state.as_ref().unwrap().expected;
        // Deterministic stale binding, without opening/closing another main fd.
        d.state.as_mut().unwrap().expected.1 ^= 1;
        assert_eq!(d.state.as_ref().unwrap().check(), Err(Error::Identity));
        assert_eq!(d.request_extra_close(), Err(Error::Active));
        child(&path, true);
        d.state.as_mut().unwrap().expected = expected;
        let closed = d.finish().unwrap();
        child(&path, false);
        drop(closed);
    }
}
