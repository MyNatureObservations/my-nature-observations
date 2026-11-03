# Actual Maintenance retention — test-only prototype (5d.3p)

## Isolation and resources

`database_state::maintenance_retention` is compiled only under
`cfg(all(test, target_os = "macos"))`. The production DatabaseState/Maintenance/
Operation API is unchanged. No writer, VFS, restore, journal command, backup,
Recovery or Ready implementation is added.

The private `Owner<'s>` contains `Option<Box<Parts<'s>>>`. Parts owns, in order:
1. `Option<rusqlite::Connection>`;
2. the REAL `Maintenance<'s>`, including its actual generation write guard;
3. the private fixture TempDir.

The fixture initializes a real DatabaseState through normal startup in a private
managed data directory BEFORE acquiring Maintenance. The test connection is then
opened READ_ONLY without CREATE. It never writes journal rows. State remains the
outer owner of its DirectoryLease/process lock. This is not a dummy Maintenance
or a replacement AtomicBool lock. There is no Connection extraction API, Clone,
Copy, Serde, PreparedWriteEpisode integration or self-referential owner.

## Close contract

`Owner::close(self)` takes its connection and calls actual Connection::close.
On error the returned Connection is immediately restored before creating the
explicit Failure { owner, error }. Failure owns the same Owner/Maintenance and
has no success or resource extraction operation. The BUSY tests require the exact
SQLite DatabaseBusy code and run SELECT 42 on the returned live connection.
A merely prepared, unfinalized statement is sufficient; it need not be currently
executing for sqlite3_close to reject it.

On success the connection slot is empty. The complete payload is consumed;
Maintenance is dropped only after that confirmed SQLite-close. A private Closed
owner keeps TempDir until the subsequent Operation test has run. This Closed type
is test-local resource bookkeeping, not a production close/restore capability.
It gives no classification, generation, handoff or Ready authority.

## Drop, failure and unwind

If Owner still has Parts, its Drop uses Box::leak on the ALREADY complete Box.
It does not drop Connection, Maintenance or TempDir. This applies both to ordinary
abandonment and to unwind, including Failure's default field destruction.
Option::take removes only the complete payload; no raw fd, ptr::read, transmute,
ManuallyDrop or mem::forget is used. No arbitrary user destructor/callback is part
of the payload. There is no fallible operation between successful payload take
and placement into the next complete owner other than an invariant assertion
whose value is established by the immediately preceding close sequence.

Permanent retention is deliberate and bounded to one child process/scenario:
- one Parts Box containing a live Connection, actual Maintenance/write guard and
  TempDir;
- in BUSY scenarios, one Box holding a real prepared Statement, intentionally
  leaked solely to make close BUSY; its Connection is retained with it;
- the outer actual DatabaseState stays alive on the child stack until explicit
  process::exit, so retained guard references are not used after state destruction.

The child NEVER returns normally through state destruction after leaking its
guard. The parent owns an enclosing TempDir and cleans up only after child exit.
OS process exit releases fd/locks and memory; nothing is claimed to survive it.
No process-lifetime resource retention occurs in the main test runner.

Panic tests inject exact &str panics before explicit close and after real BUSY
has restored the Connection. They do not prove arbitrary panics INSIDE rusqlite's
consuming close, unwinding across C, panic=abort, or panic-payloads with arbitrary
destructors. The private fixture installs no SQLite callbacks/hooks and uses an
unshared, unpoisoned connection. This scope must not be widened into a general
production panic-safety guarantee.

## Direct blocking observation

Each checkpoint checks the actual maintenance flag and failed generation.try_read.
A scoped helper thread attempts the REAL state.operation(None), followed by a
normal READ_ONLY SQLite-open if it were admitted. The existing cfg(test)
on_blocked hook runs only AFTER a real failed try_read. It sends a channel signal
and deliberately panics with an exact payload before entering blocking read().
The helper catches that exact panic and joins; no worker remains parked forever.

This proves a request hits the real Operation lock barrier rather than passing
through to open. It is not a test that leaves a thread blocked for a timed interval.
There is no correctness sleep or global shared fixture. Timeout/watchdog values
only bound broken tests. Hook state belongs to the individual DatabaseState and
is consumed on each observation; child env variables are set on Command only.

Checks run while the owner lives, after BUSY, and AFTER the actual outer Owner or
Failure has dropped/unwound. Merely retaining a borrowed Maintenance record would
fail these checks. The success control verifies Operation/open can proceed after
verified close, with generation 1 unchanged. Fixture phase stays Ready and the
recovery latch stays false; the blocking effect therefore comes from real
Maintenance, not Recovery. This is not a new Ready transition.

## Scenarios and limits

Five substantive scenarios: explicit close success; ordinary Drop retention;
pre-close panic; actual BUSY followed by Failure Drop; actual BUSY followed by
Failure unwind. A sixth libtest entry is only the subprocess entrypoint and is a
no-op outside its explicit child environment.

The positive descriptor_owner tests and preserved negative POSIX probe remain
separate and unchanged. This prototype opens no extra main File/dup and performs
no extra main-file hash/open while its SQLite connection lives. The parent opens
the fixture read-only only after child exit to verify the empty journal table.
The fixture is created before the retention episode. No RESERVED-lock claim is
made here: Maintenance is application coordination, not a SQLite kernel lock.

Not established: bounded production retention/retry policy; a real writer;
PreparedWriteEpisode/RecoverySession/reservation integration; other process-local
opens; all native OS-close errors; safe writer-VFS unregister or close proof before
classification. Owning the real Maintenance is now demonstrated in this isolated
model, but integrating borrowed production context/session chains remains a
separate architecture task. Release abort and process crashes do not run these
unwind guarantees.
