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
