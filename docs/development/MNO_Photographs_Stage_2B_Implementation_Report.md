# Photographs Stage 2B — implementation report

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

Implemented on `feature/photographs-stage2b`, based on merged main `d72bb9e`. Baseline: [approved design review](MNO_Photographs_Stage_2B_Design_Review.md).

Latest refinement: Large is offered only for centered images. Stored legacy Large side images use Medium presentation without load-time rewriting. The final section below supersedes earlier side-Large measurements/menu descriptions.

## Changes and decisions

- Shared prose/figure CSS implements left/right wrapping and own-row layout. Small caps at 240px; large own-row at 640px; large side at min(400px, 45% of prose width). Natural preview width and available width always cap the figure. Captions float with their photograph; no crop or upscale.
- Container queries activate side placement at 640px of prose width. Below that, figures use their own row without modifying document attributes. Each photograph clears earlier floats; structural blocks clear photographs. Both prose roots contain floats and share typography/spacing.
- One native DOM overlay host per TipTap editor replaces persistent forms. Right-click, options button, Shift+F10 and Context Menu key open the same menu. Radio choices expose checked state; arrows/Home/End, Enter/Space and Escape work. Native modal dialogs provide caption/alt drafts, Apply/Cancel and focus containment.
- Drafts do not enter the document or autosave until Apply. Active drafts block navigation, finishing and window close until explicitly resolved. Transactions verify current node identity, live position, editability and session; changed fields produce an explanation rather than being overwritten. Text remains limited to 4,000 UTF-16 units and 16,384 UTF-8 bytes.
- Property actions have history boundaries before and after; no-op choices dispatch no document change. Figure arrow keys return focus/caret to adjacent prose after contextual editing. This small focus correction was identified during native checking.
- Reading mode has no editing controls or editing tab stops. Original-file access, asynchronous preview guards, unused asset retention and all portability paths remain unchanged.

No document, database, archive or proof version changed: document v2, SQLite 4, archive 5, preservation proof 5. No migrations or production Rust changes. The host uses native DOM to fit the existing NodeView; adding a Svelte overlay component was unnecessary.

## Relevant files

| Files | Purpose |
| --- | --- |
| `app/src/lib/journal-photographs.css` | Shared geometry, responsive floats and quiet overlay styling. |
| `app/src/lib/journal-image.ts`, `journal-image-view.ts`, `photograph-controls.ts` | Editor lifecycle, keyboard entry, figure rendering, contextual actions and drafts. |
| `app/src/lib/components/JournalPhotograph.svelte`, `JournalContent.svelte`, `JournalEditor.svelte` | Reader attributes/natural width, shared prose styles and draft navigation guards. |
| `app/messages/en.json`, `sv.json` | English and Swedish controls, choices and validation messages. |
| `app/tests/journal-image*.test.mjs`, `journal-photographs.test.mjs`, `journal-reading.test.mjs`, `photo-test-modules.mjs` | Schema/defaults, real transactions/history, NodeView/control events, autosave/reopen, stale targets and reader output. |
| `app/tests/map-panel.test.mjs` | Existing navigation-guard harness now supplies the photograph draft flag. No map behavior changed. |
| `app/tests/browser/` | Standalone real-component browser fixture, geometry assertions and isolated Tauri runner. Excluded from the production entry/build. |
| `app/src-tauri/src/photographs/tests.rs` | All six placement/size combinations saved/reloaded without resending or modifying asset data, including missing-original state. |

## Local verification

Environment: macOS 27.2, Node 26.13.0, npm 11.20.0, Rust/Cargo 1.99.1. CI retains its existing Node 26.12.1 baseline.

| Check | Result |
| --- | --- |
| Translation compilation | Passed. |
| `npm run check` | 0 errors, 0 warnings. |
| `npm test` | 382 passed; no failures/skips. |
| `npm run build` | Passed, static production build. |
| `cargo test --locked --lib` | 900 passed, 2 existing ignored tests. Includes photograph backup/export/import/Restore/Recovery and missing-original coverage. |
| Selected integration targets from `testing.md` | 18 passed: database crash 3, database safety 1, descriptor owner 10, Recovery handoff 4. |
| `cargo fmt --all -- --check`, `git diff --check`, fixture shell syntax | Passed. |

