use super::*;
use crate::backup::{self, BackupKind};
use rusqlite::ffi;
use std::{cell::Cell, rc::Rc};

pub(super) fn scenario(
    action: impl for<'a, 's> FnOnce(RestoreContext<'a, 's>, &RecoverySession<'_, '_>, &Path),
) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("book");
    let state = DatabaseState::default();
    let path = state.initialize_legacy_startup(&root).unwrap();
    {
        let db = Connection::open(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES('entry','2026-10-18','Old',?1,1,'created','updated')", [r#"{"type":"doc","content":[{"type":"paragraph"}]}"#]).unwrap();
    }
    let original = fs::read(&path).unwrap();
    let archive = dir.path().join("incoming.mno-backup");
    backup::create_backup(
        &Connection::open(&path).unwrap(),
        &archive,
        BackupKind::Manual,
    )
    .unwrap();
    let flag = || Arc::new(AtomicBool::new(false));
    let incoming = backup::restore::prepare_candidate(&archive, flag()).unwrap();
    with_active(&state, &path, incoming, flag(), |mut ctx| {
        {
            let auth = ctx.authorize()?;
            drop(auth.publish_pending().unwrap());
        }
        let receipt = pending::classification::classify_session(
            ctx.maintenance.recovery_session().unwrap(),
            flag(),
        )
        .into_completion_proof()
        .unwrap()
        .publish_receipt()
        .unwrap();
        drop(receipt);
        let q_path = root.join(".mno-restore-completion-v1.json");
        let q = fs::read(&q_path).unwrap();
        let protection = ctx.protection.as_ref().unwrap().path().to_owned();
        let protection_bytes = fs::read(&protection).unwrap();
        let maintenance = ctx.maintenance;
        let session = maintenance.recovery_session().unwrap();
        action(ctx, &session, &root);
        assert!(maintenance.recovery_session().is_err());
        drop(session);
        assert_eq!(maintenance.generation(), 1);
        assert!(maintenance.ensure_usable().is_err());
        assert!(crate::startup::DirectoryLease::open_existing(&root).is_err());
        assert_eq!(fs::read(&q_path).unwrap(), q);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(&protection).unwrap(), protection_bytes);
        assert!(root.join("recovery/pending-restore.json").is_file());
        Ok(())
    })
    .unwrap();
    assert!(state.operation(Some(1)).is_err());
    assert!(state.maintenance().unwrap().ensure_usable().is_err());
}
// A real outstanding SQLite statement, with no Rust borrow carried into close.
fn statement(db: &Connection) -> *mut ffi::sqlite3_stmt {
    let mut stmt = std::ptr::null_mut();
    // SAFETY: test-owned live connection; the test finalizes the statement exactly once.
    unsafe {
        assert_eq!(
            ffi::sqlite3_prepare_v2(
                db.handle(),
                c"SELECT 1".as_ptr(),
                -1,
                &mut stmt,
                std::ptr::null_mut()
            ),
            ffi::SQLITE_OK
        );
        assert_eq!(ffi::sqlite3_step(stmt), ffi::SQLITE_ROW);
    }
    stmt
}
fn busy(error: &rusqlite::Error) {
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.extended_code == ffi::SQLITE_BUSY)
    );
}
fn paths(ctx: &RestoreContext<'_, '_>) -> [PathBuf; 2] {
    [
        ctx.incoming.database_path(),
        ctx.protection.as_ref().unwrap().candidate().database_path(),
    ]
}
#[test]
fn all_three_close_in_order_and_success_drop_cleans_staging() {
    scenario(|ctx, session, _| {
        let files = paths(&ctx);
        crate::backup::restore::close_results::take();
        let points = Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = points.clone();
        let closed = hooks::with(
            move |p| captured.borrow_mut().push(p.to_owned()),
            || ctx.close_owned_connections(session).unwrap(),
        );
        assert_eq!(
            &*points.borrow(),
            &[
                "owners-transferred",
                "active-close-guard",
                "after-active",
                "after-incoming"
            ]
        );
        closed
            .parts()
            ._binding
            .session
            .check_session(session)
            .unwrap();
        assert_eq!(
            closed.parts()._binding.restore_id,
            session.maintenance_binding().unwrap().0
        );
        assert_eq!(
            closed.parts()._binding.database,
            private::file_identity(&closed.parts()._active.file).unwrap()
        );
        assert!(files.iter().all(|p| p.is_file()));
        assert_eq!(
            crate::backup::restore::close_results::take(),
            vec![ffi::SQLITE_OK, ffi::SQLITE_OK]
        );
        drop(closed);
        assert!(
            crate::backup::restore::close_results::take().is_empty(),
            "no second SQLite-close during successful Drop"
        );
        assert!(files.iter().all(|p| !p.exists()));
    });
}
fn busy_case(index: usize, finalize_before_drop: bool) {
    scenario(|ctx, session, _| {
        let files = paths(&ctx);
        let db = match index {
            0 => ctx.active.connection(),
            1 => ctx.incoming.source().unwrap(),
            _ => ctx
                .protection
                .as_ref()
                .unwrap()
                .candidate()
                .source()
                .unwrap(),
        };
        let stmt = statement(db);
        let failed = ctx
            .close_owned_connections(session)
            .err()
            .expect("BUSY must not issue proof");
        match (index, failed.error()) {
            (0, ContextCloseError::Active(e))
            | (1, ContextCloseError::Incoming(e))
            | (2, ContextCloseError::Protection(e)) => busy(e),
            _ => panic!("wrong close stage: {failed:?}"),
        }
        let Retained::Closing(closing) = &*failed._retained else {
            panic!("closing owner")
        };
        closing.binding.session.check_session(session).unwrap();
        assert_eq!(
            matches!(closing.active, Some(ActiveOwner::Closed(_))),
            index > 0
        );
        assert_eq!(
            matches!(closing.incoming, Some(PreparedOwner::Closed(_))),
            index > 1
        );
        assert!(files.iter().all(|p| p.is_file()));
        if finalize_before_drop {
            assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
            drop(failed);
            assert!(files.iter().all(|p| !p.exists()));
        } else {
            drop(failed);
            // The still-live statement proves that close did not invalidate its resources.
            assert_eq!(unsafe { ffi::sqlite3_step(stmt) }, ffi::SQLITE_DONE);
            if index > 0 {
                assert!(files[index - 1].is_file());
            }
            assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
            // Retention after BUSY is intentionally conservative; no unsafe retry API.
        }
    });
}
#[test]
fn actual_busy_in_active_retains_owner() {
    busy_case(0, false);
}
#[test]
fn actual_busy_in_incoming_retains_staging_after_active_closed() {
    busy_case(1, false);
}
#[test]
fn actual_busy_in_protection_retains_staging_after_active_and_incoming_closed() {
    busy_case(2, false);
}
#[test]
fn successful_drop_retry_after_busy_cleans_staging() {
    for i in 0..3 {
        busy_case(i, true);
    }
}
#[test]
fn panic_at_ownership_and_close_boundaries_retains_busy_staging() {
    for point in [
        "owners-transferred",
        "active-close-guard",
        "after-active",
        "after-incoming",
    ] {
        scenario(|ctx, session, _| {
            let files = paths(&ctx);
            let stmt = statement(
                ctx.protection
                    .as_ref()
                    .unwrap()
                    .candidate()
                    .source()
                    .unwrap(),
            );
            let hit = Rc::new(Cell::new(false));
            let reached = hit.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hooks::with(
                    move |p| {
                        if p == point && !reached.replace(true) {
                            panic!("injected {point}");
                        }
                    },
                    || ctx.close_owned_connections(session),
                )
            }));
            assert!(result.is_err());
            assert!(hit.get(), "must reach {point}");
            assert!(files[1].is_file());
            assert_eq!(unsafe { ffi::sqlite3_step(stmt) }, ffi::SQLITE_DONE);
            assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
        });
    }
}
#[test]
fn invalid_binding_retains_context_and_never_returns_close_proof() {
    scenario(|mut ctx, session, _| {
        let files = paths(&ctx);
        ctx.generation += 1;
        let failed = ctx.close_owned_connections(session).err().unwrap();
        assert!(matches!(failed.error(), ContextCloseError::Binding));
        assert!(matches!(&*failed._retained, Retained::Before(_)));
        assert!(files.iter().all(|p| p.is_file()));
        drop(failed);
        assert!(files.iter().all(|p| !p.exists()));
    });
}

