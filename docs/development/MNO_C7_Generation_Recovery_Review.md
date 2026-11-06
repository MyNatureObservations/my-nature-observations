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

### C7.3b admitted Windows startup

The locked SQLite Windows VFS uses read/write sharing for ordinary database
opens. The native C7.2 probe demonstrated the compatible overlapping read
pins and ordinary SQLite writes. C7.3b reuses this handoff inside the existing
startup lease. The resolver protects the selection while resolving its exact
UUID and checking the retained directory and database handles.

An admitted database must have a standalone rollback-format header and no
unexpected sidecars. A restrictive read pin denies writes and deletion during
current schema, integrity and stored-content verification. Startup reconfirms
the existing establishment marker. An overlapping read pin allows compatible
writes while denying deletion. File identity must match before releasing the
restrictive pin. The existing read-write opener then checks that SQLite can
actually open the database for writes without CREATE and closes it before Ready.
Ready retains the database and directory pins. Normal SQLite operations use
the selected path and retain their existing locking and transaction behavior.
An admitted restart checks current data independently of the original digest.

Pending and unsafe managed layouts return blocked startup errors. Neither the
presence of a legacy journal nor evidence in an unselected generation changes
the selected path. The native tests exercise the same DataLocation installer
used by production Tauri setup through Tauri's mock runtime. They open the
selected database through the production opener, edit it, drop the app state,
then start again and read the edit. The tests check the selection and legacy
bytes, preserve an old WAL, and exercise conflicting handles, lease exclusion,
rename denial and rejected managed layouts. This verifies managed Tauri state
without requiring a Windows desktop window or launching an installer.

Windows execution of these new tests remains pending CI. The existing
cooperating-writer model, older valid record replay and total managed evidence
loss limits apply. Power-loss durability is outside this startup check.

### C7.3b.1 Windows loader investigation

[Run 42183716412](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42183716412)
compiled the Windows library test executable, which exited with
0xC0000139 before printing a harness header. The selected integration
executables ran 37 tests successfully. The library log contains neither
the entry point nor the DLL and cannot distinguish a direct import failure
from one in a dependent DLL or initializer. No library test result can
be compared with the C5c four-failure baseline from this run.

The C7.3b lockfile matches C7.3a. The new Windows-only Tauri test feature
and the mock runtime call can change the test executable's linked imports.
The managed startup changes reuse the existing file-information, handle,
sharing and persistence APIs. The available evidence does not identify
which binary or DLL requested the unresolved symbol.

The opt-in windows-loader-diagnostic scope builds both library executables
on Windows Server 2022, prints their import differences, checks direct
imports against DLL exports and attempts to list tests. If Windows SDK
debugging tools are installed, it also captures loader snaps for the failing
executable. The export check runs in PowerShell and the executable launch
and loader traces provide the process-specific evidence. No application
changes are included in this investigation.

### C7.3b.2 confirmed loader failure and test boundary

[Native diagnostic run 42185653450](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42185653450)
found a new direct import from the C7.3b library test executable,
comctl32.dll!TaskDialogIndirect. The working C7.3a executable lacked that
import and listed tests successfully on the same runner. Loader snaps
identify the failed name lookup in Common Controls 5.82.20348.5139.
The subsequent userenv.dll message reports the process-wide initialization
failure. The missing export belongs to Common Controls and requires
[version 6](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-taskdialogindirect).

The mock-runtime test instantiated the generic app setup helper, including
its native startup error branch. That branch links tauri-plugin-dialog
and its rfd 0.16 common-controls-v6 backend, which directly references
TaskDialogIndirect. The Windows-only Tauri test feature enabled the mock
runtime. The logs identify the linked import, and the source identifies
the dialog call chain. A feature-only build was not separately measured.

The lifecycle test now calls the same startup-to-DataLocation installer
as app setup with a capture callback. Production supplies app.manage
and handles native dialogs outside that shared boundary. The test uses
the captured path with the production SQLite opener, checks path agreement
and verifies edits across a second startup. The test also checks that
blocked startup never invokes the installer. This supersedes the mock
runtime described above. Actual Tauri state registration, lookup and
desktop startup remain outside this test's coverage.

The focused windows-startup-baseline scope lists library tests, runs native
startup regressions and runs the full Windows library suite for comparison
with the four known failures. Native verification of the correction awaits
that run. Local macOS all-target compilation and eight startup-error tests
passed.

### C7.3c pending Windows startup

