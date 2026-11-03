use rusqlite::{Connection, params};
use std::fs;
use uuid::Uuid;

#[test]
fn unfinished_transaction_preserves_saved_data() {
    // Create a unique temporary database for this test.
    let database_path = std::env::temp_dir().join(format!("mno-test-{}.sqlite", Uuid::new_v4()));

    {
        let mut connection =
            Connection::open(&database_path).expect("Could not create test database");

        connection
            .execute_batch(
                "CREATE TABLE journal_entries (
                    id TEXT PRIMARY KEY,
                    content TEXT NOT NULL
                );",
            )
            .expect("Could not create test table");

        // Save the original content.
        connection
            .execute(
                "INSERT INTO journal_entries (id, content) VALUES (?1, ?2)",
                params!["test-entry", "Original text"],
            )
            .expect("Could not save original content");

        // Begin an update, but do not commit it.
        {
            let transaction = connection
                .transaction()
                .expect("Could not begin transaction");

            transaction
                .execute(
                    "UPDATE journal_entries SET content = ?1 WHERE id = ?2",
                    params!["Unsaved change", "test-entry"],
                )
                .expect("Could not update test entry");

            // Dropping an uncommitted transaction rolls it back.
        }
    }

    // Reopen the database and verify the original content survived.
    let connection = Connection::open(&database_path).expect("Could not reopen test database");

    let content: String = connection
        .query_row(
            "SELECT content FROM journal_entries WHERE id = ?1",
            ["test-entry"],
            |row| row.get(0),
        )
        .expect("Could not read test entry");

    assert_eq!(content, "Original text");

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("Could not check database integrity");

    assert_eq!(integrity, "ok");

    drop(connection);

    fs::remove_file(&database_path).expect("Could not remove test database");
}
