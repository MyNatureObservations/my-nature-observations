import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("../src/lib/journal-empty.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText;
const { isDefinitelyEmptyJournalDocument, isDefinitelyEmptyJournalJson } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const cases = JSON.parse(readFileSync(new URL("./fixtures/journal-empty-documents.json", import.meta.url), "utf8"));
for (const example of cases) {
  test(`conservative emptiness: ${example.name}`, () => {
    const before = JSON.stringify(example);
    const json = example.json ?? JSON.stringify(example.document);
    assert.equal(isDefinitelyEmptyJournalJson(json), example.empty);
    if (!("json" in example)) assert.equal(isDefinitelyEmptyJournalDocument(example.document), example.empty);
    assert.equal(JSON.stringify(example), before);
  });
}
