import assert from "node:assert/strict";
import test from "node:test";
import { getSchema } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { history, undo, redo, closeHistory } from "@tiptap/pm/history";
import { JournalImage, photographs } from "./photo-test-modules.mjs";
import { harness } from "./journal-editor-test-harness.mjs";
const { capturePhotoPosition, mapPhotoPosition, insertPhotograph, readPhotograph } = photographs;
const schema = getSchema([StarterKit, JournalImage]);
const paragraph = text => ({ type: "paragraph", ...(text ? { content: [{ type: "text", text }] } : {}) });
const attrs = { nodeId: "d581b905-b692-498d-8955-830251b82825", assetId: "b7d05577-9248-49b7-af7b-2a240ee52bd7", placement: "left", size: "small" };
function state(content = [paragraph("abc")], pos = 2) {
  const doc = schema.nodeFromJSON({ type: "doc", content });
  return EditorState.create({ schema, doc, selection: TextSelection.create(doc, pos), plugins: [history()] });
}
function insert(current, extra = {}) { return current.apply(insertPhotograph(current.tr, capturePhotoPosition(current), { ...attrs, ...extra })); }
function image(current) { let result; current.doc.forEach((node, pos) => { if (node.type.name === "journalImage") result = { node, pos }; }); return result; }
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
const asset = owner => ({ asset_id: attrs.assetId, entry_id: owner, original_path: "/missing/original.png", mime: "image/png", width: 80, height: 40, preview: "aGVsbG8=" });

for (const kind of ["paragraph", "heading", "list"]) test(`insertion splits paragraphs and follows enclosing ${kind}`, () => {
  const content = kind === "paragraph" ? [paragraph("abc"), paragraph("tail")]
    : kind === "heading" ? [{ type: "heading", attrs: { level: 2 }, content: [{ type: "text", text: "abc" }] }, paragraph("tail")]
    : [{ type: "bulletList", content: [{ type: "listItem", content: [paragraph("abc")] }] }, paragraph("tail")];
  const current = insert(state(content, kind === "list" ? 4 : 2));
  const nodes = current.doc.toJSON().content;
  assert.equal(nodes[1].type, "journalImage");
  assert.equal(nodes[1].attrs.placement, "left");
  assert.equal(nodes[1].attrs.size, "small");
  if (kind === "paragraph") { assert.equal(nodes[0].content[0].text, "a"); assert.equal(nodes[2].content[0].text, "bc"); }
  else assert.equal(nodes[0].type, kind === "list" ? "bulletList" : "heading");
  current.doc.check();
});

test("sequential insertion order, caption/alt changes, removal and undo/redo preserve references", () => {
  const original = state();
  let current = insert(original);
  const first = current.doc.toJSON();
  const dispatch = tr => { current = current.apply(tr); };
  assert.equal(undo(current, dispatch), true);
  assert.deepEqual(current.doc.toJSON(), original.doc.toJSON());
  assert.equal(redo(current, dispatch), true);
  assert.deepEqual(current.doc.toJSON(), first);
  const { node, pos } = image(current);
  current = current.apply(closeHistory(current.tr.setNodeMarkup(pos, undefined, { ...node.attrs, caption: "Visible caption", alt: "Independent alternative" })));
  assert.equal(image(current).node.attrs.caption, "Visible caption");
  assert.equal(image(current).node.attrs.alt, "Independent alternative");
  assert.equal(image(current).node.attrs.assetId, attrs.assetId);
  current = current.apply(closeHistory(current.tr.delete(pos, pos + node.nodeSize)));
  assert.equal(image(current), undefined);
  assert.equal(undo(current, dispatch), true);
  assert.equal(image(current).node.attrs.caption, "Visible caption");
  current = insert(current, { nodeId: "f07c5922-8ebd-4d83-b869-c2e30e6b5e52" });
  const ids = current.doc.toJSON().content.filter(node => node.type === "journalImage").map(node => node.attrs.nodeId);
  assert.deepEqual(ids, [attrs.nodeId, "f07c5922-8ebd-4d83-b869-c2e30e6b5e52"]);
});

