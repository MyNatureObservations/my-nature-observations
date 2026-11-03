//! Completion metadata is never authority. Publication requires an owned proof.
use super::*;

// Q's envelope version is independent; embedded P remains strictly validated.
const COMPLETION_FORMAT_VERSION: u32 = 1;

const RECEIPT: &str = ".mno-restore-completion-v1.json";
// A pending marker is at most 16 KiB. A byte needs at most six JSON escape
// bytes; 1024 bytes covers the fixed envelope. This is not a journal size cap.
const RECEIPT_LIMIT: u64 = 6 * LIMIT + 1024;

#[derive(Debug)]
pub(crate) enum ReceiptError {
    Invalid,
    TooLarge,
    Conflict,
    Binding,
    AlreadyExists,
    Unsupported,
    Cancelled,
    Io(io::Error),
}
type ReceiptResult<T> = std::result::Result<T, ReceiptError>;
impl From<io::Error> for ReceiptError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::AlreadyExists {
            Self::AlreadyExists
        } else {
            Self::Io(e)
        }
    }
}
impl From<MarkerError> for ReceiptError {
    fn from(e: MarkerError) -> Self {
        match e {
            MarkerError::TooLarge => Self::TooLarge,
            MarkerError::Binding => Self::Binding,
            MarkerError::AlreadyExists => Self::AlreadyExists,
            MarkerError::Unsupported => Self::Unsupported,
            MarkerError::Invalid => Self::Invalid,
            MarkerError::Io(e) => Self::from(e),
        }
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Outcome {
    Old,
    New,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CompletionReceipt {
    format: String,
    version: u32,
    pending_json: String,
    #[serde(deserialize_with = "deserialize_outcome")]
    verified_outcome: Outcome,
}
// String::deserialize rejects maps, sequences, null, booleans and numbers;
// unlike an externally tagged enum it cannot accept {"old":null}.
fn deserialize_outcome<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Outcome, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "old" => Ok(Outcome::Old),
        "new" => Ok(Outcome::New),
        _ => Err(serde::de::Error::custom("expected old or new")),
    }
}
fn parse_receipt(bytes: &[u8]) -> ReceiptResult<CompletionReceipt> {
    if bytes.len() as u64 > RECEIPT_LIMIT {
        return Err(ReceiptError::TooLarge);
    }
    let value: CompletionReceipt =
        serde_json::from_slice(bytes).map_err(|_| ReceiptError::Invalid)?;
    if value.format != "mno-restore-completion" || value.version != COMPLETION_FORMAT_VERSION {
        return Err(ReceiptError::Invalid);
    }
    // Reuse strict versions, paths, fingerprints, nested fields and duplicate checks.
    parse(value.pending_json.as_bytes())?;
    Ok(value)
}
fn read_bounded(file: &File, limit: u64) -> ReceiptResult<Vec<u8>> {
    if file.metadata()?.len() > limit {
        return Err(ReceiptError::TooLarge);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        while bytes.len() as u64 <= limit {
            let remaining = (limit + 1 - bytes.len() as u64) as usize;
            let count = file.read_at(&mut buffer[..remaining.min(4096)], bytes.len() as u64)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        if bytes.len() as u64 > limit {
            return Err(ReceiptError::TooLarge);
        }
        Ok(bytes)
    }
    #[cfg(not(unix))]
    {
        Err(ReceiptError::Unsupported)
    }
}
fn read_receipt_bytes(file: &File) -> ReceiptResult<Vec<u8>> {
    let bytes = read_bounded(file, RECEIPT_LIMIT)?;
    parse_receipt(&bytes)?;
    Ok(bytes)
}
pub(super) fn read_pending_bytes(file: &File) -> ReceiptResult<Vec<u8>> {
    let bytes = read_bounded(file, LIMIT)?;
    parse(&bytes)?;
    Ok(bytes)
}

fn current(file: &File, path: &Path, directory: bool) -> ReceiptResult<()> {
    let now = open_plain(path, directory)?;
    if identity(file)? != identity(&now)? {
        return Err(ReceiptError::Binding);
    }
    Ok(())
}
fn bind(root: &Path, root_file: &File, receipt: &CompletionReceipt) -> ReceiptResult<()> {
    current(root_file, root, true)?;
    let pending = parse(receipt.pending_json.as_bytes())?;
    if identity(root_file)? != pending.directory_identity {
        return Err(ReceiptError::Binding);
    }
    // Metadata binding only, not SQLite verification. Do NOT open/close another
    // active-database fd: on Unix that could release SQLite's POSIX read locks.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let db = fs::symlink_metadata(root.join("mno.sqlite"))?;
        if !db.is_file()
            || db.file_type().is_symlink()
            || db.nlink() != 1
            || Identity::from_parts(db.dev(), db.ino()) != pending.database_identity
        {
            return Err(ReceiptError::Binding);
        }
    }
    let recovery = root.join(DIRECTORY);
    match fs::symlink_metadata(&recovery) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
        Ok(_) => {
            let directory = open_plain(&recovery, true)?;
            // Missing pending is valid metadata after a future cleanup step.
            // This never authorizes accepting an empty recovery directory.
            match fs::symlink_metadata(recovery.join(NAME)) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
                Ok(_) => {
                    let marker = open_plain(&recovery.join(NAME), false)?;
                    if read_pending_bytes(&marker)? != receipt.pending_json.as_bytes() {
                        return Err(ReceiptError::Conflict);
                    }
                    current(&marker, &recovery.join(NAME), false)?;
                }
            }
            current(&directory, &recovery, true)?;
        }
    }
    current(root_file, root, true)
}
// Private fields; no conversion to Assessment, authorization, startup or maintenance.
// Retains exactly the inspected receipt object. Its metadata is not a stable
// SQLite snapshot or proof; a future classifier must perform all checks again.
pub(crate) struct LoadedReceipt {
    root: File,
    file_identity: Identity,
    file: File,
    bytes: Vec<u8>,
    receipt: CompletionReceipt,
}
impl LoadedReceipt {
    // Barrier only: this metadata type does not authorize cleanup.
    pub(super) fn sync_barrier(&self, name: &str, point: SyncPoint) -> io::Result<()> {
        classification::result::receipt::barrier(
            name,
            point,
            if point == SyncPoint::Directory {
                &self.root
            } else {
                &self.file
            },
        )
    }
    pub(super) fn pending_bytes(&self) -> &[u8] {
        self.receipt.pending_json.as_bytes()
    }
    pub(super) fn outcome(&self) -> &Outcome {
        &self.receipt.verified_outcome
    }
    // Observation only, private to this module: never authorizes missing Q.
    // Zero links is valid for the retained object's properties after a test-only
    // unlink; the named check below always requires exactly one link.
    fn check_handle(&self) -> ReceiptResult<()> {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = self.file.metadata()?;
            let directory = self.root.metadata()?;
            if !metadata.is_file()
                || metadata.nlink() > 1
                || !directory.is_dir()
                || identity(&self.file)? != self.file_identity
            {
                return Err(ReceiptError::Binding);
            }
            let bytes = read_receipt_bytes(&self.file)?;
            if bytes != self.bytes || parse_receipt(&bytes)? != self.receipt {
                return Err(ReceiptError::Binding);
            }
            let pending = parse(self.receipt.pending_json.as_bytes())?;
            if identity(&self.root)? != pending.directory_identity {
                return Err(ReceiptError::Binding);
            }
            // Recheck metadata after the bounded read, without resolving Q's name.
            let after = self.file.metadata()?;
            if !after.is_file() || after.nlink() > 1 || identity(&self.file)? != self.file_identity
            {
                return Err(ReceiptError::Binding);
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(ReceiptError::Unsupported)
        }
    }
    fn check_named_entry(&self) -> ReceiptResult<()> {
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: retained parent fd, fixed single-component Q name, valid
            // output storage. Never follows a symlink or opens another object.
            if unsafe {
                libc::fstatat(
                    self.root.as_raw_fd(),
                    c".mno-restore-completion-v1.json".as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(io::Error::last_os_error().into());
            }
            // SAFETY: fstatat succeeded and initialized the complete stat.
            let stat = unsafe { stat.assume_init() };
            if stat.st_mode & libc::S_IFMT != libc::S_IFREG
                || stat.st_nlink != 1
                || Identity::from_parts(stat.st_dev as u64, stat.st_ino) != self.file_identity
            {
                return Err(ReceiptError::Binding);
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(ReceiptError::Unsupported)
        }
    }
    fn check_name(&self, root: &Path) -> ReceiptResult<()> {
        current(&self.root, root, true)?;
        self.check_named_entry()?;
        self.check_handle()?;
        self.check_named_entry()?;
        current(&self.root, root, true)
    }
    // Metadata observation only. The consuming final-proof path owns the sole
    // unlink authority; this method neither removes Q nor accepts its absence.
    pub(in crate::backup::restore_context::pending) fn check_unlink_target(
        &self,
        directory: &File,
    ) -> ReceiptResult<()> {
        if identity(directory)? != identity(&self.root)? {
            return Err(ReceiptError::Binding);
        }
        self.check_handle()?;
        self.check_named_entry()
    }
    // Missing-name observation is gated by the actual consuming unlink proof,
    // and this exact retained receipt, not metadata, booleans or caller IDs.
    pub(in crate::backup::restore_context::pending) fn check_after_unlink(
        &self,
        root: &Path,
        proof: &classification::result::receipt::all_closed::QUnlinked<'_, '_>,
    ) -> ReceiptResult<()> {
        #[cfg(target_os = "macos")]
        {
            proof.require_receipt(self)?;
            self.check_unlinked_handle(root)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (root, proof);
            Err(ReceiptError::Unsupported)
        }
    }
    pub(in crate::backup::restore_context::pending) fn check_after_handoff(
        &self,
        root: &Path,
        handoff: &classification::result::receipt::all_closed::RunningHandoff<'_>,
    ) -> ReceiptResult<()> {
        handoff.require_receipt(self)?;
        self.check_unlinked_handle(root)
    }
    pub(in crate::backup::restore_context::pending) fn check_after_startup_handoff(
        &self,
        root: &Path,
        handoff: &classification::result::receipt::all_closed::StartupHandoff<'_>,
    ) -> ReceiptResult<()> {
        handoff.require_receipt(self)?;
        self.check_unlinked_handle(root)
    }
    fn check_unlinked_handle(&self, root: &Path) -> ReceiptResult<()> {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::MetadataExt;
            self.check_handle()?;
            if self.file.metadata()?.nlink() != 0 {
                return Err(ReceiptError::Binding);
            }
            bind(root, &self.root, &self.receipt)?;
            self.check_handle()?;
            if self.file.metadata()?.nlink() != 0 {
                return Err(ReceiptError::Binding);
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = root;
            Err(ReceiptError::Unsupported)
        }
    }
    pub(super) fn check(&self, root: &Path) -> ReceiptResult<()> {
        self.check_name(root)?;
        // Preserve all existing database/P/root binding checks. Handle-only
        // observation cannot substitute for this production verification path.
        bind(root, &self.root, &self.receipt)?;
        self.check_name(root)
    }
}

pub(crate) fn read(root: &Path) -> ReceiptResult<Option<LoadedReceipt>> {
    if !cfg!(target_os = "macos") {
        return Err(ReceiptError::Unsupported);
    }
    let root_file = open_plain(root, true)?;
    let path = root.join(RECEIPT);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            current(&root_file, root, true)?;
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let file = open_plain(&path, false)?;
    let bytes = read_receipt_bytes(&file)?;
    let receipt = parse_receipt(&bytes)?;
    bind(root, &root_file, &receipt)?;
    current(&file, &path, false)?;
    if read_receipt_bytes(&file)? != bytes {
        return Err(ReceiptError::Binding);
    }
    current(&file, &path, false)?;
    bind(root, &root_file, &receipt)?;
    Ok(Some(LoadedReceipt {
        root: root_file,
        file_identity: identity(&file)?,
        file,
        bytes,
        receipt,
    }))
}

pub(super) fn receipt_bytes(pending: &[u8], outcome: Outcome) -> ReceiptResult<Vec<u8>> {
    let pending_json = std::str::from_utf8(pending)
        .map_err(|_| ReceiptError::Invalid)?
        .to_owned();
    let bytes = serde_json::to_vec(&CompletionReceipt {
        format: "mno-restore-completion".into(),
        version: COMPLETION_FORMAT_VERSION,
        pending_json,
        verified_outcome: outcome,
    })
    .map_err(|_| ReceiptError::Invalid)?;
    parse_receipt(&bytes)?;
    Ok(bytes)
}
pub(super) fn publish_authorized(
    auth: &mut classification::result::receipt::Authorization<'_, '_, '_>,
) -> ReceiptResult<()> {
    #[cfg(target_os = "macos")]
    {
        publishing::authorized(auth)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = auth;
        Err(ReceiptError::Unsupported)
    }
}

// Only the Authorization-taking entry is available in production. Free-byte
// diagnostic helpers below remain strictly test-only.
#[cfg(target_os = "macos")]
mod publishing {
    use super::*;
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
    };

