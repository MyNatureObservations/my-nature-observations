use super::*;

// Compile actual production owners/factories; stub only I/O and adjacent backends.
// Positive controls must compile before expected diagnostic checks.
fn declaration(source: &str, prefix: &str) -> String {
    let start = source.find(prefix).unwrap();
    let mut depth = 0;
    for (offset, ch) in source[start..].char_indices() {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return source[start..start + offset + 1].to_owned();
            }
        }
    }
    panic!("unterminated declaration")
}
fn signature(source: &str, prefix: &str) -> String {
    let start = source.find(prefix).unwrap();
    let end = source[start..].find('{').unwrap() + start;
    format!("{} {{ unimplemented!() }}", &source[start..end])
}
#[test]
fn write_preparation_compiler_checks_actual_factories_and_session_ownership() {
    let context_source = include_str!("../../../../restore_context.rs");
    let pending_source = include_str!("../../../pending.rs");
    let production_source = include_str!("../../write_preparation.rs");
    let mut production = production_source
        .split("#[cfg(all(test, target_os = \"macos\"))]")
        .next()
        .unwrap()
        .to_owned();
    for name in ["fn verify(", "fn retained_name_matches("] {
        let body = declaration(&production, name);
        production = production.replace(&body, &signature(&body, name));
    }
    let dir = tempfile::TempDir::new().unwrap();
    let projected = dir.path().join("write_preparation.rs");
    let active_source = include_str!("../../write_preparation/active_close.rs");
    let mut active = active_source
        .split("#[cfg(all(test, target_os = \"macos\"))]")
        .next()
        .unwrap()
        .to_owned();
    for name in [
        "fn check_before(",
        "fn check_after(",
        "fn check_common_after(",
        "fn check_abc_connections_closed(",
    ] {
        let body = declaration(&active, name);
        active = active.replace(&body, &signature(&body, name));
    }
    let body = declaration(&active, "pub(crate) fn check_before_writer(");
    active = active.replace(
        &body,
        &signature(&body, "pub(crate) fn check_before_writer("),
    );
    // Runtime close/handoff is exercised with real resources by subprocesses.
    // Keep its ACTUAL consuming signature here, just as the I/O checks above;
    // adjacent resource stubs must not pretend to implement SQLite close.
    let body = declaration(&active, "fn close_b(");
    active = active.replace(&body, &signature(&body, "fn close_b("));
    let body = declaration(&active, "fn close_c(");
    active = active.replace(&body, &signature(&body, "fn close_c("));
    active.push_str("\npub(crate) fn close_c_for_probe<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->std::result::Result<PreparedWriteEpisode<'m,'s>,Box<ActiveCloseFailure<'m,'s>>>{p.close_c()}\n");
    active.push_str("\npub(crate) fn close_b_for_probe<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->std::result::Result<PreparedWriteEpisode<'m,'s>,Box<ActiveCloseFailure<'m,'s>>>{p.close_b()}\n");
    active = active.replace("mod preflight;", "").replace(
        "#[cfg(all(target_os = \"macos\", any(test, panic = \"abort\")))]\nmod writer;",
        "",
    );
    // Adjacent backend type paths only; actual enum and ownership code remain.
    active = active
        .replace("crate::backup::restore::", "crate::owner::")
        .replace("crate::backup::pre_restore::", "crate::owner::");
    let active_path = dir.path().join("active_close.rs");
    fs::write(&active_path, active).unwrap();
    production.push_str(&format!(
        "\n#[path={active_path:?}] pub(crate) mod active_close;\n"
    ));
    fs::write(&projected, production).unwrap();
    let context = declaration(context_source, "pub(crate) struct RestoreContext");
    let policy = format!(
        "{} {} {}",
        declaration(context_source, "struct TerminalPolicy"),
        declaration(context_source, "enum TerminalPhase"),
        declaration(context_source, "impl TerminalPolicy")
    );
    let authorization = declaration(context_source, "pub(crate) struct AuthorizedRestore");
    let marker = declaration(pending_source, "pub(crate) struct PendingMarker");
    let publish = signature(pending_source, "fn publish_with<");
    let outer = signature(context_source, "pub(crate) fn with_restore_context<");
    let inner = signature(context_source, "fn with_active<");
    let common = format!(
        r#"
extern crate uuid;
mod database_state {{ pub mod recovery_session {{
    pub struct RecoverySession<'m,'s>(std::marker::PhantomData<&'m crate::owner::Maintenance<'s>>);
    pub struct ContextSessionBinding<'m,'s>(std::marker::PhantomData<&'m crate::owner::Maintenance<'s>>);
    impl RecoverySession<'_, '_> {{
        pub fn session_id(&self)->uuid::Uuid{{unimplemented!()}}
        pub fn bind_context<'m,'s>(&self,_:&'m crate::owner::Maintenance<'s>)->Result<ContextSessionBinding<'m,'s>,String>{{unimplemented!()}}
        pub fn directory_identity(&self)->Result<(u64,u64),String>{{unimplemented!()}}
    }}
    impl ContextSessionBinding<'_, '_> {{
        pub fn check_session(&self,_:&RecoverySession<'_, '_>)->Result<(),String>{{unimplemented!()}}
        pub fn database_path(&self)->&std::path::Path{{unimplemented!()}}
        pub fn operation_id(&self)->uuid::Uuid{{unimplemented!()}}
    }}
}} }}
mod owner {{
    use std::{{fs::File, path::{{Path, PathBuf}}, io, sync::{{Arc, atomic::AtomicBool}}}};
    pub struct DatabaseState;
    pub struct Maintenance<'s>(pub &'s DatabaseState);
    impl Maintenance<'_>{{pub fn operation_id(&self)->uuid::Uuid{{unimplemented!()}}}}
    pub struct ActiveFiles;
    pub struct ActiveDatabase;
    impl ActiveDatabase {{ pub fn close_inner(&mut self)->rusqlite::Result<ActiveFiles>{{unimplemented!()}} pub fn path(&self)->&Path{{unimplemented!()}}}}
    pub struct PreparedRestore;
    pub struct PreparedCloseFailure;
    pub struct ClosedPreparedRestore;
    pub struct ProtectionRecord<'m,'s>(std::marker::PhantomData<&'m Maintenance<'s>>);
    pub struct VerifiedProtection<'m,'s>(std::marker::PhantomData<&'m Maintenance<'s>>);
    #[derive(Debug)] pub struct ContextError;
    type Result<T> = std::result::Result<T,ContextError>;
    #[derive(Debug)] enum MarkerError {{Binding}}
    type MarkerResult<T> = std::result::Result<T,MarkerError>;
    #[derive(PartialEq)] struct Identity;
    impl Identity{{fn from_parts(_:u64,_:u64)->Self{{Self}}}}
    #[derive(Debug, Clone, Copy, PartialEq, Eq)] enum Point {{ FileSync, Published, DirectorySync, FinalSync }}
    {policy}
    {context}
    {authorization}
    {marker}
    impl<'m,'s> RestoreContext<'m,'s> {{
        pub fn authorize(&mut self)->Result<AuthorizedRestore<'_,'m,'s>> {{ unimplemented!() }}
    }}
    {publish}
    pub mod tauri {{ pub struct AppHandle; }}
    {outer}
    {inner}
    #[path={projected:?}] pub mod write_preparation;
}}
use owner::{{RestoreContext,PreparedRestore}};
use owner::write_preparation::{{PendingWrite,DurableWritePreparation}};
use owner::write_preparation::active_close::{{WriteInputs,WriteEpochInputs,EpochVerificationView,PreparedWriteEpisode}};
use database_state::recovery_session::RecoverySession;
fn consume<'m,'s>(c:RestoreContext<'m,'s>,s:RecoverySession<'m,'s>)->PendingWrite<'m,'s>{{
    c.publish_write_preparation().unwrap().prepare_write(s).unwrap()
}}
fn context_for_probe<'m,'s>(_:&'m owner::Maintenance<'s>)->RestoreContext<'m,'s>{{unimplemented!()}}
fn session_for_probe<'m,'s>()->RecoverySession<'m,'s>{{unimplemented!()}}
"#
    );
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let uuid = fs::read_dir(&deps)
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
    let rusqlite = fs::read_dir(&deps)
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
    let compile = |probe: &str| {
        let source = dir.path().join("probe.rs");
        fs::write(&source, format!("{common}\n{probe}")).unwrap();
        std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--cfg=test",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("--extern")
            .arg(format!("uuid={}", uuid.display()))
            .arg("--extern")
            .arg(format!("rusqlite={}", rusqlite.display()))
            .arg(source)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        r#"
fn c_close_valid<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->PreparedWriteEpisode<'m,'s>{owner::write_preparation::active_close::close_c_for_probe(p).unwrap()}
fn b_close_valid<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->PreparedWriteEpisode<'m,'s>{owner::write_preparation::active_close::close_b_for_probe(p).unwrap()}
fn episode_valid<'m,'s>(p:WriteEpochInputs<'m,'s>)->PreparedWriteEpisode<'m,'s>{p.prepare_episode().unwrap()}
fn view_valid(p:&WriteEpochInputs<'_,'_>){p.with_verification(|v|{let _=(v.generation(),v.session_id(),v.operation_id(),v.pending_bytes().len());});}
fn epoch_valid<'m,'s>(p:WriteInputs<'m,'s>)->WriteEpochInputs<'m,'s>{p.into_epoch_inputs()}
fn close_valid<'m,'s>(p:PendingWrite<'m,'s>)->WriteInputs<'m,'s>{p.close_active().unwrap()}
fn valid<'m,'s>(c:RestoreContext<'m,'s>,s:RecoverySession<'m,'s>)->PendingWrite<'m,'s>{consume(c,s)}
fn scoped(app:&owner::tauri::AppHandle,c:PreparedRestore){
    owner::with_restore_context(app,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|{drop(consume(c,session_for_probe()));Ok(())}).unwrap();
}
"#,
    );
    assert!(
        positive.status.success(),
        "positive probe: {}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (name, probe, reason) in [
        (
            "C close private",
            "fn bad(p:PreparedWriteEpisode<'_ ,'_>){let _=p.close_c();}",
            "E0624",
        ),
        (
            "C close once",
            "fn bad(p:PreparedWriteEpisode<'_ ,'_>){let _=owner::write_preparation::active_close::close_c_for_probe(p);drop(p);}",
            "E0382",
        ),
        (
            "C close no ready",
            "fn bad(p:PreparedWriteEpisode<'_ ,'_>){let p=owner::write_preparation::active_close::close_c_for_probe(p).unwrap();p.into_ready();}",
            "E0599",
        ),
        (
            "C close no completion",
            "fn bad(p:PreparedWriteEpisode<'_ ,'_>){let p=owner::write_preparation::active_close::close_c_for_probe(p).unwrap();p.completion_proof();}",
            "E0599",
        ),
        (
            "B close creates no Ready authority",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let p=owner::write_preparation::active_close::close_b_for_probe(p).unwrap();let _=p.into_ready();}",
            "E0599",
        ),
        (
            "B close creates no CompletionProof",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let p=owner::write_preparation::active_close::close_b_for_probe(p).unwrap();let _=p.completion_proof();}",
            "E0599",
        ),
        (
            "B close consumes episode exactly once",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let _=owner::write_preparation::active_close::close_b_for_probe(p);drop(p);}",
            "E0382",
        ),
        (
            "B close stays private",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let _=p.close_b();}",
            "E0624",
        ),
        (
            "move real borrowed Maintenance shape",
            "fn bad<'s>(m:owner::Maintenance<'s>){let c=context_for_probe(&m);let p=consume(c,session_for_probe()).close_active().unwrap().into_epoch_inputs().prepare_episode().unwrap();drop(m);drop(p);}",
            "E0505",
        ),
        (
            "mutable Maintenance before borrowed chain ends",
            "fn bad<'s>(mut m:owner::Maintenance<'s>){let c=context_for_probe(&m);let p=consume(c,session_for_probe()).close_active().unwrap().into_epoch_inputs().prepare_episode().unwrap();let _=&mut m;drop(p);}",
            "E0502",
        ),
        (
            "prepared Maintenance lifetime",
            "fn bad<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->PreparedWriteEpisode<'static,'s>{p}",
            "lifetime may not live long enough",
        ),
        (
            "episode private construction",
            "fn bad(){let _=PreparedWriteEpisode{inputs:panic!()};}",
            "E0451",
        ),
        (
            "episode cannot clone",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "episode no extraction",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){drop(p.inputs);}",
            "E0616",
        ),
        (
            "epoch consumed by preparation",
            "fn bad(p:WriteEpochInputs<'_,'_>){let _=p.prepare_episode();drop(p);}",
            "E0382",
        ),
        (
            "episode cannot classify",
            "fn bad(p:PreparedWriteEpisode<'_,'_>){let _=p.classify_session();}",
            "E0599",
        ),
        (
            "episode state lifetime",
            "fn bad<'m,'s>(p:PreparedWriteEpisode<'m,'s>)->PreparedWriteEpisode<'m,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "view outlives owner",
            "fn bad<'a>(p:WriteEpochInputs<'_,'_>)->EpochVerificationView<'a>{p.with_verification(|v|v).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "view outer callback escape",
            "fn bad<'a>(a:&owner::tauri::AppHandle,c:PreparedRestore)->EpochVerificationView<'a>{owner::with_restore_context(a,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|{let e=consume(c,session_for_probe()).close_active().unwrap().into_epoch_inputs();Ok(e.with_verification(|v|v).unwrap())}).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "view escape",
            "fn bad<'a>(p:&WriteEpochInputs<'_,'_>)->EpochVerificationView<'a>{p.with_verification(|v|v).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "bytes escape",
            "fn bad<'a>(p:&WriteEpochInputs<'_,'_>)->&'a [u8]{p.with_verification(|v|v.pending_bytes()).unwrap()}",
            "E0515",
        ),
        (
            "consume borrowed owner",
            "fn bad(p:WriteEpochInputs<'_,'_>){p.with_verification(|v|{drop(p);let _=v.generation();});}",
            "E0505",
        ),
        (
            "view private",
            "fn bad(){let _=EpochVerificationView{pending_bytes:&[],session_id:panic!(),operation_id:panic!(),generation:0};}",
            "E0451",
        ),
        (
            "view no extraction",
            "fn bad(v:EpochVerificationView<'_>){let _=v.file();}",
            "E0599",
        ),
        (
            "view no clone",
            "fn bad(v:EpochVerificationView<'_>){let _=v.clone();}",
            "E0599",
        ),
        (
            "epoch consumes inputs",
            "fn bad(p:WriteInputs<'_,'_>){let _=p.into_epoch_inputs();drop(p);}",
            "E0382",
        ),
        (
            "epoch not clone",
            "fn bad(p:WriteEpochInputs<'_,'_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "epoch private factory",
            "fn bad(){let _=WriteEpochInputs{inputs:panic!()};}",
            "E0451",
        ),
        (
            "epoch no extraction",
            "fn bad(p:WriteEpochInputs<'_,'_>){drop(p.inputs);}",
            "E0616",
        ),
        (
            "epoch no SQL",
            "fn bad(p:WriteEpochInputs<'_,'_>){let _=p.connection();}",
            "E0599",
        ),
        (
            "epoch reuse",
            "fn bad(p:WriteEpochInputs<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "epoch state lifetime",
            "fn bad<'m,'s>(p:WriteEpochInputs<'m,'s>)->WriteEpochInputs<'m,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "epoch maintenance lifetime",
            "fn bad<'m,'s>(p:WriteEpochInputs<'m,'s>)->WriteEpochInputs<'static,'s>{p}",
            "lifetime may not live long enough",
        ),
        (
            "epoch callback escape",
            "fn bad<'m,'s>(a:&owner::tauri::AppHandle,c:PreparedRestore)->WriteEpochInputs<'m,'s>{owner::with_restore_context(a,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|Ok(consume(c,session_for_probe()).close_active().unwrap().into_epoch_inputs())).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "reuse after A close",
            "fn bad(p:PendingWrite<'_,'_>){let _=p.close_active();drop(p);}",
            "E0382",
        ),
        (
            "duplicate inputs",
            "fn bad(p:WriteInputs<'_,'_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "free inputs",
            "fn bad(){let _=WriteInputs{owner:panic!()};}",
            "E0451",
        ),
        (
            "reuse inputs",
            "fn bad(p:WriteInputs<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "extend closed owner",
            "fn bad<'m,'s>(p:WriteInputs<'m,'s>)->WriteInputs<'m,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "closed callback escape",
            "fn bad<'m,'s>(a:&owner::tauri::AppHandle,c:PreparedRestore)->WriteInputs<'m,'s>{owner::with_restore_context(a,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|Ok(consume(c,session_for_probe()).close_active().unwrap())).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "reuse context",
            "fn bad(c:RestoreContext<'_,'_>){let _=c.publish_write_preparation();drop(c);}",
            "E0382",
        ),
        (
            "reuse owner",
            "fn bad(p:PendingWrite<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "reuse session",
            "fn bad<'m,'s>(p:DurableWritePreparation<'m,'s>,s:RecoverySession<'m,'s>){let _=p.prepare_write(s);drop(s);}",
            "E0382",
        ),
        (
            "clone",
            "fn bad(p:PendingWrite<'_,'_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "free pending",
            "fn bad(){let _=PendingWrite{owner:panic!()};}",
            "E0451",
        ),
        (
            "free durable preparation",
            "fn bad(){let _=DurableWritePreparation{owner:panic!()};}",
            "E0451",
        ),
        (
            "missing session",
            "fn bad(p:DurableWritePreparation<'_,'_>){let _=p.prepare_write();}",
            "E0061",
        ),
        (
            "flag instead of session",
            "fn bad(p:DurableWritePreparation<'_,'_>){let _=p.prepare_write(true);}",
            "E0308",
        ),
        (
            "callback escape",
            "fn bad<'m,'s>(a:&owner::tauri::AppHandle,c:PreparedRestore)->PendingWrite<'m,'s>{owner::with_restore_context(a,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|Ok(consume(c,session_for_probe()))).unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "extend state",
            "fn bad<'m,'s>(p:PendingWrite<'m,'s>)->PendingWrite<'m,'static>{p}",
            "lifetime may not live long enough",
        ),
        (
            "extend maintenance",
            "fn bad<'m,'s>(p:PendingWrite<'m,'s>)->PendingWrite<'static,'s>{p}",
            "lifetime may not live long enough",
        ),
    ] {
        let output = compile(probe);
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{name} unexpectedly accepted");
        assert!(
            diagnostic.contains(reason),
            "{name}: expected {reason}: {diagnostic}"
        );
    }
}