An initial restricted Rust run could not bind local sockets for two HTTP fixture tests. The full rerun with local socket access passed. The historical managed-target probe remains explicitly excluded as documented in [testing.md](testing.md); unrestricted `cargo test` is not claimed.

### Native Tauri/WebKit evidence

Ran an isolated Tauri host containing the actual TipTap extension/NodeView and actual Svelte reader, shared stylesheet and generated synthetic PNG previews. No journal database, original file or existing test profile was accessed. The committed runner was also built and exercised. Runtime reported AppleWebKit/605.1.15.

All geometry assertions passed in English and Swedish: 11 figures at each width, matching editor/reader position and dimensions (within 1px), aspect ratio, no upscale, containment, structural/consecutive clearing, paragraph wrapping and read-only controls. Resizing/read-only switching left JSON unchanged.

| Prose width | Small figure | Large portrait side/fallback | Behavior |
| --- | --- | --- | --- |
| 760px | 240px | 342px | Side float |
| 640px | 240px | 288px | Side float |
| 639px | 240px | 600px | Own row, capped by natural width |
| 360px | 240px | 360px | Own row, capped by content width |

Fixtures also include a 900px-wide landscape for the 640px row cap, an 80px-wide preview, long unbroken captions, headings, a list and consecutive alternating-side figures.

Native interactions verified: right-click menu, selected/checked choices, Shift+F10 opening, keyboard dialog entry, Escape cancelling a typed caption, Apply committing it, focus return, arrow-to-prose typing and undo; Swedish alternative-text dialog and labels. No blocking WebKit/TipTap limitation was found.

## Exact review instructions

For a reproducible synthetic native check, from the repository root:

```sh
cd app
npm run dev -- --config tests/browser/vite.config.mjs
```

In another terminal, from the same repository root:

```sh
app/tests/browser/run-webkit.sh
```

The runner requires macOS/Xcode tools and already cached Cargo dependencies (`--offline`). It creates a fresh `/private/tmp/mno-stage2b-webkit-*` directory and opens a distinct Tauri application. The directory printed by the runner contains `results.json`. No MNO storage backend is included. Quit that test app and stop Vite after reviewing.

1. Confirm `failures: []`; choose Swedish and confirm again. Choose widths 760, 640, 639 and 360 to inspect portrait/landscape wrapping and long captions. Scroll through consecutive photographs, heading/list boundaries and the separate reader.
2. Focus a photograph with Tab, then Shift+F10 (or the Context Menu key). Use arrows/Home/End and Enter/Space. Confirm checked placement/size choices; use Escape to return focus. The options button also opens the menu.
3. Open caption, type a draft and press Escape; reopen and confirm the old caption. Apply a different caption, edit alt separately, and verify the caption stays independent. Use figure arrow keys to resume prose typing; undo/redo property changes and typing independently.
4. In an isolated MNO test journal, insert portrait/landscape JPEG/PNG files, change all placements/sizes, finish editing, reopen and restart. Compare reader geometry. Verify controls disappear outside editing. Attempt navigation/close with an open draft: resolve it through Apply/Cancel before leaving.
5. In that test journal, remove a photograph and undo; back up/export and import/Restore using existing supported workflows. After removing an external synthetic original, saved previews must remain available. Automated Rust tests cover these storage paths; a new end-to-end native backup/Restore session was not performed for this presentation change.

## Limits and delivery

Native verification covers this macOS WebKit runtime; Windows/Linux browser behavior and a full VoiceOver audit are not claimed. Container-query fallback is own-row presentation. Saved placement remains unchanged.

No Recovery/Restore UX, overview, original-image or cleanup work is included. The two earlier untracked investigation reports are preserved outside this commit. PR creation triggers the existing pull-request CI workflow; stop after starting it, without polling or merging.

