import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { compile } from "svelte/compiler";

const placesPath = new URL("../src/lib/places.ts", import.meta.url);
const componentPath = new URL("../src/lib/components/EntryPlace.svelte", import.meta.url);
const mapPath = new URL("../src/lib/components/PlaceMap.svelte", import.meta.url);
const editorPath = new URL("../src/lib/components/JournalEditor.svelte", import.meta.url);
const routes = [
  new URL("../src/routes/journal/new/+page.svelte", import.meta.url),
  new URL("../src/routes/journal/entries/[id]/+page.svelte", import.meta.url),
];

function placesHarness(invoke) {
  const context = vm.createContext({ invoke });
  const reader = readFileSync(new URL('../src/lib/journal-data.ts',import.meta.url),'utf8').replace(/^import .*;\n/gm,'').replace(/^export /gm,'');
  vm.runInContext(ts.transpileModule(reader,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
  const source = readFileSync(placesPath, "utf8")
    .replace(/^import .*;\n/gm, "")
    .replace(/^export /gm, "")
    .replace(/^type [\s\S]*?^};\n/gm, "");
  vm.runInContext(ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None },
  }).outputText, context);
  return {
    filter: (places, query, locale) => vm.runInContext("filterPlaces", context)(places, query, locale),
    move: (current, length, direction) => vm.runInContext("movePlaceIndex", context)(current, length, direction),
    mapStart: (journal, fallback) => vm.runInContext("mapStartCoordinates", context)(journal, fallback),
    mapDraft: (lat, lon) => vm.runInContext("mapDraftCoordinates", context)(lat, lon),
    parse: (lat, lon) => vm.runInContext("parsePlaceCoordinates", context)(lat, lon),
    usage: (...args) => vm.runInContext("readPlaceUsage", context)(...args),
    list: (generation) => vm.runInContext("listPlaces", context)(generation),
    update: (...args) => vm.runInContext("updatePlace", context)(...args),
    delete: (...args) => vm.runInContext("deletePlace", context)(...args),
    set: (...args) => vm.runInContext("setJournalPlace", context)(...args),
  };
}

test("saved-place filtering is local, case-insensitive and keeps duplicate names", () => {
  const h = placesHarness(async () => { throw new Error("no IPC expected"); });
  const places = [
    { id: "1", name: "Ängen", latitude: 1, longitude: 2, revision: "1" },
    { id: "2", name: "ängen", latitude: 3, longitude: 4, revision: "1" },
    { id: "3", name: "Skogen", latitude: null, longitude: null, revision: "1" },
  ];
  assert.deepEqual(h.filter(places, "ÄNG", "sv").map((place) => place.id), ["1", "2"]);
  assert.equal(h.filter(places, "", "sv").length, 3);
});

test("keyboard result navigation wraps safely and has no selection for an empty result", () => {
  const h = placesHarness(async () => {});
  assert.equal(h.move(-1, 3, 1), 0);
  assert.equal(h.move(-1, 3, -1), 2);
  assert.equal(h.move(2, 3, 1), 0);
  assert.equal(h.move(0, 3, -1), 2);
  assert.equal(h.move(0, 0, 1), -1);
});

test("map start prefers the journal snapshot, then a future default, otherwise the world", () => {
  const h = placesHarness(async () => {});
  const journal = { latitude: 59.3, longitude: 18.1 };
  const fallback = { latitude: 57.7, longitude: 11.9 };
  assert.deepEqual({ ...h.mapStart(journal, fallback) }, journal);
  assert.deepEqual({ ...h.mapStart(null, fallback) }, fallback);
  assert.equal(h.mapStart(null, null), null);
});

test("map draft needs two explicit coordinates, including an explicit zero pair", () => {
  const h = placesHarness(async () => {});
  for (const pair of [["", ""], ["  ", " \t"], ["0", ""], ["", "0"], ["91", "0"]]) {
    assert.equal(h.mapDraft(...pair), null);
  }
  assert.deepEqual([...h.mapDraft("0", "0")], [0, 0]);
  assert.deepEqual([...h.mapDraft(" 59,3 ", "18,1")], [59.3, 18.1]);
});

