# WriteEpochInputs — unopened shared resource owner (5d.3e)

`WriteInputs::into_epoch_inputs(self) -> WriteEpochInputs<'m, 's>` is a crate-
private consuming, infallible whole-owner move. The result has one private field,
containing the original WriteInputs. It is not a writer, active lock epoch,
verification result, or new authorization. No constructor from paths, IDs,
metadata or booleans exists. No Clone/Copy/Serde or resource accessor is exposed.
Both Maintenance and DatabaseState lifetimes remain unchanged.

## Resources and dependency order

The aggregate contains exactly the original CloseOwner/ActiveCloseParts:

- A's confirmed-closed SQLite connection has already been consumed. Its extra
  main-file File, physical identity and canonical path remain in ActiveFiles.
- B incoming PreparedRestore and C VerifiedProtection.candidate still own their
  read-only SQLite connections, retained staging files, manifests and staging
  directories. VerifiedProtection retains the existing Maintenance-bound backup
  path and binding; no new archive File is opened or claimed to be owned.
- OwnedPending retains root/recovery/P files, exact original bytes, captured
  identities, restore operation and generation.
- The original ContextSessionBinding, Maintenance borrow, original session and
  exclusive Reservation remain in the same aggregate. DirectoryLease and the
  process lock remain owned by DatabaseState, not copied into this object.

Dropping any extra fd for the main inode could affect POSIX locks on that inode.
B/C, the protected ZIP, P and directories are intended to be different objects;
future writer integration must verify that distinction rather than trust names.

## Transfer, errors and Drop

The method body consists solely of `WriteEpochInputs { inputs: self }`.
There is no allocation, callback, field take, check, file lookup, SQLite call,
new open, close or fallible midpoint. Therefore it needs no transfer failure
owner or partial-transfer rollback. A caller handling a later error can retain
or return the entire aggregate. No independent proof is emitted.

Normal Drop currently delegates to unchanged WriteInputs/CloseOwner destruction:
A's retained files, B/C controlled shutdown/staging cleanup, P and binding, then
session/reservation. Recovery remains set and generation unchanged. This normal
cleanup is appropriate because no writer can be created or borrowed through the
new API. Failed B/C close still retains connection and staging through their
existing PreparedRestore Drop contract.

On unwind the existing CloseOwner guard retains the entire chain, including the
session. No new permanent-retention mechanism is introduced. Release abort and
process death do not execute normal unwind guarantees; durable P remains the
startup blocker. Panic tests cover immediately before/after the indivisible move;
there is no panicking instruction inside the move to inject into.

## Boundary against moving the lock problem

The type exposes neither owned/borrowed File or Connection nor an extraction
method. A caller cannot create a writer from this API while separately dropping
its resource dependencies. Any future writer factory must consume this complete
owner into its own guarded aggregate and keep it until checked writer close.
That future change requires a fresh review, including Drop and error retention.

This is NOT proof of process-wide fd safety. It does not govern arbitrary other
modules opening the known main path, nor is it a kernel-lock test. Maintenance
call-site controls, permission-bearing open APIs, writer close proof and the
5d.3d inventory must still be integrated before opening a writer. Adding a
separate writer beside this owner and trusting its current normal Drop would
reintroduce the original bug and is outside the contract.

## Tests

Real durable P, existing authorized context, closed A and a real RecoverySession
are used. Tests observe the same descriptors, retained P identities/bytes,
Maintenance pointer, session ID, live B/C, real competing-session Busy, unchanged
main bytes, generation and Recovery. B/C close observations are empty across
the move and normal Drop handles staging before reservation release. Error return
keeps the whole owner; panic before/after transfer retains staging and reservation.

The no-I/O guarantee follows directly from the complete production method body;
fd equality/identity observations are supporting runtime checks, not an OS syscall
trace. Test observations outside that body may open P or hash B/C as before.
No writer is running during these tests.

Compiler probes project the actual production owner/methods with I/O/backend
stubs, require positive compilation, and reject reuse, Clone, free construction,
private-field extraction, SQL access, lifetime extension and outer callback escape.
Runtime tests exercise the real I/O and binding checks omitted by those stubs.

## Borrowed inspection (5d.3f)

`WriteEpochInputs::with_verification` supplies an `EpochVerificationView` only
inside a higher-ranked callback. Its private fields borrow stored P bytes and
carry the original session ID, operation ID and generation. These are historical
metadata, not freshly checked disk state or a detached authorization. Missing
private owner components return None rather than manufacture information.

The view has no File, fd, Connection, path, mutable resource, extraction, Clone,
Copy or serialization API. It cannot escape the callback, including as a borrowed
byte slice. Ordinary scalar metadata or caller-created byte copies may be returned;
these carry no authority. No I/O, hash, sync, duplication, open or close is done by
the view or its getters. The inspection callback is caller code and is not an I/O
sandbox; it must not use unrelated path helpers during a future writer epoch.

The view owns no resource and adds no Drop behavior. A callback panic leaves an
externally held owner intact; if the owner itself unwinds, the existing whole-chain
retention applies. Normal owner destruction and B/C close order are unchanged.
This does not establish writer/kernel-lock safety or replace the pending fresh
identity, hash, artifact and reservation checks required before future writing.

Runtime tests use actual durable P/session resources and check metadata, unchanged
P, live B/C, competing-session Busy and Recovery/generation, including a caught
callback panic. Compiler probes reject view/byte escape, owner consumption during
inspection, free construction, Clone and resource extraction. No-I/O follows from
the inspected implementation, not from fd-number observations or syscall tracing.