The ordering review found a bounded path using the C7.3b resolver and the
C7.1a publisher. Startup retains the directory lease and checks the record,
filesystem identity and sidecars. It hashes pending bytes through the same
restrictive handle used to protect read-only SQLite validation. That handle
requests write access for flushing and shares reads only. No SQL writer opens
at this point. SQLite's ordinary read-only open shares both read and write,
as required to coexist with the flush handle. Native verification of this
specific combination is part of the new Windows tests.

After verification, startup overlaps the immutable pin with the mutable
bridge, checks identity and flushes the file and generation directories.
It releases the selection record's read pin to permit replacement, checks
its bytes again under the lease, then uses the same protected publication
function as the C7.1a tests. The candidate pins stay live through publication
and handle-based acknowledgement. Startup releases the restrictive pin and
confirms writable SQLite access only after acknowledgement succeeds. The
admitted record retains the candidate digest for provenance. Ordinary
admitted startup validates current content independently of that digest.

Before publication, failure leaves a pending record eligible for a fresh
verification attempt. After intent creation, surviving intent or staging
evidence blocks startup even when the head contains valid admitted bytes.
After acknowledgement, a failed writable open leaves an admitted record
which can be revalidated on restart. Every failure withholds Ready and
the location callback. Tests inject errors across verification, replacement,
acknowledgement and the writable-open boundary, including while the SQLite
connection is live before its checked close. These are deterministic error
injections. The C7.1a tests continue to exercise native sharing conflicts
and a real disposition failure on the same publication implementation.

The Windows lifecycle tests check pending-to-admitted startup, preservation
and a subsequent edit and reopen through the captured DataLocation. Actual
Tauri state registration and desktop startup remain outside that boundary.
Local macOS all-target compilation, the codec test and 11 C4 regressions
passed. Full platform CI is required for native acceptance. The accepted
cooperating-writer, older-record replay and total managed evidence loss
limits remain. Physical power-loss durability remains unverified.

## C7.4a Restore and Recovery integration review

C7.3c is accepted within the tested Windows scope of
[run 42192369790](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42192369790).
The two admission tests ran and passed. The Windows library baseline remains
four known failures. The captured DataLocation tests exercise the shared
installer callback, while actual Tauri registration and desktop startup
still require acceptance.

### Existing macOS workflow

| Step | Current behavior and code |
| --- | --- |
| Choose and validate | The native picker in `choose_restore_backup` uses RestoreState's busy reservation. `validate_selection` requires a Ready operation permit, reads the active schema, checks the backup manifest and verifies the archive. It returns a one-use token bound to the archive hash and current process generation. |
| Confirm | The settings Restore page shows confirmation after choosing a valid backup. Cancel discards the frontend token. Confirm invokes `restore_selected_backup`, which takes the backend choice and verifies the token. The command runs blocking work off the UI thread and emits the mode-change event when it returns. |
| Preserve and commit | `activate_selection` obtains Maintenance, checks the process generation and revalidates the archive hash. The macOS activation engine prepares the candidate, retains the active source and writes verified protection backup evidence. It publishes its legacy recovery marker binding old and new content before copying to a separate local staging database. It verifies staged content before activation. |
| Close and restart | The activation engine uses its close-owner and marker contracts to check SQLite closure and persistence around macOS activation. The running recovery latch keeps ordinary operations closed. Successful activation tells the frontend to quit and reopen, without granting Ready in the old process. `quit_after_restore` uses normal application exit with the existing work and unsaved-change guards. |
| Blocked startup | `begin_with` retains a RecoveryGate for recognized legacy artifacts or sidecars. App setup withholds DataLocation. The layout selects RecoveryPanel through `database_mode`. The macOS status engine classifies marker, active database, candidate and protection evidence and offers verified actions such as Continue, FinishNew or KeepPrevious. |
| Recovery completion and errors | `run_recovery` reserves work, runs the confirmed macOS engine and installs DataLocation only when startup admission has granted Ready. Mode changes return the shell to the journal. Invalid startup without a retained RecoveryGate goes through the native error and exit path. Restore errors use localized invalid/failed messages, with quit errors handled separately. |

Sources for the command and disk paths.
[restore_app.rs](../../app/src-tauri/src/restore_app.rs),
[activation.rs](../../app/src-tauri/src/backup/restore_context/activation.rs),
[startup recovery engine](../../app/src-tauri/src/backup/restore_context/activation/startup.rs),
[recovery.rs](../../app/src-tauri/src/recovery.rs),
[startup_error.rs](../../app/src-tauri/src/startup_error.rs).

