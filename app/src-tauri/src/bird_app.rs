//! Small Tauri boundary for journal-origin creation and read-only reopening.
use crate::{
    database_state::{DatabaseState, Snapshot},
    journal_search::SearchState,
};
use serde::{Deserialize, Serialize};
use tauri::State;
// Manual values and opaque preview IDs cross this boundary. Fetched JSON stays internal.
#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum WeatherInput {
    Set {
        context: crate::bird_visit_weather::Context,
        payload: serde_json::Value,
        confirmed: bool,
    },
    Remove {
        kind: String,
    },
    Fetched {
        preview_id: String,
    },
}
impl WeatherInput {
    fn change(self, id: &str) -> Result<crate::bird_visit_weather::Change, String> {
        use crate::bird_visit_weather::{Change, Record};
        Ok(match self {
            Self::Set {
                context,
                payload,
                confirmed,
            } => Change::Set {
                record: Record::new(id, "manual", &context, &payload.to_string())?,
                confirmed,
            },
            Self::Remove { kind } if matches!(kind.as_str(), "manual" | "fetched") => {
                Change::Remove(kind)
            }
            _ => return Err("visit_weather_change_invalid".into()),
        })
    }
}
#[cfg(target_os = "macos")]
fn resolve_weather(
    input: WeatherInput,
    previews: Option<&crate::bird_weather_fetch::PreviewState>,
    receipts: &mut Vec<crate::bird_weather_fetch::Receipt>,
    generation: u32,
    id: &str,
    expected: Option<&str>,
    date: &str,
    place: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<crate::bird_visit_weather::Change, String> {
    if let WeatherInput::Fetched { preview_id } = input {
        let receipt = previews.ok_or("bird_weather_preview_expired")?.resolve(
            &preview_id,
            generation,
            id,
            expected,
            date,
            place,
            start,
            end,
        )?;
        let change = crate::bird_visit_weather::Change::Set {
            record: receipt.record.clone(),
            confirmed: false,
        };
        receipts.push(receipt);
        Ok(change)
    } else {
        input.change(id)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RowInput {
    #[serde(default)]
    row_id: Option<String>,
    #[serde(default)]
    photo: Option<crate::bird_photos::Photo>,
    species_id: String,
    taxonomy_version: String,
    count: String,
    #[serde(default)]
    comment: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowView {
    row_id: Option<String>,
    photo: Option<crate::bird_photos::Photo>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<EbirdView>,
    edit_token: String,
    id: String,
    visit_date: String,
    journal_entry_ids: Vec<String>,
    visit_start: Option<String>,
    visit_end: Option<String>,
    place: crate::place_registry::Place,
    rows: Vec<RowView>,
    weather: Vec<crate::bird_visit_weather::Record>,
}
#[tauri::command]
pub(crate) fn bird_observations_available(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
) -> bool {
    available_with_module(&module, &state)
}
fn available_with_module(module: &crate::bird_module::BirdModule, state: &DatabaseState) -> bool {
    module.enabled() && available(state)
}
fn available(state: &DatabaseState) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    (|| -> Result<bool, String> {
        let path = state.ready_database_path()?;
        let _permit = state.operation(None)?;
        let db = crate::database::open_existing(&path)?;
        crate::database::ebird_schema::weather_schema(&db).map_err(|e| e.to_string())
    })()
    .unwrap_or(false)
}

#[tauri::command]
pub(crate) fn search_bird_species(
    module: State<'_, crate::bird_module::BirdModule>,
    query: String,
) -> Result<Vec<crate::species_registry::Species>, String> {
    search_bird_species_with_module(&module, query)
}
pub(crate) fn search_bird_species_with_module(
    module: &crate::bird_module::BirdModule,
    query: String,
) -> Result<Vec<crate::species_registry::Species>, String> {
    let _module_permit = module.command()?;
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    Ok(crate::species_registry::search(&query))
}
#[tauri::command]
pub(crate) fn match_bird_paste_names(
    module: State<'_, crate::bird_module::BirdModule>,
    names: Vec<String>,
) -> Result<Vec<Vec<crate::species_registry::Species>>, String> {
    match_bird_paste_names_with_module(&module, names)
}
pub(crate) fn match_bird_paste_names_with_module(
    module: &crate::bird_module::BirdModule,
    names: Vec<String>,
) -> Result<Vec<Vec<crate::species_registry::Species>>, String> {
    let _module_permit = module.command()?;
    if !cfg!(target_os = "macos") {
        return Err("bird_platform_unsupported".into());
    }
    if names.len() > 200 || names.iter().any(|name| name.len() > 200) {
        return Err("bird_paste_limit".into());
    }
    Ok(names
        .iter()
        .map(|name| crate::species_registry::exact(name))
        .collect())
}

#[tauri::command]
pub(crate) fn create_journal_bird_list(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    previews: State<'_, crate::bird_weather_fetch::PreviewState>,
    generation: u32,
    request_id: String,
    entry_id: Option<String>,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
    weather: Option<Vec<WeatherInput>>,
) -> Result<String, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        create_with_previews(
            &state,
            &search,
            Some(&previews),
            CreateInput {
                generation,
                request_id,
                entry_id,
                visit_date,
                place_id,
                rows,
                visit_start,
                visit_end,
                weather: weather.unwrap_or_default(),
            },
        )
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            state,
            search,
            previews,
            generation,
            request_id,
            entry_id,
            visit_date,
            place_id,
            rows,
            visit_start,
            visit_end,
            weather,
        );
        Err("bird_platform_unsupported".into())
    }
}
#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateInput {
    generation: u32,
    request_id: String,
    entry_id: Option<String>,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
    #[serde(default)]
    weather: Vec<WeatherInput>,
}
#[cfg(target_os = "macos")]
#[cfg(test)]
fn create_in_session(
    state: &DatabaseState,
    search: &SearchState,
    input: CreateInput,
) -> Result<String, String> {
    create_with_previews(state, search, None, input)
}
#[cfg(target_os = "macos")]
pub(crate) fn create_with_previews(
    state: &DatabaseState,
    search: &SearchState,
    previews: Option<&crate::bird_weather_fetch::PreviewState>,
    input: CreateInput,
) -> Result<String, String> {
    if !available(state) {
        return Err("bird_schema_unsupported".into());
    }
    let rows = validate_rows(input.rows)?;
    let mut receipts = Vec::new();
    let weather = input
        .weather
        .into_iter()
        .map(|w| {
            match resolve_weather(
                w,
                previews,
                &mut receipts,
                input.generation,
                &input.request_id,
                None,
                &input.visit_date,
                &input.place_id,
                input.visit_start.as_deref(),
                input.visit_end.as_deref(),
            )? {
                crate::bird_visit_weather::Change::Set { record, .. } => Ok(record),
                _ => Err("visit_weather_change_invalid".to_owned()),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    crate::bird_observations::create_with_weather_checked(
        state,
        search,
        input.generation,
        &input.visit_date,
        &input.place_id,
        &rows,
        input.entry_id.as_deref(),
        input.visit_start.as_deref(),
        input.visit_end.as_deref(),
        Some(&input.request_id),
        &weather,
        |db| {
            for receipt in receipts {
                receipt.check(db)?;
            }
            Ok(())
        },
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
                row_id: r.row_id,
                photo: r.photo,
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
            row_id: None,
            photo: None,
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
    #[test]
    fn manual_weather_command_contract_is_atomic_independent_and_requires_stale_confirmation() {
        let root = tempfile::TempDir::new().unwrap();
        let directory = root.path().join("journal");
        let state = DatabaseState::default();
        let path = state.initialize_startup(&directory).unwrap();
        let search = SearchState::default();
        let mut wire: serde_json::Value = serde_json::from_str(CREATE_WIRE).unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-12','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'c','u')",[input().entry_id]).unwrap();
        db.execute(
            "INSERT INTO places VALUES (?1,'Lake',59.0,18.0,1)",
            [input().place_id],
        )
        .unwrap();
        let journal_weather = r#"{"period":{"kind":"day","date":"2026-11-12"},"temperatureC":42}"#;
        db.execute(
            "INSERT INTO journal_weather VALUES (?1,1,?2,NULL,1)",
            rusqlite::params![input().entry_id, journal_weather],
        )
        .unwrap();
        db.close().map_err(|(_, e)| e).unwrap();
        let context = serde_json::json!({"date":"2026-11-12","place_id":wire["placeId"],"place_name":"Lake","latitude":59,"longitude":18,"start":"11:17","end":"11:20"});
        let manual = serde_json::json!({"action":"set","context":context,"payload":{"period":{"kind":"day","date":"2026-11-12"},"temperatureC":7,"windSpeedMs":0},"confirmed":false});
        wire["weather"] = serde_json::json!([manual]);
        crate::bird_observations::reserve_id(&state, &search, 1, &input().request_id).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut invalid = wire.clone();
        invalid["weather"][0]["payload"]["temperatureC"] = (-500).into();
        assert!(
            create_in_session(&state, &search, serde_json::from_value(invalid).unwrap()).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let id = create_in_session(
            &state,
            &search,
            serde_json::from_value(wire.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            create_in_session(
                &state,
                &search,
                serde_json::from_value(wire.clone()).unwrap()
            )
            .unwrap(),
            id
        );
        let view = read_view(&state, 1, None, &id).unwrap();
        assert!(!view.weather[0].stale);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&view.weather[0].payload_json).unwrap()["temperatureC"],
            7
        );
        let mut edit = wire.clone();
        edit.as_object_mut().unwrap().remove("requestId");
        edit["id"] = id.clone().into();
        edit["expectedToken"] = view.edit_token.into();
        edit["weather"] = serde_json::json!([]);
        edit["visitDate"] = "2026-11-13".into();
        let token = update_in_session(
            &state,
            &search,
            serde_json::from_value(edit.clone()).unwrap(),
        )
        .unwrap();
        let stale = read_view(&state, 1, None, &id).unwrap();
        assert!(stale.weather[0].stale);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stale.weather[0].context_json).unwrap()["date"],
            "2026-11-12"
        );
        edit["expectedToken"] = token.into();
        edit["weather"] = wire["weather"].clone();
        edit["weather"][0]["context"]["date"] = "2026-11-13".into();
        edit["weather"][0]["payload"]["period"]["date"] = "2026-11-13".into();
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            update_in_session(
                &state,
                &search,
                serde_json::from_value(edit.clone()).unwrap()
            )
            .unwrap_err(),
            "visit_weather_confirmation_required"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        edit["weather"][0]["confirmed"] = true.into();
        let token = update_in_session(
            &state,
            &search,
            serde_json::from_value(edit.clone()).unwrap(),
        )
        .unwrap();
        assert!(!read_view(&state, 1, None, &id).unwrap().weather[0].stale);
        drop(state);
        let state = DatabaseState::default();
        state.initialize_startup(&directory).unwrap();
        let view = read_view(&state, 1, None, &id).unwrap();
        assert_eq!(view.edit_token, token);
        assert!(!view.weather[0].stale);
        edit["expectedToken"] = token.into();
        edit["weather"] = serde_json::json!([{"action":"remove","kind":"manual"}]);
        let mut failed = edit.clone();
        failed["rows"][0]["count"] = "0".into();
        let before = std::fs::read(&path).unwrap();
        assert!(
            update_in_session(&state, &search, serde_json::from_value(failed).unwrap()).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut late = edit.clone();
        late["generation"] = 2.into();
        assert_eq!(
            update_in_session(&state, &search, serde_json::from_value(late).unwrap()).unwrap_err(),
            crate::database_state::GENERATION_CONFLICT
        );
        update_in_session(&state, &search, serde_json::from_value(edit).unwrap()).unwrap();
        assert!(read_view(&state, 1, None, &id).unwrap().weather.is_empty());
        let db = crate::database::open_existing(&path).unwrap();
        let preserved: String = db
            .query_row(
                "SELECT manual_json FROM journal_weather WHERE entry_id=?1",
                [input().entry_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(preserved, journal_weather);
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
            let found = search_bird_species_with_module(
                &crate::bird_module::test_module(true),
                query.into(),
            )
            .unwrap();
            assert_eq!(found[0].id, "avibase-FBBBA943", "query: {query}");
            assert_eq!(found[0].swedish.as_deref(), Some("skata"));
        }
        for query in ["kråka", "KRÅKA"] {
            let found = search_bird_species_with_module(
                &crate::bird_module::test_module(true),
                query.into(),
            )
            .unwrap();
            assert_eq!(found[0].id, "avibase-E4EE9AC4");
            assert_eq!(found[0].swedish.as_deref(), Some("Kråka"));
        }
        assert_eq!(
            search_bird_species_with_module(
                &crate::bird_module::test_module(true),
                "gråkråka".into()
            )
            .unwrap()[0]
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
        let view = read_view(&state, 1, input().entry_id.as_deref(), &id).unwrap();
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
                read_view(&state, 1, input().entry_id.as_deref(), &new_id)
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
            serde_json::to_value(
                read_view(&reopened, 1, input().entry_id.as_deref(), &id).unwrap()
            )
            .unwrap(),
            wire
        );
        let mut edit: serde_json::Value = serde_json::from_str(CREATE_WIRE).unwrap();
        edit.as_object_mut().unwrap().remove("requestId");
        edit["id"] = id.clone().into();
        edit["expectedToken"] = read_view(&reopened, 1, input().entry_id.as_deref(), &id)
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
        let updated = read_view(&reopened, 1, input().entry_id.as_deref(), &id).unwrap();
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
        let mut expected: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/bird-crow-view.json")).unwrap();
        request.rows = vec![RowInput {
            row_id: None,
            photo: None,
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
        expected["rowId"] = serde_json::to_value(
            &read_view(&state, 1, entry.as_deref(), &id).unwrap().rows[0].row_id,
        )
        .unwrap();
        expected["photo"] = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(&read_view(&state, 1, entry.as_deref(), &id).unwrap().rows[0])
                .unwrap(),
            expected
        );
        drop(state);
        let reopened = DatabaseState::default();
        reopened.initialize_startup(&root).unwrap();
        assert_eq!(
            serde_json::to_value(&read_view(&reopened, 1, entry.as_deref(), &id).unwrap().rows[0])
                .unwrap(),
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
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    generation: u32,
    entry_id: String,
) -> Result<Snapshot<Vec<ListView>>, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        // Each read checks the same generation; do not nest operation permits.
        let path = state.ready_database_path()?;
        let _permit = state.operation(Some(generation))?;
        let db = crate::database::open_existing(&path)?;
        let mut ids=db.prepare("SELECT list_id FROM journal_bird_lists WHERE entry_id=?1 ORDER BY list_id COLLATE BINARY").map_err(|e|e.to_string())?.query_map([&entry_id],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
        if crate::database::ebird_schema::validate_schema(&db).map_err(|e| e.to_string())? {
            ids.extend(
                db.prepare(
                    "SELECT visit_id FROM journal_ebird_visits WHERE entry_id=?1 ORDER BY visit_id",
                )
                .map_err(|e| e.to_string())?
                .query_map([&entry_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?,
            );
        }
        drop(db);
        drop(_permit);
        let mut lists = Vec::new();
        for id in ids {
            lists.push(read_view(&state, generation, Some(&entry_id), &id)?);
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
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    generation: u32,
    entry_id: Option<String>,
    id: String,
) -> Result<Snapshot<ListView>, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        let view = read_view(&state, generation, entry_id.as_deref(), &id)?;
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
    entry: Option<&str>,
    id: &str,
) -> Result<ListView, String> {
    if let Some(source) = read_ebird_view(state, generation, entry, id)? {
        return Ok(source);
    }
    let list = crate::bird_observations::reopen(state, generation, id)?;
    if entry.is_some_and(|entry| !list.journal_entries.iter().any(|e| e == entry)) {
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
                row_id: r.row_id,
                photo: r.photo,
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
        source: None,
        edit_token,
        id: list.id,
        visit_date: list.visit_date,
        journal_entry_ids: list.journal_entries,
        visit_start: list.visit_start,
        visit_end: list.visit_end,
        place,
        rows,
        weather: list.weather,
    })
}

#[tauri::command]
pub(crate) fn update_journal_bird_list(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    previews: State<'_, crate::bird_weather_fetch::PreviewState>,
    generation: u32,
    id: String,
    entry_id: Option<String>,
    expected_token: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
    weather: Option<Vec<WeatherInput>>,
) -> Result<String, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        update_with_previews(
            &state,
            &search,
            Some(&previews),
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
                weather: weather.unwrap_or_default(),
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            state,
            search,
            previews,
            generation,
            id,
            entry_id,
            expected_token,
            visit_date,
            place_id,
            rows,
            visit_start,
            visit_end,
            weather,
        );
        Err("bird_platform_unsupported".into())
    }
}

#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditInput {
    generation: u32,
    id: String,
    entry_id: Option<String>,
    expected_token: String,
    visit_date: String,
    place_id: String,
    rows: Vec<RowInput>,
    visit_start: Option<String>,
    visit_end: Option<String>,
    #[serde(default)]
    weather: Vec<WeatherInput>,
}
#[cfg(target_os = "macos")]
#[cfg(test)]
fn update_in_session(
    state: &DatabaseState,
    search: &SearchState,
    input: EditInput,
) -> Result<String, String> {
    update_with_previews(state, search, None, input)
}
#[cfg(target_os = "macos")]
pub(crate) fn update_with_previews(
    state: &DatabaseState,
    search: &SearchState,
    previews: Option<&crate::bird_weather_fetch::PreviewState>,
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
        weather,
    } = input;
    let rows = rows
        .into_iter()
        .map(|r| {
            let count = r.count.parse::<i64>().map_err(|_| "bird_count_invalid")?;
            if count <= 0 || count.to_string() != r.count {
                return Err("bird_count_invalid".to_owned());
            }
            Ok(crate::bird_observations::SpeciesCount {
                row_id: r.row_id,
                photo: r.photo,
                avibase_id: r.species_id,
                taxonomy_version: r.taxonomy_version,
                count,
                comment: r.comment,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut receipts = Vec::new();
    let weather = weather
        .into_iter()
        .map(|w| {
            resolve_weather(
                w,
                previews,
                &mut receipts,
                generation,
                &id,
                Some(&expected_token),
                &visit_date,
                &place_id,
                visit_start.as_deref(),
                visit_end.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    crate::bird_observations::update_with_weather_checked(
        state,
        search,
        generation,
        &id,
        entry_id.as_deref(),
        &expected_token,
        &visit_date,
        &place_id,
        visit_start.as_deref(),
        visit_end.as_deref(),
        &rows,
        &weather,
        |db| {
            for receipt in receipts {
                receipt.check(db)?;
            }
            Ok(())
        },
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
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
    id: String,
    expected_token: String,
    confirmed: bool,
) -> Result<(), String> {
    let _module_permit = module.command()?;
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
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    generation: u32,
    id: String,
) -> Result<Snapshot<bool>, String> {
    let _module_permit = module.command()?;
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
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
) -> Result<String, String> {
    let _module_permit = module.command()?;
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

#[cfg(target_os = "macos")]
fn all_views(state: &DatabaseState, generation: u32) -> Result<Vec<ListView>, String> {
    let path = state.ready_database_path()?;
    let permit = state.operation(Some(generation))?;
    let db = crate::database::open_existing(&path)?;
    // Select owners, not joined links: each list appears once, including orphans.
    let ids=db.prepare("SELECT id FROM bird_lists ORDER BY visit_date DESC,visit_start IS NULL,visit_start,id COLLATE BINARY").map_err(|e|e.to_string())?.query_map([],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
    drop(db);
    drop(permit);
    let mut all = ids
        .into_iter()
        .map(|id| read_view(state, generation, None, &id))
        .collect::<Result<Vec<_>, _>>()?;
    let path = state.ready_database_path()?;
    let _operation = state.operation(Some(generation))?;
    let mut db = crate::database::open_existing(&path)?;
    if crate::database::ebird_schema::validate_schema(&db).map_err(|e| e.to_string())? {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        crate::storage_content::snapshot_v14(&tx, &std::sync::atomic::AtomicBool::new(false))
            .map_err(|_| "bird_list_invalid")?;
        let records = crate::ebird_persistence::records(&tx)?;
        for record in &records {
            if let crate::ebird_persistence::Record::Visit { id, .. } = record {
                if let Some(view) = ebird_view_from_records(&records, None, id)? {
                    all.push(view);
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(all)
}

#[tauri::command]
pub(crate) fn read_bird_lists(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    generation: u32,
) -> Result<Snapshot<Vec<ListView>>, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        let lists = all_views(&state, generation)?;
        Ok(state.operation(Some(generation))?.snapshot(lists))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, generation);
        Err("bird_platform_unsupported".into())
    }
}
#[tauri::command]
pub(crate) fn set_journal_bird_link(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    search: State<'_, SearchState>,
    generation: u32,
    entry_id: String,
    id: String,
    linked: bool,
) -> Result<(), String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        set_link_for_visit(&state, &search, generation, &id, &entry_id, linked)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, search, generation, entry_id, id, linked);
        Err("bird_platform_unsupported".into())
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn set_link_for_visit(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry_id: &str,
    linked: bool,
) -> Result<(), String> {
    if !available(state) {
        return Err("bird_schema_unsupported".into());
    }
    if read_ebird_view(state, generation, None, id)?.is_some() {
        return set_ebird_link(state, search, generation, id, entry_id, linked);
    }
    crate::bird_observations::set_link(state, search, generation, id, entry_id, linked)
}

#[cfg(all(test, target_os = "macos"))]
mod overview_tests {
    use super::*;
    use crate::bird_observations as birds;
    use rusqlite::{Connection, OpenFlags};
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, atomic::AtomicBool},
    };
    fn fixture() -> (tempfile::TempDir, DatabaseState, PathBuf, SearchState) {
        let temp = tempfile::TempDir::new().unwrap();
        let state = DatabaseState::default();
        let path = state
            .initialize_startup(&temp.path().join("journal"))
            .unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        db.execute(
            "INSERT INTO places VALUES ('11111111-1111-4111-8111-111111111111','Synthetic lake',59.33,18.07,1)",
            [],
        )
        .unwrap();
        db.close().unwrap();
        (temp, state, path, SearchState::default())
    }
    fn entry(path: &std::path::Path, id: &str) {
        let db = crate::database::open_existing(path).unwrap();
        db.execute("INSERT INTO journal_entries VALUES (?1,'2026-11-13','Synthetic','{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\"}]}',2,'created','updated')",[id]).unwrap();
        db.close().unwrap();
    }
    fn create(state: &DatabaseState, search: &SearchState) -> String {
        let request = birds::reserve(state, search, 1).unwrap();
        // Flat JSON is the real Tauri command contract, explicitly without an entry.
        let input=serde_json::from_value(serde_json::json!({"generation":1,"requestId":request,"entryId":null,"visitDate":"2026-11-13","placeId":"11111111-1111-4111-8111-111111111111","visitStart":"08:07","visitEnd":null,"rows":[{"speciesId":"avibase-FBBBA943","taxonomyVersion":"AviList v2026","count":"6","comment":"Synthetic comment"}]})).unwrap();
        create_in_session(state, search, input).unwrap()
    }
    #[test]
    fn standalone_command_and_overview_preserve_identity_links_and_restart() {
        let (temp, state, path, search) = fixture();
        let id = create(&state, &search);
        let db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        db.close().unwrap();
        let saved = birds::reopen(&state, 1, &id).unwrap();
        assert!(saved.journal_entries.is_empty());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            birds::create_with_identity(
                &state,
                &search,
                1,
                &saved.visit_date,
                &saved.place_id,
                &saved.rows,
                None,
                saved.visit_start.as_deref(),
                saved.visit_end.as_deref(),
                Some(&id)
            )
            .unwrap(),
            id
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);

        assert_eq!(saved.rows[0].comment.as_deref(), Some("Synthetic comment"));
        entry(&path, "first");
        entry(&path, "second");
        for e in ["first", "first", "second"] {
            birds::set_link(&state, &search, 1, &id, e, true).unwrap();
        }
        assert_eq!(all_views(&state, 1).unwrap().len(), 1);
        assert_eq!(
            birds::reopen(&state, 1, &id).unwrap().journal_entries,
            vec!["first", "second"]
        );
        let mut edit = serde_json::json!({"generation":1,"entryId":null,"id":id,"expectedToken":read_view(&state,1,None,&id).unwrap().edit_token,"visitDate":"2026-11-14","placeId":"11111111-1111-4111-8111-111111111111","visitStart":null,"visitEnd":"19:59","rows":[{"speciesId":"avibase-FBBBA943","taxonomyVersion":"AviList v2026","count":"8","comment":"Updated"}]});
        update_in_session(
            &state,
            &search,
            serde_json::from_value(edit.clone()).unwrap(),
        )
        .unwrap();
        let updated = birds::reopen(&state, 1, &id).unwrap();
        assert_eq!(updated.journal_entries, vec!["first", "second"]);
        assert_eq!(updated.rows[0].count, 8);
        // Journal-origin access requires the actual link, while standalone access does not.
        birds::set_link(&state, &search, 1, &id, "first", false).unwrap();
        birds::set_link(&state, &search, 1, &id, "first", false).unwrap();
        assert!(read_view(&state, 1, Some("first"), &id).is_err());
        assert!(read_view(&state, 1, Some("second"), &id).is_ok());
        assert!(read_view(&state, 1, None, &id).is_ok());
        edit["entryId"] = "first".into();
        edit["expectedToken"] = read_view(&state, 1, None, &id).unwrap().edit_token.into();
        assert!(update_in_session(&state, &search, serde_json::from_value(edit).unwrap()).is_err());
        // Delete the remaining journal owner: its observation survives and stays discoverable.
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("DELETE FROM journal_entries WHERE id='second'", [])
            .unwrap();
        db.close().unwrap();
        assert!(
            birds::reopen(&state, 1, &id)
                .unwrap()
                .journal_entries
                .is_empty()
        );
        assert_eq!(all_views(&state, 1).unwrap().len(), 1);
        let other = create(&state, &search);
        assert_ne!(other, id);
        assert_eq!(all_views(&state, 1).unwrap().len(), 2);
        drop(state);
        let state = DatabaseState::default();
        state
            .initialize_startup(&temp.path().join("journal"))
            .unwrap();
        assert_eq!(birds::reopen(&state, 1, &id).unwrap().rows[0].count, 8);
        assert_eq!(all_views(&state, 1).unwrap().len(), 2);
        delete_in_session(
            &state,
            &search,
            DeleteInput {
                generation: 1,
                id: id.clone(),
                expected_token: read_view(&state, 1, None, &id).unwrap().edit_token,
                confirmed: true,
            },
        )
        .unwrap();
        assert!(read_view(&state, 1, None, &id).is_err());
        assert_eq!(
            birds::create_with_identity(
                &state,
                &search,
                1,
                &saved.visit_date,
                &saved.place_id,
                &saved.rows,
                None,
                saved.visit_start.as_deref(),
                saved.visit_end.as_deref(),
                Some(&id)
            )
            .unwrap_err(),
            "bird_request_retired"
        );

        assert_eq!(all_views(&state, 1).unwrap()[0].id, other);
        let db = crate::database::open_existing(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM places", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.close().unwrap();
    }
    #[test]
    fn standalone_and_multi_linked_lists_are_in_current_archives() {
        let (temp, state, path, search) = fixture();
        let unlinked = create(&state, &search);
        let linked = create(&state, &search);
        entry(&path, "first");
        entry(&path, "second");
        for e in ["first", "second"] {
            birds::set_link(&state, &search, 1, &linked, e, true).unwrap();
        }
        let mut db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let proof = crate::storage_content::verify_v14(&mut db, &AtomicBool::new(false)).unwrap();
        let backup = temp.path().join("visits.mno-backup");
        crate::backup::create_backup(&db, &backup, crate::backup::BackupKind::Manual).unwrap();
        let export = temp.path().join("visits.mno-export");
        crate::open_export::export(&mut db, &export, &AtomicBool::new(false)).unwrap();
        db.close().unwrap();
        let candidate =
            crate::backup::restore::prepare_candidate(&backup, Arc::new(AtomicBool::new(false)))
                .unwrap();
        assert_eq!(
            crate::storage_content::verified_snapshot_v14(
                candidate.source().unwrap(),
                &AtomicBool::new(false)
            )
            .unwrap(),
            proof
        );
        for (id, expected) in [(&unlinked, 0), (&linked, 2)] {
            assert_eq!(
                candidate
                    .source()
                    .unwrap()
                    .query_row(
                        "SELECT count(*) FROM journal_bird_lists WHERE list_id=?1",
                        [id],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                expected
            );
        }
        drop(candidate);
        let imported = crate::open_import::build_for_schema13(
            &mut fs::File::open(export).unwrap(),
            temp.path(),
            &AtomicBool::new(false),
            14,
        )
        .unwrap();
        assert_eq!(imported.proof().sha256, proof.sha256);
        let mut db = crate::database::open_existing(imported.path()).unwrap();
        crate::storage_content::verify_v14(&mut db, &AtomicBool::new(false)).unwrap();
        db.close().unwrap();
    }
    #[test]
    fn overview_orders_null_times_last_and_detail_journal_targets_preserve_owners() {
        let (_temp, state, path, search) = fixture();
        let mut ids = Vec::new();
        for _ in 0..5 {
            ids.push(create(&state, &search));
        }
        let db = crate::database::open_existing(&path).unwrap();
        for (i, (date, time)) in [
            ("2026-11-13", None),
            ("2026-11-13", Some("18:07")),
            ("2026-11-13", Some("08:15")),
            ("2026-11-13", Some("08:15")),
            ("2026-11-12", Some("00:01")),
        ]
        .iter()
        .enumerate()
        {
            db.execute(
                "UPDATE bird_lists SET visit_date=?2,visit_start=?3 WHERE id=?1",
                rusqlite::params![ids[i], date, time],
            )
            .unwrap();
        }
        db.close().unwrap();
        let mut ties = vec![ids[2].clone(), ids[3].clone()];
        ties.sort();
        assert_eq!(
            all_views(&state, 1)
                .unwrap()
                .into_iter()
                .map(|v| v.id)
                .collect::<Vec<_>>(),
            vec![
                ties[0].clone(),
                ties[1].clone(),
                ids[1].clone(),
                ids[0].clone(),
                ids[4].clone()
            ]
        );
        assert!(journal_targets(&state, 1, &ids[0]).unwrap().is_empty());
        entry(&path, "first");
        entry(&path, "second");
        let db = crate::database::open_existing(&path).unwrap();
        db.execute("UPDATE journal_entries SET title='',content_json='{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"Useful journal preview\"}]}]}' WHERE id='first'",[]).unwrap();
        db.close().unwrap();
        birds::set_link(&state, &search, 1, &ids[0], "first", true).unwrap();
        birds::set_link(&state, &search, 1, &ids[0], "first", true).unwrap();
        let targets = journal_targets(&state, 1, &ids[0]).unwrap();
        assert_eq!(targets.len(), 2);
        let first = targets.iter().find(|e| e.id == "first").unwrap();
        assert!(first.linked);
        assert_eq!(first.preview, "Useful journal preview");
        assert!(first.title.is_empty());
        assert!(!targets.iter().find(|e| e.id == "second").unwrap().linked);
        birds::set_link(&state, &search, 1, &ids[0], "second", true).unwrap();
        birds::set_link(&state, &search, 1, &ids[0], "first", false).unwrap();
        let list = birds::reopen(&state, 1, &ids[0]).unwrap();
        assert_eq!(list.journal_entries, vec!["second"]);
        let new_token = birds::update_in_context(
            &state,
            &search,
            1,
            &list.id,
            None,
            &birds::edit_token(&list),
            &list.visit_date,
            &list.place_id,
            list.visit_start.as_deref(),
            list.visit_end.as_deref(),
            &list.rows,
        )
        .unwrap();
        assert_eq!(new_token, birds::edit_token(&list));
        let before = fs::read(&path).unwrap();
        assert!(journal_targets(&state, 999, &ids[0]).is_err());
        assert!(journal_targets(&state, 1, "missing").is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalTarget {
    pub(crate) id: String,
    entry_date: String,
    title: String,
    preview: String,
    pub(crate) linked: bool,
}
#[cfg(target_os = "macos")]
pub(crate) fn journal_targets(
    state: &DatabaseState,
    generation: u32,
    id: &str,
) -> Result<Vec<JournalTarget>, String> {
    let path = state.ready_database_path()?;
    let _permit = state.operation(Some(generation))?;
    let mut db = crate::database::open_existing(&path)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    (if crate::database::ebird_schema::validate_schema(&tx).map_err(|_| "bird_list_invalid")? {
        crate::storage_content::snapshot_v14(&tx, &std::sync::atomic::AtomicBool::new(false))
    } else {
        crate::storage_content::snapshot_v13(&tx, &std::sync::atomic::AtomicBool::new(false))
    })
    .map_err(|_| "bird_list_invalid")?;
    let imported: bool =
        if crate::database::ebird_schema::validate_schema(&tx).map_err(|e| e.to_string())? {
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM ebird_visits WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?
        } else {
            false
        };
    if !imported
        && !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bird_lists WHERE id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
    {
        return Err("bird_list_missing".into());
    }
    let rows=tx.prepare(if imported {"SELECT e.id,e.entry_date,e.title,e.content_json,EXISTS(SELECT 1 FROM journal_ebird_visits l WHERE l.entry_id=e.id AND l.visit_id=?1) FROM journal_entries e ORDER BY e.entry_date DESC,e.created_at DESC,e.id COLLATE BINARY"} else {"SELECT e.id,e.entry_date,e.title,e.content_json,EXISTS(SELECT 1 FROM journal_bird_lists l WHERE l.entry_id=e.id AND l.list_id=?1) FROM journal_entries e ORDER BY e.entry_date DESC,e.created_at DESC,e.id COLLATE BINARY"}).map_err(|e|e.to_string())?.query_map([id],|r|{
        let raw:String=r.get(3)?;
        let text=crate::journal_search::document_text(&raw).split_whitespace().collect::<Vec<_>>().join(" ");
        let preview=text.chars().take(160).collect::<String>();
        Ok(JournalTarget{id:r.get(0)?,entry_date:r.get(1)?,title:r.get(2)?,preview,linked:r.get(4)?})
    }).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(rows)
}
#[tauri::command]
pub(crate) fn read_bird_journal_entries(
    module: State<'_, crate::bird_module::BirdModule>,
    state: State<'_, DatabaseState>,
    generation: u32,
    id: String,
) -> Result<Snapshot<Vec<JournalTarget>>, String> {
    let _module_permit = module.command()?;
    #[cfg(target_os = "macos")]
    {
        let entries = journal_targets(&state, generation, &id)?;
        Ok(state.operation(Some(generation))?.snapshot(entries))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, generation, id);
        Err("bird_platform_unsupported".into())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod photo_contract_tests {
    use super::*;
    #[test]
    fn photo_creation_command_json_saves_and_reopens_owned_preview() {
        let temp = tempfile::TempDir::new().unwrap();
        let state = DatabaseState::default();
        let path = state
            .initialize_startup(&temp.path().join("journal"))
            .unwrap();
        let db = crate::database::open_existing(&path).unwrap();
        let place = "11111111-1111-4111-8111-111111111111";
        db.execute(
            "INSERT INTO places VALUES(?1,'Test place',59,18,1)",
            [place],
        )
        .unwrap();
        db.close().unwrap();
        let original = temp.path().join("photo.png");
        image::RgbImage::from_pixel(5, 4, image::Rgb([20, 40, 60]))
            .save(&original)
            .unwrap();
        let bytes = std::fs::read(&original).unwrap();
        let row_id = uuid::Uuid::new_v4().to_string();
        let mut photo = crate::bird_photos::Photo::from_asset(
            crate::photographs::prepare(&original, row_id.clone()).unwrap(),
        );
        photo.caption = Some("A caption, not a row comment".into());
        let search = SearchState::default();
        let request = crate::bird_observations::reserve(&state, &search, 1).unwrap();
        let json = serde_json::json!({"generation":1,"requestId":request,"entryId":null,"visitDate":"2026-11-17","placeId":place,"visitStart":"08:07","visitEnd":null,"rows":[{"speciesId":"avibase-FBBBA943","taxonomyVersion":crate::species_registry::VERSION,"count":"4","comment":"Keep comment","rowId":row_id,"photo":photo}]});
        let id = create_in_session(
            &state,
            &search,
            serde_json::from_value(json.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(id, request);
        let view = read_view(&state, 1, None, &id).unwrap();
        assert_eq!(view.rows[0].row_id.as_ref(), Some(&row_id));
        assert_eq!(view.rows[0].photo.as_ref(), Some(&photo));
        assert_eq!(view.rows[0].comment.as_deref(), Some("Keep comment"));
        assert_eq!(view.rows[0].count, "4");
        assert_eq!(
            create_in_session(&state, &search, serde_json::from_value(json).unwrap()).unwrap(),
            id
        );
        assert_eq!(std::fs::read(original).unwrap(), bytes);
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "bird_app/schema13_tests.rs"]
mod schema13_tests;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EbirdRow {
    fields: crate::ebird_persistence::Fields,
    additions: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EbirdView {
    submission_id: String,
    fields: crate::ebird_persistence::Fields,
    rows: Vec<EbirdRow>,
    unresolved: Vec<EbirdRow>,
    additions: Vec<String>,
}
#[cfg(target_os = "macos")]
fn read_ebird_view(
    state: &DatabaseState,
    generation: u32,
    entry: Option<&str>,
    id: &str,
) -> Result<Option<ListView>, String> {
    use crate::ebird_persistence as store;
    let path = state.ready_database_path()?;
    let _operation = state.operation(Some(generation))?;
    let mut db = crate::database::open_existing(&path)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    if !crate::database::ebird_schema::validate_schema(&tx).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    if !tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ebird_visits WHERE id=?1)",
            [id],
            |r| r.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    crate::storage_content::snapshot_v14(&tx, &std::sync::atomic::AtomicBool::new(false))
        .map_err(|_| "bird_list_invalid")?;
    let all = store::records(&tx)?;
    let view = ebird_view_from_records(&all, entry, id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(view)
}
#[cfg(target_os = "macos")]
fn ebird_view_from_records(
    all: &[crate::ebird_persistence::Record],
    entry: Option<&str>,
    id: &str,
) -> Result<Option<ListView>, String> {
    use crate::ebird_persistence::{self as store, Record};
    let Some((submission, fields)) = all.iter().find_map(|r| {
        if let Record::Visit {
            id: owner,
            submission_id,
            source_json,
        } = r
        {
            (owner == id).then_some((submission_id, source_json))
        } else {
            None
        }
    }) else {
        return Ok(None);
    };
    if entry.is_some_and(|e| {
        !all.iter()
            .any(|r| matches!(r,Record::Link{entry_id,visit_id} if entry_id==e&&visit_id==id))
    }) {
        return Err("bird_link_missing".into());
    }
    let fields: store::Fields = serde_json::from_str(fields).map_err(|e| e.to_string())?;
    let mut source_rows = all
        .iter()
        .filter_map(|r| {
            if let Record::Row {
                id: row,
                visit_id,
                position,
                active,
                source_json,
            } = r
            {
                (visit_id == id).then_some((row, *position, *active, source_json))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    source_rows.sort_by_key(|r| (r.1, r.0));
    let mut rows = Vec::new();
    let mut active = Vec::new();
    let mut unresolved = Vec::new();
    for (row, _, enabled, json) in source_rows {
        let f: store::Fields = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let additions = all
            .iter()
            .filter_map(|r| {
                if let Record::Addition {
                    row_id: Some(r),
                    content,
                    ..
                } = r
                {
                    (r == row).then_some(content.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if enabled {
            rows.push(RowView {
                row_id: Some(row.clone()),
                photo: None,
                species_id: String::new(),
                taxonomy_version: String::new(),
                count: f["Count"].clone(),
                comment: f.get("Observation Details").cloned(),
                english: f["Common Name"].clone(),
                swedish: None,
                scientific: f["Scientific Name"].clone(),
            });
            active.push(EbirdRow {
                fields: f,
                additions,
            });
        } else if !additions.is_empty() {
            unresolved.push(EbirdRow {
                fields: f,
                additions,
            });
        }
    }
    let place = crate::place_registry::Place {
        id: format!("ebird:{}", fields["Location ID"]),
        name: fields["Location"].clone(),
        latitude: Some(
            fields["Latitude"]
                .parse()
                .map_err(|_| "bird_list_invalid")?,
        ),
        longitude: Some(
            fields["Longitude"]
                .parse()
                .map_err(|_| "bird_list_invalid")?,
        ),
        revision: "0".into(),
    };
    let start = fields.get("Time").and_then(|s| {
        let (t, p) = s.split_once(' ')?;
        let (h, m) = t.split_once(':')?;
        let h = h.parse::<u32>().ok()?;
        Some(format!(
            "{:02}:{m}",
            h % 12 + if p == "PM" { 12 } else { 0 }
        ))
    });
    let view = ListView {
        source: Some(EbirdView {
            submission_id: submission.clone(),
            fields: fields.clone(),
            rows: active,
            unresolved,
            additions: all
                .iter()
                .filter_map(|r| {
                    if let Record::Addition {
                        visit_id,
                        row_id: None,
                        content,
                        ..
                    } = r
                    {
                        (visit_id == id).then_some(content.clone())
                    } else {
                        None
                    }
                })
                .collect(),
        }),
        edit_token: String::new(),
        id: id.into(),
        visit_date: fields["Date"].clone(),
        journal_entry_ids: all
            .iter()
            .filter_map(|record| match record {
                Record::Link { entry_id, visit_id } if visit_id == id => Some(entry_id.clone()),
                _ => None,
            })
            .collect(),
        visit_start: start,
        visit_end: None,
        place,
        rows,
        weather: Vec::new(),
    };
    Ok(Some(view))
}
#[cfg(target_os = "macos")]
fn set_ebird_link(
    state: &DatabaseState,
    search: &SearchState,
    generation: u32,
    id: &str,
    entry: &str,
    linked: bool,
) -> Result<(), String> {
    search.with_write(||{let path=state.ready_database_path()?;let _operation=state.operation(Some(generation))?;let mut db=crate::database::open_existing(&path)?;let tx=db.transaction().map_err(|e|e.to_string())?;
 if linked{tx.execute("INSERT INTO journal_ebird_visits(entry_id,visit_id) VALUES(?1,?2) ON CONFLICT DO NOTHING",rusqlite::params![entry,id]).map_err(|e|e.to_string())?;}else{tx.execute("DELETE FROM journal_ebird_visits WHERE entry_id=?1 AND visit_id=?2",rusqlite::params![entry,id]).map_err(|e|e.to_string())?;}
 crate::storage_content::snapshot_v14(&tx,&std::sync::atomic::AtomicBool::new(false)).map_err(|_|"bird_list_invalid")?;tx.commit().map_err(|e|e.to_string())})
}
