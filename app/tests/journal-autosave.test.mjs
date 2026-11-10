import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { compile, parse } from "svelte/compiler";
import { harness } from "./journal-editor-test-harness.mjs";

test("A → B → A during a save automatically persists A again", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  assert.equal(h.writes[0].contentJson, '{"text":"B"}');
  h.edit("A");
  assert.equal(h.pending(), false);
  await h.succeed(0);
  assert.equal(h.pending(), true);
  assert.equal(h.canLeave(), false);
  await h.tick();
  assert.equal(h.writes.length, 2);
  assert.equal(h.writes[1].contentJson, '{"text":"A"}');
  await h.succeed(1);
  assert.equal(h.pending(), false);
  assert.equal(h.canLeave(), true);
  await h.tick();
  assert.equal(h.writes.length, 2);
  h.cleanup();
});

test("normal autosave writes once and reverting before saving writes nothing", async () => {
  const h = await harness();
  h.edit("B");
  h.edit("A");
  await h.tick();
  assert.equal(h.writes.length, 0);
  h.edit("B");
  await h.tick();
  await h.succeed(0);
  assert.equal(h.pending(), false);
  await h.tick();
  assert.equal(h.writes.length, 1);
  h.cleanup();
});

test("queued saves remain serial and skip redundant writes", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("C");
  await h.tick();
  assert.equal(h.writes.length, 1);
  await h.succeed(0);
  assert.equal(h.writes.length, 2);
  assert.equal(h.writes[1].contentJson, '{"text":"C"}');
  await h.succeed(1);
  await h.tick();
  assert.equal(h.writes.length, 2);
  assert.equal(h.pending(), false);
  h.cleanup();
});

test("retry preserves its snapshot and schedules the latest content after success", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("A");
  h.writes[0].reject(new Error("temporary failure"));
  await h.flush();
  await h.tick();
  assert.equal(h.writes[1].contentJson, '{"text":"B"}');
  await h.succeed(1);
  await h.tick();
  assert.equal(h.writes[2].contentJson, '{"text":"A"}');
  await h.succeed(2);
  assert.equal(h.pending(), false);
  h.cleanup();
});

test("discarding while a write runs prevents follow-up autosave", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("A");
  const leaving = h.run("leaveWithoutSaving()");
  await h.succeed(0);
  await leaving;
  await h.tick();
  assert.equal(h.writes.length, 1);
  h.cleanup();
});

test("destroying the editor during a save prevents follow-up writes", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("A");
  h.cleanup();
  await h.succeed(0);
  await h.tick();
  assert.equal(h.writes.length, 1);
});

async function rejectWrite(h, index) {
  h.writes[index].reject(new Error("disk unavailable"));
  await h.flush();
}

async function failAllAttempts(h, start = 0) {
  await rejectWrite(h, start);
  assert.equal(h.run("isRetrying"), true);
  assert.ok(h.retryDelays().includes(2000));
  await h.tick(2000);
  await rejectWrite(h, start + 1);
  assert.ok(h.retryDelays().includes(5000));
  await h.tick(5000);
  await rejectWrite(h, start + 2);
  assert.equal(h.run("isSaving"), false);
  assert.equal(h.run("isRetrying"), false);
  assert.equal(h.run("saveError"), true);
}

test("three failed attempts keep text, warning and leave protection without looping", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  assert.equal(h.text(), "B");
  assert.equal(h.pending(), true);
  assert.equal(h.canLeave(), false);
  assert.equal(await h.canClose(), false);
  for (let i = 0; i < 5; i++) { await h.tick(2000); await h.tick(5000); }
  assert.equal(h.writes.length, 3);
  assert.equal(h.run("saveError"), true);
  assert.equal(h.run("savedContentJson"), '{"text":"A"}');
  h.cleanup();
});

test("new edits restart autosave after final failure and success clears warning", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  h.edit("C");
  assert.equal(h.run("saveError"), true);
  assert.equal(h.writes.length, 3);
  await h.tick();
  assert.equal(h.writes[3].contentJson, '{"text":"C"}');
  assert.equal(h.run("saveError"), true);
  await h.succeed(3);
  assert.equal(h.run("saveError"), false);
  assert.equal(h.pending(), false);
  assert.equal(h.canLeave(), true);
  assert.equal(await h.canClose(), true);
  await h.tick();
  assert.equal(h.writes.length, 4);
  h.cleanup();
});

test("manual retry saves current text, cancels debounce and ignores double clicks", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  h.edit("C");
  const retry = h.run("retrySave()");
  await h.run("retrySave()");
  await h.tick();
  assert.equal(h.writes.length, 4);
  assert.equal(h.writes[3].contentJson, '{"text":"C"}');
  assert.equal(h.run("saveError"), true);
  await h.succeed(3);
  await retry;
  assert.equal(h.run("saveError"), false);
  assert.equal(h.pending(), false);
  await h.tick();
  assert.equal(h.writes.length, 4);
  h.cleanup();
});

