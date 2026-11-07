# MNO C8 cross-platform readiness review

Reviewed branch `codex/cross-platform-ci` at `d5818aa`. Verification
below is scoped to the cited automated or desktop evidence. Windows
desktop evidence comes from the [C7.5c retest](MNO_C7_Generation_Recovery_Review.md#c75c-windows-desktop-recovery-acceptance).
Linux evidence comes from Ubuntu 24.04 CI. Fedora KDE and Ubuntu
desktop acceptance are still separate milestones.

| Area | macOS | Windows | Linux |
| --- | --- | --- | --- |
| First launch | **Verified** in the macOS release workflow and startup tests | **Verified** on Windows 11 as a standard user and in native NTFS tests | **Verified in CI** for database establishment. Desktop startup unverified |
| Journal read/write | **Verified** on macOS | **Verified** for legacy and recovered journals on Windows 11 | **Verified in CI** at the Rust/SQLite boundary. Desktop persistence unverified |
| Backup | **Verified** on macOS | **Verified** for manual backup creation on Windows 11 | **Verified in CI** for shared backup code. Desktop picker and output unverified |
| Ordinary Restore | **Verified** through the existing macOS flow | **Gated by default**. The opted-in desktop flow passed [C8b.2](#c8b2-windows-ordinary-restore-desktop-acceptance) | **Unsupported** |
| Blocked-startup Recovery | **Verified** through the existing macOS flow and regressions | **Verified** for the missing-selection desktop fixture. Other failure boundaries have scoped native test coverage | **Unsupported** |
| Installation / packaging | **Verified** for Apple Silicon DMG distribution | **Verified** for a local unsigned current-user x64 NSIS acceptance installer. Public distribution unverified | **Unsupported** by the current reviewed packaging process |
| Automated verification | **Verified** frontend, Rust and arm64 packaging checks | **Verified with baseline exceptions** for Rust compilation, startup, Recovery and selected integration tests | **Verified** on Ubuntu 24.04 for Rust compilation and selected tests |
| Manual desktop acceptance | **Verified** for the published macOS scope | **Verified** for the C7.5c scope. The opted-in ordinary Restore flow also passed C8b.2 | **Unknown** across Ubuntu and Fedora KDE |

## Implementation and evidence

- [Startup and directory ownership](../../app/src-tauri/src/startup.rs)
  acquire the directory lease before resolving or creating a journal.
  Windows uses the existing NTFS establishment path for legacy startup.
  Managed startup validates the selected generation, verifies Pending
  candidates before admission and reopens Admitted generations for edits.
  Blocked managed startup retains the lease. The startup adapter leaves
  the Recovery WebView available without registering journal DataLocation.
  The Windows desktop retest verified this route, including picker,
  confirmation, restart and editing. Native tests cover lease contention
  and injected failures. Neither establishes all concurrency or
  physical power-loss scenarios.
- [Windows managed selection](../../app/src-tauri/src/startup/established/managed_layout.rs)
  and the [protected publisher](../../app/src-tauri/src/startup/established/selection_publication.rs)
  retain prior journals and recovery evidence. The accepted limits for
  cooperating writers, replay of an older valid selection record and
  complete loss of managed evidence still apply.
- [CI](../../.github/workflows/ci.yml) uses Ubuntu 24.04 with WebKitGTK
  4.1 development dependencies for Rust visibility. Linux shares Unix
  establishment and SQLite paths and has a native positional-hashing
  lock regression. Managed startup, publication and Recovery commands
  are Windows-specific. The old macOS descriptor-bound SQLite opening
  cannot be carried over unchanged to Linux, as B2a reproduced.
  A source-driven Linux desktop smoke test is viable in principle
  through the existing Tauri dev entry with isolated DEV identity.
  WebKitGTK, desktop dialogs, map rendering and filesystem behavior
  still need an actual VM check. A packaged build invokes the notices
  generator, whose [target policy](../../app/scripts/notices/policy.mjs)
  currently accepts macOS arm64 and Windows x64 only. Linux packaging
  therefore needs a separately reviewed native dependency inventory.
- macOS retains its existing Restore and Recovery paths. The Windows
  NSIS build keeps the production identifier and uses Roaming AppData.
  Desktop testing requires a separate disposable account with synthetic
  data. Use local Windows packaging for private artifacts, since the
  public CI workflow uploads artifacts accessible to repository readers.
  [Packaging and acceptance record](MNO_C7_Generation_Recovery_Review.md#c75a-internal-windows-installer-preflight-and-acceptance)

## Four Windows baseline failures

| Failure | Classification and priority |
| --- | --- |
| `compiler_rejects_sql_access_and_snapshot_replacement` | Fixture portability defect. The generated Rust source refers to a missing `tests` module (E0583), so the intended negative compile checks are not established on Windows. Fix separately before relying on Windows coverage of that invariant. |
| `refused_connection_remains_a_network_failure` | Platform-dependent transport result in a one-second localhost test. Windows reports Timeout where the test expects Network. The cause of that timing is unproven. Investigate separately as a possible error-message classification issue. |
| `legacy_recovery_reads_absent_weather_and_requires_safe_startup_before_write` | Legacy test helper hits intentionally unsupported Windows schema-1 migration. Production startup rejects older schemas rather than migrating them here. Address when historical migration or fixture portability has its own scope. |
| `legacy_reads_do_not_migrate_and_corrupt_weather_is_not_presented` | Same restricted legacy fixture setup, before its intended weather assertions. A supported-schema Windows fixture could independently test corrupt-weather presentation in a later task. |

Evidence: [C5c logs](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/41894600284),
[desktop-adapter CI](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42241130250),
and [baseline comparison](../../.github/scripts/windows-library-baseline.py).
Keep these failures visible and the baseline unchanged for C8b.

## Recommended C8b — ordinary Windows Restore acceptance build

The healthy-journal workflow is the nearest functional gap. C7.4b
already implements the internal Windows path and C7.5c verified the
downstream Recovery and restart behavior on a real desktop. The
[Restore gate](../../app/src-tauri/src/restore_app.rs) still returns
available only on macOS. Enable the existing Windows path for a
local acceptance build through an explicit build-time opt-in.
Keep default Windows builds gated until the desktop result is reviewed.

- **Reuse.** Keep the existing Settings route, native picker, one-use
  digest and generation-bound confirmation, maintenance guard, lease,
  verified candidate builder, protected publisher, Quit command and
  startup admission. Current format-5 backups with the current schema
  already suffice. Older backups and Open Import are outside scope.
- **Minimum changes.** Add the narrow opt-in to the shared availability
  check and require Ready for the Windows picker and confirmation.
  Use existing restart UI with Windows wording that explains previous
  journals are retained. Check the handoff when
  `mno:database-mode-changed` unmounts the Restore route. The shared shell
  must distinguish acknowledged Pending from a publication error and
  retain a controlled Quit action. Keep macOS behavior intact.
- **Invariants.** Before confirmation, leave authority and existing
  files untouched. After confirmation, drain journal operations and
  hold the exclusive lease. Reject changed backups and stale tokens.
  Preserve the backup, legacy database, prior generations and
  interruption evidence. After the write latch, deny stale editor
  writes. Report preparation only after acknowledgement and grant
  writable access only after restart admission. Ambiguous publication
  must block without fallback.
- **Automated acceptance.** Run native Windows tests for default gate
  denial and opt-in Ready-only enforcement, legacy and admitted restore,
  changed archive and token rejection, publication faults and
  preservation, admission and edits after reopening. Exercise the
  production UI event ordering with mocked IPC, including error
  reporting, successful Quit affordance and guarded navigation.
  Retain macOS Restore regressions and compare the Windows baseline.
- **Desktop acceptance.** Build locally and use an ordinary disposable
  Windows 11 account. Restore from a valid current backup first from a
  legacy journal and then from an admitted one. Check picker cancel,
  invalid backup, confirmation and native Quit/relaunch. Verify the
  expected content, edit and restart again. Compare the original backup
  and previously active database hashes and check that prior generation
  directories remain. Verify the default installer keeps Restore hidden.
- **Deferred.** Public Windows distribution and signing, Linux notices
  and packaging, Linux Restore/Recovery, historical migration, Open
  Import, generation cleanup and storage-protocol redesign. Existing
  power-loss and replay limitations remain explicit.

## C8b implementation and local acceptance

Ordinary Windows Restore requires the explicit Cargo feature
`windows-restore-acceptance`. The shared backend gate applies to
both picker and confirmation. Default Windows builds keep Restore
unavailable. The picker requires a Ready journal, and confirmation
uses the existing one-use token, Ready maintenance gate and archive
digest verification. Windows Recovery remains a separate blocked-startup
action using the same candidate and publication architecture.

After a confirmed restore, the backend distinguishes an acknowledged
Pending publication from an uncertain publication after the write
latch. The shared shell offers Quit in both cases and reports
preparation only for acknowledgement. Errors before the latch leave
the journal available and Settings offers a retry or return. Restart
admission alone can grant writable access. Prior databases, external
originals, backups and diagnostic evidence are preserved.

From `app`, using the reviewed local Windows MSVC and notices tooling
and after `npm.cmd ci` and `npm.cmd run i18n:compile`:

```powershell
npm.cmd run tauri build -- --ci --no-sign --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.windows-test.conf.json --features windows-restore-acceptance -- --locked
```

Keep the installer local. Omit the entire feature option for an ordinary
build. Confirm Settings omits Restore in the default build, and use
the focused native default/opt-in gate tests to verify backend behavior.
The feature is not part of any installer workflow defaults.

After review, use the existing disposable `MNO-Test` account and
synthetic data only. Verify the installer checksum and current-user
identity before installation.

1. Open the existing journal, save a distinctive synthetic entry and
   create a backup outside the data directory. Record its SHA-256.
2. Edit that entry and save a distinguishable post-backup change.
   Quit cleanly and record the active database hash while closed.
3. Relaunch and open Settings → Restore. Check cancellation and
   invalid-backup rejection. Select the valid backup, review the
   retained-journal explanation and confirm.
4. Verify the restart-required shell has no editor, and use its Quit
   action. A second instance must not open an independent journal.
5. Reopen MNO. Verify the original text is active and the post-backup
   change is absent. Make a new edit, quit and reopen to verify it.
6. Compare the backup and previous active database hashes, and verify
   the prior generation directories remain. Use the same process on
   a fresh disposable legacy installation if verifying legacy-to-managed
   desktop coverage. Do not alter database or selection files.

Automated coverage checks the gate, candidate validation, native
publication/admission, preservation, fail-closed outcomes and frontend
event-before-reply handoff. The established Windows baseline remains
visible. The [focused Windows run](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42265179086) passed. Power-loss durability remains unverified.

### C8b.2 Windows ordinary Restore desktop acceptance

**PASS within the opted-in desktop scope.** The tester used a Windows 11
x64 laptop, the disposable standard-user account `MNO-Test` and an existing
synthetic journal. The local unsigned NSIS installer was built from
`b1192263` with MSVC v143 `14.43.34808` and the explicit
`windows-restore-acceptance` Cargo feature. The installer SHA-256 was
verified after copying it into the test account:

`7F4F0A4CF5216A22CB0C1FE9DD649B4C114F3A1F09A8BFF98505BDC53FBFBDA4`

The tester opened the existing journal normally and confirmed that
Settings offered Restore. They saved and reopened `C8b – Restore-test`
with Version A, created a backup, then saved and reopened Version B.
They selected the Version A backup through the existing picker, confirmed
with **Prepare restore**, and followed MNO's instructions to exit and
restart. The normal home page opened. The active entry contained Version A
and no Version B text. A new edit saved after Restore persisted across a
second full exit and restart.

After closing MNO, read-only inspection found these directories:

- `a5780f08-4eaa-44f0-b493-1d88c16031d3`
- `cb82a89f-9cd1-4c1a-a62c-8d7683cd6c71`
- `recovery-evidence-a5780f08-4eaa-44f0-b493-1d88c16031d3`

Their presence is consistent with the generation-preservation model.
The listing does not identify which generation contains Version B or
independently establish the integrity of each preserved database.

Default Windows builds remain gated. Manual coverage of all
pre-publication failures, uncertain publication outcomes, interruption
and crash scenarios, and physical power-loss durability remains outside
this acceptance. Windows signing, public distribution and general
production readiness remain separate milestones.