    fn named_identity(directory: &File, name: &CString) -> io::Result<Identity> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: owned directory/name are live; stat is initialized only on success.
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG || stat.st_nlink != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsafe receipt file",
            ));
        }
        Ok(Identity::from_parts(stat.st_dev as u64, stat.st_ino))
    }
    struct Temporary<'a> {
        file: File,
        directory: &'a File,
        name: CString,
        published: bool,
    }
    impl<'a> Temporary<'a> {
        fn new(directory: &'a File) -> io::Result<Self> {
            let name = CString::new(format!(".mno-completion-{}", uuid::Uuid::new_v4()))
                .map_err(io::Error::other)?;
            // SAFETY: live directory and NUL-terminated single-component name.
            // Exclusive creation never follows or replaces an existing entry.
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let file = unsafe { File::from_raw_fd(fd) };
            Ok(Self {
                file,
                directory,
                name,
                published: false,
            })
        }
        fn publish(&mut self) -> ReceiptResult<File> {
            if identity(&self.file)? != named_identity(self.directory, &self.name)? {
                return Err(ReceiptError::Binding);
            }
            // macOS atomic no-clobber rename, relative to the retained directory.
            // Unsupported filesystems/errors fail closed; NEVER fall back to rename.
            let destination = CString::new(RECEIPT).map_err(|_| ReceiptError::Invalid)?;
            #[cfg(test)]
            classification::result::receipt::event("rename-before-syscall")?;
            if unsafe {
                libc::renameatx_np(
                    self.directory.as_raw_fd(),
                    self.name.as_ptr(),
                    self.directory.as_raw_fd(),
                    destination.as_ptr(),
                    libc::RENAME_EXCL,
                )
            } != 0
            {
                let error = io::Error::last_os_error();
                // Capture errno before the test observer can make system calls.
                #[cfg(test)]
                classification::result::receipt::event("rename-syscall-error")?;
                return Err(error.into());
            }
            self.published = true; // Set before any subsequent fallible operation.
            if identity(&self.file)? != named_identity(self.directory, &destination)? {
                return Err(ReceiptError::Binding);
            }
            Ok(self.file.try_clone()?)
        }
    }
    impl Drop for Temporary<'_> {
        fn drop(&mut self) {
            if !self.published
                && matches!((identity(&self.file), named_identity(self.directory, &self.name)), (Ok(a), Ok(b)) if a == b)
            {
                // Only the original private temporary entry, never a final receipt.
                unsafe {
                    libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
                }
            }
        }
    }
    pub(super) fn authorized(
        auth: &mut classification::result::receipt::Authorization<'_, '_, '_>,
    ) -> ReceiptResult<()> {
        auth.check(None)?;
        let root = auth.root().to_owned();
        if let Some(existing) = auth.existing() {
            // Preserve the original opened object, including its exact bytes.
            existing.check(&root)?;
            if parse_receipt(&existing.bytes)? != parse_receipt(auth.bytes())? {
                return Err(ReceiptError::Conflict);
            }
            classification::result::receipt::barrier(
                "existing-file",
                SyncPoint::VerifiedFile,
                &existing.file,
            )?;
            classification::result::receipt::barrier(
                "existing-directory",
                SyncPoint::Directory,
                &existing.root,
            )?;
            classification::result::receipt::barrier(
                "existing-final",
                SyncPoint::FinalFileBarrier,
                &existing.file,
            )?;
            existing.check(&root)?;
            return auth.check(None);
        }
        classification::result::receipt::event("before-temporary")?;
        let root_file = open_plain(&root, true)?;
        if identity(&root_file)?
            != parse(parse_receipt(auth.bytes())?.pending_json.as_bytes())?.directory_identity
        {
            return Err(ReceiptError::Binding);
        }
        let mut temporary = Temporary::new(&root_file)?;
        let split = auth.bytes().len() / 2;
        temporary.file.write_all(&auth.bytes()[..split])?;
        classification::result::receipt::event("partial-write")?;
        temporary.file.write_all(&auth.bytes()[split..])?;
        temporary.file.flush()?;
        if read_receipt_bytes(&temporary.file)? != auth.bytes() {
            return Err(ReceiptError::Binding);
        }
        classification::result::receipt::barrier(
            "receipt-file",
            SyncPoint::VerifiedFile,
            &temporary.file,
        )?;
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(temporary.name.as_bytes());
        classification::result::receipt::event("before-publication")?;
        auth.check(Some(name))?;
        if read_receipt_bytes(&temporary.file)? != auth.bytes() {
            return Err(ReceiptError::Binding);
        }
        current(&root_file, &root, true)?;
        // A late collision is conservatively uncertain; never overwrite or
        // adopt a new object behind the retained evidence's back.
        let file = match temporary.publish() {
            Ok(file) => file,
            Err(error) => {
                // publish may have renamed successfully before its final check.
                auth.may_have_receipt = temporary.published;
                return Err(error);
            }
        };
        auth.may_have_receipt = true;
        drop(temporary);
        // Retain the published fd in the proof even when a subsequent check or
        // barrier fails. Installing evidence is not a durability authorization.
        let bytes = auth.bytes().to_vec();
        let receipt = parse_receipt(&bytes)?;
        auth.install(LoadedReceipt {
            root: root_file,
            file_identity: identity(&file)?,
            file,
            bytes,
            receipt,
        });
        classification::result::receipt::event("published")?;
        auth.check(None)?;
        let loaded = auth.existing().ok_or(ReceiptError::Binding)?;
        classification::result::receipt::barrier(
            "receipt-directory",
            SyncPoint::Directory,
            &loaded.root,
        )?;
        classification::result::receipt::barrier(
            "receipt-final",
            SyncPoint::FinalFileBarrier,
            &loaded.file,
        )?;
        classification::result::receipt::event("final-check")?;
        auth.check(None)
    }
    #[cfg(test)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Point {
        Temporary,
        PartialWrite,
        FileSync,
        BeforePublish,
        Published,
        DirectorySync,
        FinalSync,
        FinalCheck,
    }
    #[cfg(test)]
    fn phase(point: Point, hook: &mut impl FnMut(Point) -> io::Result<()>) -> ReceiptResult<()> {
        hook(point).map_err(Into::into)
    }
    #[cfg(test)]
    pub(super) fn publish(
        root: &Path,
        bytes: &[u8],
        hook: impl FnMut(Point) -> io::Result<()>,
    ) -> ReceiptResult<LoadedReceipt> {
        publish_with_sync(root, bytes, hook, durability::sync_opened)
    }
    // This seam lives entirely inside the macOS cfg(test) publisher, not in
    // production durability code. Errors returned by the sync operation itself
    // propagate through exactly the same ? path as real system-call failures.
    #[cfg(test)]
    pub(super) fn publish_with_sync(
        root: &Path,
        bytes: &[u8],
        mut hook: impl FnMut(Point) -> io::Result<()>,
        mut synchronize: impl FnMut(SyncPoint, &File) -> io::Result<()>,
    ) -> ReceiptResult<LoadedReceipt> {
        let receipt = parse_receipt(bytes)?;
        let root_file = open_plain(root, true)?;
        bind(root, &root_file, &receipt)?;
        // Test engine always requires the original pending marker to remain.
        let recovery_file = open_plain(&root.join(DIRECTORY), true)?;
        let marker = open_plain(&root.join(DIRECTORY).join(NAME), false)?;
        let check = || -> ReceiptResult<()> {
            bind(root, &root_file, &receipt)?;
            current(&recovery_file, &root.join(DIRECTORY), true)?;
            current(&marker, &root.join(DIRECTORY).join(NAME), false)?;
            if read_pending_bytes(&marker)? != receipt.pending_json.as_bytes() {
                return Err(ReceiptError::Conflict);
            }
            Ok(())
        };
        check()?;
        let destination = root.join(RECEIPT);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(ReceiptError::AlreadyExists),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let mut temporary = Temporary::new(&root_file)?;
        phase(Point::Temporary, &mut hook)?;
        let split = bytes.len() / 2;
        temporary.file.write_all(&bytes[..split])?;
        phase(Point::PartialWrite, &mut hook)?;
        temporary.file.write_all(&bytes[split..])?;
        temporary.file.flush()?;
        if read_receipt_bytes(&temporary.file)? != bytes {
            return Err(ReceiptError::Binding);
        }
        phase(Point::FileSync, &mut hook)?;
        synchronize(SyncPoint::VerifiedFile, &temporary.file)?;
        phase(Point::BeforePublish, &mut hook)?;
        check()?;
        // Recheck actual bytes at the last injectable pre-publication boundary,
        // not merely the caller's already parsed input buffer.
        if read_receipt_bytes(&temporary.file)? != bytes {
            return Err(ReceiptError::Binding);
        }
        let file = temporary.publish()?;
        drop(temporary);
        // No cleanup guard owns the final path: ALL late errors preserve it.
        phase(Point::Published, &mut hook)?;
        current(&file, &destination, false)?;
        phase(Point::DirectorySync, &mut hook)?;
        synchronize(SyncPoint::Directory, &root_file)?;
        phase(Point::FinalSync, &mut hook)?;
        synchronize(SyncPoint::FinalFileBarrier, &file)?;
        phase(Point::FinalCheck, &mut hook)?;
        check()?;
        current(&file, &destination, false)?;
        if read_receipt_bytes(&file)? != bytes {
            return Err(ReceiptError::Binding);
        }
        Ok(LoadedReceipt {
            root: root_file,
            file_identity: identity(&file)?,
            file,
            bytes: bytes.to_vec(),
            receipt,
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;

#[cfg(all(test, not(target_os = "macos")))]
mod unsupported_tests {
    use super::*;
    #[test]
    fn receipt_read_is_unsupported_without_creating_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("absent");
        assert!(matches!(read(&path), Err(ReceiptError::Unsupported)));
        assert!(!path.exists());
    }
}