test("another failed manual retry remains bounded and retains warning and text", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  const retry = h.run("retrySave()");
  await failAllAttempts(h, 3);
  await retry;
  assert.equal(h.text(), "B");
  assert.equal(h.pending(), true);
  assert.equal(h.run("saveError"), true);
  await h.tick(2000);
  await h.tick(5000);
  assert.equal(h.writes.length, 6);
  h.cleanup();
});

test("new autosave failure keeps warning and does not keep retrying", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  h.edit("C");
  await h.tick();
  await failAllAttempts(h, 3);
  assert.equal(h.text(), "C");
  assert.equal(h.pending(), true);
  await h.tick(2000);
  await h.tick(5000);
  assert.equal(h.writes.length, 6);
  h.cleanup();
});

test("warning remains when a successful recovery saves an older snapshot", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  const retry = h.run("retrySave()");
  h.edit("C");
  await h.succeed(3);
  await retry;
  assert.equal(h.run("saveError"), true);
  assert.equal(h.pending(), true);
  assert.equal(h.persisted(), '{"text":"B"}');
  assert.equal(h.text(), "C");
  assert.equal(h.canLeave(), false);
  await h.tick();
  assert.equal(h.writes[4].contentJson, '{"text":"C"}');
  await h.succeed(4);
  assert.equal(h.run("saveError"), false);
  assert.equal(h.pending(), false);
  h.cleanup();
});

test("discard during retry delay prevents automatic and manual recovery writes", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await rejectWrite(h, 0);
  const leaving = h.run("leaveWithoutSaving()");
  await h.run("retrySave()");
  await h.tick(2000);
  await leaving;
  await h.tick(5000);
  assert.equal(h.writes.length, 1);
  h.cleanup();
});

for (const revertDuringRetry of [false, true]) {
  test(`A → B → A with failed attempts and eventual success (revert during retry: ${revertDuringRetry})`, async () => {
    const h = await harness();
    h.edit("B");
    await h.tick();
    await rejectWrite(h, 0);
    if (revertDuringRetry) await h.tick(2000);
    h.edit("A");
    assert.equal(h.pending(), false);
    if (!revertDuringRetry) await h.tick(2000);
    await h.succeed(1);
    assert.equal(h.persisted(), '{"text":"B"}');
    assert.equal(h.pending(), true);
    await h.tick();
    assert.equal(h.writes.length, 3);
    assert.equal(h.writes[2].contentJson, '{"text":"A"}');
    await h.succeed(2);
    assert.equal(h.persisted(), JSON.stringify({ text: h.text() }));
    assert.equal(h.pending(), false);
    assert.equal(h.run("saveError"), false);
    await h.tick(2000);
    await h.tick(5000);
    assert.equal(h.writes.length, 3);
    h.cleanup();
  });

  test(`A → B → A with all attempts failing leaves no stale error (revert during retry: ${revertDuringRetry})`, async () => {
    const h = await harness();
    h.edit("B");
    await h.tick();
    await rejectWrite(h, 0);
    if (revertDuringRetry) await h.tick(2000);
    h.edit("A");
    if (!revertDuringRetry) await h.tick(2000);
    await rejectWrite(h, 1);
    await h.tick(5000);
    await rejectWrite(h, 2);
    assert.equal(h.persisted(), JSON.stringify({ text: h.text() }));
    assert.equal(h.pending(), false);
    assert.equal(h.run("saveError"), false);
    assert.equal(h.canLeave(), true);
    await h.tick(2000);
    await h.tick(5000);
    assert.equal(h.writes.length, 3);
    h.edit("C");
    assert.equal(h.run("saveError"), false);
    await h.tick();
    await h.succeed(3);
    assert.equal(h.persisted(), JSON.stringify({ text: h.text() }));
    h.cleanup();
  });
}


test("close requests after a save error can be cancelled without losing text or autosave", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await failAllAttempts(h);
  assert.equal(await h.canClose(), false);
  assert.equal(await h.canClose(), false);
  assert.equal(h.run("showLeaveConfirmation"), true);
  assert.equal(h.run("pendingClose"), true);
  // The existing Continue editing button only dismisses the close request.
  h.run("pendingClose = false; showLeaveConfirmation = false;");
  assert.equal(h.destroyed(), 0);
  assert.equal(h.text(), "B");
  assert.equal(h.run("isLeavingWithoutSaving"), false);
  h.edit("C");
  await h.tick();
  await h.succeed(3);
  assert.equal(h.pending(), false);
  assert.equal(h.run("saveError"), false);
  h.cleanup();
});

test("confirmed close waits for an active write and discards later edits without another save", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("C");
  assert.equal(await h.canClose(), false);
  const leaving = h.run("leaveWithoutSaving()");
  assert.equal(h.destroyed(), 0);
  await h.tick();
  assert.equal(h.writes.length, 1);
  await h.succeed(0);
  await leaving;
  assert.equal(h.destroyed(), 1);
  assert.equal(h.persisted(), '{"text":"B"}');
  await h.tick(2000);
  await h.tick(5000);
  assert.equal(h.writes.length, 1);
  h.cleanup();
});