Sources for UI and application setup.
[Restore page](../../app/src/routes/settings/restore/+page.svelte),
[RecoveryPanel](../../app/src/lib/components/RecoveryPanel.svelte),
[app layout](../../app/src/routes/+layout.svelte),
[lib.rs](../../app/src-tauri/src/lib.rs).

### Windows restore sequence

1. Reuse the picker, busy reservation, one-use confirmation and current
   format-5/schema-4 validation. The choice must bind the archive digest and
   the current journal context. Open the archive read-only with write/delete
   denial during the confirmed read and hash the same object used by C1.
   Changes between selection and confirmation must require a new choice.
2. Prepare and independently verify privately before entering the durable
   transition. The existing UI validates before confirmation and performs
   the retained build after confirmation. This avoids keeping a prepared
   SQLite owner alive while the user decides. C1 extraction may occur for
   validation before confirmation, but the managed namespace must remain
   absent on a legacy root until explicit commit.
3. Promote the relevant C7.2 construction and ownership checks into one
   production builder. Reuse the private ACL and C1 validation code. Use an
   exclusive generation destination, independently verify the final file,
   check identity, link count and sidecars, and check every SQLite close.
   Flush candidate bytes and the same-volume directory chain. C7.2 tested
   the handle handoff with fixtures. It did not establish durable production
   publication of a retained directory. A staged file prepared outside the
   authoritative namespace must be revalidated at its final location.
4. At commit, use the existing `DatabaseState::maintenance` barrier to drain
   journal and backup operations, check the confirmation context again and
   latch normal operations closed before introducing managed evidence or
   publishing a record. The macOS RecoverySession factory is platform-gated.
   Add a narrow Windows capability under the same state and lease. Preserve
   operation exclusion, stale editor rejection and the old journal's file
   ownership. The last successful old-journal write before the latch stays
   in the retained previous journal.
5. Publish only pending through the C7.1a publisher under the lease. Its
   low-level function accepts a replace flag, so the caller must enforce
   the expected transition and current head identity. Initial publication
   is no-clobber. Replacement requires the previously admitted head to
   match. Retain the candidate protection through publication and checked
   close of owned resources. Keep normal commands closed once publication
   may have happened. Reuse the explicit quit and reopen flow, with the
   original DataLocation inaccessible through normal operation gates.
6. On reopening, C7.3a detects the managed layout, C7.3c verifies and admits
   the pending generation, and C7.3b provides ordinary mutable opening.
   Install DataLocation only after Ready. No running-process path switch
   is required. The original legacy database or previous generation stays
   where it was. Windows needs no old-database copy to preserve it.

Reusable production boundaries.
[C1 preparation](../../app/src-tauri/src/backup/restore.rs),
[protected publisher](../../app/src-tauri/src/startup/established/selection_publication.rs),
[Windows resolver](../../app/src-tauri/src/startup/established/managed_layout.rs),
[state and maintenance](../../app/src-tauri/src/database_state.rs).
The [C7.2 probe](../../app/src-tauri/src/backup/restore/tests/candidate_handoff.rs)
supplies the verified handoff pattern. The legacy macOS protection backup,
filesystem activation and marker proof chain stay on the macOS path.

### Legacy transition and interruption

C7.3 can handle an established legacy root coexisting with a valid managed
selection. The presence of `root/mno.sqlite` and its sidecars does not
override the selected generation. Existing legacy recovery markers are a
different case and conflict with managed startup. Ordinary legacy startup
continues without creating the generations directory or selection.

The first durable managed artifact is an important commit boundary. If
construction creates the generations directory and then fails before
publishing a valid head, startup correctly blocks. The original journal
still exists, but automatic fallback would violate the approved rule.
Prepare before this boundary, and take the irreversible process latch
before creating managed evidence. A brief incomplete state is acceptable
only when explicit Recovery is available. No new authoritative format is
proposed. The existing intent marker and retained namespace provide the
interruption evidence.

