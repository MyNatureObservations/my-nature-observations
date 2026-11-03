use super::*;
use crate::journal;
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    state: DatabaseState,
    search: SearchState,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("journal.sqlite");
        let db = Connection::open(&path).unwrap();
        database::initialize_schema(&db).unwrap();
        Self {
            _dir: dir,
            path,
            state: DatabaseState::ready_for_test(),
            search: SearchState::default(),
        }
    }
    fn open(&self) -> Result<Connection, String> {
        Connection::open(&self.path).map_err(|e| e.to_string())
    }
    fn insert(&self, id: &str, title: &str, text: &str, date: &str) {
        self.raw(id, title, &document(text), date);
    }
    fn raw(&self, id: &str, title: &str, raw: &str, date: &str) {
        self.open()
            .unwrap()
            .execute(
                "INSERT INTO journal_entries VALUES (?1,?2,?3,?4,1,'created','updated')",
                params![id, date, title, raw],
            )
            .unwrap();
    }
    fn find(&self, query: &str) -> Results {
        self.search
            .search(&self.state, || self.open(), query, 0)
            .unwrap()
            .data
    }
    fn save(&self, id: &str, text: &str) -> Result<journal::SavedJournalEntry, String> {
        let generation = self.state.operation(None)?.snapshot(()).generation;
        self.search.with_write(|| {
            journal::save_in_session(
                &self.state,
                generation,
                || self.open(),
                Some(id.into()),
                "2026-10-26".into(),
                "Saved title".into(),
                document(text),
            )
        })
    }
}
fn document(text: &str) -> String {
    serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}).to_string()
}
fn ids(results: Results) -> Vec<String> {
    results.entries.into_iter().map(|hit| hit.id).collect()
}

