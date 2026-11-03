# Q removal: preparation and visible unlink (4d.2)

`FinalFilesVerified::prepare_q_removal(self)` consumes the existing final proof.
It returns `QRemovalPrepared` or `QRemovalFailure`. `QRemovalPrepared::unlink_q(self)`
returns `QUnlinked` or `QRemovalFailure`. All fields are private. These internal,
non-cloneable, non-serializable capabilities expose neither SQL nor drift admission.
No command, UI or production restore coordinator is connected to these methods.

## Ownership and verification

Each owner retains the complete `Retained` aggregate: establishment descriptor,
AllConnectionsClosed, Running context files where applicable, classifier files,
Q evidence with original bytes/identity/descriptor, operation bindings, session
and the same exclusive reservation. The session continues to borrow DatabaseState
and, for Running, Maintenance. DirectoryLease stays in DatabaseState. No new
reservation or OS lock is acquired; no Maintenance loan is detached.

Preparation renews full file checks, all eleven existing final-verification
barriers and the final file checks. The existing establishment descriptor is
reused. Holding a prepared object grants no immunity from later file changes:
unlink_q renews the same verification and all barriers before mutation.

Immediately before unlink the implementation rechecks files, cancellation,
Q's retained handle, fixed name, parent identity and exactly-one-link condition.
LoadedReceipt's additional narrow helper is metadata observation only. Existing
LoadedReceipt::check, Evidence and FileChecks still require named Q presence.
The only mutation is macOS unlinkat against the retained root descriptor and the
fixed .mno-restore-completion-v1.json component. Linux/Windows reject before mutation.

## Progress and errors

- QRequired: no unlink syscall attempted by this operation; Q remains required.
- UnlinkAttempted: the syscall was attempted but did not confirm success. This
  is not reported as unchanged or as a successful unlink.
- Unlinked: unlinkat returned zero. Progress is set immediately, before any
  hook, allocation or fallible post-check. A later error/cancellation retains
  the entire chain and this progress; it never recreates Q.

Failure progress is diagnostic, not authorization. Ordinary error/drop destroys
closed file/staging owners before releasing the reservation. It never changes
Recovery, Ready or generation. The OS lock stays in DatabaseState.

Consuming entry methods move the existing Option-owned aggregate directly into
the existing DuringVerification unwind guard, with no callbacks or fallible work
in between. An unexpected panic conservatively retains that aggregate using the
same pre-existing unwind retention contract: files/staging and the reservation
may remain retained until process termination. Such a panic does not issue a
success/failure capability or enable another recovery session. No live SQLite
connection or VFS callback remains at this point.

## Explicit durability boundary

QUnlinked proves a successful visible name operation only. This step deliberately
performs NO root-directory synchronization after unlink and creates NO
DiskRecoveryCompleted. The next step must synchronize the removal and perform
post-unlink verification before issuing any disk-completion proof.

Journal, protection, establishment and Q durability are reconfirmed while Q is
still named. Consequently journal safety must already hold before the last
persistent recovery signal can disappear. Late cancellation/checkpoint failure
is not evidence that the deletion did not occur.

| Crash point | Possible restarted layout | Startup contract |
| --- | --- | --- |
| Before unlink | Q remains | Recovery gate; fresh session/classification required |
| After successful unlink, before directory sync | Q absent or reappearing after storage recovery | Q present: recovery gate. Q absent: normal conservative startup checks |

Normal startup with no Q does not reconstruct this operation's outcome or a
previous process's late errors. It checks establishment, existing database/schema,
sidecars and reserved artifacts under its own DirectoryLease. No CREATE of a
missing established journal is permitted. Current-process failure always remains
Recovery, regardless of what a later startup may accept.

No promise of persistent blocking after every late error can be made once the
last disk signal is absent. No new marker is introduced in this step.

## Threat model and tests

DirectoryLease serializes MNO processes and Reservation serializes sessions.
fstatat followed by unlinkat is not atomic inode-conditional deletion; arbitrary
same-user external manipulation in the last syscall window remains outside the
accepted guarantee. No filesystem sandbox or blind rollback is added.

Tests cover Running and Startup, each pre-barrier failure, late artifact changes,
pre/post-unlink cancellation/error, retention, panic, real EACCES from unlinkat,
compiler capability/lifetime boundaries and subprocess kills before/after unlink.
The actual permission-error test is skipped only for privileged root execution.
Subprocess kills exercise process-crash visibility, not power-loss persistence.
No test treats QUnlinked or Q absence as current-process Ready authorization.