## PR #26 manual-test UX corrections

This follow-up keeps the existing branch/PR and all stored contracts. It supersedes the visible “own row” terminology above with **Centered / Centrerad**; the persisted placement remains `row`.

### Findings and corrections

1. **Centering:** figures previously used `margin: 24px 0`, so natural width/orientation changed their apparent alignment. The shared `row` rule now uses automatic horizontal margins and centered caption text. Figures still clear floats, remain bounded by their preview's natural dimensions, and allow no prose beside them. Existing `row` documents acquire the presentation without JSON rewrites.
2. **Insertion label:** `photo_insert` now reads **Insert image / Infoga bild**. The toolbar's visible text also supplies its accessible name, so there is no conflicting accessible label. Other photograph terminology and internal identifiers are unchanged.
3. **Writing after insertion:** reproduced in the original native fixture before production changes. `insertPhotograph` already creates an empty paragraph after the atomic image and puts a `TextSelection` there. Wrapping works in that paragraph. The actual failure was contextual focus restoration: choosing placement closed the menu by focusing the figure, whose focus handler reselected the atom. Native typing could replace the image instead of writing in its following paragraph. The earlier checks exercised pre-existing prose/arrow navigation, missing this sequence.

   After a placement or size choice, if the immediately following node is an empty paragraph, the property transaction now selects that paragraph and returns focus to the editor. It reuses the existing paragraph; no placeholder text, grouping, extra nodes or schema changes. With existing nonempty prose, figure focus restoration remains unchanged. An active-value choice can move selection but changes no document/history entry. Property and subsequent typing remain separate undo actions.
4. **Empty paragraph equivalence:** expanded native cases exposed a 9.5px vertical offset after an empty paragraph: the reader's `1em` minimum was 19px, while the editor's trailing `<br>` occupied a 28.5px line. The shared minimum is now `1.5em`, matching the existing line height. This keeps editor/reader geometry equal for newly inserted images and empty writing paragraphs.

Production changes are limited to `journal-photographs.css`, `photograph-controls.ts`, the default row label in `journal-image.ts`, and the two translation files. `insertPhotograph`, heading/list insertion, identifiers, assets, autosave, navigation guards and storage formats are unchanged.

### Follow-up verification

- Translation compilation passed; type/Svelte check: **0 errors, 0 warnings**; static build passed.
- Frontend: **391 passed**, including new caption/placement labels, empty-paragraph menu focus, active-value behavior, typing, independent undo/redo, fresh insertion into empty/partial entries, autosave and JSON save/reopen. Existing heading/list, stale-target and unsaved-change coverage still passes.
- Focused Rust photograph suite: **10 passed, 1 existing ignored**, covering persistence, missing originals, unused assets, backup/export/import and Restore/Recovery. Rust formatting passed. No Rust source or storage contract changed in this follow-up; the prior full library/integration results above are historical, not new reruns.
- Actual Tauri/WebKit: **17 figures × 4 prose widths** (760/640/639/360) in each locale, plus **8 fresh-writing scenarios per locale** (empty/partial × left/right × small/large), all with `failures: []`. Assertions include centered portrait/landscape/tiny figures and captions, no centered wrapping, natural limits, unchanged JSON, empty paragraph geometry, text selection/focus, continued wrapping below the figure, undo/redo and reader round-trip.
- Native keyboard input separately verified English landscape/Left in an empty entry and Swedish portrait/Right after existing prose. In both cases, choosing placement allowed immediate typing without placeholder prose or an extra arrow key; the image remained and prose continued below it. Native typing undo/redo preserved the photograph.
- Whitespace and fixture shell syntax checks passed. No migration or version changes; macOS/WebKit-only native coverage remains the limitation.

### Exact manual retest

Use an isolated MNO test journal:

