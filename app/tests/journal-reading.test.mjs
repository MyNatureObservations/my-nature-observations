import { imageUrl, photographsUrl } from "./photo-test-modules.mjs";
import { weatherUrl, tileUrl } from "./weather-test-modules.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import { compile } from "svelte/compiler";
import { render } from "svelte/server";
import ts from "typescript";

const source = path => readFileSync(new URL(path, import.meta.url), "utf8");
const moduleUrl = code => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const messagesUrl = new URL("../src/lib/paraglide/messages.js", import.meta.url).href;
const resolveImports = (code, local = {}) => code.replace(/from (["'])([^"']+)\1/g,
  (_, quote, name) => `from ${quote}${local[name] ?? import.meta.resolve(name)}${quote}`);
const emptyUrl = moduleUrl(ts.transpileModule(source("../src/lib/journal-empty.ts"), {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext },
}).outputText);
const readingUrl = moduleUrl(resolveImports(ts.transpileModule(source("../src/lib/journal-reading.ts"), {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext },
}).outputText, { "$lib/journal-empty": emptyUrl, "$lib/journal-image": imageUrl }));
const reading = await import(readingUrl);
const messages = await import(messagesUrl);

// Compile and render the actual Svelte presentation components. No DOM, Editor,
// Tauri app or database is created; only the route's invoke bridge is doubled.
function componentUrl(path, local = {}, contents = source(path)) {
  const compiled = compile(contents, { filename: path, generate: "server" });
  assert.equal(compiled.warnings.length, 0);
  return moduleUrl(resolveImports(compiled.js.code, {
    "$lib/journal-reading": readingUrl,
    "$lib/paraglide/messages.js": messagesUrl,
    ...local,
  }));
}
// SSR does not run effects. Seed only the initial link in this presentation fixture;
// the asynchronous snapshot reader is exercised separately with its real handlers.
const snapshots = {
  'place-a': {place:{id:'saved-a',name:'Gamla eken',latitude:1,longitude:2,placeRevision:'1'},revision:'1'},
  'place-b': {place:{id:'saved-b',name:'Långt platsnamn '.repeat(20),latitude:null,longitude:null,placeRevision:'2'},revision:'3'},
};
const placesUrl = moduleUrl(resolveImports(ts.transpileModule(source('../src/lib/places.ts'), {
  compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext},
}).outputText, {'$lib/journal-data':moduleUrl('export function createJournalReader(){return async()=>{throw new Error("SSR must not read places")}}'),'@tauri-apps/api/core':moduleUrl('export async function invoke(){throw new Error("SSR must not read or write places")}')}));
const placeUrl = componentUrl('../src/lib/components/EntryPlace.svelte', {
 '$lib/places':placesUrl,
 '$lib/place-map.svelte':moduleUrl('export const PLACE_MAP="mno:place-map"'),
 '$lib/components/PlaceMap.svelte':moduleUrl('export default function(){throw new Error("Read-only must not render map")}'),
}, source('../src/lib/components/EntryPlace.svelte').replace('$state<JournalPlace>(emptyLink())', `$state<JournalPlace>(untrack(() => (${JSON.stringify(snapshots)})[entryId] ?? emptyLink()))`));
const photographUrl = componentUrl("../src/lib/components/JournalPhotograph.svelte", { "$lib/journal-photographs": photographsUrl, "@tauri-apps/api/core": moduleUrl('export async function invoke(){throw new Error("SSR must not read photographs")}') });
const contentUrl = componentUrl("../src/lib/components/JournalContent.svelte", { "$lib/components/JournalPhotograph.svelte": photographUrl });
const { default: JournalReading } = await import(componentUrl("../src/lib/components/JournalReading.svelte", {
  "$lib/components/DeleteEntryDialog.svelte": componentUrl("../src/lib/components/DeleteEntryDialog.svelte"),
  "$lib/components/EntryOptions.svelte": componentUrl("../src/lib/components/EntryOptions.svelte"),
  "$lib/components/EntryPlace.svelte": placeUrl,
  "$lib/components/JournalContent.svelte": contentUrl,
  "$lib/components/WeatherTile.svelte": tileUrl,
  "$lib/weather": weatherUrl,
}));
const show = props => render(JournalReading, { props: {
  entries: [], isLoading: false, errorMessage: "", locale: "sv", onEntryDeleted() {}, ...props,
} }).body.replace(/<!--[\s\S]*?-->/g, "");
const text = value => ({ type: "text", text: value });
const paragraph = value => ({ type: "paragraph", content: [text(value)] });
const document = content => JSON.stringify({ type: "doc", content });
const entry = (id, date, content = [paragraph(id)]) => ({
  id, entryDate: date, title: id, contentJson: document(content), editable: true, documentVersion: 1,
});

