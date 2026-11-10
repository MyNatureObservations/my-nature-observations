import { imageUrl } from "./photo-test-modules.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { compile } from "svelte/compiler";
import { render } from "svelte/server";

const source = path => readFileSync(new URL(path, import.meta.url), "utf8");
const url = code => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const transpile = (code, module = ts.ModuleKind.ESNext) => ts.transpileModule(code, {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module },
}).outputText;
const resolveImports = (code, local) => code.replace(/from (["'])([^"']+)\1/g,
  (_, quote, name) => `from ${quote}${local[name] ?? import.meta.resolve(name)}${quote}`);
const emptyUrl = url(transpile(source("../src/lib/journal-empty.ts")));
const readingUrl = url(resolveImports(transpile(source("../src/lib/journal-reading.ts")), { "$lib/journal-empty": emptyUrl, "$lib/journal-image": imageUrl }));
const reading = await import(readingUrl);
const languageUrl = url('export const language = { current: "sv" };');
const messagesUrl = new URL("../src/lib/paraglide/messages.js", import.meta.url).href;
const panelSource = source("../src/lib/components/CalendarDayPanel.svelte");
const document = text => JSON.stringify({ type: "doc", content: [{ type: "paragraph", ...(text ? { content: [{ type: "text", text }] } : {}) }] });
const entry = (id, date = "2026-10-21", contentJson = document(id)) => ({ id, title: id, entryDate: date, contentJson });

async function panelHtml(entries, { loading = false, error = false } = {}) {
  // Seed only initial state for SSR; render the actual panel template and links.
  const seeded = panelSource.replace('$state<ReadingEntry[]>([])', `$state<ReadingEntry[]>(${JSON.stringify(entries)})`)
    .replace('let isLoading = $state(true)', `let isLoading = $state(${loading})`)
    .replace('let loadError = $state(false)', `let loadError = $state(${error})`);
  const compiled = compile(seeded, { filename: "CalendarDayPanel.svelte", generate: "server" });
  assert.equal(compiled.warnings.length, 0);
  const component = await import(url(resolveImports(compiled.js.code, {
    "$lib/journal-reading": readingUrl,
    "$lib/journal-data": url('export function createJournalReader() { return () => { throw new Error("SSR must not read"); }; }'),
    "$lib/language.svelte": languageUrl,
    "$lib/paraglide/messages.js": messagesUrl,
  })));
  return render(component.default, { props: { date: "2026-10-21", onClose() {} } }).body;
}

async function harness(responses) {
  const calls = [];
  const effects = [];
  const listeners = new Map();
  const context = vm.createContext({
    $props: () => ({ date: "2026-10-21", onClose() {} }),
    $state: value => value,
    $derived: value => value,
    $effect: callback => effects.push(callback),
    visibleJournalEntries: reading.visibleJournalEntries,
    language: { current: "sv" },
    Intl, Date,
    console: { error() {} },
    window: { addEventListener: (name, callback) => listeners.set(name, callback), removeEventListener: name => listeners.delete(name) },
    invoke(command, args) {
      calls.push({ command, args });
      if (command === "new_journal_session") return Promise.resolve({ generation: 1, data: null });
      assert.equal(command, "read_journal_entries", "no writes or extra per-entry reads");
      assert.ok(responses.length, "unexpected read");
      return responses.shift();
    },
  });
  const run = code => vm.runInContext(code, context);
  run(transpile(source("../src/lib/journal-data.ts").replace(/^import .*;\n/gm, "").replace(/^export /gm, ""), ts.ModuleKind.None));
  run(transpile(panelSource.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, ""), ts.ModuleKind.None));
  const cleanups = effects.map(callback => callback());
  const flush = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
  await flush();
  return { run, calls, flush, listeners, dispose: () => cleanups.forEach(cleanup => cleanup?.()) };
}
const snapshot = entries => ({ generation: 1, data: entries });

test("Continue and All entries open the same journal at the latest experience date", () => {
  const home = source("../src/routes/+page.svelte");
  const newEntry = source("../src/routes/journal/new/+page.svelte");
  assert.match(home, /href="\/journal\?latest=true" class="journal-card"/);
  assert.match(newEntry, /href="\/journal\?latest=true" class="back-link">\s*← \{m\.journal_all_entries\(/);
  assert.match(home, /href="\/journal\/new" class="journal-card"/);
  assert.doesNotMatch(home, /Visa journalens läsvy|journal-reading-test-link/);
  assert.equal(compile(home, { filename: "+page.svelte", generate: "client" }).warnings.length, 0);
  const target = reading.resolveJournalTarget([entry("old-created-last", "2020-01-01"), entry("latest-experience", "2026-10-21")], new URLSearchParams("latest=true"));
  assert.equal(target.anchor, "journal-date-2026-10-21");
});

test("saved entry returns to the reading view or its originating Search or History", async () => {
  const path = "../src/routes/journal/entries/[id]/+page.svelte";
  const compiled = compile(source(path), { filename: path, generate: "server" });
  for (const [query, href] of [
    ["", "/journal?latest=true"],
    ["?from=search&q=red+fox", "/search?q=red%20fox"],
    ["?from=history&year=2026&month=10", "/history?year=2026&amp;month=10"],
    ["?from=history&undated=true", "/history?undated=true"],
  ]) {
    const stub = url("export default function Stub() {}");
    const component = await import(url(resolveImports(compiled.js.code, {
      "$app/state": url(`export const page = { params: { id: "saved" }, url: new URL(${JSON.stringify(`https://example.invalid/journal/entries/saved${query}`)}) };`),
      "$lib/journal-data": url('export function createJournalReader() { return () => { throw new Error("SSR must not read"); }; }'),
      "$lib/components/JournalEditor.svelte": stub,
      "$lib/components/DatePicker.svelte": stub,
      "$lib/components/EntryPlace.svelte": stub,
      "$lib/language.svelte": languageUrl,
      "$lib/paraglide/messages.js": messagesUrl,
    })));
    assert.ok(render(component.default).body.includes(`href="${href}"`), query || "saved entry without an origin");
  }
});

test("day panel shares visibility filtering, keeps same-day order and never writes", async () => {
  const data = [entry("other-day", "2026-10-20"), entry("older"), entry("empty", "2026-10-21", document("")),
    entry("unknown", "2026-10-21", '{"type":"doc","content":[{"type":"future"}]}'), entry("newer")];
  const before = JSON.stringify(data);
  const h = await harness([Promise.resolve(snapshot(data))]);
  const entries = h.run("entries");
  assert.deepEqual(Array.from(entries, e => e.id), ["newer", "unknown", "older"]);
  assert.equal(JSON.stringify(data), before);
  const html = await panelHtml(entries);
  assert.match(html, /href="\/journal\?date=2026-10-21"/);
  assert.match(html, /href="\/journal\?entry=newer"/);
  assert.doesNotMatch(html, /entry=empty|entry=other-day|\/journal\/entries\//);
  assert.match(html, /href="\/journal\/new\?date=2026-10-21"/);
  assert.equal(h.calls.filter(c => c.command === "read_journal_entries").length, 1);
  h.dispose();
});

test("empty-only days, errors and loading do not advertise hidden or stale entries", async () => {
  const h = await harness([Promise.resolve(snapshot([entry("empty", "2026-10-21", document(""))]))]);
  assert.equal(h.run("entries.length"), 0);
  for (const options of [{}, { loading: true }, { error: true }]) {
    const html = await panelHtml(options.loading || options.error ? [entry("stale")] : [], options);
    assert.doesNotMatch(html, /href="\/journal\?(?:entry|date)=/);
    assert.match(html, /href="\/journal\/new\?date=2026-10-21"/);
  }
  h.dispose();
  const failed = await harness([Promise.reject(new Error("isolated read failure"))]);
  assert.equal(failed.run("loadError"), true);
  assert.equal(failed.run("entries.length"), 0);
  failed.dispose();
});

test("date changes, refresh events and teardown cannot accept stale responses", async () => {
  let resolve;
  const h = await harness([new Promise(done => { resolve = done; }), Promise.resolve(snapshot([entry("right", "2026-10-22")]))]);
  h.run('date = "2026-10-22"');
  // An old response must be rejected even before the next date effect executes.
  resolve(snapshot([entry("wrong")]));
  await h.flush();
  assert.equal(h.run("entries.length"), 0);
  h.run('void loadEntries(date)');
  await h.flush();
  assert.equal(h.run("entries[0].id"), "right");
  h.dispose();
  assert.equal(h.listeners.size, 0);
  let late;
  const disposed = await harness([new Promise(done => { late = done; })]);
  disposed.dispose();
  late(snapshot([entry("late")]));
  await disposed.flush();
  assert.equal(disposed.run("entries.length"), 0);
});

test("journal change refresh removes an entry that has become definitely empty", async () => {
  const h = await harness([Promise.resolve(snapshot([entry("a")])), Promise.resolve(snapshot([entry("a", "2026-10-21", document(""))]))]);
  assert.equal(h.run("entries.length"), 1);
  h.listeners.get("mno:journal-changed")();
  await h.flush();
  assert.equal(h.run("entries.length"), 0);
  h.dispose();
});
