//! Small Tauri boundary for journal-origin creation and read-only reopening.
use crate::{
    database_state::{DatabaseState, Snapshot},
    journal_search::SearchState,
};
use serde::{Deserialize, Serialize};
use tauri::State;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RowInput {
    species_id: String,
    taxonomy_version: String,
    count: String,
    #[serde(default)]
    comment: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowView {
    species_id: String,
    taxonomy_version: String,
    count: String,
    comment: Option<String>,
    english: String,
    swedish: Option<String>,
    scientific: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListView {
    edit_token: String,
    id: String,
    visit_date: String,
    visit_start: Option<String>,
    visit_end: Option<String>,
    place: crate::place_registry::Place,
    rows: Vec<RowView>,
}
#[tauri::command]
pub(crate) fn bird_observations_available(state: State<'_, DatabaseState>) -> bool {
    available(&state)
}
fn available(state: &DatabaseState) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    (|| -> Result<bool, String> {
        let path = state.ready_database_path()?;
        let _permit = state.operation(None)?;
        let db = crate::database::open_existing(&path)?;
        crate::database::bird_requests::validate_schema(&db).map_err(|e| e.to_string())
    })()
    .unwrap_or(false)
}

#[tauri::command]
pub(crate) fn search_bird_species(
    query: String,
) -> Result<Vec<crate::species_registry::Species>, String> {
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    Ok(crate::species_registry::search(&query))
}
#[tauri::command]
pub(crate) fn create_journal_bird_list(
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
    request_id: String,
    entry_id: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        create_in_session(
            &state,
            &search,
            CreateInput {
                generation,
                request_id,
                entry_id,
                visit_date,
                place_id,
                rows,
                visit_start,
                visit_end,
            },
        )
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            state,
            search,
            generation,
            request_id,
            entry_id,
            visit_date,
            place_id,
            rows,
            visit_start,
            visit_end,
        );
        Err("bird_platform_unsupported".into())
    }
}
#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateInput {
    generation: u32,
    request_id: String,
    entry_id: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
}
#[cfg(target_os = "macos")]
fn create_in_session(
    state: &DatabaseState,
    search: &SearchState,
    input: CreateInput,
) -> Result<String, String> {
    if !available(state) {
        return Err("bird_schema_unsupported".into());
    }
    let rows = validate_rows(input.rows)?;
    crate::bird_observations::create_with_identity(
        state,
        search,
        input.generation,
        &input.visit_date,
        &input.place_id,
        &rows,
        Some(&input.entry_id),
        input.visit_start.as_deref(),
        input.visit_end.as_deref(),
        Some(&input.request_id),
    )
}
#[cfg(target_os = "macos")]
fn validate_rows(
    rows: Vec<RowInput>,
) -> Result<Vec<crate::bird_observations::SpeciesCount>, String> {
    if rows.is_empty() || rows.len() > 11172 {
        return Err("bird_rows_invalid".into());
    }
    let mut identities = std::collections::HashSet::new();
    rows.into_iter()
        .map(|r| {
            if r.taxonomy_version != crate::species_registry::VERSION
                || crate::species_registry::get(&r.species_id).is_none()
            {
                return Err("bird_species_invalid".into());
            }
            if !identities.insert(r.species_id.clone()) {
                return Err("bird_species_duplicate".into());
            }
            if !crate::database::bird_comments::valid_comment(r.comment.as_deref()) {
                return Err("bird_comment_invalid".into());
            }
            let count = r.count.parse::<i64>().map_err(|_| "bird_count_invalid")?;
            if count <= 0 || count.to_string() != r.count {
                return Err("bird_count_invalid".into());
            }
            Ok(crate::bird_observations::SpeciesCount {
                avibase_id: r.species_id,
                taxonomy_version: r.taxonomy_version,
                count,
                comment: r.comment,
            })
        })
        .collect()
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    fn row(id: &str, count: &str, version: &str) -> RowInput {
        RowInput {
            species_id: id.into(),
            taxonomy_version: version.into(),
            count: count.into(),
            comment: None,
        }
    }
    // The same flat JSON payload is exercised by the real TypeScript invoke
    // helper. No desktop mock runtime or mocked persistence result is involved.
    const CREATE_WIRE: &str = include_str!("../../tests/fixtures/bird-create-command.json");
    fn input() -> CreateInput {
        serde_json::from_str(CREATE_WIRE).unwrap()
    }
    fn journal_inventory(
        path: &std::path::Path,
    ) -> Vec<(String, String, String, String, i64, String, String)> {
        let db = crate::database::open_existing(path).unwrap();
        let result=db.prepare("SELECT id,entry_date,title,content_json,document_version,created_at,updated_at FROM journal_entries ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        db.close().map_err(|(_, e)| e).unwrap();
        result
    }
    #[test]
    fn search_command_returns_exact_swedish_english_and_scientific_names() {
        for query in [
            "Skata",
            "skata",
            "SKATA",
            "Pica pica",
            "PICA PICA",
            "Eurasian Magpie",
            "EURASIAN MAGPIE",
        ] {
            let found = search_bird_species(query.into()).unwrap();
            assert_eq!(found[0].id, "avibase-FBBBA943", "query: {query}");
            assert_eq!(found[0].swedish.as_deref(), Some("skata"));
        }
        for query in ["kråka", "KRÅKA"] {
            let found = search_bird_species(query.into()).unwrap();
            assert_eq!(found[0].id, "avibase-E4EE9AC4");
            assert_eq!(found[0].swedish.as_deref(), Some("Kråka"));
        }
        assert_eq!(
            search_bird_species("gråkråka".into()).unwrap()[0]
                .referral
                .as_deref(),
            Some("gråkråka")
        );
    }
    #[test]
    fn command_contract_saves_reopens_and_replays_same_request_without_writes() {
        let root = tempfile::TempDir::new().unwrap();
        let directory = root.path().join("journal");
        let state = DatabaseState::default();
        let path = state.initialize_startup(&directory).unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        let request = input();
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-12','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"Synthetic\"}]}]}',2,'created','updated')",[&request.entry_id]).unwrap();
        db.execute(
            "INSERT INTO places VALUES (?1,'Synthetic lake',59.33,18.07,1)",
            [&request.place_id],
        )
        .unwrap();
        db.close().map_err(|(_, e)| e).unwrap();
        let journal_before = journal_inventory(&path);
        let search = SearchState::default();
        crate::bird_observations::reserve_id(&state, &search, 1, &request.request_id).unwrap();
        let id = create_in_session(&state, &search, request).unwrap();
        assert_eq!(
            serde_json::to_value(&id).unwrap(),
            serde_json::Value::String(id.clone())
        );
        let view = read_view(&state, 1, &input().entry_id, &id).unwrap();
        let wire = serde_json::to_value(&view).unwrap();
        assert_eq!(wire["visitStart"], "11:17");
        assert_eq!(wire["visitEnd"], "11:20");
        assert_eq!(wire["rows"][0]["speciesId"], "avibase-E15BC0CB");
        assert_eq!(wire["rows"][0]["swedish"], "talgoxe");
        assert_eq!(wire["rows"][0]["count"], "6");
        assert_eq!(wire["rows"][1]["count"], "4");
        assert_eq!(
            wire["rows"][0]["comment"],
            input().rows[0].comment.as_deref().unwrap()
        );
        let mut duplicate = input();
        duplicate.rows.push(row(
            "avibase-E15BC0CB",
            "2",
            crate::species_registry::VERSION,
        ));
        assert_eq!(
            create_in_session(&state, &search, duplicate).unwrap_err(),
            "bird_species_duplicate"
        );
        assert_eq!(journal_inventory(&path), journal_before);
        let bytes = std::fs::read(&path).unwrap();
        for _ in 0..2 {
            assert_eq!(create_in_session(&state, &search, input()).unwrap(), id);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM bird_lists", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM journal_bird_lists", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.close().map_err(|(_, e)| e).unwrap();
        let mut bad = input();
        bad.visit_date = "2026-11-13".into();
        bad.rows[0].count = "0".into();
        assert_eq!(
            create_in_session(&state, &search, bad).unwrap_err(),
            "bird_count_invalid"
        );
        let mut stale = input();
        stale.generation = 2;
        stale.visit_date = "2026-11-13".into();
        assert_eq!(
            create_in_session(&state, &search, stale).unwrap_err(),
            crate::database_state::GENERATION_CONFLICT
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let mut conflict = input();
        conflict.rows[0].count = "7".into();
        assert_eq!(
            create_in_session(&state, &search, conflict).unwrap_err(),
            "bird_request_conflict"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        for _ in 0..2 {
            let mut visit = input();
            visit.request_id = uuid::Uuid::new_v4().to_string();
            visit.visit_start = None;
            visit.visit_end = None;
            crate::bird_observations::reserve_id(&state, &search, 1, &visit.request_id).unwrap();
            let new_id = create_in_session(&state, &search, visit).unwrap();
            assert_ne!(new_id, id);
            assert_eq!(
                read_view(&state, 1, &input().entry_id, &new_id)
                    .unwrap()
                    .visit_start,
                None
            );
        }
        assert_eq!(journal_inventory(&path), journal_before);
        let read_db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            crate::journal::read_journal_entries_with_connection(&read_db)
                .unwrap()
                .len(),
            1
        );
        read_db.close().map_err(|(_, e)| e).unwrap();
        drop(state);
        let reopened = DatabaseState::default();
        reopened.initialize_startup(&directory).unwrap();
        assert_eq!(create_in_session(&reopened, &search, input()).unwrap(), id);
        assert_eq!(journal_inventory(&path), journal_before);
        assert_eq!(
            serde_json::to_value(read_view(&reopened, 1, &input().entry_id, &id).unwrap()).unwrap(),
            wire
        );
        let mut edit: serde_json::Value = serde_json::from_str(CREATE_WIRE).unwrap();
        edit.as_object_mut().unwrap().remove("requestId");
        edit["id"] = id.clone().into();
        edit["expectedToken"] = read_view(&reopened, 1, &input().entry_id, &id)
            .unwrap()
            .edit_token
            .into();
        edit["rows"][0]["count"] = "9".into();
        let token = update_in_session(
            &reopened,
            &search,
            serde_json::from_value(edit.clone()).unwrap(),
        )
        .unwrap();
        let updated = read_view(&reopened, 1, &input().entry_id, &id).unwrap();
        assert_eq!(updated.edit_token, token);
        assert_eq!(updated.rows[0].count, "9");
        assert_eq!(journal_inventory(&path), journal_before);
        assert_eq!(
            update_in_session(&reopened, &search, serde_json::from_value(edit).unwrap())
                .unwrap_err(),
            "bird_edit_stale"
        );
        let mut deletion =
            serde_json::json!({"generation":1,"id":id,"expectedToken":token,"confirmed":false});
        assert_eq!(
            delete_in_session(
                &reopened,
                &search,
                serde_json::from_value(deletion.clone()).unwrap()
            )
            .unwrap_err(),
            "bird_delete_confirmation_required"
        );
        deletion["confirmed"] = true.into();
        delete_in_session(
            &reopened,
            &search,
            serde_json::from_value(deletion).unwrap(),
        )
        .unwrap();
        assert!(!crate::bird_observations::exists(&reopened, 1, &id).unwrap());
        assert_eq!(journal_inventory(&path), journal_before);
    }
    #[test]
    fn crow_supplement_survives_save_and_reopen_without_changing_identity() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("journal");
        let state = DatabaseState::default();
        let path = state.initialize_startup(&root).unwrap();
        let mut request = input();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/bird-crow-view.json")).unwrap();
        request.rows = vec![RowInput {
            species_id: expected["speciesId"].as_str().unwrap().into(),
            taxonomy_version: expected["taxonomyVersion"].as_str().unwrap().into(),
            count: expected["count"].as_str().unwrap().into(),
            comment: Some(expected["comment"].as_str().unwrap().into()),
        }];
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-12','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'created','updated')",[&request.entry_id]).unwrap();
        db.execute(
            "INSERT INTO places VALUES (?1,'Synthetic lake',59.33,18.07,1)",
            [&request.place_id],
        )
        .unwrap();
        db.close().map_err(|(_, e)| e).unwrap();
        let entry = request.entry_id.clone();
        crate::bird_observations::reserve_id(
            &state,
            &SearchState::default(),
            1,
            &request.request_id,
        )
        .unwrap();
        let id = create_in_session(&state, &SearchState::default(), request).unwrap();
        assert_eq!(
            serde_json::to_value(&read_view(&state, 1, &entry, &id).unwrap().rows[0]).unwrap(),
            expected
        );
        drop(state);
        let reopened = DatabaseState::default();
        reopened.initialize_startup(&root).unwrap();
        assert_eq!(
            serde_json::to_value(&read_view(&reopened, 1, &entry, &id).unwrap().rows[0]).unwrap(),
            expected
        );
    }
    #[test]
    fn creation_accepts_only_catalog_id_version_and_positive_exact_count() {
        for rows in [
            vec![],
            vec![row("unknown", "1", crate::species_registry::VERSION)],
            vec![row("avibase-DF1E2A1D", "1", "older")],
            vec![row(
                "avibase-DF1E2A1D",
                "0",
                crate::species_registry::VERSION,
            )],
            vec![row(
                "avibase-DF1E2A1D",
                "9223372036854775808",
                crate::species_registry::VERSION,
            )],
        ] {
            assert!(validate_rows(rows).is_err());
        }
        assert_eq!(
            validate_rows(vec![row(
                "avibase-DF1E2A1D",
                "6",
                crate::species_registry::VERSION
            )])
            .unwrap()[0]
                .count,
            6
        );
    }
}