async function routeHarness(response, generation = 1, { url = "https://example.invalid/journal", holdTicks = false } = {}) {
  const calls = [];
  let mount;
  let afterNavigation;
  let effectCleanup;
  let effect;
  const scrolls = [];
  const ticks = [];
  const pageState = { url: new URL(url) };
  const context = vm.createContext({
    $state: value => value,
    $effect: callback => { effect = callback; },
    untrack: callback => callback(),
    page: pageState,
    afterNavigate: callback => { afterNavigation = callback; },
    resolveJournalTarget: reading.resolveJournalTarget,
    document: { getElementById: id => ({ scrollIntoView: options => { scrolls.push({ id, options }); } }) },
    tick: () => holdTicks ? new Promise(resolve => ticks.push(resolve)) : Promise.resolve(),
    onMount: callback => { mount = callback; },
    language: { current: "sv" },
    m: messages,
    invoke(command) {
      calls.push(command);
      if (command === "read_journal_entries") return response;
      if (command === "new_journal_session") return Promise.resolve({ generation, data: null });
      throw new Error(`Forbidden command: ${command}`);
    },
    setTimeout() { throw new Error("Reading must not schedule autosave"); },
  });
  const transpile = code => ts.transpileModule(code, {
    compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None },
  }).outputText;
  const reader = source("../src/lib/journal-data.ts").replace(/^import .*;\n/gm, "").replace(/^export /gm, "");
  const page = source("../src/routes/journal/+page.svelte")
    .match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, "");
  vm.runInContext(transpile(reader), context);
  vm.runInContext(transpile(page), context);
  const unmount = mount();
  afterNavigation();
  const drain = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
  const runEffect = () => { effectCleanup?.(); effectCleanup = effect(); };
  const flush = async () => { await drain(); runEffect(); await drain(); };
  const dispose = () => { effectCleanup?.(); unmount(); };
  await flush();
  return {
    calls, dispose, flush, scrolls,
    async navigate(url) {
      pageState.url = new URL(url, pageState.url);
      afterNavigation();
      runEffect();
      await drain();
    },
    async render() { const pending = ticks.splice(0); pending.forEach(resolve => resolve()); await drain(); },
    message: () => vm.runInContext("navigationMessage", context),
    props: () => vm.runInContext('({ entries, isLoading, errorMessage })', context),
  };
}

test("newest date groups render first while same-day order and source data stay unchanged", () => {
  const entries = [
    entry("old", "2020-01-01"),
    entry("a", "2026-10-21"), entry("b", "2026-10-21"),
    entry("c", "2027-01-01"), entry("d", "2027-01-01"),
  ];
  const before = JSON.stringify(entries);
  const groups = reading.groupJournalEntries(entries);
  assert.deepEqual(groups.map(group => group.entries.map(e => e.id)), [["c", "d"], ["a", "b"], ["old"]]);
  assert.deepEqual(reading.groupJournalEntries(entries), groups, "repeated grouping must stay stable");
  const html = show({ entries });
  assert.ok(html.indexOf('id="journal-entry-c"') < html.indexOf('id="journal-entry-d"'));
  assert.ok(html.indexOf('id="journal-entry-d"') < html.indexOf('id="journal-entry-a"'));
  assert.ok(html.indexOf('id="journal-entry-b"') < html.indexOf('id="journal-entry-old"'));
  assert.ok(html.indexOf('id="journal-entry-a"') < html.indexOf('id="journal-entry-b"'));
  assert.equal((html.match(/datetime="2026-10-21"/g) ?? []).length, 1);
  assert.equal(JSON.stringify(entries), before);
});

