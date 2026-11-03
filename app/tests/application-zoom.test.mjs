import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
import { compile } from 'svelte/compiler';
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const source = read('../src/lib/application-zoom.ts');
const js = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { createZoomController, routeZoomKey, ZOOM_STORAGE_KEY } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
const menuSource = read('../src/lib/native-menu-labels.ts');
const menuJs = ts.transpileModule(menuSource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
  .replace('"$lib/paraglide/messages.js"', JSON.stringify(new URL('../src/lib/paraglide/messages.js', import.meta.url).href));
const { nativeMenuLabels } = await import(`data:text/javascript;base64,${Buffer.from(menuJs).toString('base64')}`);
function harness(saved = null, overrides = {}) {
  const calls = [], writes = [], failures = [], applied = [];
  const controller = createZoomController({
    setZoom: async level => { calls.push(level); },
    storage: () => ({ getItem: key => { assert.equal(key, ZOOM_STORAGE_KEY); return saved; }, setItem: (key, value) => { assert.equal(key, ZOOM_STORAGE_KEY); writes.push(value); } }),
    onFailure: () => failures.push(true), onApplied: () => applied.push(true), ...overrides
  });
  return { controller, calls, writes, failures, applied };
}
test('all levels, boundaries, reset and idempotent initialization', async () => {
  const h = harness();
  await h.controller.initialize(); await h.controller.initialize();
  await h.controller.request('out');
  for (let i = 0; i < 6; i++) await h.controller.request('in');
  assert.equal(h.controller.level, 2);
  for (let i = 0; i < 6; i++) await h.controller.request('out');
  assert.equal(h.controller.level, 1);
  await h.controller.request('in'); await h.controller.request('reset');
  assert.deepEqual(h.calls, [1, 1.25, 1.5, 1.75, 2, 1.75, 1.5, 1.25, 1, 1.25, 1]);
});
for (const saved of ['1', '1.25', '1.5', '1.75', '2']) {
  test(`restore and persist ${saved} across controller instances`, async () => {
    const h = harness(saved); await h.controller.initialize();
    assert.equal(h.controller.level, Number(saved));
    const next = harness(h.writes.at(-1)); await next.controller.initialize();
    assert.equal(next.controller.level, Number(saved));
  });
}
test('missing, unsupported and malformed preferences safely become 100%', async () => {
  for (const value of [null, '', 'NaN', 'Infinity', '0', '-1', '3', '1.1', '{}', ' 2 ', '2x']) {
    const h = harness(value); await h.controller.initialize();
    assert.deepEqual(h.calls, [1]); assert.equal(h.writes.at(-1), '1');
  }
});
test('unavailable storage does not prevent session zoom', async () => {
  const h = harness(null, { storage: () => { throw Error('denied'); } });
  await h.controller.request('in'); assert.equal(h.controller.level, 1.25);
  assert.deepEqual(h.failures, []);
});
test('API failure preserves applied level and stored preference; next action recovers', async () => {
  let fail = false;
  const h = harness(null, { setZoom: async () => { if (fail) throw Error('API'); } });
  await h.controller.initialize(); fail = true;
  await h.controller.request('in');
  assert.equal(h.controller.level, 1); assert.deepEqual(h.writes, ['1']);
  assert.equal(h.failures.length, 1);
  fail = false; await h.controller.request('in');
  assert.equal(h.controller.level, 1.25); assert.equal(h.applied.length, 2);
});
test('failed restoration can be retried through reset', async () => {
  let fail = true;
  const h = harness('2', { setZoom: async () => { if (fail) throw Error('API'); } });
  await h.controller.initialize(); assert.equal(h.writes.length, 0);
  fail = false; await h.controller.request('reset');
  assert.equal(h.controller.level, 1); assert.deepEqual(h.writes, ['1']);
});
test('rapid requests serialize after restoration and use successfully applied state', async () => {
  let active = 0, maxActive = 0;
  const h = harness('1.5', { setZoom: async () => {
    active++; maxActive = Math.max(maxActive, active);
    await new Promise(resolve => setTimeout(resolve, 1)); active--;
  } });
  await Promise.all([h.controller.request('in'), h.controller.request('out'), h.controller.request('in')]);
  assert.equal(maxActive, 1); assert.equal(h.controller.level, 1.75);
  assert.deepEqual(h.writes, ['1.5', '1.75', '1.5', '1.75']);
});
function keyEvent(key, fields = {}) {
  return { key, metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, isComposing: false,
    repeat: false, prevented: 0, stopped: 0,
    preventDefault() { this.prevented++; }, stopImmediatePropagation() { this.stopped++; }, ...fields };
}
test('Mac and Ctrl shortcuts route plus/equal/minus/reset exactly once', () => {
  for (const mac of [true, false]) {
    for (const [key, action] of [['+', 'in'], ['=', 'in'], ['-', 'out'], ['0', 'reset']]) {
      const calls = [], event = keyEvent(key, { metaKey: mac, ctrlKey: !mac, shiftKey: key === '+' });
      routeZoomKey(event, mac, action => calls.push(action));
      assert.deepEqual(calls, [action]); assert.equal(event.prevented, 1); assert.equal(event.stopped, 1);
    }
  }
});
test('repeat is consumed without another action; unrelated editing shortcuts are untouched', () => {
  const calls = [], repeat = keyEvent('+', { repeat: true });
  routeZoomKey(repeat, true, action => calls.push(action)); assert.equal(repeat.prevented, 1);
  for (const event of [keyEvent('z'), keyEvent('b'), keyEvent('+', { metaKey: false }),
    keyEvent('+', { altKey: true }), keyEvent('+', { isComposing: true }), keyEvent('0', { shiftKey: true }),
    keyEvent('+', { ctrlKey: true }), keyEvent('+', { metaKey: false, ctrlKey: true })]) {
    routeZoomKey(event, true, action => calls.push(action)); assert.equal(event.prevented, 0);
  }
  assert.deepEqual(calls, []);
});
test('application integration owns shortcuts once and keeps zoom outside conditional startup UI', async () => {
  const component = read('../src/lib/components/ApplicationZoom.svelte');
  compile(component, { generate: 'client' });
  assert.match(component, /getCurrentWebview\(\)\.setZoom\(level\)/);
  assert.match(component, /addEventListener\("keydown", keydown, true\)/);
  assert.match(component, /removeEventListener\("keydown", keydown, true\)/);
  assert.match(component, /listen<string>\("mno:zoom"/);
  assert.match(component, /role="status"/);
  const layout = read('../src/routes/+layout.svelte');
  assert.ok(layout.indexOf('<ApplicationZoom />') < layout.indexOf('{#if mode === "ready"}'));
  const menu = read('../src-tauri/src/macos_menu.rs');
  assert.match(menu, /Menu::with_items/); assert.match(menu, /window\.close\(\)/);
  assert.match(menu, /id, "", false, None::<&str>/); // no competing native accelerator
  const config = JSON.parse(read('../src-tauri/tauri.conf.json'));
  assert.equal(config.app.windows[0].zoomHotkeysEnabled, false);
  const capability = JSON.parse(read('../src-tauri/capabilities/default.json'));
  assert.deepEqual(capability.windows, ['main']);
  assert.ok(capability.permissions.includes('core:webview:allow-set-webview-zoom'));
  const messages = await import('../src/lib/paraglide/messages.js');
  for (const locale of ['sv', 'en']) for (const key of ['zoom_in', 'zoom_out', 'zoom_actual_size', 'zoom_failed']) {
    assert.ok(messages[key]({}, { locale }));
  }
  assert.equal(messages.zoom_in({}, {locale: 'sv'}), 'Zooma in');
  assert.equal(messages.zoom_in({}, {locale: 'en'}), 'Zoom In');
});

function componentHarness({ delayed = false, updateMenu = async () => {} } = {}) {
  const effects = [], keys = new Map(), zooms = [], labels = [];
  let mount, callback, resolveListener, stops = 0;
  const context = vm.createContext({
    $state: value => value, $effect: fn => effects.push(fn), onMount: fn => { mount = fn; },
    createZoomController, routeZoomKey, nativeMenuLabels,
    getCurrentWebview: () => ({ setZoom: async value => { zooms.push(value); } }),
    localStorage: { getItem: () => '1.5', setItem: () => {} },
    navigator: { platform: 'MacIntel' }, language: { current: 'en' },
    m: Object.fromEntries(['zoom_in', 'zoom_out', 'zoom_actual_size'].map(key => [key, (_, {locale}) => `${locale}:${key}`])),
    invoke: async (command, args) => { assert.equal(command, 'update_native_menu_labels'); labels.push(args.labels); await updateMenu(args.labels); },
    listen: (event, fn) => {
      assert.equal(event, 'mno:zoom'); callback = fn;
      return new Promise(resolve => { resolveListener = () => resolve(() => { stops++; }); if (!delayed) resolveListener(); });
    },
    window: { addEventListener: (name, fn, capture) => { assert.equal(capture, true); keys.set(name, fn); },
      removeEventListener: (name, fn, capture) => { assert.equal(keys.get(name), fn); assert.equal(capture, true); keys.delete(name); } }
  });
  const script = read('../src/lib/components/ApplicationZoom.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
  const cleanup = mount();
  return { zooms, labels, keys, cleanup, effects, context, resolveListener: () => resolveListener(),
    menu: payload => callback({payload}), stops: () => stops };
}
const settle = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
test('actual component shares restored state across menu and keyboard, updates locale, and cleans up', async () => {
  const h = componentHarness(); await settle();
  h.effects.forEach(fn => fn()); await settle();
  assert.deepEqual(h.zooms, [1.5]);
  assert.equal(h.labels[0].zoomIn, 'Zoom In');
  h.menu('in'); h.keys.get('keydown')(keyEvent('-')); await settle();
  assert.deepEqual(h.zooms, [1.5, 1.75, 1.5]);
  h.menu('unknown'); await settle(); assert.equal(h.zooms.length, 3);
  vm.runInContext('language.current = "sv"', h.context); h.effects.forEach(fn => fn()); await settle();
  assert.equal(h.labels.at(-1).actualSize, 'Verklig storlek');
  h.cleanup(); assert.equal(h.keys.size, 0); assert.equal(h.stops(), 1);
  h.menu('in'); await settle(); assert.equal(h.zooms.length, 3);
});
test('listener resolving after unmount is stopped once without enabling menus', async () => {
  const h = componentHarness({delayed: true});
  h.cleanup(); h.resolveListener(); await settle(); h.effects.forEach(fn => fn()); await settle();
  assert.equal(h.stops(), 1); assert.equal(h.labels.length, 0);
});

test('all retained native handles have complete English and Swedish labels', () => {
  const menu = read('../src-tauri/src/macos_menu.rs');
  const keys = new Set([...menu.matchAll(/(?:native!|heading!)\(\s*"([^"]+)"/g)].map(match => match[1]));
  for (const key of ['quit', 'zoomIn', 'zoomOut', 'actualSize']) keys.add(key);
  for (const locale of ['en', 'sv']) {
    const labels = nativeMenuLabels(locale);
    assert.deepEqual(Object.keys(labels).sort(), [...keys].sort());
    for (const label of Object.values(labels)) assert.ok(label.trim());
  }
  assert.deepEqual(['file','edit','view','window','help'].map(key => nativeMenuLabels('sv')[key]),
    ['Arkiv','Redigera','Visa','Fönster','Hjälp']);
  assert.deepEqual(['file','edit','view','window','help'].map(key => nativeMenuLabels('en')[key]),
    ['File','Edit','View','Window','Help']);
  assert.equal(nativeMenuLabels('sv').quit, 'Avsluta MNO');
  assert.equal(nativeMenuLabels('en').quit, 'Quit MNO');
});
test('localization changes retained labels only, preserving native actions and stable identification', () => {
  const menu = read('../src-tauri/src/macos_menu.rs');
  assert.doesNotMatch(menu, /submenu\.text\(\)|== "View"/);
  const update = menu.slice(menu.indexOf('pub fn update_labels'));
  assert.doesNotMatch(update, /with_items|with_id|append\(|on_menu_event|close\(/);
  assert.match(update, /state::<LocalizedMenu>/);
  assert.match(menu, /&app_menu, &file, &edit, &view, &window, &help/);
  assert.match(menu, /WINDOW_SUBMENU_ID/); assert.match(menu, /HELP_SUBMENU_ID/);
  for (const action of ['about','services','hide','hide_others','close_window','undo','redo','cut','copy','paste','select_all','fullscreen','minimize','maximize']) {
    assert.ok(menu.includes(`PredefinedMenuItem::${action}(`), action);
  }
  assert.match(menu, /QUIT_ID, quit_title, true, Some\("Cmd\+Q"\)/);
  assert.match(menu, /window\.close\(\)/);
  assert.match(menu, /emit_to\(window\.label\(\), "mno:zoom"/);
});
test('repeated language switching updates all labels without changing zoom or registering more handlers', async () => {
  const h = componentHarness(); await settle();
  for (const locale of ['sv','en','sv','en','sv','en']) {
    vm.runInContext(`language.current = "${locale}"`, h.context);
    h.effects.forEach(fn => fn()); await settle();
    assert.equal(h.labels.at(-1).view, locale === 'sv' ? 'Visa' : 'View');
    assert.equal(h.labels.at(-1).undo, locale === 'sv' ? 'Ångra' : 'Undo');
    assert.equal(h.keys.size, 1);
    h.menu('in'); await settle(); h.menu('out'); await settle();
  }
  assert.equal(h.labels.length, 6);
  assert.deepEqual(h.zooms, [1.5, ...Array.from({length: 6}, () => [1.75, 1.5]).flat()]);
  h.cleanup(); assert.equal(h.stops(), 1);
});
test('menu update errors are recoverable and do not change zoom state', async () => {
  let fail = true;
  const h = componentHarness({updateMenu: async () => { if (fail) throw Error('menu'); }});
  await settle(); h.effects.forEach(fn => fn()); await settle();
  assert.equal(vm.runInContext('menuFailed', h.context), true);
  assert.equal(vm.runInContext('failed', h.context), false);
  fail = false; vm.runInContext('language.current = "sv"', h.context);
  h.effects.forEach(fn => fn()); await settle();
  assert.equal(vm.runInContext('menuFailed', h.context), false);
  assert.equal(h.labels.at(-1).file, 'Arkiv');
  assert.deepEqual(h.zooms, [1.5]); h.cleanup();
});
test('rapid locale changes serialize native updates and finish in the latest language', async () => {
  let active = 0, maximum = 0;
  const applied = [];
  const h = componentHarness({updateMenu: async labels => {
    active++; maximum = Math.max(maximum, active);
    await new Promise(resolve => setTimeout(resolve, 1));
    applied.push(labels.file); active--;
  }});
  await settle();
  for (const locale of ['sv', 'en', 'sv']) {
    vm.runInContext(`language.current = "${locale}"`, h.context); h.effects.forEach(fn => fn());
  }
  await vm.runInContext('menuUpdates', h.context);
  assert.equal(maximum, 1); assert.deepEqual(applied, ['Arkiv', 'File', 'Arkiv']);
  h.cleanup();
});
