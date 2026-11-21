# Optional macOS bird module — local implementation, 20 November 2026

Base: public main `7d450c7f665392a69734e1040ece34fc0cc3221c`.
Branch: `codex/mac-optional-bird-module`. Reviewed local checkpoint;
accepted DEV behavior and archive checks are recorded below. Replacement-package
acceptance remains pending.

## Scope and account preference

The [explicit product decision](../requirements/Egna_fagelobservationer.md#valbar-fågelmodul--uttryckligt-produktbeslut-20-november-2026)
keeps journal writing, journal photographs, the shared viewer and journal weather
in core. Bird observations default off and require Settings → Modules activation.

Rust owns a typed version-1 preference outside journal content. On macOS the paths
are under `~/Library/Application Support/org.mynatureobservations.preferences/`:

- Production: `org.mynatureobservations.app/bird-module-v1.json`.
- DEV: `org.mynatureobservations.app.dev/bird-module-v1.json`.

Tauri's account configuration base and actual app identifier determine the path.
The sibling namespace matters: macOS app configuration and journal data directories
can otherwise coincide. Missing, malformed or unknown-version preferences default
off. Preference writes use a synced private temporary file and rename; failure
leaves the in-memory choice unchanged and reports an error. An uncertain IPC
response is followed by a read of authoritative state rather than a blind toggle.

This is one choice per account/app identifier, not per test profile or journal.
Changing `MNO_TEST_DATA_DIR`, importing or restoring another journal does not
change it. A new DEV journal can therefore inherit an explicitly enabled DEV
choice; a missing choice is always off. DEV cannot enable production.

## Boundaries and lifecycle

All registered bird commands check activation, including catalog/paste matching,
reservation, reads, CRUD, links, photo preparation and weather preview/review/cancel.
Platform checks remain. A command holds admission through completion; changing
the preference waits for already admitted work and invalidates all visit-weather
preview slots. Re-enabling cannot revive their receipts.

UI availability refreshes immediately through module/journal invalidation events.
Home, sidebar, diary, calendar, linked previews and linked-photo galleries omit
bird content when off. Direct bird routes show a localized disabled message and
core return path. Core journal editors are not remounted on preference changes;
core photo loading and journal weather remain independent.

Before disabling, the bird form uses its existing focused Stay/Discard dialog.
Cancel keeps draft and choice. Pending photo/write operations and uncertain save,
delete or link states must be resolved before completing disablement; the toggle
shows an actionable message and remains on. Admitted backend operations are also
protected against a concurrent direct setting command by the admission lock.
Leaving the bird view disposes photo/weather callbacks; a late response cannot
apply to a later view. Return goes to the originating entry if it still exists,
otherwise Home. Enabling from a disabled direct bird route returns to Home.

Disabling never deletes or rewrites bird content. Persistence, source-preserving
startup upgrades, shared place integrity and complete archive validation do not
consult UI activation. Schema 12/archive format 13 remain unchanged. The existing
full Restore/Open Import production regression explicitly executes with a disabled
module fixture, preserving linked/unlinked observations and their weather/photos.

## Recorded automated evidence

Using Node 26.12.1 and CI-pinned Rust 1.99.1:

- Complete frontend suite: **688 passed**, zero failed/skipped. After the final
  Stay/focus-restoration refinement, **90 affected frontend tests passed**.
- Isolated rendered Chromium toggle test: **1 passed**; default-off checkbox,
  keyboard Space/focus, veto, preference-write failure, committed/lost-response
  recovery and successful disable.
- Complete macOS Rust library suite: **1,036 passed, 2 existing ignored**, zero
  failures. Includes preview invalidation and disabled-module archive regression.
- After the final preference-path isolation correction, **5 focused module tests
  passed**, including the additional sibling-directory test; all-targets compilation
  and CI-pinned formatting/check passed. Unaffected archive tests were not repeated.
- Svelte: zero errors and warnings. Documentation links and `git diff --check`
  checked separately at delivery.

These are automated results, separate from the owner-reported DEV and archive
acceptance below. Replacement-package acceptance remains pending.
UX-005 is unchanged and not investigated.

## User-accepted macOS DEV subset

The project owner accepted core-only behavior, explicit activation and remembered
preference; existing bird content returned on reactivation. Stay preserved an
unsaved draft; explicit discard disabled the module; reactivation showed the
previously saved data. These are manual DEV results, separate from automated
coverage. Module-setting styling/copy were not accepted in that check; the focused
toggle styling, copy and the collapsed Modules section were subsequently accepted.

### User-accepted optional-module archive checks

The owner reported successful manual macOS DEV acceptance of Backup/Restore and
Export/Open Import while disabled. After replacement, core journal content was
available and bird UI stayed hidden. Explicit activation restored all bird content;
activation and content persisted after restart. The test also confirmed that DEV
journal profiles share the same DEV preference. This is manual DEV archive evidence,
not packaged-release acceptance; earlier historical acceptance remains preserved.

### Setting presentation — accepted macOS DEV

The setting now follows the existing settings group typography and spacing, with
a restrained green switch, native checkbox keyboard behavior, visible focus and
an associated description. Swedish copy: “Registrera fåglar från dina besök. Dina
uppgifter finns kvar när modulen är avstängd.” English is localized naturally.
Only presentation/copy changed; activation, safeguards and storage are unchanged.
Focused verification: 90 UI/lifecycle tests passed, plus one rendered browser test
covering switch semantics/state, green accent, keyboard focus, both languages and
320px layout. Svelte reported zero errors/warnings; `git diff --check` passed.
The owner accepted the green toggle styling and concise Swedish/English copy.
Modules now uses the existing settings native `details`/`summary` pattern, matching
Language, Data and security and About: 18px outline icon, quiet heading, chevron,
spacing, focus indicator and reduced-motion treatment. It starts collapsed;
opening/closing changes no activation state and uses native keyboard behavior.
The owner also accepted the collapsed Modules section integration. Focused
verification passed: 90 UI/lifecycle tests, one rendered browser test (collapsed
initial state, keyboard expand/collapse without activation writes, accepted toggle
behavior), Svelte with zero errors/warnings and `git diff --check`.

## Final owner-reported macOS DEV acceptance — 20 November 2026

**PASS within the tested DEV scope:** default-off core journal behavior; explicit
activation, restart and access to existing bird content; Stay preserving an
unsaved draft and explicit discard permitting disablement; Backup/Restore and
Export/Open Import while off, followed by reactivation and restart; accepted
green toggle, concise copy and initially collapsed Modules section. The account's
DEV preference is shared across DEV journal profiles and isolated from production.

This consolidates the owner's manual results above, not inferred automated or
packaged acceptance. Test profiles and archives remain intact. The replacement
production DMG still needs its own build identity and packaged acceptance.

## Isolated macOS DEV checklist — retained for reference

Use synthetic data only. Keep the existing source/target profiles and archives.
From a terminal, create a new persistent disposable parent under your home; its
journal child must not exist before first launch:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
export MNO_MODULE_PROFILE="$(mktemp -d "$HOME/mno-module-dev-XXXXXX")"
export MNO_TEST_DATA_DIR="$MNO_MODULE_PROFILE/journal"
test ! -e "$MNO_TEST_DATA_DIR"
npm run tauri dev
```

Keep this terminal and reuse the same exported path on every restart. The DEV
module setting is account-wide: if previously enabled during testing, turn it off
through Settings before checking the core-only presentation; do not delete a
preference file or journal to simulate reset. Automated tests cover missing-choice
and upgrade default-off semantics without touching the user's preferences.

1. With the module off, create text, a disposable journal photo/caption and journal
   weather. Check Home, sidebar, diary/calendar and shared viewer have only core
   content. A direct `/observations` route must explain the disabled state and
   offer Home. Check Swedish and English.
2. Enable Settings → Modules → Bird observations. Create one standalone visit and
   one journal-linked visit with counts, comments, photo/caption and weather.
   Check previews, diary and markers. Restart the same profile: choice/content stay.
3. Edit a visit without saving, request disable, choose Stay: retain form values,
   focus and enabled state. Retry and explicitly discard: return to the originating
   entry or Home and hide bird content. During a pending operation, wait for the
   reported operation/check to settle before retrying; do not force quit.
4. Re-enable: confirm identical visits, links/photos/weather. Disable while a core
   journal editor is open: keep its text/editor and save safeguards intact.
5. With birds off, create Backup and Export using normal Settings. Restore/Open
   Import into separate disposable DEV journal targets, keeping the source intact.
   Restart each target, confirm the account choice stays off, then enable and
   compare linked/unlinked visits, photos/captions, weather and journal links.
   Existing production-path automated tests cover internal interruption/proof cases.

No DMG was rebuilt. The old accepted `1685de6` candidate is superseded for the new
scope and remains byte-identical. The existing release draft stays unpublished
and unchanged. Build and accept a replacement only in a separately authorized step.

## Optional-module archive acceptance — retained procedure

Use the existing synthetic source `/Users/stefanantonsson/mno-birds-AFP613/journal`.
Do not replace it, copy database files into it, or use a production journal.

**Verified isolation:** `environment::data_directory` honors `MNO_TEST_DATA_DIR`
for DEV journal storage; the Tauri wrapper supplies identifier
`org.mynatureobservations.app.dev`. `bird_module::preference_path` instead uses the
account configuration base plus identifier, not the journal path. All three DEV
profiles in this procedure therefore share the same DEV module choice. Production
uses a different identifier/preference. Do not edit/delete either preference file,
change HOME, or change the production setting. Run one MNO DEV instance at a time.

### 1. Prepare persistent target roots and launch the source

Run once in a terminal (these commands are instructions, not executed acceptance):

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
export MNO_MODULE_ARCHIVES_ROOT="$(mktemp -d "$HOME/mno-module-archives-XXXXXX")"
mkdir "$MNO_MODULE_ARCHIVES_ROOT/archives" "$MNO_MODULE_ARCHIVES_ROOT/restore" "$MNO_MODULE_ARCHIVES_ROOT/import"
printf 'Keep this test root: %s\n' "$MNO_MODULE_ARCHIVES_ROOT"
test ! -e "$MNO_MODULE_ARCHIVES_ROOT/restore/journal"
test ! -e "$MNO_MODULE_ARCHIVES_ROOT/import/journal"
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

Keep that terminal/export for every subsequent command. In the startup log verify
the DEV data directory matches the source exactly. If it differs, stop.

If needed, enable the module only to identify existing fixtures: one linked and
one unlinked observation, photos/captions, row comments, weather and journal links.
Note recognizable values and the associated journal text/photo/caption/weather.
Do not delete or modify the fixtures. Then disable via Settings → Modules and
confirm bird UI disappears while the core journal remains usable.

With the module **off**, use Settings to create a Backup and an Export in the
new `archives` directory. Use distinct filenames; retain previous archives.
Record which is the backup and which is the open export. Do not enable the module
again in the source during this sequence. Quit MNO normally after completion.
If the dev server still runs, stop it only after the app is fully closed.

### 2. Restore into the fresh Restore target, initially disabled

```sh
test ! -e "$MNO_MODULE_ARCHIVES_ROOT/restore/journal"
MNO_TEST_DATA_DIR="$MNO_MODULE_ARCHIVES_ROOT/restore/journal" npm run tauri dev
```

Confirm the log identifies the Restore target, not the source. Its parent exists
but its journal child must be absent on this first launch. Check Settings → Modules
is off: this is the shared DEV choice left off in step 1, not an independent
per-journal default. Confirm normal core startup and no bird UI.

Use Settings → Restore, choose the newly created backup, and prepare Restore.
Follow the normal quit instruction. Relaunch **the same target**:

```sh
MNO_TEST_DATA_DIR="$MNO_MODULE_ARCHIVES_ROOT/restore/journal" npm run tauri dev
```

Complete **Verify and open journal**. Confirm the module remains off and journal
text, journal photo/caption, shared viewer and journal weather work; bird home/
sidebar actions, linked references/thumbnails, diary items and markers stay hidden.

Explicitly enable the module. Compare linked and unlinked observations, counts,
comments, photos/captions, manual/fetched weather and any stale context, and the
original journal links. Open the viewer from observation detail and a linked
journal. Quit normally and repeat the same launch command: choice stays **on**
and content persists. **Before moving to the Import target, turn the module off
in this Restore target and quit.** This also changes the shared DEV choice; it
is essential to test Import while disabled.

### 3. Open Import into a different fresh target, initially disabled

```sh
test ! -e "$MNO_MODULE_ARCHIVES_ROOT/import/journal"
MNO_TEST_DATA_DIR="$MNO_MODULE_ARCHIVES_ROOT/import/journal" npm run tauri dev
```

Confirm the Import target in the log and the module off. Use Settings → Open Import
with the new open export, review and confirm. Follow the quit/reopen instructions;
this replaces only the disposable Import target. Relaunch **that same target**:

```sh
MNO_TEST_DATA_DIR="$MNO_MODULE_ARCHIVES_ROOT/import/journal" npm run tauri dev
```

Complete **Verify and open imported journal** through the normal screen. Confirm
core journal content works and bird UI remains hidden; importing must not enable
the module. Explicitly enable and perform the same linked/unlinked content, photo,
caption, comment, weather, viewer and link comparisons as in step 2. Quit and
relaunch using that same command again: choice stays **on** and content persists.

If also checking off-state restart, disable in this target, quit and relaunch it
once more; core-only presentation should remain. This changes the shared DEV
choice for all DEV profiles, not their data. Leave it at the desired explicit
choice through Settings; never reset it by deleting files. Retain all source/
target roots and archives. Do not launch a target command with the source path.

The accepted results are recorded separately above. Retain this procedure for
repeatability; it does not establish future packaged acceptance.

## Final combined review and verification — 20 November 2026

Reviewed the full change against public main
`7d450c7f665392a69734e1040ece34fc0cc3221c`, including the previously untracked
module implementation, fixtures and report. **No concrete correctness or
data-preservation blocker found.**

The preference remains account-scoped, outside journal roots, with separate DEV
and production identities. Bird command guards cover reads and writes; admitted
operations finish before a preference transition, which invalidates preview slots.
UI safeguards and component disposal protect drafts and late results. Core journal
editors are not remounted. Startup, migration, archive proofs and shared integrity
rules remain independent; there are no schema/archive/dependency/version or
Windows/Linux activation changes. Existing test assertions were retained when
adapting command signatures and frontend lifecycle harnesses.

Final checks on the reviewed code, using Node 26.12.1 and Rust 1.99.1:

- Complete frontend suite: **688 passed**, zero failed/skipped.
- Rendered module control/section: **1 passed** (keyboard/focus, state, collapse
  without activation changes, localization and failure/uncertain-response handling).
- Complete macOS Rust library suite: **1,037 passed, 2 existing ignored**, zero failed.
- `cargo check --all-targets --locked`: passed.
- CI-pinned `cargo fmt --all -- --check`: passed.
- Svelte: **zero errors and warnings**.
- **60 local documentation file/heading links** and `git diff --check`: passed.

Owner-reported final DEV acceptance is recorded separately above. Replacement
packaged acceptance remains outstanding; UX-005 and deferred product scope are
unchanged non-blocking follow-ups. The old DMG's SHA-256 was reverified as
`8592c81a052a72298ea516ff581cb152b13734c12812bb952e38bde62c510108`.
The unpublished release draft and artifact were not changed, and no CI run or
replacement build was started. Source/target profiles and archives were preserved.
