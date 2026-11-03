use super::*;
use rusqlite::Connection;

fn schema3() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db.transaction().unwrap();
    crate::database::initialize_schema(&tx).unwrap();
    crate::database::weather_schema::upgrade_in_transaction(&tx).unwrap();
    crate::database::place_schema::upgrade_in_transaction(&tx).unwrap();
    create_in_transaction(&tx).unwrap();
    tx.commit().unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    db
}

#[test]
fn current_database_has_exact_schema3_and_empty_setting() {
    let db = schema3();
    assert!(validate_schema3(&db).unwrap());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM weather_default_place", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn setting_constraints_accept_active_and_tombstone_and_reject_invalid_values() {
    let db = schema3();
    let place = "00000000-0000-4000-8000-000000000001";
    db.execute(
        "INSERT INTO places(id,name,latitude,longitude,revision) VALUES(?1,'Home',59.0,18.0,1)",
        [place],
    )
    .unwrap();
    db.execute("INSERT INTO weather_default_place VALUES(1,?1,1)", [place])
        .unwrap();
    db.execute(
        "UPDATE weather_default_place SET place_id=NULL,revision=2 WHERE singleton=1",
        [],
    )
    .unwrap();
    assert!(validate_schema3(&db).unwrap());
    assert!(
        db.execute("INSERT INTO weather_default_place VALUES(2,NULL,1)", [])
            .is_err()
    );
    assert!(
        db.execute("UPDATE weather_default_place SET revision=0", [])
            .is_err()
    );
    assert!(
        db.execute(
            "UPDATE weather_default_place SET place_id='missing',revision=3",
            []
        )
        .is_err()
    );
}

#[test]
fn exact_validator_rejects_changed_schema3() {
    let db = schema3();
    db.execute_batch("CREATE TABLE unexpected(value TEXT)")
        .unwrap();
    assert!(!validate_schema3(&db).unwrap());
}
