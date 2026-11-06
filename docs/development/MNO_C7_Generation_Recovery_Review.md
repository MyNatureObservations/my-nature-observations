# C7 generation recovery: review and proposed stages

Status: proposal for review, not authorization or implementation of C7 behavior.
Reviewed branch: `codex/cross-platform-ci`, commit
`7308677c9951c33f0f837b3ca6037c557328dc32`.
[C6 acceptance and limitations](testing.md#c6-manual-windows-11-acceptance).

## Recommendation and evidence boundary

Use C4's single versioned authoritative selection record, with `pending` and
`admitted` phases. Retain previous generations until explicit user deletion.
Do not add C2/C3 history chains, `authority.json`, a custom VFS or custom locking.
The accepted limitation remains: replaying an older valid record can select an
older retained journal. A single record cannot detect rollback of itself. Missing
or corrupt state is different: never infer the selected journal by scanning for
an older database, choose the newest folder, or create an empty journal.

C1 verified the existing backup-to-candidate preparation boundary on all three
platforms. C2/C3 supplied lease/admission lessons; C4 is the selection model to
carry forward. C4 tests synthetic databases, not production schema/content or
hostile pathname replacement. Its Windows directory barriers still return
Unsupported; its modeled barriers are not evidence of real persistence. C5c and
C6 establish Windows first initialization and limited application acceptance,
not production generation selection or restoration.

## Current restrictions and reuse

Paths below are repository-relative. Locations identify current functions/guards,
not a proposal to remove whole platform guards.

| Area | Current code and restriction | Reuse / required change |
| --- | --- | --- |
| Restore commands | [restore_app.rs](../../app/src-tauri/src/restore_app.rs): `validate_selection` (56), `activate_selection` (86), non-macOS branches in `choose_restore_backup` (139) and `restore_selected_backup` (167) | Reuse picker, one-use token, busy guard and restart interaction. Current validation opens a Ready old DB: unsuitable for a damaged journal. Replace only the approved command route after kernel verification. |
| Recovery ownership | [database_state/recovery_session.rs](../../app/src-tauri/src/database_state/recovery_session.rs): `startup` (20), `running` (31) reject non-macOS | Reuse the one `DatabaseState`, operation/maintenance exclusion and root lease. Do not globally enable the legacy macOS recovery proof factories. |
| Legacy activation | [restore_context.rs](../../app/src-tauri/src/backup/restore_context.rs): `activation` (313) macOS-only, `migration` (316) macOS/Linux-only | Keep legacy protocol compatibility isolated. Do not port in-place activation to Windows alongside generation activation. |
| Legacy evidence/completion | [established.rs](../../app/src-tauri/src/startup/established.rs): `RetainedMarker::open_at/check`; [classification.rs](../../app/src-tauri/src/backup/restore_context/pending/classification.rs): `inspect`/non-macOS guards around 377/407; [completion.rs](../../app/src-tauri/src/backup/restore_context/pending/completion.rs): receipt/admission guards; [write_preparation.rs](../../app/src-tauri/src/backup/restore_context/pending/write_preparation.rs): non-macOS rejection (201) | These are legacy-marker/proof restrictions, not C4 admission APIs. Do not reinterpret legacy pending files as generation records; conflicting protocols must block. |
| Legacy persistence | [pre_restore/durability.rs](../../app/src-tauri/src/backup/pre_restore/durability.rs): `require_supported` (22), `sync`/`sync_opened` reject Windows | Do not turn these on merely because first initialization works. Reuse checked-close/preservation principles, not assumptions about in-place replacement. |
| Candidate verification | [restore.rs](../../app/src-tauri/src/backup/restore.rs): `prepare_candidate`, `prepare_in`, `prepare_open_file`, `PreparedRestore::source/try_close`; [private.rs](../../app/src-tauri/src/backup/restore/private.rs) and its Windows ACL/identity code | Reuse bounded ZIP/manifest/schema/document/content checks, hashes, private directories, retained ownership and checked close. Current public preparation stages under the OS temp directory; generation ownership/publication is not implemented. |
| Candidate evidence | [candidate_builder.rs](../../app/src-tauri/src/backup/restore/tests/candidate_builder.rs) | Three C1 cases cover complete current backup round-trip, extraction failures and truncation; include images/places/weather and preservation of an unrelated damaged journal. |
| Selection/admission model | [single_record.rs](../../app/src-tauri/tests/generation_activation_prototype/single_record.rs): `single_record/select/startup/admit`; parent prototype `Lease`, `verify`, `real_barrier` | Reuse contract and tests, not the prototype wholesale. It uses `persist` replacement, whole-file hashing, a synthetic schema and a fixture lease. |
| Startup routing | [startup.rs](../../app/src-tauri/src/startup.rs): `begin_with` (370), `DirectoryLease::database_path`, `StartupReady::database_path`; [database_state.rs](../../app/src-tauri/src/database_state.rs): `initialize_startup` (82); [database.rs](../../app/src-tauri/src/database.rs): `expected_database_path` (12), `DataLocation`, validators | Root path is currently fixed to `mno.sqlite`. Recovery artifacts/sidecars retain a `RecoveryGate`, but many validation errors return Err and release the local lease. A safe damaged-journal restore needs a retained blocked owner with no Ready/DataLocation. Route all consumers to one selected path for the lifetime of the process. |
| Windows initialization persistence | [established/windows.rs](../../app/src-tauri/src/startup/established/windows.rs): `Boundary`, `publish/confirm/sync_created_database` | Reuse local-NTFS checks, write-capable flushes, bounded directory identity checks and error handling. Current marker publication is explicitly **no-clobber**; it does not validate replacing an existing selection record. |

Historical database migration remains restricted; C7 initially accepts only current
backup format 5/schema 4. Restoring an older backup through migration is a separate
work item. Filesystem generation IDs are not the existing in-memory IPC generation
counter: reuse stale-operation protection without conflating the two identities.

## Proposed end-to-end contract

1. **Own the root.** Keep the existing app identifier and Roaming AppData root.
   Acquire the lifetime root lease and verify directory identity, local NTFS and
   permissions before any preparation/publication. Exclude unsafe paths and an
   active other owner; those failures must not gain recovery permissions.
2. **Prepare separately.** Open the selected archive read-only; retain/bind the
   selected input rather than trusting a later pathname lookup. Reuse C1's strict
   checks to produce a private standalone current-schema database. Do not require
   the previous DB to open, validate, produce a protection backup or migrate.
3. **Establish a retained generation.** Place a verified closed candidate under a
   unique private generation directory on the same supported volume as the root.
   Reserve destinations exclusively. If copying from OS temp is necessary, use
   create-new, hash/content verification and independent SQLite reopening of the
   final copy; never assume a cross-volume rename is atomic. Require checked close,
   no unexplained sidecars, final file/directory persistence and unchanged identity.
   `PrivateDir` currently deletes on Drop: introduce explicit retained ownership
   before a candidate can be referenced. Never let publication/error cleanup delete
   selected, possibly selected or previous generations.
4. **Select pending, then restart.** Once normal operations are drained under the
   existing maintenance barrier (or a typed blocked-startup owner), publish one
   versioned pending record binding a canonical generation ID and candidate hash.
   Require checked closure of all owned SQLite connections before handoff; close
   failures block publication. Do not swap an open SQLite database. Latch ordinary operations closed from the
   selection transition through process exit, including late publication errors.
5. **First admission.** Next startup validates the authoritative record, its exact
   pending candidate digest, production schema/content, identity and persistence.
   Publish admitted state only after verification. Return Ready only after required
   barriers and independent checked reopening; never grant writes to pending state.
   A published-but-unacknowledged state must be revalidated/reconfirmed on restart.
6. **Ordinary startup.** Admitted journals are mutable: validate their current
   database/content and path ownership, not equality with the original activation
   digest. Ordinary saves do not rewrite the selection record. Existing application
   reads/writes/backups share the one admitted `DataLocation` and `DatabaseState`.

This avoids the need to open the old live inode through Linux `/dev/fd` aliases;
it does not prove descriptor-bound SQLite identity by itself. C4 only models a
private cooperative namespace. Production tests must establish Windows retained
handle/sharing, directory pinning, reparse rejection and checked-close ordering.
In particular, C1's Windows `open_retained` uses FILE_SHARE_READ only: that immutable
pin deliberately denies new writable flush/SQLite handles. It cannot be carried
unchanged into ordinary mutable startup. Establish a compatible overlapping
identity pin before releasing the restrictive handle, and test the exact flush,
close and writable-open order; never solve a sharing error by dropping every pin
and trusting a later path lookup. C7.2 must verify this handoff on NTFS.
Before/after pathname equality alone is not proof against replacement-and-restoration
races; do not claim Linux activation ready from this design review.

## Failure and transition rules

- Before selection publication, an established previous record remains authoritative;
  candidate construction never modifies the old journal or input backup. It may
  leave an unselected candidate/evidence on failure; automatic garbage collection
  is outside C7. Controlled temporary cleanup requires proven ownership and close.
- After publication has possibly occurred, report failure and keep the process
  blocked. Never promise the old selection survived a late flush failure. Restart
  may see pending or admitted; reconfirm the actual state. Unresolved temporary,
  corrupt/missing record, invalid selected path or conflicting legacy marker blocks.
- Preserve previous database bytes and all its sidecars even when damaged. An
  explicit restore from a valid backup can select a new generation without touching
  that old directory. Do not replay/delete old sidecars to make the candidate work.
- Keep sidecar handling conservative until separately tested. A selected admitted
  journal's unexpected WAL/journal state must not be discarded, skipped or treated
  as an empty database. Pending candidates must be standalone. Normal SQLite crash
  recovery/WAL support needs explicit tests before any guard relaxation.
- **Legacy journals stay in place.** Ordinary legacy startup does not write a
  selection record. Explicit restore introduces generation management by selecting
  its verified candidate. Once transition or generation evidence exists, a missing
  selection blocks instead of selecting the legacy root database. Missing selection
  alone never authorizes first-time initialization.
- Missing/corrupt selection repair is an explicit user-confirmed restore operation,
  not an automatic fallback. Preserve malformed metadata/evidence before replacing
  it and retain all generation directories. The production layout recognizer and minimal blocked-startup UI require tests
  before integration.

One shared selection/admission kernel should be the target architecture. Keep
macOS's current path and Linux's current restrictions unchanged during Windows
validation; temporary legacy-protocol routing is compatibility, not a second
Windows recovery framework. Do not maintain separate copies of candidate validation,
coordination, formats or UI. Migrating macOS routing later requires its own reviewed
compatibility boundary; C7 does not silently convert old macOS recovery records.

## Windows persistence: established versus still required

C5a/C5c tests and C5b.1 unelevated Windows 11 observations support local NTFS file
creation without clobber, checked SQLite close/reopen, write-capable file/directory
flush operations, sharing failures and bounded application-directory barriers.
C6 adds observed ordinary use and backup creation. These do not establish selection
record replacement durability. C4's real Windows directory barrier is still rejected.

C7 must test same-volume replacement of an existing pending/admitted record, not
reuse no-clobber marker publication unchanged. Evaluate a small documented native
operation, checked file and directory flushes, with source/destination sharing and
identity checks. No delete-then-rename, cross-volume copy fallback or success after
an unconfirmed barrier. If a replacement API reports failure after possible
publication, classify/reconfirm disk state rather than assuming the old record.
Keep this operation local to selection publication, not a generic replacement layer.

Process termination tests establish restart outcomes, not physical power-loss
safety. OS crash/NTFS recovery, ancestor persistence and storage that ignores flushes
remain bounded practical risks. Network/redirected/non-NTFS locations remain
unsupported unless independently established. Do not promise protection against
coordinated deletion/replay of all state or deliberate same-user tampering.

## Proposed stages (not implemented)

| Stage / scope and reuse | Safety invariants | Focused automated tests | Remaining limitation / completion criterion |
| --- | --- | --- | --- |
| **C7.1 — selection codec and NTFS publication boundary**, inert module using C4 record rules and C5 Windows handle/flush operations | Strict bounded versioned record; pending/admitted; same-volume publication; retain old data/evidence; no unsafe replacement fallback | Unknown/duplicate fields, bad IDs/versions, missing/truncated records; first publication and actual replacement; injected file/directory failures; sharing/reparse/identity changes; bounded subprocess interruptions before/after both phase publications | No production selection or UI. Complete when Windows native replacement and repeat-startup classification are demonstrated; macOS/Linux record regressions unchanged. Stop if native replacement safety cannot be established. |
| **C7.2 — retained current-backup generation builder**, reuse C1 extraction/content checks, private ACL/identity and checked close | Original backup/old journal untouched; no-clobber final generation; no Drop deletion after ownership transfer; final independently verified bytes/content; no selected candidate yet | Three C1 cases plus final-copy/destination conflicts, permissions/disk/flush/close failures, source replacement, altered candidate, sidecars, immutable-to-mutable pin/flush handoff and ownership/error cleanup; damaged old DB is never opened | No activation, migration or Ready. Complete when real format-5/schema-4 candidates with images/places/weather survive independent reopen and all failure paths preserve inputs. Shared tests on macOS/Ubuntu/Windows. |
| **C7.3 — selected startup and admission kernel**, reuse one root lease, DatabaseState, DataLocation and production validators; keep legacy startup until explicit restore | All DB consumers agree on one path; pending has no normal writes; admitted allows legitimate edits without per-save head updates; ambiguous selection never creates/falls back; only safe blocked owners can offer restore | Healthy legacy/current first launch; selection deletion with legacy DB present; pending admission; edit/checked-close/restart; invalid content/path and missing selected DB; concurrent owner/late writes; damaged DB retains exclusive blocked owner; legacy-marker conflict; failure after admitted publication | No user restore command enablement yet. Complete after full three-platform integration CI, with known unrelated Windows failures kept visible and macOS/Linux routing unchanged. |
| **C7.4 — connect explicit restore and restart**, reuse existing picker/token/busy handling, maintenance exclusion and restart flow | One kernel for healthy and blocked-startup restore; no dependency on reading damaged old DB; changed backup/token rejected; publication latches writes closed; preserve every prior generation and backup | End-to-end synthetic backup -> final generation -> pending -> restarted admission; cancellation, repeated clicks, queued saves, corruption and process interruptions at each handoff; ambiguous state requires explicit action | No Import, historical migration, cleanup UI or legacy macOS conversion. Complete after focused Windows tests and full CI at the production command integration boundary; restoration stays unsupported until this boundary is approved. |
| **C7.5 — unelevated Windows 11 acceptance** using an isolated ordinary test account, local NTFS and synthetic data | Same identity/root, no real journal; verify prior generations and original backup remain intact; no silent rollback/empty journal | Retain automated matrix and replay-limitation regression; no need to rerun unrelated suites after documentation-only acceptance | Manually restore a real synthetic backup containing entries/images/places/weather, restart/edit/restart, restore with a damaged old fixture, exercise interrupted/pending and corrupt/missing head cases safely, record warnings/permission failures. Complete only within stated observed scope; no power-loss certification. |

Use targeted local tests first and focused platform CI for C7.1/C7.2. Full CI belongs
at C7.3/C7.4 production integration boundaries. The four known Windows library
failures remain failures, not skips or evidence of new regression. No production
readiness claim follows from a green isolated prototype alone.

## Decisions before production integration

1. Specify the exact on-disk evidence the production layout recognizer will use
   to enforce the approved legacy and missing-selection rules.
2. Approve minimal blocked-startup choices: explicit backup restore or exit, with
   all journal operations unavailable. Current startup error UI hides the window
   and exits, and the current restore picker requires a Ready database.
3. Confirm conservative sidecar handling for the first Windows restore scope;
   automatic WAL/crash recovery and manual previous-generation selection remain
   separate approvals. No automatic deletion or rollback UI is proposed.

Recommended next implementation only: **C7.1**, after reviewing this plan. Keep
layout-recognition/UI details open before C7.3/C7.4 rather than expanding the first stage.

## C7.1 implementation and verification

The test-only codec in [generation_selection.rs](../../app/src-tauri/src/startup/generation_selection.rs)
accepts version 1, a canonical v4 generation UUID, a pending or admitted phase
and a lowercase SHA-256 candidate digest. The parser bounds record size and
rejects unknown and duplicate fields. The digest binds first admission. This
stage checks record transitions and leaves database admission to later work.

The classifier takes independent layout evidence. Proven fresh and untouched
legacy layouts can have no record. A managed layout requires a valid record.
Contradictory evidence, unknown layout, or evidence of interrupted publication
blocks selection. The inputs are fixtures for the future layout recognizer.
Production must establish freshness or legacy status from its layout and
establishment evidence. Once a restore introduces generation management,
retained generation or transition evidence must prevent legacy fallback if
the record goes missing. Existing journals stay in place until explicit restore.

The Windows [publication probe](../../app/src-tauri/src/startup/established/selection_probe.rs)
uses the C5 directory lease and local NTFS boundary. It creates and flushes
an interruption marker, then writes, flushes and independently rereads a
create-new staged record. Same-directory MoveFileExW uses WRITE_THROUGH and
adds REPLACE_EXISTING only for a verified transition from an existing record.
The probe rereads the published bytes and flushes the record and bounded
directories before removing the interruption marker. A marker surviving a
crash conservatively blocks selection. Any failure before acknowledgement
leaves evidence. The probe has no automatic repair or cleanup for interrupted
publication. Its namespace assumes cooperating owners under the lease.

The cross-platform test covers codec and layout classification. The Windows
checks cover repeated native replacement and lease reacquisition, sharing
denial, corrupt records, and injected failures before and after replacement.
The fault tests exercise returned errors at explicit boundaries. They do not
simulate OS crashes or loss of power. The existing
`windows-initialization-probe` CI scope runs the new tests on Windows NTFS.
Native replacement evidence remains pending that run. Older valid record
replay remains an accepted limitation. Candidate persistence, physical
power-loss durability and production startup integration remain outside C7.1.

### C7.1a acknowledgement ownership

C7.1 released the marker handle before replacement and later removed the
marker by pathname. Its published record handle also allowed competing
writers after the byte check. Both gaps crossed the acknowledgement boundary.

The isolated Windows probe now creates the marker with write and DELETE
access and zero sharing. It retains that handle through replacement,
verification and flush. The published file is opened with read and write
access, zero sharing and reparse-point protection. The probe reads and
flushes that same handle and retains it through acknowledgement.
Acknowledgement uses FileDispositionInfo on the owned marker handle.
Successful disposition removes the marker when that handle closes. A failed
disposition propagates its Windows error and leaves the marker as blocking
evidence. Ordinary handle close on an earlier error leaves the marker too.
[Microsoft API reference](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle).

The focused Windows job runs the existing C7.1 tests and the new native
checks. A child process attempts marker deletion and rename immediately
after replacement, and record write, deletion and replacement after the
verification and flush barriers. The parent waits at those boundaries while
holding the handles, then checks acknowledgement and reopening. A separate
test calls the real disposition API on a handle lacking DELETE and verifies
that reopening blocks. These tests still require a completed Windows run.
The error hooks model interruption at publication boundaries. Physical
power-loss behavior and protection after handle release remain unverified.

### C7.2 retained candidate and writable handoff

C7.1a was accepted after Windows run
[42176298998](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42176298998).
The [C7.2 Windows probe](../../app/src-tauri/src/backup/restore/tests/candidate_handoff.rs)
reuses C1's private extraction from a read-only archive handle and checked
SQLite close. It copies the closed database into a new private directory
using exclusive file creation, flushes the copy, then independently verifies
file identity, the manifest digest, SQLite integrity, current schema and
stored content including image assets. The fixture removes the external
image original before construction. Failed retained copies stay in the
fixture for inspection.

Before modeled admission the probe reacquires a restrictive read handle and
rechecks identity, digest and content. A second read handle shares reads
and writes while continuing to deny delete access. Both handles are live
when identity is compared. The restrictive handle is released after
verification, and SQLite opens read-write without CREATE while the second
handle keeps the object pinned. The connection closes before the pin is
released. Subsequent ordinary reopening validates current content after a
legitimate edit. The bundled SQLite Windows VFS normally shares reads and
writes, which is compatible with this proposed overlap. Native tests are
required to establish that the actual handoff succeeds.

The probe covers repeated retained construction, truncation, archive tampering,
copy and verification failure, a byte-identical replacement object, altered
bytes, conflicting handles and injected handoff failure. It rejects unexpected
candidate sidecars before opening SQLite and preserves their bytes. Tests
also preserve the source backup, active journal and previous generation.
C1 retains its separate mid-extraction failure checks.

Local macOS checks passed the three C1 tests and the selection codec test.
The focused Windows CI scope runs both C1 and C7.2. C7.2 uses a private fixture
namespace with directory pins. Admission is an in-process test boundary.
After admission, write sharing relies on cooperating journal owners.
Durable directory publication, cross-process admission and production layout
recognition remain for later stages. Power-loss durability remains unverified.

## C7.2 review and C7.3 startup integration audit

C7.2 at `afd4d40d0f9751369de766e635c6371fb2538a54` is accepted within its
fixture scope. The reported native Windows result is
[run 42177014641](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42177014641).
The review found no blocking defect in the isolated probe. Production
integration has additional gates described below.

| Classification | Finding |
| --- | --- |
| Verified in scope | `construct` uses C1 extraction and checked close. Exclusive creation prevents overwriting a retained candidate. `verify` checks the standalone header, digest, SQLite integrity and existing schema/content validation. The image fixture lacks the external original, so the check depends on stored assets. |
| Verified in scope | `admit` compares Windows file IDs while the restrictive and permissive read pins overlap. The restrictive pin denies write and delete access. The bridge allows SQLite writes while denying delete access. The fixture exercises conflicting handles, rename denial, a byte-identical replacement before admission and successful edits after admission. The connection precedes the bridge in ownership and closes before its release. |
| Nonblocking | The test reuses one private fixture namespace and probes Windows sharing with competing handles in the same process. It does not exercise a startup lease across competing processes or identify a SQLite handle directly. The identity argument depends on the pinned directory chain and compatible native sharing. |
| Nonblocking | The pending object holds directory pins but releases the database pin between construction and modeled admission. Admission rechecks both identity and bytes. The test demonstrates in-process revalidation and preserves the backup, old generation and unrelated journal data. It provides no persisted proof of the original file ID across restart. |
| Blocking for production enablement | A valid digest alone cannot authorize writable access to a hard link to another journal. Resolve canonical fixed names under the lease, check each directory component and database handle for reparse points, require the supported local volume and reject multiple links to the database. Carry the verified path and pins into Ready. Add real Windows alias, sharing and owner-exclusion regressions before enabling the managed route. |
| Deferred | After the restrictive pin is released, the bridge intentionally permits other write-compatible opens. The probe assumes cooperating writers under a private root. It provides no protection against independent same-user writes after admission, and sidecar presence checks do not prevent arbitrary external sidecar creation. The probe neither flushes a published generation directory nor proves power-loss durability. |

### Production entry points

1. In [startup.rs](../../app/src-tauri/src/startup.rs),
   `begin_with` acquires `DirectoryLease` at line 375. Classify the root
   under that lease before the legacy database path is derived at line 392.
   Keep `checkpoint` and reject conflicts between legacy recovery artifacts
   and generation state. Use `established::read` and the existing Windows
   establishment confirmation for established roots. Do not publish an
   initialization marker to repair an ambiguous managed root.
2. Use [generation_selection.rs](../../app/src-tauri/src/startup/generation_selection.rs)
   for the bounded codec and classification contract. The filesystem
   recognizer must supply real evidence and reject I/O failures, reparse
   points and reserved name aliases. Resolve only the record's canonical
   UUID beneath a fixed generations directory with a fixed database filename.
   Never scan generations to choose a substitute.
3. For pending selection, reuse C7.2's digest, integrity, schema and content
   checks under restrictive handles. Require standalone SQLite state and
   preserve all unexpected sidecars. Acquire the compatible bridge before
   releasing the immutable pin. Confirm the candidate's bounded directory
   persistence and acknowledge the admitted record while writes are still
   denied. Reuse the C5 checked flush primitives and the C7.1a publication
   order. File flushes requiring write access must occur during verified
   candidate preparation or under a separately verified sharing order.
   The C5 initialization marker and its no-clobber publisher cannot stand
   in for generation acknowledgement.
4. `StartupReady::database_path` and `ensure_current` at lines 269 and
   278 are the owner boundary. Store the selected path and retained pins
   there. [database_state.rs](../../app/src-tauri/src/database_state.rs)
   `initialize_startup` at line 82 installs the owner before Ready.
   Its `ready_database_path` at line 121 currently recomputes the legacy
   path from the lease. Make it return the same selected path as startup.
   Keep `DirectoryLease::database_path` as the legacy protocol path for
   existing recovery callers.
5. [lib.rs](../../app/src-tauri/src/lib.rs) installs `DataLocation` at
   line 101 only after successful startup. Preserve that gate.
   [database.rs](../../app/src-tauri/src/database.rs) `open_database` at
   line 28 uses that location for read-write open without CREATE.
   `initialize_database` at line 43 supplies current schema/content checks.
   On admitted restart, reacquire the lease and pins and validate current
   state without comparing ordinary edits to the pending digest.
   A writable open failure must withhold Ready on that attempt. A clean
   admitted record can be revalidated on a later attempt.

### Legacy discrimination and failure policy

Use the retained generations namespace itself as evidence that explicit
restore has started. The publisher must establish and flush it before first
selection publication and retain it thereafter. Any selection record,
reserved staging or intent name, or generations namespace triggers managed
inspection. An empty, malformed or inaccessible namespace is still evidence
and cannot authorize legacy fallback. Absence of all managed evidence allows
the existing legacy path and existing fresh-creation proof to run. A newly
created root, new lock and lock-only inventory are required for creation.
A missing record alone carries no authority. No extra marker is proposed
for this scope. Loss of both the selection and every managed artifact
cannot be distinguished from an untouched legacy layout. An independent
marker would be a separate decision if that stronger detection requirement
is adopted. Older valid record replay remains accepted.

| Startup state | Required result |
| --- | --- |
| Established legacy root with current valid database and no managed evidence | Continue current startup at the same root path. Preserve existing establishment behavior and never write a generation selection as part of ordinary legacy startup. |
| Damaged legacy database | Preserve the existing error path and all files. No fallback creation or implicit restore. |
| Managed evidence with missing or malformed record, alias, unknown version or conflicting legacy artifacts | Block before opening SQLite. Preserve the record, legacy root and all generations. |
| Valid record with missing selected directory or database, unsafe identity, pending digest mismatch or integrity/content failure | Block. Preserve evidence and withhold Ready and DataLocation. |
| Unexpected selected WAL, SHM or rollback journal | Block without replay or cleanup. Sidecars in an unselected previous journal stay untouched and cannot choose the active journal. |
| Intent or staged evidence after interrupted admission or publication | Block even when the visible record parses. An intact pending record without interruption evidence can resume verification. An acknowledged admitted record can reopen after current validation. |
| Concurrent owner, sharing denial or persistence failure | Block or return the existing in-use outcome. No retry that weakens sharing, no file deletion and no alternative journal selection. |

### Small C7.3 slices and completion gates

1. **Resolve layout and own the path.** Add a Windows managed resolver under
   the existing lease and thread a single selected path through Ready and
   DataLocation. Keep managed cases blocked while this slice is verified.
   Test established/fresh discrimination, missing records with a surviving
   generations directory, conflicting artifacts, names and inaccessible
   files. Verify legacy startup on Windows, macOS and Linux.
2. **Admitted startup with retained identity.** Verify the pinned directory
   chain, file identity, link count, sidecars and current SQLite content
   before issuing Ready. Run native Windows cases for reparse points, hard
   links, replacement, sharing denial and a competing startup process.
   Test that edits survive restart and that journal and backup path
   consumers agree. Pending records stay blocked in this slice.
3. **Pending admission and acknowledgement.** Revalidate the pending digest
   and candidate before the protected C7.1a transition. Test interruption
   before and after record replacement, verification/flush errors, marker
   removal failure and a subsequent writable-open failure. Verify that
   no failure exposes a writable DataLocation or selects an older journal.
   Exercise native Windows restarts and retain C4 and legacy startup
   regressions on macOS and Linux.

Complete C7.3 only when native Windows tests cover both startup phases
and failure boundaries, the full platform CI comparison preserves the
known baseline, and legacy paths retain their behavior. Restore commands,
Recovery UI, layout migration, destructive cleanup and physical power-loss
claims are outside this integration. Before implementation, confirm that
retained namespace evidence and the cooperating-writer model are the
intended limits. C7.4 must establish and persist that namespace before
publishing its first selection.

### C7.3a production layout and path ownership

Under the existing Windows directory lease, the
[layout resolver](../../app/src-tauri/src/startup/established/managed_layout.rs)
recognizes the reserved selection, staging, interruption and generations
names before the legacy path is derived. Any surviving managed evidence
prevents legacy fallback. The resolver rejects noncanonical case aliases,
missing or invalid metadata, incomplete publication and conflicts with
legacy recovery artifacts. A managed root requires existing establishment
evidence. Absence of managed evidence allows the existing legacy path,
including its separate first-creation checks.

For a valid record the resolver inspects only the selected UUID directory
and its fixed journal filename. It checks local NTFS, volume and file
metadata through retained handles, rejects reparse points and multiple
links, and checks for SQLite sidecars by presence. Both phases end in
the typed disabled outcome. The resolver never invokes SQLite for a
managed journal. Its file checks establish the current layout and object
type. Pending digest and cross-restart admission validation belong to
later slices. A plain substituted file still cannot receive Ready.

StartupReady owns the resolved path and lease, with a retained Windows
database pin on successful legacy startup. The pin allows compatible
SQLite access while denying delete sharing. Windows validates the legacy
database's volume, link count and type before Ready, and the operation
gate checks the named identity against the pin. The startup result and
ready_database_path both read the Ready path. Existing app setup installs
that same result as DataLocation. The old lease-derived path remains
limited to the existing legacy recovery gate. macOS and Linux continue
using their existing startup checks and paths.

The native Windows tests exercise both managed phases, missing and
malformed state, an unsupported version, conflicting legacy evidence,
interruption, sidecars, hard links, a junction and a missing database.
They check that rejected startup preserves the legacy and candidate
bytes and grants no operation. Existing fresh and legacy tests run in
the same Windows scope. Local macOS startup tests passed. Native Windows
and Linux evidence awaits the full CI run. The accepted cooperating-writer
and total-managed-artifact-loss limits still apply.
