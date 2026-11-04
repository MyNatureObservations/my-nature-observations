import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, readdirSync } from 'node:fs';
import { digest, validateBundle } from './policy.mjs';
const app = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const generated = resolve(app, 'src-tauri/notices');
validateBundle(generated);
const resources = process.argv[2] === '--resources';
const bundle = resources ? process.argv[3] : process.argv[2];
if (bundle) {
  const packaged = resources ? resolve(bundle) : resolve(bundle, 'Contents/Resources/notices');
  if (resources && validateBundle(packaged).target !== 'x86_64-pc-windows-msvc') throw Error('Packaged Windows target mismatch');
  validateBundle(packaged);
  function compare(directory = '') {
    for (const entry of readdirSync(resolve(generated, directory), { withFileTypes: true })) {
      if (entry.name === '.gitkeep') continue;
      // Use relative paths for both trees.
      const rel = directory ? `${directory}/${entry.name}` : entry.name;
      if (entry.isDirectory()) compare(rel);
      else if (digest(readFileSync(resolve(generated, rel))) !== digest(readFileSync(resolve(packaged, rel)))) throw Error(`Packaged resource differs: ${rel}`);
    }
  }
  compare();
}
console.log(bundle ? 'Packaged notice bundle matches generated output.' : 'Offline notice bundle validated.');