| Failure boundary | Required disposition |
| --- | --- |
| Cancel, validation failure or interrupted private preparation | Preserve input and previous data. No managed evidence on a previously legacy root. A new explicit attempt can retry. Close verified handles before cleanup of exclusively owned scratch files. |
| Failure introducing durable generation evidence or during publication | Preserve candidate and intent/staging evidence. Block ordinary operations and require explicit Recovery if the layout is incomplete or ambiguous. |
| Pending acknowledged, before quit or restart | Keep the old process gated. A later startup can verify and admit the pending candidate. |
| Admission verification fails before publication | Preserve the pending record and files. Transient sharing problems can retry by restart. Persistent digest/content/path failures require Recovery. |
| Admission interrupted during publication | Block on surviving intent or staging evidence. No automatic repair or selection of the old journal. |
| Admitted acknowledgement succeeds but writable open fails | Withhold Ready. A clean admitted record can be revalidated on restart. Persistent failures require explicit Recovery. |

### Windows blocked-startup Recovery

Today the managed resolver returns errors through the generic startup
error path. App setup hides the window and exits after the native message.
A damaged legacy journal can take the same path. Existing RecoveryGate
cases can reach the shell but Windows status and action commands return
unsupported. The picker currently requires Ready and the active schema,
so it cannot provide an escape route for a damaged journal.

C7.4c needs a narrowly typed Windows recoverable verdict retaining the
existing DirectoryLease without DataLocation. Only a successfully acquired
and verified supported root may grant that capability. In-use, unsafe
path, unavailable permissions and unsupported filesystem errors stay
outside it. Reuse RecoveryPanel and the Restore picker/confirmation
components in that shell. The first choice should be explicit restore
from a verified backup or quit. Candidate construction must depend on
the archive and safe root, never on opening the damaged prior database.

A missing selected generation, malformed/unsupported record, damaged
legacy database, failed pending verification or persistent admitted open
failure can use that same recovery route. Interrupted publication needs
an additional authorization rule. The current publisher cannot reuse a
surviving create-new intent path, and simply deleting stale markers would
remove the evidence that prevents unsafe admission. Under the retained
lease, preserve the exact old head and any intent/staged evidence before
a user-authorized replacement. Verify which objects belong to the old
episode and fail closed when ownership cannot be established. Decide the
evidence-retention and recovery publication sequence before coding that
route. Choosing a previous generation is deferred. If later offered, it
requires an explicit selection and independent verification of current
content, with no automatic rollback or fallback scan.

### Stages and decisions

- **C7.4b, explicit restore and controlled restart.** Wire the Windows
  candidate builder, archive token, maintenance latch and pending publisher
  beneath the existing command boundary. Keep end-user availability gated
  until C7.4c provides an escape from interrupted first publication. Native
  tests cover legacy and managed origins, changed backup, cancellation,
  queued writes, competing owner, candidate copy/flush/close faults,
  publication and restart admission. Compare previous bytes and sidecars
  and verify stored images after reopening. Completion requires a native
  command-boundary round trip and deterministic fault gates.
- **C7.4c, blocked startup recovery.** Route owned safe-root failures to the
  existing shell with a narrow backup restore action. Reuse the C7.4b builder
  and C7.1a acknowledgement. Resolve the interrupted publication evidence
  policy first. Native tests cover each blocked state, no DataLocation or
  stale writable operation, safe exit, preservation of conflicting evidence,
  and explicit recovery from a damaged old journal. Completion requires an
  end-to-end Windows desktop check with an isolated ordinary account and
  synthetic data. Check confirmation, quit/reopen, Tauri DataLocation
  registration, a legitimate edit, and a second restart.
- **C7.4d only if needed.** Address specific compatibility or UX gaps from
  the desktop check using the same Restore and Recovery surfaces. Keep
  previous-generation selection and cleanup outside this stage unless
  separately approved.

For b and c, run existing macOS restore, guarded quit, recovery activation
and startup regressions, plus Linux legacy startup and unsupported-command
checks. Use full platform CI at production integration boundaries and
compare the known four Windows library failures. Neither prior CI nor the
planned desktop acceptance establishes physical power-loss durability.

**Review verdict.** The shared architecture supports a staged Windows
implementation. End-user Restore availability is blocked until safe
blocked-startup Recovery exists. Approve the b-before-c rollout gate,
the exact point at which managed evidence becomes authoritative, and
the c policy for preserving and superseding interrupted publication
evidence before implementation. The cooperating-writer, older-valid-record
replay and total managed evidence loss limits still apply.

### C7.4b implementation boundary

Restore availability is queried by the Settings navigation and direct
Restore route. Both backend Restore commands enforce the same compile-time
platform decision before opening a picker or consuming a confirmation.
Windows stays disabled in all builds. Native tests call the internal
implementation. Revisit the gate only after C7.4c and desktop acceptance.

