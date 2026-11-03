use super::super::tests::{fixture, owner, save, tree};
use super::*;
use crate::database_state::DatabaseState;
use rusqlite::{Connection, OpenFlags, types::Value};
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Output},
};
const CHILD: &str = "backup::restore_context::activation::marker_contract::classification::transition::tests::transition_child";
fn setup(root: &Path, target: Target, installed: bool, park: bool) -> ImportV2 {
    let mut m = fixture(root, false);
    m.target = target;
    if installed {
        fs::copy(root.join("new.sqlite"), root.join("data/mno.sqlite")).unwrap();
    }
    if target == Target::Old && installed && park {
        fs::remove_file(root.join("data/recovery").join(&m.candidate_file)).unwrap();
    }
    save(root, &m);
    m
}
fn rows(path: &Path) -> Vec<Vec<Value>> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let values = {
        let mut q=db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id COLLATE BINARY").unwrap();
        q.query_map([], |r| {
            (0..7)
                .map(|i| r.get(i))
                .collect::<rusqlite::Result<Vec<Value>>>()
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    db.close().unwrap();
    values
}
fn protection(root: &Path, m: &ImportV2) -> Vec<u8> {
    fs::read(
        root.join("data/backups/pre-restore")
            .join(&m.protection_file),
    )
    .unwrap()
}
fn ready(state: &DatabaseState, root: &Path, target: Target) {
    let op = state
        .operation(None)
        .expect("must use real Ready admission");
    assert_eq!(
        rows(&root.join("data/mno.sqlite")),
        rows(&root.join(if target == Target::Old {
            "old.sqlite"
        } else {
            "new.sqlite"
        }))
    );
    drop(op);
}
fn launch(root: &Path, mode: &str, mutation: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("MNO_ACTIVATION_ROOT", root)
        .env("MNO_ACTIVATION_MODE", mode)
        .env("MNO_V2_MUTATION", mutation)
        .output()
        .unwrap()
}
pub(super) fn checkpoint(name: &str) -> Outcome<()> {
    if name != "v2-before-install" {
        return Ok(());
    }
    let Some(root) = std::env::var_os("MNO_ACTIVATION_ROOT") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let data = root.join("data");
    match std::env::var("MNO_V2_MUTATION")
        .unwrap_or_default()
        .as_str()
    {
        "active" => {
            let db = Connection::open(data.join("mno.sqlite"))?;
            db.execute("UPDATE journal_entries SET title='changed'", [])?;
            activation::close(db);
        }
        "candidate" => {
            let m = marker(&data).map_err(error)?;
            let p = data.join("recovery").join(if m.target == Target::Old {
                m.previous_candidate_file
            } else {
                m.candidate_file
            });
            let db = Connection::open(p)?;
            db.execute("UPDATE journal_entries SET title='changed candidate'", [])?;
            activation::close(db);
        }
        "marker" => {
            let mut m = marker(&data).map_err(error)?;
            m.target = if m.target == Target::Old {
                Target::New
            } else {
                Target::Old
            };
            save(&root, &m);
        }
        "sidecar" => fs::write(data.join("mno.sqlite-journal"), b"unsafe")?,
        "rename-error" => {
            fs::set_permissions(data.join("recovery"), fs::Permissions::from_mode(0o500))?
        }
        _ => {}
    }
    Ok(())
}
#[test]
fn transition_child() {
    let Some(root) = std::env::var_os("MNO_ACTIVATION_ROOT") else {
        return;
    };
    // Existing v1 child protocols use a different --exact target, never this one.
    let root = PathBuf::from(root);
    let target = marker(&root.join("data")).unwrap().target;
    let state = owner(&root);
    let result = activation::startup::run_v2_for_test(&state);
    if std::env::var("MNO_V2_MUTATION").as_deref() == Ok("rename-error") {
        fs::set_permissions(
            root.join("data/recovery"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(
            matches!(result,Err(activation::Error::Io(ref e)) if e.kind()==io::ErrorKind::PermissionDenied)
        );
    }
    if let Err(e) = result {
        assert!(state.operation(None).is_err());
        eprintln!("V2_BLOCKED:{e:?}");
        std::process::exit(73);
    }
    ready(&state, &root, target);
}
#[test]
fn new_and_old_complete_via_existing_admission_and_normal_restart() {
    for (target, installed, park) in [
        (Target::New, false, true),
        (Target::Old, true, true),
        (Target::Old, true, false),
        (Target::New, true, false),
        (Target::Old, false, false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, target, installed, park);
        let protected = protection(&root, &m);
        let state = owner(&root);
        activation::startup::run_v2_for_test(&state).unwrap();
        ready(&state, &root, target);
        assert!(!root.join("data/recovery").exists());
        let audit = root.join("data/backups/activations").join(&m.operation_id);
        assert_eq!(
            fs::read(audit.join(activation::MARKER)).unwrap(),
            serde_json::to_vec(&m).unwrap()
        );
        assert_eq!(protection(&root, &m), protected);
        if target == Target::Old {
            assert_eq!(
                rows(&audit.join(&m.candidate_file)),
                rows(&root.join("new.sqlite"))
            );
        }
        drop(state);
        let next = DatabaseState::default();
        next.initialize_legacy_startup(&root.join("data")).unwrap();
        ready(&next, &root, target);
    }
}
const POINTS: [&str; 13] = [
    "v2-target-durable",
    "v2-before-park",
    "v2-after-park",
    "v2-sync-park",
    "v2-before-install",
    "v2-after-install",
    "v2-sync-install",
    "v2-selected-verified",
    "v2-before-retire",
    "v2-after-retire",
    "v2-sync-retire",
    "v2-before-handoff",
    "startup-before-admit",
];
#[test]
fn process_interruptions_resume_only_the_durable_selected_journal() {
    for target in [Target::New, Target::Old] {
        for p in POINTS {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            let m = setup(&root, target, target == Target::Old, true);
            let protected = protection(&root, &m);
            let out = launch(&root, &format!("crash:{p}"), "");
            assert_eq!(
                out.status.code(),
                Some(88),
                "{p}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                fs::read_to_string(root.join("events"))
                    .unwrap()
                    .lines()
                    .last(),
                Some(p)
            );
            assert_eq!(protection(&root, &m), protected);
            // Real startup always precedes any diagnostic read of active SQLite.
            let state = DatabaseState::default();
            let started = state.initialize_legacy_startup(&root.join("data"));
            if [
                "v2-after-retire",
                "v2-sync-retire",
                "v2-before-handoff",
                "startup-before-admit",
            ]
            .contains(&p)
            {
                started.unwrap();
            } else {
                assert!(started.is_err());
                assert!(state.operation(None).is_err());
                activation::startup::run_v2_for_test(&state).unwrap();
            }
            ready(&state, &root, target);
            assert_eq!(protection(&root, &m), protected);
        }
    }
}
#[test]
fn repeated_interruptions_do_not_lose_either_journal() {
    for target in [Target::New, Target::Old] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, target, target == Target::Old, true);
        let protected = protection(&root, &m);
        for p in [
            "v2-after-park",
            "v2-after-install",
            "v2-selected-verified",
            "v2-before-retire",
        ] {
            let out = launch(&root, &format!("crash:{p}"), "");
            assert_eq!(
                out.status.code(),
                Some(88),
                "{p}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                fs::read_to_string(root.join("events"))
                    .unwrap()
                    .lines()
                    .last(),
                Some(p)
            );
            assert_eq!(protection(&root, &m), protected);
        }
        let state = owner(&root);
        activation::startup::run_v2_for_test(&state).unwrap();
        ready(&state, &root, target);
        assert_eq!(protection(&root, &m), protected);
    }
}
#[test]
fn injected_errors_keep_recovery_and_retries_observe_actual_files() {
    for target in [Target::New, Target::Old] {
        for p in POINTS {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            let m = setup(&root, target, target == Target::Old, true);
            let protected = protection(&root, &m);
            let out = launch(&root, &format!("fail:{p}"), "");
            assert_eq!(
                out.status.code(),
                Some(73),
                "{p}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(String::from_utf8_lossy(&out.stderr).contains("V2_BLOCKED:"));
            assert_eq!(protection(&root, &m), protected);
            let state = DatabaseState::default();
            if state.initialize_legacy_startup(&root.join("data")).is_err() {
                assert!(state.operation(None).is_err());
                activation::startup::run_v2_for_test(&state).unwrap();
            }
            ready(&state, &root, target);
        }
    }
}
#[test]
fn changed_files_sidecars_and_real_rename_error_cannot_admit() {
    for mutation in ["active", "candidate", "marker", "sidecar", "rename-error"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, Target::Old, true, false);
        let protected = protection(&root, &m);
        let out = launch(&root, "", mutation);
        assert_eq!(
            out.status.code(),
            Some(73),
            "{mutation}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("V2_BLOCKED:"));
        assert!(root.join("data/recovery").exists());
        assert_eq!(protection(&root, &m), protected);
        let state = owner(&root);
        assert!(state.operation(None).is_err());
        // The other new journal remains independently present even for a tampered active file.
        assert_eq!(
            rows(&root.join("data/recovery").join(&m.candidate_file)),
            rows(&root.join("new.sqlite"))
        );
    }
}
#[test]
fn missing_invalid_or_ambiguous_files_block_without_mutation() {
    for mode in [
        "active-missing",
        "active-corrupt",
        "candidate-missing",
        "candidate-corrupt",
        "protection-missing",
        "protection-corrupt",
        "wal",
        "unknown-file",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let m = setup(&root, Target::New, false, true);
        match mode {
            "active-missing" => fs::remove_file(root.join("data/mno.sqlite")).unwrap(),
            "active-corrupt" => fs::write(root.join("data/mno.sqlite"), b"broken").unwrap(),
            "candidate-missing" => {
                fs::remove_file(root.join("data/recovery").join(&m.candidate_file)).unwrap()
            }
            "candidate-corrupt" => fs::write(
                root.join("data/recovery").join(&m.candidate_file),
                b"broken",
            )
            .unwrap(),
            "protection-missing" => fs::remove_file(
                root.join("data/backups/pre-restore")
                    .join(&m.protection_file),
            )
            .unwrap(),
            "protection-corrupt" => fs::write(
                root.join("data/backups/pre-restore")
                    .join(&m.protection_file),
                b"broken",
            )
            .unwrap(),
            "wal" => fs::write(root.join("data/mno.sqlite-wal"), b"block").unwrap(),
            _ => fs::write(root.join("data/recovery/unexpected"), b"block").unwrap(),
        }
        let state = owner(&root);
        let before = tree(&root);
        assert!(activation::startup::run_v2_for_test(&state).is_err());
        assert!(state.operation(None).is_err());
        assert_eq!(tree(&root), before);
    }
}
#[test]
fn identical_content_does_not_require_invented_swap_history() {
    for target in [Target::New, Target::Old] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mut m = fixture(&root, true);
        m.target = target;
        save(&root, &m);
        let protected = protection(&root, &m);
        let state = owner(&root);
        activation::startup::run_v2_for_test(&state).unwrap();
        ready(&state, &root, target);
        assert_eq!(protection(&root, &m), protected);
    }
}
#[test]
fn busy_active_database_never_changes_disk_or_grants_ready() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    setup(&root, Target::New, false, true);
    let state = owner(&root);
    let db = Connection::open(root.join("data/mno.sqlite")).unwrap();
    db.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let before = tree(&root);
    assert!(activation::startup::run_v2_for_test(&state).is_err());
    assert!(state.operation(None).is_err());
    assert_eq!(tree(&root), before);
    db.execute_batch("ROLLBACK").unwrap();
    db.close().unwrap();
    activation::startup::run_v2_for_test(&state).unwrap();
    ready(&state, &root, Target::New);
}
