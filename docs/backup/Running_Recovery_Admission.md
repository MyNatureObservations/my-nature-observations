# Controlled Running admission (4e.2)

`Maintenance::resume_verified(&mut self, RunningHandoff)` is internal and not
connected to a command, restore coordinator or frontend. StartupHandoff cannot be
supplied; StartupOutcome/RecoveryGate admission is unchanged. Success returns unit,
not a reusable unlocking token. Failure retains the handoff and any new SQLite
binding in RunningAdmissionFailure. No new process lock or reservation is taken.

## Fresh verification and origin

The detached session must match the actual Maintenance address, DatabaseState,
operation UUID, original generation and reservation slot. The reservation remains
owned and active, and Recovery, recovery_required and maintenance must all remain
set. DirectoryLease checks its retained directory and lock-file identities.
The immutable session UUID and operation records remain those moved by 4e.1.

The shared physical verification backend rechecks exact pending binding, physical
journal identity and full SQLite-file SHA-256, protection archive identity/hash,
all directory identities, establishment descriptor/name/bytes, absent Q/P/recovery,
SQLite sidecars and reserved artifacts. The new private Running receipt mode
requires the real RunningHandoff and exact receipt reference retained by it. It
reuses the old unlinked-Q handle validation, including zero links and exact bytes;
it never makes a missing-Q boolean an authorization. Named-Q verification remains
mandatory for earlier chain stages. No historical disk proof alone grants Ready.

## Read-only SQLite validation

After fresh checks, the existing BoundSqlite opens the retained journal object via
its descriptor-bound custom VFS with READ_ONLY and NOFOLLOW, without CREATE.
The existing VerifiedJournal performs integrity, exact schema and document checks
in a read transaction. Its scope ends before explicit BoundSqlite::try_close,
which uses Connection::close / sqlite3_close and then checks VFS unregister.
No schema initializer or repair path is invoked. Persistent journal/WAL writes and
journal deletion are denied by the existing VFS. Artifact checks run before open,
after open and after close. This is validation, never automatic SQLite recovery.
Hash/identity and artifact checks are repeated after close and at the final boundary.

A real outstanding statement yields SQLITE_BUSY. Close failures retain the failed
BoundSqlite, including its connection and registration; unregister failure retains
registered allocations. The error records the SQLite code or unregister failure.
The validation connection is the first field of its aggregate, before all handoff
file/staging owners. Existing BoundSqlite conservative Drop rules remain in force.
On validation error the still-live binding is retained as well. No automatic reopen
or retry is performed. These paths keep phase/generation unchanged.

## Lock order and commit point

The original OS process lock and session reservation remain held. The original
Maintenance RwLock write guard already excludes normal operations; it is not
reacquired. All filesystem checks, SQL, explicit close, cancellation checks,
allocation and test callbacks finish before publication. Controlled checked_add
computes the next u32 generation; exhaustion returns an owning error without wrap.

The final sequence is exactly:

1. Assign the checked next generation through the existing write guard.
2. Store recovery_required=false with Release ordering.
3. Store phase=Ready with Release ordering.

These three operations cannot allocate, perform I/O, invoke callbacks or panic.
They are not a multiword hardware atomic transaction: the existing write guard
prevents journal operations from observing intermediate states. Closed file owners
and the reservation then drop while Maintenance still owns that write guard;
Maintenance is released by the caller afterwards. Drop alone never grants Ready.
Old and New both increment exactly once by consuming the noncloneable handoff.
Queued saves validate their supplied generation only after acquiring their read
permit, before opening SQLite, so old payloads are rejected rather than relabelled.

## Errors, cancellation, panic and restart

Every ordinary failure before the commit point owns the whole validation/handoff
aggregate and leaves Recovery and generation unchanged. An unwind during validation
conservatively retains the aggregate, following the existing recovery policy.
No hook, fallible operation or resource split lies between validated return and
the three publication operations. Cancellation after the last check may therefore
still result in successful admission. There is no new check inside the commit point.

A process crash after durable disk completion but before Ready leaves no new disk
marker. A later process uses normal strict startup on the already completed disk
state. This step changes no crash protocol. The generation remains process-local.

## Tests and boundaries

Deterministic tests cover Old/New, a genuinely queued old operation, concurrent
Maintenance, old/new generations, real SQLITE_BUSY, other close errors, unregister
failure, real SQL authorization failure during validation, every boundary error,
late file/artifact changes, cancellation, wrong guard/generation/reservation,
overflow and panic with retained file ownership. Compiler probes retain the real
resume_verified method body while stubbing verification backends, and require the
consuming Running type. Existing callback/lifetime probes continue to run.
All injection and state-corruption helpers are cfg(test), not release behavior.

No production restore, frontend restart, Startup admission or automatic recovery
is enabled. Linux/Windows recovery remains Unsupported. DirectoryLease coordinates
MNO processes; arbitrary external writes between the last check and publication
remain outside the existing guarantee. Power-loss behavior is not established by
these runtime tests. The original Maintenance must stay with its coordinator;
its recorded address is equality-only and never dereferenced as a saved pointer.
