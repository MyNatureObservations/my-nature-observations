# Terminal guard — test-only prototype (5d.3v)

## Isolation and ownership

All code is beneath active_close's existing cfg(all(test, target_os = "macos"))
boundary. No production API, writer, VFS, journal mutation or Ready path changes.
The existing 3r outer/prepared helpers are reused with sibling-only visibility.
They create real DatabaseState, original Maintenance (owned by the unchanged
3p Owner), real RestoreContext/A/B/C/protection, durable P, RecoverySession and
reservation, checked A-close and PreparedWriteEpisode. No substitute guard or
restore-authority constructor is used.

TerminalEpisode<'m,'s> privately owns an in-memory Connection and the entire real
PreparedWriteEpisode borrowing that outer Maintenance. The outer Owner also keeps
its original read-only connection. State owns DirectoryLease/process lock. No
extra main descriptor is opened or closed during any active writer lock: there
is no writer or RESERVED-lock claim. No resource extraction API is provided.

## Arming and checked close

TerminalEpisode is armed at private construction, before consuming Connection.
Its Drop checks private State; if Armed it emits the test observation and invokes
std::process::abort() BEFORE automatic field destruction. The ordinary Rust
field-drop sequence cannot destroy its Connection/prepared chain on this branch.
The borrowed Maintenance is still owned outside this inner scope.

close takes the Connection only while this owner is already armed. Actual
Connection::close OK is the only arm that changes State to Closed. It then
returns the whole PreparedWriteEpisode, never individual resources or an authority
proof. The boolean argument is only the test's panic-after-OK injection; it cannot
turn an error into success or disarm a live Connection. There are no Clone/Copy/
Serde implementations or public constructors/reset methods.

On Err the exact returned Connection is restored before any observation. The
armed episode and SQLite error are returned together. The BUSY scenario requires
actual DatabaseBusy, queries SELECT 42 through the retained live Connection and
observes the actual Operation lock barrier and reservation Busy under the same
outer Maintenance. Drop of this failure episode aborts. It never silently releases
its resources or returns a close-success proof.

The sole success path checks M still blocks after inner close, drops the returned
never-active PreparedWriteEpisode, then explicitly closes the original outer
owner connection. Only then is M released. A subsequent Maintenance can be
acquired; Recovery remains active and generation remains 1. This is not a writer
close ledger or production admission.

## Panics and terminal outcomes

The existing 3t rollback-hook capture/Drop mechanism is factored into a sibling
helper and reused without changing its mechanism. It panics on capture removal
INSIDE rusqlite 0.40.2 Connection::close. The original 3t caught-panic/exit-73 test
remains separately green. Here the inner TerminalEpisode unwinds to its Drop,
which aborts instead of returning to normal operation. No catch attempts to
recover or reuse the lost Connection.

Other terminal cases are ordinary Drop without close, Drop after actual BUSY,
and an exact test panic after observed close OK but BEFORE disarming. Each remains
armed until its Drop aborts. The after-OK case is explicitly a wrapper panic, not
mislabelled as the internal hook panic.

Important boundary: guard Drop cannot prevent Connection/InnerConnection
DESTRUCTORS ALREADY RUN inside rusqlite during unwind. It does not recover the
consumed Connection and does not certify its native final state. Abort only stops
execution from the abort point. This tests unwind-build behavior at selected
points, not all internal panics, arbitrary panic-payload destructors, FFI unwind
or shipping panic=abort behavior. No production fail-stop policy is installed.

## Subprocess evidence and Recovery

All scenarios run in isolated children with private TMPDIR, output discarded,
30-second watchdog and kill/wait on timeout. Child environment is set on Command,
not global process environment. Only the child entrypoint is a no-op in parent
execution. The two parent tests run five substantive scenarios; three libtest
entries are not three independent full scenarios.

Parent requires exact ordered event-file contents. OK must exit 0; all four
terminal scenarios must report Unix SIGABRT, not simply a non-zero status. The
internal case additionally requires the real hook-destructor marker. The expected
sequence forbids disarm or safe-return on error and forbids any continuation after
terminal Drop. Timeout, ordinary test panic, unrelated abort without guard marker,
normal return and wrong ordering fail. SIGABRT alone is insufficient evidence.
The final test-only guard marker is emitted immediately before abort; it has no
arbitrary callback or assertion after it. Event I/O is TEST instrumentation only,
not a proposed allocation-/I/O-free production abort implementation. It can fail
and then parent rejects the sequence. This is not a proof of every library Drop.

Before the dangerous call, actual durable P is published via the reused real
chain and exact expected bytes are copied outside the managed data directory.
After each child finishes (including OK), parent verifies exact P bytes and real
startup::begin returns RecoveryGate with database_recovery_artifact_present.
Canonical containment handles macOS /var aliases. Parent cleanup runs after the
gate drops and child exits. Existing startup code gives the pre-open blocking
property; no fresh SQLite-open counter is introduced. No simplified marker,
automatic recovery, classification or Ready authorization is created.

Permanent retention of one unfinished Statement in BUSY uses the existing 3p
Box::leak technique solely to provoke actual close BUSY. The armed owner retains
its Connection and the actual outer guard through the observed failure; process
abort releases them via OS teardown. No retention persists in the test runner or
survives process death. No strömavbrott/power-loss durability proof follows.

## Remaining production work

The test model has no active writer, complete production connection ledger,
backup_finish contract, VFS-unregister path, process-wide descriptor/open barrier
or closed-episode classification port. It does not modify with_active or its HRTB.
The outer Maintenance owner and inner terminal policy would need separate review
before production integration, including build panic policy. POSIX-lock safety
and the negative descriptor-close probe remain separate concerns.
