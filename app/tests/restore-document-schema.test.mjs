import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {getSchema} from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';

test('Rust restore compatibility descriptor matches the actual unconfigured StarterKit', () => {
  const editor = readFileSync(new URL('../src/lib/components/JournalEditor.svelte', import.meta.url),'utf8');
  assert.match(editor, /extensions:\s*\[StarterKit, JournalImage\.configure/);
  const schema = getSchema([StarterKit]);
  const actual = {
    nodes: Object.fromEntries(Object.entries(schema.nodes).map(([name,type]) => [name,{
      content: type.spec.content ?? '', groups: type.groups,
      attrs: type.spec.attrs ?? {}, marks: type.markSet === null ? null : type.markSet.map(m => m.name),
    }])),
    marks: Object.fromEntries(Object.entries(schema.marks).map(([name,type]) => [name,{
      attrs: type.spec.attrs ?? {}, rank: type.rank,
      excludes: Object.keys(schema.marks).filter(other => type.excludes(schema.marks[other])),
    }])),
  };
  const expected = JSON.parse(readFileSync(new URL('../src-tauri/src/backup/document-schema-v1.json',import.meta.url),'utf8'));
  assert.deepEqual(actual,expected);
  // The editor library itself confirms the structures used in the Rust fixtures.
  const fixtures = readFileSync(new URL('../src-tauri/src/backup/restore/tests.rs',import.meta.url),'utf8');
  const doc = fixtures.match(/let doc\s*=\s*r#"(.*?)"#;/s)[1];
  const node = schema.nodeFromJSON(JSON.parse(doc));
  node.check();
  assert.equal(node.textContent.includes('Title'),true);
});


test('current Tiptap serializes exact large list starts without image nodes', () => {
  const schema = getSchema([StarterKit]);
  assert.equal(schema.nodes.image, undefined);
  for (const start of [9007199254740992, -9007199254740992, 1000000000000000100, 1e30]) {
    const document = {type:'doc', content:[{type:'orderedList', attrs:{start}, content:[{type:'listItem', content:[{type:'paragraph'}]}]}]};
    const node = schema.nodeFromJSON(document);
    node.check();
    assert.equal(JSON.parse(JSON.stringify(node.toJSON())).content[0].attrs.start, start);
  }
});
