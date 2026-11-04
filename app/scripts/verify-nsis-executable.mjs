import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Tauri CLI 2.12.2 / bundler 2.10.2, crates/tauri-bundler/src/bundle.rs:
// patch_binary replaces the FIRST UNK token, then bundle_project restores the
// original build output. Reproduce that exact patch, not a general PE exemption.
const originalToken = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK');
const nsisToken = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS');
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

export function verifyNsisExecutable(original, packaged) {
  const offset = original.indexOf(originalToken);
  const diagnostics = {
    originalBytes: original.length,
    packagedBytes: packaged.length,
    originalSha256: sha256(original),
    packagedSha256: sha256(packaged),
    patchOffset: offset,
  };
  if (offset < 0) throw Error(`Missing original Tauri bundle token: ${JSON.stringify(diagnostics)}`);
  const expected = Buffer.from(original);
  nsisToken.copy(expected, offset);
  diagnostics.expectedNsisSha256 = sha256(expected);
  if (!expected.equals(packaged)) {
    throw Error(`Packaged executable differs beyond the exact Tauri NSIS token patch: ${JSON.stringify(diagnostics)}`);
  }
  return diagnostics;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [, , originalPath, packagedPath] = process.argv;
    if (!originalPath || !packagedPath || process.argv.length !== 4) throw Error('Usage: node scripts/verify-nsis-executable.mjs <original app.exe> <extracted app.exe>');
    const result = verifyNsisExecutable(readFileSync(originalPath), readFileSync(packagedPath));
    console.log(JSON.stringify({ originalPath, packagedPath, ...result }, null, 2));
  } catch (error) {
    console.error(String(error));
    process.exitCode = 1;
  }
}
