# Private terminal policy link (5d.4i)

RestoreMaintenanceScope still owns the original Maintenance. It now also owns a
private TerminalPolicy latch. with_active lends that same latch and Maintenance
to RestoreContext under the existing HRTB. PendingWrite retains the context;
close_active copies only its borrowed policy reference into the existing
CloseOwner before transferring any resource. ActiveCloseParts/LiveSources retain
exactly the original A/B/C/P, bindings, session and reservation. State continues
to own DirectoryLease and the process lock. No parallel resource owner is added.

The internal phase distinguishes NeverArmed, Armed and reserved Finished. There
is NO production arming path and NO reset/disarm or success constructor. Finished
is intentionally unreachable until a real checked close producer is designed.
The latch contains phase metadata only; no Clone/Copy of authority or owned
resource is introduced. Some direct historical test fixtures use None, since
they borrow Maintenance without a RestoreMaintenanceScope; production
with_active always installs Some(&scope.terminal_policy). New integration tests
use that actual scope, same Maintenance and actual complete restore resources.
The optional fixture representation grants no production alternative factory.

CloseOwner::Drop first reads the latch and immediately calls process::abort if
Armed, before its before/after payload fields can drop. NeverArmed retains the
existing normal Drop and whole-owner unwind retention. Scope::Drop likewise
aborts while Armed before its real Maintenance field drops. Neither abort path
performs I/O, allocation, callbacks, SQLite operations or fallible RefCell loans.
The latch uses Cell and is not a cross-thread synchronization primitive. No
cross-thread sharing or process-wide OS-open barrier is claimed.

Only cfg(all(test, target_os="macos")) exposes the private arm_for_test method.
The production path cannot yet activate this policy. Controlled error returns do
not alter phase; nor does existing A-close success. No new B/C-close call,
writer, backup_finish, close proof, classification gate or Ready path is added.
Normal never-armed acquisition/release ordering and Maintenance's existing unwind
poisoning remain unchanged. The borrowed reference's Drop performs no action.

Tests execute the actual production CloseOwner and RestoreMaintenanceScope, not
a replacement terminal owner. An integration parent runs four subprocess modes:
inner armed Drop, actual controlled AfterClose checkpoint error with retained
owner, inner unwind, and outer armed Drop. Each must produce the exact pre-Drop
observation sequence and SIGABRT within a watchdog. In inner cases a real B hook
capture must NOT be destroyed; this witnesses abort before B's field destructor.
Outer mode first safely drops the never-armed inner chain, confirms Maintenance
still excludes Operation, then arms/drops the actual outer scope. The outer
pre-Maintenance-Drop order additionally follows directly from the actual custom
Drop body; no new Maintenance destructor instrumentation is installed. Test I/O
is outside the production abort path. These probes do not directly observe every
field destructor or distinguish every imaginable unrelated signal source.

All modes publish real durable P and create a real RecoverySession before the
terminal point. Parents require unchanged exact P bytes and actual startup
RecoveryGate after process exit. The controlled-error case requires reaching the
injected AfterClose checkpoint and the exact Checkpoint error, retained session,
binding and resources. It is not presented as a new B-close/BUSY implementation.
Existing BUSY and terminal-prototype regression tests remain relevant separately.
The normal integration test uses the linked never-armed whole chain and verifies
inner cleanup does not release Maintenance, journal bytes/generation/Recovery
remain unchanged, and Maintenance releases at the outer boundary. Existing
context unwind tests verify unchanged never-armed poisoning/release semantics.
Compiler harnesses project the actual policy definitions together with actual
owner definitions; they do not substitute a fake policy implementation.

A guard cannot recover a Connection consumed by rusqlite::close(self), nor stop
library destructors already run before its own Drop during unwind. This step
therefore does NOT make active B/C-close safe. Cargo.toml release specifies
panic="abort"; no delivered artifact was inspected. No project set_hook/take_hook
installation was found in the reviewed source; library/default hooks and future
hooks still require review. Before future activation, effective production panic
strategy, hook policy, actual B/C/writer progress, explicit close/backup/VFS
results and POSIX descriptor ordering need separate approval. Unwind tests are
not proof of release panic=abort or power-loss durability. Memory locks end at
process death; retained P, not leaked memory, blocks next startup.