#[test]
fn compiler_enforces_private_proof_consumption_and_borrowed_lifetimes() {
    // Compile the REAL close module and actual context/authorization declarations.
    // Only unrelated database creation/verifier backends are stubbed; no runtime
    // assertion relies on these stubs. A positive probe must compile first.
    fn declaration(source: &str, prefix: &str) -> String {
        let start = source.find(prefix).unwrap();
        let mut depth = 0;
        let mut opened = false;
        for (offset, ch) in source[start..].char_indices() {
            if ch == '{' {
                depth += 1;
                opened = true;
            }
            if ch == '}' {
                depth -= 1;
                if opened && depth == 0 {
                    return source[start..start + offset + 1].to_owned();
                }
            }
        }
        panic!("unterminated declaration")
    }
    let source = include_str!("../../restore_context.rs");
    let context = declaration(source, "pub(crate) struct RestoreContext");
    let policy = format!(
        "{} {} {}",
        declaration(source, "struct TerminalPolicy"),
        declaration(source, "enum TerminalPhase"),
        declaration(source, "impl TerminalPolicy")
    );
    let authorization = declaration(source, "pub(crate) struct AuthorizedRestore");
    // Keep the real callback signature. Only its I/O body is replaced.
    let callback_start = source.find("fn with_active<").unwrap();
    let callback_end = source[callback_start..].find(") -> Result<T> {").unwrap()
        + callback_start
        + ") -> Result<T>".len();
    let callback = format!(
        "pub {} {{ unimplemented!() }}",
        &source[callback_start..callback_end]
    );
    let close = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backup/restore_context/close.rs");
    let public_start = source.find("pub(crate) fn with_restore_context<").unwrap();
    let public_end = source[public_start..].find(") -> Result<T> {").unwrap()
        + public_start
        + ") -> Result<T>".len();
    let public_callback = format!(
        "{} {{ unimplemented!() }}",
        &source[public_start..public_end]
    );
    let common = format!(
        r#"
        extern crate rusqlite;
        extern crate uuid;
        mod database_state {{ pub mod recovery_session {{
            pub struct RecoverySession<'a, 's>(pub &'a (), pub &'s ());
            pub struct ContextSessionBinding<'a, 's>(pub &'a (), pub &'s ());
            pub struct ClosedContextBinding<'s>(pub &'s ());
            impl<'s> ContextSessionBinding<'_, 's> {{pub fn into_closed_record(self)->ClosedContextBinding<'s>{{ClosedContextBinding(self.1)}}}}
            impl RecoverySession<'_, '_> {{
                pub fn maintenance_binding(&self)->Option<(uuid::Uuid,u32)>{{unimplemented!()}}
                pub fn directory_identity(&self)->Result<(u64,u64),String>{{unimplemented!()}}
                pub fn bind_context<'a, 's>(&self, _:&'a crate::backup::restore_context::Maintenance<'s>)->Result<ContextSessionBinding<'a, 's>,String>{{unimplemented!()}}
            }}
            impl ContextSessionBinding<'_, '_> {{
                pub fn database_path(&self)->&std::path::Path{{unimplemented!()}}
                pub fn operation_id(&self)->uuid::Uuid{{unimplemented!()}}
                pub fn check_session(&self,_:&RecoverySession)->Result<(),String>{{Ok(())}}
            }}
        }} }}
        mod backup {{
            pub mod restore {{
                pub struct ClosedPreparedRestore;
                pub struct PreparedRestore;
                pub struct PreparedCloseFailure {{ pub error:rusqlite::Error }}
                impl PreparedRestore {{ pub fn try_close(self)->Result<ClosedPreparedRestore,PreparedCloseFailure>{{unimplemented!()}} }}
            }}
            pub mod pre_restore {{
                pub struct ProtectionRecord<'a, 's>(pub &'a (), pub &'s ());
                pub struct ClosedProtectionRecord;
                impl ProtectionRecord<'_, '_> {{pub fn into_closed_record(self)->ClosedProtectionRecord{{ClosedProtectionRecord}}}}
                pub struct VerifiedProtection<'a, 's>(pub &'a (), pub &'s ());
                impl<'a, 's> VerifiedProtection<'a, 's> {{
                    pub fn is_bound_to(&self,_:&super::restore_context::Maintenance<'_>,_:&super::restore_context::ActiveDatabase)->bool{{true}}
                    pub fn into_parts(self)->(super::restore::PreparedRestore,ProtectionRecord<'a, 's>){{unimplemented!()}}
                }}
            }}
            pub mod restore_context {{
                use super::restore::PreparedRestore;
                use super::pre_restore::VerifiedProtection;
                use rusqlite::Connection;
                pub struct Maintenance<'a>(pub &'a ());
                impl Maintenance<'_>{{pub fn generation(&self)->u32{{1}} pub fn operation_id(&self)->uuid::Uuid{{unimplemented!()}}}}
                pub struct ActiveDatabase {{ connection:Option<Connection>, files:Option<ActiveFiles> }}
                type Path=std::path::Path;
                #[derive(Debug)] pub enum ContextError {{Coordination(String),InvalidProof}}
                type Result<T>=std::result::Result<T,ContextError>;
                #[derive(PartialEq)] pub struct FileIdentity;
                impl FileIdentity {{fn components(&self)->(u64,u64){{unimplemented!()}}}}
                mod private {{pub fn file_identity(_:&std::fs::File)->Result<super::FileIdentity,()>{{unimplemented!()}}}}
                struct ActiveFiles {{file:std::fs::File,identity:FileIdentity,path:std::path::PathBuf}}
                impl ActiveDatabase {{fn ensure_current(&self)->std::result::Result<(),()>{{Ok(())}} fn path(&self)->&std::path::Path{{unimplemented!()}} fn identity(&self)->FileIdentity{{unimplemented!()}}}}
                {policy}
    {context}
                {authorization}
                type DatabaseState=();
                use std::sync::{{Arc, atomic::AtomicBool}};
                {callback}
                pub mod tauri {{ pub struct AppHandle; }}
                {public_callback}
                pub struct StateOnly<'s>(pub &'s DatabaseState);
                // Test-only projection carries no Maintenance lifetime or authority.
                pub fn project<'m,'s>(context:RestoreContext<'m,'s>)->StateOnly<'s>{{StateOnly(context.maintenance.0)}}
                impl RestoreContext<'_, '_> {{pub fn authorize(&mut self)->AuthorizedRestore<'_,'_, '_>{{AuthorizedRestore{{context:self}}}}}}
                #[path={close:?}] pub mod close;
            }}
        }}
        use backup::restore_context::{{RestoreContext,Maintenance}};
        use backup::restore_context::close::ContextConnectionsClosed;
        use database_state::recovery_session::RecoverySession;
    "#
    );
    let dir = tempfile::TempDir::new().unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let rlib = fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("librusqlite-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .unwrap();
    let uuid_rlib = fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libuuid-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .unwrap();
    let compile = |probe: &str| {
        let input = dir.path().join("probe.rs");
        fs::write(&input, format!("{common}\n{probe}")).unwrap();
        std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--extern")
            .arg(format!("rusqlite={}", rlib.display()))
            .arg("--extern")
            .arg(format!("uuid={}", uuid_rlib.display()))
            .arg(input)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        "fn intended(c:RestoreContext<'_, '_>,s:&RecoverySession){let _=c.close_owned_connections(s);}",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    // Separate state lifetime is fixed outside the higher-ranked guard borrow.
    let split_callback = compile(
        r#"
        fn boundary<'state>(state:&'state ())->backup::restore_context::StateOnly<'state> {
            backup::restore_context::with_active(state,std::path::Path::new("unused"),
                backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                |context| Ok(backup::restore_context::project(context))).unwrap()
        }
        fn separated<'m,'state>(context:RestoreContext<'m,'state>,session:&RecoverySession<'m,'state>) {
            let _:Result<ContextConnectionsClosed<'m,'state>,_>=context.close_owned_connections(session);
        }
    "#,
    );
    assert!(
        split_callback.status.success(),
        "{}",
        String::from_utf8_lossy(&split_callback.stderr)
    );
    for (probe, expected) in [
        (
            r#"fn state_only_public_escape(app:&backup::restore_context::tauri::AppHandle){let _=backup::restore_context::with_restore_context(app,backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|context|Ok(backup::restore_context::project(context)));}"#,
            "lifetime may not live long enough",
        ),
        (
            r#"fn state_only_outlives_owner(){let result;{let state=();result=backup::restore_context::with_active(&state,std::path::Path::new("unused"),backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|context|Ok(backup::restore_context::project(context))).unwrap();}let _=result.0;}"#,
            "E0597",
        ),
        (
            r#"fn public_escape(app:&backup::restore_context::tauri::AppHandle){let _=backup::restore_context::with_restore_context(app,backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|context|Ok(context));}"#,
            "lifetime may not live long enough",
        ),
        (
            "fn callback_escape(state:&()){let _=backup::restore_context::with_active(state,std::path::Path::new(\"unused\"),backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|context|Ok(context));}",
            "lifetime may not live long enough",
        ),
        (
            "fn callback_error(state:&(),session:&RecoverySession){let _=backup::restore_context::with_active(state,std::path::Path::new(\"unused\"),backup::restore::PreparedRestore,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|context|Ok(context.close_owned_connections(session).err().unwrap()));}",
            "lifetime may not live long enough",
        ),
        (
            "fn missing(c:RestoreContext<'_, '_>){let _=c.close_owned_connections();}",
            "E0061",
        ),
        (
            "fn ended(c:RestoreContext<'_, '_>,s:RecoverySession){drop(s);let _=c.close_owned_connections(&s);}",
            "E0382",
        ),
        (
            "fn twice(c:RestoreContext<'_, '_>,s:&RecoverySession){let _=c.close_owned_connections(s);let _=c.close_owned_connections(s);}",
            "E0382",
        ),
        (
            "fn borrowed(mut c:RestoreContext<'_, '_>,s:&RecoverySession){let a=c.authorize();let _=c.close_owned_connections(s);drop(a);}",
            "E0505",
        ),
        (
            "fn clone(c:ContextConnectionsClosed<'_, '_>){let _=c.clone();}",
            "E0599",
        ),
        (
            "fn escape(c:RestoreContext<'_, '_>,s:&RecoverySession)->ContextConnectionsClosed<'static, 'static>{c.close_owned_connections(s).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn fabricate()->ContextConnectionsClosed<'static, 'static>{ContextConnectionsClosed{}}",
            "private",
        ),
    ] {
        let negative = compile(probe);
        let errors = String::from_utf8_lossy(&negative.stderr);
        assert!(
            !negative.status.success() && errors.contains(expected),
            "{probe}: {errors}"
        );
    }
}

