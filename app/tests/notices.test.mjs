import { clientInventory } from '../scripts/notices/client-inventory.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { assertTarget, validateNative, satisfies, assertText, digest, reviewedException, evidence, escapeHtml, validateBundle } from '../scripts/notices/policy.mjs';

test('SPDX alternatives, conjunctions, nesting and exceptions keep their meaning', () => {
  assert.equal(satisfies('MIT OR Apache-2.0', ['MIT']), true);
  assert.equal(satisfies('Apache-2.0 AND ISC', ['Apache-2.0']), false);
  assert.equal(satisfies('Apache-2.0 AND ISC', ['Apache-2.0', 'ISC']), true);
  assert.equal(satisfies('(MIT OR Apache-2.0) AND BSD-3-Clause', ['MIT', 'BSD-3-Clause']), true);
  assert.equal(satisfies('MIT AND Apache-2.0 WITH LLVM-exception', ['MIT','Apache-2.0']), false);
  assert.equal(satisfies('MIT AND Apache-2.0 WITH LLVM-exception', ['MIT','Apache-2.0 WITH LLVM-exception']), true);
  assert.equal(satisfies('MIT/Apache-2.0', ['MIT']), true);
  for (const expr of ['', 'MIT OR', 'MIT unknown', 'MIT OR Unknown', '(MIT']) assert.throws(() => satisfies(expr, ['MIT']));
});
test('real upstream omissions are reviewed without invented attribution', () => {
  const entry = {name:'objc2',version:'0.6.5',sourceChecksum:'checksum',commit:'commit',reason:'Upstream omission reviewed',notices:[{}],noCopyrightSupplied:true};
  assert.equal(reviewedException(entry, entry, 'checksum','commit'), entry);
  for (const changed of [{version:'0.6.6'}, {name:'other'}]) assert.throws(() => reviewedException(entry,{...entry,...changed},'checksum','commit'));
  assert.throws(() => reviewedException(entry,entry,'changed','commit'));
  assert.throws(() => reviewedException(entry,entry,'checksum','changed'));
  assert.throws(() => reviewedException(undefined,entry,'checksum','commit'));
  assertText('Permission is hereby granted, free of charge. Actual upstream MIT terms without a holder.');
  assert.throws(() => assertText('Copyright (c) <year> <copyright holders>'));
  assert.throws(() => assertText(''));
});
function fixture(fn) {
  const dir = mkdtempSync(resolve(tmpdir(), 'mno-notices-test-'));
  try { fn(dir); } finally { rmSync(dir,{recursive:true}); }
}
test('evidence checksum and path confinement', () => fixture(dir => {
  writeFileSync(resolve(dir,'license.txt'),'Copyright Real Upstream\nMIT terms');
  const item={file:'license.txt',sha256:digest('Copyright Real Upstream\nMIT terms')};
  assert.match(evidence(dir,item),/Real Upstream/);
  assert.throws(() => evidence(dir,{...item,sha256:'wrong'}));
  assert.throws(() => evidence(dir,{...item,file:'../outside'}));
}));
test('offline bundle rejects missing/lost notices, sources and network resources', () => fixture(dir => {
  mkdirSync(resolve(dir,'sources'));
  const text='Copyright <upstream> & source terms';
  const component={name:'synthetic',version:'1',expression:'MPL-2.0',selected:['MPL-2.0'],texts:[text]};
  const saveInventory=() => writeFileSync(resolve(dir,'inventory.json'),JSON.stringify({components:[component]}));
  saveInventory();
  writeFileSync(resolve(dir,'sources/source.crate'),'synthetic source');
  const source={name:'synthetic',version:'1',file:'sources/source.crate',sha256:digest('synthetic source')};
  const saveSources=s => writeFileSync(resolve(dir,'source-checksums.json'),JSON.stringify(s));
  saveSources([source]);
  const html=`<pre>${escapeHtml(text)}</pre><a href="sources/source.crate">source</a>`;
  writeFileSync(resolve(dir,'THIRD-PARTY-NOTICES.html'),html);
  assert.doesNotThrow(() => validateBundle(dir));
  saveSources([]); assert.throws(() => validateBundle(dir),/Missing MPL/);
  saveSources([{...source,sha256:'wrong'}]); assert.throws(() => validateBundle(dir),/checksum/);
  saveSources([source]);
  writeFileSync(resolve(dir,'THIRD-PARTY-NOTICES.html'),html+'<script src="https://example.invalid"></script>');
  assert.throws(() => validateBundle(dir),/External/);
  writeFileSync(resolve(dir,'THIRD-PARTY-NOTICES.html'),'<p>notice lost</p>');
  assert.throws(() => validateBundle(dir),/Lost notice/);
  writeFileSync(resolve(dir,'THIRD-PARTY-NOTICES.html'),html+'<a href="sources/absent">missing</a>');
  assert.throws(() => validateBundle(dir),/offline reference/);
  component.selected=['MIT']; saveInventory(); assert.throws(() => validateBundle(dir),/Unsatisfied/);
}));

