# Recovery file checks – 4c.2c.4c.3b

These are reusable read-only observations after SQLite/VFS shutdown, not final
verification authority. No FinalFilesVerified, durability barriers, deletion,
generation change, Ready transition, SQLite-open or automatic recovery is added.
Recovery remains macOS-only; Linux/Windows fail closed as Unsupported.

## API and ownership

`AllConnectionsClosed::file_checks(&self) -> Result<FileChecks<'_, '_>, FileCheckError>`
opens and retains the existing establishment marker, checks the files, and returns
an observation object borrowing the entire closed capability. `FileChecks::check()`
repeats the checks. Neither object authorizes mutation. There are no free identity
or path parameters, Clone/Copy/Serde, SQL accessors, or unlock methods.
The borrow retains the actual RecoverySession, reservation, DatabaseState-owned
DirectoryLease, process lock, Maintenance when Running, and all source handles.
Dropping the observation closes only its establishment-marker descriptor; it does
not consume the parent capability or change phase/generation.

## Existing reference values

The shared `OperationBinding::check_files` is used both by live Observation
reverification and by these post-close observations. It reuses Inputs/Evidence,
LoadedReceipt::check, safe name/identity checks, pending parsing, and the existing
streaming hash routine. Existing live SQLite/VFS checks remain around this shared
file-only part; the post-close path never invokes them.

- `database_sha256` is the previously accepted SHA-256 of the *entire physical
  SQLite file*, established before classification's SQLite-open and compared
  around strict logical verification. It is never replaced with a new baseline.
- Pending `old_content`/`new_content` are logical journal fingerprints. They are
  different from the physical file hash; exact unchanged file bytes preserve the
  previously verified logical content without reopening SQLite.
- Pending `protection_sha256` is the expected SHA-256 of the complete backup
  archive, previously extracted and strictly verified as Old. No new extraction
  occurs. Retained archive and backup-directory descriptors remain authoritative.
- Exact retained pending bytes, restore-ID, physical database/directory identities
  and Q bytes/outcome are checked using the existing verification chain.

File name identity is checked against retained descriptors before and after
streaming. Replacements, symlinks and multiple hard links are rejected. The hash
uses bounded 64 KiB reads with cancellation checks; it imposes no book-size limit.
Errors distinguish owner/binding, journal content, protection, P/Q/identity,
establishment, reserved artifacts, and I/O failures through FileCheckError/Reason.

## Establishment

`startup::established::RetainedMarker` opens the fixed `.mno-initialized-v1.json`
name with openat relative to the retained root descriptor, O_NOFOLLOW,
O_NONBLOCK and O_CLOEXEC. It requires a regular single-link file. fstatat with
AT_SYMLINK_NOFOLLOW compares its current named identity to the retained object
before and after reading. Bounded positional reads validate the exact version-1
bytes using the same format predicate as startup. Absence, invalid content,
replacement, symlink and unexpected type fail; nothing is created or repaired.

## Reserved artifact policy

Post-close observations reject any object at `recovery` or `pending-restore.json`
in the data directory and any SQLite sidecar `mno.sqlite-journal`, `-wal`, `-shm`.
They reuse the classifier's existing sidecar checks (NotFound only means absent).
Any recovery entry is rejected, even if an empty directory, symlink or partial file.

The root namespaces `.mno-completion-*`, `.mno-init-*`, `.pending-*` and
`.mno-restore-completion*` are reserved. The only permitted completion name is the
verified `.mno-restore-completion-v1.json` object. Completion temporary recognition
is shared with Evidence, including non-UTF8 suffixes. Unknown versions and orphan
temporaries fail closed and are never cleaned. Other names, user notes and
unrelated files are not rejected. Existing finished backups are preserved.

## Limits and remaining work

These checks do not establish durability or a permanently stable post-close
snapshot. SQLite locks are already released. DirectoryLease coordinates MNO
processes; it is not a filesystem sandbox against arbitrary same-user writers.
Observed name/content changes are rejected. Arbitrary external manipulation that
changes and restores objects between observations cannot be excluded.
Root artifact enumeration and existing Inputs/Q checks still use paths with
retained-root identity checks bracketing them; marker open/check and absence of
P/recovery use directory-relative operations. No atomic filesystem-wide snapshot
is claimed. Hashing and name observations remain separate system calls.

Cancellation/error/panic never changes recovery artifacts, journal bytes, phase or
generation. A caller retains AllConnectionsClosed on ordinary error because the
API borrows it. Panicking while borrowing cannot free its protected resources.
Access-time updates caused by ordinary filesystem reads are not journal writes.

4c.3c must consume the real capability, retain all handles (including establishment),
perform final checks before and after mandatory durability barriers, and return a
private success/failure owner. A later Q removal must perform fresh immediate
checks; this observation is not permission to remove Q or enter Ready.

## Verification

Deterministic isolated tests cover unchanged Startup and Running, unrelated files,
changed bytes and replaced journal/archive/Q/establishment files, symlinks and
hardlinks, missing/invalid establishment, returned P/recovery/sidecars, reserved
temporaries, cancellation and panic after an actual hash block. Input tree bytes
are compared after attacks and checks to ensure checks do not clean or repair.
Running/Startup tests retain reservation/process lock and Recovery with unchanged
generation. Existing full-suite tests cover the earlier capability and close chain.
No timing-based races or release test hooks are introduced. Physical power-loss
behavior is outside this step, which adds no durability operations.

Non-blocking future test improvement: add targeted syscall-failure injection for
RetainedMarker openat/fstatat/read_at. Current failure handling is supported by
code review and existing surrounding error tests; dedicated injection coverage
can be added separately.

## Consuming checkpoint implemented in 4c.3c

The new consuming final verifier reuses `check_retained`, the same implementation
as FileChecks::check, and transfers the retained establishment descriptor into its
owned success/failure chain. See [Recovery_Final_Verification.md](Recovery_Final_Verification.md)
for the required barrier order and limited authority. FileChecks itself remains
read-only and does not authorize any mutation.
