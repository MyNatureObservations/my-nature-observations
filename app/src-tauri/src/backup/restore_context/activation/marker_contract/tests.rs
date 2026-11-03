use super::*;
use crate::database_state::DatabaseState;
use serde_json::{Value, json};
use std::{fs, process::Command, sync::atomic::AtomicBool};

const ID: &str = "12345678-1234-4234-8234-123456789abc";
// A literal wire fixture independent of the constructor and its derived names.
const V2: &str = r#"{"format":"mno-open-import-activation","version":2,"operation_id":"12345678-1234-4234-8234-123456789abc","schema_version":0,"target":"new","content_contract":{"format":"mno-open-import-content","version":1},"old_content":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","entry_count":37},"new_content":{"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","entry_count":42},"protection_file":"pre-import-12345678-1234-4234-8234-123456789abc.mno-backup","protection_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","candidate_file":"next-12345678-1234-4234-8234-123456789abc.sqlite","previous_candidate_file":"previous-12345678-1234-4234-8234-123456789abc.sqlite"}"#;
fn fixture() -> Value {
    serde_json::from_str(V2).unwrap()
}
fn reject(value: &Value) {
    assert!(
        matches!(
            parse(&serde_json::to_vec(value).unwrap()),
            Err(Error::Binding)
        ),
        "accepted {value}"
    );
}
fn proof(hash: char, count: u64) -> ContentProof {
    ContentProof {
        version: 1,
        sha256: hash.to_string().repeat(64),
        entry_count: count,
    }
}
#[test]
fn literal_v2_roundtrips_both_targets_and_constructor_preserves_roles() {
    for target in [Target::New, Target::Old] {
        let marker = ImportV2::new(
            ID.into(),
            target,
            PreviousContent::try_from(proof('a', 37)).unwrap(),
            ReplacementContent::try_from(proof('b', 42)).unwrap(),
            "c".repeat(64),
        )
        .unwrap();
        let mut wire = fixture();
        wire["target"] = json!(if target == Target::New { "new" } else { "old" });
        assert_eq!(serde_json::to_value(&marker).unwrap(), wire);
        assert_eq!(
            parse(&serde_json::to_vec(&wire).unwrap()).unwrap(),
            Contract::ImportV2(marker)
        );
    }
    // Identical old/new journals are legitimate; their roles still stay explicit.
    let marker = ImportV2::new(
        ID.into(),
        Target::Old,
        PreviousContent::try_from(proof('a', 0)).unwrap(),
        ReplacementContent::try_from(proof('a', 0)).unwrap(),
        "c".repeat(64),
    )
    .unwrap();
    assert!(parse(&serde_json::to_vec(&marker).unwrap()).is_ok());
}
#[test]
fn all_fields_required_at_every_level_and_no_extra_fields() {
    for path in [
        None,
        Some("content_contract"),
        Some("old_content"),
        Some("new_content"),
    ] {
        let value = fixture();
        let object = path.map_or(&value, |key| &value[key]).as_object().unwrap();
        for key in object.keys() {
            let mut changed = value.clone();
            let parent = if let Some(path) = path {
                &mut changed[path]
            } else {
                &mut changed
            };
            parent.as_object_mut().unwrap().remove(key);
            reject(&changed);
        }
        let mut changed = value;
        let parent = if let Some(path) = path {
            &mut changed[path]
        } else {
            &mut changed
        };
        parent["unexpected"] = json!(true);
        reject(&changed);
    }
}
fn duplicate(object: &Value, key: &str) -> String {
    let mut fields = Vec::new();
    for (name, value) in object.as_object().unwrap() {
        let field = format!("{}:{value}", serde_json::to_string(name).unwrap());
        fields.push(field.clone());
        if name == key {
            fields.push(field);
        }
    }
    format!("{{{}}}", fields.join(","))
}
#[test]
fn duplicate_keys_rejected_even_when_values_are_identical() {
    let value = fixture();
    for key in value.as_object().unwrap().keys() {
        assert!(
            parse(duplicate(&value, key).as_bytes()).is_err(),
            "duplicate {key}"
        );
    }
    for parent in ["content_contract", "old_content", "new_content"] {
        for key in value[parent].as_object().unwrap().keys() {
            let original = value[parent].to_string();
            let wire = value
                .to_string()
                .replacen(&original, &duplicate(&value[parent], key), 1);
            assert!(parse(wire.as_bytes()).is_err(), "duplicate {parent}.{key}");
        }
    }
}
#[test]
fn invalid_formats_versions_targets_schema_and_contracts_rejected() {
    for (field, values) in [
        (
            "format",
            vec![
                json!("mno-database-activation"),
                json!("unknown"),
                json!(""),
            ],
        ),
        (
            "version",
            vec![
                json!(0),
                json!(1),
                json!(3),
                json!(-1),
                json!(2.0),
                json!("2"),
            ],
        ),
        ("schema_version", vec![json!(1), json!(-1), json!("0")]),
        (
            "target",
            vec![json!("New"), json!("ready"), json!(0), json!(null)],
        ),
    ] {
        for invalid in values {
            let mut value = fixture();
            value[field] = invalid;
            reject(&value);
        }
    }
    for (key, invalid) in [
        ("format", json!("mno-database-content")),
        ("format", json!("")),
        ("version", json!(0)),
        ("version", json!(2)),
        ("version", json!("1")),
    ] {
        let mut value = fixture();
        value["content_contract"][key] = invalid;
        reject(&value);
    }
    for key in fixture().as_object().unwrap().keys() {
        let mut value = fixture();
        value[key] = Value::Null;
        reject(&value);
        let mut value = fixture();
        value[key] = json!([]);
        reject(&value);
    }
}
#[test]
fn invalid_uuid_hashes_counts_and_names_rejected() {
    for id in [
        "",
        "not-a-uuid",
        "00000000-0000-0000-0000-000000000000",
        "12345678-1234-1234-8234-123456789abc",
        "12345678-1234-4234-0234-123456789abc",
        "12345678-1234-4234-8234-123456789ABC",
    ] {
        let mut value = fixture();
        value["operation_id"] = json!(id);
        // Keep all names consistent so a filename mismatch cannot mask a
        // missing UUID version/variant check.
        value["protection_file"] = json!(format!("pre-import-{id}.mno-backup"));
        value["candidate_file"] = json!(format!("next-{id}.sqlite"));
        value["previous_candidate_file"] = json!(format!("previous-{id}.sqlite"));
        reject(&value);
    }
    for invalid in [
        "",
        "a",
        &"a".repeat(63),
        &"a".repeat(65),
        &"A".repeat(64),
        &"g".repeat(64),
    ] {
        for key in ["old_content", "new_content"] {
            let mut value = fixture();
            value[key]["sha256"] = json!(invalid);
            reject(&value);
        }
        let mut value = fixture();
        value["protection_sha256"] = json!(invalid);
        reject(&value);
    }
    for key in ["old_content", "new_content"] {
        for count in [
            json!(-1),
            json!(1.5),
            json!("37"),
            json!(null),
            json!(true),
            serde_json::from_str::<Value>("18446744073709551616").unwrap(),
        ] {
            let mut value = fixture();
            value[key]["entry_count"] = count;
            reject(&value);
        }
    }
    for key in [
        "protection_file",
        "candidate_file",
        "previous_candidate_file",
    ] {
        for name in [
            "",
            "/tmp/next.sqlite",
            "../next.sqlite",
            "sub/next.sqlite",
            "sub\\next.sqlite",
            "next-other.sqlite",
        ] {
            let mut value = fixture();
            value[key] = json!(name);
            reject(&value);
        }
        let mut value = fixture();
        value[key] = json!(value[key].as_str().unwrap().replace("789abc", "789abd"));
        reject(&value);
    }
    let mut swapped = fixture();
    let candidate = swapped["candidate_file"].clone();
    swapped["candidate_file"] = swapped["previous_candidate_file"].clone();
    swapped["previous_candidate_file"] = candidate;
    reject(&swapped);
}
#[test]
fn bounded_complete_document_parsing() {
    let mut bytes = V2.as_bytes().to_vec();
    bytes.resize(LIMIT as usize, b' ');
    assert!(parse(&bytes).is_ok());
    bytes.push(b' ');
    assert!(parse(&bytes).is_err());
    assert!(parse(&V2.as_bytes()[..V2.len() - 1]).is_err());
    assert!(parse(format!("{V2}{{}}").as_bytes()).is_err());
    assert!(parse(b"[]").is_err());
}
fn v1(import: bool) -> Value {
    json!({"format":if import {IMPORT_FORMAT} else {"mno-database-activation"},
        "version":1,"operation_id":ID,"schema_version":0,
        "old_content":{"version":1,"sha256": if import {super::super::importing::EMPTY_RESTORE_SHA256.to_owned()} else {"a".repeat(64)}},
        "new_content":{"version":1,"sha256":"b".repeat(64)},
        "protection_file":"pre-restore-original.mno-backup","protection_sha256":"c".repeat(64),
        "candidate_file":format!("next-{ID}.sqlite")})
}
#[test]
fn unchanged_v1_contracts_stay_separate_and_keep_empty_import_proof() {
    for import in [false, true] {
        let value = v1(import);
        let bytes = serde_json::to_vec(&value).unwrap();
        let marker = parse_v1_marker(&bytes).unwrap();
        assert_eq!(marker.is_import(), import);
        assert_eq!(serde_json::to_value(&marker).unwrap(), value);
        assert!(matches!(
            (parse(&bytes).unwrap(), import),
            (Contract::ImportV1(_), true) | (Contract::RestoreV1(_), false)
        ));
        let mut invalid = value.clone();
        invalid["old_content"]["sha256"] = json!("d".repeat(64));
        assert_eq!(
            parse(&serde_json::to_vec(&invalid).unwrap()).is_ok(),
            !import
        );
        for key in ["target", "content_contract", "previous_candidate_file"] {
            let mut extra = value.clone();
            extra[key] = fixture()[key].clone();
            reject(&extra);
        }
        for parent in ["old_content", "new_content"] {
            let mut invalid = value.clone();
            invalid[parent]["version"] = json!(2);
            reject(&invalid);
            let mut invalid = value.clone();
            invalid[parent]["entry_count"] = json!(0);
            reject(&invalid);
        }
    }
    assert!(parse_v1_marker(V2.as_bytes()).is_err());
}
#[test]
fn existing_import_proofs_and_counts_are_carried_verbatim() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::database::initialize_schema(&db).unwrap();
    let old = crate::open_import::verify_database(&mut db, &AtomicBool::new(false)).unwrap();
    db.execute("INSERT INTO journal_entries VALUES('old-json','legacy date','Åäö','{ \"text\":\"a\", \"text\":\"b\" }',1,'created','updated')", []).unwrap();
    let new = crate::open_import::verify_database(&mut db, &AtomicBool::new(false)).unwrap();
    let marker = ImportV2::new(
        ID.into(),
        Target::New,
        PreviousContent::try_from(old.clone()).unwrap(),
        ReplacementContent::try_from(new.clone()).unwrap(),
        "c".repeat(64),
    )
    .unwrap();
    assert_eq!(marker.old_content.0.sha256, old.sha256);
    assert_eq!(marker.old_content.0.entry_count, old.entry_count);
    assert_eq!(marker.new_content.0.sha256, new.sha256);
    assert_eq!(marker.new_content.0.entry_count, new.entry_count);
    assert_ne!(marker.old_content.0, marker.new_content.0);
    let mut invalid = new;
    invalid.version = 2;
    assert!(PreviousContent::try_from(invalid.clone()).is_err());
    assert!(ReplacementContent::try_from(invalid).is_err());
    db.close().unwrap();
}

