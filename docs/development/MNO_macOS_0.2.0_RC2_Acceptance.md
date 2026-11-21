# macOS 0.2.0 RC2 — replacement build and acceptance plan

Base: public main `fb2bf9a8014da0332f55a1912e022ad53a30d5de`.
Branch: `codex/macos-v0.2.0-rc2`. Build from the clean local documentation checkpoint;
record its exact commit, artifact path, inspection and checksum in the build report.
Version remains 0.2.0, identifier `org.mynatureobservations.app`, Apple Silicon
`aarch64-apple-darwin`, minimum macOS 13.0, default production features.

RC2 supersedes [RC1](MNO_macOS_0.2.0_RC_Acceptance.md) because it includes explicit,
default-off bird-module activation. Preserve RC1's bytes, checksum and acceptance
history. The existing GitHub release draft remains unpublished/unmodified and
still contains RC1; it must not be mistaken for RC2. RC2 packaged acceptance
passed within the scope recorded below; publication remains paused. [Bilingual release notes](MNO_macOS_0.2.0_Release_Notes.md).

## Build and evidence

Use CI-pinned Node 26.12.1, Rust 1.99.1 and cargo-about 0.9.3. From `app/` build
`npm run tauri build -- --target aarch64-apple-darwin --bundles dmg --ci`, with
`MACOSX_DEPLOYMENT_TARGET=13.0`, ad hoc signing, no notarization credentials and
no DEV overlay or feature flags. Generate notices through the before-build hook.
Keep RC2 under `app/src-tauri/target/release-candidates/macos-v0.2.0-rc2/` with
an RC2/source-qualified filename and matching `.dmg.sha256` manifest.

Inspect the app in the finished read-only-mounted DMG: Info.plist and Mach-O
identity/minOS, architecture, strict signature verification, notarization status,
embedded MIT license, matching `aarch64-apple-darwin` notices, DMG integrity/hash.
Do not start or install the app as part of inspection.

Reuse PR #22's accepted DEV evidence and passing tests (688 frontend, one rendered
UI, 1,037 Rust passed/two existing ignored, all-targets, Svelte and formatting).
Only documentation/release preparation changes here; focused version/notices
checks, documentation links, diff checks and the production build are appropriate.

## Accepted RC2 identity and packaged results

- Tested clean build commit: `9fa9da6074bd579a5eaadcb56aafa6019587530e`.
- DMG: `My-Nature-Observations-0.2.0-macos-arm64-rc2-9fa9da6.dmg`.
- SHA-256: `4b08e77f979caa209cb919a04e91a8cc5537ff97e1c444d501bf3e574f69b1e8`,
  reverified when recording acceptance. No rebuild or modification.
- Later documentation commits record acceptance; they are not the tested build.
- Finished-DMG inspection verified 0.2.0, arm64, `org.mynatureobservations.app`,
  macOS 13.0 minimum, valid deep/strict ad hoc app signature, embedded MIT license,
  matching Apple Silicon notices and DMG integrity. App not notarized; DMG unsigned.
  Build completed with rust-objcopy debug-information stripping warnings, retained
  in the local build/inspection record.

**Owner-reported packaged macOS acceptance: PASS within this scope.**

- Fresh account: default-off core experience and explicit activation.
- Upgrade from public v0.1.1: journal content preserved; bird module stays off.
- Disable/re-enable and restart preserve bird content and account preference.
- Backup/Restore and Export/Open Import across separate test accounts: core content
  before activation, bird content after explicit activation, then persistence on restart.

These manual packaged results are distinct from earlier DEV and automated tests.

### Archive-transfer investigation and evidence limits

Initial Restore failed because `/Users/Shared/0.2.0.mno-backup` had mode 600 and
was owned by `MNOTEST`; target account `mno1` could not read it. Both running apps
used the shared installed app, whose 13 regular files matched accepted RC2.
A readable copy passed ZIP/CRC, manifest/database checksums, SQLite integrity and
foreign-key checks, then RC2's real `verify_backup` and Restore `validate_archive`
validators in an isolated source probe with CI-pinned Rust 1.99.1 (**1 test passed**).
The owner subsequently confirmed manual Restore succeeded with the readable copy.

The inspected format-13/schema-12 archive contained linked/unlinked bird lists,
stored image data, comments, weather and links. Its bird-photo caption was null;
the owner confirmed it was absent in the source too. **No caption loss was
established, and this archive test did not verify a nonempty bird-photo caption.**
Earlier independent caption coverage remains historical evidence, not an expanded
claim for this fixture. Cross-account original-file references did not cause the
archive rejection; stored previews and external originals remain distinct.