#[test]
fn non_busy_close_error_retains_connection_files_and_staging_without_proof() {
    use std::os::fd::AsRawFd;
    scenario(|ctx, session, _| {
        let staging = paths(&ctx);
        let descriptor = ctx.active.files.as_ref().unwrap().file.as_raw_fd();
        let stmt = statement(ctx.active.connection());
        let failed =
            hooks::with_close_error(ffi::SQLITE_IOERR, || ctx.close_owned_connections(session))
                .err()
                .expect("IOERR must not issue a close proof");
        assert!(
            matches!(failed.error(), ContextCloseError::Active(rusqlite::Error::SqliteFailure(e, _)) if e.extended_code == ffi::SQLITE_IOERR)
        );
        let Retained::Closing(held) = &*failed._retained else {
            panic!("retained closing owner")
        };
        let Some(ActiveOwner::Live(active)) = &held.active else {
            panic!("live active connection")
        };
        assert!(active.connection().is_autocommit());
        active.ensure_current().unwrap();
        assert_eq!(active.files.as_ref().unwrap().file.as_raw_fd(), descriptor);
        assert!(matches!(held.incoming, Some(PreparedOwner::Live(_))));
        assert!(matches!(held.protection, Some(PreparedOwner::Live(_))));
        assert!(staging.iter().all(|p| p.is_file()));
        assert!(unsafe { libc::fcntl(descriptor, libc::F_GETFD) } >= 0);
        assert_eq!(unsafe { ffi::sqlite3_step(stmt) }, ffi::SQLITE_DONE);
        assert_eq!(unsafe { ffi::sqlite3_finalize(stmt) }, ffi::SQLITE_OK);
        drop(failed); // Safe retry succeeds after the real statement was finalized.
        assert!(staging.iter().all(|p| !p.exists()));
    });
}

