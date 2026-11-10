import { existsSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve, relative } from 'node:path';

// Audit emitted client modules, including runtime code from devDependencies.
/** @returns {import('vite').Plugin} */
export function clientInventory() {
  let root = '', ssr = false;
  return {
    name: 'mno-notice-inventory',
    configResolved(config) { root = config.root.replaceAll('\\', '/'); ssr = !!config.build.ssr; },
    generateBundle(_options, bundle) {
      if (ssr) return;
      const ids = Object.values(bundle).filter(x => x.type === 'chunk').flatMap(chunk =>
        Object.entries(chunk.modules).filter(([, value]) => value.renderedLength > 0).map(([id]) => id));
      ids.push(...[...this.getModuleIds()].filter(id => /\.css(?:\?|$)/.test(id)));
      /** @type {Map<string, {name: string, version: string, path: string}>} */
      const packages = new Map();
      /** @param {string} id */
      function include(id) {
        id = id.replace(/^\0/, '').split('?')[0].replaceAll('\\', '/');
        if (!id.includes('/node_modules/')) return;
        let directory = dirname(id);
        while (directory.startsWith(root) && !existsSync(resolve(directory, 'package.json'))) directory = dirname(directory);
        const manifest = resolve(directory, 'package.json');
        if (!existsSync(manifest)) throw new Error(`Cannot identify bundled dependency: ${id}`);
        const pkg = JSON.parse(readFileSync(manifest, 'utf8'));
        const path = relative(root, directory).replaceAll('\\', '/');
        packages.set(path, { name: pkg.name, version: pkg.version, path });
      }
      ids.forEach(include);
      // The compiler emits Paraglide's runtime into src/lib/paraglide.
      include(resolve(root, 'node_modules/@inlang/paraglide-js/package.json'));
      mkdirSync(resolve(root, '.svelte-kit'), { recursive: true });
      writeFileSync(resolve(root, '.svelte-kit/notices-client.json'), JSON.stringify([...packages.values()].sort((a,b) => a.name.localeCompare(b.name)), null, 2) + '\n');
    }
  };
}
