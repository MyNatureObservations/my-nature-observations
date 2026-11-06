//! Protected Windows selection publication under the startup lease.
use super::*;
use crate::startup::{
    DirectoryLease,
    generation_selection::{LIMIT, Record},
};
use windows_sys::Win32::{
    Foundation::GENERIC_WRITE,
    Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_INFO, FileDispositionInfo, MOVEFILE_REPLACE_EXISTING,
        SetFileInformationByHandle,
    },
};

use super::managed_layout::{HEAD, INTENT, STAGED};

pub(super) fn read_record(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    read_record_handle(&file)
}

fn read_record_handle(file: &File) -> io::Result<Vec<u8>> {
    let metadata = file.metadata()?;
    if !metadata.is_file() || !plain(&metadata) {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    Record::decode(&bytes)?;
    Ok(bytes)
}

pub(super) fn acknowledge(marker: File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the buffer has the layout and size required by FileDispositionInfo.
    // The owned handle remains live throughout the call and closes on return.
    if unsafe {
        SetFileInformationByHandle(
            marker.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of_val(&disposition) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Step {
    Intent,
    Prepared,
    Replaced,
    Flushed,
}

pub(super) fn publish_record(
    lease: &DirectoryLease,
    record: &Record,
    replace: bool,
    mut hook: impl FnMut(Step) -> io::Result<()>,
) -> io::Result<()> {
    let bytes = record.encode()?;
    let root = &lease.root;
    let intent = root.join(INTENT);
    let staged = root.join(STAGED);
    let head = root.join(HEAD);
    let create = |path: &Path| OpenOptions::new().write(true).create_new(true).open(path);
    // Keep evidence across replacement and every fallible persistence barrier.
    let marker = OpenOptions::new()
        .write(true)
        .access_mode(GENERIC_WRITE | DELETE)
        .share_mode(0)
        .create_new(true)
        .open(&intent)?;
    flush(&marker)?;
    lease.boundary.sync()?;
    hook(Step::Intent)?;

    let mut output = create(&staged)?;
    output.write_all(&bytes)?;
    flush(&output)?;
    drop(output);
    if read_record(&staged)? != bytes {
        return Err(invalid());
    }
    lease.boundary.sync()?;
    hook(Step::Prepared)?;
    lease.ensure_current().map_err(io::Error::other)?;
    lease.boundary.check()?;
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(wide(&staged).as_ptr(), wide(&head).as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    hook(Step::Replaced)?;
    let published = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&head)?;
    if read_record_handle(&published)? != bytes {
        return Err(invalid());
    }
    flush(&published)?;
    lease.boundary.sync()?;
    hook(Step::Flushed)?;
    // A stale marker after a crash blocks reopening. The record and its
    // directory entry have passed all barriers before marker removal.
    acknowledge(marker)?;
    drop(published);
    Ok(())
}
