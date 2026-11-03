import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { compile } from "svelte/compiler";
import { render } from "svelte/server";
const read = path => readFileSync(new URL(path, import.meta.url), "utf8");
const source = read("../src/routes/history/+page.svelte");
const nav = read("../src/lib/components/AppNavigation.svelte");
const entry = read("../src/routes/journal/entries/[id]/+page.svelte");
const script = source => ts.transpileModule(source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, ""), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;
const flush = async () => { for (let i = 0; i < 16; i++) await Promise.resolve(); };
const overview = { totalEntries: 54, years: [
  { year: 2026, count: 52, months: [{month:10,count:51},{month:2,count:1}] },
  { year: 2024, count: 1, months: [{month:2,count:1}] },
], undatedCount: 1 };
const plain = value => JSON.parse(JSON.stringify(value));
function harness(url = "https://mno.invalid/history") {
  const calls = [], readers = [], remembered = [];
  let mount, navigate, changed;
  const context = vm.createContext({ URL, $state: v => v, $derived: v => v,
    onMount: f => mount = f, afterNavigate: f => navigate = f,
    replaceState: (url, state) => { remembered.push(String(url)); context.page.url = url; assert.deepEqual(state, {}); },
    page: {url: new URL(url), state: {}}, language:{current:"sv"},
    createJournalReader: () => { const reader = readers.length; readers.push(reader); return (command,args) => new Promise((resolve,reject) => calls.push({reader,command,args,resolve,reject})); },
    window: { addEventListener: (_name,f) => changed = f, removeEventListener: () => {} },
  });
  vm.runInContext(script(source), context);
  const dispose = mount();
  return {calls, remembered, dispose, run: code => vm.runInContext(code, context), changed: () => changed(),
    navigate: url => {context.page.url = new URL(url, context.page.url); navigate();},
    async ready(data = overview) { calls[0].resolve({generation:1,data}); await flush(); },
  };
}
test("overview opens only the newest year and loads no month documents", async () => {
  const h = harness(); assert.equal(h.calls.length, 1); assert.equal(h.calls[0].command, "journal_history_overview");
  await h.ready(); assert.deepEqual(plain(h.run("openYears")), {2026:true});
  assert.equal(h.run("Object.values(buckets).every(b => !b.open && !b.loaded)"), true);
  assert.equal(h.calls.length, 1); h.dispose();
});
test("opening a month lazily loads once, closing and reopening retains its entries", async () => {
  const h = harness(); await h.ready(); h.run('toggleMonth("2026-10", true)');
  assert.equal(h.calls.length, 2); assert.deepEqual(plain(h.calls[1].args), {year:2026,month:10,offset:0});
  h.calls[1].resolve({data:{entries:[{id:"one"}],hasMore:false}}); await flush();
  h.run('toggleMonth("2026-10", false); toggleMonth("2026-10", true)');
  assert.equal(h.calls.length, 2); assert.equal(h.run('buckets["2026-10"].entries[0].id'), "one");
  assert.match(h.remembered[0], /history\?year=2026&month=10$/); h.dispose();
});
test("show more uses the accumulated offset and prevents duplicate requests", async () => {
  const h = harness(); await h.ready(); h.run('toggleMonth("2026-10", true)');
  h.calls[1].resolve({data:{entries:Array.from({length:50},(_,i)=>({id:String(i)})),hasMore:true}}); await flush();
  h.run('loadEntries("2026-10", true); loadEntries("2026-10", true)');
  assert.equal(h.calls.length, 3); assert.equal(h.calls[2].args.offset, 50);
  h.calls[2].resolve({data:{entries:[{id:"last"}],hasMore:false}}); await flush();
  assert.equal(h.run('buckets["2026-10"].entries.length'), 51);
  h.run('loadEntries("2026-10", true)'); assert.equal(h.calls.length, 3); h.dispose();
});
test("concurrent months use independent readers and closing invalidates a pending reply", async () => {
  const h = harness(); await h.ready(); h.run('toggleMonth("2026-10", true); toggleMonth("2024-2", true)');
  assert.notEqual(h.calls[1].reader, h.calls[2].reader);
  h.run('toggleMonth("2026-10", false)'); h.calls[1].resolve({data:{entries:[{id:"late"}],hasMore:false}});
  h.calls[2].resolve({data:{entries:[{id:"current"}],hasMore:false}}); await flush();
  assert.equal(h.run('buckets["2026-10"].entries.length'), 0);
  assert.equal(h.run('buckets["2024-2"].entries[0].id'), "current");
  h.run('toggleMonth("2026-10", true)'); assert.equal(h.calls.length, 4); h.dispose();
});
test("journal changes and disposal cannot admit late overview or month responses", async () => {
  const h = harness(); await h.ready(); h.run('toggleMonth("2026-10", true)'); h.changed();
  h.calls[2].resolve({data:{totalEntries:0,years:[],undatedCount:0}}); await flush();
  h.calls[1].resolve({data:{entries:[{id:"obsolete"}],hasMore:false}}); await flush();
  assert.equal(h.run("overview.totalEntries"), 0); assert.deepEqual(plain(h.run("buckets")), {});
  h.changed(); h.dispose(); h.calls[3].resolve({data:overview}); await flush(); assert.equal(h.run("overview"), null);
});
test("failed reads expose a retry and empty results remain safe", async () => {
  const h = harness(); h.calls[0].reject("blocked"); await flush(); assert.equal(h.run("failed"), true);
  h.run("loadOverview()"); h.calls[1].resolve({data:overview}); await flush();
  h.run('toggleMonth("2026-2", true)'); h.calls[2].reject("generation conflict"); await flush();
  assert.equal(h.run('buckets["2026-2"].failed'), true);
  h.run('loadEntries("2026-2")'); h.calls[3].resolve({data:{entries:[],hasMore:false}}); await flush();
  assert.equal(h.run('buckets["2026-2"].failed'), false); assert.equal(h.run('buckets["2026-2"].loaded'), true); h.dispose();
});
test("return URL opens the selected older year and month without loading other months", async () => {
  const h = harness("https://mno.invalid/history?year=2024&month=2"); await h.ready();
  assert.equal(h.run("openYears[2024]"), true); assert.equal(h.run('buckets["2024-2"].open'), true);
  assert.equal(h.calls.length, 2); assert.equal(h.calls[1].args.year, 2024);
  assert.equal(h.run('entryHref({id:"a/b"}, buckets["2024-2"])'), "/journal/entries/a%2Fb?from=history&year=2024&month=2"); h.dispose();
});
test("undated entries load separately and invalid URL selections do not fetch entries", async () => {
  const h = harness("https://mno.invalid/history?undated=true"); await h.ready();
  assert.deepEqual(plain(h.calls[1].args), {year:null,month:null,offset:0});
  assert.equal(h.run('entryHref({id:"x"}, buckets.undated)'), "/journal/entries/x?from=history&undated=true"); h.dispose();
  const bad = harness("https://mno.invalid/history?year=2024&month=13"); await bad.ready(); assert.equal(bad.calls.length, 1); bad.dispose();
});
function entryContext(query = "", date = "") {
  const context = vm.createContext({URL, $state:v=>v,$derived:v=>v, onMount(){},createJournalReader:()=>()=>{},
    page:{params:{id:"entry"},url:new URL(`https://mno.invalid/journal/entries/entry${query}`)},language:{current:"sv"},m:{history_invalid_date:()=>"Invalid date"}});
  vm.runInContext(script(entry.replace('let entryDate = $state("")', `let entryDate = $state(${JSON.stringify(date)})`).replace('let isLoading = $state(true)', 'let isLoading = $state(false)')), context);
  return code => vm.runInContext(code,context);
}
test("entry return links preserve only safe timeline periods and leave normal/search navigation intact", () => {
  assert.equal(entryContext("?from=history&year=2024&month=2")("backHref"), "/history?year=2024&month=2");
  assert.equal(entryContext("?from=history&undated=true")("backHref"), "/history?undated=true");
  assert.equal(entryContext("?from=history&year=https://other&month=1")("backHref"), "/history");
  assert.equal(entryContext()("backHref"), "/journal/entries");
  assert.equal(entryContext("?from=search&q=%C3%B6rn")("backHref"), "/search?q=%C3%B6rn");
});
test("strict date fallback rejects normalized invalid dates without changing the stored string", () => {
  for (const date of ["bad", "", "2025-02-29", "2024-02-30", "0000-01-01", "2026-13-01"]) {
    const run = entryContext("",date); assert.equal(run("validEntryDate"), false); assert.equal(run("entryDate"), date);
    assert.equal(run("formattedDate"), "Invalid date");
  }
  for (const date of ["2024-02-29", "0001-01-01", "9999-12-31"]) assert.equal(entryContext("",date)("validEntryDate"), true);
  assert.match(entry, /if isEditing && validEntryDate/);
  assert.match(entry, /onEditingChange/);
});
function visibilityHarness() {
  const calls=[],effects=[]; const context=vm.createContext({URL, $state:v=>v,$derived:v=>v,$effect:f=>effects.push(f),
    PLACE_MAP: 'mno:place-map', getContext: () => ({ close() {} }),
    createJournalReader:()=> (command,args)=>new Promise((resolve,reject)=>calls.push({command,args,resolve,reject})),
    page:{url:new URL("https://mno.invalid/")},afterNavigate(){},language:{current:"sv"},
    window:{addEventListener(){},removeEventListener(){}},clearTimeout(){}});
  vm.runInContext(script(nav),context);
  return {calls,effects,run:code=>vm.runInContext(code,context)};
}
test("navigation eligibility is refreshed on panel open and saved entries, hiding failures and stale replies", async () => {
  const h = visibilityHarness(); assert.equal(h.run("historyVisible"), false);
  h.run('activePanel="left"'); const effect = h.effects[3]; const cleanup = effect();
  assert.equal(h.calls[0].command, "journal_history_overview"); h.calls[0].resolve({data:{navigationVisible:true}}); await flush();
  assert.equal(h.run("historyVisible"), true); h.run("handleJournalChanged()");
  assert.equal(h.calls[1].command, "journal_history_overview"); h.calls[1].resolve({data:{navigationVisible:false}}); await flush();
  assert.equal(h.run("historyVisible"), false);
  h.run("refreshHistoryVisibility(); refreshHistoryVisibility()"); h.calls[4].resolve({data:{navigationVisible:true}}); await flush();
  h.calls[3].resolve({data:{navigationVisible:false}}); await flush(); assert.equal(h.run("historyVisible"), true);
  h.run("refreshHistoryVisibility()"); cleanup(); h.run('activePanel=null'); h.calls[5].resolve({data:{navigationVisible:true}}); await flush();
  assert.equal(h.run("historyVisible"), false);
  h.run('activePanel="left"; refreshHistoryVisibility()'); h.calls[6].reject("recovery"); await flush(); assert.equal(h.run("historyVisible"), false);
});
const moduleUrl = code => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const resolveImports = (code,local) => code.replace(/from (["'])([^"']+)\1/g,(_m,q,name)=>`from ${q}${local[name]??import.meta.resolve(name)}${q}`);
async function historyHtml(locale="sv", showEntries=false) {
  const buckets = Object.fromEntries([['2026-10',{}],['2026-2',{}],['2024-2',{}],['undated',{}]].map(([key])=>[key,{open:false,entries:[],loaded:false,loading:false,failed:false,hasMore:false}]));
  if (showEntries) Object.assign(buckets['2026-10'], {open:true,year:2026,month:10,loaded:true,hasMore:true,entries:[{id:'a/b',title:'<img src=x>',entryDate:'2026-10-26'}]});
  const seeded=source.replace('let overview = $state<Overview | null>(null)',`let overview = $state<Overview | null>(${JSON.stringify(overview)})`)
    .replace('let buckets = $state<Record<string, Bucket>>({})',`let buckets = $state<Record<string, Bucket>>(${JSON.stringify(buckets)})`)
    .replace('let openYears = $state<Record<number, boolean>>({})','let openYears = $state<Record<number, boolean>>({2026:true})')
    .replace('let loading = $state(true)','let loading = $state(false)');
  const compiled=compile(seeded,{filename:'History.svelte',generate:'server'});
  const local={'$app/state':moduleUrl('export const page={url:new URL("https://mno.invalid/history"),state:{}};'),
    '$app/navigation':moduleUrl('export const afterNavigate=()=>{}; export const replaceState=()=>{};'),
    '$lib/journal-data':moduleUrl('export const createJournalReader=()=>()=>{};'),
    '$lib/language.svelte':moduleUrl(`export const language={current:"${locale}"};`),
    '$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href};
  const component=await import(moduleUrl(resolveImports(compiled.js.code,local)));
  return render(component.default).body;
}
test("actual timeline markup exposes native keyboard controls, counts and closed months in both languages", async () => {
  for (const component of [source,nav,entry]) assert.equal(compile(component,{generate:'client'}).warnings.length,0);
  const html=await historyHtml();
  assert.match(html, /<details[^>]*open[^>]*><summary[^>]*><span[^>]*>2026/);
  assert.match(html, /<details(?![^>]*open)[^>]*><summary[^>]*><span[^>]*>2024/);
  assert.equal((html.match(/<details[^>]*open/g)||[]).length,1);
  assert.match(html,/51 anteckningar/); assert.match(html,/1 anteckning<\/span>/); assert.match(html,/Utan giltigt journaldatum/);
  assert.match(html, /href="\/"/); assert.match(await historyHtml("en"), /Without a valid journal date/);
  assert.match(nav,/if historyVisible/); assert.match(nav,/href="\/history"/);
  assert.doesNotMatch(source, /@html|edit=true/);
  for (const locale of ['sv','en']) {
    const messages=JSON.parse(read(`../messages/${locale}.json`));
    for (const key of ['title','intro','count','count_one','undated','invalid_date','back','loading','error','retry','more','empty','untitled']) assert.equal(typeof messages[`history_${key}`], 'string');
  }
});

test("actual expanded month markup renders safe reading links and a translated paging control", async () => {
  const html = await historyHtml("sv", true);
  assert.match(html, /href="\/journal\/entries\/a%2Fb\?from=history&amp;year=2026&amp;month=10"/);
  assert.match(html, /&lt;img src=x>/); assert.doesNotMatch(html, /<img /);
  assert.match(html, /Visa fler/); assert.match(await historyHtml("en",true), /Show more/);
});

test("back navigation restores a newly selected period and ignores unknown periods", async () => {
  const h = harness(); await h.ready();
  h.navigate("/history?year=2024&month=2"); assert.equal(h.calls.length, 2);
  assert.equal(h.run('buckets["2024-2"].open'), true);
  h.navigate("/history?year=1900&month=1"); assert.equal(h.calls.length, 2); h.dispose();
});

test("navigation visibility ignores replies after component disposal", async () => {
  const h = visibilityHarness(); h.run('activePanel="left"; refreshHistoryVisibility()');
  const dispose = h.effects[4](); dispose(); h.calls[0].resolve({data:{navigationVisible:true}}); await flush();
  assert.equal(h.run("historyVisible"), false);
});

test("the timeline and its navigation link can wrap and remain reachable in small windows", () => {
  assert.match(nav, /\.left-panel \{[^}]*box-sizing: border-box;[^}]*overflow-y: auto;[^}]*width: min\(360px, 100vw\)/);
  assert.match(source, /li a \{[^}]*flex-wrap: wrap;[^}]*overflow-wrap: anywhere;/);
  assert.match(source, /@media \(max-width: 600px\)/);
  assert.match(read('../src/routes/+layout.svelte'), /:global\(:focus-visible\)/);
});

test("timeline navigation reuses calendar heading typography without changing the page title", () => {
  assert.match(nav, /class="panel-section-heading history-link" href="\/history"/);
  const style = nav.match(/\.history-link \{([^}]+)\}/)[1];
  assert.match(style, /text-transform: uppercase/);
  assert.doesNotMatch(style, /font-|letter-spacing|color:/);
  assert.match(source, /<h1>\{m.history_title/);
  assert.equal(JSON.parse(read('../messages/sv.json')).history_title, 'Tidslinje');
  assert.equal(JSON.parse(read('../messages/en.json')).history_title, 'Timeline');
});