test("coordinate input preserves zero, accepts Swedish decimals and requires a pair", () => {
  const h = placesHarness(async () => {});
  assert.deepEqual({ ...h.parse("", "") }, { latitude: null, longitude: null });
  assert.deepEqual({ ...h.parse("0", "0") }, { latitude: 0, longitude: 0 });
  assert.deepEqual({ ...h.parse("59,33", "18,07") }, { latitude: 59.33, longitude: 18.07 });
  assert.equal(h.parse("59", ""), null);
  assert.equal(h.parse("91", "18"), null);
  assert.equal(h.parse("NaN", "18"), null);
});

test("place adapter sends exact generations and revisions and rejects stale responses", async () => {
  const calls = [];
  const h = placesHarness(async (command, args) => {
    calls.push({ command, args });
    if (command === "list_places") return { generation: 7, data: [] };
    return { generation: 7, data: { place: null, revision: "9223372036854775807" } };
  });
  await h.list(7);
  const saved = await h.set(7, "entry", "9223372036854775806", null);
  assert.equal(saved.revision, "9223372036854775807");
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    { command: "list_places", args: { generation: 7 } },
    { command: "set_journal_place", args: { generation: 7, entryId: "entry", expectedRevision: "9223372036854775806", placeId: null } },
  ]);
  const stale = placesHarness(async () => ({ generation: 8, data: [] }));
  await assert.rejects(stale.list(7), (error) => error === "journal_generation_conflict");
});

test("place picker compiles and retains explicit create-then-link retry protection", () => {
  const source = readFileSync(componentPath, "utf8");
  compile(source, { filename: componentPath.pathname, generate: "client" });
  assert.match(source, /pending = \{ kind: "link", placeId \}/);
  assert.match(source, /if \(!entryId \|\| generation === null\)/);
  assert.match(source, /role="listbox"/);
  assert.match(source, /role="combobox"/);
  assert.match(source, /aria-activedescendant=/);
  assert.match(source, /aria-selected=/);
  assert.match(source, /link\.place\?\.name/);
  assert.match(source, /onclick=\{\(\) => selectPlace\(place\)\}/, "mouse selection writes immediately");
  assert.match(source, /event\.key === "Enter"[\s\S]*?selectPlace\(filteredPlaces\[activeIndex\]\)/, "Enter writes the focused result");
  assert.match(source, /if \(busy\) return false;[\s\S]*?const sequence = \+\+writeSequence/, "double writes are blocked");
  assert.match(source, /const saved = await setJournalPlace[\s\S]*?open = false;[\s\S]*?catch/, "the dialog closes only after a confirmed write");
  assert.match(source, /sequence !== writeSequence \|\| id !== entryId \|\| currentGeneration !== generation/, "late writes cannot publish a selection");
  assert.match(source, /place_revision_conflict[\s\S]*?readJournalPlace\(id, currentGeneration\)/, "a conflict refreshes the revision without discarding the pending choice");
  assert.doesNotMatch(source, /onclick=\{useSelected\}/);
});