test("close without pending changes proceeds without a confirmation", async () => {
  const h = await harness();
  assert.equal(await h.canClose(), true);
  assert.equal(h.run("showLeaveConfirmation"), false);
  h.edit("B");
  await h.tick();
  assert.equal(await h.canClose(), false);
  h.run("pendingClose = false; showLeaveConfirmation = false;");
  await h.succeed(0);
  assert.equal(await h.canClose(), true);
  assert.equal(h.run("showLeaveConfirmation"), false);
  h.cleanup();
});

test("session generation accompanies writes and conflicts stop queued, automatic and manual retries", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  assert.equal(h.writes[0].generation, 1);
  h.edit("C");
  await h.tick();
  h.writes[0].reject("journal_generation_conflict");
  await h.flush();
  await h.tick();
  await h.tick(5000);
  await h.run("retrySave()");
  h.edit("D");
  await h.tick();
  assert.equal(h.writes.length, 1);
  assert.equal(h.run("generationConflict"), true);
  assert.equal(h.run("saveError"), true);
  assert.equal(h.text(), "D");
  assert.equal(h.persisted(), '{"text":"A"}');
  assert.equal(h.canLeave(), false);
  assert.equal(await h.canClose(), false);
});

test("a delayed retry keeps its original generation and stops immediately on conflict", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  await rejectWrite(h, 0);
  await h.tick();
  assert.equal(h.writes[1].generation, 1);
  h.writes[1].reject("journal_generation_conflict");
  await h.flush();
  await h.tick(5000);
  assert.equal(h.writes.length, 2);
  assert.equal(h.pending(), true);
});

test("new entries acquire a generation before their first save", async () => {
  const h = await harness({ isNew: true });
  h.edit("First entry");
  await h.tick();
  assert.equal(h.writes.length, 1);
  assert.equal(h.writes[0].generation, 1);
});

test("a generation conflict keeps leave protection even if text returns to the old saved snapshot", async () => {
  const h = await harness();
  h.edit("B");
  await h.tick();
  h.edit("A");
  h.writes[0].reject("journal_generation_conflict");
  await h.flush();
  assert.equal(h.pending(), true);
  assert.equal(h.canLeave(), false);
  assert.equal(await h.canClose(), false);
  await h.tick();
  assert.equal(h.writes.length, 1);
});

// The compiler assertion verifies the real route boundary. The scenarios below
// exercise the actual editor script with isolated mounts, not a browser router.
test("entry route keys the whole child page on the current URL ID", () => {
  const layout = readFileSync(new URL("../src/routes/journal/entries/[id]/+layout.svelte", import.meta.url), "utf8");
  const ast = parse(layout, { modern: true });
  const meaningful = nodes => nodes.filter(n => !["Comment"].includes(n.type) && !(n.type === "Text" && !n.data.trim()));
  const [boundary] = meaningful(ast.fragment.nodes);
  assert.equal(meaningful(ast.fragment.nodes).length, 1);
  assert.equal(boundary.type, "KeyBlock");
  assert.equal(layout.slice(boundary.expression.start, boundary.expression.end), "page.params.id");
  const [child] = meaningful(boundary.fragment.nodes);
  assert.equal(meaningful(boundary.fragment.nodes).length, 1);
  assert.equal(child.type, "RenderTag");
  assert.equal(child.expression.callee.name, "children");
  const compiled = compile(layout, { filename: "+layout.svelte", generate: "client" });
  assert.equal(compiled.warnings.length, 0);
});

const entryUrl = id => `https://example.invalid/journal/entries/${id}`;
const mountEntry = (id, extra = {}) => harness({
  journalEntryId: id,
  initialText: `text-${id}`,
  entryDate: id === "A" ? "2026-10-17" : "2026-10-18",
  ...extra,
});

test("allowed A to B remount binds content, date and every write to B", async () => {
  const a = await mountEntry("A");
  assert.equal(a.canLeave(entryUrl("B")), true);
  a.cleanup();
  const b = await mountEntry("B");
  assert.equal(b.run("entryId"), "B");
  assert.equal(b.run("savedEntryDate"), "2026-10-18");
  assert.equal(b.text(), "text-B");
  b.edit("changed-B");
  await b.tick();
  assert.equal(b.writes[0].id, "B");
  assert.equal(b.writes[0].entryDate, "2026-10-18");
  assert.equal(b.writes[0].contentJson, '{"text":"changed-B"}');
  await b.succeed(0);
  await a.tick();
  assert.equal(a.writes.length, 0);
  assert.equal(b.canLeave(entryUrl("A")), true);
  b.cleanup();
  const back = await mountEntry("A");
  assert.equal(back.run("entryId"), "A");
  assert.equal(back.run("savedEntryDate"), "2026-10-17");
  assert.equal(back.text(), "text-A");
  back.cleanup();
});

