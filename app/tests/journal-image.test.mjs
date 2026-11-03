import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";
import * as core from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

import { JournalImage } from "./photo-test-modules.mjs";

test("document v2 image is atomic and preserves independent node captions", () => {
  const schema = core.getSchema([StarterKit, JournalImage]);
  const image = (nodeId, caption) => ({ type: "journalImage", attrs: {
    nodeId, assetId: "b7d05577-9248-49b7-af7b-2a240ee52bd7", placement: "left", size: "small", caption, alt: "Bird" } });
  const document = { type: "doc", content: [image("bada5145-7426-435a-8fe4-945b95d495c5", "First"), image("d581b905-b692-498d-8955-830251b82825", "Second")] };
  const node = schema.nodeFromJSON(document);
  node.check();
  assert.equal(node.firstChild.isAtom, true);
  assert.equal(node.firstChild.isLeaf, true);
  assert.deepEqual(JSON.parse(JSON.stringify(node.toJSON())), document);
  assert.equal(core.getSchema([StarterKit]).nodes.journalImage, undefined);
});


test("v2 defaults remain row/large and every placement/size round-trips without changing IDs", () => {
  const schema = core.getSchema([StarterKit, JournalImage]);
  const ids = {nodeId:"d581b905-b692-498d-8955-830251b82825",assetId:"b7d05577-9248-49b7-af7b-2a240ee52bd7"};
  assert.equal(schema.nodes.journalImage.create(ids).attrs.placement,"row");
  assert.equal(schema.nodes.journalImage.create(ids).attrs.size,"large");
  for(const placement of ["left","right","row"]) for(const size of ["small","medium","large"]) {
    const node=schema.nodes.journalImage.create({...ids,placement,size,caption:"Long caption ".repeat(100),alt:"Separate"});
    assert.deepEqual(schema.nodeFromJSON(JSON.parse(JSON.stringify(node.toJSON()))).toJSON(),node.toJSON());
    assert.equal(node.attrs.nodeId,ids.nodeId);assert.equal(node.attrs.assetId,ids.assetId);
  }
});


test("photograph insertion and centered placement use the approved English and Swedish labels", () => {
  for(const [locale,insert,row] of [["en","Insert image","Centered"],["sv","Infoga bild","Centrerad"]]) {
    const messages=JSON.parse(readFileSync(new URL(`../messages/${locale}.json`,import.meta.url),"utf8"));
    assert.equal(messages.photo_insert,insert);assert.equal(messages.photo_row,row);assert.equal(messages.photo_medium,locale==="en"?"Medium":"Mellan");
  }
  assert.equal(JournalImage.options.labels().row,"Centered");
  const editorSource=readFileSync(new URL("../src/lib/components/JournalEditor.svelte",import.meta.url),"utf8");
  assert.match(editorSource,/m\.photo_insert\(/);
});