test("full rich text renders headings, nested marks, lists and escaped text without an editor", () => {
  const entries = [entry("rich", "2026-10-21", [
    { type: "heading", attrs: { level: 2 }, content: [text("Rubrik")] },
    { type: "paragraph", content: [
      { ...text("Fet kursiv"), marks: [{ type: "bold" }, { type: "italic" }] },
      { type: "hardBreak" }, text('<img src=x onerror="danger()">'),
      { ...text("Länk"), marks: [{ type: "link", attrs: { href: "https://example.invalid/?a=1&b=2" } }] },
    ] },
    { type: "bulletList", content: [{ type: "listItem", content: [paragraph("Punkt")] }] },
    { type: "orderedList", attrs: { start: 3 }, content: [{ type: "listItem", content: [paragraph("Nummer")] }] },
    { type: "blockquote", content: [paragraph("Citat")] },
    { type: "codeBlock", content: [text("<kod>")] },
    { type: "horizontalRule" },
  ])];
  const before = entries[0].contentJson;
  const html = show({ entries });
  assert.match(html, /<h2[^>]*>Rubrik<\/h2>/);
  assert.match(html, /<strong><em>Fet kursiv<\/em><\/strong>/);
  assert.match(html, /<ul>.*<li>.*Punkt/s);
  assert.match(html, /<ol start="3">.*Nummer/s);
  assert.match(html, /<blockquote>.*Citat/s);
  assert.match(html, /<pre><code>&lt;kod>/);
  assert.match(html, /<br/);
  assert.match(html, /<hr/);
  assert.match(html, /&lt;img/);
  assert.doesNotMatch(html, /<img|contenteditable|<textarea|<script/);
  assert.match(html, /href="https:\/\/example.invalid/);
  assert.equal(entries[0].contentJson, before);
});

test("unsafe links cannot become executable HTML", () => {
  for (const href of ["javascript:alert(1)", "data:text/html,danger", "\u0000javascript:alert(1)"]) {
    assert.equal(reading.safeJournalHref(href), null);
    const html = show({ entries: [entry("safe", "2026-10-21", [{ type: "paragraph", content: [
      { ...text("Readable link"), marks: [{ type: "link", attrs: { href } }] },
    ] }])] });
    assert.match(html, /Readable link/);
    assert.doesNotMatch(html, /href="(?:javascript|data):/);
  }
});

test("invalid or unsupported content gives a local error while other entries remain readable", () => {
  for (const contentJson of ["invalid JSON", "{}", document([{ type: "unknown" }]),
    document([{ type: "heading", attrs: { level: 99 }, content: [text("Invalid")] }])]) {
    const entries = [{ ...entry("bad", "2026-10-21"), contentJson }, entry("good", "2026-10-21")];
    const html = show({ entries });
    assert.match(html, /role="alert"/);
    assert.match(html, /id="journal-entry-good"/);
    assert.match(html, /<p[^>]*>good<\/p>/);
    assert.equal(entries[0].contentJson, contentJson);
  }
});

test("loading, empty journal and read failure are distinct states", () => {
  assert.match(show({ isLoading: true }), /role="status"/);
  assert.doesNotMatch(show({ isLoading: true }), /första berättelse/);
  assert.match(show({}), /första berättelse/);
  const failed = show({ errorMessage: "Läsfel" });
  assert.match(failed, /role="alert"/);
  assert.match(failed, /Läsfel/);
  assert.doesNotMatch(failed, /första berättelse/);
});

test("each entry has a stable ID anchor and a native detail link and closed options above the content", () => {
  const html = show({ entries: [entry("A", "2026-10-21"), entry("B", "2026-10-21")] });
  for (const id of ["A", "B"]) {
    assert.match(html, new RegExp(`id="journal-entry-${id}"`));
    const article = html.match(new RegExp(`<article id="journal-entry-${id}"[\\s\\S]*?</article>`))[0];
    assert.ok(article.indexOf('class="journal-text') > article.indexOf('aria-label="Anteckningsalternativ"'));
    assert.match(article, /aria-label="Anteckningsalternativ" aria-expanded="false"/);
    assert.match(article, /class="actions[^"]*" hidden/);
    assert.match(html, new RegExp(`class="entry-open[^"]*" href="/journal/entries/${id}"`));
    assert.match(html, new RegExp(`href="/journal/entries/${id}\\?edit=true&amp;from=journal"`));
  }
});

test("photo-only entries retain a keyboard-accessible native detail link", () => {
  const photo = { type: "journalImage", attrs: {
    nodeId: "d581b905-b692-498d-8955-830251b82825",
    assetId: "b7d05577-9248-49b7-af7b-2a240ee52bd7",
    placement: "row", size: "large",
  } };
  const html = show({ entries: [{ ...entry("photo", "2026-10-21", [photo]), documentVersion: 2 }], locale: "en" });
  assert.match(html, /<a class="entry-open[^"]*" href="\/journal\/entries\/photo">View entry/);
  assert.match(html, /<figure/);
});

test("entry clicks preserve selection, photos, inline links and menu interactions", () => {
  const script = source("../src/lib/components/JournalReading.svelte")
    .split("  function openEntryFromClick")[1].split("  $effect")[0];
  let opens = 0;
  let selected = false;
  class Element {
    constructor(selector) { this.selector = selector; }
    closest(selectors) { return selectors.split(", ").includes(this.selector) ? this : null; }
  }
  const article = { querySelector: selector => {
    assert.equal(selector, ".entry-open");
    return { click: () => { opens++; } };
  } };
  const window = { getSelection: () => ({
    isCollapsed: !selected,
    containsNode: node => node === article,
  }) };
  const handler = vm.runInNewContext(ts.transpileModule(
    `function openEntryFromClick${script}
openEntryFromClick;`,
    { compilerOptions: { target: ts.ScriptTarget.ESNext } },
  ).outputText, { Element, window });
  const click = (selector, overrides = {}) => handler({
    button: 0, currentTarget: article, target: new Element(selector), ...overrides,
  });
  for (const selector of ["a", "button", ".mno-photograph", ".entry-metadata", ".entry-options"]) click(selector);
  click("p", { metaKey: true });
  click("p", { defaultPrevented: true });
  selected = true;
  click("p");
  assert.equal(opens, 0);
  selected = false;
  click("p");
  assert.equal(opens, 1);
});

test("reading menus offer Delete only for editable entries with a loaded generation", () => {
  const saved = entry("saved", "2026-10-21");
  assert.match(show({ entries: [saved], generation: 7, locale: "en" }), /Delete entry/);
  assert.doesNotMatch(show({ entries: [saved], locale: "en" }), /Delete entry/);
  assert.doesNotMatch(show({ entries: [{ ...saved, editable: false }], generation: 7, locale: "en" }), /Delete entry/);
});

test("shared confirmation identifies the selected entry and focuses Cancel first", async () => {
  const path = "../src/lib/components/DeleteEntryDialog.svelte";
  const component = await import(componentUrl(path));
  const html = render(component.default, { props: {
    title: "A lake walk", date: "2026-10-21", locale: "en", busy: false,
    oncancel() {}, onconfirm() {},
  } }).body.replace(/<!--[\s\S]*?-->/g, "");
  assert.match(html, /Permanently delete “A lake walk” \(2026-10-21\)\? Original photo files will be kept[.]/);
  assert.match(html.match(/<button[^>]*>[^<]*<\/button>/)[0], />Cancel</);
  const script = source(path).match(/  function mountDialog[\s\S]*?\n  }/)[0];
  const focused = [];
  class HTMLElement {
    isConnected = true;
    focus() { focused.push("trigger"); }
  }
  const mount = vm.runInNewContext(ts.transpileModule(
    `${script}\nmountDialog;`, { compilerOptions: { target: ts.ScriptTarget.ESNext } },
  ).outputText, { document: { activeElement: new HTMLElement() }, HTMLElement });
  const lifecycle = mount({
    showModal: () => focused.push("modal"),
    querySelector: selector => {
      assert.equal(selector, "button");
      return { focus: () => focused.push("Cancel") };
    },
    close: () => focused.push("closed"),
  });
  lifecycle.destroy();
  assert.deepEqual(focused, ["modal", "Cancel", "closed", "trigger"]);
});

test("reading deletion cancels safely, reports failure on the selected entry, and commits only that entry", async () => {
  const component = source("../src/lib/components/JournalReading.svelte");
  const script = component.slice(component.indexOf("  let deleteTarget"), component.indexOf("  function openEntryFromClick"));
  const first = entry("first", "2026-10-22");
  const selected = entry("selected", "2026-10-21");
  const next = entry("next", "2026-10-20");
  const calls = [], effects = [];
  let fail = true;
  let complete;
  const context = vm.createContext({
    $state: value => value,
    groups: [{ entries: [first, selected, next] }],
    generation: 7, first, selected, next,
    closeAndFocus: () => effects.push("trigger"),
    invoke(command, args) {
      calls.push([command, { ...args }]);
      if (fail) return Promise.reject(new Error("synthetic deletion failure"));
      return new Promise(resolve => { complete = resolve; });
    },
    onEntryDeleted: id => effects.push(id),
    window: { dispatchEvent: event => effects.push(event.type) },
    tick: async () => {},
    Event,
    journalEntryAnchor: reading.journalEntryAnchor,
    document: { getElementById: id => ({ querySelector: () => ({ focus: () => effects.push(id) }) }) },
  });
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  const run = code => vm.runInContext(code, context);
  run("requestDelete(selected, closeAndFocus); cancelDelete()");
  await run("confirmDelete()");
  assert.deepEqual(calls, []);

  run("requestDelete(selected, closeAndFocus)");
  await run("confirmDelete()");
  assert.equal(run("deleteFailedId"), "selected");
  assert.deepEqual(effects, ["trigger", "trigger"]);

  fail = false;
  run("requestDelete(selected, closeAndFocus); requestDelete(first, closeAndFocus)");
  const deletion = run("confirmDelete()");
  await run("confirmDelete()");
  run("cancelDelete()");
  assert.equal(run("deleteTarget.id"), "selected");
  assert.deepEqual(effects, ["trigger", "trigger", "trigger"]);
  complete();
  await deletion;
  assert.deepEqual(calls, [
    ["delete_journal_entry", { id: "selected", generation: 7 }],
    ["delete_journal_entry", { id: "selected", generation: 7 }],
  ]);
  assert.deepEqual(effects, ["trigger", "trigger", "trigger", "selected", "mno:journal-changed", "journal-entry-next"]);
});

test("entry options leave Tab available and Escape closes and focuses the trigger", () => {
  const options = source("../src/lib/components/EntryOptions.svelte");
  const script = options.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, "");
  const listeners = new Map();
  let startEffect;
  let focusCount = 0;
  const Node = class {};
  const item = new Node();
  const button = Object.assign(new Node(), {
    focus: () => { focusCount++; },
    parentElement: { contains: target => target === item || target === button },
  });
  const props = Object.assign(() => ({ locale: "en", children() {} }), { id: () => "fixture" });
  const context = vm.createContext({
    $props: props,
    $state: value => value,
    $effect: callback => { startEffect = callback; },
    Node, button,
    document: {
      addEventListener: (name, handler) => listeners.set(name, handler),
      removeEventListener: name => listeners.delete(name),
    },
  });
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  const run = code => vm.runInContext(code, context);
  assert.match(options, /<button[\s\S]*?onclick=\{toggle\}/);
  run("trigger = button; toggle()");
  assert.equal(run("open"), true);
  const stop = startEffect();
  let prevented = 0;
  const key = value => listeners.get("keydown")({
    key: value, target: item,
    preventDefault: () => { prevented++; },
    stopPropagation() {},
  });
  key("Tab");
  assert.equal(prevented, 0);
  assert.equal(run("open"), true);
  key("Escape");
  assert.equal(prevented, 1);
  assert.equal(run("open"), false);
  assert.equal(focusCount, 2, "opening and Escape focus the trigger");
  stop();
  assert.equal(listeners.size, 0);
});