test("mapped insertion follows edits, ignores selection movement, and detects deleted positions", () => {
  let current = state();
  let anchor = capturePhotoPosition(current);
  const edit = current.tr.insertText("X", 1);
  anchor = mapPhotoPosition(anchor, edit); current = current.apply(edit);
  current = current.apply(current.tr.setSelection(TextSelection.create(current.doc, 4)));
  current = current.apply(insertPhotograph(current.tr, anchor, attrs));
  assert.equal(current.doc.toJSON().content[0].content[0].text, "Xa");
  const original = state();
  assert.equal(mapPhotoPosition(capturePhotoPosition(original), original.tr.delete(0, original.doc.content.size)).valid, false);
});

test("preview reads reject obsolete generations and wrong entry owners", async () => {
  await assert.rejects(readPhotograph(async command => command === "get_journal_photograph" ? asset("entry") : { generation: 2 }, "entry", 1, attrs.assetId), /generation_conflict/);
  await assert.rejects(readPhotograph(async command => command === "get_journal_photograph" ? asset("other") : { generation: 1 }, "entry", 1, attrs.assetId), /missing_asset/);
});

async function photoHarness(options = {}) {
  const pending = deferred(); let owner; let generation = 1;
  const current = state();
  const h = await harness({ realState: current, initialDocument: current.doc.toJSON(), isNew: true, photoInvoke(command, args) {
    if (command === "select_journal_photograph") return Promise.resolve("/source.png");
    if (command === "prepare_journal_photograph") { owner = args.entryId; return pending.promise; }
    if (command === "new_journal_session") return Promise.resolve({ generation, data: null });
  }, ...options });
  return { h, pending, owner: () => owner, nextGeneration: () => generation++ };
}

test("preparation inserts only after success and v2 autosave keeps prepared assets across undo/removal", async () => {
  const { h, pending, owner } = await photoHarness();
  const operation = h.run("insertPhoto()"); await h.flush();
  assert.equal(h.run("photoBusy"), true); assert.equal(h.canLeave(), false);
  assert.equal(image(h.run("editor.state")), undefined);
  pending.resolve(asset(owner())); await operation;
  assert.equal(image(h.run("editor.state")).node.attrs.assetId, attrs.assetId);
  assert.equal(h.run("documentVersion"), 2);
  await h.tick(); assert.equal(h.writes[0].command, "save_journal_with_photographs");
  assert.equal(h.writes[0].assets[0].entry_id, h.writes[0].entryId);
  await h.succeed(0);
  assert.equal(h.pending(), false);
  const current = h.run("editor.state"), { pos, node } = image(current);
  h.run("editor").view.dispatch(current.tr.delete(pos, pos + node.nodeSize));
  await h.tick(); assert.equal(h.writes[1].assets.length, 0);
  assert.equal(h.run("preparedPhotographs.size"), 1);
  await h.succeed(1); h.cleanup();
});

for (const reason of ["failure", "destroy", "discard", "generation"]) test(`asynchronous preparation is safe after ${reason}`, async () => {
  const { h, pending, owner, nextGeneration } = await photoHarness();
  const operation = h.run("insertPhoto()"); await h.flush();
  if (reason === "destroy") h.cleanup();
  if (reason === "discard") { h.canLeave(); await h.run("leaveWithoutSaving()"); }
  if (reason === "generation") nextGeneration();
  if (reason === "failure") pending.reject(new Error("bad image")); else pending.resolve(asset(owner()));
  await operation;
  assert.equal(h.writes.length, 0);
  if (reason === "failure" || reason === "generation") { assert.ok(h.run("photoError")); assert.equal(image(h.run("editor.state")), undefined); h.cleanup(); }
  if (reason === "discard") { assert.equal(image(h.run("editor.state")), undefined); h.cleanup(); }
});

test("a first text save in flight determines the prepared asset owner before insertion", async () => {
  const { h, pending, owner } = await photoHarness();
  const editor = h.run("editor"); editor.view.dispatch(editor.state.tr.insertText("X",1));
  await h.tick(); assert.equal(h.writes[0].command,"save_journal_entry");
  const operation = h.run("insertPhoto()");await h.flush();assert.equal(owner(),undefined);
  await h.succeed(0);assert.equal(owner(),"entry");
  pending.resolve(asset(owner()));await operation;await h.tick();
  assert.equal(h.writes[1].entryId,"entry");assert.equal(h.writes[1].assets[0].entry_id,"entry");
  await h.succeed(1);h.cleanup();
});