test("unsaved A blocks B and continuing keeps the same A editor and save target", async () => {
  const a = await mountEntry("A");
  a.edit("unsaved-A");
  assert.equal(a.canLeave(entryUrl("B")), false);
  assert.equal(a.run("showLeaveConfirmation"), true);
  assert.equal(a.run("pendingNavigationUrl"), entryUrl("B"));
  // The Continue editing button only dismisses the pending dialog.
  a.run("pendingClose = false; showLeaveConfirmation = false;");
  assert.equal(a.text(), "unsaved-A");
  assert.equal(a.run("entryId"), "A");
  await a.tick();
  assert.equal(a.writes[0].id, "A");
  await a.succeed(0);
  assert.equal(a.canLeave(entryUrl("B")), true);
  a.cleanup();
});

test("discard navigation waits for A write and B starts with saving enabled", async () => {
  let b;
  const a = await mountEntry("A", { onGoto: async destination => {
    assert.equal(destination, entryUrl("B"));
    // The same beforeNavigate callback must permit the confirmed navigation.
    assert.equal(a.canLeave(destination), true);
    a.cleanup();
    b = await mountEntry("B");
  } });
  a.edit("saving-A");
  await a.tick();
  assert.equal(a.canLeave(entryUrl("B")), false);
  a.edit("discarded-A");
  const leaving = a.run("leaveWithoutSaving()");
  await a.flush();
  assert.equal(b, undefined, "must wait for the original A write");
  await a.succeed(0);
  await leaving;
  assert.equal(a.writes.length, 1);
  assert.equal(a.writes[0].id, "A");
  assert.equal(a.writes[0].contentJson, '{"text":"saving-A"}');
  assert.equal(b.run("isLeavingWithoutSaving"), false);
  assert.equal(b.pending(), false);
  assert.equal(b.text(), "text-B");
  b.edit("saved-B");
  await b.tick();
  assert.equal(b.writes[0].id, "B");
  await b.succeed(0);
  await a.tick();
  assert.equal(a.writes.length, 1, "discarded A must not autosave into either post");
  b.cleanup();
});

test("late A load after teardown cannot replace B content or its save ID", async () => {
  let resolveLoad;
  const loadResponse = new Promise(resolve => { resolveLoad = resolve; });
  const a = await mountEntry("A", { loadResponse });
  assert.equal(a.run("isLoading"), true);
  assert.equal(a.canLeave(entryUrl("B")), true);
  a.cleanup();
  const b = await mountEntry("B");
  resolveLoad({ generation: 1, data: {
    id: "A", entryDate: "2026-10-17", contentJson: '{"text":"late-A"}',
  } });
  await a.flush();
  assert.equal(a.run("editor"), null);
  assert.equal(a.run("entryId"), null);
  assert.equal(b.run("entryId"), "B");
  assert.equal(b.run("savedEntryDate"), "2026-10-18");
  assert.equal(b.text(), "text-B");
  assert.equal(a.writes.length, 0);
  b.edit("still-B");
  await b.tick();
  assert.equal(b.writes[0].id, "B");
  await b.succeed(0);
  b.cleanup();
});

test("teardown cancels an A autosave timer before B becomes editable", async () => {
  const a = await mountEntry("A");
  a.edit("never-save-A");
  assert.equal(a.canLeave(entryUrl("B")), false);
  // No save has started, so the existing discard path completes immediately.
  await a.run("leaveWithoutSaving()");
  a.cleanup();
  const b = await mountEntry("B");
  await a.tick();
  assert.equal(a.writes.length, 0);
  assert.equal(b.text(), "text-B");
  assert.equal(b.run("isLeavingWithoutSaving"), false);
  b.cleanup();
});


const emptyDocument = { type: "doc", content: [{ type: "paragraph" }] };
const textDocument = value => ({ type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: value }] }] });

test("Done on a new empty document does not save or claim to have saved", async () => {
  const h = await harness({ isNew: true, initialDocument: emptyDocument });
  await h.run("finishEditing()");
  assert.equal(h.writes.length, 0);
  assert.equal(h.run("entryId"), null);
  assert.equal(h.run("isEditing"), false);
  assert.equal(h.run("saveMessage"), "");
  assert.equal(h.canLeave(), true);
  assert.equal(await h.canClose(), true);
  h.cleanup();
});

test("clearing a new entry cancels pending autosave and permits safe navigation", async () => {
  const h = await harness({ isNew: true, initialDocument: emptyDocument });
  h.editDocument(textDocument("draft"));
  assert.equal(h.canLeave(), false);
  h.editDocument(emptyDocument);
  assert.equal(h.pending(), false);
  await h.tick();
  await h.run("saveEntry()");
  assert.equal(h.writes.length, 0);
  assert.equal(h.canLeave(), true);
  h.cleanup();
});

test("whitespace and formatting only do not create a new entry", async () => {
  const h = await harness({ isNew: true, initialDocument: emptyDocument });
  h.editDocument({ type: "doc", content: [{ type: "heading", attrs: { level: 2 }, content: [
    { type: "text", text: " \u00a0\ufeff", marks: [{ type: "bold" }] }, { type: "hardBreak" },
  ] }] });
  await h.tick();
  await h.run("finishEditing()");
  assert.equal(h.writes.length, 0);
  assert.equal(h.canLeave(), true);
  h.cleanup();
});

