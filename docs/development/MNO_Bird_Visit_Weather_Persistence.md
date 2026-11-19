# Independent visit weather — persistence foundation

Base: public main `e9e5ce6810f17d81a3501d6ddd037c9228b29a90`.
Branch: `codex/mac-bird-visit-weather-persistence`. Persistence checkpoint
`34af3c9079f9a844ad4c9242e441f673a182cb33`; manual UI and reviewed daily-fetch
work below are included in the final local review. The owner accepted the manual
and daily-fetch flows and presentation, including isolated archive round-trips.
Product decisions are recorded in [the requirements](../requirements/Egna_fagelobservationer.md#besökets-väder--beslut-19-november-2026) and [implementation plan](../requirements/Egna_fagelobservationer_implementation.md#4-listans-eget-frivilliga-väder-och-samlad-acceptans).

## Model and boundaries

Schema 12 adds only `bird_visit_weather`, keyed by list ID and `manual`/`fetched`.
Each part stores its original JSON value, original visit context (date, place ID,
name and coordinates, optional start/end), and a sticky stale flag. Original
context does not depend on a historical place still existing. No list ID is put
into `journal_weather`. List deletion cascades to owned weather; journal deletion
only removes journal links. Existing journal text, weather and photos remain owned
by the journal.

The existing strict weather-value validator is reused. Manual values describe a
visit; fetched values must be daily and use the existing `historical-weather`
reanalysis or `forecast-daily` source contracts. Provenance, model, attribution,
retrieval time, timezone and daily aggregates are retained in the fetched JSON.
Resolution and forecast status are derived from the validated part, never guessed
from visit times. Today's data cannot be labelled completed historical weather.
Hourly and current-weather snapshots are not accepted as fetched visit weather.

Internal create/update variants use the existing list write/generation guard,
transaction and edit token. Weather is part of the token; nonempty weather is also
bound to creation receipts, retaining the old request digest for no-weather calls.
Invalid parts roll back the list, rows, links and receipts together. Late writes
must still match generation, list existence, edit token and captured context.
There is no weather upsert that recreates a missing list.

Ordinary list edits and shared-place edits invalidate weather inside their existing
transactions. Staleness is sticky: reverting date/place/times or coordinates does
not reactivate old values. Reads also compare the captured context. The current
summary excludes stale parts. Manual reuse and replacing stale manual data require
an explicit confirmation; reuse rebinds the manually confirmed values to the new
context. Fetched parts remain independent and stale until replaced/removed.

## Upgrade and archives

Ordinary macOS startup uses the existing protected candidate upgrade to schema 12,
retaining the independently verified source copy. Pending Restore/Import/Recovery
keeps its existing startup barrier. Full schema validators recognize only the exact
new layout. Existing journal-weather schema-10/11 compatibility remains, and the
same strict reader also recognizes schema 12 on macOS.

Backup format 13 uses the verified SQLite snapshot. Export format 13 adds only
`bird_visit_weather.jsonl` to the exact ZIP inventory, using existing checksums,
row counts and limits. Content proof 13 includes source JSON, context and stale
state alongside the frozen previous content proof. Import rejects invalid sources,
contexts, references, duplicate parts and malformed inventory/records before
activation. Older supported production exports (formats 5–12) are first verified
using their frozen contracts, normalized privately to schema 12 with no visit
weather, then verified again. Frozen older readers are retained.

New restore marker 13 and import markers 42–50 explicitly bind source format to
schema 12/proof 13; older marker contracts remain unchanged. Previous journals are
still protected before replacement. No second recovery authority is introduced.
Older app versions must reject the new schema/archives; they are not forward
compatible. Windows/Linux startup and feature gates remain unchanged.

## Remaining work

The initial checkpoint had no visit-weather UI. Subsequent sections record the
manual-only UI and the later reviewed daily-fetch implementation, both using the
existing macOS list Save transactions. Manual behavior/presentation has DEV
acceptance; daily fetching and isolated archive round-trips also have DEV
acceptance as recorded below. Neither has packaged acceptance. Existing test profiles, user journals and archives were not used or
modified by automated development checks.

The internal fetched-value input is not a public command boundary. The daily-fetch
commands below reuse the server-held preview/receipt pattern, bound to generation,
edit token and visit context, rather than accepting provider JSON from the
frontend. No generic weather framework or hourly selection is introduced.

## Local validation — 19 November 2026

All Rust checks use the unchanged CI pin, **1.99.1**.

- Complete macOS library suite: **1,021 passed, 0 failed, 2 existing ignored**.
- Final strengthened visit-weather suite: **10 passed**, including atomic create,
  receipt retry/conflict/deleted-request replay, independent journal weather,
  explicit manual confirmation, sticky date/time/place staleness, coordinate-edit
  invalidation, rollback after partial weather/cascade work, expired generations
  and late writes to missing/stale lists.
- Archive coverage includes linked/unlinked/stale parts, multiple journal links,
  full format-13 Restore and Open Import activation/admission/restart, restoration
  of a schema-4 backup and normalization/activation of a format-12 export. The
  replaced journal is deliberately different and its weather is verified in the
  protection archive. Each source export format 5–12 is separately normalized and
  compared with its frozen original content proof. Rehashed invalid weather
  payloads are rejected for semantic/reference reasons, not merely checksums.
- Protected schema-11 → 12 upgrade: **2 passed**, covering interruption/retry at
  every existing publication checkpoint, exact retained source bytes, original
  journal/photo/weather content and invalid-candidate rejection.
- Affected weather filter: **83 passed, 1 existing ignored**. The final stronger
  schema-10/11 compatibility regression also passed separately. Existing photo
  suite: **5 passed**; current overview archive regression: **1 passed**.
- All-targets compilation and pinned formatting passed. No frontend code changed,
  so frontend/Svelte tests were not repeated. Documentation links and
  `git diff --check` passed.

The complete suite was run before the last test-only strengthening; those changed
tests were then rerun successfully. The final place regression also explicitly
passed switching to a different place ID at identical coordinates, retaining the
original context and stale status. The first broad run exposed current-schema
expectations still pinned to schema 11 and two unchanged subprocess race failures
(`startup_success_and_retry_matrix`, `early_pending_validation_keeps_live_b_c_locks`).
Current expectations were updated only after identifying their ordinary-startup
fixtures; frozen older contracts were retained explicitly. Both subprocess tests
passed in isolation, and the final full run passed without relaxing assertions.
No root cause for the initial subprocess races is claimed.

Native Windows/Linux tests and manual macOS visit-weather acceptance were not run.
The UI/network gates remain closed; the existing synthetic user profile was not
opened. The next ordinary macOS startup with this branch upgrades an eligible
profile to schema 12 through the protected path, so any further DEV review should
continue using disposable data, not the production journal.

## Review correction — numeric coordinates

Review found that comparing raw JSON numbers rejected valid integer coordinates
such as `59` when the captured context serialized the same f64 value as `59.0`.
The comparison now uses the validated numeric values, with exact f64 equality and
no rounding or geographic tolerance. Existing finite-number, range, type and
required-field checks still run before comparison. Schema 12, archive format 13
and the stored JSON/proof contracts are unchanged.

After this correction, **11 focused visit-weather tests passed** with CI-pinned
Rust 1.99.1. New cases cover integer/decimal/mixed forms for manual and fetched
weather, tiny genuine differences, missing/non-numeric coordinates, out-of-range
and non-finite values. The production guarded creation and Backup/Restore plus
Export/Open Import candidate paths preserve integer-form coordinates unchanged.
Rehashed export payloads with differing or invalid coordinates are rejected without
changing the active journal. All-targets compilation, pinned formatting and
`git diff --check` passed again; unaffected full-suite evidence above was reused.

The reviewed coordinate blocker is resolved. No further concrete correctness or
data-preservation blocker was identified. Test profiles and existing archives were
not opened or modified; no visit-weather UI or network path was activated.

## Manual-only UI — functional DEV report; visual review pending

Creation/editing now has a compact Weather control alongside visit metadata.
Its dialog embeds the existing WeatherForm fields, units, validation and translations
without a nested HTML form or an independent Save action. All changes, including
explicit removal, are staged until the observation's normal Save; existing Cancel,
leave-page and uncertain-response safeguards remain in charge.

The existing macOS create/update commands accept only manual Set and explicit
Remove intents. They use the guarded weather-aware list transactions. Captured
place/date/times/coordinates must match the transaction's current context. The
reader returns validated list-owned weather; no journal-weather write is used.
No schema, archive format, permission/capability or platform-gate change is needed.

Detail shows a compact current summary, a manual label and expandable details.
Stale values are shown separately with their original place/date/times/coordinates
and excluded from current summaries. The form retains sticky staleness even after
changing back, and an explicit “Use for this visit” action confirms reuse against
the current context.
A further context change clears that confirmation. Saving unrelated edits retains
old weather as stale; modifying/reusing it requires confirmation. Removal remains
separate and takes effect only on Save. Concurrent shared-place coordinate changes
still fail the existing context/token checks rather than rebinding silently.

Validation covers local add/edit/cancel/remove, invalid and uncertain Save results,
sticky staleness, explicit reuse, both languages, compact/detail output and absence
of nested forms/fetch controls. A production command-contract regression verifies
creation, lost-response retry, stale rejection/confirmation, restart, removal,
rollback, generation rejection and independence from stored journal weather.
Archive/migration tests from the foundation remain applicable and were rerun in the
affected bird test filter. The owner reports that manual weather behaves as
expected; this is functional DEV feedback, not visual or packaged acceptance.

Recorded local results for the manual UI:

- Full frontend suite: **643 passed** before the last focused additions/corrections.
  Final affected bird/weather UI suite: **102 passed, 0 failed, 0 skipped**.
- Affected macOS Rust `bird_` filter: **77 passed, 0 failed, 0 ignored**, including
  the new real command-contract test, protected migration and archive coverage.
- CI-pinned Rust **1.99.1** all-targets compilation and formatting check passed.
- Final Svelte check: **0 errors, 0 warnings**. `git diff --check` passed.

For manual DEV review, quit any running DEV instance and relaunch the same
synthetic profile from `app/` after generating localization:

```sh
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

Create a visit with manual weather, Save and reopen/restart. Edit weather and
Cancel to verify the previous values remain. Change the visit date/time/place
without confirming reuse: save and confirm old weather is stale, retains its
original context, and contributes nothing to the current summary. Explicitly
confirm reuse in Edit, Save and reopen. Also check coordinate changes on a shared
place, explicit weather removal and Cancel, and that linked journal weather stays
unchanged. Repeat the visible labels and keyboard flow in Swedish and English.
These steps remain useful for repeat checks; only the owner's reported functional
result is recorded as observed DEV feedback.

## Presentation refinement — visual acceptance pending

The owner reported that manual weather behaves as expected but did not accept its
presentation. Weather now uses the journal's recognizable icon/control conventions
beside date/place metadata in creation, editing and observation detail. The compact
native modal dialog contains keyboard focus, supports Escape, and restores focus
to the trigger. No large inline editor is shown below species rows.

“Done” applies weather only to the observation draft. Panel Cancel/Close/Escape
restores the weather draft from when the panel opened, including fields, removal
and confirmation state. The observation's own Cancel/Save and uncertain-response
protections remain in place. Implicit outer-form submission while the panel is
open is blocked. A context change after staged reuse requires fresh confirmation.
Removal feedback remains visible beside the metadata control after closing.

Overview and linked-journal previews use the shared compact weather summary, with
existing icons/units and only current visit-owned parts. Missing/stale weather adds
no summary markup. The journal entry's own weather remains separate. The shorter
stale question is “The visit has changed. Does the weather still apply?”; original
context and source information stay under expandable details. Reuse is explicit.

Final checks for this presentation refinement: **648 frontend tests passed, 0
failed, 0 skipped**; Svelte **0 errors, 0 warnings**; `git diff --check` passed.
Coverage includes local panel snapshots/cancellation, explicit reuse and renewed
confirmation, current/stale/missing summaries, metadata placement, real shared
field restoration and keyboard/focus handlers. Native macOS visual/keyboard
acceptance remains for the owner; controller/render tests do not establish it.
No storage, schema/archive, network or platform-gate changes were made in this
presentation step. Existing profiles and archives remain untouched.

## Date-change review and layout correction

The owner reported that changing a saved visit from 18 to 19 November left only a
generic Weather control in detail, with no clear review indication. An isolated
production-path regression reproduces precisely that date-only edit: Save sends
no weather changes, both original weather JSON/context strings remain unchanged,
the stored stale flag is true, and restart reopens the November 19 visit with its
November 18 weather retained but excluded from current summaries. This passed
before the visual correction. No persistence defect was found in that sequence.
The pre-fix rendered UI regression failed because the closed control still said
only “Weather”, indistinguishable from missing weather.

Editing and saved detail now visibly say “Weather needs review” at visit metadata.
Date changes do not automatically open a dialog or require reuse: an unchanged
weather draft can still be saved as stale. Opening review groups the question,
original visit context, explicit “Yes, use this weather” and removal together
before the fields. Confirmation errors sit in that group. From saved detail the
actions enter the existing Edit route and focus the requested choice; they never
automatically confirm, remove or save weather. The user confirms there and uses
Done followed by the observation's normal Save. Cancel/Escape and draft snapshots
remain unchanged. New date/context changes invalidate prior confirmation.

Current overview/journal-preview weather uses available space to the right of
place/time and wraps when needed. Missing/stale values still add no current
summary. The editor dialog is capped at 780px and viewport width, with conditions
left and numeric fields/description right; below 680px these columns stack. The
44px-minimum reuse button stays beside removal and validation.

Final local checks: **651 frontend tests passed, 0 failed, 0 skipped**; Svelte
**0 errors, 0 warnings**; pinned Rust 1.99.1 date-edit/restart regression **1
passed**; pinned formatting and `git diff --check` passed. Existing Rust production
logic was not changed. Isolated Chromium geometry checks in Swedish and English
at actual viewport widths 1000px and 500px verified same-row preview weather,
side-by-side/stacked editor columns, viewport bounds, action height and review
placement. A 240px preview with a long place name wrapped weather without overflow.
These synthetic browser checks do not establish visual acceptance in macOS Tauri.
The user's profiles and archives were not opened or modified.

## Preview weather adjacency

The owner found the other weather presentation changes clearer and requested one
further preview adjustment. Weather now follows visit time immediately, or location
when time is absent, with the journal metadata's compact 8px/16px spacing. It no
longer aligns to the right edge or follows a differing visit date. Any extra date
follows the weather; flex wrapping remains natural at narrow widths. The same
shared preview serves the overview and linked journal previews. Current/stale
filtering, review actions and all persistence behavior are unchanged.

Focused bird UI checks: **73 passed, 0 failed, 0 skipped**, including timed/untimed
visits in both preview contexts and both languages. `git diff --check` passed.
This records the owner's feedback, not acceptance of the new adjacency layout.

## Reviewed daily fetching — live DEV confirmed; revised UI pending

The owner accepted manual weather behavior and presentation in macOS DEV,
including the final inline placement immediately after visit time, or place when
time is absent. This is distinct from automated coverage and packaged acceptance.
The owner subsequently confirmed live daily fetching in macOS DEV. Presentation
and confirmation were not accepted; the later refinement below remains pending.

### Daily service and representation

Visits always request daily data for their own validated coordinates and date.
The journal's current-conditions behavior is unchanged. Recent/local-today visits
use ECMWF daily forecasts through the existing forecast endpoint; older visits use
the existing ERA5 archive path. The API's local offset routes near-midnight days;
there is no substitution of current conditions for a visit time. Existing HTTP,
response-size/time limits, strict units/value/source validation, error mapping and
attribution are reused. Unsupported/future dates and missing data fail visibly.

Existing schema 12/archive format 13 already represents daily period, requested and
sampled coordinates, timezone, model, forecast/reanalysis source, retrieval time,
attribution and validated canonical-unit values. No schema/archive change is
needed. Review identifies today's full-day result as a forecast which can include
the remainder of the day. Stored detail derives the retrieval-local day from the
saved timestamp/timezone, rather than relabelling it on restart. The shared detail
component accepts an optional visit-only forecast label; journal defaults remain
unchanged. Existing fixed-offset aggregate/DST limitations remain; no precise
visit-time or hourly claim is introduced.

### Preview, draft and guarded Save

`bird_weather_fetch` holds bounded, volatile server receipts (10-minute TTL,
maximum 16 ready receipts). Binding includes generation, owner/request identity,
expected list edit token when editing, full draft visit context and place-register
revision. Creating a visit may reserve its existing durable creation-request ID
early; this writes no list, rows, journal links or weather. Reservation is shared
between fetching and Save to avoid competing IDs.

No database permit or connection spans the network request. The backend validates
the binding before and after the response, and again on explicit review. Context
changes, cancellation and superseding requests reject late responses. Review
changes only the form draft. The normal Save sends an opaque preview ID; provider
JSON is recovered server-side and checked again inside the existing guarded list
transaction before writes. Manual Set and fetched Set remain separate; manual
display precedence is retained. A completed creation retry still uses the durable
request proof, while an expired uncommitted preview cannot write.

Panel Cancel/Escape revokes its new/unapplied previews and restores the prior draft.
An applied preview is still only a draft until observation Save. New context makes
it stale; fetched weather must be fetched again or removed, never manually
reconfirmed. Existing saved stale weather can remain preserved while a changed
visit is saved. Refetch does not change manual fields. Known pre-write preview
rejection enables safe refetch without losing entered data; an already uncertain
creation remains locked to its original request. HTTP may finish after cancellation,
but its late result is rejected and cannot be applied.

Receipts are not persisted or archived and are invalid after restart, expiry,
context/generation change or bounded-cache eviction. Already saved weather remains
fully readable and archived independently of those receipts. No Windows/Linux
feature gate, capability, application identity or data path was changed.

### Validation

- Frontend: **659 passed, 0 failed, 0 skipped**.
- Affected bird Rust filter: **82 passed, 0 failed, 0 ignored**, including guarded
  creation/editing, rollback/expiry/deleted-owner rejection, independent journal
  weather and complete stored-record preservation through Backup/Restore and
  Export/Open Import candidates.
  After strengthening the early-cancellation race, the **4 server-preview tests**
  passed again. Cancellation nonces are remembered in a bounded volatile queue so
  cancellation arriving before the blocking HTTP task starts also prevents fetch.
- Historical/journal/visit weather filter: **37 passed, 0 failed, 1 existing ignored**
  (`live_forecast_contract_synthetic_coordinates`, explicit live HTTPS test).
  Fixtures cover local-today, recent/history routing, UTC/local-date boundaries,
  exact visit coordinates, missing values, invalid units, service/network failures
  and rejection of current-only responses. No live external calls were used.
- UI controller/render tests cover server-bound preview/review, cancellation,
  context/generation changes during requests, expired results, draft-only apply,
  manual precedence, forecast labels, stable request identity and uncertain retry.
- CI-pinned Rust **1.99.1** all-targets compilation and formatting passed; Svelte
  **0 errors, 0 warnings**; `git diff --check` passed.

The owner subsequently confirmed that live fetching works in macOS DEV. This
does not establish every service failure scenario or packaged acceptance; automated
fixtures and command-contract tests remain distinct evidence. No user profile or
existing archive was opened or modified by development checks.

### Same-profile macOS DEV check

Quit any running DEV instance, then from `app/`:

```sh
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. Use a disposable visit with its own coordinate-backed place and today's date.
   Open Weather → Fetch daily weather. Confirm daily/whole-day forecast wording,
   source/retrieval details and that manual values remain separate.
2. Cancel the panel once and confirm saved weather is unchanged. Fetch again,
   choose Use weather (which now closes the dialog), then Save the observation.
   Reopen and restart using the same command/profile; verify both weather sources
   and the journal entry's unchanged independent weather.
3. Try yesterday and an older historical date. Check daily aggregates rather than
   current conditions at a visit time. An unsupported future date must show an
   understandable error without losing manual input.
4. Change date/place/time after saving. Confirm stale fetched values are excluded
   and can only be fetched again or removed. Cancel an in-progress fetch, change
   the visit and retry; the old result must not appear or be applied.
5. Check Swedish/English labels and keyboard/Cancel/Escape behavior. Do not Restore
   or Open Import over this source profile; archive preservation is already covered
   by isolated automated targets. These instructions do not claim manual acceptance.

## Fetched confirmation and information hierarchy refinement

Owner feedback: live daily fetching works in macOS DEV; its presentation and
confirmation flow are not yet accepted. The new “Use weather” action performs the
existing server/context/expiry review, validates any pending manual input, updates
only the observation form and closes the dialog with focus restored to its trigger.
There is no additional Done step for fetched review. Only the observation's normal
Save persists the result. Done remains for the separate manual-only editing path.

Failed review, expiry or invalid manual input leaves the dialog and fetched preview
open with an actionable error. Nothing is applied on that failure. Untouched
manual weather, including deliberately retained stale values, is not overwritten or
removed. Pending valid manual edits remain in the observation form as before.
Cancel before application restores the previous panel state and revokes its pending
preview; cancelling the observation still leaves saved content unchanged. A late
second close cannot revoke an already accepted result.

Read-only visit weather now exposes condition, temperature, daily min/max, wind and
precipitation directly. Manual/fetched parts use restrained side-by-side sections
where space permits and stack at narrow widths. Daily date/scope and the forecast
notice appear once; the value labels retain their actual mean/max/sum semantics.
Model identifiers, retrieval time, secondary fields and original stale context are
under “Source and details”. Open-Meteo/CC BY attribution stays visible. The redundant
panel summary and repeated preview copy are removed; interface text no longer calls
the form a “draft”. Stale review and explicit manual reuse remain unchanged.

The shared WeatherDetails component offers optional compact rendering; journal
weather keeps its existing defaults. No backend, persistence, schema/archive,
fetching scope or platform-gate change was made in this refinement.

Validation: **663 frontend tests passed, 0 failed, 0 skipped**, including **121
focused bird/weather checks**. Coverage includes integrated one-action apply/close,
expiry/application failure staying open, Cancel, unchanged manual records, visible
localized values/attribution and metadata ordering. Svelte: **0 errors, 0 warnings**;
`git diff --check` passed. Existing passing Rust results were reused because this
step changes no Rust code. Isolated Chromium fixtures in Swedish and English at
1000px/500px widths verified visible primary values and attribution while source
metadata remains collapsed. This is not native macOS visual acceptance.

Same-profile DEV retry: fetch weather, then choose Use weather once. Verify the
dialog closes, save the observation normally and reopen Weather. Check that useful
daily values are already visible, Source and details expands model/retrieval data,
and manual weather remains separate. Also try Cancel before using the preview.
Profiles and archives remain untouched by development checks; no commit/push/CI.

## Accepted weather DEV checks and isolated manual archive procedure

The owner accepted macOS DEV behavior and presentation for manual visit weather,
live daily fetching and the simplified Use weather flow. Older dates, stale fetched
weather after changing visit dates, refetching and persistence after restart were
also accepted. These are owner-reported manual DEV results, separate from the
recorded automated results. They are not packaged-release acceptance. The owner subsequently completed the
archive procedure below; its accepted results are recorded in the final acceptance
section. No manual archive operation was performed by the development agent.

### Prepare source and isolated targets

Use this public checkout/branch and the existing synthetic source only:
`/Users/stefanantonsson/mno-birds-AFP613/journal`. Do not use the production app or
production journal. Keep one Terminal tab for this procedure; do not repeat the
initial `mktemp` setup when restarting a target. All targets and archives remain
under this new, persistent home-directory parent; no cleanup is prescribed.

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
export MNO_WEATHER_TEST_ROOT="$(mktemp -d "$HOME/mno-visit-weather-archives.XXXXXX")"
mkdir "$MNO_WEATHER_TEST_ROOT/archives" "$MNO_WEATHER_TEST_ROOT/restore" "$MNO_WEATHER_TEST_ROOT/import"
printf 'Keep this test directory: %s\nArchives: %s/archives\n' "$MNO_WEATHER_TEST_ROOT" "$MNO_WEATHER_TEST_ROOT"
npm run i18n:compile

mno_weather_restore_target() {
  MNO_TEST_DATA_DIR="$MNO_WEATHER_TEST_ROOT/restore/journal" npm run tauri dev
}
mno_weather_import_target() {
  MNO_TEST_DATA_DIR="$MNO_WEATHER_TEST_ROOT/import/journal" npm run tauri dev
}

MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

Quit any other DEV instance before launching. Check the logged DEV directory on
every launch. Only one profile using this DEV identity may run at a time. After
quitting, wait for the Terminal prompt; if only the development launcher remains,
stop it with Ctrl+C after the app has closed. Never stop an archive write midway.

In the source UI, identify existing disposable visits satisfying the following
fixture (add missing synthetic examples only if needed), and note their places,
dates/times, values and links or take screenshots:

- A linked visit with current manual weather and current fetched daily weather.
  Record its source, daily date, units, retrieval time and forecast status; use a
  result fetched for its local today to cover the full-day forecast notice.
- An unlinked visit with preserved stale fetched weather: change its visit date
  without refetching/removing/reusing the old part. Record the original context in
  Source and details. It must show Needs review and contribute no current summary.
- The linked journal entry's independent weather, text, journal photo/caption and
  linked observation photo/caption/comment where present. Keep original photo
  files where they are; this procedure does not move, remove or modify them.

Save and reopen any changes before archiving. In Settings, create a Backup named
`visit-weather-source.mno-backup` and an Export named
`visit-weather-source.mno-export`, both in the printed `archives` directory. Use
Cmd+Shift+G in the native file picker to reach that directory. Wait for each success
notice, then quit the source app. Do not reopen the source for Restore or Import.

### 1. Backup → Restore

First launch only: the journal child must be absent, including as a dangling link.
These guards stop the launch if it already exists:

```sh
test ! -e "$MNO_WEATHER_TEST_ROOT/restore/journal" &&
test ! -L "$MNO_WEATHER_TEST_ROOT/restore/journal" &&
mno_weather_restore_target
```

Confirm the DEV log ends in `/restore/journal`. In Settings → Restore, choose the
source `.mno-backup` from `archives`, then confirm Replace journal. When instructed,
use Quit MNO. After the app exits, restart **the same target**:

```sh
mno_weather_restore_target
```

Choose Verify and open journal on the recovery screen; do not choose Keep previous.
After the successful Home confirmation, run the comparison below. Quit normally
and run `mno_weather_restore_target` once more to repeat it after another restart.
Then fully quit this target before starting the import target.

### 2. Export → Open Import

First launch only:

```sh
test ! -e "$MNO_WEATHER_TEST_ROOT/import/journal" &&
test ! -L "$MNO_WEATHER_TEST_ROOT/import/journal" &&
mno_weather_import_target
```

Confirm the DEV log ends in `/import/journal`. Leave this fresh journal empty.
In Settings → Import, choose `visit-weather-source.mno-export`. Confirm the checked
preview with Import journal (or the offered replacement confirmation if applicable).
When the app says the journal switch is prepared, use Quit MNO. Restart the same
target:

```sh
mno_weather_import_target
```

Choose Verify and open imported journal, or the corresponding Verify and open
journal action offered for this import; do not choose the previous journal. After
the successful verification/Home confirmation, compare below. Quit and run
`mno_weather_import_target` again to confirm preservation after another restart.

### Shared user-visible comparison for both targets

- Locate both linked and unlinked visits in Observations. Check place/date/times,
  species/counts and that the unlinked visit is still unlinked.
- Compare manual values and fetched daily values with the source notes, including
  min/max/wind/precipitation units where present. Source/date, retrieval metadata
  and forecast status must survive. Forecast status describes the original
  retrieval, not whether the archive was restored on a later calendar day.
- Check the stale visit: Needs review remains visible, the original weather and
  context remain available, and no stale values appear as current conditions.
  Do not refetch or reuse during this preservation comparison.
- Open the linked journal entry. Its own weather must remain unchanged and
  independent; its observation preview must open the same linked visit. Check
  text, photos, captions, row comments and the shared viewer from both entry and
  observation. Photos use the existing original/preview policy; original files
  are not copied into a backup by this test.
- Repeat after the extra restart. Record PASS/FAIL separately for Restore and
  Open Import, with the generated target parent. If a check fails, keep all
  profiles/archives and report the mismatch; do not reset or retry over the source.

No injected interruptions, direct SQLite inspection or proof manipulation are
needed here; existing isolated automated tests cover those boundaries. This is a
prepared checklist, not a claim that either manual archive path has passed.


## Final owner-reported macOS DEV acceptance — 19 November 2026

The owner accepted the following in the synthetic DEV profiles. These are manual
results, distinct from the automated synthetic tests and not packaged acceptance:

- Manual weather, cancellation, removal and explicit reuse.
- Daily fetching for today and older dates, the simplified **Use weather** flow
  and the final presentation, including inline weather after visit time/place.
- Stale weather after visit-context changes and persistence after restart.
- Backup → Restore into a separate disposable target, checked again after another
  restart; Export → Open Import into another target, also checked after another
  restart.
- Preservation of weather values, source/date/forecast metadata and stale original
  context, for linked and unlinked observations, alongside unchanged journal
  weather, photos, captions and journal links.

Source/target profiles and archives were preserved. This acceptance does not claim
Windows/Linux visit-weather activation, hourly precision, packaged acceptance or
resolution of UX-005.

## Final combined review and checks — 19 November 2026

Reviewed the complete branch, including previously uncommitted UI/fetching work,
against public main base `e9e5ce6810f17d81a3501d6ddd037c9228b29a90`. No concrete
correctness or data-preservation blocker was found. Independent manual/fetched
parts retain the existing summary precedence; stale parts are excluded without
discarding their original context. Place/context invalidation and saving share
the guarded transactions. Server-held previews are rechecked for context, owner,
expiry and generation before application and inside the saving transaction.
Cancel remains draft-only.

The review also checked protected schema-12 upgrades, source preservation, exact
archive-13 inventory/content proofs and activation-marker bindings, normalization
of supported older archives, strict schema-12 journal-weather compatibility,
daily/forecast labels and attribution. Windows/Linux activation gates remain in
place. No production code was changed during this final review.

Final checks on macOS, using the unchanged CI Rust pin **1.99.1**:

- Complete frontend suite: **663 passed, 0 failed, 0 skipped**.
- Complete Rust library suite: **1,031 passed, 0 failed, 2 existing ignored**.
  The ignored tests are the manual synthetic photograph measurements and explicit
  live HTTPS forecast verification; deterministic weather fixtures ran normally.
- Svelte check: **0 errors, 0 warnings**.
- `cargo check --all-targets`: passed.
- CI-pinned `cargo fmt --all -- --check`: passed.
- Changed-document validation: **34 local file/heading links** resolve across
  the three changed Markdown documents.
- `git diff --check`: passed.

Remaining follow-ups are packaged acceptance, native-dialog system localization,
known non-blocking UX-005 and deferred bird features. These are not asserted as
completed by this review. Existing profiles and archives were not touched; no CI,
push, PR, merge or release was started.