## Internal pre-lock check (5d.3h)

`WriteEpochInputs::check_before_writer(&self) -> Result<(), PreflightError>`
borrows the original aggregate, accepts no arguments/callback, and emits no proof.
It is only for BEFORE a future writer lock episode. Success expires as an
observation; a future consuming writer factory must repeat relevant checks.

It reuses check_after before/after inspection: actual session/reservation and
Maintenance binding, generation, retained root/recovery/P identities, P bytes,
main identity, and live B/C snapshots with manifest hashes. Main must also be a
regular non-symlink with one link. Durable P comes from the existing publication
ownership chain; this method does not redo publication barriers or prove hardware
durability. It compares the protected archive's current SHA-256 to P's original
archive hash, verifies its expected managed path and plain parent directories,
and rechecks its named identity against the opened archive. No original archive
inode was retained by WriteInputs: an identical-byte replacement is not claimed
to be distinguishable from the original ZIP.

Allowed layout is the existing recovery directory containing only canonical P;
Q, sidecars (including ASCII case aliases), reserved root temporaries and other
recovery entries block. Unrelated root files remain allowed by shared policy.
No files are deleted, repaired, created or written by the check.

I/O inventory: check_after -> session binding -> DirectoryLease checks opens and
closes temporary directory/lock handles. P checks open/close root/recovery/P and
read retained P with read_exact_at. B/C source performs SQLite is_autocommit,
metadata and a staging try_clone/hash/drop. Preflight opens/closes one protected
archive with open_plain and reads it with read_at; it reads directory metadata,
Q metadata and root/recovery directory listings (temporary directory descriptors).
It opens no main-file handle and no new SQLite connection. These composed helpers
are NOT approved under an active SQLite lock episode; rejected opens and alias
races would require episode-level retention before such reuse.

Errors borrow rather than consume: original files, P, B/C, Maintenance and the
same session/reservation remain owned. No field take or partial transfer occurs.
The existing CloseOwner unwind guard is unchanged. A test-only one-shot panic
checkpoint after initial verification proves retention with a real owner; it is
absent from production. No new permanent retention mechanism is introduced.

Tests cover success; altered P bytes/identity, main identity, generation, session
binding, B/C bytes, archive bytes, sidecars and reserved artifacts; caught panic;
real competing-session Busy and continued Recovery/generation. Existing compiler
probes still compile the real owners; the new method body is stubbed there because
it adds no new lifetime boundary. Runtime tests exercise its actual I/O.

Not established here: a main-file SHA-256 baseline or logical content validation,
a writable descriptor/native writer, process-wide fd exclusion, writer-close proof,
rollback-journal policy, or future Ready authority. External concurrent mutation
and path TOCTOU remain within the previously documented threat-model boundary.

## Consuming preparation owner (5d.3j)

`WriteEpochInputs::prepare_episode(self)` returns either a private-field
`PreparedWriteEpisode<'m, 's>` or a boxed `EpisodePreparationFailure<'m, 's>`.
It calls the existing check_before_writer exactly once while the entire self
still owns its original CloseOwner. No field split or alternate check is added.
Success moves the whole original WriteEpochInputs into the prepared owner.
Failure moves it into the failure owner beside the original PreflightError.
`error()` borrows that error; `into_inputs(self: Box<Self>)` returns only the
complete original aggregate. Debug formatting prints the error, not resources,
and cannot consume the owner. No standalone preparation or writing proof exists.

The preflight itself still performs the I/O documented above. The new wrapper
adds no opens, closes, SQL, callbacks or descriptor access. Boxing the failure
allocates; allocation failure follows Rust's abort behavior, not recoverable
unwind. No new retention scheme or unsafe code is introduced.

Both success and failure have unchanged nested normal destruction: existing
unopened resource cleanup closes B/C safely, handles staging, and drops the
session/reservation after dependent owners. Ordinary abandonment does not undo
P or release Recovery. It is safe only for this unopened aggregate. Panic during
preflight unwinds self through the existing CloseOwner, which conservatively
retains the whole chain; no resource has been extracted. A prepared owner that
itself unwinds follows the same nested guard. Process abort cannot promise Drop;
durable P remains the startup blocker.

The prepared type has no constructor apart from the consuming method, no Clone,
Copy or Serde, and no resource accessors, writer/open/close methods, SQL,
classification, handoff or Ready authority. It retains both original lifetimes.
Maintenance stays externally owned; DirectoryLease/process lock stay in state.

This is a checked preparation observation plus ownership, not proof that other
SQLite locks are absent, not a stable authorization result, and not POSIX-lock
safety. Future consuming writer integration must repeat relevant fresh checks,
close the process-wide fd boundary and enforce checked writer-close ordering.
No writer slot or writer-close implementation is introduced here.

Tests use real durable P/session/closed-A/live-B/C resources. They cover complete
successful transfer, same descriptors/Maintenance/session and real competing
reservation Busy; normal staging cleanup before reservation release; changed P
and replaced main-file rejection with whole-owner return; safe error formatting;
and a precise panic from the real preflight while self is consumed. Existing
compiler projection keeps the new production factory/types intact, stubbing only
the existing I/O backend. Positive preparation compiles; private construction,
Clone, field extraction, reuse after consumption, classification method and
state-lifetime extension are rejected. No-writer-open follows from the reviewed
factory and its existing preflight, not a syscall-count assertion.
