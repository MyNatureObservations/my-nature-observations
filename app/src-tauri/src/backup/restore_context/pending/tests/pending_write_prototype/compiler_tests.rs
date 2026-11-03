use super::*;

// Ownership probes compile the actual prototype factory and owner declarations.
// Only the verifier/I/O backends are replaced; real runtime tests cover those.
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
fn pending_write_prototype_compiler_checks_real_factory_ownership_and_callbacks() {
    let context_source = include_str!("../../../../restore_context.rs");
    let pending_source = include_str!("../../../pending.rs");
    let prototype_source = include_str!("../pending_write_prototype.rs");
    let mut prototype = prototype_source
        .split("#[cfg(test)]\n")
        .next()
        .unwrap()
        .to_owned();
    let verifier = declaration(&prototype, "fn verify(");
    prototype = prototype.replace(&verifier, &signature(&verifier, "fn verify("));
    let dir = tempfile::TempDir::new().unwrap();
    let projected = dir.path().join("prototype.rs");
    fs::write(&projected, prototype).unwrap();
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
mod owner {{
    use std::{{fs::File, path::{{Path, PathBuf}}, io, sync::{{Arc, atomic::AtomicBool}}}};
    pub struct DatabaseState;
    pub struct Maintenance<'s>(pub &'s DatabaseState);
    pub struct ActiveDatabase;
    pub struct PreparedRestore;
    pub struct VerifiedProtection<'m,'s>(std::marker::PhantomData<&'m Maintenance<'s>>);
    #[derive(Debug)] pub struct ContextError;
    type Result<T> = std::result::Result<T,ContextError>;
    #[derive(Debug)] struct MarkerError;
    type MarkerResult<T> = std::result::Result<T,MarkerError>;
    struct Identity;
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
    #[path={projected:?}] pub mod prototype;
    pub fn prepare<'m,'s>(context:RestoreContext<'m,'s>)->prototype::PendingWrite<'m,'s>{{
        prototype::prepare(context,&mut |_|Ok(())).unwrap()
    }}
}}
use owner::{{RestoreContext,DatabaseState,Maintenance,PreparedRestore}};
use owner::prototype::PendingWrite;
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
    let compile = |probe: &str| {
        let source = dir.path().join("probe.rs");
        fs::write(&source, format!("{common}\n{probe}")).unwrap();
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
            .arg(format!("uuid={}", uuid.display()))
            .arg(source)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = compile(
        r#"
fn consume<'m,'s>(c:RestoreContext<'m,'s>)->PendingWrite<'m,'s>{owner::prepare(c)}
fn scoped(app:&owner::tauri::AppHandle,c:PreparedRestore){
    owner::with_restore_context(app,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|{drop(owner::prepare(c));Ok(())}).unwrap();
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
            "reuse context",
            "fn bad(c:RestoreContext<'_,'_>){let _p=owner::prepare(c);drop(c);}",
            "E0382",
        ),
        (
            "reuse owner",
            "fn bad(p:PendingWrite<'_,'_>){drop(p);drop(p);}",
            "E0382",
        ),
        (
            "clone owner",
            "fn bad(p:PendingWrite<'_,'_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "free construction",
            "fn bad(){let _=PendingWrite{retained:None};}",
            "E0451",
        ),
        (
            "callback escape",
            "fn bad<'m,'s>(a:&owner::tauri::AppHandle,c:PreparedRestore)->PendingWrite<'m,'s>{owner::with_restore_context(a,c,std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),|c|Ok(owner::prepare(c))).unwrap()}",
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
        (
            "flag is not proof",
            "fn bad(){let _=owner::prepare(true);}",
            "E0308",
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
