import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { compile } from 'svelte/compiler';
const read = name => readFileSync(new URL(`../src/lib/components/${name}.svelte`, import.meta.url), 'utf8');
const source = read('PlaceMapPanel');
function harness(reader = async () => ({ place: null }), props = {}, create = async () => ({ id: 'new', name: 'Lake', latitude: 0, longitude: 0, revision: '1' }), savedPlaces = [], mutations = {}) {
  let destroy;
  let closes = 0;
  const context = vm.createContext({ $state: v => v, $derived: v => v, filterPlaces: places => places, $props: () => ({ onclose() { closes++; }, ...props }),
    beforeNavigate() {}, goto: async()=>{}, formatJournalDate:value=>value, readPlaceUsage:async(_gen,id)=>({placeId:id,isWeatherDefault:false,totalCount:0,entries:[]}), parsePlaceCoordinates: (lat, lon) => { if (!lat.trim() && !lon.trim()) return {latitude:null,longitude:null}; const a=Number(lat), b=Number(lon); return lat.trim() && lon.trim() && Number.isFinite(a) && Number.isFinite(b) && Math.abs(a)<=90 && Math.abs(b)<=180 ? {latitude:a,longitude:b} : null; },
    tick: async () => {}, window: { innerWidth: 1000, innerHeight: 800 },
    createPlace: create, mapDraftCoordinates: (lat, lon) => lat && lon ? [Number(lat), Number(lon)] : null,
    listPlaces: async () => savedPlaces,
    onMount() {}, onDestroy: f => destroy = f,
    invoke: async () => ({ generation: 3 }), readWeatherDefaultPlace: reader, ...mutations });
  const script = source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  return { context, run: code => vm.runInContext(code, context), destroy: () => destroy(), closes: () => closes };
}
function journalHarness(id = 'entry', writer = async (_generation, _id, revision, placeId) => ({ place: placeId ? { id: placeId, name: 'Lake', latitude: 1, longitude: 2 } : null, revision: String(Number(revision) + 1) })) {
  let request;
  const writes = [];
  const effects = [];
  const state = value => value;
  const derived = { by: fn => fn() };
  const context = vm.createContext({
    $state: state, $derived: derived, $effect: fn => effects.push(fn),
    $props: () => ({ entryId: id, generation: 3, editing: true }),
    PLACE_MAP: 'mno:place-map', getContext: () => ({ open: value => { request = value; return () => {}; } }),
    onDestroy() {}, untrack: fn => fn(), tick: async () => {},
    filterPlaces: places => places,
    m: new Proxy({}, { get: (_, key) => () => key }),
    setJournalPlace: async (...args) => { writes.push(args); return writer(...args); },
    readJournalPlace: async () => ({ place: null, revision: '0' }),
  });
  const script = read('EntryPlace').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import[\s\S]*?;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  return { run: code => vm.runInContext(code, context), writes, request: () => request, effects };
}
const lake = { id: 'lake', name: 'Lake', latitude: 1, longitude: 2, revision: '1' };
function leafletHarness(savedPlaces) {
  class Element {
    parentElement = null;
    children = [];
    listeners = {};
    attributes = {};
    setAttribute(key, value) { this.attributes[key] = value; }
    addEventListener(name, handler) { this.listeners[name] = handler; }
    append(child) { child.parentElement = this; this.children.push(child); }
    querySelector() { return this.children[0]; }
    closest() { return null; }
    focus() {}
  }
  const root = new Element(), selected = [], contexts = [], points = [];
  let popup;
  function layer(coordinates) {
    const element = new Element(); element.parentElement = root;
    const point = { lat: coordinates[0], lng: coordinates[1] };
    const result = {
      handlers: {}, element, getElement: () => element, getLatLng: () => point,
      on(name, handler) { this.handlers[name] = handler; return this; },
      addTo() { return this; }, bindTooltip() { return this; },
      setLatLng(pair) { point.lat = pair[0]; point.lng = pair[1]; },
    };
    return result;
  }
  const nativeMap = { closePopup() {}, getZoom: () => 12, setView() {}, mouseEventToLatLng: () => ({ lat: 6, lng: 7 }) };
  const leaflet = {
    circleMarker: pair => { const point = layer(pair); points.push(point); return point; },
    marker: pair => layer(pair), divIcon() {},
    DomEvent: { disableClickPropagation() {} },
    popup: () => ({ setLatLng() { return this; }, setContent(content) { popup = content; return this; }, openOn() { return this; } }),
  };
  const context = vm.createContext({ Element, $state: v => v, $effect() {}, onMount() {},
    $props: () => ({ latitude: '', longitude: '', places: savedPlaces, onCoordinates() {}, onSelectPlace: place => selected.push(place.id), onCreateHere: (...args) => contexts.push(args) }),
    document: { createElement: () => new Element() },
    m: new Proxy({}, { get: () => () => '' }), nativeMap, nativeLeaflet: leaflet, root,
  });
  const places = readFileSync(new URL('../src/lib/places.ts', import.meta.url), 'utf8').replace(/^import .*;\n/gm, '').replace(/^export /gm, '');
  vm.runInContext(ts.transpileModule(places, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  const script = read('PlaceMap').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  vm.runInContext('map=nativeMap; leaflet=nativeLeaflet; mapElement=root; savedLayer={clearLayers(){}}; updateSavedPlaces()', context);
  return { points, selected, contexts, root, popup: () => popup, run: code => vm.runInContext(code, context),
    contextAt(target) {
      const event = { target, clientX: 140, clientY: 220, prevented: false, stopped: false,
        preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; } };
      context.event = event; vm.runInContext('nativeContextMenu(event)', context); return event;
    } };
}
test('saved marker selects by ID; coincident markers offer every place in a popup', () => {
  const single = leafletHarness([lake]); single.points[0].handlers.click();
  assert.deepEqual(single.selected, ['lake']);
  const overlap = leafletHarness([lake, { ...lake, id: 'other', name: '<Other>' }]);
  assert.equal(overlap.points.length, 1);
  overlap.points[0].handlers.click(); assert.equal(overlap.selected.length, 0);
  const buttons = overlap.popup().children;
  assert.equal(buttons.length, 2); assert.equal(buttons[1].textContent, '<Other>');
  buttons[1].listeners.click({ stopPropagation() {} });
  assert.deepEqual(overlap.selected, ['other']);
});
test('native context menu preserves map or marker coordinates and suppresses browser handling without selection', () => {
  const map = leafletHarness([lake]);
  const markerEvent = map.contextAt(map.points[0].element);
  assert.equal(markerEvent.prevented, true); assert.equal(markerEvent.stopped, true);
  assert.deepEqual(map.contexts[0].slice(0, 2), [1, 2]);
  map.contextAt(map.root);
  assert.deepEqual(map.contexts[1].slice(0, 2), [6, 7]);
  assert.equal(map.contexts[1][2].x, 140); assert.equal(map.selected.length, 0);
  map.run('latitude="8"; longitude="9"; updateMarker()');
  const marker = map.run('marker'); map.contextAt(marker.element);
  assert.deepEqual(map.contexts[2].slice(0, 2), [8, 9]);
});
test('confirmed journal write closes the real panel callback; new journal preserves draft without a write', async () => {
  for (const id of ['entry', null]) {
    const journal = journalHarness(id);
    await journal.run('showPicker()');
    const map = harness(undefined, { request: journal.request() });
    await map.run("loadCenter()"); map.run(`selected = ${JSON.stringify(lake)}`);
    await map.run('confirm()');
    assert.equal(map.closes(), 1); assert.equal(map.run('error'), '');
    assert.equal(journal.writes.length, id ? 1 : 0);
    assert.equal(journal.run('dirty'), !id);
    if (!id) {
      assert.equal(journal.run('pendingPlace.id'), 'lake');
      journal.run('entryId="created-entry"');
      await journal.run('applyPending(pending, entryId, generation)');
      assert.equal(journal.writes[0][1], 'created-entry');
    }
  }
  assert.match(read('EntryPlace'), /untrack\(\(\) => \{ void loadLink/);
});
test('real journal errors keep map open and retain draft; late generation result is not success', async () => {
  for (const failure of ['failed', 'journal_generation_conflict', 'place_revision_conflict']) {
    const journal = journalHarness('entry', async () => { throw failure; });
    await journal.run('showPicker()');
    const map = harness(undefined, { request: journal.request() });
    await map.run("loadCenter()"); map.run(`selected = ${JSON.stringify(lake)}`); await map.run('confirm()');
    assert.equal(map.closes(), 0); assert.equal(map.run('error'), 'save');
    assert.equal(journal.run('pendingPlace.id'), 'lake');
  }
  let finish;
  const journal = journalHarness('entry', () => new Promise(resolve => finish = resolve));
  await journal.run('showPicker()');
  const pending = journal.request().select(lake);
  journal.run('generation = 4'); finish({ place: lake, revision: '1' });
  assert.equal(await pending, false);
});
test('journal removal uses existing owner flow and closes only on success', async () => {
  for (const id of ['entry', null]) {
    const journal = journalHarness(id);
    journal.run(`link={place:${JSON.stringify(lake)}, revision:"2"}; pendingPlace=${JSON.stringify(lake)}; pending={kind:"link",placeId:"lake"}`);
    await journal.run('showPicker()');
    const map = harness(undefined, { request: journal.request() });
    await map.run('remove()'); assert.equal(map.closes(), 1);
    assert.equal(journal.run('pending'), null);
    if (id) assert.equal(journal.writes[0][3], null);
  }
  const journal = journalHarness('entry', async () => { throw 'failed'; });
  journal.run(`link={place:${JSON.stringify(lake)}, revision:"2"}`);
  await journal.run('showPicker()');
  const map = harness(undefined, { request: journal.request() });
  await map.run('remove()'); assert.equal(map.closes(), 0);
  assert.equal(journal.run('link.place.id'), 'lake');
});
test('current journal place is selected by UUID even when another place shares coordinates; cancel writes nothing', async () => {
  const journal = journalHarness();
  journal.run(`link={place:${JSON.stringify(lake)},revision:"2"}`);
  await journal.run('showPicker()');
  const map = harness(undefined, { request: journal.request() }, undefined, [lake, { ...lake, id: 'second', name: 'Other' }]);
  await map.run('loadCenter()');
  assert.equal(map.run('selected.id'), 'lake');
  assert.equal(map.run('latitude'), '1');
  map.run('pick(places[1]); close()');
  assert.equal(journal.writes.length, 0);
  assert.equal(journal.run('link.place.id'), 'lake');
});
test('context menu is positioned at the point and creation action is not duplicated', async () => {
  const map = harness();
  await map.run('contextPoint(0, 0, {x: 450, y: 320})');
  assert.equal(map.run('menuX'), 450); assert.equal(map.run('menuY'), 320);
  assert.equal(map.run('latitude'), '0');
  assert.match(source, /latitude && longitude && !menu && !form/);
  assert.match(source, /class="context-menu"[\s\S]*role="menu"/);
  assert.match(source, /onpointerdown/);
  assert.match(source, /height: min\(30vh,240px\)[\s\S]*overflow-y: auto/);
  assert.match(read('PlaceMap'), /contextTargets.set\(element, \(\) => marker!\.getLatLng\(\)\)/);
  assert.match(read('PlaceMap'), /map.mouseEventToLatLng\(event\)/);
});
test('Leaflet panes are confined below the sibling context menu', () => {
  const css = compile(source, { generate: 'client' }).css.code;
  assert.match(css, /\.map-content[^{}]*\{[^{}]*isolation:\s*isolate/);
  assert.match(css, /\.context-menu[^{}]*\{[^{}]*z-index:\s*160/);
  const leaflet = readFileSync(new URL('../node_modules/leaflet/dist/leaflet.css', import.meta.url), 'utf8');
  assert.match(leaflet, /\.leaflet-pane\s*\{\s*z-index:\s*400/);
});
test('default coordinates center the map without selecting a point; clicks remain local', async () => {
  const h = harness(async () => ({ place: { latitude: 59, longitude: 18 } }));
  await h.run('loadCenter()');
  assert.equal(h.run('center.latitude'), 59);
  assert.equal(h.run('latitude'), '');
  h.run('mark(0, 0)'); assert.equal(h.run('latitude'), '0');
  assert.doesNotMatch(source, /setWeatherDefaultPlace|setJournalPlace|fetch_historical_weather/);
});
test('creating a place never selects it for a journal until separately confirmed', async () => {
  let selected = 0, creates = 0;
  const h = harness(undefined, { request: { mode: 'journal', select: async () => { selected++; return true; } } }, async () => {
    creates++; return { id: 'new', name: 'Lake', latitude: 0, longitude: 0, revision: '1' };
  });
  await h.run('loadCenter()');
  h.run('contextPoint(0, 0); startCreate(); name = "Lake"');
  assert.equal(creates, 0);
  await h.run('registerPlace()');
  assert.equal(creates, 1); assert.equal(selected, 0); assert.equal(h.run('notice'), true);
  await h.run('confirm()'); assert.equal(selected, 1); assert.equal(h.closes(), 1);
});
test('save and use creates once then closes on journal success or a preserved new draft', async () => {
  for (const id of ['entry', null]) {
    let creates = 0;
    const journal = journalHarness(id); await journal.run('showPicker()');
    const panel = harness(undefined, { request: journal.request() }, async () => { creates++; return lake; });
    await panel.run('loadCenter()'); panel.run('mark(1,2); name="Lake"');
    await panel.run('registerPlace(true)');
    assert.equal(creates, 1); assert.equal(panel.closes(), 1);
    assert.equal(journal.writes.length, id ? 1 : 0);
    if (!id) assert.equal(journal.run('pendingPlace.id'), 'lake');
  }
});
test('partial save preserves registration and retry only repeats application, including thrown conflicts', async () => {
  for (const mode of ['journal', 'default']) {
    let creates = 0, selections = 0;
    const panel = harness(undefined, { request: { mode, select: async () => { if (++selections === 1) throw 'journal_generation_conflict'; return true; } } }, async () => { creates++; return lake; });
    await panel.run('loadCenter()'); panel.run('mark(1,2); name="Lake"');
    await panel.run('registerPlace(true)');
    assert.equal(panel.closes(), 0); assert.equal(panel.run('notice'), true);
    assert.equal(panel.run('error'), 'link'); assert.equal(panel.run('selected.id'), 'lake');
    assert.equal(panel.run('form'), false);
    await panel.run('confirm()'); assert.equal(panel.closes(), 1); assert.equal(creates, 1); assert.equal(selections, 2);
  }
});
test('orientation save never applies a place and cancel never creates one', async () => {
  let creates = 0, selections = 0;
  const panel = harness(undefined, { request: { mode: 'orientation', select: async () => { selections++; return true; } } }, async () => { creates++; return lake; });
  await panel.run('loadCenter()'); panel.run('mark(1,2); name="Lake"; form=true; form=false');
  assert.equal(creates, 0);
  await panel.run('registerPlace(true)'); assert.equal(creates, 1); assert.equal(selections, 0); assert.equal(panel.closes(), 0);
});
test('failed owner confirmation keeps the map open and busy creation cannot be closed or duplicated', async () => {
  let finish;
  const h = harness(undefined, { request: { mode: 'default', select: async () => false } }, () => new Promise(resolve => finish = resolve));
  await h.run('loadCenter()'); h.run('mark(1,2); name="Lake"');
  const pending = h.run('registerPlace()');
  h.run('close()'); await h.run('registerPlace()'); assert.equal(h.closes(), 0);
  finish({ id: 'p', name: 'Lake', latitude: 1, longitude: 2, revision: '1' }); await pending;
  await h.run('confirm()'); assert.equal(h.closes(), 0); assert.equal(h.run('error'), 'link');
  assert.equal(h.run('eligible({latitude:null,longitude:null})'), false);
});
test('missing default and read failures permit world view', async () => {
  for (const fail of [false, true]) {
    const h = harness(async () => { if (fail) throw 'failed'; return { place: null }; });
    await h.run('loadCenter()');
    assert.equal(h.run('loaded'), true); assert.equal(h.run('center'), null);
    assert.equal(h.run('failed'), fail);
  }
});
test('late response from a closed panel cannot change a reopened panel', async () => {
  let resolve;
  const old = harness(() => new Promise(r => resolve = r));
  const pending = old.run('loadCenter()');
  for (let i = 0; i < 8; i++) await Promise.resolve();
  old.destroy();
  const current = harness(); await current.run('loadCenter()');
  resolve({ place: { latitude: 50, longitude: 10 } }); await pending;
  assert.equal(old.run('center'), null); assert.equal(current.run('center'), null);
});
test('navigation uses existing top toggle, focus restoration and a layout-stable overlay', () => {
  compile(source, { generate: 'client' });
  const navigation = read('AppNavigation'); compile(navigation, { generate: 'client' });
  assert.match(navigation, /onclick=\{\(\) => togglePanel\("top"\)\}/);
  assert.match(navigation, /placeMap.request[\s\S]*PlaceMapPanel/);
  const controller = readFileSync(new URL('../src/lib/place-map.svelte.ts', import.meta.url), 'utf8');
  assert.match(controller, /returnFocus.focus\(\)/);
  assert.match(source, /event.key === "Escape"/);
  assert.match(source, /position: fixed/); assert.match(source, /100dvh/);
  assert.match(source, /clip-path/); assert.match(source, /prefers-reduced-motion/);
  assert.match(read('PlaceMap'), /invalidateSize/);
  assert.match(read('EntryPlace'), /\.overlay \{ z-index: 200/);
  const layout = readFileSync(new URL('../src/routes/+layout.svelte', import.meta.url), 'utf8');
  assert.match(layout, /<AppNavigation\s*\/>[\s\S]*\{@render children\(\)\}/);
});

test('registry creates name-only places, while invalid coordinate pairs never write', async () => {
 const calls=[];
 const h=harness(undefined,{},async(...args)=>{calls.push(args);return {...lake,latitude:args[2],longitude:args[3]};});
 await h.run('loadCenter()');await h.run('startCreate(false)');h.run('name="Forest"');await h.run('registerPlace()');
 assert.deepEqual(calls,[[3,'Forest',null,null]]);assert.equal(h.run('selected.latitude'),null);
 for(const [lat,lon] of [['1',''],['','2'],['NaN','2'],['91','2'],['1','181'],['Infinity','2']]){
  h.context.lat=lat;h.context.lon=lon;h.run('latitude=lat;longitude=lon');await h.run('registerPlace()');assert.equal(h.run('error'),'coordinates');
 }
 assert.equal(calls.length,1);
});
test('name-only journal choice is linked explicitly, but default weather still requires coordinates',async()=>{
 for(const mode of ['journal','default']){
  const chosen=[];const h=harness(undefined,{request:{mode,select:async p=>{chosen.push(p);return true;}}},async()=>({...lake,latitude:null,longitude:null}));
  await h.run('loadCenter()');h.run('name="Forest"');await h.run('registerPlace(true)');
  assert.equal(chosen.length,mode==='journal'?1:0);assert.equal(h.closes(),mode==='journal'?1:0);
  if(mode==='default')assert.equal(h.run('error'),'weatherCoordinates');
 }
});
test('register edits use original revision, add or remove coordinates and never apply a journal selection',async()=>{
 for(const [lat,lon] of [['',''],['60','19']]){
  const calls=[],selected=[];
  const h=harness(undefined,{request:{mode:'journal',select:async p=>{selected.push(p);return true;}}},undefined,[lake],{
   updatePlace:async(...args)=>{calls.push(args);return {...lake,name:args[2],latitude:args[3],longitude:args[4],revision:'2'};},
  });
  await h.run('loadCenter()');h.run('pick(places[0]);startEdit();name="New name"');h.context.lat=lat;h.context.lon=lon;h.run('latitude=lat;longitude=lon');await h.run('registerPlace(true)');
  assert.equal(calls.length,1);assert.equal(calls[0][1].revision,'1');assert.equal(calls[0][2],'New name');assert.equal(calls[0][3],lat?60:null);
  assert.equal(h.run('selected.revision'),'2');assert.equal(h.run('form'),false);assert.deepEqual(selected,[]);assert.equal(h.closes(),0);
 }
});
test('registry deletion only happens on confirmation and reports protected references',async()=>{
 for(const blocked of [false,true]){
  const calls=[];const h=harness(undefined,{},undefined,[lake],{deletePlace:async(...args)=>{calls.push(args);if(blocked)throw 'place_in_use';}});
  await h.run('loadCenter()');h.run('pick(places[0]);deleteTarget=selected');assert.equal(calls.length,0);
  await h.run('erasePlace()');assert.deepEqual(calls,[[3,'lake']]);assert.equal(h.run('places.length'),blocked?1:0);
  assert.equal(h.run('error'),blocked?'inUse':'');assert.equal(h.closes(),0);
 }
});
test('revision conflict reloads current register without overwriting draft or retrying automatically',async()=>{
 let reads=0,writes=0;const current={...lake,name:'Other edit',revision:'2'};
 const h=harness(undefined,{},undefined,[lake],{listPlaces:async()=>++reads===1?[lake]:[current],updatePlace:async()=>{writes++;throw 'place_revision_conflict';}});
 await h.run('loadCenter()');h.run('pick(places[0]);startEdit();name="My draft"');await h.run('registerPlace()');
 assert.equal(h.run('error'),'conflict');assert.equal(h.run('name'),'My draft');assert.equal(h.run('editing.revision'),'1');assert.equal(h.run('selected.revision'),'2');assert.equal(writes,1);
 h.run('startEdit()');assert.equal(h.run('discardOpen'),true);h.run('discard()');assert.equal(h.run('name'),'Other edit');assert.equal(h.run('editing.revision'),'2');
});
test('active weather default rejects coordinate removal without losing draft',async()=>{
 const h=harness(undefined,{},undefined,[lake],{updatePlace:async()=>{throw 'place_coordinates_required_by_weather_default';}});
 await h.run('loadCenter()');h.run('pick(places[0]);startEdit();latitude="";longitude=""');await h.run('registerPlace()');
 assert.equal(h.run('error'),'defaultCoordinates');assert.equal(h.run('form'),true);assert.equal(h.run('latitude'),'');assert.equal(h.run('selected.latitude'),1);
});
test('unsaved registry form survives close, selection and navigation until explicit discard',async()=>{
 let navigate,guard,cancelled=0;
 const h=harness(undefined,{controller:{setCloseGuard:f=>{guard=f;return ()=>{};}}},undefined,[lake],{beforeNavigate:f=>navigate=f,onMount:f=>f()});
 await h.run('loadCenter()');await h.run('startCreate(false)');h.run('name="Unsaved"');
 h.run('close()');assert.equal(h.closes(),0);assert.equal(h.run('discardOpen'),true);
 h.run('discardOpen=false;pick(places[0])');assert.equal(h.run('name'),'Unsaved');assert.equal(h.run('form'),true);
 navigate({cancel(){cancelled++;}});assert.equal(cancelled,1);assert.equal(h.run('dirty()'),true);
 assert.equal(guard(),false);h.run('discard()');assert.equal(h.closes(),1);
});
test('registry writes serialize, ignore late results and block use after generation change',async()=>{
 let finish,writes=0;
 const h=harness(undefined,{},undefined,[lake],{updatePlace:async()=>{writes++;return new Promise(r=>finish=r);}});
 await h.run('loadCenter()');h.run('pick(places[0]);startEdit();name="Changed"');const pending=h.run('registerPlace()');await h.run('registerPlace()');h.run('close()');assert.equal(writes,1);assert.equal(h.closes(),0);
 h.destroy();finish({...lake,name:'Changed',revision:'2'});await pending;assert.equal(h.run('places[0].name'),'Lake');
 const changed=harness(undefined,{},undefined,[lake],{updatePlace:async()=>{throw 'journal_generation_conflict';}});
 await changed.run('loadCenter()');changed.run('pick(places[0]);startEdit();name="Changed"');await changed.run('registerPlace()');assert.equal(changed.run('generation'),null);assert.equal(changed.run('form'),true);assert.equal(changed.run('error'),'generation');
});

test('confirmed registry write with failed session reread is not reported as a failed write or retried',async()=>{
 let checks=0,creates=0;
 const h=harness(undefined,{},async()=>{creates++;return lake;},[],{invoke:async()=>{if(++checks>2)throw 'unavailable';return {generation:3};}});
 await h.run('loadCenter()');h.run('name="Lake"');await h.run('registerPlace()');
 assert.equal(creates,1);assert.equal(h.run('error'),'savedUnverified');assert.equal(h.run('generation'),null);assert.equal(h.run('places.length'),0);
 await h.run('registerPlace()');assert.equal(creates,1);
});
test('map controller retains dirty panel when navigation closes or another owner opens it',()=>{
 const state=v=>v;state.raw=v=>v;
 const c=vm.createContext({$state:state,HTMLElement:class {},document:{activeElement:null}});
 const source=readFileSync(new URL('../src/lib/place-map.svelte.ts',import.meta.url),'utf8').replace(/^import .*;$/gm,'').replace(/export /g,'');
 vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 vm.runInContext('const controller=createPlaceMap();const first={mode:"orientation"};controller.open(first);controller.setCloseGuard(()=>false);controller.close();controller.open({mode:"default"})',c);
 assert.equal(vm.runInContext('controller.request===first',c),true);
 vm.runInContext('controller.setCloseGuard(()=>true);controller.close()',c);assert.equal(vm.runInContext('controller.request',c),null);
});

test('coordinate-less register place can be edited in default mode without becoming the default',async()=>{
 const noPoint={...lake,latitude:null,longitude:null};let writes=0;
 const h=harness(undefined,{request:{mode:'default',select:async()=>{writes++;return true;}}},undefined,[noPoint]);
 await h.run('loadCenter()');h.run('pick(places[0])');await h.run('confirm()');assert.equal(writes,0);
 h.run('startEdit();latitude="1";longitude="2"');h.run('protect(resetForm)');assert.equal(h.run('discardOpen'),true);
 h.run('discard()');assert.equal(h.run('latitude'),'');assert.equal(h.run('longitude'),'');assert.equal(h.run('selected.id'),'lake');
});

function realController() {
 const state=v=>v;state.raw=v=>v;
 const c=vm.createContext({$state:state,HTMLElement:class {},document:{activeElement:null}});
 const source=readFileSync(new URL('../src/lib/place-map.svelte.ts',import.meta.url),'utf8').replace(/^import .*;$/gm,'').replace(/export /g,'');
 vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 return vm.runInContext('createPlaceMap()',c);
}
async function guardedPanel(select,place,create) {
 const controller=realController();controller.open({mode:'journal',select});
 const h=harness(undefined,{request:controller.request,controller,onclose:()=>controller.close()},create,[place],{onMount:f=>f()});
 for(let i=0;i<30 && !h.run('loaded');i++)await Promise.resolve();
 assert.equal(h.run('loaded'),true);
 return {h,controller};
}
test('both use buttons close through the actual controller guard only after accepted selection, with or without coordinates',async()=>{
 for(const coordinates of [false,true])for(const creating of [false,true])for(const outcome of ['accepted','rejected','error']){
  const place={...lake,latitude:coordinates?1:null,longitude:coordinates?2:null};
  let creates=0,selections=0;
  const {h,controller}=await guardedPanel(async()=>{selections++;if(outcome==='error')throw 'place_revision_conflict';return outcome==='accepted';},place,async()=>{creates++;return place;});
  if(creating){await h.run('startCreate(false)');h.run('name="Lake"');if(coordinates)h.run('mark(1,2)');await h.run('registerPlace(true)');}
  else{h.run('pick(places[0])');await h.run('confirm()');}
  assert.equal(selections,1);assert.equal(creates,creating?1:0);
  assert.equal(controller.request===null,outcome==='accepted',`${creating?'save and use':'use'} / coordinates=${coordinates} / ${outcome}`);
  assert.equal(h.run('error'),outcome==='accepted'?'':creating?'link':'save');
  assert.equal(controller.busy,false);
  if(creating && outcome!=='accepted'){
   h.context.accept=async()=>true;h.run("request.select=accept");await h.run('confirm()');assert.equal(creates,1);assert.equal(controller.request,null);
  }
 }
});
test('actual controller cannot close a pending selection; accepted removal also releases the local guard',async()=>{
 let finish;
 const {h,controller}=await guardedPanel(()=>new Promise(r=>finish=r),lake);
 h.run('pick(places[0])');const pending=h.run('confirm()');controller.close();assert.notEqual(controller.request,null);
 finish(true);await pending;assert.equal(controller.request,null);
 const removed=await guardedPanel(async()=>true,lake);removed.h.context.accept=async()=>true;removed.h.run("request.remove=accept");
 await removed.h.run('remove()');assert.equal(removed.controller.request,null);
});

test('explicit reuse of the same place ID sends one protected write for the current register snapshot',async()=>{
 for(const changes of [{name:'Renamed'},{latitude:0,longitude:0},{name:'Renamed',latitude:null,longitude:null}]){
  const current={...lake,...changes,revision:'2'};
  const h=journalHarness('entry',async(_gen,_id,revision)=>({place:{...current,placeRevision:current.revision},revision:String(Number(revision)+1)}));
  h.run(`link={place:${JSON.stringify({...lake,placeRevision:'1'})},revision:"4"}`);
  h.run(`const current=${JSON.stringify(current)}`);await h.run('selectPlace(current)');
  assert.equal(h.writes.length,1);assert.deepEqual(h.writes[0],[3,'entry','4','lake']);
  assert.equal(h.run('link.revision'),'5');assert.equal(h.run('link.place.placeRevision'),'2');assert.equal(h.run('link.place.name'),current.name);
  assert.equal(h.run('link.place.latitude'),current.latitude);assert.equal(h.run('link.place.longitude'),current.longitude);
 }
});
test('same-ID selection retains conflict, pending draft and late-reply protections',async()=>{
 const failed=journalHarness('entry',async()=>{throw 'place_revision_conflict';});
 failed.run(`link={place:${JSON.stringify({...lake,placeRevision:'1'})},revision:"4"}`);
 assert.equal(await failed.run(`selectPlace(${JSON.stringify({...lake,name:'New',revision:'2'})})`),false);
 assert.equal(failed.run('dirty'),true);assert.equal(failed.run('pendingPlace.name'),'New');assert.equal(failed.writes.length,1);
 let release;
 const late=journalHarness('entry',()=>new Promise(r=>release=r));
 late.run(`link={place:${JSON.stringify({...lake,placeRevision:'1'})},revision:"4"}`);
 const pending=late.run(`selectPlace(${JSON.stringify({...lake,name:'New',revision:'2'})})`);
 late.run('generation=4');release({place:{...lake,name:'New',placeRevision:'2'},revision:'5'});
 assert.equal(await pending,false);assert.equal(late.run('link.place.name'),'Lake');
});
test('selection actions precede register management and journal unlink is a separate group',()=>{
 const primary=source.slice(source.indexOf('<div class="primary-actions">'),source.indexOf('<section class="action-group"'));
 assert.match(primary,/class="primary"[\s\S]*onclick=\{confirm\}/);assert.match(primary,/onclick=\{close\}/);
 assert.doesNotMatch(primary,/startEdit|erasePlace|onclick=\{remove\}/);
 const manage=source.slice(source.indexOf('aria-label={m.place_manage_heading'),source.indexOf('{#if discardOpen}'));
 assert.match(manage,/place_new/);assert.match(manage,/place_registry_edit/);assert.match(manage,/place_registry_delete/);
 const link=source.slice(source.indexOf('aria-label={m.place_journal_link_heading'),source.indexOf('</aside>'));
 assert.match(link,/onclick=\{remove\}/);assert.doesNotMatch(link,/onclick=\{confirm\}/);
 assert.match(source,/flex-wrap: wrap/);assert.match(source,/overflow-wrap: anywhere/);
});

test('blocked deletion opens a modal, preserves the selected register place and resumes selection after Close',async()=>{
 let deletes=0;
 const {h,controller}=await guardedPanel(async()=>true,lake);
 h.context.deletePlace=async()=>{deletes++;throw 'place_in_use';};
 h.context.focusLog=[];h.run('blockedButton={focus:()=>focusLog.push("dialog")};deleteButton={focus:()=>focusLog.push("delete")};pick(places[0]);deleteTarget=selected');
 const before=h.run('JSON.stringify(places)');await h.run('erasePlace()');
 assert.equal(h.run('blockedOpen'),true);assert.equal(h.run('deleteTarget'),null);assert.equal(h.run('JSON.stringify(places)'),before);assert.equal(h.run('selected.revision'),'1');assert.equal(h.run('selected.id'),'lake');assert.equal(deletes,1);
 assert.ok(h.context.focusLog.includes('dialog'));controller.close();assert.notEqual(controller.request,null);
 h.run('dismissBlocked()');await Promise.resolve();assert.equal(h.run('blockedOpen'),false);assert.equal(h.run('error'),'');assert.equal(h.context.focusLog.at(-1),'delete');assert.equal(deletes,1);
 await h.run('confirm()');assert.equal(controller.request,null);
});
test('blocked dialog supports Tab and Escape, uses neutral bilingual copy, and other errors remain inline',async()=>{
 const h=harness(undefined,{},undefined,[lake],{deletePlace:async()=>{throw 'place_in_use';}});
 await h.run('loadCenter()');h.context.focusLog=[];h.run('pick(places[0]);deleteTarget=selected;blockedButton={focus:()=>focusLog.push("dialog")};deleteButton={focus:()=>focusLog.push("delete")}');await h.run('erasePlace()');
 const keys=[];h.context.event={key:'Tab',shiftKey:true,preventDefault(){keys.push('prevent');},stopPropagation(){keys.push('stop');}};
 h.run('blockedKeys(event)');assert.deepEqual(keys,['prevent','stop']);assert.equal(h.run('blockedOpen'),true);
 h.context.event.key='Escape';h.run('blockedKeys(event)');await Promise.resolve();assert.equal(h.run('blockedOpen'),false);assert.equal(h.context.focusLog.at(-1),'delete');assert.equal(h.run('selected.id'),'lake');
 await h.run('mutationError("place_write_failed")');assert.equal(h.run('blockedOpen'),false);assert.equal(h.run('error'),'save');
 assert.match(source,/role="dialog" aria-modal="true" aria-labelledby="place-blocked-title" aria-describedby="place-blocked-description"/);
 assert.match(source,/class="workspace" inert=\{blockedOpen\}/);assert.match(source,/error && error !== "inUse"/);assert.match(source,/onclick=\{dismissBlocked\}/);
 for(const locale of ['sv','en']){
  const messages=JSON.parse(readFileSync(new URL(`../messages/${locale}.json`,import.meta.url),'utf8'));
  assert.equal(messages.place_registry_blocked_title,locale==='sv'?'Platsen kan inte raderas':'The place cannot be deleted');
  assert.equal(messages.place_registry_close,locale==='sv'?'Stäng':'Close');
  assert.match(messages.place_registry_blocked_description,locale==='sv'?/journalanteckning eller som standardplats/:/journal entry or as the default/);
 }
});

const usageData={placeId:'lake',isWeatherDefault:true,totalCount:2,entries:[{id:'entry-one',entryDate:'2026-10-30',title:''},{id:'entry-two',entryDate:'legacy date',title:'Long title '.repeat(1000)}]};
test('blocked usage loads both reference types, preserves complete lists and refreshes changed usage without deleting',async()=>{
 let reads=0,deletes=0;
 const h=harness(undefined,{},undefined,[lake],{deletePlace:async()=>{deletes++;throw 'place_in_use';},readPlaceUsage:async()=>{reads++;return reads===1?structuredClone(usageData):{placeId:'lake',isWeatherDefault:false,totalCount:0,entries:[]};}});
 await h.run('loadCenter()');h.run('pick(places[0]);deleteTarget=selected');await h.run('erasePlace()');
 assert.equal(h.run('usage.totalCount'),2);assert.equal(h.run('usage.isWeatherDefault'),true);assert.equal(h.run('usage.entries[1].title.length'),usageData.entries[1].title.length);assert.equal(h.run('selected.id'),'lake');
 await h.run('loadUsage()');assert.equal(h.run('usage.totalCount'),0);assert.equal(h.run('usage.isWeatherDefault'),false);assert.equal(deletes,1);assert.equal(h.run('blockedOpen'),true);
 assert.match(source,/usage.totalCount === 0/);assert.match(source,/usage.entries as entry/);assert.doesNotMatch(source,/usage.entries.slice/);
 assert.match(source,/entry.title.trim\(\) \|\| m.search_untitled/);assert.match(source,/formatJournalDate\(entry.entryDate, locale\)/);
});
test('usage failure is never no-references; retry can recover and late reads cannot replace newer or closed dialogs',async()=>{
 let fail=true,release;
 const h=harness(undefined,{},undefined,[lake],{readPlaceUsage:async()=>{if(fail)throw 'failed';return usageData;}});
 await h.run('loadCenter()');h.run('pick(places[0]);blockedOpen=true;usagePlaceId="lake"');await h.run('loadUsage()');assert.equal(h.run('usageError'),true);assert.equal(h.run('usage'),null);
 fail=false;await h.run('loadUsage()');assert.equal(h.run('usageError'),false);assert.equal(h.run('usage.totalCount'),2);
 h.context.readPlaceUsage=()=>new Promise(r=>release=r);const pending=h.run('loadUsage()');h.run('dismissBlocked()');release(usageData);await pending;assert.equal(h.run('usage'),null);
 h.run('blockedOpen=true');const old=h.run('loadUsage()');h.context.readPlaceUsage=async()=>({...usageData,totalCount:0,entries:[]});await h.run('loadUsage()');release(usageData);await old;assert.equal(h.run('usage.totalCount'),0);
 const generation=h.run('loadUsage()');h.run('generation=4');await generation;assert.equal(h.run('usage'),null);
});
test('multi-control dialog focus loops only at boundaries and Escape restores focus',async()=>{
 const h=harness();let focused='',prevented=0;
 const first={focus:()=>focused='first'},last={focus:()=>focused='last'},middle={focus:()=>focused='middle'};
 h.context.controls=[first,middle,last];h.context.document={activeElement:last};h.context.event={key:'Tab',shiftKey:false,preventDefault(){prevented++;},stopPropagation(){}};
 h.run('blockedDialog={querySelectorAll:()=>controls};blockedOpen=true');h.run('blockedKeys(event)');assert.equal(focused,'first');assert.equal(prevented,1);
 h.context.document.activeElement=first;h.context.event.shiftKey=true;h.run('blockedKeys(event)');assert.equal(focused,'last');
 h.context.document.activeElement=middle;h.context.event.shiftKey=false;const before=prevented;h.run('blockedKeys(event)');assert.equal(prevented,before);
 h.context.document.activeElement={};h.run('blockedKeys(event)');assert.equal(focused,'first');
 h.context.event.key='Escape';h.run('blockedKeys(event)');assert.equal(h.run('blockedOpen'),false);
});
test('usage link closes dialog and map before normal navigation and blocks a changed database session',async()=>{
 const events=[];
 const h=harness(undefined,{onclose:()=>events.push('close')},undefined,[lake],{goto:async path=>{assert.equal(h.run('blockedOpen'),false);events.push(path);}});
 await h.run('loadCenter()');h.context.value=usageData;h.run('usage=value;blockedOpen=true');await h.run('openUsageEntry("entry-one")');assert.deepEqual(events,['close','/journal/entries/entry-one']);
 h.run('usage=value;blockedOpen=true');h.context.invoke=async()=>({generation:4});await h.run('openUsageEntry("entry-one")');assert.equal(events.length,2);assert.equal(h.run('usageError'),true);assert.equal(h.run('usage'),null);assert.equal(h.run('blockedOpen'),true);
});
test('usage navigation respects the real editor guard and preserves an unsaved draft when navigation is cancelled',async()=>{
 const editor=read('JournalEditor');const body=editor.match(/beforeNavigate\(\(navigation\) => \{([\s\S]*?)\n  \}\);/)[1];
 const c=vm.createContext({deletingEntry:false,photoDraft:false,hasPendingChanges:true,isSaving:false,weatherDirty:false,weatherBusy:false,placeDirty:false,placeBusy:false,allowDiscardNavigation:false,pendingNavigationUrl:null,showLeaveConfirmation:false,draft:'Unsaved journal text'});
 vm.runInContext(`function guard(navigation){${body}}`,c);let cancelled=false;
 const h=harness(undefined,{},undefined,[lake],{goto:async path=>{c.navigation={to:{url:new URL(path,'https://mno.invalid')},cancel(){cancelled=true;}};vm.runInContext('guard(navigation)',c);}});
 await h.run('loadCenter()');h.context.value=usageData;h.run('usage=value;blockedOpen=true');await h.run('openUsageEntry("entry-one")');
 assert.equal(cancelled,true);assert.equal(vm.runInContext('showLeaveConfirmation',c),true);assert.equal(vm.runInContext('draft',c),'Unsaved journal text');assert.equal(vm.runInContext('allowDiscardNavigation',c),false);assert.equal(h.closes(),1);
});

async function mountedMap(props = {}) {
 let mount,cleanup,resize,frameId=0,viewBounds=null;
 const frames=new Map(),views=[],published=[],markers=[],tileHandlers={},mapHandlers={};
 let dimensions={x:0,y:0},center={lat:0,lng:0},zoom=0,mapOptions,tileOptions,reloads=0,tileZoom=0;
 const element={setAttribute(){},addEventListener(){},removeEventListener(){}};
 const emitMap=(name)=>{for(const handler of mapHandlers[name]??[])handler();};
 const activeTiles=new Map();
 const tileLayer={
  on(name,fn){tileHandlers[name]=fn;return this;},off(){for(const key of Object.keys(tileHandlers))delete tileHandlers[key];},addTo(){tileZoom=Math.max(tileOptions.minNativeZoom,Math.round(zoom));if(reloads && tileLayer.onReload)tileLayer.onReload(tileZoom);return this;},
  remove(){reloads++;for(const {tile,coords} of [...activeTiles.values()])tileHandlers.tileunload?.({tile,coords});activeTiles.clear();return this;},
  getTileSize:()=>({x:256,y:256}),redraw(){throw new Error('redraw must not be used at fractional zoom');},
 };
 const nativeMap={
  on(names,fn){for(const name of names.split(' '))(mapHandlers[name]??=[]).push(fn);return this;},
  getContainer:()=>element,getSize:()=>dimensions,getZoom:()=>zoom,getCenter:()=>center,
  getBoundsZoom(bounds){assert.deepEqual(bounds,{south:-85.0511287798,west:-180,north:85.0511287798,east:180});return Math.max(mapOptions.minZoom,Math.floor(Math.log2(Math.min(dimensions.x,dimensions.y)/256)*10)/10);},
  setView(point,next){emitMap('movestart');emitMap('zoomstart');center={lat:point[0],lng:point[1]};zoom=next;views.push({point:[...point],zoom});emitMap('moveend');emitMap('zoomend');return this;},
  getZoomScale:(to,from)=>2**(to-from),getPixelBounds:()=>viewBounds??{min:{x:0,y:0},max:{x:256,y:256}},invalidateSize(options){assert.deepEqual({...options},{pan:false});},
  remove(){nativeMap.removed=true;},
 };
 const fakeLeaflet={
  map(_element,options){mapOptions=options;center={lat:options.center[0],lng:options.center[1]};zoom=options.zoom;return nativeMap;},
  tileLayer(url,options){assert.equal(url,'https://tile.openstreetmap.org/{z}/{x}/{y}.png');tileOptions=options;return tileLayer;},
  latLngBounds:([south,west],[north,east])=>({south,west,north,east}),layerGroup:()=>({addTo(){return this;},clearLayers(){}}),divIcon:()=>({}),
  marker(coordinates){const marker={point:[...coordinates],on(){return this;},addTo(){return this;},getElement(){},setLatLng(value){this.point=[...value];},remove(){this.removed=true;}};markers.push(marker);return marker;},
 };
 const c=vm.createContext({$state:v=>v,$effect(){},$props:()=>({latitude:'',longitude:'',onCoordinates:(...args)=>published.push(args),...props}),
  onMount:fn=>mount=fn,ResizeObserver:class{constructor(fn){resize=fn;}observe(){}disconnect(){resize=null;}},
  requestAnimationFrame:fn=>{frames.set(++frameId,fn);return frameId;},cancelAnimationFrame:id=>frames.delete(id),fakeLeaflet,
  m:new Proxy({},{get:()=>()=>''}),mapDraftCoordinates:(lat,lon)=>{if(!lat.trim()||!lon.trim())return null;const a=Number(lat),b=Number(lon);return Number.isFinite(a)&&Number.isFinite(b)&&Math.abs(a)<=90&&Math.abs(b)<=180?[a,b]:null;},groupMappedPlaces:()=>[],
 });
 const script=read('PlaceMap').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,'').replace('await import("leaflet")','await Promise.resolve(fakeLeaflet)');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 c.element=element;vm.runInContext('mapElement=element',c);cleanup=mount();for(let i=0;i<6;i++)await Promise.resolve();
 return {run:code=>vm.runInContext(code,c),views,published,markers,map:nativeMap,layer:tileLayer,
  options:()=>({map:mapOptions,tile:tileOptions}),reloads:()=>reloads,tileZoom:()=>tileZoom,
  resize(x,y){dimensions={x,y};resize?.();},flush(){for(const [id,fn] of [...frames]){frames.delete(id);fn();}},
  userView(point,next){center={lat:point[0],lng:point[1]};zoom=next;emitMap('movestart');emitMap('zoomstart');emitMap('moveend');},
  bounds(value){viewBounds=value;emitMap('moveend');},
  tile(name,tile,coords={x:0,y:0,z:0}){if(name==='tileloadstart')activeTiles.set(tile,{tile,coords});if(name==='tileunload')activeTiles.delete(tile);tileHandlers[name]?.({tile,coords});},
  destroy(){cleanup();},callbacks:()=>({...tileHandlers}),
 };
}
test('mounted map uses form, explicit selection, journal snapshot, default, then world without publishing coordinates',async()=>{
 const point={latitude:5,longitude:6};
 for(const [props,expected] of [
  [{latitude:'1',longitude:'2',focusCenter:point,journalCenter:{latitude:7,longitude:8},defaultCenter:{latitude:9,longitude:10}},[1,2]],
  [{focusCenter:point,journalCenter:{latitude:7,longitude:8}},[5,6]],
  [{journalCenter:point,defaultCenter:{latitude:9,longitude:10}},[5,6]],
  [{journalCenter:{latitude:NaN,longitude:1},defaultCenter:point},[5,6]],
  [{},[0,0]],
 ]){
  const h=await mountedMap(props);h.resize(900,500);h.flush();assert.deepEqual([h.map.getCenter().lat,h.map.getCenter().lng],expected);assert.deepEqual(h.published,[]);
  assert.equal(h.options().tile.noWrap,true);assert.equal(h.options().tile.minNativeZoom,0);h.destroy();
 }
});
test('world fitting waits for real dimensions, contains the whole world in narrow or low maps, and avoids duplicate fits',async()=>{
 for(const [width,height] of [[1200,500],[190,600],[1000,64]]){
  const h=await mountedMap();h.flush();assert.equal(h.views.length,0);
  h.resize(width,height);h.flush();assert.equal(h.views.length,1);const worldPixels=256*2**h.map.getZoom();assert.ok(worldPixels<=width+.001 && worldPixels<=height+.001);
  h.resize(width,height);h.flush();assert.equal(h.views.length,1);
  h.resize(width/2,height/2);h.flush();assert.equal(h.views.length,2);assert.ok(256*2**h.map.getZoom()<=Math.min(width/2,height/2)+.001);h.destroy();
 }
});
test('resizing preserves user pan and zoom, explicit selection and local marker coordinates',async()=>{
 const h=await mountedMap();h.resize(900,500);h.flush();h.userView([20,30],4);const count=h.views.length;
 h.resize(500,300);h.flush();assert.equal(h.views.length,count);assert.deepEqual([h.map.getCenter().lat,h.map.getCenter().lng,h.map.getZoom()],[20,30,4]);
 h.run('focusCenter={latitude:50,longitude:10};focusSelectedPlace()');const selectedCount=h.views.length;h.resize(1000,600);h.flush();assert.equal(h.views.length,selectedCount);assert.equal(h.map.getCenter().lat,50);
 h.run('latitude="51";longitude="11";updateMarker()');assert.deepEqual(h.markers[0].point,[51,11]);h.destroy();
});
test('tile errors remain until relevant failures recover or leave the view; load or unrelated success alone cannot clear them',async()=>{
 const h=await mountedMap();h.resize(256,256);h.flush();const failed={},other={};
 h.tile('tileloadstart',failed);h.tile('tileerror',failed);h.tile('tileloadstart',other);h.tile('tileload',other);h.tile('load',other);assert.equal(h.run('tilesUnavailable'),true);
 h.bounds({min:{x:300,y:0},max:{x:556,y:256}});assert.equal(h.run('tilesUnavailable'),false);
 h.bounds({min:{x:0,y:0},max:{x:256,y:256}});assert.equal(h.run('tilesUnavailable'),true);
 h.tile('tileunload',failed);assert.equal(h.run('tilesUnavailable'),false);h.tile('tileerror',failed);assert.equal(h.run('tilesUnavailable'),false);h.destroy();
});
test('retry serializes requests, preserves camera and markers, and handles failure followed by success without stale events',async()=>{
 const h=await mountedMap({latitude:'1',longitude:'2'});h.resize(256,256);h.flush();h.userView([1,2],0);const original={};
 h.tile('tileloadstart',original);h.tile('tileerror',original);let current;
 h.layer.onReload=()=>{h.tile('tileunload',original);current={};h.tile('tileloadstart',current);};
 const before={center:{...h.map.getCenter()},zoom:h.map.getZoom(),marker:[...h.markers[0].point],views:h.views.length};
 h.run('retryTiles();retryTiles()');assert.equal(h.reloads(),1);assert.equal(h.run('retrying'),true);
 h.tile('tileerror',original);h.tile('load',original);assert.equal(h.run('retrying'),true);assert.equal(h.run('tilesUnavailable'),true);
 h.tile('tileerror',current);assert.equal(h.run('retrying'),false);assert.equal(h.run('tilesUnavailable'),true);
 const previous=current;h.layer.onReload=()=>{h.tile('tileunload',previous);current={};h.tile('tileloadstart',current);};h.run('retryTiles()');h.tile('tileload',current);
 assert.equal(h.run('retrying'),false);assert.equal(h.run('tilesUnavailable'),false);assert.equal(h.reloads(),2);
 assert.deepEqual({...h.map.getCenter()},before.center);assert.equal(h.map.getZoom(),before.zoom);assert.deepEqual(h.markers[0].point,before.marker);assert.equal(h.views.length,before.views);assert.deepEqual(h.published,[]);
 const late=h.callbacks();h.destroy();late.tileloadstart({tile:{},coords:{x:0,y:0,z:0}});late.tileerror({tile:current,coords:{x:0,y:0,z:0}});h.run('retryTiles();updateViewSize()');assert.equal(h.run('tiles.size'),0);assert.equal(h.reloads(),2);
});

test('fractional retry reinitializes integer tiles, retains empty-batch warning and permits repeated attempts',async()=>{
 const h=await mountedMap();h.resize(1000,650);h.flush();assert.equal(h.map.getZoom(),1.3);
 const failed={};h.tile('tileloadstart',failed,{x:0,y:0,z:1});h.tile('tileerror',failed,{x:0,y:0,z:1});
 const before={center:{...h.map.getCenter()},zoom:h.map.getZoom(),views:h.views.length};
 h.run('retryTiles()');assert.equal(h.tileZoom(),1);assert.equal(h.run('retrying'),false);assert.equal(h.run('tilesUnavailable'),true);
 h.tile('load',{});assert.equal(h.run('tilesUnavailable'),true);
 let current;h.layer.onReload=z=>{current={};h.tile('tileloadstart',current,{x:0,y:0,z});};
 h.run('retryTiles();retryTiles()');assert.equal(h.reloads(),2);assert.equal(h.run('retrying'),true);assert.equal(h.run('tilesUnavailable'),true);
 h.tile('tileerror',failed,{x:0,y:0,z:1});assert.equal(h.run('retrying'),true);
 h.tile('tileerror',current,{x:0,y:0,z:1});assert.equal(h.run('retrying'),false);assert.equal(h.run('tilesUnavailable'),true);
 h.run('retryTiles()');h.tile('tileload',current,{x:0,y:0,z:1});assert.equal(h.run('retrying'),false);assert.equal(h.run('tilesUnavailable'),false);
 assert.equal(h.reloads(),3);assert.deepEqual({...h.map.getCenter()},before.center);assert.equal(h.map.getZoom(),before.zoom);assert.equal(h.views.length,before.views);assert.deepEqual(h.published,[]);assert.deepEqual(h.markers,[]);h.destroy();
});
