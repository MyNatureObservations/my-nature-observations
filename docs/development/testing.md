# Local checks and CI

## Tools and scope

The initial CI baseline is Node.js **26.12.1**, Rust **1.99.1** with rustfmt,
and the standard GitHub **macos-15** runner. Use these versions locally when
comparing a failure with CI. Install Xcode Command Line Tools for macOS Rust/Tauri
builds. The Node distribution supplies npm; CI logs all tool versions.

Use `npm ci` and Cargo's `--locked` flag to retain the committed dependency locks.
Run the commands below from a Git checkout. Tests use synthetic fixtures and
temporary databases; no running MNO app or personal journal is required.

## Frontend

From the repository root:

```sh
cd app
npm ci
npm run i18n:compile
npm run check
npm test
npm run build
```

`npm test` runs the frontend test files explicitly listed in the `test` script in
[`app/package.json`](../../app/package.json). That list, rather than a fixed file
count, defines the suite used locally and in CI. Historical prototypes such as
`tests/recovery-startup.test.mjs` are not included automatically. Add a newly
committed test file to the script when introducing it.
The tests use Node's test runner, TypeScript and Svelte already in the project.
They do not replace manual tests of native dialogs, keyboard interaction or Tauri.
Compile translations before tests/checks in a fresh checkout.

## Rust

From `app/src-tauri`:

```sh
cargo test --locked --lib
cargo test --locked --test database_crash --test database_safety --test descriptor_owner --test recovery_handoff_prototype
cargo fmt --all -- --check
```

The library suite covers production data paths, compatibility, backups, open
exports/imports and Recovery, including temporary-database subprocess tests.
The selected integration targets cover database crash/transaction safety and
isolated descriptor-ownership and Recovery-handoff models. Some tests require
macOS and `/private/tmp`; this initial CI job therefore runs on macOS only.

From the repository root, check uncommitted whitespace with:

```sh
git diff --check
```

### Historical managed-target probe: explicit exclusion

`tests/managed_target_probe.rs` and its model are retained unchanged. The test
`reserved_lock_must_survive_closing_another_descriptor_bound_target` reproducibly
fails: closing an independently held descriptor releases the prototype's
process-level POSIX file lock, even though SQLite's local bookkeeping still
reports a reservation. A child process then acquires a write transaction.

This is a standalone historical model, not the current production implementation.
CI selects named integration targets rather than hiding failures with
`continue-on-error`, changing assertions, or claiming that unrestricted
`cargo test` passes. Reproduce the excluded case separately with:

```sh
cargo test --locked --test managed_target_probe reserved_lock_must_survive_closing_another_descriptor_bound_target -- --exact --nocapture
```

Resolving or retiring this model requires a separate decision. Its exclusion
must remain visible when reporting test coverage.

## GitHub Actions

`.github/workflows/ci.yml` contains one job on `macos-15`. It runs on pull requests
targeting `main`, pushes to `main`, and manual dispatch. Named steps run the checks
above, cache npm/Cargo downloads and Rust build artifacts, and cancel superseded
runs for the same workflow/ref. Only repository read permission is requested.
There is no publishing, deployment, signing, or release build.

The whitespace step compares the PR with its base, or a main push with its
previous commit; manual runs check the selected tip commit. It also checks that
the commands have not modified tracked files.

After review, the first authorized push/PR must produce a real green Actions
run. Inspect the runner/tool versions, each named step, and that the expected
frontend/library/integration targets ran without being silently skipped. Manual
dispatch becomes available once the workflow exists on the default branch.
A push to a feature branch alone does not trigger this workflow.

Passing this job establishes macOS code-quality coverage, not Windows/Linux
support or validated installation packages. Those checks belong to issue #11.

## Native development environment

`cd app && npm run tauri dev` automatically uses the separate DEV application
identity and persistent root. Production builds retain their original identity.
For exact paths, isolated-profile rules and manual acceptance steps, see
[Development and Production environments](MNO_Development_Production_Environments.md).
Do not use a debug build with the Production identifier, even with a test override;
it now fails before plugin or database startup.

## Bird observations: macOS schema-upgrade DEV acceptance

**Accepted within this DEV scope**, based on the project owner's manual report,
recorded on 2026-11-12. Tested source:
`e049c118cf46ff11bf98837964e008a72f0dbabc` on
`codex/bird-observations-persistence`.

- A genuine schema-4 journal contained synthetic text, a photo, a place and weather.
- Ordinary macOS startup upgraded it to schema 5 and preserved that content.
- The independently retained schema-4 copy matched the original SHA-256:
  `2383faaa9d087c391f1b14cf43d4af25caa29d7d71569bd3c0a823742d39d426`.
  This identifies the tested source database/copy, not an application installer.
- Restart preserved the content and left exactly one migration copy.
- A fresh journal opened normally and used schema 5.

