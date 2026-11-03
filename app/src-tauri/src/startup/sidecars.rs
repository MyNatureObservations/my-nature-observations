//! Presence-only bootstrap check. Never open SQLite to determine journal state.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
use super::artifacts::SIDECAR_SUFFIXES as SUFFIXES;
use super::*;
pub(super) fn check(
    database: &Path,
    metadata: &mut impl FnMut(&Path) -> io::Result<fs::Metadata>,
) -> Result<(), String> {
    match super::artifacts::sidecars(database, metadata) {
        Ok(false) => Ok(()),
        Ok(true) => Err("database_sqlite_sidecar_present".into()),
        Err(e) => Err(format!("database_sqlite_sidecar_check_failed: {e}")),
    }
}
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests;