#[test]
fn title_body_prefix_and_swedish_letters() {
    let f = Fixture::new();
    f.insert("title", "Hackspettar", "vid ån", "2026-01-01");
    f.insert("body", "Skog", "En hackspett i björken", "2026-01-02");
    f.insert("other", "Ål", "al och örn", "2026-01-03");
    assert_eq!(f.find("hackspett").entries.len(), 2);
    assert_eq!(f.find("hackspett*").entries.len(), 2);
    assert_eq!(ids(f.find("BJÖRK")), ["body"]);
    assert_eq!(ids(f.find("ål")), ["other"]);
    assert!(f.find("bjork").entries.is_empty());
    assert!(f.find("spett").entries.is_empty());
}
#[test]
fn words_are_anded_phrases_are_literal_and_operators_are_not_executable() {
    let f = Fixture::new();
    f.insert("one", "", "stor hackspett vid ån", "2026-01-01");
    f.insert("two", "", "hackspett och stor örn", "2026-01-01");
    f.insert("three", "", "OR NEAR", "2026-01-01");
    assert_eq!(f.find("stor hackspett").entries.len(), 2);
    assert_eq!(ids(f.find("\"stor hackspett\"")), ["one"]);
    assert_eq!(ids(f.find("OR")), ["three"]);
    assert!(f.find("hackspett OR örn").entries.is_empty());
    assert!(f.find("\" OR 1=1 --").entries.is_empty());
    assert!(expression(&"x".repeat(513)).is_err());
    assert!(expression(&vec!["x"; 33].join(" ")).is_err());
    assert_eq!(expression(" * , ! ").unwrap(), "");
}
#[test]
fn title_weight_then_date_and_id_produce_stable_ranking() {
    let f = Fixture::new();
    f.insert("a", "hackspett", "skog", "2026-01-01");
    f.insert("b", "hackspett", "skog", "2026-02-01");
    f.insert("c", "hackspett", "skog", "2026-02-01");
    f.insert("body", "skog", "hackspett", "2026-12-01");
    assert_eq!(ids(f.find("hackspett")), ["b", "c", "a", "body"]);
}
#[test]
fn extraction_keeps_inline_words_and_block_boundaries_without_attrs_or_images() {
    let raw = r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"hack","marks":[{"type":"bold"}]},{"type":"text","text":"spett"}]},{"type":"bulletList","content":[{"type":"listItem","content":[{"type":"paragraph","content":[{"type":"text","text":"örn"}]}]}]},{"type":"image","attrs":{"src":"secret.jpg","alt":"imageword"}}]}"#;
    let text = document_text(raw);
    assert!(text.contains("hackspett\n"));
    assert!(text.contains("örn\n"));
    assert!(!text.contains("secret"));
    assert!(!text.contains("imageword"));
    assert!(!text.contains("paragraph"));
}
#[test]
fn legacy_unknown_and_duplicate_documents_are_read_without_rewriting() {
    let f = Fixture::new();
    let documents = [
        ("empty", "{}"),
        ("legacy", r#"{ "text" : "Original älg" }"#),
        (
            "unknown",
            r#"{"type":"doc","content":[{"type":"future","attrs":{"text":"secret"},"content":[{"type":"text","text":"Räv"}]}]}"#,
        ),
        ("duplicate", r#"{"text":"first","text":"second"}"#),
    ];
    for (id, raw) in documents {
        f.raw(id, "Legacy title", raw, "2026-01-01");
    }
    f.open()
        .unwrap()
        .execute(
            "UPDATE journal_entries SET document_version=99 WHERE id='unknown'",
            [],
        )
        .unwrap();
    let before = std::fs::read(&f.path).unwrap();
    assert_eq!(ids(f.find("älg")), ["legacy"]);
    assert_eq!(ids(f.find("räv")), ["unknown"]);
    assert_eq!(f.find("Legacy").entries.len(), 4);
    assert!(f.find("secret").entries.is_empty());
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    for (id, raw) in documents {
        let stored: String = f
            .open()
            .unwrap()
            .query_row(
                "SELECT content_json FROM journal_entries WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, raw);
    }
}
#[test]
fn actual_save_invalidates_cache_and_failed_save_does_not_destroy_it() {
    let f = Fixture::new();
    f.insert("entry", "Old", "gammal", "2026-01-01");
    f.find("gammal");
    assert_eq!(
        f.search
            .search(&f.state, || panic!("cache must be reused"), "gammal", 0)
            .unwrap()
            .data
            .entries
            .len(),
        1
    );
    f.save("entry", "nytt innehåll").unwrap();
    assert!(f.find("gammal").entries.is_empty());
    assert_eq!(ids(f.find("nytt")), ["entry"]);
    assert!(
        f.search
            .with_write(|| Err::<(), _>("write failed".into()))
            .is_err()
    );
    f.search
        .search(
            &f.state,
            || panic!("failed write must retain valid cache"),
            "nytt",
            0,
        )
        .unwrap();
}
#[test]
fn index_build_and_queries_do_not_change_source_bytes_or_schema() {
    let f = Fixture::new();
    f.insert("entry", "Title", "<b>örn</b>", "2026-01-01");
    let before = std::fs::read(&f.path).unwrap();
    let hit = f.find("örn").entries.remove(0);
    assert_eq!(hit.excerpt, "<b>örn</b>\n\n");
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert!(database::validate_schema(&f.open().unwrap()).unwrap());
}
#[test]
fn maintenance_recovery_and_stale_generation_never_open_source() {
    let f = Fixture::new();
    f.insert("entry", "", "gammal", "2026-01-01");
    f.find("gammal");
    let guarded = Arc::new(Fixture::new());
    guarded.insert("entry", "", "gammal", "2026-01-01");
    guarded.find("gammal");
    let maintenance = guarded.state.maintenance().unwrap();
    let (waiting, observed) = mpsc::channel();
    guarded.state.on_blocked(move || {
        waiting.send(()).unwrap();
    });
    let worker = {
        let guarded = guarded.clone();
        thread::spawn(move || {
            guarded.search.search(
                &guarded.state,
                || panic!("Recovery must reject even a cached search before opening"),
                "gammal",
                0,
            )
        })
    };
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    maintenance.require_recovery();
    drop(maintenance);
    assert!(
        matches!(worker.join().unwrap(), Err(e) if e == crate::database_state::RECOVERY_REQUIRED)
    );
    let blocked = DatabaseState::default();
    assert!(
        f.search
            .search(&blocked, || panic!("startup"), "gammal", 0)
            .is_err()
    );
    {
        let mut maintenance = f.state.maintenance().unwrap();
        maintenance.advance_generation().unwrap();
    }
    let rebuilt = f.search.search(&f.state, || f.open(), "gammal", 0).unwrap();
    assert_eq!(rebuilt.generation, 2);
    let stale = f.search.with_write(|| {
        journal::save_in_session(
            &f.state,
            1,
            || panic!("old generation"),
            Some("entry".into()),
            "2026-01-01".into(),
            "title".into(),
            document("changed"),
        )
    });
    assert!(matches!(stale, Err(e) if e == crate::database_state::GENERATION_CONFLICT));
}
#[test]
fn paging_and_rebuild_after_deleted_row() {
    let f = Fixture::new();
    for n in 0..52 {
        f.insert(&format!("{n:02}"), "Fågel", "text", "2026-01-01");
    }
    let first = f.find("fågel");
    assert_eq!(first.entries.len(), 50);
    assert!(first.more);
    let second = f
        .search
        .search(&f.state, || panic!("cached"), "fågel", 50)
        .unwrap()
        .data;
    assert_eq!(second.entries.len(), 2);
    assert!(!second.more);
    f.search
        .with_write(|| {
            f.open()?
                .execute("DELETE FROM journal_entries WHERE id='00'", [])
                .map_err(sql)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(f.find("fågel").entries[0].id, "01");
}
#[test]
fn simultaneous_build_and_save_cannot_publish_a_stale_cache_after_commit() {
    let f = Arc::new(Fixture::new());
    f.insert("entry", "", "gammal", "2026-01-01");
    let (building, wait_build) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let searcher = {
        let f = f.clone();
        thread::spawn(move || {
            f.search
                .search(
                    &f.state,
                    || {
                        building.send(()).unwrap();
                        wait_release.recv().unwrap();
                        f.open()
                    },
                    "gammal",
                    0,
                )
                .unwrap()
        })
    };
    wait_build.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        f.search.0.try_lock().is_err(),
        "real build must own the invalidation lock"
    );
    let (saved, wait_saved) = mpsc::channel();
    let writer = {
        let f = f.clone();
        thread::spawn(move || {
            f.save("entry", "nytt").unwrap();
            saved.send(()).unwrap();
        })
    };
    release.send(()).unwrap();
    searcher.join().unwrap();
    wait_saved.recv_timeout(Duration::from_secs(5)).unwrap();
    writer.join().unwrap();
    assert!(f.find("gammal").entries.is_empty());
    assert_eq!(ids(f.find("nytt")), ["entry"]);
}
