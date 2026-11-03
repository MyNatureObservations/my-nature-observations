# Recovery ownership handoff (4e.1)

This step transfers ownership only. `DiskRecoveryCompleted::into_running_handoff`
and `into_startup_handoff` consume actual durable disk-completion evidence.
Neither result grants Ready, SQL access, a generation change or startup admission.
No production coordinator invokes these entry points yet.

Before the first split, shared after-unlink FileChecks reverify Recovery, the
session/reservation and lease, origin/operation/generation, directory and journal
identities, full journal SHA-256, protection archive identity/hash, establishment
marker identity/bytes, and absence of Q/P/recovery, sidecars and reserved artifacts.
Running additionally requires the identical live Maintenance guard; Startup must
have actual Startup origin. A final recheck follows the preparation checkpoint.

The private stage owner consumes the disk wrapper, QUnlinked, retained marker,
AllConnectionsClosed and ResourcesClosed through their Option-owned units.
ContextConnectionsClosed projects its already-closed resources, converts its
ProtectionRecord and ContextSessionBinding to inert records, and ends all
Maintenance loans. Closed classifier resources are separated from Access::Session.
The final step consumes that same session, preserving its actual UUID and moving
its original Reservation into a state-borrowing detached session. No new UUID,
reservation, OS lock or Arc for reservation ownership is introduced.

Files and marker precede the detached session in field/destruction order. The
original context/classifier resource order is preserved. The original DirectoryLease
and Startup RecoveryGate stay in DatabaseState. The state lifetime remains borrowed;
the Running Maintenance lifetime disappears only from a successful handoff. An
error retains its actual stage with both original lifetimes conservatively.

Preparation errors retain the original disk capability. Errors at later stage
boundaries retain the whole current aggregate. Unexpected unwind uses the existing
conservative policy to retain the aggregate rather than run staging cleanup during
the operation. Ordinary result Drop cleans only already-closed resources, then
releases the session slot. Neither Drop path grants Ready or changes generation.
The test checkpoints are absent from release; production has no supplied callback
or arbitrary destructor between a stage take and its replacement.

The detached Maintenance address is equality-only, never dereferenced. Together
with state identity, original operation UUID and generation it is an inert record,
not an authorization. The original guard must remain with the coordinator; later
admission will need fresh checks and must not rely on an address alone.

The outer with_restore_context HRTB is unchanged and prevents state-borrowing
results escaping. Internal with_active fixes the state lifetime, so its coordinator
can receive the handoff, end immutable Maintenance loans and borrow its original
guard mutably. The existing callback compiler probes remain required.

Future work must add fresh post-transfer file verification and separate Running
and Startup admission. Historical disk evidence cannot alone authorize Ready.
The retained Inputs, OperationBinding, closed context records, Q descriptor/bytes
and establishment marker remain available to a future private verifier. Current
QUnlinked-based verification is used before extraction; no weaker verifier or
boolean missing-Q bypass is introduced here. Arbitrary same-user external changes
between checks remain outside the existing threat-model guarantee. macOS is the
only recovery platform; Linux/Windows reject before transfer.
