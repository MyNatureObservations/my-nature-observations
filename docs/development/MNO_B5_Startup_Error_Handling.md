# B5 — expected startup errors

## Approach

Tauri 2.12.3 executes the application setup callback on its main event loop after
creating configured windows. A setup error becomes a Tauri panic. MNO's existing
panic hook aborts without a release diagnostic, so ordinary journal startup errors
previously terminated the app without an MNO explanation.

The existing dialog plugin supplies a standalone native message dialog. Its
asynchronous `show` API lets setup return to the main loop; `blocking_show` is
explicitly unsuitable on the main thread. No parent window or frontend is needed.

Only expected data-root resolution and database startup errors take the new path:

- No DataLocation is installed; normal journal access stays unavailable.
- Hide the main window and display the native error dialog.
- Closing the dialog, including its single Exit button, requests application exit
  with status 1 using Tauri's normal lifecycle. No journal was admitted for use.
- A retained RecoveryGate still returns successfully to the existing Recovery UI.
- Programming errors, plugin failures and other unexpected setup panics retain the
  original panic policy. No catch_unwind, repair, retry or replacement database.

The small wrapper in `startup_error.rs` delegates to the unchanged DatabaseState
and startup engine. First-run detection, locks, leases, path guards, validation,
Restore/Import, formats and environment identities are unchanged.

## Wording and language

Paraglide and the saved interface language live in frontend JavaScript/localStorage;
they are unavailable reliably before frontend initialization. The native message
therefore uses a single bilingual fallback rather than a second localization system.

Title: **MNO kunde inte öppna journalen / MNO could not open the journal**

Reason (only reliably identified categories):

- Directory in use: **Journalens datakatalog används redan. / The journal data
  directory is already in use.**
- Schema/content validation: **Journalen kunde inte godkännas vid kontrollen. /
  The journal could not pass validation.**
- All other expected startup failures, including open/permission errors and invalid
  SQLite: **Ett problem uppstod när journalen skulle öppnas. / A problem occurred
  while opening the journal.**

Then: **Journalen har inte öppnats. Behåll dina befintliga filer och säkerhetskopior
 tills problemet har undersökts. / The journal has not been opened. Keep your
 existing files and backups until the problem has been investigated.**

Button: **Avsluta MNO / Exit MNO**.

Raw error text, filesystem paths and stack traces are never interpolated. The text
means that journal access was not admitted; SQLite may have been opened read-only
for validation. It does not claim that no operational files were written: initial
lock/establishment metadata may already exist on some failure paths.

## Focused tests

Seven new Rust tests cover real mode-000 directory/database access denial (ordinary
non-root macOS account), invalid SQLite, rejected schema with byte preservation,
directory lock contention, retained Recovery gate, and bilingual/path-free messages.
They use only temporary synthetic roots. Assertions include no ready path and no
operation admission after failure. Source wiring checks require asynchronous dialog
closure to request exit; actual native presentation/termination needs package testing.
Existing startup, Recovery and panic-policy tests remain unchanged.

## Isolated packaged acceptance

Never use RC B1/B2, DEV or Production for these faults. Build a fresh standalone
release test app with an unused identifier, for example:

```json
{
  "productName": "MNO B5 Startup Test",
  "identifier": "org.mynatureobservations.app.test.b5.UNIQUE"
}
```

Replace UNIQUE with a unique alphanumeric suffix. From `app/` run:

```sh
npm run tauri build -- --bundles app --config /absolute/path/to/test.conf.json
```

Copy only the new bundle to a new test-artifact folder. For local macOS testing,
ad hoc sign that copy and verify it, as documented for the isolated RC process.
Do not modify Gatekeeper policy or replace any installed app. Release builds ignore
MNO_TEST_DATA_DIR, so the unique build identity is required for isolation.

For this identity the temporary synthetic root is
`~/Library/Application Support/org.mynatureobservations.app.test.b5.UNIQUE`.
Before any fixture creation, require that this exact root does not already exist.
Use an ordinary non-root account. Never change permissions on a pre-existing root.

1. **Inaccessible directory:** create the new synthetic root exclusively, then give
   it mode 000. Launch only the new test bundle from Finder. Expect the bilingual
   generic message, no normal journal, and successful exit when closing the dialog.
   Restore mode 700 on this exact test root after the app has exited.
2. **Invalid SQLite:** in that synthetic root create `mno.sqlite` exclusively with
   bytes `not a SQLite database` (Python `open(path, "xb")` avoids overwrites).
   Launch again. Expect the generic error, exit, and unchanged synthetic DB bytes.
3. **Database access denied:** give only that known synthetic file mode 000. Launch
   again, confirm error/exit, then restore mode 600. Retain all test files; do not
   delete them automatically. Confirm no replacement SQLite was created.
4. Confirm both languages are readable, no raw path appears, and no journal opens.
   A generic macOS crash notice or a frozen dialog is a failure; record it.

If any required path already exists unexpectedly, stop and choose another unique
identity/build. Do not reuse personal backups or journals. Package test results
and any remaining human checks are recorded below when available.

## Implementation verification results

- Rust library: **912 passed, 2 ignored**, including all seven new tests, existing
  startup/Recovery and panic policy tests. Formatting check passed.
- Frontend: **411 passed**; Svelte/TypeScript: **0 errors, 0 warnings**.
- Optimized macOS release `.app` built successfully with isolated identity
  `org.mynatureobservations.app.test.b5.20261031`; ad hoc bundle signature verified.
- Local artifact (not installed): `release-candidates/B5-20261031/MNO B5 Startup Test.app`
  under the project workspace. Apple Silicon arm64, version 0.1.0.
- A new B5-only Application Support root was created exclusively, with mode 000,
  and that test app was launched. Process sampling showed an active AppKit main
  loop and a worker waiting in `CFUserNotificationDisplayAlert`, confirming the
  standalone macOS dialog path was reached. No SQLite was created in this scenario.
- The computer-use tool refuses access to system UserNotificationCenter for safety
  reasons. Thus **dialog appearance, text readability and button-driven exit were
  not verified**. No attempt was made to bypass that restriction. The isolated
  test process was terminated explicitly; its root's permissions restored to 700.
- The same B5-only root now contains an exclusively created `mno.sqlite` with bytes
  `not a SQLite database`, ready for manual invalid-SQLite package testing. That
  scenario was not launched. No existing RC, DEV or Production journal was touched.
- For the prepared artifact, manual acceptance can begin with invalid SQLite.
  After confirming exit, use mode 000 on that known synthetic file for the database
  denial case, restoring mode 600 afterward. To repeat directory denial, use mode
  000 on this B5-only root and restore mode 700 after exit. Keep all fixture files.
- Parentless dialogs in the current dependency use macOS system UserNotificationCenter;
  the app's own hidden-window accessibility tree will not expose the alert.
- This remains local testing: no notarisation/public distribution changes. Any
  Gatekeeper approval and the remaining native acceptance must be performed by
  the user. These limitations are not claims that packaged UI acceptance passed.
