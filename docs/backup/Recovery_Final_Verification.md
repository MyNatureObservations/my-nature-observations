# Consuming final file verification – 4c.3c

This internal macOS-only checkpoint consumes `AllConnectionsClosed`. It does not
activate restore, delete Q, change generation, enter Ready, or open SQLite.
Linux/Windows retain Unsupported recovery behavior with no fallback.

## API and ownership

`AllConnectionsClosed::verify_final(self) -> Result<FinalFilesVerified<'a>, FinalVerificationFailure<'a>>`

Both private-field result owners retain the complete AllConnectionsClosed. Neither
is Clone/Copy/Serde, exposes SQL, authorizes cleanup, or has an unlock method.
Running retains its ContextConnectionsClosed, actual Maintenance borrow and
session binding; Startup retains the genuine startup session without a fictitious
context. The session retains its exclusive reservation and borrow of the
DatabaseState-owned DirectoryLease/process lock. Existing operation/session,
physical identity, path and generation checks are reused.

A private Retained owner is established before any checks. After successful first
file verification it additionally owns the exact establishment-marker descriptor.
The borrowed FileChecks is converted into that descriptor; it is not stored
alongside its borrowed owner. This avoids self-reference, unsafe lifetime extension
and a second establishment open between checks. If the first check fails, no
marker retention is promised; all pre-existing AllConnectionsClosed resources
remain owned. Later failures retain the opened establishment descriptor too.

## Exact order

1. Use the existing file_checks constructor: check platform, cancellation, phase,
   reservation, session/operation/generation, lease and retained directory/file
   bindings, then validate all file hashes, names, Q, establishment and artifact
   absence. All SQLite/VFS connections were already explicitly closed.
2. Retain that exact establishment object.
3. Apply the following mandatory barriers using retained descriptors only:

| Order | Object | Mechanism on macOS |
| --- | --- | --- |
| 1 | Active journal | F_FULLFSYNC (VerifiedFile) |
| 2 | Protection archive | F_FULLFSYNC (VerifiedFile) |
| 3 | Q | F_FULLFSYNC (VerifiedFile) |
| 4 | Establishment marker | F_FULLFSYNC (VerifiedFile) |
| 5 | backups/pre-restore | fsync |
| 6 | backups | fsync |
| 7 | Data directory | fsync |
| 8 | Active journal | F_FULLFSYNC (FinalFileBarrier) |
| 9 | Protection archive | F_FULLFSYNC (FinalFileBarrier) |
| 10 | Q | F_FULLFSYNC (FinalFileBarrier) |
| 11 | Establishment marker | F_FULLFSYNC (FinalFileBarrier) |

The directory order confirms the backup namespace from child to parent. Root
covers Q, establishment and journal entries and the observed absence of recovery.
Ancestor creation was already made durable by the earlier startup/protection
protocol; this operation creates no directory or name. There is no recovery
handle sync here: recovery was removed and that removal already confirmed.

4. Repeat the SAME FileChecks implementation with the SAME retained establishment
   handle after every required barrier has completed. This rechecks current names,
   physical identities, exact Q/pending bytes and outcome, established format,
   absence of P/recovery/sidecars/reserved temporaries, and full journal/archive
   hashes against existing reference values. Recheck phase/session/generation and
   cancellation at the final boundary.
5. Only after all checks succeed return FinalFilesVerified.

Cancellation is checked before each barrier and throughout streaming verification.
Every sync error returns a failure naming the barrier; no best-effort fallback,
retry or partial-success capability exists. Existing durability::sync_opened is
used, including its checked return codes and interrupted-call handling.
No reference hash is created or updated by this operation.

## Error, cancellation, panic and Drop

Ordinary failures retain the complete closed chain, reservation, session and file
handles. The error distinguishes file checks, durability failure and test checkpoint
I/O failure; it does not claim earlier barriers were undone. Q remains present.
No files are removed or repaired, and phase and generation remain unchanged.

DuringVerification establishes an unwind-retention guard before checks start.
Unexpected unwinding conservatively forgets the entire closed owner, including
staging, marker, reservation and retained descriptors. No cleanup runs on this
panic path. The retained borrows are never accessed; there is no live SQL/VFS
callback and no unsafe lifetime extension. Descriptors remain until process exit;
the same live state stays Recovery/Busy. This intentional panic-only retention
avoids deletion even of private staging. Ordinary success/failure moves the owner
out and introduces no leak. Dropping a returned result follows the already approved
closed-resource Drop contract (known closed staging can be cleaned; Q and finished
backups remain). No destructor activates Ready or changes generation. The process
lock stays owned by DatabaseState. Release panic=abort terminates the process and
produces no successful verification result. No retry or SQLite recovery occurs.

## Limited authority and TOCTOU

FinalFilesVerified describes the successful checkpoint and accepted sync results.
It is not a permanent snapshot after SQLite locks were released. The shared
process lock coordinates MNO processes, not arbitrary same-user external writers.
Retained descriptors bind barriers to previously verified objects; repeated name,
byte/hash and layout checks detect observed changes. Transient external changes
between system calls are outside the existing threat model. Successful system
barriers cannot independently prove hardware behavior during real power failure.

Before any later Q deletion, new immediate checks of session/phase/generation,
lease, current names/physical identities, hashes, Q bytes, establishment and artifact
layout are required. No Q deletion or normal-drift API is part of this step.

## Tests

Tests exercise both genuine Running and Startup chains. They observe the exact
11-barrier order with real sync calls; inject errors at each required barrier;
mutate journal, archive, Q and establishment before/after barriers; replace the
journal with byte-identical content; reintroduce P, recovery, all three SQLite
sidecars and a reserved temporary. Attacks target the actual pending-bound archive,
including Running's newly created protection. Explicit hook-reached assertions
prevent early rejection from passing a boundary test accidentally.

Tests also cover cancellation and caught panic before/after barriers, unchanged
Q and journal at ordinary failures, retained reservation/process lock, Recovery
and generation. Negative compiler probes use actual capability declarations and
consuming API with only the I/O backend stubbed; a positive control must compile
before checking private construction, reuse, Clone/Copy/Serde, lifetime escape,
early guard Drop and absence of cleanup/unlock APIs. Hooks remain cfg(test).
Real power-loss/subprocess crash behavior and the separately documented
RetainedMarker syscall-injection improvement remain future work.
