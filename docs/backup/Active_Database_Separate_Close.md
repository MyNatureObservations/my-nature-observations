# Separate ActiveDatabase close (5d.2)

## Scope and ownership

`PendingWrite::close_active(self)` is a crate-private consuming transition to
`WriteInputs<'maintenance, 'state>`. It grants no writable target, SQL accessor,
copy, cleanup or admission authority. The caller cannot construct or duplicate
its fields. Both original lifetimes remain; the outer Maintenance and
DatabaseState retain their existing ownership. There is no new reservation,
DirectoryLease, OS lock or SQLite open.

Before: PendingWrite -> WriteOwner -> RestoreContext(A, B, C), OwnedPending,
ContextSessionBinding, RecoverySession -> Reservation. DirectoryLease remains
in DatabaseState. A owns a read-only Connection and retained ActiveFiles;
B is incoming PreparedRestore, C is VerifiedProtection with its PreparedRestore
candidate and protection binding. Each PreparedRestore owns its read snapshot,
staging and identity/hash evidence. P owns the original retained root, recovery
and marker files, exact bytes and captured identities; it no longer borrows A.

After: WriteInputs -> CloseOwner -> ActiveCloseParts(closed ActiveFiles, live B,
live C, Maintenance borrow, generation, original P, binding, original session).
The session is the last resource field, as in the previous aggregate. Normal
Drop therefore handles A/B/C and P before releasing the reservation. B/C remain
open until their owner is dropped; their existing controlled PreparedRestore
Drop prevents staging removal before confirmed close. The protected archive
itself is not staging and is not removed.

## Checks and close

Before any split: check the real retained session against its existing binding,
verify original P and its context/operation/generation/files, and verify both
B/C sources. The contiguous ownership move contains no callback, allocation,
fallible operation or unsafe code. Test checkpoints run only once a complete
old or new aggregate owns all resources.

Only A's existing `ActiveDatabase::close_inner` is called. It uses
`Connection::close` (`sqlite3_close`), not close_v2 as proof. Outstanding SQLite
statements/BLOB/backup objects prevent successful close; real SQLITE_BUSY is an
error, not success. On error the original Connection and ActiveFiles are put
back into A; no WriteInputs is issued. On success only A's retained files replace
the now-empty A owner. A file-only after-check verifies its identity/name, P's
retained and named identities and exact bytes, session/binding/generation, and
still-live B/C sources. After-check failure retains the closed progress rather
than inventing an open connection.

A currently uses SQLite's process-global native default VFS. It has **no owned
RegisteredVfs**. There is therefore no A-specific unregister operation or error
to test; unregistering the global VFS would be incorrect. The later managed
write-target stage must own and explicitly unregister its separate VFS after
verified close. Existing BoundSqlite registration/shutdown is unchanged.

## Failure, Drop and panic

`ActiveCloseFailure` owns CloseOwner, so both pre-split and post-split errors
retain all remaining resources. Its error accessor gives no close/write proof.
On ordinary Drop the original safe close/retention implementations apply. A
failed close can be retried by A's Drop; success releases its dependencies, a
repeated failure conservatively retains the Connection and files. B/C retain
staging if their own close fails. Recovery stays set and the state-owned lease
remains. Ordinary owner Drop may release the reservation only after dependent
owners have been handled; it does not mean the failed operation succeeded.

During unwind CloseOwner retains the entire remaining aggregate, including the
reservation. Existing A close guard retains file dependencies if close panics.
This is intentionally conservative process-lifetime retention. Release panic
abort is not recoverable unwind: existing P/startup blocking remains essential.
No Drop changes generation or activates Ready.

## Tests and limits

Runtime tests use real durable P publication, actual RecoverySession and the
existing close seam. They require exact intended checkpoints/errors. They cover
successful A-only close, real outstanding-statement BUSY, injected non-BUSY
SQLite error, failure/panic at ownership boundaries and inside A close, changed
P rejection before close, retained B/C, original session/Busy, original journal
and P bytes, staging cleanup before reservation release, and process lock/
Recovery/generation invariants. Compiler probes project the actual production
owners/factory bodies, stubbing only I/O checks and adjacent dependencies; a
positive control must compile before negative diagnostics are accepted.

Confirmed sqlite3_close is proof of SQLite connection shutdown, not independent
proof of every native VFS/kernel close result or new disk durability. SQLite
can suppress native close errors. No writable-target durability claim is made.
External same-user filesystem manipulation remains outside the existing
cooperating-process threat model; detected changes reject conservatively.
Linux/Windows recovery support is not expanded. The public production restore
coordinator and writer VFS remain unimplemented.