#[tauri::command]
pub(crate) fn read_journal_bird_lists(
    state: State<'_, DatabaseState>,
    generation: u32,
    entry_id: String,
) -> Result<Snapshot<Vec<ListView>>, String> {
    #[cfg(target_os = "macos")]
    {
        // Each read checks the same generation; do not nest operation permits.
        let path = state.ready_database_path()?;
        let _permit = state.operation(Some(generation))?;
        let db = crate::database::open_existing(&path)?;
        let ids=db.prepare("SELECT list_id FROM journal_bird_lists WHERE entry_id=?1 ORDER BY list_id COLLATE BINARY").map_err(|e|e.to_string())?.query_map([&entry_id],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
        drop(db);
        drop(_permit);
        let mut lists = Vec::new();
        for id in ids {
            lists.push(read_view(&state, generation, &entry_id, &id)?);
        }
        let permit = state.operation(Some(generation))?;
        Ok(permit.snapshot(lists))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, generation, entry_id);
        Err("bird_platform_unsupported".into())
    }
}
#[tauri::command]
pub(crate) fn read_journal_bird_list(
    state: State<'_, DatabaseState>,
    generation: u32,
    entry_id: String,
    id: String,
) -> Result<Snapshot<ListView>, String> {
    #[cfg(target_os = "macos")]
    {
        let view = read_view(&state, generation, &entry_id, &id)?;
        Ok(state.operation(Some(generation))?.snapshot(view))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, generation, entry_id, id);
        Err("bird_platform_unsupported".into())
    }
}
#[cfg(target_os = "macos")]
fn read_view(
    state: &DatabaseState,
    generation: u32,
    entry: &str,
    id: &str,
) -> Result<ListView, String> {
    let list = crate::bird_observations::reopen(state, generation, id)?;
    if !list.journal_entries.iter().any(|e| e == entry) {
        return Err("bird_link_missing".into());
    }
    let path = state.ready_database_path()?;
    let _permit = state.operation(Some(generation))?;
    let db = crate::database::open_existing(&path)?;
    let place = crate::place_registry::read_place(&db, &list.place_id)?;
    let edit_token = crate::bird_observations::edit_token(&list);
    let rows = list
        .rows
        .into_iter()
        .map(|r| {
            let s = if r.taxonomy_version == crate::species_registry::VERSION {
                crate::species_registry::get(&r.avibase_id)
            } else {
                None
            };
            RowView {
                species_id: r.avibase_id.clone(),
                taxonomy_version: r.taxonomy_version,
                count: r.count.to_string(),
                comment: r.comment,
                english: s.map_or_else(|| r.avibase_id.clone(), |s| s.english.clone()),
                swedish: s.and_then(|s| s.swedish.clone()),
                scientific: s.map_or(String::new(), |s| s.scientific.clone()),
            }
        })
        .collect();
    Ok(ListView {
        edit_token,
        id: list.id,
        visit_date: list.visit_date,
        visit_start: list.visit_start,
        visit_end: list.visit_end,
        place,
        rows,
    })
}