test("a queued empty document waits for the first ID and then clears that same entry", async () => {
  const h = await harness({ isNew: true, initialDocument: emptyDocument });
  h.editDocument(textDocument("first snapshot"));
  await h.tick();
  assert.equal(h.writes.length, 1);
  assert.equal(h.writes[0].id, null);
  h.editDocument(emptyDocument);
  assert.equal(h.canLeave(), false, "an in-flight write still blocks navigation");
  assert.equal(await h.canClose(), false);
  const queued = h.run("saveEntry()");
  await h.flush();
  assert.equal(h.writes.length, 1);
  await h.succeed(0);
  assert.equal(h.writes.length, 2);
  assert.equal(h.writes[1].id, "entry");
  assert.equal(h.writes[1].contentJson, JSON.stringify(emptyDocument));
  await h.succeed(1);
  assert.equal(await queued, true);
  await h.tick();
  assert.equal(h.writes.length, 2);
  assert.equal(h.run("entryId"), "entry");
  assert.equal(h.pending(), false);
  assert.equal(h.canLeave(), true);
  h.cleanup();
});

test("clearing an existing post continues saving with its original ID", async () => {
  const h = await harness({ initialDocument: textDocument("saved") });
  h.editDocument(emptyDocument);
  assert.equal(h.canLeave(), false);
  await h.tick();
  assert.equal(h.writes[0].id, "entry");
  assert.equal(h.writes[0].contentJson, JSON.stringify(emptyDocument));
  await h.succeed(0);
  assert.equal(h.run("entryId"), "entry");
  assert.equal(h.pending(), false);
  h.cleanup();
});

test("future image or unknown content keeps autosave and navigation protection", async () => {
  for (const type of ["image", "future"]) {
    const h = await harness({ isNew: true, initialDocument: emptyDocument });
    const document = { type: "doc", content: [{ type, attrs: { src: "photo.jpg" } }] };
    h.editDocument(document);
    assert.equal(h.canLeave(), false);
    await h.tick();
    assert.equal(h.writes.length, 1);
    assert.equal(h.writes[0].contentJson, JSON.stringify(document));
    await h.succeed(0);
    h.cleanup();
  }
});

test("clearing during a failed first-write retry retains queue and snapshot contracts", async () => {
  const h = await harness({ isNew: true, initialDocument: emptyDocument });
  h.editDocument(textDocument("captured"));
  await h.tick();
  await rejectWrite(h, 0);
  h.editDocument(emptyDocument);
  assert.equal(h.canLeave(), false);
  await h.tick(2000);
  assert.equal(h.writes[1].id, null);
  assert.equal(h.writes[1].contentJson, JSON.stringify(textDocument("captured")));
  await h.succeed(1);
  await h.tick();
  assert.equal(h.writes[2].id, "entry");
  assert.equal(h.writes[2].contentJson, JSON.stringify(emptyDocument));
  await h.succeed(2);
  assert.equal(h.pending(), false);
  h.cleanup();
});


test("direct edit starts only after loading the original entry and does not save on open or Done", async () => {
  const h = await mountEntry("B", { startInEditMode: true, manuallyStartEditing: false });
  assert.equal(h.run("isEditing"), true);
  assert.equal(h.run("entryId"), "B");
  assert.equal(h.pending(), false);
  await h.tick();
  assert.equal(h.writes.length, 0);
  await h.run("finishEditing()");
  assert.equal(h.writes.length, 0);
  h.cleanup();
});

test("direct edit keeps autosave and navigation protection bound to the selected ID", async () => {
  const h = await mountEntry("B", { startInEditMode: true, manuallyStartEditing: false });
  h.edit("direct-B");
  assert.equal(h.canLeave(entryUrl("A")), false);
  await h.tick(1999);
  assert.equal(h.writes.length, 0);
  await h.tick(2000);
  assert.equal(h.writes.length, 1);
  assert.equal(h.writes[0].id, "B");
  assert.equal(h.canLeave(entryUrl("A")), false);
  await h.succeed(0);
  assert.equal(h.canLeave(entryUrl("A")), true);
  h.cleanup();
});

test("ordinary entry links remain read-only and failed direct loads do not enable editing", async () => {
  const h = await mountEntry("A", { manuallyStartEditing: false });
  assert.equal(h.run("isEditing"), false);
  assert.equal(h.writes.length, 0);
  h.cleanup();
  const missing = await mountEntry("missing", {
    startInEditMode: true, manuallyStartEditing: false,
    loadResponse: Promise.resolve({ generation: 1, data: null }),
  });
  assert.equal(missing.run("isEditing"), false);
  assert.equal(missing.run("entryId"), null);
  assert.equal(missing.writes.length, 0);
  missing.cleanup();
});