#[test]
fn source_phase_compiler_checks_actual_resource_variants() {
    let source = include_str!("../../write_preparation/active_close.rs");
    // Real declarations and borrow method. Only adjacent backend resources are
    // opaque stubs; no runtime closed resources are fabricated by this harness.
    let context_source = include_str!("../../../../restore_context.rs");
    let common = format!(
        r#"
        mod model {{
            struct PreparedRestore;
            struct PreparedCloseFailure;
            struct ClosedPreparedRestore;
            struct VerifiedProtection<'m,'s>(std::marker::PhantomData<(&'m (), &'s ())>);
            struct ProtectionRecord<'m,'s>(std::marker::PhantomData<(&'m (), &'s ())>);
            enum ActiveCloseError {{ Binding }}
            {}
            {}
            {}
            {}
            {}
            {}
            {}
            {}
            INNER_PROBE
        }}
        OUTER_PROBE
    "#,
        declaration(source, "struct LiveSources"),
        declaration(source, "enum SourcePhase"),
        declaration(source, "impl<'m, 's> SourcePhase"),
        declaration(context_source, "struct TerminalPolicy"),
        declaration(context_source, "enum TerminalPhase"),
        declaration(context_source, "impl TerminalPolicy"),
        declaration(source, "struct BTransferGuard"),
        declaration(source, "impl Drop for BTransferGuard")
    );
    let dir = tempfile::TempDir::new().unwrap();
    let compile = |inside: &str, outside: &str| {
        let path = dir.path().join("phase.rs");
        fs::write(
            &path,
            common
                .replace("INNER_PROBE", inside)
                .replace("OUTER_PROBE", outside),
        )
        .unwrap();
        std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-A",
                "warnings",
            ])
            .arg(&path)
            .arg("-o")
            .arg(dir.path().join("phase.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        "fn valid<'m,'s>(b:PreparedRestore,c:VerifiedProtection<'m,'s>)->SourcePhase<'m,'s>{SourcePhase::Live(LiveSources{incoming:Some(b),protection:Some(c)})} fn guarded<'p>(policy:&'p TerminalPolicy,b:PreparedRestore)->BTransferGuard<'p>{BTransferGuard{terminal_policy:policy,incoming:Some(b),outcome:None}}",
        "",
    );
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    for (inside, outside, reason) in [
        (
            "fn bad(g:BTransferGuard<'_>){let _=g.clone();}",
            "",
            "E0599",
        ),
        (
            "fn require_copy<T:Copy>(){} fn bad(){require_copy::<BTransferGuard<'_>>();}",
            "",
            "E0277",
        ),
        (
            "fn bad(g:BTransferGuard<'_>){drop(g);drop(g);}",
            "",
            "E0382",
        ),
        (
            "fn bad(g:BTransferGuard<'_>)->Option<PreparedRestore>{g.incoming}",
            "",
            "E0509",
        ),
        (
            "fn bad(g:BTransferGuard<'_>)->Option<Result<ClosedPreparedRestore,PreparedCloseFailure>>{g.outcome}",
            "",
            "E0509",
        ),
        (
            "fn bad(g:BTransferGuard<'_>){g.into_outcome();}",
            "",
            "E0599",
        ),
        ("fn bad(g:BTransferGuard<'_>){g.take();}", "", "E0599"),
        ("fn bad(g:BTransferGuard<'_>){g.try_close();}", "", "E0599"),
        (
            "fn bad(p:&TerminalPolicy,b:PreparedRestore){let _=BTransferGuard::new(p,b);}",
            "",
            "E0599",
        ),
        (
            "fn bad(g:BTransferGuard<'_>){g.into_completion_proof();}",
            "",
            "E0599",
        ),
        (
            "fn bad(g:BTransferGuard<'_>){let _:ClosedPreparedRestore=g;}",
            "",
            "E0308",
        ),
        (
            "fn bad<'p>(g:BTransferGuard<'p>)->BTransferGuard<'static>{g}",
            "",
            "lifetime may not live long enough",
        ),
        ("", "fn bad(g:model::BTransferGuard<'_>){drop(g);}", "E0603"),
        (
            "fn bad(p:SourcePhase<'_,'_>){drop(p);drop(p);}",
            "",
            "E0382",
        ),
        (
            "fn bad(p:SourcePhase<'_,'_>){let _=p.clone();}",
            "",
            "E0599",
        ),
        (
            "fn bad<'m,'s>(b:PreparedRestore,c:VerifiedProtection<'m,'s>){let _=SourcePhase::BClosed{incoming:Some(b),protection:Some(c),record:None};}",
            "",
            "E0308",
        ),
        (
            "fn bad(p:SourcePhase<'_,'_>){if let SourcePhase::BClosed{incoming:_,protection,..}=p{let _:ClosedPreparedRestore=protection;}}",
            "",
            "E0308",
        ),
        (
            "fn bad(b:Option<PreparedRestore>){let _:ClosedPreparedRestore=b;}",
            "",
            "E0308",
        ),
        (
            "fn bad(){let _:ClosedPreparedRestore=None::<PreparedRestore>;}",
            "",
            "E0308",
        ),
        (
            "fn bad(p:SourcePhase<'_,'_>){let _:ClosedPreparedRestore=p;}",
            "",
            "E0308",
        ),
        (
            "fn bad(p:SourcePhase<'_,'_>){p.into_completion_proof();}",
            "",
            "E0599",
        ),
        (
            "fn bad<'m,'s>(p:SourcePhase<'m,'s>)->SourcePhase<'static,'s>{p}",
            "",
            "lifetime may not live long enough",
        ),
        ("", "fn bad(p:model::SourcePhase<'_,'_>){drop(p);}", "E0603"),
    ] {
        let result = compile(inside, outside);
        assert!(!result.status.success());
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(
            diagnostic.contains(reason),
            "expected {reason}: {diagnostic}"
        );
    }
}
