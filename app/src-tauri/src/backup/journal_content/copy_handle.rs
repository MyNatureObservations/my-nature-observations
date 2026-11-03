//! Test-only, opaque backup handle: never exposes source Connection or raw pointer.
use super::super::isolated_copy::CopyDestination;
use super::VerifiedJournal;
use rusqlite::{Connection, ffi};
use std::ptr::NonNull;
#[derive(Debug)]
pub(in super::super) enum CopyStartError {
    Source,
    Sqlite(i32),
}
impl<'db> VerifiedJournal<'db> {
    pub(in super::super) fn start_copy<'s>(
        &'s self,
        destination: CopyDestination<'s>,
    ) -> Result<CopyHandle<'s, 'db>, CopyStartError> {
        CopyHandle::new(self, destination.into_connection())
    }
}
// Mirrors SQLite's handle contract, but unlike rusqlite 0.40.2 Backup::Drop,
// explicitly observes backup_finish. Borrows prohibit destination API calls.
pub(in super::super) struct CopyHandle<'s, 'db> {
    raw: Option<NonNull<ffi::sqlite3_backup>>,
    _source: &'s VerifiedJournal<'db>,
    _destination: &'s mut Connection,
}
impl<'s, 'db> CopyHandle<'s, 'db> {
    fn new(
        source: &'s VerifiedJournal<'db>,
        destination: &'s mut Connection,
    ) -> Result<Self, CopyStartError> {
        source.ensure().map_err(|_| CopyStartError::Source)?;
        // The candidate already verified DELETE and denies PRAGMAs. Keep its policy intact.
        if source.candidate.is_none() {
            let mode: String = source
                .database
                .pragma_query_value(Some("main"), "journal_mode", |r| r.get(0))
                .map_err(|_| CopyStartError::Source)?;
            if mode != "delete" {
                return Err(CopyStartError::Source);
            }
        }
        // SAFETY: distinct live connections, literal NUL-terminated database names;
        // destination exclusively borrowed for the entire handle lifetime.
        let raw = unsafe {
            ffi::sqlite3_backup_init(
                destination.handle(),
                c"main".as_ptr(),
                source.database.handle(),
                c"main".as_ptr(),
            )
        };
        let raw = NonNull::new(raw).ok_or_else(|| unsafe {
            CopyStartError::Sqlite(ffi::sqlite3_errcode(destination.handle()))
        })?;
        Ok(Self {
            raw: Some(raw),
            _source: source,
            _destination: destination,
        })
    }
    pub(in super::super) fn step(&self) -> i32 {
        unsafe { ffi::sqlite3_backup_step(self.raw.unwrap().as_ptr(), 1) }
    }
    pub(in super::super) fn remaining(&self) -> i32 {
        unsafe { ffi::sqlite3_backup_remaining(self.raw.unwrap().as_ptr()) }
    }
    pub(in super::super) fn finish(mut self) -> i32 {
        unsafe { ffi::sqlite3_backup_finish(self.raw.take().unwrap().as_ptr()) }
    }
}
impl Drop for CopyHandle<'_, '_> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            unsafe {
                ffi::sqlite3_backup_finish(raw.as_ptr());
            }
        }
    }
}
