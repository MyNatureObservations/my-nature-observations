import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

const source = readFileSync(new URL("../src/lib/journal-data.ts", import.meta.url), "utf8")
  .replace(/^import .*;\n/gm, "").replace(/^export /gm, "");
const script = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;
function harness() {
  const calls = [];
  const context = vm.createContext({ invoke: (command) => new Promise((resolve, reject) => calls.push({ command, resolve, reject })) });
  vm.runInContext(script, context);
  const run = code => vm.runInContext(code, context);
  run("const read = createJournalReader()");
  const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
  return { calls, run, flush };
}
test("normal reads retain their session generation", async () => {
  const h = harness();
  const result = h.run('read("get_journal_entry")');
  h.calls[0].resolve({ generation: 1, data: { text: "A" } });
  await h.flush();
  h.calls[1].resolve({ generation: 1, data: null });
  assert.equal((await result).data.text, "A");
});
test("a delayed response from before maintenance is rejected without replacing content", async () => {
  const h = harness();
  const result = h.run('read("get_journal_entry")');
  const rejected = assert.rejects(result, error => error === "journal_generation_conflict");
  h.calls[0].resolve({ generation: 1, data: { text: "old" } });
  await h.flush();
  h.calls[1].resolve({ generation: 2, data: null });
  await rejected;
});
test("an older request cannot replace a newer read even within the same generation", async () => {
  const h = harness();
  const older = h.run('read("get_journal_entry")');
  const rejected = assert.rejects(older, error => error === "journal_read_superseded");
  const newer = h.run('read("get_journal_entry")');
  h.calls[1].resolve({ generation: 2, data: "new" });
  await h.flush();
  h.calls[2].resolve({ generation: 2, data: null });
  assert.equal((await newer).data, "new");
  h.calls[0].resolve({ generation: 1, data: "old" });
  await h.flush();
  h.calls[3].resolve({ generation: 2, data: null });
  await rejected;
});


test("full journal reads preserve documents and use the existing generation check", async () => {
  const h = harness();
  const contentJson = '{ "type": "doc", "content": [], "extra": 1.00 }';
  const entries = [{ id: "entry", entryDate: "2026-10-21", title: "Title", contentJson }];
  const result = h.run('read("read_journal_entries")');
  assert.equal(h.calls[0].command, "read_journal_entries");
  h.calls[0].resolve({ generation: 1, data: entries });
  await h.flush();
  assert.equal(h.calls[1].command, "new_journal_session");
  h.calls[1].resolve({ generation: 1, data: null });
  const snapshot = await result;
  assert.equal(snapshot.generation, 1);
  assert.equal(snapshot.data, entries);
  assert.equal(snapshot.data[0].contentJson, contentJson);
});

test("a superseded full journal read cannot replace a newer result", async () => {
  const h = harness();
  const older = h.run('read("read_journal_entries")');
  const rejected = assert.rejects(older, error => error === "journal_read_superseded");
  const newer = h.run('read("read_journal_entries")');
  h.calls[1].resolve({ generation: 1, data: [] });
  await h.flush();
  h.calls[2].resolve({ generation: 1, data: null });
  assert.equal((await newer).data.length, 0);
  h.calls[0].resolve({ generation: 1, data: [{ id: "stale" }] });
  await h.flush();
  h.calls[3].resolve({ generation: 1, data: null });
  await rejected;
});
