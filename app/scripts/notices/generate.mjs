import { readFileSync, writeFileSync, readdirSync, mkdirSync, copyFileSync, rmSync } from 'node:fs';
import { resolve, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { assertTarget, validateNative, digest, escapeHtml as h, allowed, satisfies, assertText, evidence, reviewedException, validateBundle } from './policy.mjs';

const app = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const rust = resolve(app, 'src-tauri'), registry = resolve(app, 'licensing'), out = resolve(rust, 'notices');
const json = path => JSON.parse(readFileSync(path));
const run = (program, args, cwd = rust) => execFileSync(program, args, { cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
const target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET || (process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'aarch64-apple-darwin');
const native = json(resolve(registry, assertTarget(process.platform, process.arch, target)));
validateNative(native, target);
if (native.tauriCliVersion && json(resolve(app, 'node_modules/@tauri-apps/cli/package.json')).version !== native.tauriCliVersion) throw Error('Review Windows native installer tooling for this Tauri CLI');
if (run('cargo', ['about', '--version']).trim() !== 'cargo-about 0.9.3') throw Error('Install cargo-about 0.9.3 with --locked');
const compiler = run('rustc', ['-vV']).replace(/\r\n/g, '\n');
if (!compiler.includes(`release: ${native.rustVersion}\n`) || !compiler.includes(`commit-hash: ${native.rustCommit}\n`)) throw Error('Rust runtime evidence must be reviewed for this compiler');
evidence(registry, native.sourceLock);
run('cargo', ['fetch', '--locked', '--target', native.target]);
const report = JSON.parse(run('cargo', ['about', 'generate', '--manifest-path', 'Cargo.toml', '--config', '../licensing/about.toml', '--locked', '--target', native.target, '--features', 'tauri/custom-protocol', '--format', 'json', '--fail']));
const lock = readFileSync(resolve(rust, 'Cargo.lock'), 'utf8');
const checksums = new Map(lock.split('[[package]]').slice(1).map(block => {
  const field = key => block.match(new RegExp(`^${key} = "([^"]+)"`, 'm'))?.[1];
  return [`${field('name')}@${field('version')}`, field('checksum')];
}));
const exceptions = json(resolve(registry, 'exceptions.json')).entries;
const components = [], sources = [];
// Remove only generated output, never user journals or build artifacts.
mkdirSync(out, { recursive: true });
for (const name of readdirSync(out)) if (name !== '.gitkeep') rmSync(resolve(out, name), { recursive: true });
mkdirSync(resolve(out, 'sources'));
function licenseFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).filter(f => f.isFile() && /^(?:licen[cs]e|copying|copyright|notice)(?:$|[._-])/i.test(f.name)).map(f => readFileSync(resolve(directory, f.name), 'utf8'));
}
function add(component) {
  component.texts = [...new Set(component.texts)];
  component.selected = [...new Set(component.selected)].sort();
  component.texts.forEach(assertText);
  if (!component.texts.length || !satisfies(component.expression, component.selected)) throw Error(`Incomplete notices: ${component.name}`);
  components.push(component);
}
for (const { package: pkg, license: expression } of report.crates) {
  if (!pkg.source) continue; // MNO's own MIT license is separate.
  const key = `${pkg.name}@${pkg.version}`, directory = dirname(pkg.manifest_path);
  for (const item of native.bundledFiles || []) if (item.crate === pkg.name) {
    if (pkg.version !== item.version || checksums.get(key) !== item.sourceChecksum || digest(readFileSync(resolve(directory, item.file))) !== item.sha256) throw Error(`Unreviewed native binary: ${pkg.name}`);
    item.verified = true;
  }
  const blocks = report.licenses.filter(l => l.used_by.some(u => u.crate.name === pkg.name && u.crate.version === pkg.version));
  let selected = blocks.map(l => l.id), texts = blocks.map(l => l.text), review;
  if (texts.some(text => /<year>|<copyright/i.test(text))) {
    const vcs = json(resolve(directory, '.cargo_vcs_info.json'));
    review = reviewedException(exceptions.find(e => e.name === pkg.name && e.version === pkg.version), pkg, checksums.get(key), vcs.git.sha1);
    selected = selected.filter(id => id !== 'MIT' && id !== 'BSD-3-Clause');
    selected.push(review.selected);
    texts = blocks.filter(l => !/<year>|<copyright/i.test(l.text)).map(l => l.text);
    texts.push(...review.notices.map(item => evidence(registry, item)));
  }
  texts.push(...licenseFiles(directory));
  // SQLite itself is public domain; its Rust binding has a separate MIT license.
  if (pkg.name === 'libsqlite3-sys') {
    const header = readFileSync(resolve(directory, 'sqlite3/sqlite3.c'), 'utf8').match(/\/\*\n\*\* 2001 September 15[\s\S]*?\*\//)?.[0] || '';
    if (!header.includes('The author disclaims copyright')) throw Error('SQLite public-domain notice missing');
    texts.push(header);
  }
  add({ name: pkg.name, version: pkg.version, ecosystem: 'Rust (target + build dependencies)', expression, selected, texts,
    review: review ? { commit: review.commit, sourceChecksum: review.sourceChecksum, reason: review.reason, noCopyrightSupplied: review.noCopyrightSupplied } : undefined });
  if (selected.includes('MPL-2.0')) {
    const archive = resolve(directory, '../../../cache', basename(dirname(directory)), `${key.replace('@', '-')}.crate`);
    const bytes = readFileSync(archive), sha256 = digest(bytes);
    if (sha256 !== checksums.get(key)) throw Error(`Cargo archive checksum: ${key}`);
    const file = `sources/${pkg.name}-${pkg.version}.crate`;
    copyFileSync(archive, resolve(out, file));
    sources.push({ name: pkg.name, version: pkg.version, file, sha256, origin: `https://crates.io/crates/${pkg.name}/${pkg.version}`, license: 'MPL-2.0' });
  }
}
if ((native.bundledFiles || []).some(item => !item.verified)) throw Error('Reviewed native loader missing from target dependencies');
const npmLock = json(resolve(app, 'package-lock.json'));
for (const pkg of json(resolve(app, '.svelte-kit/notices-client.json'))) {
  const locked = npmLock.packages[pkg.path];
  if (!locked || locked.version !== pkg.version) throw Error(`Client lock mismatch: ${pkg.name}`);
  const manifest = json(resolve(app, pkg.path, 'package.json'));
  if (manifest.name !== pkg.name || manifest.version !== pkg.version) throw Error('Client inventory is stale');
  const expression = manifest.license;
  const selected = [...allowed].filter(id => expression.includes(id));
  add({ name: pkg.name, version: pkg.version, ecosystem: 'Bundled frontend (including generated Paraglide runtime)', expression, selected, texts: licenseFiles(resolve(app, pkg.path)) });
}
for (const c of native.components) {
  add({ name: c.name, version: c.version, ecosystem: 'Rust runtime', expression: c.expression,
    selected: [...allowed].filter(id => c.expression.includes(id)), texts: c.notices.map(item => evidence(registry, item)) });
}
components.sort((a,b) => `${a.ecosystem}/${a.name}/${a.version}`.localeCompare(`${b.ecosystem}/${b.name}/${b.version}`));
sources.sort((a,b) => a.name.localeCompare(b.name));
const texts = new Map();
for (const c of components) for (const text of c.texts) texts.set(digest(text), text);
const html = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>MNO — Third-party notices</title>
<style>body{max-width:72rem;margin:3rem auto;padding:0 1.5rem;font:16px/1.5 system-ui;color:#20251f;background:#faf9f4}pre{white-space:pre-wrap;overflow-wrap:anywhere}section{margin:2rem 0}a{color:#34573b}</style>
<h1>My Nature Observations — Third-party notices</h1><p>MNO is MIT-licensed. Third-party components retain their own licenses. ${target === 'aarch64-apple-darwin' ? 'This offline inventory includes macOS arm64 target dependencies and conservative build/runtime notices. System frameworks are provided by macOS and are not bundled here.' : 'This offline inventory includes Windows x86_64 MSVC target dependencies, native loader/runtime and installer notices. Windows and the separately installed Evergreen WebView2 Runtime retain their own terms. Microsoft runtime code is not relicensed under MNO’s MIT license.'}</p>
<h2>MPL source availability</h2><p>The corresponding, unmodified source packages for the MPL-2.0 components are supplied below under their original licenses. Open .crate files with a tar/gzip archive tool. SHA-256 digests are in source-checksums.json. MNO does not modify these components.</p>
<ul>${sources.map(s => `<li><a href="${s.file}">${h(s.name)} ${h(s.version)} source</a></li>`).join('')}</ul>
<h2>Components</h2>${components.map(c => `<section><h3>${h(c.name)} ${h(c.version)}</h3><p>${h(c.ecosystem)} · Declared: ${h(c.expression)} · Included license terms: ${h(c.selected.join(', '))}</p>${c.review ? `<p>Reviewed upstream evidence: ${h(c.review.commit)}. ${h(c.review.reason)}</p>` : ''}<ul>${c.texts.map(t => `<li><a href="#text-${digest(t)}">License / notice ${digest(t).slice(0,12)}</a></li>`).join('')}</ul></section>`).join('')}
<h2>License texts and notices</h2>${[...texts].sort(([a],[b]) => a.localeCompare(b)).map(([id,text]) => `<section id="text-${id}"><h3>${id.slice(0,12)}</h3><pre>${h(text)}</pre></section>`).join('')}</html>\n`;
writeFileSync(resolve(out, 'THIRD-PARTY-NOTICES.html'), html);
writeFileSync(resolve(out, 'inventory.json'), JSON.stringify({ format: 1, target: native.target, rustVersion: native.rustVersion, rustCommit: native.rustCommit,
  inputs: { cargoLock: digest(lock), npmLock: digest(readFileSync(resolve(app, 'package-lock.json'))), client: digest(readFileSync(resolve(app, '.svelte-kit/notices-client.json'))) }, components }, null, 2) + '\n');
writeFileSync(resolve(out, 'source-checksums.json'), JSON.stringify(sources, null, 2) + '\n');
if (native.installerTerms) {
  writeFileSync(resolve(out, 'WINDOWS-INSTALLER-LICENSE.txt'), readFileSync(resolve(app, '../LICENSE'), 'utf8') + '\n\nMicrosoft Visual C++ runtime components only — separate terms follow. MNO source code remains MIT-licensed.\n\n' + evidence(registry, native.installerTerms));
}
validateBundle(out);
console.log(`Validated ${components.length} components, ${texts.size} distinct notices, ${sources.length} MPL sources; ${Buffer.byteLength(html)} HTML bytes.`);
