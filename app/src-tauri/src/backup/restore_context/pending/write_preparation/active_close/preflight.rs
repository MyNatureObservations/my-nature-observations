//! Pre-lock inspection only. No writer is opened and no proof is returned.
use super::*;

pub(super) fn check(epoch: &WriteEpochInputs<'_, '_>) -> std::result::Result<(), PreflightError> {
    if !cfg!(target_os = "macos") {
        return Err(PreflightError::Binding);
    }
    let parts = epoch
        .inputs
        .owner
        .after
        .as_ref()
        .ok_or(PreflightError::Binding)?;
    let sources = parts.sources.live().map_err(PreflightError::Existing)?;
    check_after(parts).map_err(PreflightError::Existing)?;
    #[cfg(test)]
    panic_probe::checkpoint();
    let pending = parts.pending.as_ref().ok_or(PreflightError::Binding)?;
    let ActiveState::Closed(active) = &parts.active else {
        return Err(PreflightError::Binding);
    };
    let main_metadata = fs::symlink_metadata(&active.path).map_err(PreflightError::Io)?;
    if !main_metadata.is_file() || main_metadata.file_type().is_symlink() {
        return Err(PreflightError::Binding);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if main_metadata.nlink() != 1
            || active.file.metadata().map_err(PreflightError::Io)?.nlink() != 1
        {
            return Err(PreflightError::Binding);
        }
    }
    let data =
        parse(&pending.bytes).map_err(|e| PreflightError::Existing(ActiveCloseError::Marker(e)))?;
    let protection = sources.protection.as_ref().ok_or(PreflightError::Binding)?;
    let archive_path = pending.root.join(&data.protection_path);
    if protection.path() != archive_path {
        return Err(PreflightError::Binding);
    }
    // All opens here precede a future writer episode. The retained ZIP is NOT
    // claimed to have an original physical identity: P binds its exact hash.
    for directory in [
        pending.root.join("backups"),
        pending.root.join("backups/pre-restore"),
    ] {
        if !private::is_plain_directory(
            &fs::symlink_metadata(directory).map_err(PreflightError::Io)?,
        ) {
            return Err(PreflightError::Binding);
        }
    }
    let archive = open_plain(&archive_path, false)
        .map_err(|e| PreflightError::Existing(ActiveCloseError::Marker(e)))?;
    #[cfg(unix)]
    {
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::FileExt;
        let mut hash = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0; 65536];
        loop {
            let count = archive
                .read_at(&mut buffer, offset)
                .map_err(PreflightError::Io)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
            offset += count as u64;
        }
        if format!("{:x}", hash.finalize()) != data.protection_sha256
            || !private::same_file(&archive, &archive_path).map_err(PreflightError::Io)?
        {
            return Err(PreflightError::Binding);
        }
    }
    let artifacts = &crate::startup::artifacts::RECEIPT;
    match fs::symlink_metadata(pending.root.join(artifacts)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(PreflightError::Io(e)),
        Ok(_) => return Err(PreflightError::Binding),
    }
    if crate::startup::artifacts::unexpected_root(&pending.root, None)
        .map_err(PreflightError::Io)?
        || crate::startup::artifacts::sidecars(&pending.root.join("mno.sqlite"), &mut |p| {
            fs::symlink_metadata(p)
        })
        .map_err(PreflightError::Io)?
    {
        return Err(PreflightError::Binding);
    }
    for entry in fs::read_dir(pending.root.join(DIRECTORY)).map_err(PreflightError::Io)? {
        if entry.map_err(PreflightError::Io)?.file_name() != NAME {
            return Err(PreflightError::Binding);
        }
    }
    // Recheck bindings and P after potentially lengthy archive verification.
    check_after(parts).map_err(PreflightError::Existing)
}

#[cfg(test)]
pub(super) mod panic_probe {
    use std::cell::Cell;
    thread_local! { static ARMED: Cell<bool> = const { Cell::new(false) }; }
    pub(super) fn checkpoint() {
        if ARMED.with(|a| a.replace(false)) {
            panic!("preflight checkpoint");
        }
    }
    pub(in super::super) fn with<T>(action: impl FnOnce() -> T) -> T {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                ARMED.with(|a| a.set(false));
            }
        }
        ARMED.with(|a| a.set(true));
        let _reset = Reset;
        action()
    }
}
