import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, readdirSync } from 'node:fs';
import { digest, validateBundle } from './policy.mjs';
const app = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const generated = resolve(app, 'src-tauri/notices');
const generatedInventory = validateBundle(generated);
const resources = process.argv[2] === '--resources';
const bundle = resources ? process.argv[3] : process.argv[2];
const extractedTarget = process.argv[4] ?? 'x86_64-pc-windows-msvc';
if (resources && (!bundle || process.argv.length > 5 || !['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu'].includes(extractedTarget))) {
  throw Error('Usage: validate.mjs --resources <notices-directory> [x86_64-pc-windows-msvc|x86_64-unknown-linux-gnu]');
}
if (bundle) {
  const packaged = resources ? resolve(bundle) : resolve(bundle, 'Contents/Resources/notices');
  const packagedInventory = validateBundle(packaged);
  if (resources && (packagedInventory.target !== extractedTarget || generatedInventory.target !== extractedTarget)) {
    throw Error(`Expected generated and packaged target ${extractedTarget}; got ${generatedInventory.target} and ${packagedInventory.target}`);
  }
  if (resources && extractedTarget === 'x86_64-unknown-linux-gnu') {
    const license = digest(readFileSync(resolve(app, '../LICENSE')));
    if ([generated, packaged].some(root => digest(readFileSync(resolve(root, 'LICENSE'))) !== license)) {
      throw Error('Linux resource LICENSE differs from the MNO license');
    }
  }
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
