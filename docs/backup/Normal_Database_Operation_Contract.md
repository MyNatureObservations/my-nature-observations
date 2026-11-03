# Normal database Operation contract (5d.3n)

## Current call-site contract

Normal journal reads/saves acquire a real DatabaseState::Operation BEFORE
SQLite open. They keep that permit while the local Connection and its
statements/iterators are used and destroyed. Connections must not be returned,
extracted or otherwise retained beyond the permit. Declare the Connection after
the Operation and statement/query owners after the Connection, preserving reverse
local destruction order on return, errors and unwind.

The six journal production call sites use database::open_database; its signature
still returns a standalone rusqlite::Connection. The contract is therefore a
reviewed call-order rule, NOT a type/lifetime guarantee of that helper. A new
caller must not assume the helper acquires Operation or enforces correct state.
No typed wrapper or new opening route is introduced by this step.

Maintenance reserves exclusivity and takes the generation RwLock for writing.
It waits for existing Operations holding read guards. Reserved Maintenance rejects
new operations when they reach admission; an active write guard can cause queued
operation() calls to wait. After that wait admission rechecks generation, phase,
Recovery and maintenance. 'Blocked' does not universally mean immediate error.
Old autosave payloads retain their original generation and are not relabelled.

Manual backup has its own READ_ONLY source-open helper. It acquires Operation
AFTER the destination dialog and BEFORE source open, keeps it through the backup,
and destroys the source before the permit. The blocking worker retains the
operation even if the requester stops awaiting it. This path does not call the
normal READ_WRITE open_database helper and is not changed here.

## Close and scope limitations

Connection destruction before Operation destruction is not a checked SQLite-close
proof. The bundled rusqlite attempts sqlite3_close during implicit destruction
but does not propagate that Drop error. No claim is made here about hidden close
failures, leaked statements/raw handles or every native/kernel close result.

A future active writer must explicitly handle transaction/backup finish, close
BUSY/errors, dependent descriptor retention and private VFS unregister. It may not
ignore close errors and then release permits or extra main-file descriptors.
POSIX-lock safety additionally requires control of every relevant same-inode fd,
including hash/sync helpers and rejected alias opens, plus external process tests.
Normal Operation call order does not establish that process-wide guarantee.

Startup/create/schema validation occurs under its separate DirectoryLease and
startup contract before Ready. Recovery A/B/C, BoundSqlite, completion and admission
have separate resource proofs and close contracts. They must not be converted to
normal Ready-Operation acquisition simply to reuse this documentation.

## Regression evidence and precision

New journal::normal_open_contract_tests use the REAL save_in_session function,
real DatabaseState and private SQLite fixtures. Its existing injectable open
closure uses the same READ_WRITE-without-CREATE flags as normal production open;
it does not instantiate a Tauri AppHandle or directly exercise open_database's
DataLocation lookup/timeout setup.

A cfg(test)-only DatabaseState observation asserts that no Maintenance is active
and try_write fails while the single-threaded fixture is inside the open closure.
The only read guard in that fixture is the Operation acquired by save_in_session.
This observes the permit before open and during a real SQL query; removing that
permit acquisition makes the assertions fail. It is not an fd tracer or a
verified close observation.

Three new tests cover:
- permit held before/after successful real open, actual save result, then available
  Maintenance after save_in_session returns;
- actual SQLITE_CANTOPEN on a missing file without CREATE, exact injected error,
  reached open closure, missing file remains absent, permit released afterwards;
- exact panic after successful SQLite open/query, caught unwind and subsequent
  Maintenance availability. Local Connection precedes outer Operation destruction
  by inspected code structure, not by an assertion that close can never fail.

Existing tests are reused rather than copied:
- database_state::tests::maintenance_waits_for_operations_and_reserves_exclusivity
  exercises real SQLite transaction, Maintenance waiting and exclusive write guard;
- queued_save_checks_generation_after_maintenance_releases_lock verifies a queued
  operation rechecks generation; reservation_rejects_new_operations_and_cancel_preserves_generation
  covers reserved maintenance admission;
- journal tests for stale generations and queued real save assert the open closure
  is not called after invalidation; startup_gate_tests block open before startup;
- manual_backup tests cover concurrent backup starts, dialog cancellation under
  Maintenance, startup blocking, relevant synchronization conditions and
  established startup/save/verified backup.

The property that an ongoing worker retains its database Operation even when
its frontend requester stops awaiting it is supported by code review, but is
not directly regression-tested by these tests.

No compiler probe is added: no new type boundary is introduced and the existing
free Connection return cannot truthfully be claimed to prevent escape statically.
The call-order guarantee remains a combination of code review and runtime
regression observations, not stronger restore authorization.