test("Leaflet map is locally bundled and coordinates remain usable without tiles", () => {
  const source = readFileSync(mapPath, "utf8");
  compile(source, { filename: mapPath.pathname, generate: "client" });
  assert.match(source, /import\("leaflet"\)/);
  assert.match(source, /leaflet\/dist\/leaflet\.css/);
  assert.match(source, /tile\.openstreetmap\.org/);
  assert.match(source, /OpenStreetMap contributors/);
  assert.match(source, /on\("tileerror"/);
  assert.match(source, /map\.on\("click"[\s\S]*?publish\(event\.latlng\)/);
  assert.match(source, /draggable: true/);
  assert.match(source, /marker\.on\("dragend"/);
  assert.match(source, /circleMarker/, "saved places with coordinates are displayed");
  assert.match(source, /latitude;[\s\S]*?longitude;[\s\S]*?updateMarker\(\)/, "manual fields update the marker");
  assert.match(source, /initialCoordinates\(\)/);
  assert.match(source, /zoomSnap: 0\.1/, "world sizing can use a fractional zoom");
  assert.match(source, /mercatorLimit = 85\.0511287798/, "world bounds reach the Web Mercator latitude limit");
  assert.match(source, /map\.getBoundsZoom\(world\)/, "the entire initial world fits inside the measured map viewport");
  assert.match(source, /noWrap: true/, "the tile layer does not repeat the world around the fitted view");
  assert.match(source, /size.x <= 0 \|\| size.y <= 0/, "world zoom waits for a measurable map");
  assert.doesNotMatch(source, /map\.fitWorld\(|map\.fitBounds\(/, "a second fit cannot zoom the initial world back out");
  assert.match(source, /map\.invalidateSize\(\{ pan: false \}\)/, "Leaflet updates after the map element actually resizes");
  assert.match(source, /resizeObserver\?\.disconnect\(\)/, "size observation ends with the map");
  assert.doesNotMatch(source, /center:\s*\[62,\s*15\]/, "there is no country-specific fallback");
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.match(pkg.dependencies.leaflet, /^\^1\./);
  assert.match(pkg.devDependencies["@types/leaflet"], /^\^1\./);
});

test("place dialog keeps its header fixed and scrolls only its responsive content", () => {
  const source = readFileSync(componentPath, "utf8");
  assert.match(source, /class="panel-content"/);
  assert.match(source, /\.panel\{display:flex;flex-direction:column;[\s\S]*?overflow:hidden/);
  assert.match(source, /\.panel-content\{min-height:0;overflow:auto/);
  assert.match(source, /\.actions\{position:sticky/);
  assert.match(source, /defaultCenter=\{defaultCoordinates\}/);
  assert.match(source, /class:map-mode=\{mode === "new"\}/);
  assert.match(source, /\.panel\.map-mode \{ width: 95vw; height: 95vh/);
  assert.match(source, /grid-template-rows: minmax\(180px, 1fr\) auto auto auto/, "the map row receives remaining height");
  assert.match(source, /\.panel\.map-mode \.form :global\(\.map\) \{ flex: 1 1 auto; height: 100%/, "the Leaflet element fills its grid row");
  assert.match(source, /@media \(max-height: 560px\)[\s\S]*?min-height: 280px/, "short windows can scroll their content");
});

test("journal navigation and Done guard include place drafts and writes", () => {
  const source = readFileSync(editorPath, "utf8");
  assert.match(source, /!placeDirty && !placeBusy/);
  assert.match(source, /weatherBusy \|\| placeBusy/);
  assert.match(source, /if \(placeDirty\) \{ placeWarning = true; return; \}/);
  assert.match(source, /if \(placeDirty && entryId\)/, "an existing entry is guarded before its journal save");
  assert.match(source, /const saved = await saveEntry\(\);[\s\S]*?if \(placeDirty\)/, "place state is checked again after journal save");
});

test("both journal routes place date and saved snapshot in one metadata row", () => {
  for (const route of routes) {
    const source = readFileSync(route, "utf8");
    compile(source, { filename: route.pathname, generate: "client" });
    assert.match(source, /class="entry-metadata"/);
    assert.match(source, /<EntryPlace/);
    assert.match(source, /onDraft=/);
    assert.match(source, /onSessionChange=/);
  }
});

test("English and Swedish place messages remain complete", () => {
  for (const locale of ["en", "sv"]) {
    const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), "utf8"));
    for (const key of ["place_choose", "place_search", "place_save_use", "place_remove", "place_revision_conflict", "place_finish_first", "place_current", "place_waiting_for_entry"]) {
      assert.equal(typeof messages[key], "string");
      assert.ok(messages[key].length > 0);
    }
  }
});

test('registry adapters bind update revision and generation without journal writes', async () => {
 const calls=[],place={id:'p',name:'Old',latitude:1,longitude:2,revision:'9223372036854775806'};
 const h=placesHarness(async(command,args)=>{calls.push({command,args});return {generation:7,data:command==='delete_place'?null:{...place,name:args.name,revision:'9223372036854775807'}};});
 await h.update(7,place,'New',null,null);await h.delete(7,'p');
 assert.deepEqual(JSON.parse(JSON.stringify(calls)),[
  {command:'update_place',args:{generation:7,id:'p',expectedRevision:place.revision,name:'New',latitude:null,longitude:null}},
  {command:'delete_place',args:{generation:7,id:'p'}},
 ]);
 const stale=placesHarness(async()=>({generation:8,data:null}));
 await assert.rejects(stale.update(7,place,'New',1,2),e=>e==='journal_generation_conflict');
 await assert.rejects(stale.delete(7,'p'),e=>e==='journal_generation_conflict');
});

function readonlyPlaceHarness(reader) {
 const calls=[],effects=[];
 const forbidden=()=>{calls.push('forbidden');throw new Error('Read-only place must not write or open a map');};
 const c=vm.createContext({$props:()=>({entryId:'entry-one',generation:3,editing:false,locale:'sv'}),$state:v=>v,$derived:{by:f=>f()},$effect:f=>effects.push(f),
  getContext:()=>({open:forbidden}),PLACE_MAP:'mno:place-map',onDestroy(){},tick:async()=>{},untrack:f=>f(),
  filterPlaces:p=>p,createPlace:forbidden,listPlaces:forbidden,setJournalPlace:forbidden,
  readJournalPlace:async(...args)=>{calls.push(args);return reader(...args);},
  m:new Proxy({},{get:(_,key)=>()=>key}),
 });
 const script=readFileSync(componentPath,'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import[\s\S]*?;\n/gm,'');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 return {run:code=>vm.runInContext(code,c),calls,effects};
}
test('read-only place reads saved snapshot rather than renamed registry and never writes or opens map',async()=>{
 const saved={place:{id:'p',name:'Historical oak',latitude:0,longitude:0,placeRevision:'1'},revision:'2'};
 const h=readonlyPlaceHarness(async()=>saved);
 for(const effect of h.effects)effect();
 for(let i=0;i<20;i++)await Promise.resolve();
 assert.equal(h.run('link.place.name'),'Historical oak');assert.equal(h.run('link.place.placeRevision'),'1');
 await h.run('showPicker()');assert.deepEqual(h.calls,[['entry-one',3]]);assert.equal(h.run('open'),false);
 assert.deepEqual(saved.place,{id:'p',name:'Historical oak',latitude:0,longitude:0,placeRevision:'1'});
});
test('read-only place accepts no link or a tombstone without selecting a default',async()=>{
 for(const revision of ['0','4']){
  const h=readonlyPlaceHarness(async()=>({place:null,revision}));await h.run('loadLink(entryId,generation)');
  assert.equal(h.run('link.place'),null);assert.equal(h.run('link.revision'),revision);assert.deepEqual(h.calls,[['entry-one',3]]);
 }
});
test('read-only snapshot ignores late responses after entry or generation changes',async()=>{
 for(const change of ['entryId="entry-two"','generation=4']){
  let release;const h=readonlyPlaceHarness(()=>new Promise(r=>release=r));
  const pending=h.run('loadLink(entryId,generation)');h.run(change);
  release({place:{id:'p',name:'Stale',latitude:1,longitude:2,placeRevision:'1'},revision:'1'});
  await pending;assert.equal(h.run('link.place'),null);
 }
 let release,calls=0;
 const fresh={place:{id:'q',name:'Fresh',latitude:null,longitude:null,placeRevision:'2'},revision:'3'};
 const h=readonlyPlaceHarness(()=>++calls===1?new Promise(r=>release=r):Promise.resolve(fresh));
 const old=h.run('loadLink(entryId,generation)');await h.run('loadLink(entryId,generation)');
 release({place:{id:'p',name:'Stale',latitude:1,longitude:2,placeRevision:'1'},revision:'1'});await old;
 assert.equal(h.run('link.place.name'),'Fresh');
});

test('place usage adapter reads with generation recheck and never invokes a mutation',async()=>{
 const calls=[],data={placeId:'p',isWeatherDefault:true,totalCount:1,entries:[{id:'e',entryDate:'2026-10-30',title:'Title'}]};
 const h=placesHarness(async(command,args)=>{calls.push({command,args});return {generation:7,data:command==='read_place_usage'?data:null};});
 assert.deepEqual(await h.usage(7,'p'),data);
 assert.deepEqual(JSON.parse(JSON.stringify(calls)),[{command:'read_place_usage',args:{generation:7,placeId:'p'}},{command:'new_journal_session'}]);
 const stale=placesHarness(async command=>({generation:command==='read_place_usage'?7:8,data}));
 await assert.rejects(stale.usage(7,'p'),e=>e==='journal_generation_conflict');
});