test("actual page reads once through generation protection and never invokes saving", async () => {
  const entries = [entry("A", "2026-10-21")];
  const h = await routeHarness(Promise.resolve({ generation: 1, data: entries }));
  assert.deepEqual(h.calls, ["read_journal_entries", "new_journal_session"]);
  assert.equal(h.props().entries, entries);
  assert.equal(h.props().isLoading, false);
  assert.equal(h.props().errorMessage, "");
  assert.match(show(h.props()), /id="journal-entry-A"/);
  h.dispose();
});

test("read errors and stale generations do not write or display an empty journal as success", async () => {
  for (const [response, generation] of [
    [Promise.reject(new Error("isolated read failure")), 1],
    [Promise.resolve({ generation: 1, data: [entry("stale", "2026-10-21")] }), 2],
  ]) {
    const h = await routeHarness(response, generation);
    assert.equal(h.props().isLoading, false);
    assert.ok(h.props().errorMessage);
    assert.equal(h.props().entries.length, 0);
    assert.match(show(h.props()), /role="alert"/);
    assert.ok(h.calls.every(command => ["read_journal_entries", "new_journal_session"].includes(command)));
    h.dispose();
  }
});

test("a late response after page teardown cannot replace page state", async () => {
  let resolve;
  const h = await routeHarness(new Promise(done => { resolve = done; }));
  h.dispose();
  resolve({ generation: 1, data: [entry("late", "2026-10-21")] });
  await h.flush();
  assert.equal(h.props().entries.length, 0);
  assert.equal(h.props().isLoading, true);
  assert.ok(!h.calls.includes("save_journal_entry"));
});