const CHILD: &str = "backup::restore_context::activation::marker_contract::tests::v2_startup_child";
#[test]
fn v2_startup_child() {
    let Some(root) = std::env::var_os("MNO_V2_CONTRACT_ROOT") else {
        return;
    };
    crate::panic_policy::install();
    let root = std::path::PathBuf::from(root);
    let state = DatabaseState::default();
    assert!(state.initialize_legacy_startup(&root).is_err());
    assert!(state.operation(None).is_err());
    assert!(super::super::startup::status(&state).is_err());
    for action in [
        super::super::startup::Action::FinishNew,
        super::super::startup::Action::KeepPrevious,
        super::super::startup::Action::Continue,
    ] {
        assert!(super::super::startup::run(&state, action).is_err());
        assert!(state.operation(None).is_err());
    }
    std::process::exit(72);
}
#[test]
fn incomplete_v2_episodes_remain_blocked_without_touching_database_or_evidence() {
    for target in ["new", "old"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let state = DatabaseState::default();
        let db = state.initialize_legacy_startup(&root).unwrap();
        drop(state);
        let before = fs::read(&db).unwrap();
        fs::create_dir(root.join("recovery")).unwrap();
        let mut value = fixture();
        value["target"] = json!(target);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(parse(&bytes).is_ok());
        let path = root.join("recovery").join(super::super::MARKER);
        fs::write(&path, &bytes).unwrap();
        // The unchanged strict v1 reader still rejects v2. Production dispatch
        // recognizes it separately but cannot accept this incomplete episode.
        assert!(matches!(
            super::super::read_marker(&root),
            Err(Error::Binding)
        ));
        for _ in 0..2 {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CHILD, "--nocapture"])
                .env("MNO_V2_CONTRACT_ROOT", &root)
                .env("TMPDIR", temp.path())
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(72),
                "{} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read(&db).unwrap(), before);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            for suffix in ["-journal", "-wal", "-shm"] {
                assert!(!std::path::PathBuf::from(format!("{}{suffix}", db.display())).exists());
            }
        }
    }
}

