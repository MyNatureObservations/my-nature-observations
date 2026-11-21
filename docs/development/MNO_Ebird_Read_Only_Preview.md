# eBird read-only import preview — local macOS DEV, 21 November 2026

Base: verified public main `2db4d2667808f632f71197994e45d227695d91c9`.
Branch: `codex/mac-ebird-import-preview`. Local checkpoint implementation; manual DEV visual assessment recorded below. [Agreed requirements](../requirements/Import_fran_eBird.md).

## Boundary

The enabled bird module's Observations overview exposes a DEV-only review entry,
separate from whole-journal Open Import. Rust checks macOS, development build and
module activation before opening the native CSV picker and checks module admission
again before the bounded read. It uses neither DatabaseState nor ID reservations.
No schema, archive, preference, place or journal content is written. The frontend
keeps only the selected file's preview; leaving disposes late responses. Native
picker cancellation preserves an existing preview. Original checklist/media links
open externally only after an explicit user action; no media is fetched by MNO.

The parser retains raw fields and physical line numbers, handles UTF-8 BOM, CSV
quoting/escaped quotes/multiline fields and omitted optional trailing fields.
It validates required columns/values, date/time/coordinates/numeric fields, IDs,
consistent checklist metadata and repeated taxa. Invalid rows remain represented
and issues are explicit, not silently corrected or dropped. X stays unspecified;
source names stay intact. Exact scientific-name candidates from the current
catalog are only name matches, not confirmed concept/version mappings. Source
place IDs/names/coordinates remain separate from any future local association.

The view shows current-file totals, validation issues, unmatched names and visits.
A visit expands to its complete species/counts/comments/media first, followed by
collapsed source details and optionally all original row fields. Only the first 100 issues are displayed, with that limit
stated; the result retains every issue. The per-checklist report is unsupported
with a localized instruction to use Download My Data. There is no Save/Import
button, persisted source comparison, reimport update or association writer yet.

## Verification

Synthetic fixtures only are in `app/tests/fixtures/ebird/`; no personal CSV or
identifying sample was copied into the repository. Focused frontend/controller,
rendered UI and Rust tests cover parsing/grouping/validation, unchanged raw values,
source-place distinction, exact-name-only candidates, media references, disabled
routes, cancellation, late results, explicit read errors and bounded file reads.
All-targets compilation and formatting use CI-pinned Rust 1.99.1; frontend tools
use Node 26.12.1. Final local evidence:

- 114 focused parser/presentation/controller and adjacent bird/module/navigation
  tests passed, including parsed chronological sorting, missing times, stable ties,
  timezone-independent localization and compact source-name previews.
- One isolated rendered browser test passed (source inspection, keyboard disclosure,
  external-link intent, cancellation and Swedish/English unsupported-format copy).
- Three Rust file-read/gate tests passed; all-targets compilation and CI-pinned
  formatting passed. Svelte: zero errors/warnings.
- Seven local documentation links and `git diff --check` passed.

These are automated checks, distinct from the limited manual visual assessment below.
The remaining DEV checklist is not claimed as accepted.

The supplied personal full export was inspected read-only through the actual
parser without storing its rows in the repository. It had no validation issues.
The supplied single-checklist report was correctly classified as unsupported.
This is local file/parser evidence, not manual desktop acceptance or a saved import.

## Same-profile macOS DEV checklist — pending

Quit the existing DEV app normally; keep all journal/profile/archive files.

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. Enable Bird observations through Settings → Modules if necessary (the DEV
   preference is account-level, shared by DEV profiles and separate from production).
   Open Observations → Review eBird export (DEV). Check the distinction from
   whole-journal Open Import and the explicit “nothing saved/no prior-import
   comparison” explanation in Swedish and English.
2. Check newest-day grouping, ascending local times and missing times last.
   Collapsed visits show at most three species/counts plus a remaining count.
   Expand a visit: birds, their comments and media appear before Source details.
   Choose the existing full `MyEBirdData.csv` from Downloads. Inspect the totals,
   an expanded visit's source place/date/time and original row fields, X counts,
   unmatched source names and multiple media references. Do not edit the CSV.
3. Click a media reference only if desired: the browser opens its Macaulay object
   page. MNO does not download an image. Confirm source IDs are not shown as new
   MNO-place associations or saved records.
4. Choose the single-checklist report: check the helpful unsupported-format
   message. Cancel the picker, then close the preview and return to Observations.
   Existing lists and journal links should be unchanged; there is no import result.
5. With the module off, confirm the DEV entry is hidden and the direct
   `/observations/ebird-preview` route gives an unavailable message with a core
   return link. No file picker should open. Check keyboard focus/disclosures.

No broad application suite, CI, PR, push or release is part of this step. Saved
import/reimport requires a separately versioned storage/archive foundation first.

## Preview presentation refinement

The initial metadata-heavy presentation was not visually accepted. The revised preview groups visits by newest date, then local start time (missing times last, checklist ID as tie-breaker). Compact species/count previews lead to complete species cards, with source comments and media beside their species. Technical and raw source values remain under Source details. The project owner subsequently gave this refinement a positive visual assessment (see the checkpoint record below); no import or persisted comparison is implemented.

## Review and local checkpoint — 21 November 2026

The project owner gave a positive visual assessment of the revised chronological,
observation-first preview. This records presentation feedback only, not acceptance
of every item in the DEV checklist, import persistence, packaged behavior or data
round-trips. The final separator-spacing correction remains available for visual
review; the rendered test asserts preserved spaces on both sides of each dot.

The complete local diff was reviewed for CSV parsing and omitted optional fields,
checklist consistency, exact count/X/name/media preservation, local chronological
ordering, invalid/unmatched input, restricted external URLs and macOS DEV/module
guards. No concrete correctness or data-preservation blocker was found for this
read-only preview. The file command has no database or ID-reservation access;
source data stays in memory, catalog matching only reads the existing registry,
and synthetic fixtures contain no personal exports. Test profiles and personal
source files were left untouched.

Existing passing evidence above is reused. After the separator-only adjustment,
12 focused parser/presentation/controller tests and one rendered interaction test
passed again (13/13). Svelte reported zero errors/warnings; seven documentation
links, including heading anchors, and `git diff --check` passed.
Persistence, reimport comparison, stable row ownership, place association and
media retrieval remain separate future work; no schema/archive changes are made.