test("only definitely empty posts and their date headings disappear, without changing input", () => {
  const entries = [
    { ...entry("empty", "2026-08-06"), contentJson: document([{ type: "paragraph" }]) },
    entry("visible", "2026-10-21"),
    { ...entry("image", "2026-10-22"), contentJson: document([{ type: "image", attrs: { src: "photo.jpg" } }]) },
    { ...entry("unknown", "2026-10-22"), contentJson: document([{ type: "future" }]) },
    { ...entry("broken", "2026-10-22"), contentJson: "not JSON" },
  ];
  const before = JSON.stringify(entries);
  const html = show({ entries });
  assert.doesNotMatch(html, /journal-entry-empty|datetime="2026-08-06"/);
  for (const id of ["visible", "image", "unknown", "broken"]) assert.ok(html.includes(`journal-entry-${id}`));
  assert.match(html, /role="alert"/);
  assert.equal(JSON.stringify(entries), before);
});

test("a journal containing only definitely empty posts shows the empty state", () => {
  const html = show({ entries: [{ ...entry("empty", "2026-08-06"), contentJson: document([{ type: "paragraph" }]) }] });
  assert.match(html, /första berättelse/);
  assert.doesNotMatch(html, /<article|datetime=/);
});

test("targets resolve visible IDs, valid dates and latest with stable same-day ties", () => {
  const entries = [entry("old", "2020-01-01"), entry("a", "2026-10-21"), entry("b", "2026-10-21"),
    { ...entry("empty", "2027-01-01"), contentJson: document([{ type: "paragraph" }]) }];
  const target = query => reading.resolveJournalTarget(entries, new URLSearchParams(query));
  assert.deepEqual(target("entry=a"), { anchor: "journal-entry-a", missing: null });
  assert.deepEqual(target("date=2026-10-21"), { anchor: "journal-date-2026-10-21", missing: null });
  assert.deepEqual(target("latest=true"), { anchor: "journal-date-2026-10-21", missing: null });
  assert.deepEqual(target("entry=empty"), { anchor: null, missing: "entry" });
  assert.deepEqual(target("date=2027-01-01"), { anchor: null, missing: "date" });
  assert.equal(target("entry=absent&date=2026-10-21&latest=true").missing, "entry");
  for (const date of ["2026-02-30", "bad", "", "2020-01-02"]) {
    assert.equal(target(`date=${date}`).missing, "date");
  }
  assert.deepEqual(reading.resolveJournalTarget([], new URLSearchParams("latest=true")), { anchor: null, missing: "latest" });
  assert.deepEqual(target("latest=false"), { anchor: null, missing: null });
  assert.match(show({ entries }), /id="journal-date-2026-10-21"/);
});

