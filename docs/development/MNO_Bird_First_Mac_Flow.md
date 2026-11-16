# First macOS journal-to-bird-list flow (local implementation)

Base: `748ea0198447844026888d4d6e36142f6958fa7f`. User-reported macOS DEV acceptance
is recorded below; this is not packaged-release acceptance.

A saved, settled journal entry offers creation. Date and an existing coordinate-bearing
place are editable suggestions. The list owns its optional local start/end times.
Species come from the offline AviList v2026 catalog; rows require exact positive
integer counts. Saving uses the existing generation/search write guards and one
transaction for list, rows and first link. Cancellation does not save a list/link.
Linked previews show place/times, a differing visit date and the first three rows; reopening carries
an explicit originating-entry route. Editing and whole-list deletion were subsequently
added. Standalone, paste, photo and list-weather interfaces remain deferred.
Optional plain-text comments are supported during creation, editing and reopening. Windows/Linux commands reject this flow.

## Persistence boundary

The merged schema 5 did not contain visit times. Earlier local schema 6 added
nullable visit times; schema 7 removed place/date uniqueness to give each visit an
independent identity. Schema 8 added nullable `bird_rows.comment`. Current schema 9 adds durable
creation-request reservations and receipts, documented below.
Strict validators for older schemas remain frozen. Ordinary macOS startup upgrades
schemas 4–8 through the same verified private candidate publisher, retaining an
independent byte-identical source copy. Existing rows and links are not deduplicated.
Interrupted recovery is resolved before any upgrade. After older-schema recovery,
creation stays unavailable until restart admits current schema 9.

Schema 8 uses content proof 9 and backup/export format 9, binding all visit/row IDs,
order, taxon identities/versions, links, times and optional plain-text comments.
Older supported readers remain frozen. Imports from formats 5–8 use their original
reader/proof before private normalization to schema 8. New interruption records
bind source format separately from resulting schema/proof. Old applications reject
schema 8. Windows/Linux retain their startup schema and feature restrictions.
Process interruption tests do not prove power-loss safety.

Catalog provenance, owner attribution decision, exact source digest and reproducible
filtering are in `app/src-tauri/catalog/README.md`. Only species rows and the four
required identity/name fields are bundled; absent Swedish names remain selectable
with English/scientific fallback. No network is required for species search.

## Original macOS DEV checklist (acceptance recorded below)

Use only synthetic data. From `app`, create a disposable parent with `mktemp -d`
and set `MNO_TEST_DATA_DIR` to an **absent child** such as `$parent/journal` before
`npm run tauri dev`. Keep the same path for restart. Confirm the DEV log resolves
that directory before entering content.

1. Save a journal entry with date and a named coordinate-bearing place; open read mode.
2. Create a bird list. Check suggested date/place, change them, and enter optional times.
3. Search Swedish and English names offline. Add species/counts, including a species
   without Swedish name; check its fallback and availability. Add four rows.
4. Save; reopen via the compact preview. Check all rows, counts, times and place,
   and return to the originating entry. Restart with the same test profile and repeat.
5. Cancel another draft (including the leave confirmation); no additional list or
   preview should appear. Try missing place/count; saving must remain unavailable.
6. Check Swedish and English labels, keyboard species selection and place selection.
   Run Backup/Export only on this disposable journal if inspecting the new format.

These steps have not been performed by the implementation agent. Original photo
files are not modified by this flow.

## Creation loading correction

The first DEV attempt exposed premature form rendering: the availability reply
mounted DatePicker while the entry date was still empty. Rendering the original
page in that intermediate state reproduces `RangeError: Invalid time value`.
Creation now waits for entry/place initialization before mounting the form. An
invalid legacy date uses an editable date input rather than passing it to the
formatted picker. Initialization errors take precedence over the loading state
and offer localized retry guidance and the existing back action. No timeout,
backend gate change or journal mutation is used. Tests exercise the mount callback,
a deferred entry reply, real page/DatePicker rendering, every failing initialization
stage, both languages, unsupported availability and cancellation/back navigation.
The existing isolated DEV profile can be reused unchanged for the desktop retest.

## Save-result correction

Read-only inspection of the existing synthetic DEV profile found one already-saved
list, its two species/count rows and its journal link. On a separate SQLite copy,
readback succeeded and another create for the same place/date reproduced
`UNIQUE constraint failed: bird_lists.place_id, bird_lists.visit_date`. That retry
failed before any new commit; the original profile's digest remained unchanged.
There is no historical response log proving when the first successful save occurred.

That finding explained the later failed retry, not the original successful save's
reported failure. The original WebView exception/IPC response was not recorded;
terminal startup output does not establish the cause. It remains a historical
uncertainty. Do not describe UNIQUE as the explanation for that first result.

The owner's subsequent multiple-visit decision supersedes the place/date rule.
A creation attempt now reserves a UUID that is also its list ID. The same request
compares date, place, optional times, ordered species/version/count rows and origin
link inside the guarded transaction. Identical replay returns the saved ID without
writes; different content returns `bird_request_conflict`. A new attempt uses a new
ID and can share place/date/times. No upsert, cache-only receipt or custom locking
is introduced. The archive preserves the ID, so replay remains detectable after
reopening. Draft request identity is retained while the form remains open; after
leaving an uncertain form, inspect the journal before starting a genuinely new visit.

