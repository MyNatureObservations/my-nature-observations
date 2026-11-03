# Verified Startup admission (4e.3)

This is an internal mechanism, not connected to production startup, restore commands or the frontend. Only a consumed `StartupHandoff` derived from `DiskRecoveryCompleted` can reach `DatabaseState::admit_startup_verified`. Running tokens are a different type and cannot be substituted.

## Owner and startup verdict

The original `OnceLock<StartupOutcome>` is never replaced. Its `RecoveryGate` retains the same `DirectoryLease`, directory descriptor and process lock. The handoff retains the same detached session, reservation, restore binding, descriptors and verified bytes. No new reservation, lease or OS lock is acquired.

A private `startup_admitted` latch is published only by successful verified admission. `normal_startup()` accepts the retained Recovery verdict only when this latch AND Ready are present, followed by the original lease identity check. Changing phase alone cannot bypass the gate. Startup recovery-session creation is disabled after admission. Subsequent Running recovery still uses the original Maintenance protocol.

## Checks and SQLite

The generation write guard is acquired before verification; poisoned coordination returns a retained failure. Detached-session checks compare the actual DatabaseState and reservation slot, Startup origin (no Maintenance/operation pointer), captured generation, active reservation, Recovery phase, original Recovery verdict and live DirectoryLease. They do not call `startup_generation()` or reacquire the generation lock.

Before open, after open, after close and immediately before publication, fresh checks validate directory and database identities, the full physical SQLite-file SHA-256, protection archive identity/hash, exact pending/restore binding and old receipt handle/bytes, establishment marker identity/bytes and absence of Q, P, recovery directory, SQLite sidecars and reserved artifacts. The missing-Q mode is private and requires the same completed Startup handoff; it is not an optional boolean.

The existing descriptor-bound read-only VFS opens only the retained database object without CREATE, recovery, schema repair or writes. The existing journal verifier checks integrity, exact schema and documents. The required order is `VerifiedJournal::finish()` (checked ROLLBACK), `Connection::close()` (sqlite3_close), then checked VFS unregister. Checks repeat after close. Any error denies admission.

## Generation, locks and publication

Startup preserves the captured process-local generation, even at u32::MAX: no addition is necessary. No operation permit can be issued by this DatabaseState before its startup authorization. Operations from the terminated process cannot carry permits into the new process. A queued request may execute only after verified publication; future frontend integration must still discard old payloads and acquire a new journal session rather than retag old autosaves.

Lock order: original DirectoryLease retained throughout; generation write guard; borrowed file/session checks; temporary read-only SQLite binding. No Maintenance is created. No `normal_startup()` reinitialization or process-lock reacquisition occurs.

After every fallible check, the commit sequence contains only three release stores:

1. startup_admitted = true
2. recovery_required = false
3. phase = Ready

There is no I/O, callback, allocation, cancellation check or fallible lock in those stores. The write guard excludes journal operations until all stores and release of the closed proof owners complete. Early observations by Maintenance still fail the phase/authorization gate or wait on the write guard. No destructor publishes Ready.

## Failure and panic

Failures retain the SQLite binding (live or failed-close), handoff, session, reservation and file owners. Database binding destruction precedes retained file and session destruction. A close failure never frees VFS callbacks prematurely. Unregister failure retains registered allocations. The existing safe PreparedRestore shutdown contract is unchanged; staging can be cleaned only once its SQLite use is safely closed.

An unwind before publication conservatively retains the complete owner, and poisons the generation write lock. Phase, startup authorization and generation remain unchanged; operations stay blocked. A failure owner's ordinary Drop may safely close resources and release the session slot, but cannot authorize normal operation. Conservative retention can last until process termination. Q is not recreated.

## Tests and boundaries

Tests cover Old/New, same owner/lease and reservation slot, real SQLITE_BUSY, IOERR, unregister failure, denied ROLLBACK after confirmed validation, wrong state/origin/operation/generation/reservation, returned artifacts, changed files/marker, directory replacement, cancellation, panics and queued operations. A phase-only test proves the immutable Recovery verdict cannot be bypassed. Compiler probes extract the real entry method and require a successful control probe before checking origin mismatch, consumption, private construction, lifetime escape, Clone and Deserialize failures.

macOS is the recovery target. Linux/Windows remain Unsupported before SQLite open/admission; no platform execution on those systems is claimed. Arbitrary same-user external mutations between the final filesystem check and publication remain outside the accepted threat model; descriptor identity/hash checks catch observed changes. Subprocess crashes are not proof of hardware power-loss guarantees. Cancellation after the last check may still be followed by success.

No production restore, frontend integration or automatic SQLite recovery is introduced. Future integration must separately handle startup retry/error presentation and frontend session lifecycle.