test("scroll waits for asynchronous reading and rendering and uses only the newest URL", async () => {
  let resolve;
  const h = await routeHarness(new Promise(done => { resolve = done; }), 1,
    { url: "https://example.invalid/journal?entry=a", holdTicks: true });
  assert.equal(h.scrolls.length, 0);
  await h.navigate("?entry=b");
  assert.equal(h.scrolls.length, 0);
  resolve({ generation: 1, data: [entry("a", "2026-10-21"), entry("b", "2026-10-21")] });
  await h.flush();
  assert.equal(h.scrolls.length, 0, "DOM tick must complete before scrolling");
  await h.navigate("?date=2026-10-21");
  await h.render();
  assert.deepEqual(h.scrolls.map(s => s.id), ["journal-date-2026-10-21"]);
  assert.ok(h.calls.every(c => ["read_journal_entries", "new_journal_session"].includes(c)));
  h.dispose();
});

test("same-page targets scroll without reloading and cancel old targets", async () => {
  const h = await routeHarness(Promise.resolve({ generation: 1, data: [entry("a", "2026-10-21"), entry("b", "2026-10-21")] }), 1,
    { url: "https://example.invalid/journal?entry=a", holdTicks: true });
  await h.navigate("?entry=b");
  await h.render();
  assert.deepEqual(h.scrolls.map(s => s.id), ["journal-entry-b"]);
  await h.navigate("?latest=true");
  await h.render();
  assert.deepEqual(h.scrolls.map(s => s.id), ["journal-entry-b", "journal-date-2026-10-21"]);
  await h.navigate("?entry=a");
  await h.navigate("/journal");
  await h.render();
  assert.equal(h.scrolls.length, 2);
  assert.deepEqual(h.calls, ["read_journal_entries", "new_journal_session"]);
  h.dispose();
});

