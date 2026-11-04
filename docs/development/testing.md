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
