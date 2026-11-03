import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { compile } from 'svelte/compiler';
const source = readFileSync(new URL('../src/lib/components/WeatherDefaultPlace.svelte', import.meta.url), 'utf8');
function harness(initial = { place: null, revision: '0' }) {
  let mount, destroy, saved = initial, failure = null;
  const writes = [];
  const context = vm.createContext({
    $state: value => value, $derived: value => value,
    getContext: () => null, PLACE_MAP: 'mno:place-map',
    onMount: f => mount = f, onDestroy: f => destroy = f, tick: async () => {},
    language: { current: 'sv' }, m: {},
    filterPlaces: places => places,
    mapDraftCoordinates: () => [0, 0],
    invoke: async () => ({ generation: 7 }),
    readWeatherDefaultPlace: async () => saved,
    listPlaces: async () => [],
    setWeatherDefaultPlace: async (...args) => {
      writes.push(args); if (failure) throw failure;
      return { place: args[2] ? { id: args[2], name: 'Lake', latitude: 0, longitude: 0 } : null, revision: '2' };
    },
  });
  const script = source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  return { run: code => vm.runInContext(code, context), writes, destroy: () => destroy(), fail: code => failure = code, saved: value => saved = value };
}
test('component compiles and offers only coordinate-bearing places', () => {
  compile(source, { generate: 'client' });
  const h = harness();
  assert.equal(h.run('eligible({latitude:null,longitude:null})'), false);
  assert.equal(h.run('eligible({latitude:0,longitude:0})'), true);
  assert.doesNotMatch(source, /setJournalPlace|fetch_historical_weather|createPlace/);
});
test('load, choose, cancel, save, change and clear use confirmed revisions', async () => {
  const h = harness(); await h.run('load()');
  assert.equal(h.run('value.revision'), '0');
  await h.run('choose()'); assert.equal(h.run('open'), true);
  h.run('close()'); assert.equal(h.writes.length, 0);
  await h.run('save("a")'); assert.equal(h.run('value.place.id'), 'a');
  assert.deepEqual(h.writes[0], [7, '0', 'a']);
  await h.run('save("b")'); assert.equal(h.run('value.place.id'), 'b');
  await h.run('save(null)'); assert.equal(h.run('value.place'), null);
  assert.equal(h.writes[2][1], '2');
});
test('write errors preserve the displayed value; conflicts reload current setting', async () => {
  for (const failure of ['failed', 'place_revision_conflict', 'journal_generation_conflict']) {
    const old = { place: { id: 'old', name: 'Old' }, revision: '4' };
    const h = harness(old); await h.run('load()'); h.fail(failure);
    await h.run('save("new")');
    assert.equal(h.run('value.place.id'), 'old');
    assert.equal(h.run('error'), failure === 'failed' ? 'failed' : 'conflict');
    h.saved({ place: null, revision: '5' }); await h.run('load()');
    assert.equal(h.run('value.revision'), '5');
  }
});
test('disposed component ignores a late read', async () => {
  const h = harness(); h.destroy(); await h.run('load()');
  assert.equal(h.run('generation'), null);
});

test('settings terminology is localized consistently without changing weather actions', async () => {
  const navigation = readFileSync(new URL('../src/lib/components/AppNavigation.svelte', import.meta.url), 'utf8');
  assert.match(navigation, /m\.nav_language\(\{\}, \{ locale: language\.current \}\)/);
  assert.match(navigation, /<WeatherDefaultPlace \/>/);
  const compiledMessages = await import('../src/lib/paraglide/messages.js');
  for (const [locale, title, change, clear] of [
    ['sv', 'Språk och lokalisering', 'Byt plats', 'Rensa'],
    ['en', 'Language and localization', 'Change place', 'Clear'],
  ]) {
    const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
    assert.equal(messages.nav_language, title);
    assert.equal(compiledMessages.nav_language({}, { locale }), title);
    assert.equal(messages.weather_default_change, change);
    assert.equal(messages.weather_default_clear, clear);
  }
});
