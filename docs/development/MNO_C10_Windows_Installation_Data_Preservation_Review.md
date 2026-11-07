
# C10a Windows installation and data preservation

Reviewed `codex/cross-platform-ci` at `217aa8f`, with Tauri CLI 2.12.2
pinned in the npm lockfile. C9h passed native CI and, as reported by the
tester, the default-feature Windows 11 installer preserved an existing
synthetic journal and passed Backup → Restore → restart → edit → save →
restart. That result covers an observed installation and journal workflow.
Uninstall, reinstall and the version-sensitive upgrade choices still need
desktop verification.

## Verified from configuration and source

| Area | Behavior |
| --- | --- |
| Executable and resources | The [Windows overlay](../../app/src-tauri/tauri.windows-test.conf.json) uses current-user NSIS and the [base config](../../app/src-tauri/tauri.conf.json) keeps bundle ID `org.mynatureobservations.app`. The stock template defaults to `%LOCALAPPDATA%\My Nature Observations` and can reuse the previous registered install path. It checks for a running app and copies the executable and packaged resources under that install directory. Those files include the notices and can be replaced on install. |
| Journal root | The application resolves its own per-user directory through [environment.rs](../../app/src-tauri/src/environment.rs). On Windows the production root is `%APPDATA%\org.mynatureobservations.app` (Roaming AppData), separate from the default installer payload. Legacy SQLite is `mno.sqlite`. Managed SQLite lives in `generations\<id>\journal.sqlite`, with `generation-selection.json`, establishment and lock state, and any interrupted-publication evidence at the root. Previous generations and diagnostic evidence under `generations` are also user data. |
| Backups | [Manual Backup](../../app/src-tauri/src/manual_backup.rs) creates a `.mno-backup` at the path selected in the save dialog. Windows generation Restore retains the earlier journal generation. It does not create the macOS pre-restore backup tree as its preservation mechanism. A manual backup stored inside either app-data directory is subject to deletion with that directory. Store a test backup outside both app-data and installer directories. |
| Upgrade and same-version reinstall | NSIS distinguishes newer, equal and older versions. Same-version maintenance offers reinstall/add or uninstall. A version upgrade can proceed in place or invoke the prior registered uninstaller, according to the maintenance choice. This is why a different commit at version `0.1.0` exercises the same-version path even when the binary is newer. The installer does not initialize or migrate SQLite. App startup independently decides whether it can open or establish the journal. |
| Uninstall | Stock NSIS removes its installed payload, resources, uninstaller, and applicable shortcuts and registrations. Its interactive confirmation includes an initially unchecked app-data deletion option. If selected outside updater mode, the template recursively removes both `%APPDATA%\<bundle-id>` and `%LOCALAPPDATA%\<bundle-id>` in current-user shell context. Leaving it unchecked is expected to retain the journal root. This conclusion is from the template, pending confirmation of the generated candidate and desktop UI. |
| Machine-wide mode | Stock per-machine mode requests elevation and uses Program Files and machine registration. MNO still resolves journal data for the user running the app. The stock deletion branch explicitly switches to current-user shell context, so it must not be assumed to target all profiles or the profile of the person whose app is being removed. Scope or elevation changes can also change installer detection and which profile is involved. Current-user installation is the reviewed release path. |

Sources: [pinned NSIS template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi#L300),
[uninstall deletion branch](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi#L815),
[managed layout](../../app/src-tauri/src/startup/established/managed_layout.rs),
and [database startup](../../app/src-tauri/src/startup.rs).

## Risk and evidence

- **High — explicit app-data deletion.** The checkbox can remove the
  entire journal, selection and generations for the affected profile,
  including any backup stored there. The standard upgrade path can
  invoke an earlier interactive uninstaller. Verify the exact prompt
  and unchecked default and document the preservation choice before
  distribution. If the candidate preselects deletion or removes data
  with it unchecked, stop the release and correct that concrete defect.
- **High — application identity or account changes.** A changed bundle ID
  or different user resolves a different journal root and can appear
  to lose data. Keep the current identity and current-user scope. Keep
  the install path separate from journal and backup directories.
- **Medium — candidate maintenance choices.** Existing C7/C8/C9
  acceptance shows install and subsequent journal use. It does not
  establish how the actual candidate presents all maintenance choices
  or whether a distinct-version upgrade preserves data through a prior
  uninstall. Packaging CI extracts and verifies the payload and notices
  without installing. Native startup and lease tests cover database
  opening, exclusion and generation admission, not NSIS lifecycle.
  [C7 evidence](MNO_C7_Generation_Recovery_Review.md)
  · [C8 evidence](MNO_C8_Cross_Platform_Readiness_Review.md)
  · [C9 evidence](MNO_C9_Windows_Quality_Readiness_Review.md)
  · [Packaging workflow](../../.github/workflows/ci.yml)
- **Low within current scope — machine-wide coexistence.** Multiple
  install scopes could present separate registrations while using the
  same journal for a given account. Defer this scope until it is needed.

## Minimal Windows desktop acceptance plan

Run this later in the disposable standard-user `MNO-Test` profile on
local NTFS. Use verified local installers and synthetic data only.
Confirm the journal and install paths first. Create a fresh manual
backup outside the journal, Local AppData and install directories.
With MNO fully closed, record its checksum and a read-only inventory
and hashes of the SQLite files, selection record and retained
generations. Keep the baseline outside the installation and data roots.
Do not move, edit or delete journal files to prepare the test.

1. **Same-version reinstall.** Run the same installer and select
   reinstall/add. Leave any option to launch the app automatically
   unchecked until the on-disk comparison. Verify the installer payload
   exists and the journal files and external backup are unchanged.
   Launch and verify the expected synthetic entry and writable access.
2. **Version upgrade.** When two approved installers have genuinely
   distinct increasing versions, repeat using the documented in-place
   upgrade choice. Capture the maintenance screen and verify that the
   newer executable is installed and the same journal opens. Record the
   uninstall-first choice if offered. If that path will be supported,
   verify it separately with app-data deletion unchecked. A newer
   commit with the same version is not evidence for this step.
3. **Uninstall and reinstall.** Quit MNO and refresh the read-only
   baseline after any app edits. Uninstall through Windows Settings
   under the same account. Confirm the app-data deletion option is
   unchecked and leave it unchecked. Verify the app registration and
   payload were removed and the journal root and external backup remain
   unchanged. Reinstall the candidate, compare the retained files
   before first launch, then reopen the same journal, save a synthetic
   edit and restart to verify persistence.

Stop on an unexpected prompt, path, hash change or missing journal.
Record source commit, installer hash, versions, selected choices and
screenshots using synthetic data only. The destructive checkbox path
and machine-wide mode are outside this acceptance.

**Release recommendation.** Inspect the generated candidate NSIS script,
complete the preservation checks above, and document the user-facing
uninstall choice and external backup location. The source review found
no need for a custom installer template. Revisit the configuration only
if candidate testing shows a concrete preservation defect.