test("entry route forwards only the explicit edit query to the existing editor", () => {
  const route = readFileSync(new URL("../src/routes/journal/entries/[id]/+page.svelte", import.meta.url), "utf8");
  assert.match(route, /startInEditMode=\{page\.url\.searchParams\.get\("edit"\) === "true"\}/);
  assert.match(route, /returnToJournal=\{page\.url\.searchParams\.get\("from"\) === "journal"\}/);
  assert.match(route, /<JournalEditor[\s\S]*?\{journalEntryId\}/);
  assert.equal(compile(route, { filename: "+page.svelte", generate: "client" }).warnings.length, 0);
});

test("direct edit waits for loading and ignores a response after teardown", async () => {
  let resolve;
  const loadResponse = new Promise(done => { resolve = done; });
  const h = await mountEntry("A", {
    startInEditMode: true, manuallyStartEditing: false, loadResponse,
  });
  assert.equal(h.run("isEditing"), false);
  assert.equal(h.run("entryId"), null);
  h.cleanup();
  resolve({ generation: 1, data: {
    id: "A", entryDate: "2026-10-17", contentJson: '{"text":"late-A"}',
  } });
  await h.flush();
  assert.equal(h.run("isEditing"), false);
  assert.equal(h.run("entryId"), null);
  assert.equal(h.run("editor"), null);
  assert.equal(h.writes.length, 0);
});

test("Done from the journal returns to the same encoded ID without an unnecessary write", async () => {
  const destinations = [];
  const h = await mountEntry("A /?&", {
    startInEditMode: true, manuallyStartEditing: false, returnToJournal: true,
    onGoto: async destination => { destinations.push(destination); assert.equal(h.canLeave(new URL(destination, "https://example.invalid").href), true); },
  });
  await h.run("finishEditing()");
  assert.deepEqual(destinations, ["/journal?entry=A%20%2F%3F%26"]);
  assert.equal(h.writes.length, 0);
  h.cleanup();
});

test("Done waits for an active write and the queued latest content before returning", async () => {
  const destinations = [];
  const h = await mountEntry("B", { returnToJournal: true, onGoto: async url => destinations.push(url) });
  h.edit("saving");
  await h.tick();
  h.edit("latest");
  const done = h.run("finishEditing()");
  await h.flush();
  assert.equal(h.canLeave(), false);
  assert.equal(destinations.length, 0);
  await h.succeed(0);
  assert.equal(h.writes.length, 2);
  assert.equal(h.writes[1].id, "B");
  assert.equal(h.writes[1].contentJson, '{"text":"latest"}');
  assert.equal(destinations.length, 0);
  await h.succeed(1);
  await done;
  assert.deepEqual(destinations, ["/journal?entry=B"]);
  assert.equal(h.pending(), false);
  h.cleanup();
});

test("failed Done from journal stays editing with content and navigation protection", async () => {
  const destinations = [];
  const h = await mountEntry("B", { returnToJournal: true, onGoto: async url => destinations.push(url) });
  h.edit("unsaved");
  const done = h.run("finishEditing()");
  await h.flush();
  await failAllAttempts(h);
  await done;
  assert.equal(destinations.length, 0);
  assert.equal(h.run("isEditing"), true);
  assert.equal(h.text(), "unsaved");
  assert.equal(h.canLeave(), false);
  h.cleanup();
});

test("edits during Done's write prevent early return to the journal", async () => {
  const destinations = [];
  const h = await mountEntry("B", { returnToJournal: true, onGoto: async url => destinations.push(url) });
  h.edit("snapshot");
  const done = h.run("finishEditing()");
  await h.flush();
  h.edit("newer text");
  await h.succeed(0);
  await done;
  assert.equal(destinations.length, 0);
  assert.equal(h.run("isEditing"), true);
  assert.equal(h.canLeave(), false);
  h.cleanup();
});

test("Done from other origins keeps the separate read view", async () => {
  const destinations = [];
  const h = await mountEntry("B", { startInEditMode: true, onGoto: async url => destinations.push(url) });
  await h.run("finishEditing()");
  assert.equal(h.run("isEditing"), false);
  assert.equal(destinations.length, 0);
  assert.equal(h.writes.length, 0);
  h.cleanup();
});

test("import navigation is blocked for unsaved text and in-flight writes, then allowed after save", async () => {
  const h = await harness();
  const destination = "https://example.invalid/settings/import";
  assert.equal(h.canLeave(destination), true);
  h.edit("not saved yet");
  assert.equal(h.canLeave(destination), false);
  await h.tick();
  assert.equal(h.canLeave(destination), false);
  await h.succeed(0);
  assert.equal(h.canLeave(destination), true);
  h.cleanup();
});

test("explicit discard before import waits for an existing save and drops only later edits", async () => {
  const visited = [];
  const h = await harness({ onGoto: async url => visited.push(url) });
  const destination = "https://example.invalid/settings/import";
  h.edit("already saving"); await h.tick(); h.edit("explicitly discarded");
  assert.equal(h.canLeave(destination), false);
  const leaving = h.run("leaveWithoutSaving()"); await h.flush();
  assert.deepEqual(visited, []);
  await h.succeed(0); await leaving;
  assert.deepEqual(visited, [destination]);
  await h.tick(); assert.equal(h.writes.length, 1);
  h.cleanup();
});

