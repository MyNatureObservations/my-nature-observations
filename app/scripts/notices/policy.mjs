import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, relative } from 'node:path';

export function assertTarget(platform, arch, target) {
  if (platform !== 'darwin' || arch !== 'arm64' || target !== 'aarch64-apple-darwin') throw Error('Notices are reviewed for macOS arm64 only; review native components before another target.');
}

export const digest = bytes => createHash('sha256').update(bytes).digest('hex');
export const escapeHtml = s => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[c]));
export const allowed = new Set(['MIT','Apache-2.0','BSD-2-Clause','BSD-3-Clause','ISC','MPL-2.0','Unicode-3.0','Zlib','BSL-1.0','0BSD','Unlicense','CC0-1.0','MIT-0','CDLA-Permissive-2.0','Apache-2.0 WITH LLVM-exception']);

// SPDX AND binds more tightly than OR; WITH belongs to one license atom.
export function satisfies(expression, selected) {
  const tokens = expression.replace(/\s*\/\s*/g, ' OR ').match(/\(|\)|[^\s()]+/g) || [];
  let i = 0;
  function atom() {
    if (tokens[i] === '(') { i++; const result = or(); if (tokens[i++] !== ')') throw Error('Unclosed expression'); return result; }
    let id = tokens[i++];
    if (tokens[i] === 'WITH') { i++; id += ' WITH ' + tokens[i++]; }
    if (!allowed.has(id)) throw Error(`Unresolved license: ${id}`);
    return selected.includes(id);
  }
  function and() { let result = atom(); while (tokens[i] === 'AND') { i++; const next = atom(); result = result && next; } return result; }
  function or() { let result = and(); while (tokens[i] === 'OR') { i++; const next = and(); result = result || next; } return result; }
  const result = or();
  if (i !== tokens.length) throw Error('Invalid license expression');
  return result;
}
export function assertText(text) {
  if (!text?.trim()) throw Error('Missing mandatory license text');
  if (/copyright[^\n]*(?:<year>|<copyright|\[year\]|\[fullname\])/i.test(text)) throw Error('Attribution placeholder');
}
export function evidence(root, item) {
  const file = resolve(root, item.file);
  if (relative(root, file).startsWith('..')) throw Error('Evidence outside registry');
  const bytes = readFileSync(file);
  if (digest(bytes) !== item.sha256) throw Error(`Evidence checksum: ${item.file}`);
  const text = bytes.toString('utf8'); assertText(text); return text;
}
export function reviewedException(entry, pkg, checksum, commit) {
  if (!entry || entry.version !== pkg.version || entry.name !== pkg.name || entry.sourceChecksum !== checksum || entry.commit !== commit) throw Error(`Review required: ${pkg.name}@${pkg.version}`);
  if (!entry.reason || !entry.notices?.length) throw Error('Incomplete reviewed exception');
  return entry;
}
export function validateBundle(root) {
  const inventory = JSON.parse(readFileSync(resolve(root, 'inventory.json')));
  if (!inventory.components.length) throw Error('Empty component inventory');
  const html = readFileSync(resolve(root, 'THIRD-PARTY-NOTICES.html'), 'utf8');
  if (/<(?:script|link|img|iframe)\b/i.test(html) || /(?:src|href)=["'](?:https?:|\/\/)/i.test(html)) throw Error('External HTML dependency');
  for (const c of inventory.components) {
    if (!c.name || !c.version || !satisfies(c.expression, c.selected)) throw Error(`Unsatisfied license: ${c.name}`);
    if (!c.texts.length) throw Error(`Missing notices: ${c.name}`);
    for (const text of c.texts) {
      assertText(text);
      if (!html.includes(escapeHtml(text))) throw Error(`Lost notice: ${c.name}`);
    }
  }
  const sources = JSON.parse(readFileSync(resolve(root, 'source-checksums.json')));
  for (const c of inventory.components.filter(c => c.selected.includes('MPL-2.0'))) {
    if (!sources.some(s => s.name === c.name && s.version === c.version)) throw Error(`Missing MPL source: ${c.name}`);
  }
  for (const s of sources) {
    if (relative(root, resolve(root, s.file)).startsWith('..') || digest(readFileSync(resolve(root, s.file))) !== s.sha256) throw Error(`Source checksum: ${s.file}`);
  }
  for (const match of html.matchAll(/href="([^"]+)"/g)) {
    const path = match[1];
    if (path.startsWith('#')) continue;
    if (relative(root, resolve(root, path)).startsWith('..') || !existsSync(resolve(root, path))) throw Error(`Missing offline reference: ${path}`);
  }
  return inventory;
}
