#[path = "../src/database/schema_v0.rs"]
mod database;

use rusqlite::{Connection, params};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use uuid::Uuid;

const ENTRY_ID: &str = "crash-test-entry";
const ORIGINAL: &str = r#"{"text":"Original"}"#;
const UNCOMMITTED: &str = r#"{"text":"Uncommitted"}"#;

#[test]
fn can_initialize_real_mno_schema() {
    let connection = Connection::open_in_memory().unwrap();
    database::initialize_schema(&connection).unwrap();

    let table_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'journal_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(table_exists, 1);
}

#[test]
fn crash_child_process() {
    if std::env::var("MNO_CRASH_TEST_CHILD").as_deref() != Ok("1") {
        return;
    }

    let path = std::env::var("MNO_CRASH_TEST_DB").unwrap();
    let mut connection = Connection::open(path).unwrap();

    let transaction = connection.transaction().unwrap();

    transaction
        .execute(
            "UPDATE journal_entries SET content_json = ?1 WHERE id = ?2",
            params![UNCOMMITTED, ENTRY_ID],
        )
        .unwrap();

    println!("TRANSACTION_READY");
    std::io::stdout().flush().unwrap();

    // Wait indefinitely. The parent test terminates this process.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[test]
fn process_crash_preserves_committed_entry() {
    let path = std::env::temp_dir().join(format!("mno-crash-{}.sqlite", Uuid::new_v4()));

    {
        let connection = Connection::open(&path).unwrap();
        database::initialize_schema(&connection).unwrap();

        connection
            .execute(
                "INSERT INTO journal_entries
                 (id, entry_date, title, content_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
                params![ENTRY_ID, "2026-10-17", "Crash test", ORIGINAL],
            )
            .unwrap();
    }

    let executable = std::env::current_exe().unwrap();

    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("crash_child_process")
        .arg("--nocapture")
        .env("MNO_CRASH_TEST_CHILD", "1")
        .env("MNO_CRASH_TEST_DB", &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();

    // Read the child's output on a separate thread.
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        loop {
            line.clear();

            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = sender.send(false);
                    break;
                }
                Ok(_) if line.contains("TRANSACTION_READY") => {
                    let _ = sender.send(true);
                    break;
                }
                Ok(_) => continue,
                Err(_) => {
                    let _ = sender.send(false);
                    break;
                }
            }
        }
    });

    // Never wait indefinitely for the child process.
    let ready = receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or(false);

    // Terminate only the child process started by this test.
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    child.wait().unwrap();

    assert!(
        ready,
        "Child process did not signal transaction readiness within 10 seconds"
    );

    let connection = Connection::open(&path).unwrap();

    let content: String = connection
        .query_row(
            "SELECT content_json FROM journal_entries WHERE id = ?1",
            [ENTRY_ID],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(content, ORIGINAL);

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();

    assert_eq!(integrity, "ok");

    drop(connection);
    fs::remove_file(&path).unwrap();
}