The UI preserves input and locks it while the result is uncertain. Check save resends
the same attempt instead of starting another. A confirmed response is independently
read back under the generation guard before navigation; confirmation and opening
failures retain the saved ID and offer reopening. Each failure stage is logged in
the WebView console rather than hidden behind one generic catch. These changes fix
missing post-commit confirmation/diagnostics, but do not invent a proven explanation
for the unrecorded original exception.

Tests cover multiple same-place/date visits with absent times, committed-response
loss followed by replay (modeled at the frontend, real persisted replay in Rust),
request/content conflict without byte changes, schema-6 migration/interruption/retry,
invalid candidates, archive round trips and activation with old formats. Native
WebView transport loss is not physically induced. Use the same existing test profile
without resetting it: startup retains its previous schema source copy before opening
the current schema described above.
Verify its old list first and create separate visits on the same place/date.

## Identity and search follow-up (2026-11-12)

The owner reports successful macOS DEV acceptance for multiple visits sharing a
place/date, with and without times, including reopening after restart. This is
DEV evidence, not packaged-release acceptance.

The bird create handler writes only list/row/link records. Its command-contract
regression now compares every stored journal-entry field before/after creation,
replay and restart. Journal prose remains independent. Current calendar markers
and day-panel queries still select journal entries only, and the calendar's plus
action opens `/journal/new`. Linked bird previews appear below prose within the
same journal article. No separate observation marker/calendar list exists yet;
that distinction is now an agreed future requirement, not implemented here.

The catalog already used Unicode lowercase for matching, but truncated substring
matches to 30 in taxonomy order. All of Skata/skata/SKATA had 189 matches, with
Pica pica at position 187 and therefore omitted. Exact names now rank before
prefixes, then other substrings, before the limit is applied. Equal-ranked hits
retain source order. Display labels/identities are unchanged. Real-catalog tests
exercise Swedish/English/scientific name casing and Å/Ä/Ö; a command-boundary test
checks the same Skata identity rather than a mocked catalog response.

## Owner acceptance and pending visual review (2026-11-12)

The owner confirms manual macOS DEV acceptance of corrected species search and
multiple visits at the same place/date, both with and without times, including
saving and reopening after restart. This accepts those functional checks only.
Overview density and the detail page's visual design remain pending UX review;
they are not accepted design. No redesign was performed during the catalog review.

The approved requirements and implementation plan now record data-updateable
reference catalogs, retained historical identity/readability, regional tie-breaking,
BirdLife Swedish names and an offline source-defined region chooser. None of those
future catalog/region functions has been implemented here. Historical name lookup
must be addressed before the first catalog replacement; the current fixed v2026
flow must not be presented as supporting catalog updates already.

## Presentation and comments refinement (local, awaiting DEV review)

Previews now use a compact unboxed link per visit below journal prose. They show
place/times and primary names/counts; the date appears only when different from the
origin entry. Detail uses journal width/metadata and softly outlined cards without
shadows. Per-installation name-view preferences are remembered in WebView local
storage and shared across list views; Swedish fallback still shows English and
scientific names when Swedish is absent. Preferences do not rewrite observations.

Schema 8 adds nullable plain-text comments (8192 UTF-8 bytes, no NUL) to bird rows.
It uses proof/archive format 9 and the same candidate upgrade/publication process.
Existing rows, including duplicate concepts, are preserved; no unique-concept
constraint is added. New command inputs reject duplicate concepts, and selecting
an existing concept in the form focuses its count without changing count/comment.
Comments participate in stable-request replay comparison and all new archive proofs.
Formats 5–8 keep their frozen readers and normalize privately when importing into
schema 8. Windows/Linux creation gates are retained.

Focused tests exercise preferences/fallback, duplicate selection, plain-text escaped
comment rendering, compact date-aware previews, retained legacy duplicate rows,
source-copy preservation, migration interruption/retry, restart and actual archive
activation with comment-bearing/new and supported older archives. Native power-loss
and packaged desktop acceptance are not implied. The new card and preview design
remains pending manual macOS DEV review.

Local verification for the comment/presentation refinement: full macOS Rust library
suite 974 passed, 2 existing ignored; full frontend suite 577 passed; focused UI
suite 61 passed; Svelte check 0 errors/warnings. Tests include literal multiline
HTML-like comments, null comments, replay/content conflict, legacy duplicate rows,
protected migration/retry and new/older archive activation. Actual visual macOS
acceptance of this revised presentation remains pending.

## Existing-list editing (local, uncommitted)

The owner accepted the improved visual direction before this step. The shared form
now serves creation and editing from the detail metadata row's accessible options
control. Editing changes date, coordinate-backed place, optional visit times and
ordered species/count/comment rows. Save and Cancel return to the list under the
originating journal route. Creation and its request identity remain separate.

