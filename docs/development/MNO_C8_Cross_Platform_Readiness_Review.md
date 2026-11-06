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
| Ordinary Restore | **Verified** through the existing macOS flow | **Gated**. Generation publication and admission are verified in native tests | **Unsupported** |
| Blocked-startup Recovery | **Verified** through the existing macOS flow and regressions | **Verified** for the missing-selection desktop fixture. Other failure boundaries have scoped native test coverage | **Unsupported** |
| Installation / packaging | **Verified** for Apple Silicon DMG distribution | **Verified** for a local unsigned current-user x64 NSIS acceptance installer. Public distribution unverified | **Unsupported** by the current reviewed packaging process |
| Automated verification | **Verified** frontend, Rust and arm64 packaging checks | **Verified with baseline exceptions** for Rust compilation, startup, Recovery and selected integration tests | **Verified** on Ubuntu 24.04 for Rust compilation and selected tests |
| Manual desktop acceptance | **Verified** for the published macOS scope | **Verified** for the C7.5c scope. Ordinary Restore remains outside it | **Unknown** across Ubuntu and Fedora KDE |

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
