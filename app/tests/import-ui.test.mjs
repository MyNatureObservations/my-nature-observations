import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';
import { compile } from 'svelte/compiler';
const source = readFileSync(new URL('../src/routes/settings/import/+page.svelte', import.meta.url), 'utf8');
const layout = readFileSync(new URL('../src/routes/+layout.svelte', import.meta.url), 'utf8');
const script = s => ts.transpileModule(s.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, ''), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;
const messages = new Proxy({}, { get: (_, key) => () => key });
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
function harness(onBlock = () => {}) {
  let destroy, guard, blocks = 0;
  const calls = [];
  const context = vm.createContext({
    $state: v => v, getContext: key => { assert.equal(key, 'mno:block-journal'); return (prepared = false) => { blocks++; onBlock(prepared); }; },
    onDestroy: f => destroy = f, beforeNavigate: f => guard = f, language: { current: 'sv' }, m: messages,
    invoke: (name, args) => new Promise((resolve, reject) => calls.push({ name, args, resolve, reject })),
  });
  vm.runInContext(script(source), context);
  return { calls, run: code => vm.runInContext(code, context), destroy: () => destroy(), blocks: () => blocks,
    canLeave() { let cancelled = false; guard({ cancel() { cancelled = true; } }); return !cancelled; } };
}
async function selected(h, token = 'selection-A', count = 2) {
  const promise = h.run('choose()'); h.calls.at(-1).resolve({ token, entry_count: count }); await promise;
}
function layoutHarness() {
  let mount, event, block;
  const calls = [];
  const context = vm.createContext({ $state: v => v, $props: () => ({ children() {} }),
    PLACE_MAP: 'mno:place-map', createPlaceMap: () => ({}),
    setContext: (key, value) => { if (key === 'mno:place-map') return; assert.equal(key, 'mno:block-journal'); block = value; }, onMount: f => mount = f,
    initializeLanguage() {}, goto: async () => {}, afterNavigate() {}, page: { url: new URL('https://example.invalid/') },
    listen: (name, callback) => { assert.equal(name, 'mno:database-mode-changed'); event = callback; return Promise.resolve(() => {}); },
    invoke: name => new Promise((resolve, reject) => calls.push({ name, resolve, reject })),
  });
  vm.runInContext(script(layout), context);
  const destroy = mount();
  return { calls, event: () => event(), block: (prepared = false) => block(prepared), destroy, run: code => vm.runInContext(code, context) };
}

