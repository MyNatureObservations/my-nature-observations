# Photographs Stage 2B — design and contract review

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

Reviewed 2026-10-31 against merged `main` at `d72bb9ea61bc7a11684970b53a29c9dcbed33ec1` (PR #25). Its tracked tree matches the inspected checkout at `b5c4dfd75d98b9f726c08c5eefacd6d8b8db6218`.

**Recommendation:** implement Stage 2B through shared presentation rules and contextual editor controls. Retain document v2 and existing storage contracts. Floats are the smallest suitable layout mechanism, subject to browser checks for caret behavior and containment. This review implements nothing.

## Current implementation

Paths below are relative to the repository root.

| Location | Current behavior and implication |
| --- | --- |
| `app/src/lib/journal-image.ts` | Selectable, atomic block `journalImage`; attributes already include identifiers, placement, size, caption and alt. Defaults are `row`/`large`. No schema extension is needed. |
| `app/src/lib/journal-image-view.ts` | Editor-owned figure loads saved preview bytes, displays caption, and provides persistent text controls. Caption/alt inputs currently dispatch a document change on every input. Mutations resolve `getPos()`; removal uses `closeHistory`. |
| `app/src/lib/components/JournalPhotograph.svelte` | Reading-mode figure uses saved previews and separate caption/alt. No editing controls; obsolete asynchronous loads are ignored. |
| `app/src/lib/journal-photographs.css` | Shared figure/image/caption styling, including rounded corners and wrapping captions. Every figure currently clears floats; placement and size have no visual effect. |
| `app/src/lib/components/JournalEditor.svelte` | TipTap, history, pending insertion mapping, serialized autosave and asset retention for undo/redo. Loaded v2 remains v2 even after its last photograph is removed. |
| `app/src/lib/components/JournalContent.svelte` and `JournalReading.svelte` | Reader renders document blocks directly. Article contains floats, but the prose root is not itself a formatting boundary. Reader/editor heading margins differ. |
| `app/src/lib/journal-photographs.ts` and `journal-reading.ts` | Shared photograph types, insertion transactions, preview loading/session checks and v1/v2 document validation. Images are top-level blocks; insertion already handles paragraphs, headings and lists. |
| `app/src-tauri/src/photographs.rs` | Validates allowed attributes, UUID v4 identifiers, unique node IDs, placement/size enums, text limits and per-entry asset references. Assets and document save together; unused assets remain stored. |

The existing NodeView has no editable child content. Preserve that ownership boundary; caption editing should not introduce a nested TipTap document or a second caption source.

## Recommended data contract

Keep the existing node unchanged:

```ts
{
  type: "journalImage",
  attrs: {
    nodeId: string, assetId: string,
    placement: "left" | "right" | "row",
    size: "small" | "large",
    caption?: string, alt?: string
  }
}
```

Identifiers remain stable through property changes. Caption belongs to the node; alt is independent and never inferred from caption. Continue keeping original paths and preview bytes outside document JSON. Display changes must not regenerate previews or modify assets.

Existing Stage 2A `row`/`large` documents open without rewriting. Preserve optional text fields when untouched. Backend text limits are 16,384 UTF-8 bytes per field; the current editor's 4,000-character limit should remain, with byte-length validation before applying a draft so frontend acceptance cannot exceed the backend contract.

No changes are needed to document v2, SQLite schema 4, archive format 5 or preservation proof 5. Existing validation, atomic save, asset ownership and portability paths continue to cover these attributes. Do not add grouping nodes, a derived layout index or migrations.

## Layout architecture

Use the existing figure in each renderer with identical presentation attributes (`data-placement`, `data-size`) and shared CSS. Keep separate editor/reader implementations; a new rendering framework is unnecessary.

Float the **whole figure**, including caption, left or right. Own-row figures do not float. Ordinary paragraphs may continue wrapping until the figure, including its caption, ends. Every new photograph clears earlier floats, including an opposite-side photograph: consecutive photographs therefore remain sequential rather than forming a gallery. Do not move nodes or insert clearing paragraphs into the saved document.

Headings, lists, blockquotes, code blocks and horizontal rules should clear preceding photographs. This gives these structural blocks their full prose width and avoids list markers squeezed beside a caption. A photograph after a heading/list starts after that block; subsequent paragraphs can wrap. This is a proposed presentation rule, not a document transformation.

Make the actual prose roots (`.tiptap`, `.journal-text`) float-containing formatting contexts with `display: flow-root`. Do not put ordinary paragraphs in individual formatting contexts or turn the prose root into flex/grid: that would defeat wrapping. MDN documents [float behavior](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/float) and [clear/formatting-context containment](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/clear).

Use content-width responsiveness, not only window width. The detail column is capped at 760px, and the application currently has a 900px minimum window width. A reasonable starting point for visual review is:

| Presentation | Proposed maximum width |
| --- | --- |
| Small, any placement | 240px |
| Large, own row | 640px |
| Large, side | Smaller of 400px and 45% of prose width |

Cap every width by available space **and natural preview width**. Supply the loaded asset's numeric width to both figure roots so the caption follows the actual image width and small previews are never enlarged. Use the same bounded placeholder width while loading. Retain aspect ratio, current 6px corners, approximately 20px side spacing, 24px vertical spacing and 8px caption gap as initial values.

Below approximately 640px of prose width, present side figures as own-row figures while retaining saved placement. Default to safe own-row layout if container queries are unavailable. [Container queries](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Containment/Container_queries) can use each prose root as the query container. These dimensions are CSS tuning values, not persisted contracts.

Align the relevant editor/reader prose metrics, especially heading margins, line height and whitespace handling. Shared figure CSS alone cannot guarantee equal wrapping. Account for the editor's existing padding when comparing usable content widths. Focus outlines and contextual controls must not change figure dimensions.

## Interaction architecture

Replace persistent inline forms with one editor-owned menu/dialog host, using existing Svelte and native DOM facilities. Extend the NodeView callback options to request this host; avoid separate stateful menus on every image. Place overlays outside the prose layout so opening them cannot change wrapping or be clipped by containment.

- Right-click on a photograph/caption selects that node and opens its menu. Preserve native context menus in prose and text fields.
- Provide a quiet, labelled options button on photograph focus/selection. Enter/Space opens the same menu; support Shift+F10 and the Context Menu key on the focused photograph.
- Use a flat menu: caption, alternative text, placement radio choices, size radio choices, and removal. Provide checked state, arrow/Home/End navigation, Escape, outside dismissal and sensible focus return. Follow the [WAI-ARIA menu-button pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/).
- Caption and alt each open a small labelled dialog with a local draft, Apply and Cancel. Escape cancels; opening, typing and cancelling do not mutate the document. Follow the [dialog focus pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/). Apply validates and changes only the chosen field, preserving current unrelated attributes.
- Keep the target node ID and a live position resolver. Before every action, resolve its current position and verify identity, editability and current session. Never retain an asynchronous numeric document offset. Close stale UI when the node disappears, the editor is destroyed or the session becomes read-only.
- Do not silently overwrite a draft if its underlying field changed meanwhile. Cancel/reload it with an explanation. Integrate pending drafts with existing close/navigation handling so users explicitly apply or discard them before leaving editing.

One deliberate property choice must be one undoable document transaction; choosing the existing value does nothing. Isolate property changes from adjacent typing with tested history boundaries. `closeHistory` resets grouping before a transaction, so verify that subsequent typing is separated too. Menu selection/focus transactions must not enter document history. Keep existing insertion/removal history and autosave behavior.

Reading mode has no options button, menu, dialog or editing-only tab stop, including when an existing editor changes to read-only. Keep preview loading guards and missing-original behavior unchanged.

## Risks and acceptance criteria

| Case / risk | Required result |
| --- | --- |
| Consecutive or alternating-side photographs | Clear predecessors; preserve document order; no side-by-side gallery or caption overlap. |
| Between paragraphs; after heading/list | Prose wraps beside a figure; structural blocks clear it. Caret navigation and selection remain usable around the atomic block. |
| Portrait, landscape, tiny preview, long caption | No cropping/upscaling; caption stays under its image, wraps long words and participates in float height. |
| Narrow content and resizing | Own-row fallback without document changes, autosave or horizontal overflow; no float leaks into another entry or metadata. |
| Repeated placement/size changes | Stable IDs/assets; no-op choices create no save/history entry; undo/redo reverses each deliberate choice without consuming adjacent typing. |
| Draft caption/alt | Cancel has no effect; Apply is one change; separate fields; stale targets cannot mutate another node. |
| Keyboard-only editing | Reach options, choose every action, edit/cancel/apply and remove; predictable focus after dismissal/removal. Localized labels and checked states. |
| Edit/read switch and older v2 entry | Equivalent prose/photo geometry; no editing controls in reading mode; existing row/large content opens unchanged. |
| Persistence and portability | Reload preserves properties, text and preview bytes; missing originals remain harmless; existing backup/export/import/Restore tests retain image data and unused assets. |

The principal uncertainty is native contenteditable caret/selection behavior around floated atomic blocks. It requires actual browser testing before accepting the layout. Do not add speculative document restructuring to address an unobserved problem.

## Focused verification plan

1. Extend `app/tests/journal-image.test.mjs` and `journal-photographs.test.mjs` for all attribute choices, repeated changes, no-op actions, history boundaries, defaults and v2 save/reopen. Exercise real ProseMirror transactions.
2. Extend `journal-image-view.test.mjs` for menu targeting after document edits, keyboard entry, local drafts, cancel/apply, readonly transitions, removed targets and late loads. Its DOM double cannot establish layout correctness.
3. Extend `journal-reading.test.mjs` for shared presentation attributes, captions, alt and absence of editing UI. SSR output also cannot establish layout correctness.
4. Use a small deterministic browser fixture in the Tauri WebKit runtime to compare editor/reader figure and text bounds at wide and narrow content widths. Cover orientations, long captions, consecutive figures, paragraphs, headings and lists; check containment, caption width, aspect ratio and no upscaling. Manually verify caret movement, selection, keyboard menus and both locales. Test narrow content without changing the shipping minimum window size.
5. Retain Rust photograph validation and portability coverage in `app/src-tauri/src/photographs/tests.rs`; add a property-rich round-trip fixture only where existing coverage lacks it. Verify layout edits preserve asset bytes and unused assets. No backend redesign is indicated.
6. During implementation, run applicable frontend, Rust and formatting checks described in `docs/development/testing.md`.

No genuinely unresolved product decision blocks implementation planning. The dimensions and structural clearing rules above are concrete defaults for visual acceptance. Any demonstrated browser limitation should be reported before changing an approved placement behavior.

## Review scope and outcome

Static code/contract review only. No production files, schemas, documents or existing reports were changed. No fresh automated tests, CI, commits or PR were created. Historical test results are not presented as new verification. Implementation awaits approval; unrelated Restore/Recovery and overview UX are excluded.
