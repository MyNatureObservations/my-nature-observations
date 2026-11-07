# C9a Windows quality and release readiness

Reviewed `codex/cross-platform-ci` at `4855aa6`. Windows 11 desktop
acceptance covers standard-user installation, normal journal use, backup,
blocked-startup Recovery and opted-in ordinary Restore. See the
[C7 Recovery record](MNO_C7_Generation_Recovery_Review.md#c75c-windows-desktop-recovery-acceptance)
and [C8 Restore record](MNO_C8_Cross_Platform_Readiness_Review.md#c8b2-windows-ordinary-restore-desktop-acceptance).
The [focused Windows CI](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42265179086)
passed against its baseline. Windows release work takes priority over
Linux desktop implementation.

## Established Windows library failures

The [C5c logs](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/41894600284)
and [baseline comparator](../../.github/scripts/windows-library-baseline.py)
identify four distinct tests and their diagnostics.

| Test | Cause and production significance | Smallest correction and priority |
| --- | --- | --- |
| `compiler_rejects_sql_access_and_snapshot_replacement` | The generated fixture still contains `mod tests;`. The positive control fails with E0583 before testing the restricted API. The source transformation matches a literal LF, which leaves the declaration in place on a CRLF checkout. The log confirms the residual declaration. The original checkout bytes were not captured. This is a coverage defect in a backup safety boundary. | Make fixture generation line-ending independent and preserve the positive control and exact negative diagnostics. **P1** before Windows release. Test-only change with macOS regression coverage. |
| `refused_connection_remains_a_network_failure` | The test drops an ephemeral localhost listener and expects refusal within one second. Reqwest returns a timeout on the Windows runner. Production maps `is_timeout()` to Timeout and other transport errors to Network. The observed mismatch does not establish that production misclassifies a genuine refusal. | Diagnose the Windows localhost result, then isolate deterministic classifier coverage from transport timing. **P2** unless desktop testing exposes a user-facing classification error. Keep production and macOS error mapping intact absent such evidence. |
| `legacy_recovery_reads_absent_weather_and_requires_safe_startup_before_write` | A test-only schema-1 initializer reaches the deliberate Windows migration rejection before exercising weather storage. Production startup does not migrate established legacy schemas on this path. | Retain the legacy assertions on supported platforms. Add separate Windows current-schema storage assertions if coverage is needed. **P2**. Preserve the migration guard. |
| `legacy_reads_do_not_migrate_and_corrupt_weather_is_not_presented` | The same fixture restriction prevents the Windows view test from reaching its read-only and corrupt-weather assertions. | Preserve the legacy test on supported platforms and use a current-schema fixture for Windows corruption behavior if needed. **P2**. No macOS production change. |

Sources: [compiler fixture](../../app/src-tauri/src/backup/journal_content/tests.rs),
[weather test](../../app/src-tauri/src/weather/historical/tests.rs),
[transport classification](../../app/src-tauri/src/weather/historical.rs),
[legacy initializer](../../app/src-tauri/src/database.rs),
[storage test](../../app/src-tauri/src/weather/storage.rs) and
[view test](../../app/src-tauri/src/weather/view.rs).

## Windows readiness and release priorities

P0 is a release prerequisite. P1 should normally precede release.
P2 may be deferred.

| Area | Current evidence | Remaining gap | Release priority |
| --- | --- | --- | --- |
| Startup, storage, permissions and locking | Desktop standard-user install and startup, persistent edits and ordinary exit/relaunch. Native NTFS tests cover first creation, reparse/share denial, retained lease and second-owner exclusion. | Check clean and upgrade installs with a synthetic journal under local Roaming AppData. Scope support to tested local storage. | P0 for upgrade and data preservation. Wider filesystem coverage P2. |
| Backup and native pickers | Backup creation and the Restore/Recovery pickers passed desktop acceptance. | Photo and export dialogs, cancel behavior and representative Unicode paths have no recorded desktop acceptance. | P1 |
| Linked photographs and external files | Photo picker, absolute original path and stored preview are implemented. Windows automated coverage includes image and backup round trips. | Desktop smoke test a JPEG/PNG with spaces and Unicode in its path, then move the original and verify the preview remains readable. No dedicated open/reveal-original action was identified in the current journal UI. Arbitrary external file attachments are outside this feature. | P1 for photo flow. P2 for additional external file actions. |
| Open Export and Import | Open Export is implemented through a native save dialog and verified archive writer. Open Import picker and activation are macOS-only. | Verify exported archive and path handling on desktop. Settings currently advertises Import on Windows, where the command rejects it and the UI maps the error to generic failure. Hide the unsupported action and guard direct navigation before shipping. | P0 for the misleading Import entry. P1 for Export desktop verification. |
| Ordinary Restore and blocked-startup Recovery | Desktop verified the happy paths within C7 and C8. Native tests cover protected publication, restart admission, diagnostic preservation and injected failures. | Retain the default Restore gate until the criteria below are met. Prior directory presence is desktop evidence. Native tests supply stronger byte-preservation evidence. | P1 for enabling Restore. Default gated distribution requires clear feature disclosure. |
| Settings and localization | Shared frontend tests cover localized strings and controls. Native menu relabeling is macOS-specific. | Check Settings, Restore/Recovery and dialogs in Swedish and English on Windows. OS dialog chrome may follow Windows settings. | P1 |
| Installer lifecycle and runtime | Current-user unsigned x64 NSIS, product identity and packaged resources verified for internal builds. | Verify upgrade from an earlier Windows build, uninstall/reinstall with journal data intact, and launch on a clean standard-user profile with the supported WebView2 runtime. Record installed files and check that the installer does not affect journal data. | P0 |
| Public artifacts and reputation | The pipeline verifies x64 PE, app metadata, notices and the expected executable patch. Local artifact hash and source provenance were recorded. | Select a release version and source commit, verify actual packaged licenses, publish checksums and record toolchain provenance. Decide signing policy and validate download/install on a representative stock Windows 11 configuration. Unsigned binaries may be blocked by Smart App Control. Signing reduces friction but cannot guarantee no SmartScreen warning. | P0 for a tested distribution path, metadata and notices. P1 for signing where the chosen distribution policy permits unsigned builds. Byte-identical rebuilds P2. |

Implementation references: [photographs](../../app/src-tauri/src/photographs.rs),
[Open Export](../../app/src-tauri/src/export_app.rs),
[Open Import](../../app/src-tauri/src/import_app.rs),
[Settings links](../../app/src/lib/components/AppNavigation.svelte),
[Windows installer overlay](../../app/src-tauri/tauri.windows-test.conf.json),
[packaging checks](../../.github/workflows/ci.yml) and
[Windows notices inventory](../../app/licensing/native.windows.json).
The current Tauri default can fetch the WebView2 bootstrapper when the
runtime is absent. Confirm this behavior or state the runtime prerequisite
for the release candidate.
[Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)
· [Microsoft Smart App Control](https://learn.microsoft.com/windows/apps/develop/smart-app-control/overview)
· [Microsoft SmartScreen reputation](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation)

## Criteria for default Windows Restore

Keep `windows-restore-acceptance` during targeted verification. The
[backend](../../app/src-tauri/src/database_state.rs) latches writes before
publication and reports acknowledged Pending separately from uncertainty.
The [shared shell](../../app/src/lib/components/RecoveryPanel.svelte)
offers Quit for either blocked outcome. Frontend tests simulate
event-before-reply and native tests inject faults. Desktop acceptance
covered the acknowledged path.

Before default enablement, verify on the opted-in build that a rejected
or changed backup leaves the old journal usable, and test the uncertain
handoff on native Windows through a controlled fault or targeted
integration seam. Require the shell to preserve a Quit action and avoid
a success claim if acknowledgement fails. Verify that a failed mode
refresh or lost IPC response cannot leave the user without a safe way
to exit after the write latch. Check retry/admission after restart,
lease exclusion and preservation of prior data with existing native
fixtures. Retain macOS regression tests and leave the macOS gate and
publisher intact. This requires representative failure coverage.
Physical power-loss certification, broad fault injection and a storage
redesign remain outside the gate decision.

## Recommended C9b — repair the Windows backup API compile fixture

Fix the newline-sensitive fixture transformation first. The missing
module stops the positive control, leaving Windows without proof that
the actual copied API rejects direct SQL access and snapshot replacement.
The change is small and can be reviewed independently of Restore
enablement and installer work.

- Make the transformation work for LF and CRLF. Assert that the intended
  declaration was removed, and keep the positive control and all
  expected negative diagnostic checks.
- Test both newline forms and run the exact compiler probe on Windows
  and macOS. If native evidence confirms the repair, retire only its
  baseline entry and verify the other three identities and reasons
  remain unchanged. No desktop test is needed for this fixture repair.
- Acceptance requires the positive control and every negative probe to
  execute on Windows, with the corresponding macOS test passing. Keep
  production code, storage behavior, Restore gating, packaging and the
  other three baseline failures out of C9b.

Next, address the Windows Import affordance and run the installer
data-preservation checks. Handle the weather failures independently in
the order of deterministic transport coverage, then the two legacy
fixtures when their Windows coverage is needed.

### C9b compiler fixture correction

Replaying the old transformation on the actual API source reproduced
the defect. The LF fixture drops the test module declaration, while the
CRLF fixture retains it and requests a module absent from the generated
fixture. This matches the Windows E0583 diagnostic. The fixture now
normalizes CRLF and asserts exactly one declaration is removed.
The regression compares LF and CRLF outputs before running the existing
positive control and negative compiler assertions.

The focused compiler test passed on macOS, along with formatting and
diff checks. The focused Windows workflow now requires an explicit
compiler-test pass and matches the other three failure identities and
reasons and the two existing ignores. Comparator tests cover both the
historical four-failure mode and the transition, including rejection
of a missing pass, a changed diagnostic or an extra failure. The C5c
baseline remains recorded. A three-failure result is pending native
Windows CI.

### C9c legacy weather fixture portability

Both tests first create a schema-0 fixture. The storage test reaches its
read-only and write-before-upgrade assertions, then the test-only
schema-1 initializer rejects migration on Windows before the supported
write check. The view test reaches its read-only and batch-limit checks,
then hits the same guard before inserting corrupt weather.

macOS retains the existing migration continuation. Windows now asserts
the exact migration rejection and unchanged legacy database bytes in
both tests, then uses a separate current-schema journal admitted by
production startup. Storage verifies that an unadmitted write cannot open
SQLite, then verifies weather writes and reads without changing journal
content. View verifies that an invalid current-schema weather row is
rejected without rewriting the database. Production migration and weather
behavior are unchanged.

Local macOS weather storage and view suites passed (nine tests total),
along with formatting, workflow YAML parsing and comparator regressions.
The focused Windows run requires explicit passes from both weather tests
and the C9b compiler test. It still checks the exact historical timeout
diagnostic and the two known ignores, and rejects unexpected failures.
The historical baseline modes remain available. The transition from
three known failures to one is pending native Windows evidence.

### C9d historical weather transport fixture

C9c Windows run 42383850180 matched the one-failure baseline. The old
test dropped its localhost listener before the request, leaving an
unowned port, and expected a refusal before the one-second request
deadline. The Windows log shows a failure in request sending before any
HTTP response and classification as Timeout. The logged diagnostic lacks
the underlying socket error and does not establish whether Windows
refused the connection or where inside request sending the deadline
expired. There is no demonstrated production classification defect.

The replacement fixture keeps the loopback port owned, accepts and reads
the HTTP request, then closes without a response. This gives the real
HTTP client a controlled non-timeout transport failure and verifies
Network classification. The existing no-response server still verifies
Timeout independently. A kernel-level refusal is no longer a separate
assertion. Production request and error mapping code is unchanged.

Local macOS historical-weather tests reported 30 passes and the existing
ignored live HTTPS test. Five comparator tests, Rust formatting, YAML
parsing and diff checks passed. The focused Windows workflow now requires
a clean unfiltered library result, explicit passes from the C9b, C9c and
C9d tests, and exactly the existing two ignores. Earlier comparison modes
remain available for historical logs. The last accepted Windows failure
remains recorded until native CI confirms the transition.

### C9e Settings Import availability

Settings previously showed Import on every platform. The command rejects
selection outside macOS, so Windows users reached a generic error after
opening a route that appeared usable. Open Import consumes a portable
`.mno-export` archive and replaces journal content through the macOS
activation and recovery protocol. Shared archive handling does not
provide equivalent Windows activation. Backup, Restore and blocked
Recovery use separate contracts and retain their existing behavior.

Settings and direct Import navigation now use a read-only capability
command that reflects the existing macOS-only backend restriction.
Windows and Linux Settings offer Export with export-only explanatory
text. A direct Import route explains unavailability in Swedish or
English and offers a way back without invoking the picker or activation.
The macOS import picker, confirmation and activation remain available.

Scoped release recommendation is to disclose that Open Import is
unavailable on Windows. If cross-platform Open Import is required for
launch, its activation design and acceptance need a separate decision.

Local checks passed for 542 frontend tests, Paraglide, Svelte, Rust
formatting and macOS all-targets compilation. Mocked availability tests
exercise both Settings and direct navigation. On the next approved
Windows desktop candidate, verify in both languages that Settings keeps
Export and Backup but omits Import, and that direct navigation to
`/settings/import` displays the unavailable message with no picker.
On macOS verify the original Import link and picker still open.
Desktop acceptance is pending.
