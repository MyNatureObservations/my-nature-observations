//! Retained P/Q objects, never authority obtained from deserialized metadata.
use super::super::completion::{self, LoadedReceipt};
use super::*;
pub(super) struct Evidence {
    recovery: Option<File>,
    marker: Option<File>,
    // Retain original handles even after our own successful name operations.
    marker_removed: bool,
    recovery_removed: bool,
    pub(super) receipt: Option<LoadedReceipt>,
    pub(super) bytes: Vec<u8>,
}
fn exists(path: &Path) -> std::result::Result<bool, Reason> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Reason::Marker),
    }
}
fn layout(
    root: &Path,
    recovery: bool,
    marker: bool,
    receipt: bool,
    temporary: Option<&std::ffi::OsStr>,
) -> std::result::Result<(), Reason> {
    if exists(&root.join(DIRECTORY))? != recovery
        || exists(&root.join(DIRECTORY).join(NAME))? != marker
        || exists(&root.join(".mno-restore-completion-v1.json"))? != receipt
    {
        return Err(Reason::Marker);
    }
    if recovery {
        let mut count = 0;
        for entry in fs::read_dir(root.join(DIRECTORY)).map_err(|_| Reason::Marker)? {
            if entry.map_err(|_| Reason::Marker)?.file_name() != NAME {
                return Err(Reason::Marker);
            }
            count += 1;
        }
        if count != usize::from(marker) {
            return Err(Reason::Marker);
        }
    }
    if crate::startup::artifacts::unexpected_root(root, temporary).map_err(|_| Reason::Marker)? {
        return Err(Reason::Marker);
    }
    Ok(())
}
impl Evidence {
    pub(super) fn load(root: &Path) -> std::result::Result<Option<Self>, Reason> {
        let recovery = if exists(&root.join(DIRECTORY))? {
            Some(open_plain(&root.join(DIRECTORY), true).map_err(|_| Reason::Marker)?)
        } else {
            None
        };
        let marker = if recovery.is_some() && exists(&root.join(DIRECTORY).join(NAME))? {
            Some(open_plain(&root.join(DIRECTORY).join(NAME), false).map_err(|_| Reason::Marker)?)
        } else {
            None
        };
        let receipt = completion::read(root).map_err(|_| Reason::Marker)?;
        layout(
            root,
            recovery.is_some(),
            marker.is_some(),
            receipt.is_some(),
            None,
        )?;
        let bytes = match (&marker, &receipt) {
            (Some(p), q) => {
                let bytes = completion::read_pending_bytes(p).map_err(|_| Reason::Marker)?;
                parse(&bytes).map_err(|_| Reason::Marker)?;
                if q.as_ref().is_some_and(|q| q.pending_bytes() != bytes) {
                    return Err(Reason::Marker);
                }
                bytes
            }
            (None, Some(q)) => q.pending_bytes().to_vec(),
            (None, None) if recovery.is_none() => return Ok(None),
            _ => return Err(Reason::Marker),
        };
        let evidence = Self {
            recovery,
            marker,
            marker_removed: false,
            recovery_removed: false,
            receipt,
            bytes,
        };
        evidence.check(root, &evidence.bytes)?;
        Ok(Some(evidence))
    }
    pub(super) fn check_unlinked(
        &self,
        root: &Path,
        bytes: &[u8],
        proof: &result::receipt::all_closed::QUnlinked<'_, '_>,
    ) -> std::result::Result<(), Reason> {
        // A returned/replaced Q, P or recovery directory is never accepted.
        layout(root, false, false, false, None)?;
        let receipt = self.receipt.as_ref().ok_or(Reason::Marker)?;
        receipt
            .check_after_unlink(root, proof)
            .map_err(|_| Reason::Marker)?;
        if receipt.pending_bytes() != bytes {
            return Err(Reason::Marker);
        }
        layout(root, false, false, false, None)
    }
    pub(super) fn check_running(
        &self,
        root: &Path,
        bytes: &[u8],
        handoff: &result::receipt::all_closed::RunningHandoff<'_>,
    ) -> std::result::Result<(), Reason> {
        layout(root, false, false, false, None)?;
        let receipt = self.receipt.as_ref().ok_or(Reason::Marker)?;
        receipt
            .check_after_handoff(root, handoff)
            .map_err(|_| Reason::Marker)?;
        if receipt.pending_bytes() != bytes {
            return Err(Reason::Marker);
        }
        layout(root, false, false, false, None)
    }
    pub(super) fn check_startup(
        &self,
        root: &Path,
        bytes: &[u8],
        handoff: &result::receipt::all_closed::StartupHandoff<'_>,
    ) -> std::result::Result<(), Reason> {
        layout(root, false, false, false, None)?;
        let receipt = self.receipt.as_ref().ok_or(Reason::Marker)?;
        receipt
            .check_after_startup_handoff(root, handoff)
            .map_err(|_| Reason::Marker)?;
        if receipt.pending_bytes() != bytes {
            return Err(Reason::Marker);
        }
        layout(root, false, false, false, None)
    }
    pub(super) fn check(&self, root: &Path, bytes: &[u8]) -> std::result::Result<(), Reason> {
        self.check_temporary(root, bytes, None)
    }
    pub(super) fn check_temporary(
        &self,
        root: &Path,
        bytes: &[u8],
        temporary: Option<&std::ffi::OsStr>,
    ) -> std::result::Result<(), Reason> {
        layout(
            root,
            self.recovery.is_some() && !self.recovery_removed,
            self.marker.is_some() && !self.marker_removed,
            self.receipt.is_some(),
            temporary,
        )?;
        if !self.recovery_removed {
            if let Some(directory) = &self.recovery {
                current(directory, &root.join(DIRECTORY), true)?;
            }
        }
        if !self.marker_removed {
            if let Some(marker) = &self.marker {
                current(marker, &root.join(DIRECTORY).join(NAME), false)?;
                if completion::read_pending_bytes(marker).map_err(|_| Reason::Marker)? != bytes {
                    return Err(Reason::Marker);
                }
            }
        }
        if let Some(receipt) = &self.receipt {
            receipt.check(root).map_err(|_| Reason::Marker)?;
            if receipt.pending_bytes() != bytes {
                return Err(Reason::Marker);
            }
        }
        Ok(())
    }
}

impl Evidence {
    pub(super) fn recovery_handle(&self) -> Option<&File> {
        self.recovery.as_ref()
    }
    pub(super) fn marker_handle(&self) -> Option<&File> {
        self.marker.as_ref()
    }
    // Only the consuming cleanup path calls these after successful unlinkat.
    // Presence checks then require absence; replacement entries are NOT accepted.
    pub(super) fn did_remove_marker(&mut self) {
        self.marker_removed = true;
    }
    pub(super) fn did_remove_recovery(&mut self) {
        self.recovery_removed = true;
    }
}
