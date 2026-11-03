import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { compile, parse } from 'svelte/compiler';
import { render } from 'svelte/server';
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const source = read('../src/lib/components/AboutMno.svelte');
function harness({ version = '9.8.7-test', failVersion = false, failNotice = false, failOpen = false, resolveLater } = {}) {
  const calls = [];
  let destroy;
  const context = vm.createContext({
    $state: x => x, language: { current: 'sv' }, m: {},
    onMount: () => {}, onDestroy: f => destroy = f,
    getVersion: async () => { if (failVersion) throw Error('unavailable'); return version; },
    invoke: async command => { calls.push(['invoke',command]); if (resolveLater) await resolveLater; if (failNotice) throw Error('notices_unavailable'); },
    openUrl: async url => { calls.push(['openUrl',url]); if (failOpen) throw Error('failed'); },
  });
  const script = source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, '');
  vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ESNext } }).outputText, context);
  return { run: code => vm.runInContext(code,context), calls, destroy: () => destroy() };
}
test('About compiles with the approved icon and original MIT text', async () => {
  compile(source, { generate:'client' });
  assert.match(source,/src-tauri\/icons\/128x128.png/);
  assert.match(source,/LICENSE\?raw/);
  assert.match(source,/<details class="license-text">/);
  assert.match(source,/<pre>\{licenseText\}<\/pre>/);
  const messages = await import('../src/lib/paraglide/messages.js');
  for (const [locale,title] of [['sv','Om MNO'],['en','About MNO']]) {
    assert.equal(messages.about_heading({}, {locale}),title);
    assert.equal(messages.about_version({version:'9.8.7-test'},{locale}),'Version 9.8.7-test');
    for (const key of ['about_app_name','about_mit_description','about_project_license','about_repository','about_third_party','about_offline_hint','about_version_loading','about_version_unavailable','about_notices_failed','about_repository_failed']) assert.ok(messages[key]({}, {locale}));
  }
});
test('version comes from the running Tauri application and failures stay local', async () => {
  const h=harness(); await h.run('loadVersion()'); assert.equal(h.run('version'),'9.8.7-test');
  const failed=harness({failVersion:true}); await failed.run('loadVersion()'); assert.equal(failed.run('version'),null); assert.equal(failed.run('versionFailed'),true);
  await failed.run('openDocument("repository")'); assert.equal(failed.calls[0][0],'openUrl');
  const disposed=harness(); disposed.destroy(); await disposed.run('loadVersion()'); assert.equal(disposed.run('version'),null);
  assert.doesNotMatch(source,/package\.json|0\.1\.0|console\.error/);
});
test('notices use the fixed resource command without broad file permissions', async () => {
  const h=harness(); await h.run('openDocument("notices")');
  assert.deepEqual(h.calls,[['invoke','open_third_party_notices']]);
  assert.equal(h.run('opening'),false); assert.equal(h.run('error'),null);
  await h.run('openDocument("repository")'); assert.deepEqual(h.calls.at(-1),['openUrl','https://github.com/MyNatureObservations/my-nature-observations']);
  const permissions=JSON.parse(read('../src-tauri/capabilities/default.json')).permissions;
  assert.ok(!permissions.includes('opener:allow-open-path'));
  const rust=read('../src-tauri/src/about.rs');
  assert.match(rust,/resolve\(NOTICES, BaseDirectory::Resource\)/);
  assert.match(rust,/const NOTICES: &str = "notices\/THIRD-PARTY-NOTICES.html"/);
});
test('missing, inaccessible and rejected resources surface localized errors without crashing', async () => {
  for (const opts of [{failNotice:true}]) {
    const h=harness(opts); await h.run('openDocument("notices")'); assert.equal(h.run('error'),'notices'); assert.equal(h.run('opening'),false);
    assert.equal(h.calls.length,1);
  }
  const h=harness({failOpen:true}); await h.run('openDocument("repository")'); assert.equal(h.run('error'),'repository');
  assert.match(source,/role="alert"/); assert.match(source,/m\.about_notices_failed/);
});
test('repeated clicks cannot launch duplicate opens and late replies ignore disposal', async () => {
  let release; const h=harness({resolveLater:new Promise(r=>release=r)});
  const pending=h.run('openDocument("notices")'); await h.run('openDocument("notices")'); assert.equal(h.calls.length,1);
  h.destroy(); release(); await pending; assert.equal(h.calls.length,1); assert.equal(h.run('error'),null);
});
test('Settings disclosures share icon, typography and native keyboard semantics', () => {
  const nav=read('../src/lib/components/AppNavigation.svelte'); compile(nav,{generate:'client'});
  const ast=parse(nav,{modern:true}), details=[];
  function walk(v) { if (!v || typeof v!=='object') return; if(v.type==='RegularElement' && v.name==='details') details.push(v); for(const c of Object.values(v)) if(Array.isArray(c)) c.forEach(walk); else if(c && typeof c==='object') walk(c); }
  walk(ast.fragment);
  for(const name of ['language-details','data-security-details','about-details']) {
    const node=details.find(n=>n.attributes.some(a=>a.name==='class' && a.value[0]?.data===name)); assert.ok(node);
    const summary=node.fragment.nodes.find(n=>n.type==='RegularElement' && n.name==='summary');
    const html=nav.slice(summary.start,summary.end); assert.match(html,/class="panel-section-heading"/); assert.match(html,/aria-hidden="true"/); assert.match(html,/<svg/); assert.match(html,/language-chevron/);
  }
  assert.doesNotMatch(nav,/\.data-security-details > summary/);
  assert.match(nav,/<AboutMno \/>/);
});
test('shared saved-place fields retain labels, decimal inputs and contained visible focus', () => {
  const panel=read('../src/lib/components/PlaceMapPanel.svelte'); compile(panel,{generate:'client'});
  for(const key of ['place_name','place_latitude','place_longitude']) assert.match(panel,new RegExp(`<label>\\{m\\.${key}[\\s\\S]*?<input`));
  assert.equal((panel.match(/inputmode="decimal"/g)||[]).length,2);
  assert.match(panel,/input \{[^}]*appearance: none;[^}]*box-sizing: border-box;[^}]*min-width: 0;[^}]*border-radius: 6px/);
  assert.match(panel,/input:focus, input:focus-visible \{ outline: 2px solid #788e7b; outline-offset: -2px;/);
  assert.match(panel,/form, label \{ min-width: 0;/);
});
test('macOS bundle declares both manually localized languages without overriding identities', () => {
  const plist=read('../src-tauri/Info.plist'); assert.match(plist,/<key>CFBundleLocalizations<\/key>/);
  for(const locale of ['en','sv']) assert.match(plist,new RegExp(`<string>${locale}</string>`));
  assert.doesNotMatch(plist,/CFBundleIdentifier|CFBundleVersion|CFBundleDevelopmentRegion|AppleLanguages/);
});

test('actual About markup renders bilingual actions, the runtime version and verbatim MIT text', async () => {
  const moduleUrl=code=>`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
  for(const [locale,notices,project] of [['sv','Tredjepartslicenser','Projektet på GitHub'],['en','Third-party licenses','Project on GitHub']]) {
    const seeded=source.replace('let version = $state<string | null>(null)', 'let version = $state<string | null>("9.8.7-test")');
    const compiled=compile(seeded,{filename:'AboutMno.svelte',generate:'server'});
    const local={
      '../../../src-tauri/icons/128x128.png':moduleUrl('export default "/approved-mno-icon.png"'),
      '../../../../LICENSE?raw':moduleUrl(`export default ${JSON.stringify(read('../../LICENSE'))}`),
      '$lib/language.svelte':moduleUrl(`export const language={current:${JSON.stringify(locale)}}`),
      '$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href,
    };
    const code=compiled.js.code.replace(/from (["'])([^"']+)\1/g,(_m,q,name)=>`from ${q}${local[name]??import.meta.resolve(name)}${q}`);
    const component=await import(moduleUrl(code));
    const html=render(component.default).body;
    assert.match(html,/Version 9.8.7-test/); assert.match(html,/My Nature Observations/);
    assert.ok(html.includes(notices)); assert.ok(html.includes(project));
    assert.match(html,/href="https:\/\/github.com\/MyNatureObservations\/my-nature-observations"/);
    assert.match(html,/Permission is hereby granted, free of charge/); assert.match(html,/approved-mno-icon.png/);
  }
});