#[test]
fn panic_inside_active_close_with_live_statement_retains_connection_and_dependencies() {
    use std::os::fd::AsRawFd;
    scenario(|ctx, session, _| {
        let staging = paths(&ctx);
        let descriptor = ctx.active.files.as_ref().unwrap().file.as_raw_fd();
        let active_stmt = statement(ctx.active.connection());
        let protection_stmt = statement(
            ctx.protection
                .as_ref()
                .unwrap()
                .candidate()
                .source()
                .unwrap(),
        );
        let visits = Rc::new(Cell::new(0));
        let reached = visits.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            hooks::with(
                move |point| {
                    if point == "active-close-guard" {
                        reached.set(reached.get() + 1);
                        panic!("panic after ActiveDatabase file guard, before explicit close");
                    }
                },
                || ctx.close_owned_connections(session),
            )
        }));
        assert!(result.is_err());
        assert_eq!(
            visits.get(),
            1,
            "active owner must not reenter explicit close during unwinding"
        );
        assert!(
            unsafe { libc::fcntl(descriptor, libc::F_GETFD) } >= 0,
            "retained file descriptor"
        );
        // Real statements remain valid after owner unwinding; therefore their
        // sqlite3 connections were not prematurely destroyed or double-closed.
        assert_eq!(unsafe { ffi::sqlite3_step(active_stmt) }, ffi::SQLITE_DONE);
        assert_eq!(
            unsafe { ffi::sqlite3_step(protection_stmt) },
            ffi::SQLITE_DONE
        );
        assert!(staging[1].is_file(), "busy protection must retain staging");
        assert!(
            !staging[0].exists(),
            "safely closed incoming staging may be cleaned"
        );
        assert_eq!(
            unsafe { ffi::sqlite3_finalize(active_stmt) },
            ffi::SQLITE_OK
        );
        assert_eq!(
            unsafe { ffi::sqlite3_finalize(protection_stmt) },
            ffi::SQLITE_OK
        );
        // These uncertain owners are intentionally retained, not retried via a raw DB handle.
    });
}