This is macOS **DEV acceptance**, not packaged-release acceptance, bird-observation
UI acceptance, or proof of physical power-loss durability. Observation UI and
creation commands remain unavailable. Windows/Linux retain their existing schema
behavior and feature gates; this report does not establish native acceptance there.

Recorded local verification for the tested checkpoint: 957 Rust library tests
passed, 0 failed and 2 remained ignored; focused startup/persistence, Export,
Backup and photo regressions passed. All-targets compilation, Rust 1.99.1
formatting and `git diff --check` passed. These results are reused for this
acceptance-only documentation update; the tests were not repeated.

The automated older-backup Restore test also checks that unfinished Recovery
blocks upgrade, the replaced observations remain protected, and bird writes are
rejected in the restored schema-4 state until the next ordinary startup upgrades
it. This is automated evidence, not an additional manual desktop acceptance claim.

## Bird observations: accepted macOS DEV flow (2026-11-13)

The owner accepted the following behavior in the existing isolated synthetic
profile on `codex/mac-bird-list-flow`. These are user-reported macOS **DEV** checks,
not packaged-release acceptance or an exhaustive failure/concurrency campaign:

- Creating, reopening and restarting with saved lists; independent visits at the
  same place/date, with and without optional visit times.
- Editing saved lists, cancelling changes and deleting complete lists.
- Compact journal previews and the approved species-card visual presentation.
- Species-row comments, remembered name-display preferences and corrected search.
- The final cohesive time field/picker and Swedish crow-name presentation,
  including the explicitly explained species-level referral from gråkråka.

Automated evidence is separate: synthetic schema upgrades/source preservation and
retry; format-9 and supported older Backup/Restore and Export/Open Import paths;
atomic rollback, stale generations, multiple links, late edits, real catalog/search
and command-contract tests, localized rendering, and keyboard/controller checks.
Loss of an IPC response is modeled/injected; it is not a physically induced native
WebView transport failure. Power-loss durability and Windows/Linux bird-feature
activation are not established by this acceptance.

Final local checks against base `748ea0198447844026888d4d6e36142f6958fa7f`
plus all uncommitted work: complete frontend suite **600 passed**; complete macOS
Rust library suite **978 passed, 0 failed, 2 existing ignored** (manual synthetic
photo measurements and the opt-in live-weather HTTPS contract). Svelte check:
**0 errors, 0 warnings**. All-targets compilation, Rust 1.99.1 formatting check and
`git diff --check` passed. Each final complete suite was run once.

Historical review blocker (subsequently corrected locally in the linked flow
record): an isolated synthetic replay of production creation after
confirmed deletion recreated the same list and its journal link in the same
generation. This was tested only in a temporary copy of the public source, without
changing checkout code or the user's profile. The passing existing suites do not
cover this create/delete/replay sequence. The correction is documented separately and remains uncommitted. See [the reviewed flow record](MNO_Bird_First_Mac_Flow.md) for the finding
and implementation scope.

### Durable creation replay correction: local verification

The follow-up schema-9 / archive-10 correction passed **601 complete frontend
tests** and **983 complete macOS Rust library tests**, with no failures and the
same **2 existing ignored tests**. The focused receipt regressions use production
reservation/create/edit/delete functions, reopen databases, exercise rollback and
upgrade interruption, and activate current/older archives. Svelte reported **0
errors / 0 warnings**; all-targets compilation, Rust 1.99.1 formatting and whitespace
checks passed. The create → delete → replay blocker is resolved within this tested
scope. The earlier accepted DEV checks remain valid observations of that earlier
build. The owner subsequently reports that the remaining schema-9 DEV flow works;
this is a user-reported DEV result, not packaged acceptance. Species selection
remains pending: the owner reports two clicks are required even immediately after
typing in the active MNO window.
All work remains uncommitted; the user's test profile was not opened or changed.


### Known non-blocking species-result interaction

