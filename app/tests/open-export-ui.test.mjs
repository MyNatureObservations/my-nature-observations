import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';
import { compile, parse } from 'svelte/compiler';
const component = readFileSync(new URL('../src/lib/components/AppNavigation.svelte', import.meta.url), 'utf8');
const script = ts.transpileModule(component.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, ''), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;
const reader = ts.transpileModule(readFileSync(new URL('../src/lib/journal-data.ts', import.meta.url), 'utf8').replace(/^import .*;\n/gm, '').replace(/^export /gm, ''), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;
const translations = Object.fromEntries(['sv','en'].map(locale => [locale, JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'))]));
function harness() {
  const calls = [];
  const context = vm.createContext({ $state: v => v, $derived: v => v, $effect() {}, afterNavigate() {}, language: { current: 'sv' },
    PLACE_MAP: 'mno:place-map', getContext: () => ({ close() {} }),
    m: new Proxy({}, { get: (_, key) => (_args, options) => translations[options.locale][key] }),
    invoke: (name, args) => {
      assert.equal(name, 'create_open_export');
      return new Promise((resolve, reject) => calls.push({ name, args, resolve, reject }));
    },
  });
  vm.runInContext(reader, context); vm.runInContext(script, context);
  return { calls, run: code => vm.runInContext(code, context) };
}
test('export working state prevents double clicks and confirms only completed export', async () => {
  const h = harness(); const operation = h.run('createOpenExport()');
  assert.equal(h.run('isExporting'), true); assert.equal(h.run('exportStatusMessage()'), translations.sv.export_creating);
  await h.run('createOpenExport()'); assert.equal(h.calls.length, 1);
  assert.match(h.calls[0].args.suggestedName, /^MNO-journal-\d{4}-\d{2}-\d{2}_\d{4}\.mno-export$/);
  assert.equal(h.calls[0].args.dialogTitle, translations.sv.export_create);
  assert.notEqual(h.run('exportStatus'), 'created');
  h.calls[0].resolve('created'); await operation;
  assert.equal(h.run('isExporting'), false); assert.equal(h.run('exportStatusMessage()'), translations.sv.export_created);
  h.run('language.current = "en"'); assert.equal(h.run('exportStatusMessage()'), translations.en.export_created);
});
test('cancelled save dialog is silent and permits another export', async () => {
  const h = harness(); const operation = h.run('createOpenExport()'); h.calls[0].resolve('cancelled'); await operation;
  assert.equal(h.run('exportStatus'), 'idle'); assert.equal(h.run('exportStatusMessage()'), ''); assert.equal(h.run('isExporting'), false);
  const next = h.run('createOpenExport()'); assert.equal(h.calls.length, 2); h.calls[1].resolve('created'); await next;
});
for (const code of ['already_exists','invalid_destination','invalid_content','busy','failed','unexpected']) {
  test(`export error ${code} is understandable and never reports success`, async () => {
    const h = harness(); const operation = h.run('createOpenExport()'); h.calls[0].reject(code); await operation;
    const expected = code === 'unexpected' ? 'failed' : code;
    assert.equal(h.run('exportStatus'), expected); assert.equal(h.run('isExporting'), false);
    assert.equal(h.run('exportStatusMessage()'), translations.sv[`export_${expected}`]);
    const retry = h.run('createOpenExport()'); h.calls[1].resolve('cancelled'); await retry;
    assert.equal(h.run('exportStatus'), 'idle');
  });
}
test('export filename uses a local date with zero padding', () => {
  const h = harness();
  assert.equal(h.run('suggestedExportFilename(new Date(2026, 0, 2, 3, 4))'), 'MNO-journal-2026-01-02_0304.mno-export');
});
test('export is a native accessible settings button with description and live status', () => {
  compile(component, { generate: 'client' });
  const ast = parse(component, { modern: true }); const buttons = [];
  function walk(value) {
    if (!value || typeof value !== 'object') return;
    if (value.type === 'RegularElement' && value.name === 'button') buttons.push(value);
    for (const child of Object.values(value)) {
      if (Array.isArray(child)) child.forEach(walk); else if (child && typeof child === 'object') walk(child);
    }
  }
  walk(ast.fragment);
  const button = buttons.find(b => b.attributes.some(a => a.name === 'onclick' && component.slice(a.start, a.end).includes('createOpenExport')));
  assert.ok(button);
  assert.equal(button.attributes.find(a => a.name === 'type').value[0].data, 'button');
  assert.equal(button.attributes.find(a => a.name === 'aria-describedby').value[0].data, 'open-export-description');
  assert.match(component.slice(button.start, button.end), /disabled=\{isExporting \|\| isCreatingBackup\}/);
  assert.match(component, /role=\{\["creating", "created"\]\.includes\(exportStatus\) \? "status" : "alert"\}/);
});
test('both languages describe saved data and exclude unsaved text and external originals', () => {
  for (const locale of ['sv','en']) {
    for (const key of ['create','description','creating','created','already_exists','invalid_destination','invalid_content','busy','failed']) assert.ok(translations[locale][`export_${key}`]);
  }
  assert.match(translations.sv.export_description, /Osparade ändringar och externa originalbilder ingår inte/);
  assert.match(translations.en.export_description, /Unsaved changes and external original images are not included/);
});


test('one disclosure contains backup and transfer groups with all four operations', () => {
  const ast = parse(component, { modern: true });
  const elements = [];
  function walk(value) {
    if (!value || typeof value !== 'object') return;
    if (value.type === 'RegularElement') elements.push(value);
    for (const child of Object.values(value)) {
      if (Array.isArray(child)) child.forEach(walk); else if (child && typeof child === 'object') walk(child);
    }
  }
  walk(ast.fragment);
  const disclosure = elements.find(n => n.name === 'details' && n.attributes.some(a => a.name === 'class' && a.value[0]?.data === 'data-security-details'));
  const groups = disclosure.fragment.nodes.filter(n => n.type === 'RegularElement' && n.name === 'section');
  assert.equal(groups.length, 2);
  assert.equal(disclosure.fragment.nodes.filter(n => n.type === 'RegularElement' && n.name === 'details').length, 0);
  const [backup, transfer] = groups.map(n => component.slice(n.start, n.end));
  assert.match(backup, /backup_heading/); assert.match(backup, /settings_backup_description/);
  assert.match(backup, /onclick=\{createManualBackup\}/); assert.match(backup, /href="\/settings\/restore"/);
  assert.doesNotMatch(backup, /createOpenExport|settings_import/);
  assert.match(transfer, /settings_transfer_heading/); assert.match(transfer, /settings_transfer_description/);
  assert.match(transfer, /onclick=\{createOpenExport\}/); assert.match(transfer, /href="\/settings\/import"/);
  assert.ok(transfer.indexOf('exportStatusMessage()') > transfer.indexOf('onclick={createOpenExport}'));
  assert.ok(transfer.indexOf('exportStatusMessage()') > transfer.indexOf('href="/settings/import"'));
  assert.match(transfer, /id="open-export-description"/);
  for (const locale of ['sv','en']) {
    for (const key of ['settings_backup_description','settings_transfer_heading','settings_transfer_description','settings_export','settings_import','import_verified']) assert.ok(translations[locale][key]);
  }
});

test('new export clears prior backup success but preserves errors and blocks concurrent backup', async () => {
  const h = harness();
  h.run('backupStatus = "created"; exportStatus = "created"');
  const operation = h.run('createOpenExport()');
  assert.equal(h.run('backupStatus'), 'idle'); assert.equal(h.run('exportStatus'), 'creating');
  await h.run('createManualBackup()'); assert.equal(h.calls.length, 1);
  h.calls[0].resolve('created'); await operation;
  h.run('backupStatus = "failed"; clearCompletedStatuses()');
  assert.equal(h.run('backupStatus'), 'failed'); assert.equal(h.run('exportStatus'), 'idle');
});


test('settings actions share one quiet button style while navigation remains native links', () => {
  const ast = parse(component, { modern: true }); const rows = [];
  function walk(value) {
    if (!value || typeof value !== 'object') return;
    if (value.type === 'RegularElement' && value.name === 'div' && value.attributes.some(a => a.name === 'class' && a.value[0]?.data === 'data-actions')) rows.push(value);
    for (const child of Object.values(value)) {
      if (Array.isArray(child)) child.forEach(walk); else if (child && typeof child === 'object') walk(child);
    }
  }
  walk(ast.fragment);assert.equal(rows.length, 2);
  for (const [index,row] of rows.entries()) {
    const actions=row.fragment.nodes.filter(n=>n.type==='RegularElement');
    assert.deepEqual(actions.map(n=>n.name), ['button','a']);
    for (const action of actions) {
      assert.equal(action.attributes.find(a=>a.name==='class').value[0].data,'backup-button');
      assert.equal(action.attributes.some(a=>['role','tabindex'].includes(a.name)),false);
    }
    assert.equal(actions[0].attributes.find(a=>a.name==='type').value[0].data,'button');
    assert.equal(actions[1].attributes.find(a=>a.name==='href').value[0].data,index===0?'/settings/restore':'/settings/import');
    assert.match(component.slice(actions[1].start,actions[1].end),/onclick=\{clearCompletedStatuses\}/);
    assert.match(component.slice(actions[0].start,actions[0].end), index===0?/m\.backup_create/:/m\.settings_export/);
    assert.match(component.slice(actions[1].start,actions[1].end), index===0?/m\.settings_restore/:/m\.settings_import/);
  }
  assert.match(component,/\.data-actions\s*\{[^}]*flex-wrap: wrap/);
  assert.match(component,/\.backup-button\s*\{[^}]*text-decoration: none/);
  assert.match(component,/\.backup-button:focus-visible/);
  assert.match(component,/\.panel-section\.transfer-section\s*\{[^}]*border-top:/);
  assert.equal(translations.sv.settings_backup_description,'Skydda journalen med en säkerhetskopia.');
  assert.equal(translations.sv.settings_transfer_description,'Exportera journalen till en annan installation eller ersätt journalen med en tidigare export.');
  assert.equal(translations.en.settings_transfer_description,'Export your journal to another installation or replace it with a previous export.');
  for(const [locale,labels] of [['sv',['Återställ…','Exportera…','Importera…']],['en',['Restore…','Export…','Import…']]]) {
    assert.deepEqual([translations[locale].settings_restore,translations[locale].settings_export,translations[locale].settings_import],labels);
  }
});