test("missing targets explain the problem without scrolling or writing", async () => {
  const h = await routeHarness(Promise.resolve({ generation: 1, data: [entry("a", "2026-10-21")] }), 1,
    { url: "https://example.invalid/journal?entry=absent" });
  assert.match(h.message(), /Anteckningen/);
  assert.equal(h.scrolls.length, 0);
  await h.navigate("?date=2026-02-30");
  assert.match(h.message(), /datumet/);
  await h.navigate("?entry=a");
  assert.equal(h.message(), "");
  assert.equal(h.scrolls[0].id, "journal-entry-a");
  h.dispose();
});

test("teardown and read failure prevent pending scrolling", async () => {
  const h = await routeHarness(Promise.resolve({ generation: 1, data: [entry("a", "2026-10-21")] }), 1,
    { url: "https://example.invalid/journal?entry=a", holdTicks: true });
  h.dispose();
  await h.render();
  assert.equal(h.scrolls.length, 0);
  const failed = await routeHarness(Promise.reject(new Error("read failed")), 1,
    { url: "https://example.invalid/journal?entry=a" });
  assert.equal(failed.scrolls.length, 0);
  assert.equal(failed.message(), "");
  failed.dispose();
});

test("latest opens the newest visible day including its heading, not the last-created article within it", async () => {
  const entries = [
    entry("older-day", "2026-10-16"),
    entry("first-on-newest-day", "2026-10-17", [paragraph("Long first entry ".repeat(100))]),
    entry("last-created-on-newest-day", "2026-10-17"),
    { ...entry("hidden-newer-day", "2026-10-18"), contentJson: document([{ type: "paragraph" }]) },
  ];
  const before = JSON.stringify(entries);
  const target = reading.resolveJournalTarget(entries, new URLSearchParams("latest=true"));
  assert.equal(target.anchor, "journal-date-2026-10-17");
  const html = show({ entries });
  const group = html.indexOf(`id="${target.anchor}"`);
  const date = html.indexOf('datetime="2026-10-17"');
  const first = html.indexOf('id="journal-entry-first-on-newest-day"');
  const last = html.indexOf('id="journal-entry-last-created-on-newest-day"');
  assert.ok(group < date && date < first && first < last);
  assert.equal(JSON.stringify(entries), before);

  let resolve;
  const h = await routeHarness(new Promise(done => { resolve = done; }), 1,
    { url: "https://example.invalid/journal?latest=true", holdTicks: true });
  assert.equal(h.scrolls.length, 0);
  resolve({ generation: 1, data: entries });
  await h.flush();
  assert.equal(h.scrolls.length, 0);
  await h.render();
  assert.equal(h.scrolls[0].id, "journal-date-2026-10-17");
  assert.equal(h.scrolls[0].options.block, "start");
  assert.equal(h.scrolls[0].options.behavior, "instant");
  // A later navigation completion must reapply the target after Kit's reset.
  await h.navigate("?latest=true");
  await h.render();
  assert.equal(h.scrolls.at(-1).id, "journal-date-2026-10-17");
  await h.navigate("?entry=last-created-on-newest-day");
  await h.render();
  assert.equal(h.scrolls.at(-1).id, "journal-entry-last-created-on-newest-day");
  await h.navigate("?date=2026-10-16");
  await h.render();
  assert.equal(h.scrolls.at(-1).id, "journal-date-2026-10-16");
  h.dispose();
});


test('protected documents warn that reading may be incomplete and offer no edit action', () => {
  const protectedEntry = {...entry('protected', '2026-10-25', [{type:'paragraph',attrs:{future:'keep'},content:[text('Visible text')]}]), editable:false};
  const html = show({entries:[protectedEntry]});
  assert.match(html, /Visible text/);
  assert.match(html, /kan vara ofullständig/);
  assert.doesNotMatch(html, /edit=true/);
});