The running journal retains its startup lease. The maintenance guard
drains journal operations while the confirmed archive is reopened with
restrictive sharing, checked against the selection digest, and passed
through C1. The candidate is closed before managed publication. The
running process latches closed before creating the first managed directory.
The publisher copies into an exclusively created database, checks identity
and digest under a restrictive pin, independently verifies SQLite and
stored content, and flushes the candidate and directory boundaries before
publishing Pending through C7.1a. The existing explicit Quit path is
available after the latch. Only C7.3c on restart can admit the candidate.

After first managed-namespace creation, a missing selection blocks
ordinary startup. Interruption during record replacement retains the
publisher's existing evidence. After acknowledgement a restart may admit
Pending even if Quit did not run. During a subsequent restore, a failure
before changing the previous authoritative record can leave that same
admitted generation selected. Previous database files, original backups,
and interrupted generation directories are retained.

Native Windows tests exercise legacy and admitted publication, restart,
the disabled command gate and injected publication failures. These
injections model returning at specific boundaries on NTFS. They do not
exercise a physical power loss or actual desktop Quit. The existing
limitations for cooperating writers, replay of an older valid record and
complete loss of managed evidence remain.

### C7.4c.1 backend supersession

Managed Windows startup failures retain the directory lease and stay in
the blocked shell. The internal confirmation boundary requires the
confirmed archive digest and serializes attempts. It reuses C1 archive
validation and C7.4b candidate preparation, without opening the old
journal during Recovery. Acknowledgement leaves the process blocked
until restart. Windows Recovery has no new Tauri command or UI surface.

On established local NTFS with a plain managed namespace, Recovery can
supersede missing or malformed heads, invalid selected generations and
interrupted publication. It retains or establishes the protected intent,
copies bounded plain single-link head, staging and intent bytes into a
private recovery-evidence directory, and flushes that evidence before
retiring any occupied staging slot by its retained handle. It holds the
intent through the existing protected publisher's acknowledgement.
Only the new Pending head is authoritative. No previous database or
backup is removed. Evidence directories are diagnostic only.

Unsafe names, object types, sharing failures, evidence above the codec
size bound, conflicting legacy artifacts and layouts lacking verified
establishment remain blocked. After an interrupted supersession, an
explicit retry can archive the surviving evidence and try another
candidate. After acknowledgement, ordinary startup verifies and admits
Pending. A retryable admission error can be retried by restarting, while
ambiguous publication requires explicit Recovery.

Native Windows tests exercise confirmed supersession, preservation,
writable reopening, lease exclusion and injected failures. Fault
injection checks individual boundaries on NTFS. Desktop behavior,
coordinated noncooperating writers, older valid head replay, total loss
of managed evidence and physical power-loss durability remain outside
the verified contract.

#### C7.4c.1a regression corrections

The admitted and pending lifecycle tests started a second in-process
startup while their blocked startup state still owned the directory
lease. The tests now assert exclusion after the competing file writer
closes, then drop the blocked state to model process exit before retrying.

The C7.4b Settings availability check added a mount hook that four
frontend test harnesses had not modeled. The Settings markup test also
searched only direct action-row children, so it missed the Restore link
inside its availability condition. The test fixtures now account for
the lifecycle hook and verify the conditional native link with both
availability responses. Production startup and the availability gate
are unchanged.

### C7.4c.2 Windows Recovery shell integration

The existing Recovery panel now offers Windows backup recovery when
managed startup is blocked, the startup lease remains valid and the
established local NTFS layout and evidence pass preflight. Ready,
unsupported and lease-less states cannot use the Windows commands.
Ordinary Windows Restore remains gated. macOS uses its existing actions.

The shared native picker selects a backup and verifies it before the
panel offers confirmation. The backend retains its digest behind a
one-use token, revalidates the archive on confirmation and runs the
existing supersession and protected Pending publication. Cancellation
or rejection does not report success. After acknowledgement the shell
asks the user to quit and reopen MNO. Startup admission remains the
only route to writable Ready.

Frontend tests model picker replies, changed-archive retry, publication,
localization and the explicit Quit request. Windows tests exercise
state eligibility and token consumption alongside the existing native
supersession tests. Actual Windows desktop IPC, picker and Quit/relaunch
remain subject to desktop acceptance. Existing filesystem and
power-loss limitations still apply.