An edit holds the existing search write/generation guard and one SQLite transaction.
It requires the list and originating link to exist and compares a content token
covering the loaded list and all links. Updates preserve list identity and every
journal link; older duplicate species and recorded taxonomy versions remain intact.
Full content/reference verification precedes commit. No schema/archive change is
needed. A stale or uncertain response retains form data and blocks resubmission;
the localized instruction is to reopen/check the saved list before another attempt.
Successful saves emit `mno:journal-changed`; mounted linked previews reread content.
Windows/Linux availability remains unchanged.

Focused coverage includes editing/cancellation and origin navigation, the actual
edit command payload, stale/deleted-list and generation rejection, rollback after
SQL changes fail final content verification, multiple-link preservation, retained
duplicate rows, and edited content through restart and Backup/Restore and
Export/Open Import verification. Manual macOS DEV editing acceptance is pending.


## macOS DEV acceptance: existing-list editing

The owner confirmed that manual macOS DEV list-editing acceptance passed using
its existing isolated test profile. This records DEV acceptance, not packaged
release acceptance. The improved visual direction is accepted; prior automated
persistence/archive evidence remains separate.

## Whole-list deletion (local work)

The list-detail options menu shares the existing confirmation dialog, with Cancel
focused and focus restored on dismissal. Confirmation identifies place, localized
date and optional visit times, explains permanent deletion of all species/comments
and all journal links, and preservation of journal prose, the saved place and
original photos. This is deletion, not unlinking.

Deletion checks the loaded content token under the existing write/generation guard
and one SQLite transaction, uses existing foreign-key cascades, verifies the
remaining content before commit, and never removes journal entries or shared
places. A late edit requires an existing list and cannot insert it. A lost response
is resolved by a generation-checked read-only existence query; no destructive retry
is automatic. If status cannot be confirmed, the detail remains with a localized
error and a read-only status check. After confirmed absence, previews refresh and
navigation returns to the originating entry, or the existing journal reader when
that entry cannot be used. No schema/archive or Windows/Linux gate change.

Synthetic tests cover cancellation, explicit confirmation, stale content and
generations, rollback after DELETE/cascades, multiple links, unchanged journal /
photo / place content and unrelated lists, late-update rejection and absence after
restart. The owner subsequently accepted the manual macOS DEV whole-list deletion behavior
in the existing isolated profile; this is not packaged-release acceptance.


## Exact visit times and crow search investigation

The shared creation/edit form now uses a compact time control with direct HH:MM
entry, native hour/minute selectors (00–23 and every minute 00–59), and an explicit
clear action. Existing optional-time validation and stored strings remain unchanged.
The popup supports keyboard navigation, Escape/focus restoration, and disabled
states during writes. No schema changes.

The source CSV/catalog investigation and sourced Swedish **Kråka** display
supplement are recorded in [catalog provenance](../../app/src-tauri/catalog/README.md).
The owner subsequently approved using this name consistently and an explicit
`gråkråka` search referral that states registration is at species level. The
original source JSON/CSV, Avibase IDs and recorded versions remain unchanged;
English naming and display preferences remain intact. This does not introduce
subspecies registration, general BirdLife integration or regional search.


## Final acceptance and complete-branch review (2026-11-13)

