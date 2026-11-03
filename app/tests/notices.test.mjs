import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { assertTarget, satisfies, assertText, digest, reviewedException, evidence, escapeHtml, validateBundle } from '../scripts/notices/policy.mjs';

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

test('macOS-only evidence cannot silently ship for another host or cross target', () => {
  assert.doesNotThrow(() => assertTarget('darwin', 'arm64', 'aarch64-apple-darwin'));
  for (const args of [['linux', 'arm64', 'aarch64-apple-darwin'], ['darwin', 'x64', 'aarch64-apple-darwin'], ['darwin', 'arm64', 'x86_64-apple-darwin']]) assert.throws(() => assertTarget(...args));
});
