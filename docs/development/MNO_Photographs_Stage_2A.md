# Photographs, Stage 2A: insertion and standalone rendering

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

Implemented on `feature/photographs-stage2a` from verified main `a7e9aef56907f3a4a779ba9ad491ed8596fb5ab9` (merged Stage 1, PR #24). The checkout was clean before branching. Project instructions and current `testing.md` were read. No synced `sources/`, personal databases, older working copies or production profiles were modified.

## User experience

The toolbar now has **Insert photograph / Infoga fotografi**. A native JPEG/PNG picker selects one file at a time. A localized processing status remains visible while Rust prepares the preview on a blocking worker rather than the UI thread. Failed preparation leaves the document unchanged and displays a localized error. No original is changed.

The current insertion point is captured before the picker opens and mapped through subsequent document transactions. Paragraphs split at the intended cursor position. Headings, lists and other enclosing blocks remain intact; the photograph is placed after the outer block. If a paragraph changes into a heading/list during preparation, the insertion follows the enclosing block. If the position is deleted, insertion stops with an explanation instead of guessing. Subsequent single-image insertions preserve document order and leave a text cursor after the image.

The existing atomic `journalImage` node is used throughout. Images are standalone blocks in both editor and reader, constrained to the content width with automatic height, no cropping and 6-pixel rounded corners. Stored `placement` and `size` are preserved, including after caption/alt editing; they do not control layout in this stage. New images start with `row`/`large`. There are no placement or sizing controls and no text wrapping.

Each editor image has a quiet **Edit caption and alternative text** action (localized in Swedish). It opens a plain caption textarea and a separate optional alternative-text input, plus Done and Remove from journal actions. Captions appear beneath the image in both views. Caption text is never substituted for alternative text. Text is rendered as text, not HTML. New field input is limited to 4000 UTF-16 units, within Stage 1's byte bound; loading existing longer metadata does not rewrite it.

The figure and controls are keyboard reachable. Focusing an editable figure selects its TipTap node; the existing undo/redo controls and deletion workflow remain available. Escape closes the caption panel and restores its trigger focus. The remove action deletes only the document node and returns focus to the editor. Its preview remains in SQLite and its original is untouched. Undo/redo keeps the same asset identity and can restore removed images.

## Persistence and asynchronous safety

- Existing v1 entries keep using the original save command until a photograph is inserted. A loaded or newly upgraded v2 entry uses Stage 1's atomic journal/assets save command. Unsupported versions and malformed documents remain protected. V2 is not automatically downgraded when its final image is removed.
- A draft entry ID is assigned before preparation. If the first text save is already in flight, insertion first waits for that save so the resource uses its returned owner ID. Subsequent text autosaves use the reserved draft ID. This avoids creating an asset for a different entry during first-save races.
- Prepared assets stay in the editor's local map through undo/redo and pending saves. Successfully saved assets are not resubmitted on every text edit. Each save captures its exact document, owner, generation and new resources; the existing save queue and retry handling remain in charge. Unused durable assets are not cleaned up.
- An editor-local operation counter, editor identity, captured generation and owner check reject obsolete preparation results after discard, destruction or entry/session replacement. Rust preparation retains the existing operation/generation guard; the frontend also checks the current backend generation before inserting. No job-management framework is added.
- Processing participates in navigation, window-close and Done guards. Choosing discard invalidates the pending result immediately. Existing unsaved-text, metadata and in-flight-write protections are preserved.
- Preview readers bind requests to entry, asset and database generation, and reject late results. NodeViews stop applying results after destruction; the Svelte reader cancels its own obsolete effects. Preview source strings are derived from JPEG/PNG bytes only in rendering, outside the TipTap JSON. No original path is used to load a saved image.
- Backend entry reads now recognize valid, correctly referenced v2 documents as editable. Manual/historical weather edit guards use that same version-aware check, avoiding a v1-only metadata lock after adding a photograph. The old v1-only save API still cannot overwrite a v2 entry.

There are **no schema, document-contract or archive-format changes** in Stage 2A. Stage 1's schema 4, document v1/v2, preservation proof 5, backup/export format 5 and activation contracts are reused. No compatibility/migration machinery or orphan collection was added.

## Changed files

| Area | Files |
|---|---|
| Editor, insertion position, resource reads, NodeView | `app/src/lib/components/JournalEditor.svelte`, `journal-image.ts`, new `journal-photographs.ts`, new `journal-image-view.ts` |
| Reader and shared styling | `JournalContent.svelte`, `JournalReading.svelte`, new `JournalPhotograph.svelte`, `journal-reading.ts`, new `journal-photographs.css`, `app/src/routes/+layout.svelte` |
| Localization | `app/messages/sv.json`, `app/messages/en.json` |
| Native picker/worker and v2 editability | `app/src-tauri/src/photographs.rs`, `journal.rs`, `lib.rs`, `weather/storage.rs`, `weather/historical/saving.rs` |
| Backend regression coverage | `app/src-tauri/src/photographs/tests.rs`, future-version fixture in `journal.rs` |
| Frontend tests and registration | New `journal-photographs.test.mjs`, `journal-image-view.test.mjs`, `photo-test-modules.mjs`, extracted reusable `journal-editor-test-harness.mjs`; existing autosave, image, reader, navigation-link and frozen-v1-schema tests; `app/package.json` |
| Documentation | This report and a Stage 2A pointer in the Stage 1 storage report |

Unqualified component/helper names in the table are under `app/src/lib/` (components under `components/`), and frontend test names under `app/tests/`. No new runtime or testing dependency was installed. The native picker follows existing Rust dialog conventions; no new frontend dialog permission is required.

## Local verification performed

| Check | Result |
|---|---|
| `npm ci` and localized message compilation | Completed using the existing lockfile and configured translation plugin |
| `npm run check` | 0 errors, 0 warnings |
| `npm test` | 374 passed, 0 failed |
| `npm run build` | Passed |
| `cargo test --locked --lib` | 900 passed, 0 failed, 2 explicit ignores |
| Required Rust integrations | `database_crash`: 3; `database_safety`: 1; `descriptor_owner`: 10; `recovery_handoff_prototype`: 4; all passed |
| `cargo fmt --all -- --check`, `git diff --check` | Passed |

The frontend tests exercise real ProseMirror transactions and history, the actual component script with the existing harness, actual NodeView event handlers with a small DOM double, and actual Svelte server rendering. They cover paragraph/heading/list insertion, mapping through edits and deleted positions, serialization, caption/alt independence, removal/undo/redo, v2 saves/reopening, first-save owner races, failed/stale preparation, generation checks, discard/navigation protection, localized reader output and preview source separation from journal JSON. They do not replace native interaction tests.

The new Rust test writes, closes and reopens a synthetic v2 journal with its original removed, verifies read editability and metadata changes, preserves the unused asset after removal, rejects future versions and verifies that the legacy text-only save cannot overwrite it. Existing backup/export/import and Recovery tests were rerun in the full suite. The previously used future-version fixture was changed from 2 to 3 because 2 is now supported; its protection assertion was retained.

The two library ignores remain the manual image measurement test and the existing explicit live-HTTPS forecast probe. The historically failing standalone managed-target probe remains excluded exactly as documented in `testing.md`; unrestricted `cargo test` is not claimed. Image-preparation algorithms and limits are unchanged; Stage 1's measurements were not rerun or presented as new Stage 2A measurements.

Local tools: macOS arm64, Node 26.13.0 (CI pins 26.12.1), npm 11.20.0 and Rust 1.99.1. All databases and file fixtures used by automated tests were synthetic and isolated.

## Manual Tauri verification still required before approval

Use an isolated synthetic profile. Native Tauri interaction and visual acceptance were **not** performed in this delivery.

1. In Swedish and English, insert JPEG and transparent PNG into the middle of a paragraph, a heading and a nested list. Verify order and that heading/list text stays intact. Insert several photographs through repeated single-file actions.
2. Use a larger source image: verify that processing is visible, writing remains responsive, the intended insertion point follows edits, and native picker cancellation or invalid input does not change the document.
3. Edit caption and alt independently with keyboard only; try multiline/plain caption text, empty fields, Tab/Shift+Tab, Escape, figure selection, removal and toolbar/keyboard undo/redo. Verify focus and screen-reader labels.
4. Autosave, finish editing, reopen and navigate through the journal reader. Verify image/caption/alt preservation, aspect ratio, rounded corners and narrow-window containment. Check existing weather/place editing on the same v2 entry.
5. Remove/rename a synthetic original outside MNO and reopen: the durable preview must still display. Removing its node must leave original and stored preview untouched; undo must restore it.
6. While processing or saving, navigate, close, continue editing or choose discard. Verify no late result enters another entry. Repeat with a database replacement/recovery transition using synthetic profiles.
7. Exercise image-only entries and multiple images through backup/Restore and open export/import in the app, confirming the already-tested storage paths through the native dialogs.

No Stage 2B work is included. The PR is for review and manual Tauri testing; do not merge or proceed to the editorial layout stage automatically. The PR triggers existing macOS CI, whose status is left to the user without waiting or polling.