The owner confirms the latest full-row hit-area change did **not** fix the
reported macOS Tauri DEV behavior: clicking the species name selects immediately;
clicking the blank right-hand area of the same visible result still requires two
clicks. This remains unaccepted, is recorded as **UX-005** in
[public UX backlog #4](https://github.com/MyNatureObservations/my-nature-observations/issues/4),
and does not block the local checkpoint. Further speculative fixes are paused.
Passing isolated browser/native-WebKit checks do not establish a fix in the actual
Tauri window. The existing test profile is preserved.

The final combined-branch review found no remaining concrete correctness or
preservation blocker after the schema-9 receipt correction. It reuses the complete
601-frontend / 983-Rust passing results above. Only subsequent UI checks were run:
45 focused bird tests, one explicit real-rendered browser regression and Svelte
(0 errors / 0 warnings), all passing. See the
[final checkpoint review](MNO_Bird_First_Mac_Flow.md#final-checkpoint-review-and-known-ux-limitation-2026-11-13)
for scope, limitations and the distinction between DEV and packaged acceptance.


## Cross-platform Stage A1: compile/test visibility

The additional CI matrix uses `ubuntu-24.04` and `windows-2022`, retaining the
existing macOS jobs unchanged. It uses the same pinned Rust toolchain and
`Cargo.lock`. Linux installs WebKitGTK 4.1 and native build dependencies; Windows
uses the runner's MSVC tools. No Tauri app is launched.

Cargo is invoked directly, without the Tauri build/package hook or the macOS-only
notice generator:

```sh
cargo check --locked --all-targets
cargo test --locked --lib
cargo test --locked --test database_crash --test database_safety --test descriptor_owner --test recovery_handoff_prototype
```

`--all-targets` checks host package targets, not other operating-system triples.
It compiles the historical managed-target probe but does not run that excluded
model. Library and integration test steps are attempted independently after
successful environment/toolchain setup, even if a preceding Cargo step fails.
Failures are not masked. The job summary separates step outcomes; use the actual
compiler diagnostic or failing assertion to classify the cause. A test-compilation
failure means the tests did not execute. macOS-only tests, including the entire
`descriptor_owner` target, remain gated; zero tests, ignored tests and skipped
steps do not establish platform coverage.

Frontend tests remain in the existing macOS job; they are not duplicated here.
These jobs do not enable Restore/Import/Recovery, change durability guarantees,
or generate Linux/Windows release packages. A1 may expose existing compilation
or test failures; report exact errors before authorizing a separate fix.

The hosted baselines are separate from manual acceptance environments:
macOS Apple Silicon, Ubuntu 26.04.1 LTS x86_64 and Fedora KDE 44 x86_64 in Proxmox,
and a physical Windows 11 laptop. No manual compatibility acceptance is implied.

For a branch without a pull request, maintainers can start the existing CI
workflow with `workflow_dispatch` targeting that branch. A branch push alone
retains the current main-only push trigger. Do not start a second run or poll
for completion when handing the first run over for review.

## Ubuntu desktop smoke tests

Both manual tests used Ubuntu 26.04.1 LTS on x86_64 with a Wayland session.

- **L2 DEV smoke test** (`a6a6664153047cc948f71d4c289513d02008bb35`).
  The app launched with isolated DEV data. A synthetic journal entry was saved
  and reopened after restart. Backup and Export were exercised in this test.
- **L3.2 installed package smoke test** (MNO 0.1.0 amd64 `.deb`, build commit
  `06a8b2058a86ad84192e70f53c2327b5b561f06d`). Inspection of the local package
  confirmed the amd64 package and the bundled MNO MIT license and third-party
  notices. The extracted notices passed Linux-target resource validation.
  The installed app completed a clean first launch with isolated production
  data. A synthetic journal entry was saved and remained available after
  restart. WebKit used a separate directory outside the journal directory.
  Backup and Export were also exercised successfully in the installed app.

The package result covers first launch, persistence, WebKit separation,
Backup and Export on this Ubuntu VM. Other distributions, architectures
and desktop sessions are outside this result.

### Ubuntu 0.1.0 Preview public-download test

The amd64 `.deb` was downloaded directly from the
[public Ubuntu Preview pre-release](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.0-ubuntu-preview).
The downloaded package matched the published SHA-256 manifest
(`eef7637c6f3fdb4a6a2deb2012c568e0f83a683c07a07e1ebbdc2f4baaf8b3e7`).
Reinstallation from that public `.deb` succeeded. After relaunch, the existing
synthetic test entry was still available in the isolated journal under the
test user's home directory. This was a same-version reinstall.

## Fedora KDE 44 RPM smoke test

The local My Nature Observations RPM reported version `0.1.0`, release
`1`, and architecture `x86_64`. Its metadata declared WebKitGTK 4.1
and GTK 3 runtime dependencies. The extracted bundle passed Linux x64
notices validation. The reported build commit is
`f84935e35fd557f7f5de70bf4b68cb1afb169b30`. It was not independently
verified from the Fedora checkout.

On the Fedora KDE 44 machine that built the RPM, the installed app passed
a clean first launch and opened the offline third-party notices from
About. A synthetic journal entry persisted after restart. Backup and
Export both succeeded through the desktop file pickers.

The same RPM also installed and launched successfully on a separate
Fedora KDE 44 x86_64 VM restored from a snapshot taken before MNO and its
build dependencies were installed. This verifies installation and first
launch on that baseline. The journal restart, Backup and Export checks
were performed on the build machine.

### Fedora 0.1.0 Preview public-download test

On the separate Fedora KDE 44 x86_64 VM, the RPM was downloaded directly
from the [public Fedora Preview pre-release](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.0-fedora-preview).
The downloaded RPM matched the published SHA-256 manifest
(`5a34415a0f9588b8dc67a34c846b461bdace7cda7b020d5ad0b772b9054df3cb`).
The same-version reinstall via DNF succeeded. After launching MNO again
from the KDE application menu, the existing synthetic test entry was
still available.

## C1: isolated restore candidate construction

The existing `backup::restore::prepare_candidate` / `prepare_in` boundary validates
an MNO backup and extracts a separate private, verified SQLite snapshot. It does
not accept an active-journal location or grant activation/startup authority.
`try_close` retains the private files after checked SQLite closure; dropping that
owner cleans staging. This is temporary candidate ownership, not a durable
journal-generation protocol or a migration to the current schema.

Run the focused contract tests from the repository root:

```sh
cargo test --manifest-path app/src-tauri/Cargo.toml --locked --lib candidate_builder
```

These ungated library tests run in the existing macOS/Linux/Windows jobs. They
cover schema-4/backup-v5 content (stored image, manual/fetched weather, places and
default place), repeated construction, checked close/read-only reopening, unchanged
inputs and a separate damaged journal with sidecars, truncation and injected
mid-extraction disk/permission errors. Existing restore tests cover tampering,
unsupported schemas/documents, cancellation, resource limits and WAL-source backup.
Injected errors do not establish real filesystem/ACL failure coverage. The current
fixture uses the existing database constructor directly, not application startup;
Windows initialization/durability rejection and the known Windows baseline failures
remain unchanged. Cross-platform success must be confirmed from CI; these tests do
not enable Restore/Import/Recovery or establish power-loss durability.

## C2: isolated generation-selection prototype

`tests/generation_activation_prototype.rs` is a standalone exploratory protocol,
not production startup, recovery, a journal-generation format or Ready authority.
It uses the existing `fs2` OS-lock primitive and `tempfile` no-clobber publication,
plus synthetic SQLite files. Private production `DirectoryLease` and state/proof
APIs are not exposed or copied into this test. C1 production content validation
would remain required; the prototype's integrity check is not its replacement.

The pre-created private root contains `generations/<uuid>/journal.sqlite`, an
existing lease file and append-only `selections/select-NNNNNNNN.json` records.
Version-1 prototype records bind a generation digest and the previous record's
hash. An explicit first selection is permitted; startup without a selection never
creates a journal. Startup validates a contiguous bounded chain (100 records),
then only the selected candidate, so damage to an older generation does not block
restoration. No generation, old sidecar or published record is deleted. The cap
is a prototype bound, not a proposed product retention policy.

Candidate checks and file/directory barriers precede publication; a verified record
is published with `persist_noclobber`, followed by selection/root directory barriers.
Publication errors never mean success or imply that the old selection survived.
Restart revalidates and repeats barriers before selecting. A process exit before
publication leaves an unknown `pending-*` record: startup blocks without cleanup.
An exit after publication can select the new generation after verification/barriers,
even if the original request never returned success. User intervention for ambiguous
states is intentionally not implemented. No fallback to an arbitrary older record.

```sh
cargo test --manifest-path app/src-tauri/Cargo.toml --locked --test generation_activation_prototype
```

Both existing selected-integration steps include this target. Protocol-only tests
explicitly use modeled barriers; they do not prove persistence. Real Unix tests use
macOS file `F_FULLFSYNC` / Linux `sync_all`, and directory `sync_all`. Windows real
barriers remain fail-closed (including unsupported directory synchronization);
the Windows-only sharing test exercises append-only selection while old files are
held without delete sharing. It does not establish Windows flush durability.

Subprocess tests exit at before/after-publication boundaries with bounded waits.
They prove process-interruption behavior only, not power-loss/torn-write safety.
Sync-failure injection covers required file/directory points. Root establishment,
C1 handoff, application-wide checked closure, active same-user path substitution,
filesystem identity/ACL hardening, physical power loss, and generation cleanup UI
remain outside this prototype. Production integration needs those separate proofs
and manual APFS/ext4/NTFS acceptance; no platform guard or production policy changes.

C2a inventory snapshots include the fixture's empty lease file by asserting its
regular-file type and zero length, rather than reading it through a second handle
while its exclusive byte range is locked. Windows correctly rejects that read
with error 33. All other file reads remain strict. A regression test checks snapshot
contents, lease exclusion before/after inventory, the Windows read rejection, and
successful reacquisition only after the owning lease closes. No lock is relaxed.

## C3: admission versus mutable startup (isolated extension)

`tests/generation_activation_prototype/admission.rs` extends the existing C2 test
target without replacing or weakening its tests. C2's chain-only startup remains
a historical model; only C3's entry points exercise the authoritative-head contract.
There is no production API or startup integration.

C1 owns a prepared immutable candidate. C3 validates its original digest before
publishing intent and again before first admission. A single versioned
`authority.json` binds the exact latest selection record hash/sequence and either
`awaiting_admission` or `admitted`. Intent is published BEFORE the append-only C2
selection, so interruption may deliberately leave an authority/record mismatch:
startup blocks; it never infers authority from the older valid chain prefix.
Missing/corrupt authority, missing latest records, gaps and conflicting hashes block.
The authority is replaced using `tempfile::persist` with explicit file/root barriers;
this atomic replacement is not a substitute for durability.

An awaiting candidate is reverified against its original activation digest and
returned only as `AwaitingAdmission`, never as a mutable journal. Explicit admission
rechecks that digest and publishes the admitted head; no mutable path is returned
until required barriers succeed. Once admitted, ordinary startup verifies CURRENT
synthetic SQLite integrity/standalone-file state rather than the original digest.
Legitimate edits and checked closure do not rewrite authority or selection records.
C1's complete MNO content validation is still required for a production integration.

The admission publication is the logical commit point, distinct from acknowledgement.
If a process exits after admitted-head publication but before acknowledgement, a new
startup can recognize admitted state only after fresh verification and successful
barriers. An intent with a matching selected record remains awaiting admission.
Missing records or leftover authority/selection temporaries remain blocked across
repeated startups; no automatic repair, rollback, empty DB creation or old-generation
cleanup occurs. Previous databases and sidecars remain untouched, even if damaged.

Tests cover checked edit/restart, strict rejection of edits BEFORE admission,
missing/truncated/conflicting state, authority sync failures, bounded subprocess
interruptions at intent/selection/admission boundaries and real Unix barriers.
Windows protocol tests explicitly model barriers; real directory synchronization
continues to return Unsupported. A Windows-only sharing test checks that an open
head without delete sharing prevents replacement without damaging the prior state.
All tests run through the existing selected-integration matrix target.

Limits: one local authoritative file detects partial history loss; it cannot detect
a coherent rollback/replay of BOTH authority and history, nor prove the latest
acknowledged head survived physical power loss. Atomic replacement, directory
persistence and NTFS durability still need platform-specific acceptance. Lease/path
identity, hostile substitution, WAL/hot-journal ordinary startup, durable root setup,
legacy adoption and explicit repair UX remain outside this prototype. The existing
100-record bound remains experimental. Process exits are not power-loss tests.

### C3a: lease owner lifetime across Unix process spawning

The prototype now explicitly unlocks in `Lease::drop`, matching the release
pattern already used by production `startup::DirectoryLease` (unchanged).
`fs2` uses Unix `flock`: a forked child's inherited open-file description can
retain the lock after the parent's descriptor closes, until exec closes it or the
child exits. Close-on-exec alone does not cover that transient pre-exec window.
This caused a prototype owner-release/liveness defect, not overlapping admission
or a failed exclusive-lock check. The prototype aborts on unlock error; it never
silently reports release. An acquired lease gets its RAII owner before subsequent
validation so error exits follow the same release rule.

The Unix-only `lease_lifecycle` regression uses a controlled fork and bounded
socket handshakes. The child performs only async-signal-safe syscalls and `_exit`.
A negative control proves plain descriptor closure retains the inherited flock;
lease drop must allow a single immediate acquisition while that child is alive.
Competing owners remain excluded before and after transfer. No sleeps or lock
retries are added. Child termination/reaping is guarded on test failures. Windows
continues to run all applicable C2/C3 lock-exclusion and sharing tests; fork-specific
coverage applies only to Unix. Existing identity/durability limitations remain.

## C4: single-record comparison prototype

`tests/generation_activation_prototype/single_record.rs` is an isolated alternative
in the same integration target. C2/C3 implementations/tests remain unchanged. It
reuses their native lease/explicit unlock, strict candidate verifier, SQLite integrity
and checked-close checks, synthetic fixtures, barriers and injection hooks.

One `single-selection.json` contains format/version, generation UUID, pending/admitted
phase and original candidate digest. There is no history, sequence chain or independent
authority witness. Selection validates/synchronizes a standalone candidate, then
publishes a pending record. Explicit admission revalidates the ORIGINAL digest and
publishes admitted state. Ordinary admitted startup validates CURRENT synthetic SQLite
integrity without comparing the original digest or rewriting the record after saves.
C1's real backup/content validation is not replaced by these synthetic integrity checks.

Publication uses a same-root temporary, file barrier, `tempfile::persist` replacement
and directory barrier. A leftover `single-pending-*` temporary blocks startup even if
an old record remains valid. Missing, malformed, truncated, duplicate-field, unsupported
or unresolved records block; startup never guesses from generation directories or
creates a DB. Missing/invalid selected generations or unaccounted sidecars also block.
A record published before acknowledgement can survive a failed operation: restart
reverifies/reconfirms barriers and honors its pending/admitted phase, never presumes
the previous record survived. Permission/sharing failures remain errors.

**Approved limitation:** the replay test restores an older internally valid admitted
record and explicitly confirms that the older generation is selected. Without independent
freshness evidence the protocol cannot detect this rollback. This is not corruption
recovery or permission to fall back automatically when selection is missing/invalid.
All generation files remain preserved; no deletion UI or cleanup policy is implemented.

| Comparison | C2/C3 | C4 |
| --- | --- | --- |
| Persistent selection metadata | N history records + authority | One record |
| Logical phases | Awaiting admission / admitted | Pending / admitted |
| Publications per completed selection/admission | Pending authority + history + admitted authority | Pending record + admitted record |
| Startup dependencies | Authority, full valid chain, selected DB | Record, selected DB |
| Extra inconsistency cases | Gaps, head/history mismatch, 100-record cap | No cross-file metadata relationship |
| Freshness limitation | Coherent authority/history rollback undetected | Valid older record replay undetected |

The existing fixture's empty `selections/` directory is unused by C4; tests assert no
history/authority files are written. Tests cover edits/restarts, strict pending digest,
record/generation faults, preserved damaged old generations/sidecars, injected permission
and sync failures, admission failures, subprocess exits before/after publication and
admission acknowledgement, real Unix barriers and Windows delete-sharing denial.
A synthetic backup sentinel is untouched because C4 does not open archives; C1 remains
the actual .mno-backup preservation/validation test coverage.

This meaningfully simplifies metadata handling and failure cases, NOT platform durability
or identity work. Windows real directory durability remains Unsupported; modeled barriers
are not persistence evidence. Subprocess exits are not power-loss tests. Existing limits
on path substitution, private-root establishment, WAL/hot-journal reopening, production
validators, legacy adoption and recovery UX remain. APFS/ext4/NTFS acceptance is separate.

## C5a: isolated Windows initialization persistence probe

`backup::windows_initialization_probe` is compiled ONLY under `cfg(test)`. It reuses
the production current-schema constructor, read-only schema/content validation,
checked SQLite close and existing private-directory ACL helper. It never enables
startup or returns Ready: successful file operations yield the explicit observation
`FilesVerifiedDirectoryPersistenceUnestablished`.

The portable test checks current schema, synthetic content, no-clobber construction
and independent reopening. Windows tests require NTFS (checked through
`GetVolumeInformationByHandleW`) and use only private temporary directories.
They examine writable-handle `File::sync_all` / `FlushFileBuffers`, read-only flush
rejection, concurrent exclusive creation, and marker publication using
`MoveFileExW(MOVEFILE_WRITE_THROUGH)` WITHOUT replacement/copy flags. No delete-then-
rename fallback exists. Temporary marker handles close before publication. Ordinary
failure cleans only owned temporaries; sharing-denied cleanup can leave evidence.
Published markers, incomplete databases and interrupted temporary evidence block
probe reinitialization. At C5a, production initialization remained unsupported.
C5c below supersedes that restriction only for checked local-NTFS establishment;
the isolated probe observation still does not itself grant production Ready.

Coverage includes late destination creation, source delete-sharing denial, injected
write/flush/permission errors, process exit before/after marker publication and schema
creation, repeated read-only reopening, missing/zero-length established DB, retained
sidecars/backups, and a synthetic directory junction rejected as a reparse point.
Junction creation uses standard Windows `mklink /J`; failure is a test failure, not a
skip. The subprocess checks are bounded and do not establish physical power-loss safety.
Injection before a write/flush is not a real storage-device failure test.

For nested private directories the probe prints actual write-capable directory-open
and `sync_all` outcomes at EACH ancestor within that private root. Both success and
denial are observations, not an approved directory-persistence primitive. No volume
handles, administrator privileges, driver or global flush are used. The existing
application-data ancestors outside the private root are not covered by these tests.

Documented API facts: FlushFileBuffers requires GENERIC_WRITE; CreateDirectoryW
creates one directory level, and directory handles use FILE_FLAG_BACKUP_SEMANTICS.
MOVEFILE_WRITE_THROUGH has documented completion semantics, but this probe does not
infer a complete nested-directory/power-loss guarantee from rename success. NTFS
namespace persistence and ordering remain a production blocker until a justified
platform contract and acceptance evidence exist.

References:
- [FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
- [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
- [CreateDirectoryW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createdirectoryw)

Local portable command:

```sh
cargo test --manifest-path app/src-tauri/Cargo.toml --locked --lib windows_initialization_probe
```

The existing CI workflow has an opt-in manual `scope=windows-initialization-probe`.
It runs ONLY the Windows probe (`--nocapture` records diagnostics) and production
Windows startup safety tests, using the normal Rust pin/cache. Default full CI,
push/PR behavior remains unchanged; the historical 12-failure baseline must be compared
with the new full run. A focused
green run is not full-suite coverage or production Windows readiness. Use full CI at
major integration boundaries, not for this isolated probe.

## C5c: production Windows establishment on local NTFS

Windows ordinary startup now uses `startup/established/windows.rs`. Existing Unix
establishment functions and their ordering are unchanged. Historical statements
above about Windows initialization rejection describe earlier stage baselines.
Migration, Restore/Recovery, retained recovery-marker support and generation
activation guards are NOT enabled by C5c.

Before creating directories, Windows captures the nearest existing containing
ancestor. It validates local-drive/NTFS support and retains a write-capable,
no-delete-sharing directory handle. After creation it retains checked handles
for each new level through that ancestor. For an existing application root the
boundary is its containing directory. Synchronization goes deepest-first and
stops at that boundary, never blindly at the drive root/system ancestors. Handle
identity and reparse checks stay mandatory. Unavailable permissions, unsupported
filesystems/remote paths and synchronization failures are errors, not fallbacks.

All newly created directory edges are flushed BEFORE publishing initialization
evidence. Marker bytes/version stay unchanged. A flushed same-directory temporary
is closed and published using MoveFileExW(MOVEFILE_WRITE_THROUGH), without replace
or copy flags. Published evidence is retained if subsequent barriers fail.
Marker confirmation and new-database flushes use checked write-capable handles.
No deletion/rename fallback, privileged volume flush or automatic sidecar cleanup.
After exclusive file reservation, transactional schema creation, checked SQLite
closure and persistence barriers, Windows independently invokes the current
read-only schema/content validator before returning Ready.

Windows startup tests cover nested boundary limits, first/repeated startup,
conflicting owners, no-clobber destination races, corrupt/missing marker/database,
retained sidecars/recovery artifacts/backups, injected write/file/directory failures,
post-publication failure, real sharing denial, reparse and nonlocal paths, subprocess
interruptions, final database-flush denial and invalid new content rejected at the
independent-validation boundary. Existing Unix startup tests remain unchanged.

Targeted commands (repository root):

```sh
cargo test --manifest-path app/src-tauri/Cargo.toml --locked --lib startup::windows_tests
cargo test --manifest-path app/src-tauri/Cargo.toml --locked --lib startup::tests
```

Run full macOS/Linux/Windows CI for this production integration boundary. Do not
skip historical failing Windows tests: initialization-dependent fixtures may now
advance to separate schema/migration restrictions. Classify exact CI diagnostics
rather than assuming all 12 prior failures disappear. The unrelated compiler-fixture
and weather-timeout issues are not changed here.

The practical persistence contract relies on checked OS flush/publication calls,
local NTFS and storage honoring flushes; it is not an absolute power-loss guarantee.
C5b.1 manually verified unelevated Windows 11/Roaming AppData directory operations,
not application acceptance. After CI, use an isolated Windows user/test environment:
first launch, save/close/reopen synthetic entries, second-instance exclusion,
restart/error behavior and retained backup/sidecar evidence. Do not use real journal
data or delete it to trigger a first launch. C2-C4 remain test-only reference models.


## C6a: internal Windows NSIS test packaging

Manually dispatch `CI` on `codex/cross-platform-ci` with
`scope=windows-test-package`. This runs only the Windows packaging job, not full
CI. It uses windows-2022, the existing Node/Rust pins, cargo-about 0.9.3 and locked
npm/Cargo dependencies. Existing full CI and focused initialization scope remain
available. C5c's Windows baseline is 353 library tests passed, four known failures
and two ignored; packaging neither reruns nor resolves those failures.

The production name, identifier and version remain unchanged. The explicit
`app/src-tauri/tauri.windows-test.conf.json` overlay selects current-user NSIS and
zlib compression. The ordinary macOS config is unchanged. The application is
unsigned; SmartScreen or organizational policy can warn or block it. WebView2
uses Tauri's existing Evergreen bootstrapper behavior and may require a download.
No application or installer is launched during CI.

Notices use a separately reviewed `native.windows.json` and the actual
`x86_64-pc-windows-msvc` Cargo graph, not the macOS target inventory. Shared Rust
source/runtime license evidence is target-independent and conservatively includes
source components that may not survive linking. Windows-specific crate exceptions
pin package checksum and VCS revision. The WebView2 static loader is byte-matched
to Microsoft SDK 1.0.3800.47 and its BSD notice is included. MSVC 14.x/Visual Studio
2022 runtime terms remain separate from MNO's MIT license and are displayed by the
installer. Native NSIS zlib-stub and Tauri helper notices are included. Toolchain,
SDK or CLI changes require review; a missing license/provenance check fails the
build. These checks are an internal packaging boundary, not a new legal certification.

The job extracts the NSIS archive with 7-Zip without executing it, verifies the
packaged application against the x86_64 build with only the exact Tauri NSIS
bundle-token patch allowed, plus metadata and the notice tree,
and uploads `mno-windows-x86_64-nsis-test-<full commit SHA>` (14-day retention).
Open the successful run's **Artifacts**, download that artifact and extract its
ZIP. It contains the `.exe`, `SHA256SUMS.txt` and `BUILD-INFO.json`. A failed job
must not be treated as producing an accepted installer. No Release/tag is created.

### Mandatory isolation and first-launch acceptance

This build uses `%APPDATA%\org.mynatureobservations.app`, the **production data
location for that Windows user**, and ignores test-data overrides. Use a separate
ordinary Windows test account, local NTFS/Roaming AppData, and synthetic data only.
Do not install/run it under an account containing a real MNO journal. DEV data
separation remains unchanged; this package does not use the DEV identity.

1. In the separate account, confirm the production data directory is absent; if
   present, stop rather than delete it to force first launch.
2. Download/extract the artifact; compare `Get-FileHash -Algorithm SHA256` for the
   installer with `SHA256SUMS.txt`. Record Windows/SmartScreen warnings; do not
   disable security protections or elevate to bypass a failure.
3. Install for the current user, without administrator elevation where supported.
4. Start normally. Confirm no startup error, create/save synthetic entries, close
   normally, restart and verify their contents.
5. Attempt a second launch: it should focus the existing window rather than start
   a competing journal owner. Close/reopen again and verify entries remain intact.
6. Record any startup/application error or unexpected data deletion/replacement.
   Keep failed-state evidence; do not remove markers or sidecars to force startup.

Do not exercise Restore/Recovery, historical migration or experimental generation
activation. Successful hosted packaging does not establish unelevated Windows 11
runtime acceptance or physical power-loss durability. Do not change published v0.1.0.

### C6b: NSIS executable integrity comparison

Tauri CLI 2.12.2 (bundler 2.10.2) patches the first
`__TAURI_BUNDLE_TYPE_VAR_UNK` token to `__TAURI_BUNDLE_TYPE_VAR_NSS` before NSIS
packs `app.exe`, then restores the unpatched release executable. Source:
[the pinned bundler implementation](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle.rs).
C6a's raw comparison across these stages therefore rejected the intended patch.
The failed run logged that patch, but did not upload binaries or per-file hashes;
the complete byte-level explanation must be confirmed by the next packaging run.

`verify-nsis-executable.mjs` constructs the exact expected NSIS bytes in memory
from the original release binary. It requires full byte equality with exactly one
extracted `app.exe`; no PE headers, signatures, code or resource regions are ignored.
Missing tokens, unexpected edits and size changes fail. Source files are never
rewritten. Both paths, sizes, original/expected/packaged SHA-256 and patch offset
are logged and included in successful artifact build metadata. Packaged PE product
metadata is also checked; existing architecture, icon and notices checks remain.
Focused synthetic-buffer tests run locally and in the packaging job. They test
comparison behavior, not actual Windows executable production or runtime acceptance.

## C6 manual Windows 11 acceptance

**Accepted within the intended C6 scope**, based on the user's manual report using
an unsigned NSIS test installer from
[CI run 41913220191](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/41913220191),
source commit `7308677c9951c33f0f837b3ca6037c557328dc32`. The completed packaging
run was independently checked once and reports success; its automated artifact
verification is separate from the following manual observations.

- NSIS installation succeeded after moving the installer to a shorter filesystem
  path. This records a practical path-related observation, not a proven cause or
  complete installer path/long-path compatibility result.
- The application launched successfully on a real Windows 11 machine.
- A new journal entry could be created and saved.
- The saved entry remained accessible after closing and restarting the application.
- Attempting another launch did not open a second independent session. This checks
  the observed single-instance behavior only, not every filesystem lock, concurrent
  initializer, multiple-user/session or abnormal-termination scenario.
- A backup file was successfully created and saved. This does not independently
  prove successful restore, all backup content or every backup failure path.

Restore, Recovery, historical database migration, experimental generation activation
and power-loss durability remain outside C6 acceptance. No exhaustive filesystem,
installation, security-warning or Windows compatibility claim follows. Account
isolation, elevation status and filesystem details were not newly specified in
this acceptance report; do not infer them from application success. The separate
ordinary-account/synthetic-data procedure above remains required for future tests.
C5b.1's unelevated directory-operation evidence remains distinct from C6 observations.

C6 is closed as a stage, not as a claim of complete Windows platform support.
[C7 architecture review and proposed stages](MNO_C7_Generation_Recovery_Review.md)
are documentation only; no C7 production behavior has been implemented.
