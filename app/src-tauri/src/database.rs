use rusqlite::{Connection, OpenFlags};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::Manager;

// Resolution only: no directories, files or SQLite handles are created here.
pub(crate) fn resolve_data_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    crate::environment::data_directory(app)
}
pub(crate) fn expected_database_path(directory: &Path) -> PathBuf {
    directory.join("mno.sqlite")
}

// Installed only after startup has acquired its lifetime lock and initialized safely.
pub(crate) struct DataLocation(PathBuf);
impl DataLocation {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}
pub fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.try_state::<DataLocation>()
        .map(|location| location.path().to_owned())
        .ok_or_else(|| "database_startup_blocked".into())
}
pub fn open_database(app: &tauri::AppHandle) -> Result<Connection, String> {
    open_existing(&database_path(app)?)
}
pub(crate) fn open_existing(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(connection)
}
// Caller holds startup DirectoryLease for established application journals.
// Opening an established journal NEVER includes CREATE. A zero-byte or partial
// first-creation file is not silently promoted to a new journal on restart.
pub(crate) fn initialize_database(path: &Path) -> Result<(), String> {
    let mut connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version < SCHEMA_VERSION {
        return Err("database_schema_upgrade_required".into());
    }
    if version != SCHEMA_VERSION {
        return Err("database_schema_incompatible".into());
    }
    let valid = crate::photographs::validate_schema(&connection)
        .map_err(|e| format!("database_schema_validation_failed: {e}"))?;
    if !valid {
        return Err("database_schema_incompatible".into());
    }
    crate::storage_content::verify_v5(&mut connection, &std::sync::atomic::AtomicBool::new(false))
        .map_err(|e| format!("database_content_validation_failed: {e:?}"))?;
    connection.close().map_err(|(_, e)| e.to_string())?;
    Ok(())
}
/// Publish the current application schema in one transaction. This is the
/// only database creator used by ordinary startup.
pub(crate) fn create_current_database(path: &Path) -> Result<(), String> {
    reserve_database_file(path)?;
    let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        initialize_schema(&tx)?;
        weather_schema::upgrade_in_transaction(&tx)?;
        place_schema::upgrade_in_transaction(&tx)?;
        weather_default_schema::create_in_transaction(&tx)?;
        crate::photographs::create_schema(&tx)?;
        tx.commit().map_err(|e| e.to_string())
    })();
    db.close().map_err(|(_, e)| e.to_string())?;
    result
}

// Schema-1 fixture constructor retained for the frozen pre-v0.1 protocol
// tests. Production startup uses create_current_database above.
#[cfg(test)]
pub(crate) fn create_new_database(path: &Path) -> Result<(), String> {
    reserve_database_file(path)?;
    let mut db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    // Publish the historical schema-1 fixture in one transaction.
    let result = (|| {
        let tx = db.transaction().map_err(|e| e.to_string())?;
        initialize_schema(&tx)?;
        weather_schema::upgrade_in_transaction(&tx)?;
        tx.commit().map_err(|e| e.to_string())
    })();
    db.close().map_err(|(_, e)| e.to_string())?;
    result
}

pub(crate) fn create_legacy_database(path: &Path) -> Result<(), String> {
    reserve_database_file(path)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    initialize_schema(&connection)
}

// Frozen schema-1 startup used only by the pre-v0.1 weather protocol tests.
// Production initialize_database above never migrates an established database.
#[cfg(test)]
pub(crate) fn initialize_schema1_database_for_test(path: &Path) -> Result<(), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let valid = validate_supported_schema(&connection)
        .map_err(|e| format!("database_schema_validation_failed: {e}"))?;
    if !valid {
        return Err("database_schema_incompatible".into());
    }
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    connection.close().map_err(|(_, e)| e.to_string())?;
    if version == 0 {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::backup::restore_context::migration::initialize(path)
            .map_err(|e| format!("database_migration_failed: {e:?}"))?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        return Err("database_migration_platform_unsupported".into());
    }
    Ok(())
}
fn reserve_database_file(path: &Path) -> Result<(), String> {
    // Reserve the name exclusively. SQLite then opens this exact pre-existing
    // empty file without CREATE; a competing file is never truncated.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let reservation = options.open(path).map_err(|e| e.to_string())?;
    drop(reservation);
    Ok(())
}

pub(crate) fn initialize_schema(connection: &Connection) -> Result<(), String> {
    initialize_schema_v0(connection)
}

// The stand-alone crash test imports the same frozen legacy initializer without
// pulling in startup's candidate-publication dependencies.
#[path = "database/schema_v0.rs"]
mod schema_v0;
use schema_v0::initialize_schema as initialize_schema_v0;

// Shared with restore verification. Reads only the supplied database; the
// reference schema is created in a separate in-memory database.
// Current application schema; protocol readers keep their own frozen versions.
#[allow(dead_code)]
pub(crate) const SCHEMA_VERSION: u32 = 4;
// The exact legacy schema supported by this validator, independently of creation policy.
const SCHEMA_V0: u32 = 0;
pub(crate) fn validate_schema(connection: &Connection) -> rusqlite::Result<bool> {
    let version: i64 = connection.pragma_query_value(Some("main"), "user_version", |r| r.get(0))?;
    if version != i64::from(SCHEMA_V0) {
        return Ok(false);
    }
    let reference = Connection::open_in_memory()?;
    initialize_schema_v0(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(schema_objects(connection)? == schema_objects(&reference)?)
}

type SchemaObject = (String, String, String, Option<String>);
fn schema_objects(connection: &Connection) -> rusqlite::Result<Vec<SchemaObject>> {
    // The supported schema has exactly three objects (including SQLite's
    // primary-key autoindex). A fourth object is enough to reject extras.
    let mut statement =
        connection.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema LIMIT 4")?;
    let mut objects = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, Option<String>>(3)?
                    .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    objects.sort();
    Ok(objects)
}

#[path = "database/weather_schema.rs"]
pub(crate) mod weather_schema;
// Schema-2 place support is shared by ordinary creation and the isolated
// candidate migration used by archive tests.
#[path = "database/place_schema.rs"]
pub(crate) mod place_schema;
#[path = "database/weather_default_schema.rs"]
pub(crate) mod weather_default_schema;
pub(crate) fn validate_supported_schema(connection: &Connection) -> rusqlite::Result<bool> {
    let version: i64 = connection.pragma_query_value(Some("main"), "user_version", |r| r.get(0))?;
    match version {
        0 => validate_schema(connection),
        1 => {
            let reference = Connection::open_in_memory()?;
            initialize_schema_v0(&reference).map_err(|_| rusqlite::Error::InvalidQuery)?;
            reference.execute_batch(weather_schema::DDL)?;
            reference.pragma_update(Some("main"), "user_version", 1)?;
            Ok(supported_objects(connection)? == supported_objects(&reference)?)
        }
        _ => Ok(false),
    }
}
fn supported_objects(connection: &Connection) -> rusqlite::Result<Vec<SchemaObject>> {
    let mut s =
        connection.prepare("SELECT type,name,tbl_name,sql FROM main.sqlite_schema LIMIT 6")?;
    let mut objects = s
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?
                    .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    objects.sort();
    Ok(objects)
}
