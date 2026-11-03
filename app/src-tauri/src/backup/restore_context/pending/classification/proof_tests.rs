use super::super::tests::{Fixture, flag, tree};
use super::super::{classify, classify_session};
use super::*;
use crate::backup::restore_context::with_active;
use crate::{backup::restore, database_state::DatabaseState};
use std::{fs, path::Path, process::Command};
const Q: &str = ".mno-restore-completion-v1.json";
const P: &str = "recovery/pending-restore.json";
fn receipt(f: &Fixture, outcome: &str) {
    fs::write(
        f.root.join(Q),
        serde_json::to_vec(&serde_json::json!({
            "format":"mno-restore-completion","version":1,
            "pending_json":fs::read_to_string(f.root.join(P)).unwrap(),
            "verified_outcome":outcome
        }))
        .unwrap(),
    )
    .unwrap();
}
fn startup(f: &Fixture) -> DatabaseState {
    let state = DatabaseState::default();
    assert_eq!(
        state.initialize_legacy_startup(&f.root).unwrap_err(),
        "database_recovery_artifact_present"
    );
    state
}
#[test]
fn old_new_proofs_keep_session_ids_restore_ids_all_pq_shapes_and_snapshots() {
    for shape in 0..4 {
        for (title, outcome, status) in [("Old", "old", Status::Old), ("New", "new", Status::New)] {
            let f = Fixture::new();
            f.title(title);
            let pending = fs::read(f.root.join(P)).unwrap();
            let data = super::super::parse(&pending).unwrap();
            if shape != 0 {
                receipt(&f, outcome);
            }
            if shape >= 2 {
                fs::remove_file(f.root.join(P)).unwrap();
            }
            if shape == 2 {
                fs::remove_dir(f.root.join("recovery")).unwrap();
            }
            let before = tree(&f.root);
            let state = startup(&f);
            let session = state.recovery_session().unwrap();
            let session_id = session.session_id();
            let proof = classify_session(session, flag())
                .into_completion_proof()
                .unwrap();
            assert_eq!(proof.assessment.status(), status);
            assert_eq!(
                state.recovery_session().err().unwrap(),
                "database_recovery_busy"
            );
            let observation = proof.assessment.observation.as_ref().unwrap();
            assert_eq!(observation.binding.restore_id.to_string(), data.restore_id);
            assert_eq!(observation.binding.directory, data.directory_identity);
            assert_eq!(observation.binding.database, data.database_identity);
            assert_eq!(observation.files.evidence.bytes, pending);
            match proof.assessment.access.as_ref().unwrap() {
                Access::Session(session) => {
                    assert_eq!(session.session_id(), session_id);
                    assert_eq!(session.maintenance_binding(), None);
                }
                _ => panic!("proof lost session"),
            }
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
            assert!(state.operation(None).is_err());
            assert_eq!(tree(&f.root), before);
            let writer = rusqlite::Connection::open(&f.path).unwrap();
            writer.busy_timeout(std::time::Duration::ZERO).unwrap();
            writer
                .execute_batch(
                    "BEGIN IMMEDIATE; UPDATE journal_entries SET title='pending external write'",
                )
                .unwrap();
            assert!(writer.execute_batch("COMMIT").is_err());
            // The proof owns the same read reservation, not just its hash.
            drop(proof);
            // Dropping a proof releases the slot, never startup readiness.
            drop(state.recovery_session().unwrap());
            writer.execute_batch("ROLLBACK").unwrap();
            drop(writer);
            assert_eq!(tree(&f.root), before);
            assert!(state.operation(None).is_err());
            assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        }
    }
}
#[test]
fn identical_fingerprints_preserve_verified_outcome_in_proof() {
    for shape in 0..4 {
        for outcome in ["old", "new"] {
            if shape == 0 && outcome == "new" {
                continue;
            }
            let f = Fixture::new();
            let path = f.root.join(P);
            let mut data: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            data["new_content"] = data["old_content"].clone();
            fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
            if shape != 0 {
                receipt(&f, outcome);
            }
            if shape >= 2 {
                fs::remove_file(f.root.join(P)).unwrap();
            }
            if shape == 2 {
                fs::remove_dir(f.root.join("recovery")).unwrap();
            }
            let state = startup(&f);
            let proof = classify_session(state.recovery_session().unwrap(), flag())
                .into_completion_proof()
                .unwrap();
            assert_eq!(
                proof.assessment.status(),
                if outcome == "new" {
                    Status::New
                } else {
                    Status::Old
                }
            );
            drop(proof);
            assert!(state.operation(None).is_err());
        }
    }
}
#[test]
fn uncertain_diagnostic_and_not_needed_rejections_retain_original_guards() {
    let f = Fixture::new();
    f.title("Neither");
    let state = startup(&f);
    let before = tree(&f.root);
    let rejected = classify_session(state.recovery_session().unwrap(), flag())
        .into_completion_proof()
        .err()
        .unwrap();
    assert!(matches!(rejected.reason(), ProofRejection::Uncertain(_)));
    assert_eq!(
        state.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    let a = rejected.into_assessment();
    assert_eq!(a.status(), Status::Uncertain);
    drop(a);
    assert!(state.operation(None).is_err());
    assert_eq!(tree(&f.root), before);
    drop(state);
    f.title("Old");
    let rejected = classify(&f.root, flag())
        .into_completion_proof()
        .err()
        .unwrap();
    assert_eq!(rejected.reason(), ProofRejection::SessionRequired);
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    drop(rejected);
    fs::remove_file(f.root.join(P)).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let guard = state.maintenance().unwrap();
    let generation = guard.generation();
    let rejected = classify_session(guard.recovery_session().unwrap(), flag())
        .into_completion_proof()
        .err()
        .unwrap();
    assert_eq!(rejected.reason(), ProofRejection::NotNeeded);
    assert_eq!(
        guard.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert_eq!(guard.generation(), generation);
    drop(rejected);
    assert!(guard.ensure_usable().is_err());
    drop(guard);
    assert!(state.operation(None).is_err());
}
#[test]
fn changed_inputs_between_assessment_and_conversion_reject_without_losing_guards() {
    for case in 0..10 {
        let f = Fixture::new();
        receipt(&f, "old");
        let state = startup(&f);
        let a = classify_session(state.recovery_session().unwrap(), flag());
        assert_eq!(a.status(), Status::Old);
        match case {
            0 => f.change_marker(
                "restore_id",
                serde_json::json!(uuid::Uuid::new_v4().to_string()),
            ),
            1 => {
                let p = f.root.join(P);
                let mut b = fs::read(&p).unwrap();
                b.push(b' ');
                fs::write(p, b).unwrap();
            }
            2 => {
                let q = f.root.join(Q);
                let mut b = fs::read(&q).unwrap();
                b.push(b' ');
                fs::write(q, b).unwrap();
            }
            3 => {
                fs::write(&f.protection, b"changed archive").unwrap();
            }
            4 => {
                fs::rename(&f.path, f.root.join("original.sqlite")).unwrap();
                fs::copy(f.root.join("original.sqlite"), &f.path).unwrap();
            }
            5 => {
                fs::rename(&f.root, f.root.with_extension("moved")).unwrap();
                fs::create_dir(&f.root).unwrap();
            }
            6 => {
                fs::write(f.root.join("recovery/unknown.tmp"), b"partial").unwrap();
            }
            7 => {
                let q = f.root.join(Q);
                fs::rename(&q, f.root.join("saved-q")).unwrap();
                std::os::unix::fs::symlink("saved-q", q).unwrap();
            }
            8 => {
                let q = f.root.join(Q);
                let bytes = fs::read(&q).unwrap();
                fs::rename(&q, f.root.join("saved-q")).unwrap();
                fs::write(q, bytes).unwrap();
            }
            _ => {
                // A raw writer bypasses SQLite locking. The retained physical
                // identity is unchanged; the conversion must detect changed bytes.
                use std::io::Write;
                let mut raw = fs::OpenOptions::new().append(true).open(&f.path).unwrap();
                raw.write_all(b"changed after assessment").unwrap();
            }
        }
        let before = tree(&f.root);
        let rejected = a.into_completion_proof().err().unwrap();
        assert!(
            matches!(rejected.reason(), ProofRejection::Invalidated(_)),
            "case {case}"
        );
        assert!(state.operation(None).is_err());
        assert_eq!(tree(&f.root), before);
        drop(rejected);
        assert!(state.operation(None).is_err());
        assert_eq!(tree(&f.root), before);
    }
}
#[test]
fn runtime_proof_retains_original_maintenance_generation_and_operation() {
    let f = Fixture::new();
    fs::remove_file(f.root.join(P)).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let candidate =
        restore::prepare_candidate(&f._dir.path().join("incoming.mno-backup"), flag()).unwrap();
    with_active(&state, &f.path, candidate, flag(), |mut ctx| {
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let before = tree(&f.root);
        let operation = ctx.maintenance.operation_id();
        let generation = ctx.maintenance.generation();
        let session = ctx.maintenance.recovery_session().unwrap();
        let session_id = session.session_id();
        let proof = classify_session(session, flag())
            .into_completion_proof()
            .unwrap();
        match proof.assessment.access.as_ref().unwrap() {
            Access::Session(session) => {
                assert_eq!(session.session_id(), session_id);
                assert_eq!(session.maintenance_binding(), Some((operation, generation)));
            }
            _ => panic!("lost running session"),
        }
        assert_eq!(
            proof
                .assessment
                .observation
                .as_ref()
                .unwrap()
                .binding
                .restore_id,
            operation
        );
        drop(proof);
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(tree(&f.root), before);
        Ok(())
    })
    .unwrap();
    assert!(state.operation(None).is_err());
}

#[test]
fn cancelled_conversion_returns_the_guarded_assessment() {
    let f = Fixture::new();
    let state = startup(&f);
    let before = tree(&f.root);
    let cancel = flag();
    let a = classify_session(state.recovery_session().unwrap(), cancel.clone());
    assert_eq!(a.status(), Status::Old);
    cancel.store(true, std::sync::atomic::Ordering::Release);
    let rejected = a.into_completion_proof().err().unwrap();
    assert_eq!(
        rejected.reason(),
        ProofRejection::Invalidated(Reason::Cancelled)
    );
    let writer = rusqlite::Connection::open(&f.path).unwrap();
    writer.busy_timeout(std::time::Duration::ZERO).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='New'")
        .unwrap();
    assert!(writer.execute_batch("COMMIT").is_err());
    drop(rejected);
    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);
    assert!(state.operation(None).is_err());
    assert_eq!(tree(&f.root), before);
}
#[test]
fn cancellation_after_first_reverification_block_keeps_all_recovery_guards() {
    use std::sync::atomic::Ordering;
    use std::{cell::Cell, os::unix::fs::MetadataExt, rc::Rc};

    let f = Fixture::new();
    fs::remove_file(f.root.join(P)).unwrap();
    fs::remove_dir(f.root.join("recovery")).unwrap();
    // More than one hash block: cancellation must interrupt the active stream,
    // not merely be noticed by a later check after a complete one-block hash.
    let document = serde_json::json!({"type":"doc","content":[{
        "type":"paragraph","content":[{"type":"text","text":"x".repeat(256 * 1024)}]
    }]});
    let db = rusqlite::Connection::open(&f.path).unwrap();
    db.execute(
        "UPDATE journal_entries SET content_json=?1",
        [document.to_string()],
    )
    .unwrap();
    drop(db);
    let state = DatabaseState::default();
    state.initialize_legacy_startup(&f.root).unwrap();
    let candidate =
        restore::prepare_candidate(&f._dir.path().join("incoming.mno-backup"), flag()).unwrap();
    with_active(&state, &f.path, candidate, flag(), |mut ctx| {
        let auth = ctx.authorize()?;
        drop(auth.publish_pending().unwrap());
        drop(auth);
        let generation = ctx.maintenance.generation();
        let operation = ctx.maintenance.operation_id();
        let session = ctx.maintenance.recovery_session().unwrap();
        let session_id = session.session_id();
        let cancel = flag();
        // Install the hook AFTER classification: it can only observe the
        // conversion's re-verification, never the initial verification.
        let assessment = classify_session(session, cancel.clone());
        assert_eq!(assessment.status(), Status::Old);
        let before = tree(&f.root);
        let metadata = fs::metadata(&f.path).unwrap();
        let expected_identity = (metadata.dev(), metadata.ino());
        let file_len = metadata.len();
        assert!(file_len > 64 * 1024);
        let converting = Rc::new(Cell::new(false));
        let blocks = Rc::new(Cell::new(0u32));
        let bytes = Rc::new(Cell::new(0u64));
        let in_conversion = converting.clone();
        let seen_blocks = blocks.clone();
        let seen_bytes = bytes.clone();
        let interrupt = cancel.clone();
        let result = super::super::hash_blocks::with_observer(
            move |file, processed| {
                assert!(in_conversion.get(), "observer must run inside conversion");
                let actual = file.metadata().unwrap();
                assert_eq!((actual.dev(), actual.ino()), expected_identity);
                // Called only AFTER real read_at + Sha256::update for this block.
                assert_eq!(processed, 64 * 1024);
                assert!(processed < file_len, "unread blocks must remain");
                assert!(!interrupt.load(Ordering::Acquire));
                seen_blocks.set(seen_blocks.get() + 1);
                seen_bytes.set(processed);
                interrupt.store(true, Ordering::Release);
            },
            || {
                converting.set(true);
                let result = assessment.into_completion_proof();
                converting.set(false);
                result
            },
        );
        assert!(!converting.get());
        assert_eq!(
            blocks.get(),
            1,
            "the first block hook must have run exactly once"
        );
        assert_eq!(bytes.get(), 64 * 1024);
        assert!(bytes.get() < file_len);
        assert!(cancel.load(Ordering::Acquire));
        let rejected = result
            .err()
            .expect("interrupted stream must not produce CompletionProof");
        assert_eq!(
            rejected.reason(),
            ProofRejection::Invalidated(Reason::Cancelled)
        );
        match rejected.assessment.access.as_ref().unwrap() {
            Access::Session(session) => {
                assert_eq!(session.session_id(), session_id);
                assert_eq!(session.maintenance_binding(), Some((operation, generation)));
            }
            _ => panic!("rejection lost the original session"),
        }
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        assert_eq!(tree(&f.root), before);
        // An independent SQLite writer proves the rejected result still owns
        // its read reservation, rather than just retaining an identity/hash.
        let writer = rusqlite::Connection::open(&f.path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE journal_entries SET title='uncommitted'")
            .unwrap();
        assert!(writer.execute_batch("COMMIT").is_err());
        drop(rejected);
        writer
            .execute_batch("ROLLBACK; BEGIN EXCLUSIVE; ROLLBACK")
            .unwrap();
        drop(writer);
        assert!(ctx.maintenance.ensure_usable().is_err());
        assert_eq!(ctx.maintenance.generation(), generation);
        assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
        assert_eq!(tree(&f.root), before);
        Ok(())
    })
    .unwrap();
    assert!(state.operation(None).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
}

#[test]
fn compiler_enforces_completion_capability_consumption_and_serde_boundaries() {
    let dir = tempfile::TempDir::new().unwrap();
    let api = super::super::capability_probe(dir.path());
    let common = format!(
        r#"
        mod classification {{
            pub(super) mod completion {{ #[derive(Debug)] pub struct ReceiptError; }}
            pub(super) mod durability {{}}
            pub(super) struct SyncPoint;
            pub(super) struct Observation;
            pub(super) struct Access<'a, 's>(&'a (), &'s ());
            impl Access<'_, '_> {{fn is_session(&self)->bool{{true}}}}
            impl Observation {{fn recheck(&self,_:&Access<'_, '_>)->Result<(),result::Reason>{{Ok(())}}}}
            #[path={api:?}] pub(crate) mod result;
            pub fn verified(owner:&())->result::Assessment<'_, '_>{{
                result::Assessment::verified(result::VerifiedStatus::Old,Observation).retain(Access(owner,owner))
            }}
        }}
        use classification::result::{{Assessment,CompletionProof,RejectedAssessment,Status}};
        fn finish(_proof:CompletionProof<'_, '_>) {{}}
    "#
    );
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let mut serde_files: Vec<_> = fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libserde-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .collect();
    serde_files.sort();
    let compile = |probe: &str, serde_path: &Path| {
        let path = dir.path().join("probe.rs");
        fs::write(&path, format!("{common}\n{probe}")).unwrap();
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
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
            .arg(format!("serde={}", serde_path.display()))
            .arg(path)
            .arg("-o")
            .arg(dir.path().join("probe.rmeta"))
            .output()
            .unwrap()
    };
    let positive = r#"
        fn publication(p:CompletionProof<'_, '_>){let _=p.publish_receipt();}
        fn cleanup(d:classification::result::ReceiptDurable<'_, '_>){let _=d.remove_pending();}
        fn close_resources(d:classification::result::RecoveryDirectoryRemoved<'_, '_>){let _=d.close_resources();}
        fn serial<T:serde::Serialize>(){}
        fn deserial<T:serde::de::DeserializeOwned>(){}
        fn serde_control(){serial::<String>();deserial::<String>();}
        fn prepare<'a,'s>(a:Assessment<'a, 's>)->Result<CompletionProof<'a, 's>,RejectedAssessment<'a, 's>>{a.into_completion_proof()}
        fn intended(owner:&()){let proof=prepare(classification::verified(owner)).unwrap();finish(proof);}
        fn rejected(owner:&()){if let Err(e)=prepare(classification::verified(owner)){let _=e.reason();let _=e.into_assessment();}}
    "#;
    // Build caches may contain several feature/profile variants. Select one by
    // a positive real-serde compile, never by accepting a failed negative probe.
    let serde = serde_files
        .into_iter()
        .find(|path| compile(positive, path).status.success())
        .expect("positive proof and real serde controls must compile");
    for (probe, code) in [
        (
            "fn twice(d:classification::result::RecoveryDirectoryRemoved<'_, '_>){let _=d.close_resources();let _=d.close_resources();}",
            "E0382",
        ),
        (
            "fn clone_closed(d:classification::result::ResourcesClosed<'_, '_>){let _=d.clone();}",
            "E0599",
        ),
        (
            "fn clone_failed(d:classification::result::ResourceCloseFailure<'_, '_>){let _=d.clone();}",
            "E0599",
        ),
        (
            "fn copy<T:Copy>(){}fn fake(){copy::<classification::result::ResourcesClosed<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn copy<T:Copy>(){}fn fake(){copy::<classification::result::ResourceCloseFailure<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::ResourcesClosed<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::ResourceCloseFailure<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn forge(){let _:classification::result::ResourcesClosed<'_, '_>=true.into();}",
            "E0277",
        ),
        (
            "fn forge(e:classification::result::ResourceCloseFailure<'_, '_>){let _:classification::result::ResourcesClosed<'_, '_>=e.into();}",
            "E0277",
        ),
        (
            "fn forge(p:classification::result::ResourceCloseProgress){let _:classification::result::ResourcesClosed<'_, '_>=p.into();}",
            "E0277",
        ),
        (
            "fn forge(){let _=classification::result::ResourcesClosed{closed:panic!()};}",
            "E0451",
        ),
        (
            "fn forge(){let _=classification::result::ResourceCloseFailure{retained:panic!(),error:classification::result::ResourceCloseError::Unsupported,progress:classification::result::ResourceCloseProgress::Unchanged};}",
            "E0451",
        ),
        (
            "fn escape(owner:&())->classification::result::ResourcesClosed<'static, 'static>{classification::verified(owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().unwrap().close_resources().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let _d=classification::verified(&owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().unwrap().close_resources().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn early_error_drop(){let owner=Box::new(());let _e=classification::verified(&owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().unwrap().close_resources().err().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn sql(d:classification::result::ResourcesClosed<'_, '_>){let _=d.connection();}",
            "E0599",
        ),
        (
            "fn mutate_owner(){let mut owner=();let p=classification::verified(&owner).into_completion_proof().unwrap();let _exclusive=&mut owner;finish(p);}",
            "E0502",
        ),
        (
            "fn forge(x:Status){let _:CompletionProof<'_, '_>=x.into();}",
            "E0277",
        ),
        (
            "fn forge(x:bool){let _:CompletionProof<'_, '_>=x.into();}",
            "E0277",
        ),
        (
            "struct Fingerprint;fn forge(x:Fingerprint){let _:CompletionProof<'_, '_>=x.into();}",
            "E0277",
        ),
        (
            "struct Metadata;fn forge(x:Metadata){let _:CompletionProof<'_, '_>=x.into();}",
            "E0277",
        ),
        (
            "fn forge(a:Assessment<'_, '_>){let _=CompletionProof{assessment:a};}",
            "E0451",
        ),
        ("fn forge(){let _=CompletionProof::new(true);}", "E0599"),
        (
            "fn fake(p:classification::result::CleanupProgress){let _:classification::result::RecoveryDirectoryRemoved<'_, '_>=p.into();}",
            "E0277",
        ),
        (
            "fn bypass(e:classification::result::ReceiptFailure<'_, '_>){let _:classification::result::RecoveryDirectoryRemoved<'_, '_>=e.into();}",
            "E0277",
        ),
        (
            "fn early_error_drop(){let owner=Box::new(());let _e=classification::verified(&owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().err().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn fake(d:classification::result::ReceiptDurable<'_, '_>){let _=classification::result::CleanupFailure{receipt:d,error:panic!(),progress:classification::result::CleanupProgress::Unchanged};}",
            "E0451",
        ),
        (
            "fn duplicate(e:classification::result::CleanupFailure<'_, '_>){let _=e.clone();}",
            "E0599",
        ),
        (
            "fn twice(d:classification::result::ReceiptDurable<'_, '_>){let _=d.remove_pending();let _=d.remove_pending();}",
            "E0382",
        ),
        (
            "fn fake(d:classification::result::ReceiptDurable<'_, '_>){let _=classification::result::RecoveryDirectoryRemoved{receipt:d};}",
            "E0451",
        ),
        (
            "fn fake(){let _:classification::result::RecoveryDirectoryRemoved<'_, '_>=true.into();}",
            "E0277",
        ),
        (
            "fn duplicate(d:classification::result::RecoveryDirectoryRemoved<'_, '_>){let _=d.clone();}",
            "E0599",
        ),
        (
            "fn copy<T:Copy>(){}fn fake(){copy::<classification::result::RecoveryDirectoryRemoved<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::RecoveryDirectoryRemoved<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::CleanupFailure<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn bypass(e:classification::result::CleanupFailure<'_, '_>){let _:classification::result::RecoveryDirectoryRemoved<'_, '_>=e.into();}",
            "E0277",
        ),
        (
            "fn escape(owner:&())->classification::result::RecoveryDirectoryRemoved<'static, 'static>{classification::verified(owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let _d=classification::verified(&owner).into_completion_proof().unwrap().publish_receipt().unwrap().remove_pending().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn duplicate(p:CompletionProof<'_, '_>){let d=p.publish_receipt().unwrap();let _=d.clone();}",
            "E0599",
        ),
        (
            "fn twice(p:CompletionProof<'_, '_>){let _=p.publish_receipt();let _=p.publish_receipt();}",
            "E0382",
        ),
        (
            "fn forge(p:CompletionProof<'_, '_>){let _=classification::result::ReceiptDurable{proof:Box::new(p)};}",
            "E0451",
        ),
        (
            "fn fake(){let _:classification::result::ReceiptDurable<'_, '_>=true.into();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::ReceiptDurable<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn fake(){decode::<classification::result::ReceiptFailure<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn copy<T:Copy>(){}fn fake(){copy::<classification::result::ReceiptDurable<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn duplicate(e:classification::result::ReceiptFailure<'_, '_>){let _=e.clone();}",
            "E0599",
        ),
        (
            "fn escape(owner:&())->classification::result::ReceiptDurable<'static, 'static>{classification::verified(owner).into_completion_proof().unwrap().publish_receipt().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let _d=classification::verified(&owner).into_completion_proof().unwrap().publish_receipt().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn duplicate(p:CompletionProof<'_, '_>){let _=p.clone();}",
            "E0599",
        ),
        (
            "fn copy<T:Copy>(){}fn forge(){copy::<CompletionProof<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn decode<T:serde::de::DeserializeOwned>(){}fn forge(){decode::<CompletionProof<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn encode<T:serde::Serialize>(){}fn forge(){encode::<CompletionProof<'static, 'static>>();}",
            "E0277",
        ),
        (
            "fn twice(p:CompletionProof<'_, '_>){finish(p);finish(p);}",
            "E0382",
        ),
        (
            "fn twice(a:Assessment<'_, '_>){let _=a.into_completion_proof();let _=a.into_completion_proof();}",
            "E0382",
        ),
        (
            "fn escape(owner:&())->CompletionProof<'static, 'static>{classification::verified(owner).into_completion_proof().unwrap()}",
            "lifetime may not live long enough",
        ),
        (
            "fn early_drop(){let owner=Box::new(());let _p=classification::verified(&owner).into_completion_proof().unwrap();drop(owner);}",
            "E0505",
        ),
        (
            "fn early_error_drop(){let owner=Box::new(());let _e=classification::verified(&owner).into_completion_proof().err().unwrap();drop(owner);}",
            "E0505",
        ),
    ] {
        let out = compile(probe, &serde);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(code),
            "expected {code}: {stderr}"
        );
    }
}

#[test]
fn assessment_and_rejection_retain_exclusive_reservation_until_drop() {
    let f = Fixture::new();
    let state = startup(&f);
    let before = tree(&f.root);
    let assessment = classify_session(state.recovery_session().unwrap(), flag());
    assert_eq!(assessment.status(), Status::Old);
    assert_eq!(
        state.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    // A deterministic pre-conversion cancellation preserves all borrowed guards.
    assessment
        .observation
        .as_ref()
        .unwrap()
        .cancel
        .store(true, std::sync::atomic::Ordering::Release);
    let rejected = assessment.into_completion_proof().err().unwrap();
    assert_eq!(
        state.recovery_session().err().unwrap(),
        "database_recovery_busy"
    );
    assert!(state.operation(None).is_err());
    assert!(crate::startup::DirectoryLease::open_existing(&f.root).is_err());
    drop(rejected);
    drop(state.recovery_session().unwrap());
    assert!(state.operation(None).is_err());
    assert_eq!(tree(&f.root), before);
}
