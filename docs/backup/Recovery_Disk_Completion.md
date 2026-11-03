# Durable disk completion after Q unlink (4d.3)

## Capability chain and ownership

`QUnlinked::complete_disk(self)` consumes the proof of a confirmed successful
unlink and returns `DiskRecoveryCompleted<'maintenance, 'state>` or
`DiskCompletionFailure<'maintenance, 'state>`. Both retain the complete original
QUnlinked owner: closed context/classifier resources, establishment marker,
journal/archive/directory descriptors, exact Q bytes and descriptor, operation
bindings, RecoverySession and the same exclusive Reservation. DirectoryLease
stays owned by DatabaseState. Running keeps its Maintenance loan; Startup has no
Maintenance. No new lock, reservation, identifier or SQLite connection is created.

Constructors and fields are private. No Clone, Copy, Serde, SQL accessor, generation
method, Ready method or failure-to-success conversion is provided. This capability
is disk evidence only. No application command or restore coordinator calls it.

## Private after-unlink verification

Existing named-Q checks remain strict and unchanged in authority. Their common
verification backend now has a private borrowed mode whose unlinked variant
requires an actual QUnlinked reference, never an application-controlled boolean.

The after-unlink FileChecks entry derives its owner and establishment descriptor
from that proof. LoadedReceipt requires reference identity with the exact receipt
still owned by that same QUnlinked value. No pointer is stored across moves.
Retained Q identity, bounded exact bytes, strict format/pending binding and zero
link count are checked without resolving the original Q name. Root/database
binding remains checked. Evidence separately requires Q, P and recovery absence;
returned names, aliases, symlinks and unknown reserved artifacts fail closed.

The shared backend still verifies current directory/database/archive identities,
full physical SQLite SHA-256, archive SHA-256, operation/session/generation,
establishment identity/content, sidecars and reserved artifact policy. It never
reopens SQLite or substitutes a logical content fingerprint for the full file hash.

## Required operation order

1. Require macOS; verify all after-unlink invariants.
2. Recheck immediately before syncing the retained data-directory descriptor.
3. `fsync` that directory; record DirectorySynced only on success.
4. Required FinalFileBarrier on the retained still-linked journal, protection
   archive and establishment marker, in that order. macOS uses F_FULLFSYNC.
5. Record FinalBarriersSynced only after all three succeed.
6. Verify the entire after-unlink state again; record FilesVerified.
7. Run the last boundary and repeat verification before constructing success.

The old unlinked Q descriptor is retained as historical evidence but is NEVER
synced as proof that its name was removed. Its parent directory must be synced.
All sync and metadata errors propagate; no best-effort fallback exists.

## Failure progress and cancellation

4d.2 continues to distinguish QRequired, UnlinkAttempted and Unlinked.
4d.3 cannot start from the first two states; its failure progress starts at
Unlinked, then DirectorySynced, FinalBarriersSynced and FilesVerified. Progress
records a completed historical step, not current admission authority. For example,
a returned artifact after FilesVerified still causes failure, not success.
An error inside the final-file group reports DirectorySynced until every required
file barrier has succeeded. No late failure regresses to a pre-unlink state.

Cancellation is fail-closed and may stop an incomplete completion operation.
Q may already be absent. No path recreates it, rolls back, changes generation or
releases Recovery. A subsequent startup cannot reconstruct an earlier process's
late errors when no recovery artifact remains; this accepted protocol limitation
requires journal safety to have been established before Q unlink.

## Panic and Drop

A single Option-owned QUnlinked aggregate is installed in DuringDisk before any
fallible operation or hook. On unexpected unwind it uses the existing conservative
retention policy: closed file/staging owners and the reservation can remain
retained until process termination, without accessing leaked borrows. No partial
projection, transmute, ptr::read or unsafe lifetime extension is used.
Ordinary success/failure Drop destroys already-closed resources/staging before
releasing the session reservation. Process locking remains DatabaseState's job.
Neither ordinary Drop nor Drop during unwinding changes phase or generation.

## Crash matrix and startup

| Interrupted point | Process-crash observation | Next startup |
| --- | --- | --- |
| After unlink, before directory fsync | Q absent on the running filesystem; storage recovery may differ | Q present: Recovery. Q absent: normal strict startup checks |
| After directory fsync | No success proof yet; final barriers still required | Same conservative artifact decision |
| During final-file group | Some file barriers may have completed; no completed capability | Same conservative artifact decision |
| After final barriers, before verification | Durability calls succeeded; final state not yet approved | Same conservative artifact decision |
| After DiskRecoveryCompleted, before Ready | Required barriers and checks succeeded; current process remains Recovery | Normal startup if no artifacts; any returned artifact blocks |

Subprocess tests stop at these real production boundaries, kill/reap the process,
and test a fresh startup. A separate test-only historical-Q layout variant checks
that Q presence always blocks. That variant is NOT an assertion that Q can replay
after successful final barriers. Process-kill tests do not prove power-loss behavior.

## Tests and limitations

Tests cover Running/Startup, retained inode identities and sync ordering, each
barrier error, late failures/cancellation, file replacement/content changes,
returned Q/P/recovery, sidecars/reserved aliases, original unlinked-Q tampering,
foreign receipt/proof rejection, root-directory replacement, panic inside sync,
normal/unwinding Drop and compiler capability/lifetime boundaries.

DirectoryLease coordinates MNO processes; Reservation coordinates recovery
sessions. Arbitrary same-user external manipulation between final checks remains
outside the accepted guarantee. Real storage/device power-loss behavior needs
separate platform/filesystem testing. Linux and Windows remain Unsupported and
have no mutation fallback. No generation change, Ready transition, production
restore, automatic SQLite recovery or UI is implemented.
