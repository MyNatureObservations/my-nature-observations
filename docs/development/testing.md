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