1. In English, confirm **Insert image**; in Swedish, confirm **Infoga bild**. Insert portrait and landscape images at both sizes, including a small natural-width source. Choose **Centered / Centrerad**: each image and caption must center, retain proportions and natural limits, and leave subsequent prose below it. Reopen an older saved `row` entry and confirm the same behavior.
2. Start an empty entry. Insert an image, choose **Left / Vänster**, and type immediately. The caret should be in the empty paragraph beside it; no placeholder or extra paragraph is needed. Add enough prose to continue below the caption. Repeat with **Right / Höger**, both sizes, and an entry already containing a paragraph, heading or list. Existing heading/list insertion must remain intact.
3. Undo typing, then undo placement: the image must remain. Redo both; verify text and placement. Save/finish and reopen; compare editor and reader. Resize to narrow content: side images must use their own row while retaining saved placement; centered images must remain centered.
4. For the database-free native fixture, use the existing two-terminal commands above. **Run geometry checks** resets its document; confirm `failures: []` in both locales. **Insert into empty entry** and **Insert after existing prose** provide fresh landscape/portrait cases. Choose placement using the actual menu, type using the keyboard, and use **Inspect writing position** to inspect selection and refresh the reader.

Commit/push this follow-up to PR #26, which triggers its existing CI workflow. Do not poll, wait for CI or merge.

## Medium / Mellan size

Added the explicit `medium` size to the existing v2 photograph contract, frontend type/reader validation, Rust validation and contextual size menu. English **Medium** and Swedish **Mellan** appear between Small and Large, using the existing radio state, keyboard navigation, undo and autosave paths. Insertion defaults, caption/alt editing and focus behavior are unchanged.

Shared CSS caps Medium at 360px, including centered placement. At prose widths of at least 640px, side placement uses `min(360px, 45% of prose width, natural preview width)`. Below that threshold the existing own-row fallback applies. Every width remains bounded by available space and natural dimensions, with unchanged aspect ratio and no crop/upscale. Small stays 240px; Large stays 640px centered and min(400px, 45%) beside prose.

### Version-contract assessment

Retained document **2**, SQLite schema **4**, archive **5** and proof **5** after inspecting `photographs::validate_document`, `journal-reading.ts`, `database::initialize_database`, `storage_content::verified_snapshot_v5`, and format-5 backup/export/import/Restore paths:

- This is an additive, explicit enum extension within the current development v2 implementation. Required attributes, node structure, identifiers, ownership, text limits, asset encoding and table inventory do not change. Unknown sizes still fail validation; no permissive fallback was introduced.
- All previously valid Small/Large documents remain valid. Defaults stay `row`/`large`; existing content is not rewritten. The v1 text grammar and its frozen validator remain unchanged.
- Schema 4 stores size inside existing `content_json` TEXT. Proof 5 hashes the exact stored JSON and asset values; no hashing domain, encoding or inventory changes. Archive 5 carries those same values and assets. Tests now verify exact JSON/proof/preview preservation with all nine size/placement combinations.
- **This is backward acceptance by the new build, not forward compatibility with older binaries.** The previous strict Rust/frontend allowlists reject `medium`. An older build's startup proof validation and archive preparation can therefore refuse a Medium-containing journal/archive. Those paths fail closed rather than interpreting the value as Small/Large or rewriting it. Use the updated build for Medium data. No older-build downgrade support or claim of universal v2 interoperability is added.

No version bump is technically necessary for the requested current development contract: there is no new storage shape, encoding or frozen v2 schema fingerprint to migrate. The older-reader limitation above is explicit; this decision does not establish a policy of silently widening future contracts.

### Verification and measurements

