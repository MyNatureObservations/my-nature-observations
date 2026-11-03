# Borrowed restore-chain integration — test-only (5d.3r)

## Construction and ownership

All new runtime code is below the existing macOS cfg(test) boundary. No production
signature, state transition, open path or restore authority is added.

The prototype reuses database_state::maintenance_retention::Owner from 5d.3p.
Only test-code visibility is widened; a short `maintenance(&self)` borrow is added.
Owner still owns the REAL Maintenance/write guard, its independent READ_ONLY
SQLite connection and auxiliary TempDir. It exposes no Connection/File extraction.
Its close and whole-Box Drop retention are unchanged.

The real existing pending::tests::Fixture initializes a managed DatabaseState,
database and fixture content before Maintenance. The outer Owner is constructed
before any restore-chain borrow. Existing context(f,m) creates real incoming B,
ActiveDatabase A, verified protection C and attaches the real bound protection.
A source backup uses SQLite Online Backup to a private snapshot/archive, never to
the managed main database. This is existing candidate/protection preparation,
not a writer or productionrestore path.

The genuine chain is:
DatabaseState -> outer Owner(Maintenance) -> borrowed RestoreContext
-> real authorized durable P -> real RecoverySession/reservation
-> PendingWrite -> actual separate A-close -> WriteInputs
-> WriteEpochInputs -> actual preflight -> PreparedWriteEpisode.

The helper checks identical Maintenance references, real session ID, operation,
generation and original ContextSessionBinding. The historical borrowed view is
scoped and discarded before later consumption. No mock session or Maintenance,
self-referential owner, unsafe or lifetime extension is used.

DirectoryLease/processlock stay owned by the real fixture DatabaseState. P and
its held files/bytes, live B/C/staging and the original session remain owned by
the unchanged production aggregates. The independent outer READ_ONLY connection
exists only to reuse the 5d.3p owner's actual close/retention contract. It is not
claimed to be a production writer or part of production ContextConnectionsClosed.

## Scenarios and destruction

Normal: scoped context reference and EpochVerificationView end while outer M
still blocks Operation. After full preparation, ordinary Drop of the never-active
inner owner closes B/C safely and removes staging; durable P remains. Real M
continues blocking. Only explicit close of the outer owner's own read-only
connection releases that M. A new Maintenance can then be acquired; the existing
P-induced Recovery latch is still active and generation is unchanged.

Error before construction: a local exact test error is observed before creating
context/P; it only demonstrates outer scope ordering, not a production failure
injection. Error during construction: the existing BeforeAuthorization checkpoint
inside publish_write_preparation_inner is reached, requires the precise Checkpoint
error, and returns the whole production failure owner. Its ordinary Drop cleans
never-activated staging; no P has been published and outer M still blocks.

Inner unwind: test panic consumes PreparedWriteEpisode inside catch_unwind while
outer Owner stays outside. The existing CloseOwner conservatively retains the
whole inner aggregate. B/C/P and reservation Busy are verified afterwards. Then
the actual outer Owner drops; its EXISTING whole-Box retention keeps actual M.

Entire-scope unwind: outer Owner is created INSIDE catch_unwind, followed by the
real borrowed chain. Stack unwind processes inner before outer. Both existing
guards retain their complete aggregates. Exact panic payload, retained B/C/P
and real Operation lock failure are checked after both destructors ran.

No new general retention mechanism is introduced. All scenarios run in child
processes; parent supplies a private TMPDIR via Command only, never global set_var.
Fixture paths must be below it. The fixture/state stays on the child stack until
process::exit so no leaked M guard outlives its backing state in a continuing
process. Parent cleanup happens after exit. A watchdog bounds broken child tests;
its polling sleep is not a correctness/ordering condition.

## Operation observation and safety boundaries

The existing assert_exclusive helper checks the actual maintenance flag and
failed generation.try_read. A real operation(None) attempt reaches the existing
on_blocked hook only after lock acquisition fails. The exact hook panic is caught
and helper thread joins. This is a direct observation of the real guard even when
Recovery is also active, not merely a recovery-admission error or a timed sleep.

The test does NOT keep Recovery/Ready unchanged from the initial fixture by
fabricating states: authentic P publication/session acquisition uses their
EXISTING Recovery transition. No new transition, reset, generation change or
Ready authorization is introduced. Early errors occur before P. After P, Recovery
continues even after safe release of the outer M on the normal path.

The normal preflight still opens/closes its documented temporary resources and
uses B/C source helpers. No active writer episode exists. Main-file bytes are
compared before/after each scenario, not as an assertion of kernel-lock safety.
The model does not prove exclusion of arbitrary same-inode opens, safe writable
VFS, writer-close progress, internal rusqlite-close panic or production fail-stop.

Internal close-panic remains the 5d.3q open decision: Connection::close(self)
consumes Connection, so catch_unwind cannot promise to recover it. No panic is
injected inside close or FFI here. Panics use exact &str test payloads, not arbitrary
panic-payload destructors.

## Static versus runtime evidence

Runtime tests execute actual production resource construction and consuming
methods using existing private fixture helpers. They do not call the production
AppHandle/with_restore_context entrypoint, change its signature or bypass it in
production. Existing HRTB compiler probes remain in force.

Three negative probes are added to the existing actual-type projection: moving
Maintenance while PreparedWriteEpisode is still used, mutable borrowing before
that chain ends, and extending its Maintenance lifetime to static. The harness
has positive controls and stubs I/O/adjacent backends, including a producer with
the same Maintenance-to-context lifetime shape. These are static lifetime proofs,
not evidence of resource I/O/session validation; runtime tests provide the latter.

Four new libtest entries cover five child scenarios (normal, early error,
construction error, inner unwind, entire-scope unwind). One entry is the child
entrypoint and is a no-op in ordinary parent execution. No standalone write or
close-completion proof, alternate restore constructor or new reservation is minted.