test('import view and global gate compile; settings link uses ordinary navigation', () => {
  compile(source, { generate: 'client' }); compile(layout, { generate: 'client' });
  const nav = readFileSync(new URL('../src/lib/components/AppNavigation.svelte', import.meta.url), 'utf8');
  assert.match(nav, /<a class="backup-button" href="\/settings\/import"(?:\s|>)/);
  assert.doesNotMatch(nav, /choose_open_import|activate_open_import/);
  assert.doesNotMatch(source, /database_mode|run_recovery|\.restart|goto\(/);
});
test('native picker cancellation is harmless and preview requires a backend selection', async () => {
  const h = harness(); await h.run('activate()'); assert.equal(h.calls.length, 0);
  const choosing = h.run('choose()'); await h.run('choose()');
  assert.equal(h.calls.length, 1, 'no duplicate picker'); assert.equal(h.calls[0].name, 'choose_open_import');
  h.calls[0].resolve(null); await choosing;
  assert.equal(h.run('phase'), 'idle'); assert.equal(h.run('notice'), 'cancelled');
  assert.equal(h.blocks(), 0); assert.equal(h.canLeave(), true);
  h.destroy(); assert.equal(h.calls.length, 1, 'no tokenless cancellation');
});
test('preview shows backend count and cancellation sends the exact token once', async () => {
  const h = harness(); await selected(h, 'exact-token', 7);
  assert.equal(h.run('phase'), 'confirmed'); assert.equal(h.run('entryCount'), 7);
  assert.equal(h.calls.length, 1, 'preview never activates');
  const cancelling = h.run('cancel()'); await h.run('cancel()'); assert.equal(h.calls.length, 2);
  assert.equal(h.calls[1].name, 'cancel_open_import'); assert.equal(h.calls[1].args.token, 'exact-token');
  h.calls[1].resolve(); await cancelling;
  assert.equal(h.run('token'), null); assert.equal(h.run('phase'), 'idle'); assert.equal(h.blocks(), 0);
});
test('invalid packages and occupied journals remain safe pre-activation errors', async () => {
  for (const error of ['invalid_package', 'occupied', 'busy', 'changed', new Error('IPC failed')]) {
    const h = harness(); const choosing = h.run('choose()'); h.calls[0].reject(error); await choosing;
    assert.equal(h.run('phase'), 'idle'); assert.equal(h.run('token'), null);
    assert.equal(h.blocks(), 0); assert.equal(h.canLeave(), true); assert.equal(h.calls.length, 1);
    assert.equal(h.run('error'), typeof error === 'string' ? error : 'failed');
  }
});
test('failed cancellation retains selection for retry and cannot activate concurrently', async () => {
  const h = harness(); await selected(h); const cancelling = h.run('cancel()');
  await h.run('activate()'); assert.equal(h.calls.length, 2);
  h.calls[1].reject('busy'); await cancelling;
  assert.equal(h.run('phase'), 'confirmed'); assert.equal(h.run('token'), 'selection-A');
  assert.equal(h.run('error'), 'cancel_failed');
  const retry = h.run('cancel()'); h.calls[2].resolve(); await retry; assert.equal(h.run('token'), null);
});
test('late picker response is released by its own token without changing a newer view', async () => {
  const old = harness(); const choosing = old.run('choose()'); old.destroy();
  const newer = harness(); await selected(newer, 'new-B');
  old.calls[0].resolve({ token: 'old-A', entry_count: 99 }); await choosing;
  assert.equal(old.calls[1].name, 'cancel_open_import'); assert.equal(old.calls[1].args.token, 'old-A');
  old.calls[1].resolve(); await settle();
  assert.equal(old.run('phase'), 'checking', 'disposed view receives no state update');
  assert.equal(newer.run('token'), 'new-B'); assert.equal(newer.run('entryCount'), 2);
});
test('unmount and late cancellation are token-bound and cannot update a newer view', async () => {
  const old = harness(); await selected(old, 'old-A'); const cancelling = old.run('cancel()'); old.destroy();
  const newer = harness(); await selected(newer, 'new-B');
  for (const call of old.calls.slice(1)) {
    assert.equal(call.name, 'cancel_open_import'); assert.equal(call.args.token, 'old-A'); call.resolve();
  }
  await cancelling;
  assert.equal(newer.run('token'), 'new-B'); assert.equal(newer.run('phase'), 'confirmed');
  assert.equal(old.run('phase'), 'cancelling');
});
test('double confirmation activates once, blocks navigation, and does not cancel the consumed token', async () => {
  const h = harness(); await selected(h); const activation = h.run('activate()');
  await h.run('activate()'); await h.run('choose()'); await h.run('cancel()');
  assert.equal(h.calls.length, 2); assert.equal(h.calls[1].name, 'activate_open_import');
  assert.equal(h.calls[1].args.token, 'selection-A'); assert.equal(h.canLeave(), false);
  h.calls[1].resolve({ activated: true, error: null, mode: 'blocked' }); await activation;
  assert.equal(h.blocks(), 1); assert.equal(h.run('phase'), 'blocked'); assert.equal(h.canLeave(), false);
  h.destroy(); assert.equal(h.calls.length, 2);
});
test('post-session errors, unknown outcomes, malformed replies and IPC failure stay blocked', async () => {
  for (const reply of [
    { activated: false, error: 'failed', mode: 'blocked' }, { activated: false, error: 'busy', mode: 'unknown' },
    { activated: false, error: 'failed', mode: 'recovery' }, { activated: true, error: null, mode: 'ready' }, null, {}, 'ipc-rejection',
  ]) {
    const h = harness(); await selected(h); const activation = h.run('activate()');
    if (reply === 'ipc-rejection') h.calls[1].reject(new Error('lost reply')); else h.calls[1].resolve(reply);
    await activation;
    assert.equal(h.blocks(), 1); assert.equal(h.run('phase'), 'blocked'); assert.equal(h.canLeave(), false);
    assert.equal(h.calls.length, 2, 'no polling, automatic quit or relaunch');
  }
});
test('only an explicit Ready pre-session failure permits another attempt', async () => {
  const h = harness(); await selected(h); const activation = h.run('activate()');
  h.calls[1].resolve({ activated: false, error: 'changed', mode: 'ready' }); await activation;
  assert.equal(h.blocks(), 0); assert.equal(h.run('phase'), 'idle'); assert.equal(h.run('error'), 'changed');
  assert.equal(h.run('token'), null); assert.equal(h.canLeave(), true);
});
test('global blocking ignores a stale Ready response and does not poll during Maintenance', async () => {
  const root = layoutHarness(); await settle(); assert.equal(root.calls.length, 1);
  const h = harness(root.block); await selected(h); const activation = h.run('activate()');
  h.calls[1].reject(new Error('uncertain IPC')); await activation; assert.equal(root.run('mode'), 'blocked');
  root.calls[0].resolve('ready'); await settle(); root.event(); await settle();
  assert.equal(root.run('mode'), 'blocked'); assert.equal(root.calls.length, 1);
});
test('backend mode event before activation reply unmounts page and cannot be undone by the reply', async () => {
  const root = layoutHarness(); await settle(); root.calls[0].resolve('ready'); await settle();
  const h = harness(root.block); await selected(h); const activation = h.run('activate()');
  root.event(); await settle(); assert.equal(root.calls[1].name, 'database_mode');
  root.calls[1].resolve('blocked'); await settle(); assert.equal(root.run('mode'), 'blocked'); h.destroy();
  h.calls[1].resolve({ activated: false, error: 'changed', mode: 'ready' }); await activation;
  assert.equal(root.run('mode'), 'blocked'); assert.equal(h.calls.length, 2);
});
test('a fresh startup Recovery flow can still become Ready after verified completion', async () => {
  const root = layoutHarness(); await settle(); root.calls[0].resolve('recovery'); await settle();
  assert.equal(root.run('mode'), 'recovery');
  root.event(); await settle(); root.calls[1].resolve('ready'); await settle();
  assert.equal(root.run('mode'), 'ready', 'local restriction does not disable verified startup admission');
});
test('both locales provide import messages, count placeholder, and neutral reopening instructions', () => {
  const sv = JSON.parse(readFileSync(new URL('../messages/sv.json', import.meta.url), 'utf8'));
  const en = JSON.parse(readFileSync(new URL('../messages/en.json', import.meta.url), 'utf8'));
  const keys = Object.keys(sv).filter(key => key.startsWith('import_')); assert.ok(keys.length >= 15);
  for (const key of keys) { assert.equal(typeof en[key], 'string'); assert.ok(sv[key]); assert.ok(en[key]); }
  for (const messages of [sv, en]) {
    assert.match(messages.import_preview, /\{count\}/);
    assert.doesNotMatch(messages.restore_reopen_instructions, /restoration|återställningen/i);
  }
});

test('replacement preview keeps both counts and requires explicit confirmation', async () => {
  const h = harness(); const choosing = h.run('choose()');
  h.calls[0].resolve({ token: 'replace-only', entry_count: 42, existing_entry_count: 37 }); await choosing;
  assert.equal(h.run('existingEntryCount'), 37); assert.equal(h.run('entryCount'), 42);
  assert.equal(h.calls.length, 1, 'preview cannot prepare an episode');
  assert.match(source, /existingEntryCount > 0/);
  assert.match(source, /import_current_count\(\{ count: existingEntryCount \}/);
  assert.match(source, /import_incoming_count\(\{ count: entryCount \}/);
  assert.match(source, /function hasExistingData\(\)[\s\S]*existingSettingCount > 0/);
  assert.match(source, /phase === "confirmed" && hasExistingData\(\) \? m.import_replace_title/);
  assert.match(source, /import_replace_valid/);
  assert.match(source, /import_replace_startup/);
  assert.match(source, /import_replace_confirm/); assert.match(source, /import_replace_button/);
  const activating = h.run('activate()'); assert.equal(h.calls[1].args.token, 'replace-only');
  h.calls[1].resolve({ activated: false, prepared: true, error: null, mode: 'blocked' }); await activating;
  assert.equal(h.blocks(), 1); assert.equal(h.run('phase'), 'blocked'); assert.equal(h.canLeave(), false);
  assert.equal(h.run('notice'), '', 'no completed import notice');
});
test('format-4 place-only journal uses replacement confirmation and shows place counts', async () => {
  const h = harness(); const choosing = h.run('choose()');
  h.calls[0].resolve({ token: 'places-only', format_version: 4, entry_count: 0,
    existing_entry_count: 0, existing_place_count: 2, imported_place_count: 1 });
  await choosing;
  assert.equal(h.run('existingPlaceCount'), 2);
  assert.equal(h.run('importedPlaceCount'), 1);
  assert.equal(h.run('formatVersion'), 4);
  assert.equal(h.calls.length, 1, 'preview does not activate');
  for (const key of ['import_current_places', 'import_incoming_places', 'import_replace_places', 'import_replace_button_v3', 'import_replace_startup_v3']) {
    assert.match(source, new RegExp(key));
    for (const locale of ['sv', 'en']) {
      const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
      assert.equal(typeof messages[key], 'string');
    }
  }
  const activation = h.run('activate()');
  assert.equal(h.calls[1].name, 'activate_open_import');
  h.calls[1].resolve({ activated: true, prepared: false, error: null, mode: 'blocked' });
  await activation;
  assert.equal(h.blocks(), 1);
});
test('a prepared result can never be treated as a safe Ready error', async () => {
  const h = harness(); await selected(h); const activating = h.run('activate()');
  h.calls[1].resolve({ activated: false, prepared: true, error: 'failed', mode: 'ready' }); await activating;
  assert.equal(h.blocks(), 1); assert.equal(h.run('phase'), 'blocked');
});


test('prepared reply after the mode event preserves its notice without granting Ready', async () => {
  const root = layoutHarness(); await settle(); root.calls[0].resolve('ready'); await settle();
  const h = harness(prepared => root.block(prepared)); await selected(h);
  const activating = h.run('activate()');
  root.event(); await settle(); root.calls[1].resolve('blocked'); await settle();
  h.destroy(); // The global gate removed the page before its command reply.
  h.calls[1].resolve({ activated: false, prepared: true, error: null, mode: 'blocked' }); await activating;
  assert.equal(root.run('mode'), 'blocked'); assert.equal(root.run('replacementPrepared'), true);
  root.event(); await settle();
  assert.equal(root.run('mode'), 'blocked'); assert.equal(root.calls.length, 2);
  assert.match(layout, /prepared=\{replacementPrepared\}/);
});
test('failed or uncertain activation never claims preparation succeeded', async () => {
  for (const result of [
    {activated:false,prepared:false,error:'failed',mode:'blocked'},
    {activated:false,prepared:true,error:'failed',mode:'blocked'},
    {activated:false,prepared:true,error:null,mode:'unknown'},
  ]) {
    const notices=[]; const h=harness(prepared=>notices.push(prepared)); await selected(h);
    const activating=h.run('activate()');h.calls[1].resolve(result);await activating;
    assert.equal(h.run('phase'),'blocked');assert.equal(notices.includes(true),false);
  }
});