- Translation compilation, type/Svelte check (**0 errors, 0 warnings**), static build, Rust formatting and whitespace checks passed.
- **395 frontend tests passed.** Coverage includes all three sizes × three placements, preserved old/default attributes, explicit unknown-size rejection, Medium checked state and keyboard traversal, no-op choice, undo/redo, stable IDs, autosave/reopen without resending assets, and editor/reader presentation attributes.
- **900 Rust library tests passed, 2 existing ignored; 18 selected integration tests passed.** Photograph fixtures now carry all nine layout combinations through exact JSON save/reopen, physical backup/Restore candidate preparation, export/import, interrupted Restore/import activation and startup Recovery. A separate unused asset and missing originals are retained. Original content proof and preview bytes match after portability round trips. The historical managed-target exclusion in `testing.md` remains unchanged.
- Actual native Tauri/WebKit fixture covers **23 figures × 5 widths** and **12 fresh-writing cases per locale**, including Medium portrait/landscape/tiny previews, centered captions, side wrapping, narrow fallback, natural limits, editor/reader geometry, unchanged JSON, typing, undo/redo and read-only transitions. No journal database or personal originals are involved. Native keyboard selection/checked state and Medium visual presentation were also inspected.

| Prose width | Medium centered | Medium side | Large side |
| --- | --- | --- | --- |
| 1000px (fixture) | 360px | 360px | 400px |
| 760px | 360px | 342px | 342px |
| 640px | 360px | 288px | 288px |
| 639px | 360px | 360px, own-row fallback | Natural/content-limited fallback |
| 360px | 360px | 360px, own-row fallback | Content-limited fallback |

The usual 760px application column makes Medium and Large side images equally wide because both respect the approved 45% prose-width constraint. Centered sizes remain distinct. An 80px-wide synthetic preview remains 80px at every size.

### Medium manual retest

1. Open a photograph's options with right-click or Shift+F10. Navigate to **Medium / Mellan** with the keyboard; select it and reopen to confirm its checked state. Verify undo/redo and that choosing it again makes no content change.
2. Test portrait and landscape images with Left, Right and Centered at all three sizes. Medium centered should cap at 360px; narrow side layouts should fall back to their own row without changing saved placement. A tiny original preview must remain tiny. Compare editing and reading modes.
3. Save/reopen a Medium entry, then round-trip a synthetic journal through backup/Restore and export/import using this updated build. Confirm Medium, captions, alt, previews and unused assets survive. Open an older Small/Large entry and confirm its properties remain unchanged.
4. In the existing database-free native fixture, select English/Swedish and run the matrix (`failures: []`). The added 1000px option demonstrates the distinct 360px/400px side caps; 760px demonstrates their shared 45% constraint. Native coverage remains macOS/WebKit; no Windows/Linux or full VoiceOver audit is claimed.

Changed areas: `journal-photographs.ts`, `journal-reading.ts`, `journal-image.ts`, `photograph-controls.ts`, `JournalEditor.svelte`, shared photograph CSS, `photographs.rs`, EN/SV messages and focused frontend/Rust/browser fixtures. No database/archive migration or unrelated UX work. Push to the existing PR #26 and stop after triggering CI; do not poll or merge.

## Final placement constraints and options click target

### Placement/size behavior and compatibility

The editor now offers Small (240px) and Medium (360px subject to 45% prose width) for Left/Right; Centered additionally offers Large (640px). Choosing Left/Right from Centered + Large changes both attributes in **one `setNodeMarkup` transaction**. History separation and the existing empty-paragraph writing focus are preserved. A stale Large menu action is rejected if the current placement has become a side placement.

Existing side + Large remains a supported **stored legacy combination**. Rust/frontend document validation still accepts it, so journal access, portability and caption/alt edits remain lossless. Shared CSS presents it with Medium limits at both wide and narrow widths, and the side menu marks Medium as the effective choice. Opening/reading a document or menu makes no content change. An explicit Medium or side-placement action stores Medium in one undoable change; changing a legacy image to Centered can retain its stored Large size. There is no load-time normalization or destructive rewrite.

Document 2, SQLite 4, archive/proof 5 remain unchanged. This is a restriction on new editor choices with a presentation fallback for existing valid data, not a new stored schema. Unknown sizes remain rejected. The previous Medium older-binary limitation still applies. No production Rust, insertion, draft-dialog, autosave or Recovery/Restore code changed.