Reviewed base is `748ea0198447844026888d4d6e36142f6958fa7f` (HEAD and the local
`origin/main` tracking ref), including all subsequent uncommitted tracked and new
files. The separate local `main` ref is older; it was not used as the feature base.
The owner accepted creation/reopening/multiple visits, editing/cancellation/deletion,
compact previews/cards, comments/preferences/search, and the final time/crow
presentation. See [the acceptance and final test record](testing.md#bird-observations-accepted-macos-dev-flow-2026-11-13).
These DEV observations are distinct from archive/migration/rollback tests and do
not claim packaged acceptance or physical power-loss guarantees.

### Review blocker: creation replay could resurrect a deleted list

Historical finding; see the subsequent local correction below.

`bird_observations.rs:100–118` uses the list row as the only creation-request
receipt. If that row is missing, an otherwise valid UUID is inserted as a new list.
`bird_observations.rs:284–293` deletes that row and its cascaded data without
invalidating the generation or retaining independent request-lifecycle evidence.
`BirdListForm.svelte:109–129` intentionally resends the same creation request after
an uncertain response. The existing late-edit rejection is correct but does not
protect this separate creation endpoint.

A focused probe in a temporary copy of the public source reused the existing
synthetic `comments::current` fixture and production create/delete/exists functions:

1. Save a list with creation request ID R and retain the original arguments.
2. Confirm deletion and verify absence in the same generation.
3. Replay the identical creation request R.
4. Observe that the same list ID, rows and originating journal link exist again.

The probe reproduced this behavior. It added no source/test change to the checkout
and never opened the user's test profile. Existing replay tests cover a still-live
list and deletion tests cover late UPDATE, leaving this combined sequence uncovered.

Smallest required correction: establish a bounded creation-request lifecycle that
survives list deletion for as long as an old request can still be submitted, or an
equivalent guard that rejects that replay without breaking independent visits or
safe lost-response handling. Add an explicit create → lost response → delete →
replay regression through the command boundary. A particular storage mechanism or
schema change is not selected in this review. No implementation fix or checkpoint
commit has been made.

### Other reviewed boundaries and deferrals

No additional concrete blocker was identified in the reviewed source for protected
schema-4–7 → 8 candidate upgrades, old-source copies and retries; format-9 proofs
and supported older archive normalization/readers; atomic edit/delete and link
preservation; independent visits; or sourced catalog display and taxon identities.
macOS activation remains explicit in startup/commands; Windows/Linux advanced-schema
and bird-feature gates remain in place. Native Windows/Linux execution was not run
for this macOS review. The source-level replay blocker overrides green suites.

Code organization/readability cleanup can wait. Remaining feature scope includes
standalone overview/entry, linking existing lists and managing additional links,
paste review, row photos, list weather, catalog updates and regional search/map
selection. Future non-bird observations/imports remain out of scope. No deferred
feature or settled product decision was added or reopened by this review.

Final complete frontend suite: **600 passed**. Complete macOS Rust library suite:
**978 passed, 0 failed, 2 existing ignored**. Svelte: **0 errors / 0 warnings**.
All-targets compilation, CI-pinned Rust **1.99.1** formatting and whitespace checks
passed. The additional isolated replay probe confirmed the blocker; it was not a
second full-suite run. No push, PR, CI dispatch or release action was performed.


## Durable request correction: schema 9 / archive 10

A single `bird_creation_requests` table stores request ID, nullable original
payload SHA-256 and completed flag. The backend issues a durable UUID reservation
before creation. Arbitrary missing IDs are never accepted as fresh requests; the
frontend retains the returned ID for uncertain saves. A lost reservation response
can leave an inert reservation but no observation list or link. No event log is
introduced.

Creation and receipt completion use one existing generation/search-guarded SQLite
transaction. Request fingerprint v1 binds date, place ID, ordered species IDs and
versions/counts/comments, originating entry and optional times. Matching completed
requests return the existing list even after edits, without re-adding links.
Changed payloads fail. Deletion preserves the completed receipt; original replay
fails as retired, including after reopening. A new reservation permits an identical
independent visit. Failed transactions leave no partial list, rows or links, and
do not complete the request. Place/date/content uniqueness is not used.

Protected macOS startup upgrades schemas 4–8 to 9 using the verified candidate and
independent source-copy path. Existing live IDs are backfilled as completed with
unknown original digest: older schemas never stored that information. Their old
creation attempts fail closed rather than inventing history; existing lists remain
readable/editable/deletable. Deleted IDs already missing from schema 8 cannot be
reconstructed, but unreserved missing IDs cannot enter the new creation boundary.
The test profile is not accessed by this implementation. Its next DEV launch will
perform the ordinary protected upgrade and retain a schema-8 source copy.

Current Backup/Export uses format 10/schema 9/proof 10 and preserves all receipts,
including retired IDs and pending reservations. Open Export adds the checksummed,
counted `bird_creation_requests.jsonl`. Validation rejects malformed states/digests,
inconsistent references and missing required live-list receipts before activation.
Frozen older readers remain intact. Older format-5–9 exports normalize private
candidates to schema 9 in both preview and activation. Older backups still restore;
after Recovery completes, ordinary restart upgrades before bird commands become
available. No upgrade runs beneath unfinished Recovery.

Older archives cannot contain history they never recorded. Their live IDs are
reserved during normalization; missing old IDs are rejected, not re-created. The
replaced journal and its receipt state remain in pre-restore protection. Restore
marker 10 and import markers 21–26 bind original archive format separately from
target schema/proof; older marker contracts remain strict. Deliberately restoring
a whole earlier snapshot can restore its earlier live lists: there is no separate
anti-rollback authority. Windows/Linux schema and activation gates stay unchanged.
No dependency or accepted UX layout change is needed.

Production-function tests cover lost-response replay, replay after edit, deletion /
restart / replay, changed content, independent identical visits, rollback, source
preservation and interrupted upgrades, actual format-10 Backup/Restore and
Export/Open Import, older exports 5–9 and backups 5/9, protection and reopening.
After older restores, missing request IDs are still rejected and fresh reservations
permit genuine new visits. Malformed receipt archive fixtures recalculate checksums
and counts to test substantive validation. Native IPC loss and power-loss safety
are not claimed. New manual schema-9 DEV acceptance remains separate.

The replay blocker is addressed by this local correction. No checkpoint commit,
push, PR, CI dispatch or release is authorized in this step.


Final local verification for the durable-request correction: **601 frontend tests
passed**; **983 macOS Rust library tests passed, 0 failed, 2 existing ignored**.
The five focused receipt tests passed, including actual archive activation and
malformed receipt payloads. Svelte: **0 errors / 0 warnings**. All-targets
compilation, CI-pinned Rust **1.99.1** formatting and `git diff --check` passed.
The initial integration run exposed three version-specific fixture expectations;
those were corrected at their intended schema boundaries without weakening old
readers. The new malformed-archive fixture was corrected to use the export API's
required read-only connection. The final complete suites are green. No native
Windows/Linux CI or new manual schema-9 acceptance is claimed.


### Species pointer selection: investigation, 2026-11-13

The owner reports the remaining schema-9 DEV flow works, but selecting a search
result requires two clicks, including after typing in an already active MNO window.
That interaction is not accepted. No cause or correction is yet established.

An isolated harness mounts the actual `BirdListForm.svelte` and its child controls,
with synthetic command replies and a nonpersistent macOS WKWebView. Native Cocoa
key events type the query; native mouse events click the second result, without
prior keyboard highlighting. One click selected the species and focused its count.
The same result held with 40 scrollable results, a click on the name's child span,
and separate animation frames between mouse-down and mouse-up. An edit fixture
with six rows selected an existing species in one click, retained six rows and
preserved its count and comment. These are isolated component/native-WebKit
observations, not verification of the owner's running Tauri window. Earlier
Chromium execution likewise did not reproduce the defect.

The source has no search-blur handler or separate pointer highlighting stage.
Selection runs through the result button's click handler; the existing-row branch
only focuses its count. Without reproduction or the failing Tauri event sequence,
changing activation to mouse-down or adding a delay would be speculative. No such
change was made. The outstanding diagnostic boundary is the actual Tauri window's
first-click event/focus/layout sequence. No user Console work is required by this
record, and no profile was opened, reset or changed. Diagnostic harnesses and logs
are temporary files outside the working tree.

Focused verification after this investigation: **45 bird frontend tests passed**,
none failed/skipped; Svelte **0 errors / 0 warnings**; `git diff --check` passed.
These passing checks do not establish that the reported two-click defect is fixed.
Only the acceptance/investigation records were edited in this follow-up; application
source and the user's synthetic profile remain unchanged.


### Full-row species hit area: local correction

The owner narrowed the reproduction: clicking the name works immediately, while
clicking the blank right side of the visible row does not. The form already uses
one semantic button and one `click` selection handler per result. The correction
makes that button an explicit full-width grid block with border-box sizing, zero
margin and `appearance: none`; result list items have zero padding/margin. The
whole row, including its padding, therefore belongs to the same button. No row
click handler, pointer-down activation, duplicate dispatch or delay was added.
The existing keyboard and count-focus implementation remains unchanged.

The original gap was still not reproduced in the isolated WKWebView: before this
change, the measured button already covered the row and a right-edge click worked.
The exact runtime-specific reason for the owner's narrower hit area is therefore
not proven. This is a focused explicit-layout correction for the reported area,
not a claim that an unobserved event-order root cause was established. Acceptance
in the actual DEV window remains pending.

`cd app && node --test tests/bird-species-hit-area.browser.mjs` runs the actual
hydrated form with synthetic Tauri replies in installed Chrome (override its path
with `CHROME_BINARY`); it does not download a browser or touch a journal. It measures
button/row bounds, verifies the click target is beyond the name on the actual button,
then performs a real mouse-down/up sequence. It covers a non-highlighted second
result's blank right side, bottom/right padding, a 480px viewport, reselecting an
existing species without duplicating it or losing its comment/count, and native
ArrowDown/Enter/Escape. This explicit browser regression is separate from the
normal dependency-free controller suite and requires installed Chrome.

Local results: **1 rendered-browser regression passed**, **45 bird UI tests
passed**, Svelte **0 errors / 0 warnings**, `git diff --check` passed. A disposable
native macOS WKWebView also selected from the right/bottom blank area once and
focused `bird-count-0`. No schema, persistence or user-profile changes were made.
The work remains uncommitted; no push, PR or CI was started.


## Final checkpoint review and known UX limitation (2026-11-13)

The owner reports that the latest full-row hit-area change **did not resolve** the
reported macOS Tauri DEV behavior: clicking the species name selects immediately,
while clicking the blank right-hand area of the same visible row still requires
two clicks. This is a **known, non-blocking UX defect**, not accepted interaction.
It is tracked as [UX-005 in public backlog #4](https://github.com/MyNatureObservations/my-nature-observations/issues/4).
Investigation stops here by owner instruction. Preserve the existing hit-area work
and its browser regression; no additional speculative fix is made. Passing isolated
browser/WebKit tests do not supersede the actual DEV observation.

### Review outcome

Reviewed all tracked and new branch changes against base
`748ea0198447844026888d4d6e36142f6958fa7f` in the public repository, including the
subsequent schema-9 receipt correction and the final UI-only follow-ups.
**No remaining concrete correctness or data-preservation blocker was identified.**
This is a local checkpoint review, not approval to release or a claim of native
cross-platform acceptance.

- `backup/restore_context/bird_upgrade.rs` prepares a separate schema-9 candidate
  from schema 4–8, compares preserved-content proofs before/after migration,
  independently reopens it and retains a byte-identical old-schema copy before
  replacement. Existing startup ownership, unfinished-Recovery handling and retry
  barriers remain in use. No migration is performed underneath unfinished Recovery.
- `bird_observations.rs` performs create/edit/delete within SQLite transactions,
  search-write and generation guards. Validations occur before commit. List identity
  and journal links survive edits; deletion cascades only owned rows/links, leaving
  journal prose and shared places. Creation does not insert a journal entry.
- Schema 9's `bird_creation_requests` is independent of list deletion: a lost-response
  retry returns a live list, changed request content is rejected, and replay after
  deletion cannot recreate the list. A new reserved identity permits a genuinely
  independent visit, even at the same place/date with identical content. Failed
  edits cannot upsert a missing list; uncertain delete is checked read-only, not
  blindly repeated.
- Backup/Restore and Export/Open Import use format/proof 10 for schema 9, preserving
  live and retired receipts as well as linked/unlinked lists, row comments, visit
  times, recorded taxon IDs/versions, places, journal documents and image previews.
  Older supported readers and proof contracts remain; private normalization checks
  old content before calculating a newer proof. Activation/protection and marker
  contracts distinguish source format and target schema. Restoring an older whole
  snapshot can restore its earlier content; no global anti-rollback history is claimed.
- Catalog IDs and recorded taxonomy versions remain storage identities. Display
  supplements and the crow referral preserve the source data and concept identity;
  attribution, provenance and the owner's licensing interpretation remain recorded.
  Catalog updates, taxonomic remapping and regional data remain future work.
- macOS alone enables the new schema/commands/UI. Windows/Linux bird-feature and
  advanced-schema startup restrictions remain. No platform identity, production
  directory, published release, dependency or CI change was introduced.

### Evidence reused and checks affected by later changes

Reuse the recorded complete suites after the durable-request correction:
**601 frontend tests passed**; **983 macOS Rust library tests passed, 0 failed,
2 existing ignored**; all-targets compilation and CI-pinned Rust 1.99.1 formatting
passed. Backend source has not changed since those results. Subsequent changes
were UI hit-area styling, its explicit browser regression and documentation.
Their focused results are **45 bird UI tests passed**, **1 rendered-browser test
passed**, Svelte **0 errors / 0 warnings**. The current documentation links and
staged whitespace are checked for this checkpoint; no full suite is repeated.

The accepted manual macOS DEV observations remain creation/reopening and multiple
visits, editing/cancellation/deletion, compact previews/cards, comments/name
preferences/search, time control and crow presentation. The owner also reports
that the remaining schema-9 DEV flow works. These reports are separate from
synthetic archive/failure tests, packaged acceptance, power-loss guarantees and the
explicitly unaccepted blank-area species interaction above.

The remaining deferred feature scope is unchanged: standalone lists/overview,
linking existing lists and additional-link UI/unlinking, paste review, row photos,
list weather, catalog updates and regional search/map selection. Readability and
performance refinements can be reviewed separately; no broad refactor is needed
for this checkpoint. No push, PR, CI, merge or publication is part of this step.


## Standalone lists, overview and journal links — local next stage

Base: public main `788a740734ca871f7590641a9a563df7fff11e24`, branch
`codex/mac-bird-observations-overview`. Local implementation, **not manually
accepted yet**. The existing synthetic profile has not been opened or modified by
the development tools. UX-005 remains non-blocking and was not investigated or
changed in this stage.

- The existing main navigation exposes **Observationer / Observations** only when
  the backend reports bird availability. The overview reads `bird_lists` owners,
  not joined journal links, and shows linked and unlinked visits once each, with
  date/place/optional times and a compact species preview. Order is visit date
  descending, then start time and ID; separate visits remain separate rows.
- Standalone creation reuses `BirdListForm`, time picker, comments, species search,
  name preferences and leave-page protection. It defaults to the local calendar
  date and sends a null journal origin. The same creation transaction writes no
  journal entry or link. Durable request reservation/replay protection is unchanged.
- Journal and standalone routes mount the same `BirdListPage` detail/controller.
  Context is carried by the route, not inferred from a list's links. Opening from
  a journal returns to that entry; overview opening returns to the overview, even
  when the list has several links. Edit Save/Cancel returns to the corresponding
  detail route; deletion returns to the origin or the existing safe fallback.
- The saved journal entry offers a picker for existing lists and a separate
  **Remove link / Ta bort koppling** confirmation. Link/unlink uses one guarded
  transaction, checks both owners, and changes only the selected join row. The
  primary key makes linking idempotent. The UI checks the desired state read-only
  after an uncertain command response; neither action invokes whole-list deletion.
  Cancel is focused by default and dialog focus returns when its trigger survives.
- Existing edit/delete guards, cascades, source protection and platform gates are
  retained. No schema, archive, taxonomy, application identity or directory change
  is needed: schema **9**, archive/proof **10** already represent these states.

Local checks: **610 complete frontend tests passed**; **52 relevant Rust library
regressions passed** (`cargo test --locked --lib bird_`), including existing
candidate upgrades, archive activation/retry and receipt preservation. After the
last focused additions, **53 bird UI tests** and **2 overview command tests**
passed; Svelte reported **0 errors / 0 warnings**; all-targets compilation,
CI-pinned Rust **1.99.1** formatting and whitespace checks passed.

New synthetic backend tests exercise null-origin creation through the command
contract, no extra journal entry/link, lost-response replay and retired requests,
overview uniqueness across two links, standalone editing/link preservation,
unlink/journal-deletion preservation, stale/missing-owner rejection, rollback and
restart. Format-10 Backup and Export candidates independently match the complete
content proof for unlinked and multi-linked lists. Existing full activation
regressions were reused; this does not claim new native desktop archive acceptance.
Frontend tests exercise route context, cancellation, link/unlink failures,
localization/rendering, availability and existing journal/Settings regressions.
The one earlier frontend failure group was an export fixture that did not model
the new availability query; only that mock and a gate regression were updated.

### Same-profile macOS DEV check (pending)

Close the existing DEV process and relaunch from this checkout with the same
isolated directory; do not reset it:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. Confirm existing synthetic journal entries/lists still open. In main navigation,
   open **Observations**; check old linked and unlinked lists are discoverable.
2. Start a standalone list, enter a row and cancel/discard. Confirm there is no new
   list or journal entry. Then save a new disposable visit with counts/comment and
   optional times. Reopen it, edit or cancel, and return to the overview.
3. From a disposable saved journal entry, choose **Link an existing list**. Cancel
   once, then link the new visit. Link it from a second synthetic entry. The
   overview must show it once; opening from each entry returns to that origin.
4. Edit the shared list and check both journal previews. Cancel removal of one
   link, then confirm it. The list and second link must remain. Check Swedish and
   English labels, keyboard focus, Escape and the existing leave-page protection.
5. If testing journal deletion, use a newly created disposable entry: its list
   must stay discoverable. Cancel then confirm deletion of a newly created list;
   its links disappear, while journal prose, saved place and unrelated lists stay.
6. Restart with the same profile and verify saved content and link state. UX-005
   remains known: click the species name to select directly; blank-area behavior
   is not part of this stage's acceptance claim.

No push, PR, CI, package or publication action was performed. Paste, photos, list
weather, catalog/region updates and calendar markers remain deferred.


## Owner acceptance of standalone/linking flow and local refinements

The owner reports that the supplied standalone/linking macOS DEV acceptance
checklist passed on the existing isolated profile. This accepts that earlier local
flow within the checklist's scope; it is not packaged-release acceptance, native
Windows/Linux acceptance or a new claim about archive/crash failure scenarios.
The following requested refinements were initially awaiting a new DEV check;
the owner acceptance recorded on 2026-11-16 below supersedes that pending status. They do not resume investigation of non-blocking UX-005.

- Creation actions/headings now say **Skapa fågelobservation / Create bird
  observation**. Related user-facing labels use observation consistently; one
  observation is still one visit with multiple species. Storage/API table names
  remain list-based and unchanged.
- The overview groups by calendar date, newest first. Start times sort ascending
  within a day; null start times come after timed visits. A binary list-ID tie
  break is stable. Missing times are not fabricated. Each owner remains one item,
  with place/optional times and its species preview under the date heading.
- The same time control now contains two native text inputs in one cohesive field.
  Tab and Shift+Tab follow the native hour/minute focus order and can leave the
  control normally. Typing a colon advances to minutes; full HH:mm paste is
  supported. Clearing, all 60 minutes, the existing picker and strict optional-time
  validation remain. Partially entered times stay invalid rather than acquiring
  invented missing values. Stored representation stays nullable HH:mm.
- Observation detail has a compact journal-links section and **Link to journal
  entry** chooser. Entries are identified by localized date and title, falling
  back to a plain-text excerpt from the existing journal-search text helper (or
  the existing untitled label when no text exists). Existing links are displayed
  as navigation links and disabled as duplicate choices. Separate unlink confirms
  the selected entry and preserves observation, prose and all other links.
- Link writes reuse the existing guarded transaction. The new journal-target read
  runs under a generation permit and a verified SQLite snapshot; it never writes.
  Link changes refresh the detail's edit token before edit/delete is allowed. If
  refresh fails, actions stay blocked with an actionable message.
- Return context stays route-owned even after the originating link is removed.
  Reading/editing the independent owner uses the existing null-origin boundary;
  first creation from a journal still commits its first link. The backend's
  generation, identity and optimistic-token guards remain intact, as does its
  explicit link check when a journal origin is supplied to that boundary.

Verification: **616 complete frontend tests passed** (including **59 bird UI
checks**), **1 real-rendered time-keyboard browser test passed**, **53 bird Rust
regressions passed** and **10 journal-search regressions passed**. Svelte: **0
errors / 0 warnings**. All-targets compilation, CI-pinned Rust **1.99.1** formatting
and whitespace checks passed. The browser test uses temporary Chrome data and
native key events to verify Tab/Shift+Tab into/out of segments, colon typing,
clearing and invalid-time preservation; it is not a Tauri desktop acceptance claim.
Synthetic production tests verify untimed-last ordering, stable ties, unique
journal choices, title/excerpt metadata, duplicate links and targeted preservation.
A failed renderer assertion was corrected to match Svelte's safe `<` escaping;
HTML-like user text remains plain text.

### Same-profile retry for these refinements (accepted; procedure retained)

Use the same DEV launch command and profile above; close/restart the DEV process
so new Rust commands are registered. Do not recreate or reset the directory.

1. Check **Skapa fågelobservation / Create bird observation** in both languages.
   Confirm existing visits still show their species, counts and comments.
2. Within one date, compare e.g. 08:15, 18:07 and no start time. The untimed visit
   must be last; the newest date group is first and each visit appears once.
3. Enter hours, Tab to minutes, Shift+Tab back, then Tab out. Try colon-based
   HH:mm typing, clear, an exact minute and an invalid/partial time. Save only valid
   complete or empty values, then reopen to verify the existing representation.
4. From an observation opened through a journal, link another synthetic entry.
   Check date/title or excerpt, Cancel and existing-link disabling. Unlink the
   original entry with confirmation: the observation and other link must remain,
   and Back still goes to the original entry.
5. Edit/save after that unlink to verify refreshed token and unchanged return
   context. Open from the overview and verify return to the overview. Restart and
   check links/content. UX-005 remains unchanged; select by clicking the species
   name when registering.

All changes remain local and uncommitted. No schema/archive change, profile reset,
main-navigation redesign, calendar marker, photo/paste-input/list-weather/catalog
or region feature was added. Windows/Linux gates are unchanged. No push, PR, CI,
build packaging or release action was performed.


## Standalone flow checkpoint review — 2026-11-16

**Accepted macOS DEV checks**, as reported by the owner: date grouping and visit
ordering; segmented keyboard time entry; linking and targeted unlinking from
observation detail; return navigation and persistence after restart. This records
owner-observed desktop behavior, separate from automated synthetic/failure tests.
It is not packaged-release acceptance or a claim of Windows/Linux bird support.

Reviewed the complete tracked and new-file change set on
`codex/mac-bird-observations-overview` against public main
`788a740734ca871f7590641a9a563df7fff11e24`. Public origin is
`MyNatureObservations/my-nature-observations`. **No concrete correctness or
data-preservation blocker remains in this review.** No implementation changes
were made during this checkpoint review.

| Boundary reviewed | Finding |
| --- | --- |
| [Command boundary](../../app/src-tauri/src/bird_app.rs) and [creation form](../../app/src/lib/components/BirdListForm.svelte) | Standalone creation supplies no journal origin; the existing atomic creation operation inserts no journal entry or link in this case. Durable reserved request IDs still prevent duplicates after a lost response and resurrection after deletion. Journal-origin creation retains its first-link transaction. |
| Overview read and [grouping](../../app/src/lib/birds.ts) | SQL reads list owners without joining journal links; each identity appears once, including unlinked observations. Both SQL and UI order newest date first, ascending actual start times, untimed visits last and binary ID ties. Sorting does not write or fill missing times. |
| [Link transaction](../../app/src-tauri/src/bird_observations.rs) | The search-write guard, expected generation permit and SQLite transaction cover owner checks and the exact selected join row. The primary key prevents duplicate links; unlink never deletes an owner. Verification precedes commit, and injected failure rolls back. Missing/deleted owners and stale generations fail rather than being recreated. |
| Journal deletion | Existing foreign-key cascades remove its join rows, not the independently owned list or species rows. Tests preserve both observation content and shared place references; unlinked observations remain discoverable. |
| Read/edit/delete lifecycle | Ready/generation checks and optimistic edit tokens remain. Link changes refresh the token before edit/delete, and failed refresh disables those actions. Late updates cannot upsert a missing observation; creation receipts survive whole-list deletion. Uncertain link responses use read-only confirmation; uncertain whole-list deletion is not blindly repeated. |
| [Return context](../../app/src/lib/components/BirdListPage.svelte) | Route context controls Back independently of link membership. Opening from a journal returns there even after unlinking it; overview opening returns to the overview. Independent-owner read/edit uses null origin while retaining list identity, generation and content-token checks. |
| Archives/platforms | Database schema, schema-upgrade code, storage proofs, Backup/Restore and Export/Open Import implementations are byte-unchanged from the base. Schema 9 / format 10 already preserve linked, multi-linked and unlinked lists and receipts. New archive candidates match full content proofs; existing activation/retry regressions passed. New commands explicitly reject non-macOS use; Windows/Linux startup/feature gates stay unchanged. |

Reused the recorded passing results: **616 frontend tests** (including **59 bird
UI tests**), **1 rendered time-keyboard browser test**, **53 bird Rust regressions**
and **10 journal-search regressions**; Svelte **0 errors / 0 warnings**; all-targets
compilation and CI-pinned Rust 1.99.1 formatting passed. No test uncertainty required
another application test run. Documentation file links and whitespace are checked
again for the checkpoint, including new files after staging.

**Non-blocking follow-ups retained:**

- [UX-005](https://github.com/MyNatureObservations/my-nature-observations/issues/4):
  clicking a species name selects immediately, but the blank right side can still
  require two clicks in the owner's macOS Tauri DEV window. The earlier hit-area
  change did not resolve it. It was not reinvestigated or changed here.
- Main-navigation **design** remains deferred. The functional Observations entry
  is part of this flow, but no broader navigation redesign is implemented or
  claimed accepted by these checks.
- The already deferred calendar markers, species paste input, photos, list weather,
  catalog updates and regional features remain outside this checkpoint.

The existing test profile and unrelated files remain untouched. This work order
ends with a local checkpoint only: no push, PR, CI, merge or release action.