#[tauri::command]
pub(crate) fn update_journal_bird_list(
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
    id: String,
    entry_id: String,
    expected_token: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        update_in_session(
            &state,
            &search,
            EditInput {
                generation,
                id,
                entry_id,
                expected_token,
                visit_date,
                place_id,
                rows,
                visit_start,
                visit_end,
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            state,
            search,
            generation,
            id,
            entry_id,
            expected_token,
            visit_date,
            place_id,
            rows,
            visit_start,
            visit_end,
        );
        Err("bird_platform_unsupported".into())
    }
}

#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditInput {
    generation: u32,
    id: String,
    entry_id: String,
    expected_token: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
}
#[cfg(target_os = "macos")]
fn update_in_session(
    state: &DatabaseState,
    search: &SearchState,
    input: EditInput,
) -> Result<String, String> {
    if !available(state) {
        return Err("bird_schema_unsupported".into());
    }
    let EditInput {
        generation,
        id,
        entry_id,
        expected_token,
        visit_date,
        place_id,
        rows,
        visit_start,
        visit_end,
    } = input;
    let rows = rows
        .into_iter()
        .map(|r| {
            let count = r.count.parse::<i64>().map_err(|_| "bird_count_invalid")?;
            if count <= 0 || count.to_string() != r.count {
                return Err("bird_count_invalid".to_owned());
            }
            Ok(crate::bird_observations::SpeciesCount {
                avibase_id: r.species_id,
                taxonomy_version: r.taxonomy_version,
                count,
                comment: r.comment,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    crate::bird_observations::update(
        state,
        search,
        generation,
        &id,
        &entry_id,
        &expected_token,
        &visit_date,
        &place_id,
        visit_start.as_deref(),
        visit_end.as_deref(),
        &rows,
    )
}

#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteInput {
    generation: u32,
    id: String,
    expected_token: String,
    confirmed: bool,
}
#[cfg(target_os = "macos")]
fn delete_in_session(
    state: &DatabaseState,
    search: &SearchState,
    input: DeleteInput,
) -> Result<(), String> {
    if !available(state) {
        return Err("bird_schema_unsupported".into());
    }
    crate::bird_observations::delete(
        state,
        search,
        input.generation,
        &input.id,
        &input.expected_token,
        input.confirmed,
    )
}
#[tauri::command]
pub(crate) fn delete_journal_bird_list(
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
    id: String,
    expected_token: String,
    confirmed: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        delete_in_session(
            &state,
            &search,
            DeleteInput {
                generation,
                id,
                expected_token,
                confirmed,
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, search, generation, id, expected_token, confirmed);
        Err("bird_platform_unsupported".into())
    }
}
#[tauri::command]
pub(crate) fn journal_bird_list_exists(
    state: State<'_, DatabaseState>,
    generation: u32,
    id: String,
) -> Result<Snapshot<bool>, String> {
    #[cfg(target_os = "macos")]
    {
        let exists = crate::bird_observations::exists(&state, generation, &id)?;
        Ok(state.operation(Some(generation))?.snapshot(exists))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, generation, id);
        Err("bird_platform_unsupported".into())
    }
}

#[tauri::command]
pub(crate) fn reserve_bird_creation(
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        crate::bird_observations::reserve(&state, &search, generation)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, search, generation);
        Err("bird_platform_unsupported".into())
    }
}