test("failed saves keep import navigation blocked and retain the journal text", async () => {
  const h = await harness();
  h.edit("must not be lost"); await h.tick(); await failAllAttempts(h);
  assert.equal(h.canLeave("https://example.invalid/settings/import"), false);
  assert.equal(h.text(), "must not be lost");
  h.cleanup();
});


test("saved editor cannot bypass reserved work through Tauri automatic destroy", async () => {
  const h = await harness();
  h.reserve();
  assert.equal(await h.canClose(), false);
  assert.equal(h.destroyed(), 0);
  assert.equal(h.closeCalls(), 1);
  assert.equal(h.run("showLeaveConfirmation"), false);
  h.reserve(false);
  assert.equal(await h.canClose(), true);
  assert.equal(h.destroyed(), 1);
  h.cleanup();
});

test("discard during reserved work stays open and restores autosaving", async () => {
  const h = await harness();
  h.edit("B");
  assert.equal(await h.canClose(), false);
  assert.equal(h.run("showLeaveConfirmation"), true);
  h.reserve();
  await h.run("leaveWithoutSaving()");
  assert.equal(h.destroyed(), 0);
  assert.equal(h.text(), "B");
  assert.equal(h.run("isLeavingWithoutSaving"), false);
  assert.equal(h.run("pendingClose"), false);
  await h.tick();
  assert.equal(h.writes.length, 1);
  await h.succeed(0);
  h.reserve(false);
  assert.equal(await h.canClose(), true);
  h.cleanup();
});

test("failed guarded close after discard retains text and permits another confirmation", async () => {
  const h = await harness();
  h.edit("B");
  assert.equal(await h.canClose(), false);
  h.failClose();
  await h.run("leaveWithoutSaving()");
  assert.equal(h.destroyed(), 0);
  assert.equal(h.run("isLeavingWithoutSaving"), false);
  assert.equal(await h.canClose(), false);
  assert.equal(h.run("showLeaveConfirmation"), true);
  h.failClose(false);
  await h.run("leaveWithoutSaving()");
  assert.equal(h.destroyed(), 1);
  h.cleanup();
});


for (const [name, raw, editable, documentVersion] of [
  ['unknown node', '{"type":"doc","content":[{"type":"image","attrs":{"src":"external.jpg"}}]}', false, 1],
  ['unknown attribute', '{"type":"doc","content":[{"type":"paragraph","attrs":{"sourceNote":"keep"}}]}', false, 1],
  ['legacy object', '{}', false, 1],
  ['duplicate inner key', '{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"first","text":"second"}]}]}', false, 1],
  ['future version', '{"type":"doc","content":[{"type":"paragraph"}]}', true, 3],
]) {
  test(`protected ${name} never reaches Tiptap or any save path`, async () => {
    const h = await harness({ initialJson: raw, editable, documentVersion, startInEditMode: true });
    assert.equal(h.run('documentProtected'), true);
    assert.equal(h.run('isEditing'), false);
    assert.equal(h.loadedDocuments.length, 0, 'original content must be checked before Tiptap can normalize it');
    assert.deepEqual(h.editingChanges, [], 'the parent must not enable the date picker');
    h.run('entryDate = "2000-01-01"; startEditing(); scheduleAutosave()');
    await h.tick();
    assert.equal(await h.run('saveEntry()'), false);
    assert.equal(await h.run('performSave()'), false);
    await h.run('retrySave()');
    await h.run('finishEditing()');
    assert.equal(h.writes.length, 0);
    assert.equal(h.persisted(), raw);
    assert.equal(h.run('isEditing'), false);
    h.cleanup();
  });
}
test('compatible content uses strict Tiptap loading and saves normal edits', async () => {
  const doc = {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'Original'}]}]};
  const h = await harness({ initialDocument: doc, startInEditMode: true });
  assert.equal(h.run('documentProtected'), false);
  assert.equal(h.run('isEditing'), true);
  assert.equal(h.editorOptions.enableContentCheck, true);
  assert.equal(h.loadedDocuments[0].options.errorOnInvalidContent, true);
  assert.equal(h.loadedDocuments[0].options.emitUpdate, false);
  const next = {type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'Changed'}]}]};
  h.editDocument(next);await h.tick();
  assert.equal(h.writes[0].contentJson, JSON.stringify(next));
  await h.succeed(0);assert.equal(h.pending(), false);h.cleanup();
});


test('backend incompatibility stops retries and queued saves while retaining draft text', async () => {
  const h = await harness();h.edit('New draft');await h.tick();
  const queued = h.run('saveEntry()');
  h.writes[0].reject('journal_document_incompatible');await queued;await h.flush();
  assert.equal(h.run('documentProtected'), true);
  assert.equal(h.run('isEditing'), false);
  assert.equal(h.text(), 'New draft');
  assert.equal(h.writes.length, 1);
  await h.tick(2000);await h.run('retrySave()');
  assert.equal(h.writes.length, 1);
  assert.equal(h.pending(), true, 'unsaved draft retains leave protection');h.cleanup();
});