### Click investigation and correction

Native WebKit tracing reproduced the reported failure on the actual visible button: pointer-down and pointer-up targeted `.mno-photo-options`, focus moved to the atomic figure and dispatched a selection transaction, but no `click` followed. `refresh()` unconditionally reassigned the button's `textContent` during that transaction, replacing the label DOM mid-gesture. Keeping the existing label node when its text is unchanged restored native click delivery; localization changes still update it normally.

The actual button now has `min-width/min-height: 40px`, border-box sizing and a capped width on small previews. Long text ellipsizes on tiny previews while retaining its complete accessible name. There is no enlarged invisible click overlay; the visible button itself owns the hit area. The normal English target measured approximately **141.44 × 40px**, previously **141.44 × 33.5px**. An 80px-wide preview has a **64 × 40px** button. Native center, upper-padding and lower-padding clicks delivered `click` and opened the menu on the first attempt. Escape/reopening, focus restoration and Shift+F10 continued to work. No pointer-down focus suppression or synthetic click forwarding was needed.

### Verification

- **408 frontend tests passed**; type/Svelte check had **0 errors and 0 warnings**; production build and whitespace checks passed.
- Added all nine stored combination/menu cases (seven choices for new layouts plus the two legacy side-Large cases), one-transaction conversion to both side placements, single undo/redo, IDs/caret preservation, stale Large action rejection, and stable label DOM across selection/locale updates. Existing save/reopen, autosave, draft and keyboard tests remain passing.
- Focused Rust photograph suite: **10 passed, 1 existing ignored**; formatting passed. These unchanged storage tests continue to round-trip all nine stored combinations, including legacy side-Large, unused assets, missing originals and backup/export/import/Restore/Recovery. Earlier full Rust/integration results are historical and were not rerun for these frontend-only changes.
- Actual Tauri/WebKit: **23 figures × five prose widths**, **12 writing cases**, and **46 button targets / 230 interior hit-test points per locale** passed. The matrix covers centered-only Large, legacy rendering without JSON changes, effective Medium caps, editor/reader parity, natural dimensions, 40px targets at 760/360px widths, label-node identity and no button interception immediately outside its visible box. Writing cases include Centered + Large → side + Medium, undo/redo and typing.
- Native interactions separately verified actual mouse delivery across center/padding, valid side menu choices, effective legacy Medium selection and atomic conversion undo/redo. Native verification remains limited to this macOS WebKit runtime; no full VoiceOver audit is claimed.

Changed production files: `photograph-controls.ts`, `journal-image-view.ts`, and `journal-photographs.css`. No translation strings changed: EN/SV labels and radio semantics are reused. Focused NodeView/browser tests, fixture diagnostics and this report were updated.

### Exact manual retest

1. Select a centered Large photograph. Choose Left, then repeat with Right: the size must become Medium immediately. Undo once: both Centered and Large return. Redo once: the side placement and Medium return. With an empty following paragraph, typing should still start beside the photograph.
2. Open side-image size menus in English and Swedish: only Small/Liten and Medium/Mellan should appear. Centered/Centrerad must additionally offer Large/Stor. Open an older side + Large entry: it remains readable at effective Medium width, with no save/change caused by opening it. Explicitly select Medium to store that choice, or Centered to retain Large.
3. Select an image and click the options button's label, top/bottom padding and edges. Each attempt should open the menu immediately. Repeat after Escape, after clicking prose, and on a small natural preview. Confirm Shift+F10, keyboard traversal and reading mode. Click just outside the visible button: ordinary image/prose selection must still work.
4. Use the existing native fixture commands. **Run geometry checks** resets the document and should show `failures: []` in both languages. Optional **Record options clicks** logs actual pointer/focus/transaction events into the temporary host's `results.json`; its display is hidden so tracing cannot move the target during a gesture. Stop recording and rerun the matrix to restore the geometry report.

Commit/push to existing PR #26 and stop after its CI trigger. Do not poll, wait for CI or merge.
