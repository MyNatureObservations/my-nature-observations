# Internal SQLite close panic — test-only (5d.3t)

The new child module is below active_close's existing
`cfg(all(test, target_os = "macos"))` boundary. No production code/API, writer,
VFS or user-journal mutation is added. rusqlite is locked at 0.40.2 with hooks;
libsqlite3-sys is 0.38.2. The actual close/remove_hooks source was inspected.

## Mechanism

The child uses a real `Connection::open_in_memory()`. A public rollback hook
captures `PanicOnRemoval`. Its callback only borrows the capture and is never
invoked by the experiment. The capture's destructor writes an observation marker
and panics on its first destruction. `Connection::close(self)` calls
`InnerConnection::close`, whose `remove_hooks` replaces the stored rollback hook.
That replacement destroys this capture on the actual library close path, before
`sqlite3_close`. No test callback before/after close substitutes for this panic.

The parent requires the before-close marker, destructor marker, exact caught
panic payload, caught marker, no returned-Result marker, and child exit code 73.
The atomic count avoids an intentional second panic if a library Drop retry
reaches the destructor again; this is not an arbitrary nested-panic experiment.
The parent runs with a 30-second watchdog, kill/wait on timeout and private child
TMPDIR. Output pipes cannot fill because child stdout/stderr are discarded.

## Observed outcome and terminal policy

In the current debug/unwind test build, the internal destructor panic is caught.
`close` does not return a Result/Connection to the caller. Library unwind may
activate Connection/InnerConnection destruction, including a close retry. The
experiment does not instrument or certify every such internal Drop, native
handle closure or remaining native state. It never reconstructs/reuses the lost
Connection or calls another database operation in the child after the panic.

After fixed test observations the child immediately calls `process::exit(73)`;
no classification, Ready or normal-work path follows. This is a terminal TEST
model, not an installed production abort guard. It deliberately does not run
payload, durable-owner or Maintenance destructors on the terminal branch.
Maintenance was acquired outside the catch and remains lexically owned there;
no general production retention after internal close panic is claimed.

A direct abort is NOT the observed outcome of this particular experiment.
Unexpected signal/abort or normal return fails the precise regression test and
requires reassessment. Release `panic=abort`, FFI boundary violations, secondary
payload destructor panics and different panic sites can terminate before catch;
this test does not generalize to those cases.

## Actual durable P and restart

Before the dangerous in-memory close, the existing real fixture/context helper
creates real A/B/C/protection and publishes P through the existing authorization
and mandatory durability barriers. `DurableWritePreparation` remains owned on
the child stack, along with actual Maintenance/state. No fake P is written.
Only the expected-byte comparison copy and observation files are test artifacts
outside the managed data root. The child reports its root; the parent validates
its canonical containment (macOS /var and /private/var aliases are permitted).

After child exit the parent checks exact P bytes, then calls actual startup::begin
on that managed root. It requires RecoveryGate with
`database_recovery_artifact_present`, not Ready. Existing early artifact checks
provide the no-normal-SQLite-open property; this test observes the real outcome,
not a newly added SQLite-open counter. Parent cleanup occurs after the gate drops.
This is a process-exit/restart test, not a power-loss experiment.

The real journal is only initialized by the existing private fixture. No writer
or restore-copy is opened. The dangerous connection is in memory, so no new main
file descriptor is opened/closed under a SQLite writer lock. This establishes no
POSIX writer-lock guarantee.

## Controls and limits

The existing maintenance_retention group separately exercises real close BUSY,
returned live Connection, explicit close success and wrapper panics before close
and after BUSY. These are not relabelled as internal panics. No standalone close
proof, classification, production retention mechanism or Ready path is created.
The child entry is a no-op in ordinary parent execution; two libtest entries
therefore contain one substantive subprocess scenario.

Remaining work: review this observation independently; separately design a
production terminal guard and actual outer Maintenance ownership before active
writer integration. Control all extra descriptors, checked backup_finish,
SQLite-close and VFS lifetime first. No production guarantee follows merely from
this test's exit code or from catching the injected panic.