test('macOS evidence rejects unmatched hosts and cross targets', () => {
  assert.doesNotThrow(() => assertTarget('darwin', 'arm64', 'aarch64-apple-darwin'));
  for (const args of [['linux', 'arm64', 'aarch64-apple-darwin'], ['darwin', 'x64', 'aarch64-apple-darwin'], ['darwin', 'arm64', 'x86_64-apple-darwin']]) assert.throws(() => assertTarget(...args));
});

test('reviewed host/target selection keeps macOS and Windows inventories separate', () => {
  assert.equal(assertTarget('darwin', 'arm64', 'aarch64-apple-darwin'), 'native.json');
  assert.equal(assertTarget('win32', 'x64', 'x86_64-pc-windows-msvc'), 'native.windows.json');
  for (const args of [['win32','arm64','x86_64-pc-windows-msvc'], ['win32','x64','aarch64-apple-darwin'], ['darwin','arm64','x86_64-pc-windows-msvc'], ['win32','x64','x86_64-pc-windows-gnu']]) assert.throws(() => assertTarget(...args));
});
test('Windows native review rejects missing dependencies, provenance and terms', () => {
  const native = JSON.parse(readFileSync(new URL('../licensing/native.windows.json', import.meta.url)));
  assert.doesNotThrow(() => validateNative(native, native.target));
  for (const name of ['Microsoft WebView2 Loader', 'Microsoft Visual C++ runtime', 'NSIS installer stub and standard plugins', 'Tauri NSIS helper']) {
    assert.throws(() => validateNative({...native, components: native.components.filter(c => c.name !== name)}, native.target), /Missing reviewed Windows/);
  }
  for (const change of [{bundledFiles: []}, {installerTerms: null}, {tauriCliVersion: null}, {sourceLock: null}, {target: 'aarch64-apple-darwin'}]) assert.throws(() => validateNative({...native, ...change}, native.target));
  const registry = new URL('../licensing/', import.meta.url);
  const root = fileURLToPath(registry);
  for (const component of native.components) for (const item of component.notices) assertText(evidence(root, item));
  const mac = JSON.parse(readFileSync(new URL('../licensing/native.json', import.meta.url)));
  assert.doesNotThrow(() => validateNative(mac, 'aarch64-apple-darwin'));
});
test('internal NSIS config preserves identity and uses explicit current-user installation', () => {
  const base = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url)));
  const windows = JSON.parse(readFileSync(new URL('../src-tauri/tauri.windows-test.conf.json', import.meta.url)));
  assert.equal(base.identifier, 'org.mynatureobservations.app');
  assert.equal(base.productName, 'My Nature Observations');
  assert.equal(base.version, '0.1.0');
  for (const key of ['identifier','productName','version','app','build']) assert.equal(windows[key], undefined);
  assert.equal(windows.bundle.windows.nsis.installMode, 'currentUser');
  assert.equal(windows.bundle.windows.nsis.compression, 'zlib');
  assert.equal(windows.bundle.licenseFile, 'notices/WINDOWS-INSTALLER-LICENSE.txt');
});

test('client inventory handles Windows separators and retains Paraglide runtime', () => fixture(root => {
  for (const [path, name] of [['node_modules/synthetic','synthetic'], ['node_modules/@inlang/paraglide-js','@inlang/paraglide-js']]) {
    mkdirSync(resolve(root,path), {recursive:true});
    writeFileSync(resolve(root,path,'package.json'), JSON.stringify({name,version:'1.0.0'}));
  }
  const plugin = clientInventory();
  const windows = path => path.replaceAll('/', '\\');
  plugin.configResolved({root: windows(root), build:{ssr:false}});
  plugin.generateBundle.call({getModuleIds: () => []}, {}, {chunk:{type:'chunk',modules:{[windows(resolve(root,'node_modules/synthetic/index.js'))]:{renderedLength:1}}}});
  const packages = JSON.parse(readFileSync(resolve(root,'.svelte-kit/notices-client.json')));
  assert.deepEqual(packages.map(p => p.name), ['@inlang/paraglide-js','synthetic']);
  assert.equal(packages.find(p=>p.name==='synthetic').path, 'node_modules/synthetic');
}));