test("deleting the intended paragraph while preparation runs never inserts at a guessed position", async () => {
  const { h, pending, owner } = await photoHarness();
  const operation=h.run("insertPhoto()");await h.flush();
  const editor=h.run("editor");editor.view.dispatch(editor.state.tr.delete(0,editor.state.doc.content.size));
  pending.resolve(asset(owner()));await operation;
  assert.equal(image(editor.state),undefined);assert.ok(h.run("photoError"));assert.equal(h.run("preparedPhotographs.size"),0);h.cleanup();
});

test("changing the paragraph to a heading during preparation inserts after the complete block", () => {
  let current=state();const anchor=capturePhotoPosition(current);
  const change=current.tr.setNodeMarkup(0,schema.nodes.heading,{level:2});
  const mapped=mapPhotoPosition(anchor,change);current=current.apply(change);
  current=current.apply(insertPhotograph(current.tr,mapped,attrs));
  assert.equal(current.doc.child(0).type.name,"heading");assert.equal(current.doc.child(0).textContent,"abc");
  assert.equal(current.doc.child(1).type.name,"journalImage");
});

for (const size of ["small", "medium", "large"]) test(`reopened v2 entries save ${size} without resubmitting stored assets`, async () => {
  const current = insert(state());
  const h = await harness({ realState: current, initialDocument: current.doc.toJSON(), documentVersion: 2, journalEntryId: "entry" });
  assert.equal(h.run("documentProtected"), false);
  const { node, pos } = image(current);
  h.run("editor").view.dispatch(current.tr.setNodeMarkup(pos, undefined, { ...node.attrs, caption: "Reopened caption", alt: "Reopened alternative", placement: "right", size }));
  await h.tick();
  assert.equal(h.writes[0].command, "save_journal_with_photographs");
  assert.equal(h.writes[0].assets.length, 0);
  assert.equal(JSON.parse(h.writes[0].contentJson).content[1].attrs.caption, "Reopened caption");
  assert.equal(JSON.parse(h.writes[0].contentJson).content[1].attrs.placement, "right");
  assert.equal(JSON.parse(h.writes[0].contentJson).content[1].attrs.size, size);
  assert.doesNotMatch(h.writes[0].contentJson, /original_path|preview|data:image/);
  await h.succeed(0);assert.equal(h.pending(),false);h.cleanup();
});


test("an open photograph draft blocks finish, navigation and window close until resolved", async () => {
  const h = await harness();
  h.run("photoDraft=true; focusPhotoDraft=()=>{photoError='draft-focused'}");
  assert.equal(h.canLeave(),false);assert.equal(h.run("photoError"),"draft-focused");
  assert.equal(await h.canClose(),false);await h.run("finishEditing()");assert.equal(h.writes.length,0);
  assert.equal(h.run("isEditing"),true);assert.equal(h.run("showLeaveConfirmation"),false);
  h.run("photoDraft=false");assert.equal(h.canLeave(),true);h.cleanup();
});


for (const placement of ["left", "right"]) for (const earlier of ["", "Earlier observation."]) test(`new ${placement} image leaves a writable paragraph after ${earlier ? "partial prose" : "an empty entry"}, with undo and save/reopen`, async () => {
  const original=state([paragraph(earlier)],earlier.length+1);
  let current=insert(original,{placement});
  const {node:photo}=image(current);
  assert.equal(current.selection.$from.parent.type.name,"paragraph");
  assert.equal(current.selection.$from.parent.content.size,0);
  const insertion=current.doc.toJSON();
  current=current.apply(closeHistory(current.tr.insertText("Fresh prose.")));
  const typed=current.doc.toJSON();
  const dispatch=tr=>{current=current.apply(tr);};
  undo(current,dispatch);assert.deepEqual(current.doc.toJSON(),insertion);
  undo(current,dispatch);assert.deepEqual(current.doc.toJSON(),original.doc.toJSON());
  redo(current,dispatch);redo(current,dispatch);assert.deepEqual(current.doc.toJSON(),typed);
  const h=await harness({realState:current,initialDocument:insertion,documentVersion:2,journalEntryId:"entry"});
  h.editDocument(typed);await h.tick();
  const reopened=schema.nodeFromJSON(JSON.parse(h.writes[0].contentJson));reopened.check();
  assert.deepEqual(reopened.toJSON(),typed);assert.equal(reopened.lastChild.textContent,"Fresh prose.");
  assert.equal(reopened.content.content.find(node=>node.type.name==="journalImage").attrs.nodeId,photo.attrs.nodeId);
  assert.equal(h.writes[0].assets.length,0);await h.succeed(0);assert.equal(h.pending(),false);h.cleanup();
});
