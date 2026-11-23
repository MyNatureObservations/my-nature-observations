# Confirmed eBird import and reimport — local macOS implementation

Base: public main `09ccdeaa51c18e5d532c7790edeb3fae84e15379`.
Branch: `codex/mac-ebird-confirmed-import`.
[Requirements](../requirements/Import_fran_eBird.md) ·
[Persistence foundation and archive evidence](MNO_Ebird_Persistence_Foundation.md).

## Implemented boundary

An enabled bird module on macOS exposes the existing full-export preview from
Observations. It now compares the selected, strictly validated source rows with
stored snapshots: new, changed, unchanged and retained visits absent from the
file. Each category exposes its visits. Selecting/reading/reviewing a file does
not write journal content or reserve persistent identities. A separate native
confirmation dialog focuses Cancel first. Only explicit confirmation imports.
Windows/Linux remain unavailable; whole-journal Open Import remains distinct.

The server retains immutable input, generation, a fingerprint of the eBird
inventory (including links and additions), a random operation identity and a
15-minute expiry. Confirmation revalidates the module, generation and inventory
inside the guarded import transaction. Changes invalidate the review. The
existing atomic import and strict complete-content proof run before commit.
Identical input is a content no-op; absent visits are never deleted.

A sent confirmation cannot be submitted again through the same operation. Lost
responses query its status. An uncertain commit can be checked against its
verified post-import inventory without replaying writes. Status is process-local,
not a durable import log: after restart or expiry the client must read/review the
file again and obtain new explicit confirmation. It never automatically retries
an unknown operation. Import continuity and unique Submission IDs remain durable
in the existing schema. Cancellation drops only an unused review capability.
Disabling the module invalidates unused reviews and blocks further bird commands.
Pending import prevents navigation/module disable until the operation settles.

## Read-only imported visits

Imported visits appear once in the existing overview, combined diary and calendar
using their own local date and the existing bird marker. Success emits the shared
journal-change event. Source-specific detail shows From eBird, the checklist link,
source species/counts (including X), comments and all valid media references.
Names and source-place snapshots remain unchanged. No catalog identity/version is
inferred, even for a scientific-name match in preview. Source details retain the
raw field inventory and source coordinates, separately from user-owned MNO places.

Ordinary manual edit/delete commands cannot mutate source visits; direct edit
routes show the localized source-lock explanation. Journal link selection,
idempotent linking and targeted unlinking reuse guarded transactions through the
source-specific link table. Journal deletion preserves imported visits. Inactive
rows with MNO additions appear read-only under Additions to review; visit-level
additions are also shown. No reassignment, imported-row photo editing, media
fetching or place-association UI is introduced.

## Storage, verification and acceptance

No schema/archive change: schema **13**, proof/archive **14** and supported older
readers remain in use. Complete source records, inactive rows, additions and links
remain covered independently of module activation. The new confirmed-command
regression reopens the database and compares its content/proof through Backup
candidate verification and Export/Open Import candidate construction. Existing
foundation tests cover protected activation, restart and older archive readers.
Synthetic fixtures only; personal exports and the user's profile were not changed.

The user accepted the first confirmed import and revised presentation in macOS
DEV, followed by restart and confirmed identical reimport without duplicates.
Controlled changed/omitted/name-change reimport and reimport of the original
after restart are now accepted as recorded below.
These manual results are distinct from automated coverage and packaged acceptance.

The entry point and heading now say Importera från eBird / Import from eBird.
Species are sorted for presentation only by displayed common name, with a
locale-aware, case-insensitive comparison and stable input-order ties. Preview,
imported detail (including unresolved rows) and compact previews use the same
sorting helper. Source snapshots, row identities, matching, comparison and visit
chronology remain unchanged. Whole row objects move together; no rows are merged.
The existing UX-005 limitation and packaged acceptance remain separate follow-ups.

## Same-profile DEV checklist

Quit the current DEV app. From the public checkout's `app` directory:

