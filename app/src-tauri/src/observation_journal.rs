//! Atomic first save of an observation-linked journal draft.
use crate::{database_state::DatabaseState, journal::SavedJournalEntry, photographs::Asset};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewPlace {
    name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Draft {
    entry_id: String,
    visit_id: String,
    visit_date: String,
    visit_place_id: String,
    entry_date: String,
    place_id: Option<String>,
    new_place: Option<NewPlace>,
    title: String,
    content_json: String,
    document_version: u8,
    assets: Vec<Asset>,
}

#[tauri::command]
pub(crate) fn save_observation_journal(
    app: tauri::AppHandle,
    state: tauri::State<'_, DatabaseState>,
    search: tauri::State<'_, crate::journal_search::SearchState>,
    module: tauri::State<'_, crate::bird_module::BirdModule>,
    generation: u32,
    draft: Draft,
) -> Result<SavedJournalEntry, String> {
    save_guarded(&state, &search, &module, generation, draft, || {
        crate::database::open_database(&app)
    })
}

fn save_guarded(
    state: &DatabaseState,
    search: &crate::journal_search::SearchState,
    module: &crate::bird_module::BirdModule,
    generation: u32,
    draft: Draft,
    open: impl FnOnce() -> Result<Connection, String>,
) -> Result<SavedJournalEntry, String> {
    let _module = module.command()?;
    search.with_write(|| {
        let _operation = state.operation(Some(generation))?;
        save(&mut open()?, draft)
    })
}

fn sql(error: rusqlite::Error) -> String {
    error.to_string()
}

fn save(db: &mut Connection, draft: Draft) -> Result<SavedJournalEntry, String> {
    let Draft {
        entry_id,
        visit_id,
        visit_date,
        visit_place_id,
        entry_date,
        place_id,
        new_place,
        title,
        content_json,
        document_version,
        assets,
    } = draft;
    if uuid::Uuid::parse_str(&entry_id).is_err() {
        return Err("journal_entry_target_invalid".into());
    }
    if new_place.is_some() && place_id.is_none() {
        return Err("place_id_invalid".into());
    }
    if !crate::backup::restore::valid_date(&entry_date) {
        return Err("journal_entry_date_invalid".into());
    }
    if crate::journal::is_definitely_empty_journal_json(&content_json) {
        return Err("journal_empty_entry".into());
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    if !crate::database::ebird_schema::validate_schema(&tx).map_err(sql)? {
        return Err("bird_schema_unsupported".into());
    }
    let manual: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bird_lists WHERE id=?1)",
            [&visit_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let imported: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ebird_visits WHERE id=?1)",
            [&visit_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let (table, owner_column) = match (manual, imported) {
        (true, false) => ("journal_bird_lists", "list_id"),
        (false, true) => ("journal_ebird_visits", "visit_id"),
        _ => return Err("bird_list_missing".into()),
    };
    let existing = tx.query_row(
        "SELECT entry_date,title,content_json,document_version FROM journal_entries WHERE id=?1",
        [&entry_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, u8>(3)?)),
    ).optional().map_err(sql)?;
    if let Some(existing) = existing {
        let linked: bool = tx
            .query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM {table} WHERE entry_id=?1 AND {owner_column}=?2)"
                ),
                params![entry_id, visit_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let place = crate::place_registry::read_link(&tx, &entry_id)?.place;
        let same_new_place = new_place.as_ref().is_none_or(|draft| {
            place.as_ref().is_some_and(|saved| {
                saved.name == draft.name
                    && saved.latitude == draft.latitude
                    && saved.longitude == draft.longitude
            })
        });
        if !linked
            || existing != (entry_date, title, content_json, document_version)
            || place.as_ref().map(|p| &p.id) != place_id.as_ref()
            || !same_new_place
        {
            return Err("journal_entry_already_exists".into());
        }
        for asset in &assets {
            if crate::photographs::get(&tx, &asset.asset_id)?.as_ref() != Some(asset) {
                return Err("journal_entry_already_exists".into());
            }
        }
        return Ok(SavedJournalEntry { id: entry_id });
    }
    let context_sql = if manual {
        "SELECT visit_date,place_id FROM bird_lists WHERE id=?1"
    } else {
        "SELECT json_extract(source_json,'$.Date'),'ebird:' || json_extract(source_json,'$.\"Location ID\"') FROM ebird_visits WHERE id=?1"
    };
    let context: (String, String) = tx
        .query_row(context_sql, [&visit_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql)?;
    if context != (visit_date, visit_place_id) {
        return Err("journal_observation_stale".into());
    }
    let already_linked: bool = tx
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {owner_column}=?1)"),
            [&visit_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if already_linked {
        return Err("journal_observation_already_linked".into());
    }
    match document_version {
        1 if assets.is_empty() => {
            crate::journal::save_journal_entry_with_connection(
                &tx,
                crate::journal::EntryTarget::New(Some(entry_id.clone())),
                entry_date,
                title,
                content_json,
            )?;
        }
        2 => crate::photographs::save_in_transaction(
            &tx,
            &entry_id,
            &entry_date,
            &title,
            &content_json,
            &assets,
            true,
        )?,
        _ => return Err("journal_document_incompatible".into()),
    }
    if let Some(place_id) = place_id {
        let place = if let Some(draft) = new_place {
            crate::place_registry::create_in_transaction(
                &tx,
                &place_id,
                &draft.name,
                draft.latitude,
                draft.longitude,
            )?
        } else {
            crate::place_registry::read_place(&tx, &place_id)?
        };
        let revision = place
            .revision
            .parse::<i64>()
            .map_err(|_| "place_revision_invalid")?;
        tx.execute(
            "INSERT INTO journal_places(entry_id,place_id,place_name_snapshot,latitude_snapshot,longitude_snapshot,place_revision_snapshot,revision) VALUES (?1,?2,?3,?4,?5,?6,1)",
            params![entry_id, place.id, place.name, place.latitude, place.longitude, revision],
        ).map_err(sql)?;
    }
    tx.execute(
        &format!("INSERT INTO {table}(entry_id,{owner_column}) VALUES (?1,?2)"),
        params![entry_id, visit_id],
    )
    .map_err(sql)?;
    crate::storage_content::snapshot_v14(&tx, &std::sync::atomic::AtomicBool::new(false))
        .map_err(|_| "journal_observation_save_failed")?;
    tx.commit().map_err(sql)?;
    Ok(SavedJournalEntry { id: entry_id })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::{database, ebird_persistence as ebird, journal_search::SearchState};

    #[test]
    fn manual_and_imported_drafts_are_atomic_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let (state, path) = ebird::tests::fixture_at_schema(&root.path().join("journal"), 13);
        let module = crate::bird_module::test_module(true);
        ebird::import(
            &state,
            &SearchState::default(),
            &module,
            1,
            &[ebird::tests::input("S191")],
        )
        .unwrap();
        let mut db = database::open_existing(&path).unwrap();
        let snapshots = |db: &Connection| {
            ebird::records(db)
                .unwrap()
                .into_iter()
                .filter(|r| !matches!(r, ebird::Record::Link { .. }))
                .collect::<Vec<_>>()
        };
        let before = snapshots(&db);
        let (manual, original_place): (String, String) = db
            .query_row("SELECT id,place_id FROM bird_lists", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        db.execute("DELETE FROM journal_bird_lists WHERE list_id=?1", [&manual])
            .unwrap();
        let imported: String = db
            .query_row("SELECT id FROM ebird_visits", [], |r| r.get(0))
            .unwrap();
        let place = uuid::Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO places VALUES(?1,'Changed journal place',60,19,1)",
            [&place],
        )
        .unwrap();
        let mut saved_entries = Vec::new();
        for (visit_id, table, owner, selected_place, version) in [
            (&manual, "journal_bird_lists", "list_id", Some(&place), 1),
            (&imported, "journal_ebird_visits", "visit_id", None, 2),
        ] {
            let id = uuid::Uuid::new_v4().to_string();
            let (visit_date, visit_place_id): (String, String) = if table == "journal_bird_lists" {
                ("2026-11-21".into(), original_place.clone())
            } else {
                let json: String = db
                    .query_row(
                        "SELECT source_json FROM ebird_visits WHERE id=?1",
                        [visit_id],
                        |r| r.get(0),
                    )
                    .unwrap();
                let source: serde_json::Value = serde_json::from_str(&json).unwrap();
                (
                    source["Date"].as_str().unwrap().into(),
                    format!("ebird:{}", source["Location ID"].as_str().unwrap()),
                )
            };
            let draft = |place_id: Option<String>| {
                Draft {
                entry_id: id.clone(),
                visit_id: visit_id.clone(),
                visit_date: visit_date.clone(),
                visit_place_id: visit_place_id.clone(),
                entry_date: "2026-11-25".into(),
                place_id,
                new_place: None,
                title: "Linked synthetic journal".into(),
                content_json: serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Linked synthetic journal"}]}]}).to_string(),
                document_version: version,
                assets: vec![],
            }
            };
            assert!(save(&mut db, draft(Some(uuid::Uuid::new_v4().to_string()))).is_err());
            let found: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=?1)",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(!found);
            let mut invalid = draft(None);
            invalid.visit_id = "missing".into();
            assert!(save(&mut db, invalid).is_err());
            let count_links = |db: &Connection| {
                db.query_row(
                    &format!("SELECT count(*) FROM {table} WHERE entry_id=?1 AND {owner}=?2"),
                    params![id, visit_id],
                    |r| r.get::<_, u32>(0),
                )
                .unwrap()
            };
            assert_eq!(count_links(&db), 0);
            let mut stale = draft(None);
            stale.visit_place_id = "changed".into();
            assert_eq!(
                save(&mut db, stale).err().unwrap(),
                "journal_observation_stale"
            );
            let search = SearchState::default();
            assert!(
                save_guarded(&state, &search, &module, 2, draft(None), || panic!(
                    "stale session opened database"
                ))
                .is_err()
            );
            assert!(
                save_guarded(
                    &state,
                    &search,
                    &crate::bird_module::test_module(false),
                    1,
                    draft(None),
                    || panic!("disabled module opened database")
                )
                .is_err()
            );
            let staged_place = uuid::Uuid::new_v4().to_string();
            let final_place = selected_place.unwrap_or(&staged_place);
            let confirmed = || {
                let mut value = draft(Some(final_place.clone()));
                if selected_place.is_none() {
                    value.new_place = Some(NewPlace {
                        name: "Reviewed source copy".into(),
                        latitude: Some(59.0),
                        longitude: Some(18.0),
                    });
                }
                value
            };
            if selected_place.is_none() {
                // Force a failure after place creation; SQLite must roll back all three inserts.
                db.execute_batch("CREATE TEMP TRIGGER stop_link BEFORE INSERT ON journal_ebird_visits BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;").unwrap();
                assert!(save(&mut db, confirmed()).is_err());
                assert!(crate::place_registry::read_place(&db, final_place).is_err());
                assert_eq!(count_links(&db), 0);
                let exists: bool = db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=?1)",
                        [&id],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert!(!exists);
                db.execute_batch("DROP TRIGGER stop_link").unwrap();
            }
            let original_entry_count: u32 = db
                .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap();
            // No response from the initial operation is used by the caller.
            save_guarded(&state, &search, &module, 1, confirmed(), || {
                database::open_existing(&path)
            })
            .unwrap();
            assert_eq!(
                save_guarded(&state, &search, &module, 1, confirmed(), || {
                    database::open_existing(&path)
                })
                .unwrap()
                .id,
                id
            );
            let latest_json = serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Latest text after confirmation"}]}]}).to_string();
            if version == 1 {
                crate::journal::save_journal_entry_with_connection(
                    &db,
                    crate::journal::EntryTarget::Existing(id.clone()),
                    "2026-11-25".into(),
                    "Latest text after confirmation".into(),
                    latest_json.clone(),
                )
                .unwrap();
            } else {
                crate::photographs::save(
                    &mut db,
                    &id,
                    "2026-11-25",
                    "Latest text after confirmation",
                    &latest_json,
                    &[],
                    false,
                )
                .unwrap();
            }
            let mut other_draft = confirmed();
            other_draft.entry_id = uuid::Uuid::new_v4().to_string();
            assert_eq!(
                save(&mut db, other_draft).err().unwrap(),
                "journal_observation_already_linked"
            );
            drop(db);
            db = database::open_existing(&path).unwrap();
            let date: String = db
                .query_row(
                    "SELECT entry_date FROM journal_entries WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(date, "2026-11-25");
            assert_eq!(count_links(&db), 1);
            assert_eq!(
                crate::place_registry::read_link(&db, &id)
                    .unwrap()
                    .place
                    .map(|p| p.id),
                Some(final_place.clone())
            );
            let reopened_contents: Vec<String> = db
                .prepare("SELECT content_json FROM journal_entries WHERE id=?1")
                .unwrap()
                .query_map([&id], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(reopened_contents, vec![latest_json.clone()]);
            let reopened_entry_count: u32 = db
                .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap();
            assert_eq!(reopened_entry_count, original_entry_count + 1);
            if selected_place.is_none() {
                let count: u32 = db
                    .query_row(
                        "SELECT count(*) FROM places WHERE name='Reviewed source copy'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(count, 1);
            }
            let reverse: String = db
                .query_row(
                    &format!("SELECT entry_id FROM {table} WHERE {owner}=?1 AND entry_id=?2"),
                    params![visit_id, id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(reverse, id);
            let set_link = |entry: &str, linked| {
                crate::bird_app::set_link_for_visit(&state, &search, 1, visit_id, entry, linked)
                    .unwrap();
            };
            let linked_entries = || {
                crate::bird_app::journal_targets(&state, 1, visit_id)
                    .unwrap()
                    .into_iter()
                    .filter(|entry| entry.linked)
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>()
            };
            assert_eq!(linked_entries(), vec![id.clone()]);
            set_link("other", true);
            set_link(&id, false);
            assert_eq!(linked_entries(), vec!["other"]);
            assert_eq!(count_links(&db), 0);
            // Match the current note exactly so only the absent link can reject the replay.
            let mut removed_replay = confirmed();
            removed_replay.title = "Latest text after confirmation".into();
            removed_replay.content_json = latest_json;
            assert_eq!(
                save(&mut db, removed_replay).err().unwrap(),
                "journal_entry_already_exists"
            );
            // The next autosave takes the ordinary existing-entry path and changes only the note.
            let autosave = confirmed();
            if version == 1 {
                crate::journal::save_journal_entry_with_connection(
                    &db,
                    crate::journal::EntryTarget::Existing(id.clone()),
                    autosave.entry_date,
                    "Edited after unlink".into(),
                    autosave.content_json,
                )
                .unwrap();
            } else {
                crate::photographs::save(
                    &mut db,
                    &id,
                    &autosave.entry_date,
                    "Edited after unlink",
                    &autosave.content_json,
                    &[],
                    false,
                )
                .unwrap();
            }
            assert_eq!(count_links(&db), 0);
            assert_eq!(linked_entries(), vec!["other"]);
            set_link("other", false);
            assert!(linked_entries().is_empty());
            saved_entries.push((visit_id.clone(), id));
        }
        let current_place: String = db
            .query_row(
                "SELECT place_id FROM bird_lists WHERE id=?1",
                [&manual],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(current_place, original_place);
        assert_eq!(snapshots(&db), before);
        drop(db);
        drop(state);
        let reopened = DatabaseState::default();
        reopened
            .initialize_schema13_startup(&root.path().join("journal"))
            .unwrap();
        let db = database::open_existing(&path).unwrap();
        for (visit, entry) in saved_entries {
            assert!(
                !crate::bird_app::journal_targets(&reopened, 1, &visit)
                    .unwrap()
                    .into_iter()
                    .any(|entry| entry.linked)
            );
            let title: String = db
                .query_row(
                    "SELECT title FROM journal_entries WHERE id=?1",
                    [&entry],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(title, "Edited after unlink");
        }
        assert_eq!(snapshots(&db), before);
    }
}
