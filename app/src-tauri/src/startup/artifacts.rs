//! Shared reserved recovery namespace. Presence checks only, never authority.
use std::{ffi::OsStr, fs, io, path::Path};
pub(crate) const RECOVERY: &str = "recovery";
pub(crate) const PENDING: &str = "pending-restore.json";
pub(crate) const RECEIPT: &str = ".mno-restore-completion-v1.json";
pub(crate) const SIDECAR_SUFFIXES: [&str; 3] = ["-journal", "-wal", "-shm"];

// Compare encoded bytes with ASCII rules only: no Unicode normalization/folding.
fn ascii_eq(name: &OsStr, expected: &[u8]) -> bool {
    name.as_encoded_bytes().eq_ignore_ascii_case(expected)
}
fn ascii_prefix(name: &OsStr, prefix: &[u8]) -> bool {
    name.as_encoded_bytes()
        .get(..prefix.len())
        .is_some_and(|bytes| bytes.eq_ignore_ascii_case(prefix))
}
pub(crate) fn completion_temporary(name: &OsStr) -> bool {
    ascii_prefix(name, b".mno-completion-")
}
// Root pending is never a valid P location. Exact canonical recovery/Q names
// have separate retained-object checks. Noncanonical case aliases fail closed,
// including on case-sensitive volumes where canonical metadata lookup misses them.
pub(crate) fn unexpected_root_name(name: &OsStr, current_temporary: Option<&OsStr>) -> bool {
    if completion_temporary(name) {
        // Deliberately exact, NOT ASCII-insensitive. Publication also validates
        // the actual named temporary inode against its retained descriptor.
        return current_temporary != Some(name);
    }
    ascii_eq(name, PENDING.as_bytes())
        || ascii_prefix(name, b".mno-init-")
        || ascii_prefix(name, b".pending-")
        || (ascii_prefix(name, b".mno-restore-completion") && name != RECEIPT)
        || (ascii_eq(name, RECOVERY.as_bytes()) && name != RECOVERY)
}
fn sidecar_name(name: &OsStr, database_name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    let base = database_name.as_encoded_bytes();
    if !ascii_prefix(name, base) {
        return false;
    }
    SIDECAR_SUFFIXES
        .iter()
        .any(|suffix| bytes[base.len()..].eq_ignore_ascii_case(suffix.as_bytes()))
}
// No metadata of child targets is followed. Any matching name blocks regardless
// of file type; enumeration errors are never safe absence. Unrelated names pass.
pub(crate) fn unexpected_root(root: &Path, temporary: Option<&OsStr>) -> io::Result<bool> {
    let mut found = false;
    for entry in fs::read_dir(root)? {
        if unexpected_root_name(&entry?.file_name(), temporary) {
            found = true;
        }
    }
    Ok(found)
}
pub(crate) fn sidecars(
    database: &Path,
    metadata: &mut impl FnMut(&Path) -> io::Result<fs::Metadata>,
) -> io::Result<bool> {
    let mut found = false;
    for suffix in SIDECAR_SUFFIXES {
        let mut name = database.as_os_str().to_os_string();
        name.push(suffix);
        match metadata(Path::new(&name)) {
            Ok(_) => found = true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    // Enumeration also detects ASCII case aliases on case-sensitive filesystems.
    // Matching names block by presence alone; targets are never followed/opened.
    let root = database
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let base = database
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "database filename missing"))?;
    for entry in fs::read_dir(root)? {
        if sidecar_name(&entry?.file_name(), base) {
            found = true;
        }
    }
    Ok(found)
}
