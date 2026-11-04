import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyNsisExecutable } from '../scripts/verify-nsis-executable.mjs';

const token = '__TAURI_BUNDLE_TYPE_VAR_UNK';
function fixture() {
  const original = Buffer.from(`MZ synthetic PE header\0${token}\0ProductName=My Nature Observations\0icon resources`);
  const packaged = Buffer.from(original.toString().replace(token, '__TAURI_BUNDLE_TYPE_VAR_NSS'));
  return { original, packaged };
}
test('exact NSIS patch preserves every other byte without modifying source buffers', () => {
  const { original, packaged } = fixture();
  const before = Buffer.from(original);
  const result = verifyNsisExecutable(original, packaged);
  assert.equal(result.originalBytes, result.packagedBytes);
  assert.notEqual(result.originalSha256, result.packagedSha256);
  assert.equal(result.expectedNsisSha256, result.packagedSha256);
  assert.equal(result.patchOffset, original.indexOf(token));
  assert.deepEqual(original, before);
});
test('unpatched and other bundle types are rejected', () => {
  const { original } = fixture();
  for (const replacement of [token, '__TAURI_BUNDLE_TYPE_VAR_MSI']) {
    assert.throws(() => verifyNsisExecutable(original, Buffer.from(original.toString().replace(token, replacement))), /differs beyond/);
  }
});
test('header, code, metadata and icon changes remain integrity failures', () => {
  const { original, packaged } = fixture();
  for (const offset of [0, 10, packaged.indexOf('ProductName'), packaged.indexOf('icon')]) {
    const changed = Buffer.from(packaged);
    changed[offset] ^= 1;
    assert.throws(() => verifyNsisExecutable(original, changed), /differs beyond/);
  }
});
test('truncation and added bytes are rejected', () => {
  const { original, packaged } = fixture();
  for (const changed of [packaged.subarray(0, packaged.length - 1), Buffer.concat([packaged, Buffer.from([0])])]) {
    assert.throws(() => verifyNsisExecutable(original, changed), /differs beyond/);
  }
});
test('missing original token fails closed', () => {
  assert.throws(() => verifyNsisExecutable(Buffer.from('no bundle token'), Buffer.from('anything')), /Missing original/);
});
test('only the first token is patched, matching the pinned bundler algorithm', () => {
  const original = Buffer.from(`${token}\0${token}`);
  const correct = Buffer.from(original.toString().replace(token, '__TAURI_BUNDLE_TYPE_VAR_NSS'));
  assert.doesNotThrow(() => verifyNsisExecutable(original, correct));
  assert.throws(() => verifyNsisExecutable(original, Buffer.from(original.toString().replaceAll(token, '__TAURI_BUNDLE_TYPE_VAR_NSS'))), /differs beyond/);
});

test('CLI reports verification metadata and returns failure for altered payloads', () => {
  const directory = mkdtempSync(join(tmpdir(), 'mno-nsis-verify-'));
  try {
    const { original, packaged } = fixture();
    const source = join(directory, 'source.exe'), payload = join(directory, 'payload.exe');
    writeFileSync(source, original); writeFileSync(payload, packaged);
    const args = [fileURLToPath(new URL('../scripts/verify-nsis-executable.mjs', import.meta.url)), source, payload];
    const good = spawnSync(process.execPath, args, {encoding:'utf8'});
    assert.equal(good.status, 0, good.stderr);
    const metadata = JSON.parse(good.stdout);
    assert.equal(metadata.expectedNsisSha256, metadata.packagedSha256);
    writeFileSync(payload, original);
    const bad = spawnSync(process.execPath, args, {encoding:'utf8'});
    assert.equal(bad.status, 1);
    assert.match(bad.stderr, /differs beyond/);
  } finally { rmSync(directory, {recursive:true}); }
});