```sh
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. Enable Bird observations if needed (the DEV account preference is shared across
   DEV journal profiles). Open Observations → Import from eBird. Select a full
   export; inspect categories, source species/X/media and checklist links.
2. Cancel the confirmation. Check that no imported visits were added. Read the
   file again, confirm once, then inspect the overview, diary and calendar.
3. Open a visit: source fields are read-only and From eBird is visible. Link it to
   two synthetic journal entries; remove one link and verify the other survives.
4. Restart with the exact same command. Verify visits and links remain. Review the
   same export: existing visits should be unchanged, with no duplicates.
5. If testing corrections/omitted visits, use disposable CSV copies, never edit the
   original export. Verify the comparison and confirm explicitly; absent visits
   remain. Automated synthetic tests cover unresolved-row additions and stale/
   lost-response paths that are not naturally reachable through the current UI.

Do not use production data. Later separate-target manual archive acceptance is recorded below.

## Automated evidence for this local step

- Complete frontend suite: **702 passed**. After the final review-capability
  replacement adjustment, affected eBird controller/parser tests: **15 passed**;
  rendered Chromium confirmation/cancel/lost-response test: **1 passed**.
- Svelte check: **0 errors, 0 warnings** after the final UI adjustment.
- Complete macOS Rust library suite with CI-pinned Rust **1.99.1**:
  **1,068 passed, 0 failed, 2 existing tests ignored**.
- Rust all-targets compilation and CI-pinned formatting check passed.
- Changed documentation relative-file links and `git diff --check` passed.

These are automated local results, not manual DEV or packaged acceptance.
These results were recorded locally before the checkpoint; nothing was pushed or published.


### Naming and display-order refinement

First import, revised presentation, restart and confirmed identical reimport
without duplicates are now accepted in macOS DEV. Controlled correction results are recorded separately below. Presentation-only refinement checks:
**116 tests passed**, including the rendered preview, Swedish Å/Ä/Ö ordering,
case-insensitive stable ties, unchanged row objects/source values, comments/media
remaining attached, chronological visit regression checks and manual-bird UI.
Svelte: **0 errors, 0 warnings**; `git diff --check` passed. No backend, matching,
schema or archive changes were made for this refinement.


### Controlled reimport acceptance preparation

Two private CSV copies were prepared outside the repository from the untouched
personal export: (1) a count correction, source-comment correction and omission
of one complete checklist; (2) the same changes plus an explicitly labelled
common-name change. Both were checked with the production CSV parser and an
independent field-by-field comparison; only the intended fields/checklist differ.
Personal checklist identifiers/values and the staged instructions are kept with
the private copies, not in repository fixtures. These checks validate the files,
not manual import acceptance. Subsequent user acceptance is recorded below.

Count/comment-only corrections should retain row continuity; a taxon-describing
name change creates a new active row and preserves the old inactive row without
moving MNO additions. The visible Additions to review section currently requires
an inactive row to have MNO additions. Without additions, retained rows remain in
storage/archives but do not create a visible review notice. Imported-row MNO
photo attachment/editing is not supported by this step.

Reimporting the original export restores its active source values and includes
the omitted checklist again. It does not undo the import history: retired rows
remain, including the test-name row, and old inactive row identities do not
reactivate. Additions remain on their original rows rather than moving back to
a new row with the original name. Visit identity and journal links remain stable.


### Controlled copy 02 — reported manual DEV result

The user successfully checked test copy 02, including the changed source name
and the preserved journal link. This records those explicit manual observations;
it does not claim every identity/archive check in the prepared procedure was
performed. Imported-row MNO photo editing remains unsupported. The follow-up
presentation puts creation/import in one wrapping action row and moves general
source-place/taxonomy explanations under Source details, retaining visible errors
and the Additions to review disclosure.


### Accepted controlled reimport — macOS DEV

The user confirmed identical import after restart without duplicates; count and
source-comment corrections updated the existing visit; a visit omitted from the
file remained in MNO; and a source-name change preserved visit identity and the
journal link. After reimporting the original and restarting, active source values
were restored and the journal link remained. These are user-reported manual DEV
results, separate from automated identity/proof coverage. No additional visual
acceptance of the latest UI refinements is inferred.

The [separate-target manual archive procedure](MNO_Ebird_Archive_DEV_Acceptance.md)
has now been accepted by the user in macOS DEV, as recorded below. It does not require SQL inspection or imply
support for imported-row MNO photo attachments.


## Final local review and acceptance checkpoint

User-reported macOS DEV acceptance covers first import, restart and identical
reimport without duplicates; count/comment correction; retention of a visit
absent from the file; source-name change preserving visit identity/journal links;
and original-file reimport restoring active source values after restart.
Backup/Restore and Export/Open Import were accepted into separate targets while
the bird module was disabled, followed by explicit activation and another restart.
These manual results remain distinct from automated content/identity/archive
coverage. No final visual acceptance beyond the reported checks is inferred.

The complete diff was reviewed against base
`09ccdeaa51c18e5d532c7790edeb3fae84e15379`: reviewed-input/state binding is checked
inside the guarded atomic transaction; stale state cannot commit; source-only
ownership and separate link/addition tables remain intact; unchanged reimport is
a no-op; unmatched/retired rows are preserved conservatively; source places do not
write MNO places. Existing module/platform gates and schema/archive contracts are
unchanged. Display sorting copies complete rows and does not change import input.
No concrete correctness or data-preservation blocker was identified.

Process-local review/status remains an explicit limitation: after process restart,
unknown status requires a fresh comparison and fresh explicit confirmation, never
an automatic replay. Persistent visit identities and missing-visit retention make
that new review safe; this is not a durable operation-history feature. Inactive
rows without additions remain stored/archived but are not shown by the review
section. Reassignment, imported-row photo attachments, media downloads, place
association, packaged acceptance and the existing UX-005 remain unfinished.

Future product direction: eBird is an optional **source connection within bird
observations**, not another observation type. Its separate activation lifecycle
is not designed or implemented here; disabling it must not delete imported
content. Current availability continues to use the existing bird-module choice.
Creating a new linked journal entry from an observation is also future work.

Validation reuses the recorded 702-test frontend and 1,068-test macOS Rust library
runs (two existing Rust tests ignored), Rust 1.99.1 all-targets compilation and
formatting, plus later focused presentation suites (latest: 105 passed). No full
suite was repeated for this review. Svelte, changed documentation links, personal
fixture exclusion and whitespace were checked before checkpointing. Personal
CSV files, test profiles and archives are outside this commit and remain intact.
