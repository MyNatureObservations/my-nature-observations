# Observation navigation, combined diary and calendar — local implementation

Branch: `codex/mac-observation-navigation-calendar`. Base: public main
`da15b1f78acb434de71e953cc0c754f439814e98`.

**Current status — 20 November 2026:** the owner has visually accepted the
navigation/calendar design, with the final quiet-link adjustment recorded below.
Earlier rejected layouts and pending-review notes are chronological history, not
the current specification. Manual evidence is visual DEV acceptance, not packaged
acceptance or a claim that all functional paths were manually retested. Final
automated validation and review are recorded at the end of this document.

## Design and scope

Implements the [agreed requirements](../requirements/Egna_fagelobservationer.md#navigation-och-typbaserad-kalender--beslut-19-november-2026),
following [restrained MNO design](../design/MNO_Design_Decisions.md#d-002--gränssnittet-ska-vara-diskret-men-begripligt)
and the [design specification](../design/MNO_Design_Specification_v1.0.md).

Current direction: preserve the two journal cards and Continue destination, with
one smaller integrated observation-entry compartment beneath. The sidebar remains
hidden by default, exposing Nature diary and Observations when opened. The earlier
text-link and four-card presentations were rejected; see the approved direction
and current checklist below. The Add observation disclosure and existing editor
guards remain in place.

## Calendar interaction

Green represents journal entries; deeper blue represents bird observations,
never an import source. One day can show both. The legend and date-button names
identify available types without requiring color recognition.

Selecting any date opens the existing day panel. Its journal section retains the
reading-view links and new-entry action. The observation section lists that day's
visits, including standalone visits, ordered by start time with untimed visits
last and stable identity tie-breaking. Multiple links do not duplicate a visit.
Observation links use the existing standalone detail route, whose Back destination
is Observations. This same panel handles mixed days and observation-only days.

The existing guarded `read_bird_lists` command is reused; calendar derivation reads
own visit dates and deduplicates identities. No write, schema, archive or backend
change is introduced. Availability is checked before observation reads, preserving
Windows/Linux gates. Journal changes refresh both marker types and the open day
contents; late or superseded reads cannot overwrite the current selection.
Recovery/journal replacement unmounts the ready navigation and reloads it on return
to Ready, so old calendar data is not reused across generations. Read failures
show localized feedback rather than silently presenting stale observation links.

## Same-profile macOS DEV acceptance — pending

Do not reset or alter the existing synthetic profile or archives. Quit the previous
DEV instance first. From this checkout:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. In Swedish and English, check Home → Write journal entry, Continue, Record
   observation and Observations. Record observation opens the existing bird form
   directly; Cancel returns to Observations. Continue keeps the journal reading view.
2. From a saved synthetic entry, use Add observation → Create new, then Cancel.
   Check Link existing; creation/opening through the entry still returns there.
3. In the left calendar compare journal-only, observation-only, mixed and empty
   dates. The mixed date shows two dots and two day-panel sections; observation-
   only dates show visits even without journal entries. Open each kind.
4. Add two synthetic visits on one date, including an unlinked visit; link one to
   two journal entries with different dates. Confirm one blue dot on the visit
   date, one panel item per visit, and unchanged green journal markers.
5. Edit one visit's date and delete another disposable visit. Confirm old/new
   markers and day-panel contents refresh. Restart using the same launch command.
6. Use Tab, Enter and Space for date buttons and Add observation. Check visible
   focus, readable legend and date labels describing both types. Opening or
   cancelling navigation from an unsaved journal must retain the existing safeguard.

This checklist is not yet manually accepted. UX-005 is unchanged and remains
non-blocking. No inactive observation modules, import providers, statistics or
weather changes are included. Existing profiles and archives were not accessed.

## Local verification — 19 November 2026

- Complete frontend suite: **671 passed, 0 failed, 0 skipped**, including eight new
  navigation/calendar tests. Existing fixture changes only provide the new
  read-only dependency; previous behavior assertions are retained.
- Isolated rendered Chrome keyboard test: **1 passed**. Enter/Space open
  observation-only/mixed dates, both content links are reachable, and the journal
  disclosure supports Tab and chooser cancellation with focus restored to its
  visible summary. The fixture supplies synthetic read-only IPC responses; this
  is not native macOS Tauri DEV acceptance.
- Existing Rust `bird_app::overview_tests`: **3 passed**, covering standalone
  ownership, multiple links, ordering, restart and archives. No Rust production
  code was changed; the complete library suite was not repeated.
- Svelte: **0 errors, 0 warnings**.
- CI-pinned Rust **1.99.1**: `cargo check --all-targets` and
  `cargo fmt --all -- --check` passed.
- **38** local documentation file/heading links resolve; `git diff --check` passed.

Changes remain local and uncommitted. No push, PR, CI dispatch or publication.


## Presentation revision — 20 November 2026 (not visually accepted)

The owner rejected the initial Home text-link presentation and found the marker
colors too similar. No visual acceptance is recorded. Home now has two coherent
card rows, **Din journal / Your journal** and **Dina observationer / Your
observations**, with localized short descriptions and unchanged destinations.
Observation cards reuse the journal card class and stack through its existing
narrow-screen rule. The underlying journal card typography, border, padding,
arrows and Continue route remain unchanged.

Bird markers and the legend use deeper blue `#376b9b`; journal green `#788e7b` is
retained. Day markers are 6px with a 5px gap, rather than 4px with a 3px gap.
Accessible date descriptions, native buttons, focus styling and mixed-day behavior
are preserved.

The isolated rendered browser fixture compared both languages at 1100px and 420px:
matching card typography/borders/padding, two-column versus stacked layout, no
horizontal overflow, 6px markers/5px separation at actual 300px calendar width,
consistent legend colors, and existing keyboard navigation/chooser cancellation.
Screenshots were inspected at those sizes. These checks do not replace macOS DEV
visual review. Retry the same profile with the command above; compare both card
rows and journal-only/bird-only/mixed calendar days in both languages and window
widths. No storage, schema, archive or platform-gate changes were made.

Revision checks: **117 focused presentation/navigation tests passed**, including
the new four-card/bilingual grouping assertion; **1 isolated rendered browser
test passed** across both languages and widths. Svelte reported **0 errors and
0 warnings**; `git diff --check` passed. No backend suite was repeated for these
presentation-only changes. All changes remain uncommitted.


## Approved Home and sidebar direction — 20 November 2026

Supersedes both rejected Home variants above. The owner-provided external sketch
`mno-home-sidebar.html` (9 November visualization) was reviewed for hierarchy and
navigation only. No sample content, placeholder screen or sketch-specific styling
was copied into production. This is an approved direction, not manual acceptance
of this implementation.

- Home: original two main cards, plus a smaller integrated **Registrera en
  observation / Record an observation** compartment. Sidebar stays closed by
  default; its existing controls now include Nature diary and Observations.
- Continue retains `/journal?latest=true` and opens the combined diary without
  opening the sidebar. Date groups are newest first. Journal entries keep their
  original same-day order, followed by visits sorted by start time, untimed last,
  then stable list ID. Each observation identity appears once, regardless of links.
- Observation-only dates are real date groups and latest/date targets. A linked
  visit retains its own date. Journal entries use compact visit references in the
  combined view; their existing text/photo components and shared gallery loading
  remain unchanged. Saved-entry detail keeps its previews and Add observation.
- Standalone visit items open the existing observation route and return to
  Observations; compact journal references open through the originating entry.
  No existing detail/edit/save/cancel route or leave-page guard was replaced.
- Initial loading waits for both guarded snapshots and checks their generations.
  Failure is shown rather than silently omitting observations. Change events reload
  content in place; superseded/disposed requests cannot replace it.
- The existing calendar/day panel remains the only calendar date selector and
  contents panel. Green/blue type markers and accessible descriptions are retained.

### Current same-profile DEV checklist (pending)

Quit the prior DEV instance and reuse the exact profile; do not reset anything:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

1. Check the two main Home cards and smaller observation compartment in Swedish
   and English, wide/narrow. Sidebar must start closed. Record an observation
   opens the existing bird form; Cancel returns through its existing route.
2. Use Continue with the sidebar closed. Confirm journal text and inline photos
   still read normally, alongside one chronological item per observation. Test
   a latest date containing only a standalone observation.
3. Link one synthetic visit to two entries on other dates. Its own diary item
   stays on the visit date once. Journal entries show compact references; opening
   one returns to its originating saved entry. Saved-entry previews remain intact.
4. Open the sidebar: use search, Nature diary, Observations and journal-only,
   observation-only and mixed dates. The existing day panel remains a compact
   contents list with the two content types separately identified.
5. Change a disposable visit's date or delete it; check diary and markers after
   returning/reloading and restarting the same profile. Check keyboard focus and
   unsaved journal protection. Do not touch production data or archives.

Validation: complete frontend suite **679 passed, 0 failed, 0 skipped**; Svelte
**0 errors, 0 warnings**; production frontend build passed. Isolated rendered
browser check **1 passed**, including Home geometry in both languages at 1100/420px
and existing calendar/chooser keyboard behavior. Existing journal rendering tests
(including inline photos and safeguards) passed; six new combined-diary checks
cover grouping/uniqueness, observation-only/latest dates, delayed and failed reads,
generation mismatch, refresh/disposal and compact references. CI-pinned Rust
**1.99.1** all-targets compilation and formatting passed. No backend production code
changed, so the already passing Rust persistence/archive tests were not repeated.
This is automated/local verification, not native DEV or packaged acceptance.

No persistence/schema/archive change, new feature module, Windows/Linux activation
or UX-005 work. Changes remain local and uncommitted; profiles/archives untouched.


## Focused proportions and alignment revision — 20 November 2026

The owner finds the direction promising; visual acceptance remains pending.
The Home observation compartment now matches the left main card's width and
left edge on wide windows, leaving the right-hand space empty. It fills the
stacked content width on narrow windows. Continue now says “Återvänd till dina
dagar i naturen.” / “Return to your days in nature.”

Observation type text in the combined diary uses restrained dark blue-grey
`#4d6472`; tiny calendar bird markers retain distinguishable blue `#376b9b`.
Date buttons reserve separate number/marker rows, including on empty dates,
with a 4px vertical gap and 5px inner clearance. Mixed markers retain their
6px size and 5px horizontal gap. Selection/focus behavior and date descriptions
are unchanged. Sidebar destinations are quiet navigation rows with `aria-current`
location indication, stronger text and a subtle edge marker for the current
destination; they retain the same routes and close behavior.

Validation: **154 focused UI/navigation tests passed**, **1 isolated rendered
browser check passed** across Swedish/English at 1100/420px. Geometry assertions
check compartment alignment, actual-size centered calendar marker rows, no number
collision and consistent clearance. Rendered sidebar checks cover both languages
and current destinations on diary/observation routes and their child routes.
Svelte: **0 errors, 0 warnings**. `git diff --check` passed. No backend or
persistence changes; all changes remain uncommitted.


## Accepted calendar and label/sidebar refinement — 20 November 2026

The owner explicitly accepted the calendar marker presentation visually. Its
colors, sizes and number/marker-row alignment are unchanged in this refinement.
This acceptance is limited to the calendar markers; the revised label/sidebar
presentation still awaits visual review.

Combined-diary observation type labels now use **13px**, medium **500** weight,
**0.25px** tracking and restrained blue `#345f80`. The label color passes 4.5:1
contrast against the MNO paper background. Ordinary journal-link styling is
unchanged, with no badges or additional borders.

Sidebar destination rows now follow the calendar (including its legend), while
search remains above it. Both destinations share base typography, weight, padding
and hit areas. Current location is indicated only by the subtle background/side
line and retained `aria-current`; active text no longer changes weight or color.
Keyboard focus and hidden-by-default behavior are preserved.

**155 focused UI/navigation tests passed**, covering both current destinations and
child routes in Swedish/English, responsive navigation, unchanged journal reading,
contrast, row ordering and equal active/base typography. Svelte: **0 errors and
0 warnings**. `git diff --check` passed. No behavior, persistence or scope change.
All work remains local and uncommitted.


## Owner visual acceptance and final quiet-link adjustment — 20 November 2026

The owner accepted the navigation/calendar presentation visually, superseding
the pending visual-review notes above. As the final adjustment, the persistent
active-destination background and side line were removed from Nature diary and
Observations. Both links retain identical quiet styling, `aria-current`, normal
hover feedback and visible keyboard focus. Opening the sidebar does not autofocus
a destination. Accepted calendar colors, size and alignment remain unchanged.

This records manual visual acceptance separately from automated checks; it is
not packaged-release acceptance. Changes remain local and uncommitted.


## Final combined review and local checkpoint — 20 November 2026

Reviewed the complete branch against
`da15b1f78acb434de71e953cc0c754f439814e98`, including all uncommitted files.
No concrete correctness or data-preservation blocker was found.

- Home destinations retain the editor and Continue routes. Continue now considers
  observation-only dates without requiring an open sidebar.
- Date grouping preserves journal order and independently dated, deduplicated
  visits. Multiple journal links do not multiply visit items.
- Compact references retain their originating entry route; standalone items keep
  the existing observation route/return behavior. Journal rich-text, inline-photo
  rendering and shared-gallery loading are unchanged. Compact references intentionally
  replace full visit previews/thumbnails only in the combined reader; saved-entry
  detail retains its existing previews and thumbnails.
- Existing create/edit/date-change/delete signals reload markers and diary data;
  superseded and disposed reads are rejected. Mixed-day selection uses the original
  day panel. Existing editor leave-page and save guards were not changed.
- Sidebar starts closed, preserves keyboard access and `aria-current`, has no
  destination autofocus and no persistent active-link decoration. Search remains
  above the calendar; quiet destination rows follow its legend.
- Availability checks precede observation reads, preserving Windows/Linux gates.
  No Rust production, schema, archive, dependency or original-photo handling change.

Final checks in this review:

- Complete frontend suite: **681 passed, 0 failed, 0 skipped**.
- Svelte: **0 errors, 0 warnings**.
- Production frontend build: passed.
- Changed-document file/heading links and `git diff --check`: passed.

Reused passing evidence: isolated rendered browser verification across both
languages and 1100/420px, including actual calendar geometry and keyboard/chooser
behavior; CI-pinned Rust **1.99.1** all-targets compilation and formatting; the
three existing Rust overview/link/archive regressions. No backend change or
review uncertainty justified repeating those checks.

UX-005 remains a separate known non-blocking issue and was not investigated.
Packaged acceptance remains outstanding. Profiles, archives and unrelated files
were not accessed or changed. This review authorizes only a local checkpoint:
no push, PR, CI dispatch, merge or publication.
