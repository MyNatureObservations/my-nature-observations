# Stage 2B browser fixture

From `app/`, run `npm run dev -- --config tests/browser/vite.config.mjs`.
Open `http://127.0.0.1:1432/tests/browser/stage2b.html` or run
`app/tests/browser/run-webkit.sh` from the repository root for an isolated native
Tauri/WebKit window. macOS, Xcode tools and cached Cargo dependencies are required
for the native runner. Its temporary directory contains `results.json`.

The fixture imports the production TipTap node, NodeView, Svelte reader and CSS.
Only asset/session reads are replaced with synthetic data by the standalone Vite
configuration. It never opens a journal database or an original image. Geometry
checks cover wide/narrow prose, all placements/sizes, captions, natural dimensions,
float boundaries, editor/reader parity and read-only transitions. Use the editor
for native caret, selection, right-click and keyboard checks in both languages.

This directory is not imported by the production application. The normal Node
test suite covers transactions and events; it does not claim browser geometry.

PR #26 UX regressions: the matrix now also checks centered portrait/landscape
figures at both sizes (including tiny natural dimensions), caption alignment,
no centered wrapping, unchanged JSON and empty-paragraph geometry. Eight fresh
insertion scenarios exercise placement menu actions, the resulting text caret and
focus, typing/undo/redo, wrapping below the figure and reader round-trip.

For native typing, choose **Insert into empty entry** (landscape) or **Insert after
existing prose** (portrait), open the photograph menu and choose Left/Right. Type
immediately without adding placeholder text or pressing an extra arrow key.
**Inspect writing position** displays selection/DOM diagnostics and refreshes the
reader with the current document. **Run geometry checks** resets the synthetic
document and runs the full matrix. `results.json` contains that matrix's result;
manual writing diagnostics stay in the window.

Medium coverage adds all nine placement/size combinations, 360px centered/side
caps, portrait/landscape/tiny previews, and a 1000px prose-width option to distinguish
Medium's 360px side cap from Large's 400px cap. At 760px both use the existing 45%
constraint (342px). The matrix covers 23 figures at five widths and 12 fresh-writing
cases in each locale; editor/reader geometry and unchanged JSON remain asserted.

Final refinements keep side-Large nodes as legacy fixtures: they render with Medium
limits without changing JSON. New placement actions convert Centered/Large to a
side/Medium in one undoable transaction. The matrix checks 46 button targets at
230 interior points (wide/narrow), stable label DOM, 40px minimum dimensions and
no invisible interception outside the visible control. Optional **Record options
clicks** writes real pointer/focus/transaction traces to the native host's
`results.json`; its output stays hidden to avoid moving targets during a gesture.
Stop recording and rerun geometry checks to restore the matrix report.