for (const destination of ["https://example.invalid/search?q=hackspett", "https://example.invalid/search?q=%C3%B6rn"]) {
  test(`search navigation retains unsaved and in-flight write guards: ${destination}`, async () => {
    const h = await harness();
    h.edit("not saved yet"); assert.equal(h.canLeave(destination), false);
    await h.tick(); assert.equal(h.canLeave(destination), false);
    await h.succeed(0); assert.equal(h.canLeave(destination), true); h.cleanup();
  });
}

for (const destination of ["https://example.invalid/history", "https://example.invalid/history?year=2024&month=2"]) {
  test(`timeline navigation retains unsaved and in-flight write guards: ${destination}`, async () => {
    const h = await harness();
    h.edit("timeline draft"); assert.equal(h.canLeave(destination), false);
    await h.tick(); assert.equal(h.canLeave(destination), false);
    await h.succeed(0); assert.equal(h.canLeave(destination), true); h.cleanup();
  });
}


for (const kind of ["dirty", "busy"]) {
  test(`finish preserves weather ${kind} started during an outstanding journal write`, async () => {
    const h = await harness({ journalEntryId: "synthetic", returnToJournal: false });
    h.edit("B");
    const finishing = h.run("finishEditing()");
    await h.flush();
    assert.equal(h.writes.length, 1);
    h.run(kind === "dirty" ? "weatherDirty = true" : "weatherBusy = true");
    await h.succeed(0);
    await finishing;
    assert.equal(h.run("isEditing"), true);
    assert.equal(h.editingChanges.includes(false), false);
    if (kind === "dirty") assert.equal(h.run("weatherWarning"), true);
    h.cleanup();
  });
}

for(const weatherSaved of [true,false]) {
  test(`new journal Done saves text before flushing manual weather; weather success=${weatherSaved}`,async()=>{
    const h=await harness({isNew:true});h.edit("New journal with weather");
    h.run(`weatherDirty=true;weatherControl={flushPending:async()=>{if(!entryId)throw 'ID required';weatherDirty=${!weatherSaved};return ${weatherSaved};}}`);
    const finishing=h.run('finishEditing()');await h.flush();assert.equal(h.writes.length,1);
    await h.succeed(0);await finishing;
    assert.equal(h.run('isEditing'),!weatherSaved);
    assert.equal(h.run('weatherDirty'),!weatherSaved);
    if(!weatherSaved){assert.equal(h.run('weatherWarning'),true);assert.equal(h.canLeave(),false);}
    h.cleanup();
  });
}

test("deletion waits for a saved read-mode entry and refreshes only after commit", async () => {
  const effects = [];
  let finishDelete;
  const h = await harness({
    journalEntryId: "entry",
    onGoto: path => effects.push(path),
    browserGlobals: { recordEffect: value => effects.push(value) },
    photoInvoke(command, args) {
      if (command !== "delete_journal_entry") return;
      assert.equal(args.id, "entry");
      assert.equal(args.generation, 1);
      assert.equal(finishDelete, undefined, "only one deletion may run");
      return new Promise(resolve => { finishDelete = resolve; });
    },
  });
  assert.equal(h.run("canDeleteEntry()"), false);
  h.edit("B");
  const finishing = h.run("finishEditing()");
  assert.equal(h.run("canDeleteEntry()"), false);
  await h.succeed(0);
  await finishing;
  assert.equal(h.run("canDeleteEntry()"), true);
  h.run('window.dispatchEvent = event => recordEffect(event.type)');
  // Verify cancel never invokes the command, and confirmation waits for commit.
  await h.run("deleteSavedEntry()");
  assert.equal(finishDelete, undefined);
  h.run("showDeleteConfirmation = true");
  const deleting = h.run("deleteSavedEntry()");
  assert.equal(h.canLeave(), false);
  await h.run("deleteSavedEntry()");
  assert.deepEqual(effects, []);
  finishDelete();
  await deleting;
  assert.equal(h.run("canDeleteEntry()"), false);
  assert.deepEqual(effects, ["mno:journal-changed", "/journal?latest=true"]);
  h.cleanup();
});

test("failed deletion keeps the saved entry detail available", async () => {
  const h = await harness({
    journalEntryId: "entry",
    manuallyStartEditing: false,
    onGoto: () => assert.fail("must stay on the detail page"),
    photoInvoke(command) {
      if (command === "delete_journal_entry") return Promise.reject("write failed");
    },
  });
  h.run("showDeleteConfirmation = true");
  await h.run("deleteSavedEntry()");
  assert.equal(h.run("deleteFailed"), true);
  assert.equal(h.run("entryDeleted"), false);
  assert.equal(h.run("canDeleteEntry()"), true);
  h.cleanup();
});