The misleading unreadable-archive message is a non-blocking
[UX-006 follow-up in issue #4](https://github.com/MyNatureObservations/my-nature-observations/issues/4).
No validation weakening or automatic permission change was introduced. Source/
target profiles and original archives were preserved. The old draft, RC1 and tags
remain unchanged; final public download acceptance has not been performed for RC2.

## Packaged acceptance procedure — retained for reference

Use separate disposable **standard macOS accounts**, for example:
`MNO-020-RC2-Fresh`, `MNO-020-RC2-Upgrade`, `MNO-020-RC2-Restore`,
`MNO-020-RC2-Import`. They must have no pre-existing MNO production preference for
the initial default-off checks. Do not delete settings/journals to manufacture a
fresh state; use new accounts. Preserve previous test accounts/profiles/archives.

Install each exact app copy in that account's `~/Applications` and launch it
explicitly from Finder after quitting any other MNO. Avoid replacing shared
`/Applications`. Verify the RC2 checksum against its manifest before installation;
About must report 0.2.0. Do not use `MNO_TEST_DATA_DIR`: production uses
`~/Library/Application Support/org.mynatureobservations.app/mno.sqlite`.

The production module choice is account-level, outside journal content:
`~/Library/Application Support/org.mynatureobservations.preferences/org.mynatureobservations.app/bird-module-v1.json`.
All production journals within one account share it; separate accounts do not.
DEV has a different preference and does not establish production defaults.
Use Settings → Modules to control the choice; never edit/delete the preference.

### 1. Fresh production account

1. Install RC2 in `MNO-020-RC2-Fresh`. Confirm Modules starts collapsed and Bird
   observations is off without a setup action. Home/sidebar/calendar/diary show
   core journal behavior; no bird creation/link actions or markers appear.
2. Create synthetic text, a disposable journal photo/caption and journal weather;
   save and restart the same installed copy. Verify content and shared viewer.
3. Explicitly enable Birds. Confirm creation/overview become available. Restart
   and verify the choice stays on. Check both languages and keyboard access.

### 2. Upgrade from public v0.1.1

1. In `MNO-020-RC2-Upgrade`, download the published
   [v0.1.1 app and manifest](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.1).
   Verify DMG SHA-256 `5f7c3de52f4b3cdafc87060853c12f3c56c3bfc85ecd4bdd3b8591a438979529`.
   Install in this account's `~/Applications` and create synthetic journal text,
   photo/caption and optionally place/weather. Save, restart and verify.
2. Create an external backup, retain it unchanged and quit. Replace only this
   account's app with the exact verified RC2. Confirm text/photo/caption survive,
   Modules remains off, and a new edit persists after restart.
3. The database upgrades to schema 12 through the protected path. Preserve the
   independent source copy and pre-upgrade backup. Do not open the upgraded
   journal using v0.1.1; replacing the app does not downgrade the database.

### 3. Existing bird content and toggle lifecycle

In the upgraded synthetic account, explicitly enable Birds and create linked and
unlinked visits with counts/comments, photo/caption and manual/fetched weather.
Use a distinct date/place to identify each. Keep one list linked to multiple
journal entries if convenient. Quit/reopen and establish the saved baseline.

Disable Birds: only presentation disappears; core journal/photo/viewer/weather
remain usable. Restart with it off; then enable and compare the same saved data,
photos/captions, comments, weather/context and journal links. Restart again with
it on. Test Stay versus explicit discard on an extra unsaved edit without changing
the baseline fixtures. Do not delete original photo files or manipulate databases.

This provides genuinely existing saved bird content before the transition; it does
not assert migration from an already public bird release (none exists).

### 4. Archives while disabled, separate targets

1. In the source account, disable Birds and create new Backup and Export through
   Settings outside application data, e.g. `~/Documents/MNO-RC2-Acceptance`.
   Copy the archives/RC2/manifest to a shared readable transfer folder, retaining
   the source files. Never restore/import over the source account.
2. In `MNO-020-RC2-Restore`, install RC2, launch a fresh core journal and explicitly
   confirm Birds is off. Settings → Restore the new backup; prepare, quit normally,
   reopen the same app/account and complete **Verify and open journal**.
3. Before enabling, confirm core content works and bird UI remains hidden. Enable
   explicitly and compare linked/unlinked visits, photos/captions, comments,
   weather and links. Originals in another account may be inaccessible: stored
   previews must still display without being represented as original files.
   Restart and confirm activation/content persist. Disable and restart once more
   to check off-state persistence if desired.
4. In `MNO-020-RC2-Import`, install the same RC2, establish a fresh journal and
   explicitly confirm Birds off. Settings → Open Import the new export; review,
   confirm, quit/reopen as instructed, and **Verify and open imported journal**.
   Repeat the core-only, explicit enable, full-content and restart checks.
5. Account choices are independent: enabling in Restore must not enable Import.
   Within each target, replacement must retain the off choice until explicitly
   changed. If reusing an account with several journals, set off through Settings
   before each replacement rather than assuming a new journal resets it.

The accepted results and limitations above are separate from this retained
procedure; it does not establish any unreported subcheck or public-download result. A later rebuilt
artifact is a new candidate and cannot inherit RC2's eventual acceptance silently.