#[test]
fn nested_objects_cannot_be_replaced_by_positional_arrays() {
    for (field, array) in [
        ("content_contract", json!(["mno-open-import-content", 1])),
        ("old_content", json!(["a".repeat(64), 37])),
        ("new_content", json!(["b".repeat(64), 42])),
    ] {
        let mut value = fixture();
        value[field] = array;
        reject(&value);
    }
    // Escaped names must still participate in duplicate-key detection.
    let wire = V2.replacen(
        "\"target\":\"new\"",
        r#""target":"new","targ\u0065t":"old""#,
        1,
    );
    assert_ne!(wire, V2);
    assert!(parse(wire.as_bytes()).is_err());
}

#[test]
fn target_requires_an_exact_json_string() {
    for target in ["new", "old"] {
        let mut value = fixture();
        value["target"] = json!(target);
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_ok());
    }
    for target in [
        json!({"new": null}),
        json!({"old": null}),
        json!({}),
        json!({"other": "new"}),
        json!([]),
        json!(["new"]),
        json!(null),
        json!(0),
        json!(1.0),
        json!(true),
        json!(false),
        json!(""),
        json!("New"),
        json!("OLD"),
        json!(" new"),
        json!("old "),
        json!("unknown"),
    ] {
        let mut value = fixture();
        value["target"] = target;
        reject(&value);
    }
}