test('reading metadata shows each saved snapshot next to weather under the shared day, in both languages',()=>{
 for(const locale of ['sv','en']){
  const html=show({entries:[entry('place-a','2026-10-30'),entry('place-b','2026-10-30')],locale,generation:3});
  assert.match(html,/Gamla eken/);assert.match(html,/Långt platsnamn/);
  assert.equal((html.match(/class="entry-date /g)??[]).length,1);
  assert.equal((html.match(/class="entry-metadata /g)??[]).length,2);
  const first=html.slice(html.indexOf('id="journal-entry-place-a"'),html.indexOf('id="journal-entry-place-b"'));
  assert.match(first,/Gamla eken/);assert.doesNotMatch(first,/Långt platsnamn/);
  assert.ok(first.indexOf('place-read')<first.indexOf('weather-tile'));
  assert.doesNotMatch(html,/place-trigger|navigation-map|Välj plats|Choose place/);
 }
 const view=source('../src/lib/components/JournalReading.svelte');
 assert.match(view,/EntryPlace entryId=\{entry.id\} \{generation\} editing=\{false\} \{locale\}/);
 assert.match(view,/flex-wrap: wrap/);assert.match(view,/max-width: 100%/);
 assert.match(source('../src/lib/components/EntryPlace.svelte'),/overflow-wrap: anywhere/);
});
test('entries without a place render no empty place wrapper, icon or placeholder',()=>{
 for(const locale of ['sv','en']){
  const html=show({entries:[entry('no-place','2026-10-30')],locale,generation:3});
  assert.doesNotMatch(html,/entry-place|place-read|⌖|Välj plats|Choose place/);
 }
});

test("v2 reader renders standalone photograph, visible caption and independent alternative text", async () => {
  const attrs = {nodeId:"d581b905-b692-498d-8955-830251b82825",assetId:"b7d05577-9248-49b7-af7b-2a240ee52bd7",placement:"right",size:"small",caption:"Visible <caption>",alt:"Independent alternative"};
  const asset = {asset_id:attrs.assetId,entry_id:"entry",original_path:"/missing/original.png",mime:"image/png",width:80,height:40,preview:"aGVsbG8="};
  const photo = componentUrl("../src/lib/components/JournalPhotograph.svelte", { "$lib/journal-photographs": photographsUrl, "@tauri-apps/api/core": moduleUrl('export async function invoke(){throw new Error("SSR must not read photographs")}') }, source("../src/lib/components/JournalPhotograph.svelte").replace('$state<PhotographAsset | null>(null)', `$state<PhotographAsset | null>(${JSON.stringify(asset)})`));
  const contents = await import(componentUrl("../src/lib/components/JournalContent.svelte", { "$lib/components/JournalPhotograph.svelte": photo }));
  const contentJson = JSON.stringify({type:"doc",content:[{type:"journalImage",attrs}]});
  for (const locale of ["sv","en"]) {
    const html = render(contents.default, {props:{contentJson,documentVersion:2,entryId:"entry",generation:1,locale}}).body;
    assert.match(html,/data:image\/png;base64,aGVsbG8=/);
    assert.match(html,/alt="Independent alternative"/);
    assert.match(html,/Visible &lt;caption(?:&gt;|>)/);
    assert.doesNotMatch(html,/original\.png/);
  }
  assert.equal(reading.readJournalDocument(contentJson,1).document,null);
  assert.equal(reading.readJournalDocument(contentJson,3).document,null);
  assert.equal(reading.readJournalDocument(contentJson,2).document.content[0].attrs.placement,"right");
});


test("all photograph placements and sizes use the same presentation contract without editing controls", () => {
  for (const placement of ["left", "right", "row"]) for (const size of ["small", "medium", "large"]) {
    const photo = {type:"journalImage",attrs:{nodeId:"d581b905-b692-498d-8955-830251b82825",assetId:"b7d05577-9248-49b7-af7b-2a240ee52bd7",placement,size,caption:"Caption",alt:"Alternative"}};
    const html = show({entries:[{...entry("entry", "2026-10-31", [photo,paragraph("Prose")]),documentVersion:2}]});
    assert.ok(html.includes(`data-placement="${placement}"`));assert.ok(html.includes(`data-size="${size}"`));
    assert.ok(html.includes("<figcaption>Caption</figcaption>"));assert.ok(!html.includes("mno-photo-options"));
  }
});


test("v2 explicitly accepts Medium, retains old Small/Large attributes, and rejects unknown sizes", () => {
  for(const size of ["small","medium","large"]) {
    const node={type:"journalImage",attrs:{nodeId:"d581b905-b692-498d-8955-830251b82825",assetId:"b7d05577-9248-49b7-af7b-2a240ee52bd7",placement:"row",size}};
    const value={type:"doc",content:[node]};const json=JSON.stringify(value,null,2);
    assert.deepEqual(reading.readJournalDocument(json,2).document,value);
    assert.equal(reading.readJournalDocument(json,1).document,null);
    node.attrs.size="huge";assert.equal(reading.readJournalDocument(JSON.stringify(value),2).document,null);
  }
});
