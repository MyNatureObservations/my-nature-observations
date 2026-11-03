use super::*;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
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
        }
    }
    fn open(&self) -> Result<Connection, String> {
        Connection::open(&self.path).map_err(|e| e.to_string())
    }
    fn insert(&self, id: &str, date: &str, created: &str) {
        self.open()
            .unwrap()
            .execute(
                "INSERT INTO journal_entries VALUES (?1,?2,?3,?4,1,?5,'1900-01-01T00:00:00Z')",
                params![id, date, format!("Title {id}"), "{}", created],
            )
            .unwrap();
    }
    fn overview(&self, now: &str) -> Overview {
        overview_in_session(&self.state, || self.open(), stamp(now))
            .unwrap()
            .data
    }
    fn month(&self, year: Option<i32>, month: Option<u8>, offset: u32) -> Entries {
        entries_in_session(&self.state, || self.open(), year, month, offset)
            .unwrap()
            .data
    }
}
fn stamp(raw: &str) -> OffsetDateTime {
    OffsetDateTime::parse(raw, &Rfc3339).unwrap()
}
const NOW: &str = "2026-03-31T12:00:00Z";
const OLD: &str = "2026-03-01T12:00:00Z";
const RECENT: &str = "2026-03-30T12:00:00Z";

#[test]
fn empty_journal_is_hidden_but_direct_history_reads_are_allowed() {
    let f = Fixture::new();
    let overview = f.overview(NOW);
    assert_eq!(
        overview,
        Overview {
            total_entries: 0,
            navigation_visible: false,
            earliest_created_at: None,
            years: vec![],
            undated_count: 0
        }
    );
    assert!(f.month(Some(2026), Some(3), 0).entries.is_empty());
    assert!(f.month(None, None, 0).entries.is_empty());
    f.insert("one", "2026-03-01", RECENT);
    assert!(!f.overview(NOW).navigation_visible);
    assert_eq!(
        f.month(Some(2026), Some(3), 0).entries.len(),
        1,
        "visibility is not admission"
    );
}
#[test]
fn years_months_counts_and_entry_order_are_stable_and_descending() {
    let f = Fixture::new();
    for (id, date, created) in [
        ("a", "2026-12-31", OLD),
        ("b", "2026-12-31", RECENT),
        ("c", "2026-12-31", RECENT),
        ("d", "2026-01-01", OLD),
        ("e", "2025-12-31", OLD),
        ("f", "2030-01-01", OLD),
    ] {
        f.insert(id, date, created);
    }
    let o = f.overview(NOW);
    assert_eq!(
        o.years.iter().map(|y| y.year).collect::<Vec<_>>(),
        [2030, 2026, 2025]
    );
    assert_eq!(
        o.years[1],
        Year {
            year: 2026,
            count: 4,
            months: vec![
                MonthCount {
                    month: 12,
                    count: 3
                },
                MonthCount { month: 1, count: 1 }
            ]
        }
    );
    assert_eq!(
        f.month(Some(2026), Some(12), 0)
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "c", "a"]
    );
    assert_eq!(o.total_entries, 6);
}
#[test]
fn count_threshold_and_exact_thirty_day_boundary_use_created_not_experience_or_update() {
    let f = Fixture::new();
    for id in ["a", "b", "c", "d"] {
        f.insert(id, "1980-01-01", OLD);
    }
    assert!(!f.overview(NOW).navigation_visible);
    f.insert("empty", "1980-01-01", OLD);
    let before = stamp(NOW) - time::Duration::nanoseconds(1);
    assert!(
        !overview_in_session(&f.state, || f.open(), before)
            .unwrap()
            .data
            .navigation_visible
    );
    assert!(
        f.overview(NOW).navigation_visible,
        "empty stored JSON objects count too"
    );
    let recent = Fixture::new();
    for id in ["a", "b", "c", "d", "e"] {
        recent.insert(id, "1900-01-01", RECENT);
    }
    assert!(
        !recent.overview(NOW).navigation_visible,
        "old experience/update dates do not age the journal"
    );
}
#[test]
fn invalid_created_values_are_ignored_and_offsets_compare_as_utc_instants() {
    let f = Fixture::new();
    for (id, created) in [
        ("bad", "not a timestamp"),
        ("date-only", "1900-01-01"),
        ("bad-month", "1900-13-01T00:00:00Z"),
        ("future", "2030-01-01T00:00:00Z"),
        ("offset", "2026-03-01T13:00:00+01:00"),
    ] {
        f.insert(id, "2026-01-01", created);
    }
    let o = f.overview(NOW);
    assert_eq!(
        o.earliest_created_at.as_deref(),
        Some("2026-03-01T13:00:00+01:00")
    );
    assert!(o.navigation_visible);
    assert!(!f.overview("2026-03-31T12:59:59+01:00").navigation_visible);
    let unknown = Fixture::new();
    for i in 0..5 {
        unknown.insert(&i.to_string(), "1900-01-01", "invalid");
    }
    assert!(!unknown.overview(NOW).navigation_visible);
    assert_eq!(unknown.overview(NOW).earliest_created_at, None);
    let future = Fixture::new();
    for i in 0..5 {
        future.insert(&i.to_string(), "1900-01-01", "2030-01-01T00:00:00Z");
    }
    assert!(!future.overview(NOW).navigation_visible);
}
#[test]
fn leap_days_strict_dates_and_undated_group_never_drop_rows() {
    let f = Fixture::new();
    f.insert("leap", "2024-02-29", OLD);
    f.insert("ordinary", "2023-02-28", OLD);
    let invalid = [
        "2023-02-29",
        "2024-02-30",
        "",
        "2026-2-01",
        "0000-01-01",
        "10000-01-01",
        "2026-01-01T12:00:00Z",
        "2026-01-０1",
    ];
    for (i, date) in invalid.iter().enumerate() {
        f.insert(&format!("invalid{i}"), date, OLD);
    }
    let o = f.overview(NOW);
    assert_eq!(o.undated_count, 8);
    assert_eq!(o.total_entries, 10);
    assert_eq!(
        o.years.iter().map(|y| y.count).sum::<u64>() + o.undated_count,
        o.total_entries
    );
    assert_eq!(f.month(Some(2024), Some(2), 0).entries[0].id, "leap");
    assert_eq!(f.month(Some(2023), Some(2), 0).entries[0].id, "ordinary");
    let undated = f.month(None, None, 0);
    assert_eq!(undated.entries.len(), 8);
    for row in undated.entries {
        assert!(row.id.starts_with("invalid"));
        assert!(invalid.contains(&row.entry_date.as_str()));
    }
}
#[test]
fn calendar_bounds_and_invalid_periods_do_not_leak_other_months() {
    let f = Fixture::new();
    for (id, date) in [
        ("first", "0001-01-01"),
        ("last", "9999-12-31"),
        ("next", "2027-01-01"),
        ("dec", "2026-12-31"),
        ("bad", "9999-13-01"),
    ] {
        f.insert(id, date, OLD);
    }
    for (year, month, id) in [
        (1, 1, "first"),
        (9999, 12, "last"),
        (2026, 12, "dec"),
        (2027, 1, "next"),
    ] {
        assert_eq!(f.month(Some(year), Some(month), 0).entries[0].id, id);
    }
    for (year, month) in [
        (Some(0), Some(1)),
        (Some(10000), Some(1)),
        (Some(2026), Some(13)),
        (Some(2026), Some(0)),
        (None, Some(1)),
        (Some(2026), None),
    ] {
        assert!(
            matches!(entries_in_session(&f.state,||panic!("invalid selector must not open"),year,month,0),Err(e) if e=="journal_history_invalid_period")
        );
    }
}
#[test]
fn pages_are_bounded_and_offsets_count_only_matching_rows() {
    let f = Fixture::new();
    for i in 0..53 {
        f.insert(&format!("valid{i:03}"), "2024-02-01", OLD);
        f.insert(&format!("bad{i:03}"), "2024-02-30", OLD);
    }
    for (year, month, prefix) in [(Some(2024), Some(2), "valid"), (None, None, "bad")] {
        let first = f.month(year, month, 0);
        assert_eq!(first.entries.len(), 50);
        assert!(first.has_more);
        let second = f.month(year, month, 50);
        assert_eq!(second.entries.len(), 3);
        assert!(!second.has_more);
        let ids = first
            .entries
            .into_iter()
            .chain(second.entries)
            .map(|e| e.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 53);
        assert!(ids.iter().all(|id| id.starts_with(prefix)));
        assert!(f.month(year, month, 53).entries.is_empty());
    }
}
#[test]
fn queries_cannot_read_documents_and_leave_all_source_bytes_and_schema_intact() {
    let f = Fixture::new();
    f.insert("legacy", "2024-02-29", OLD);
    f.insert("unknown", "bad date", OLD);
    f.open().unwrap().execute("UPDATE journal_entries SET content_json=?1, document_version=99",[r#"{ "type":"future","text":"first","text":"second","attrs":{"src":"/external/original.jpg"} }"#]).unwrap();
    let before = std::fs::read(&f.path).unwrap();
    let metadata_only = || {
        let db = f.open()?;
        db.authorizer(Some(|c: AuthContext<'_>| match c.action {
            AuthAction::Read { column_name, .. }
                if ["content_json", "document_version", "updated_at"].contains(&column_name) =>
            {
                Authorization::Deny
            }
            _ => Authorization::Allow,
        }))
        .unwrap();
        Ok(db)
    };
    assert_eq!(
        overview_in_session(&f.state, metadata_only, stamp(NOW))
            .unwrap()
            .data
            .total_entries,
        2
    );
    assert_eq!(
        entries_in_session(&f.state, metadata_only, Some(2024), Some(2), 0)
            .unwrap()
            .data
            .entries[0]
            .id,
        "legacy"
    );
    assert_eq!(
        entries_in_session(&f.state, metadata_only, None, None, 0)
            .unwrap()
            .data
            .entries[0]
            .id,
        "unknown"
    );
    assert_eq!(std::fs::read(&f.path).unwrap(), before);
    assert!(database::validate_schema(&f.open().unwrap()).unwrap());
}
#[test]
fn overview_is_a_single_snapshot_even_when_a_writer_commits_mid_scan() {
    let f = Arc::new(Fixture::new());
    f.open()
        .unwrap()
        .execute_batch("PRAGMA journal_mode=WAL")
        .unwrap();
    for i in 0..100 {
        f.insert(&i.to_string(), "2026-03-01", RECENT);
    }
    let (started, waiting) = mpsc::channel();
    let (resume, continued) = mpsc::channel();
    let reader = {
        let f = f.clone();
        thread::spawn(move || {
            overview_in_session(
                &f.state,
                || {
                    let db = f.open()?;
                    let mut first = Some((started, continued));
                    db.progress_handler(
                        100,
                        Some(move || {
                            if let Some((started, continued)) = first.take() {
                                started.send(()).unwrap();
                                continued.recv().unwrap();
                            }
                            false
                        }),
                    )
                    .unwrap();
                    Ok(db)
                },
                stamp(NOW),
            )
            .unwrap()
            .data
        })
    };
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    f.insert("imported-old", "1990-01-01", "1990-01-01T00:00:00Z");
    resume.send(()).unwrap();
    let old = reader.join().unwrap();
    assert_eq!(old.total_entries, 100);
    assert!(!old.navigation_visible);
    let fresh = f.overview(NOW);
    assert_eq!(fresh.total_entries, 101);
    assert!(fresh.navigation_visible);
}
#[test]
fn startup_recovery_and_generation_use_existing_admission_before_open() {
    let f = Arc::new(Fixture::new());
    f.insert("entry", "2026-01-01", OLD);
    let blocked = DatabaseState::default();
    assert!(overview_in_session(&blocked, || panic!("startup"), stamp(NOW)).is_err());
    assert!(entries_in_session(&blocked, || panic!("startup"), None, None, 0).is_err());
    let generation = f.state.operation(None).unwrap().snapshot(()).generation;
    {
        let mut m = f.state.maintenance().unwrap();
        m.advance_generation().unwrap();
    }
    assert_eq!(
        overview_in_session(&f.state, || f.open(), stamp(NOW))
            .unwrap()
            .generation,
        generation + 1
    );
    assert_eq!(
        entries_in_session(&f.state, || f.open(), Some(2026), Some(1), 0)
            .unwrap()
            .generation,
        generation + 1
    );
    let maintenance = f.state.maintenance().unwrap();
    let (queued, wait) = mpsc::channel();
    f.state.on_blocked(move || queued.send(()).unwrap());
    let reader = {
        let f = f.clone();
        thread::spawn(move || overview_in_session(&f.state, || panic!("Recovery"), stamp(NOW)))
    };
    wait.recv_timeout(Duration::from_secs(5)).unwrap();
    maintenance.require_recovery();
    drop(maintenance);
    assert!(matches!(reader.join().unwrap(),Err(e) if e==crate::database_state::RECOVERY_REQUIRED));
    assert!(
        matches!(entries_in_session(&f.state,||panic!("Recovery"),Some(2026),Some(1),0),Err(e) if e==crate::database_state::RECOVERY_REQUIRED)
    );
}
#[test]
fn wire_contract_preserves_raw_metadata_and_never_exposes_documents() {
    let f = Fixture::new();
    f.insert("Svenskt-ID", "not a date", OLD);
    let o = serde_json::to_value(overview_in_session(&f.state, || f.open(), stamp(NOW)).unwrap())
        .unwrap();
    assert_eq!(o["generation"], 1);
    assert_eq!(o["data"]["totalEntries"], 1);
    assert_eq!(o["data"]["undatedCount"], 1);
    assert_eq!(o["data"]["navigationVisible"], false);
    let page =
        serde_json::to_value(entries_in_session(&f.state, || f.open(), None, None, 0).unwrap())
            .unwrap();
    assert_eq!(
        page["data"]["entries"][0],
        serde_json::json!({"id":"Svenskt-ID","entryDate":"not a date","title":"Title Svenskt-ID"})
    );
    assert_eq!(page["data"]["hasMore"], false);
}
