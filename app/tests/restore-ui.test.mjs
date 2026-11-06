import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';
import { compile, parse } from 'svelte/compiler';
const source=readFileSync(new URL('../src/routes/settings/restore/+page.svelte',import.meta.url),'utf8');
function harness(available = true) {
  let guard; const calls=[];
  const script=ts.transpileModule(source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
  const context=vm.createContext({$state:v=>v,onMount:f=>{if (!available) f();},beforeNavigate:f=>guard=f,language:{current:'sv'},m:{restore_open:()=> 'Välj'},invoke:(name,args)=>new Promise((resolve,reject)=>calls.push({name,args,resolve,reject}))});
  vm.runInContext(script,context);
  if (available) vm.runInContext("available = true", context);
  return {calls,run:code=>vm.runInContext(code,context),guard:()=>{let cancelled=false;guard({cancel(){cancelled=true;}});return cancelled;}};
}
const settle=async()=>{await Promise.resolve();await Promise.resolve();};
test('restore view compiles and settings link uses ordinary guarded navigation',()=>{
  compile(source,{generate:'client'});
  const nav=readFileSync(new URL('../src/lib/components/AppNavigation.svelte',import.meta.url),'utf8');
  assert.match(nav,/href="\/settings\/restore"/);
  const editor=readFileSync(new URL('../src/lib/components/JournalEditor.svelte',import.meta.url),'utf8');
  assert.match(editor,/beforeNavigate/);assert.match(editor,/hasPendingChanges.*isSaving/);
});
test('unavailable Restore rejects direct route actions', async () => {
  const h = harness(false);
  assert.equal(h.calls[0].name, 'restore_available');
  h.calls[0].resolve(false);
  await settle();
  await h.run('choose()');
  await h.run('phase = "confirmed"; token = "opaque"; restore()');
  assert.equal(h.calls.length, 1);
});
test('picker cancellation never starts restore; confirmation requires validated token',async()=>{
  const h=harness();await h.run('restore()');assert.equal(h.calls.length,0);
  const p=h.run('choose()');assert.equal(h.calls[0].name,'choose_restore_backup');h.calls[0].resolve(null);await p;
  assert.equal(h.run('phase'),'idle');await h.run('restore()');assert.equal(h.calls.length,1);
});
test('confirmed activation stays blocked and requests quit only after an explicit click',async()=>{
  const h=harness();const p=h.run('choose()');h.calls[0].resolve({token:'opaque'});await p;
  assert.equal(h.run('phase'),'confirmed');assert.equal(h.calls.length,1);
  const restore=h.run('restore()');assert.equal(h.guard(),true);assert.equal(h.calls[1].name,'restore_selected_backup');assert.equal(h.calls[1].args.token,'opaque');
  h.calls[1].resolve();await restore;
  assert.equal(h.run('phase'),'awaiting_exit');assert.equal(h.calls.length,2,'no automatic exit or relaunch');
  assert.equal(h.guard(),true,'no journal navigation before fresh startup verification');
  const quit=h.run('quit()');assert.equal(h.calls[2].name,'quit_after_restore');h.calls[2].resolve();await quit;
  assert.equal(h.calls.length,3);
});
test('failed validation or activation never requests exit or reports success',async()=>{
  const h=harness();const p=h.run('choose()');h.calls[0].reject('invalid_backup');await p;
  assert.equal(h.run('phase'),'failed');assert.equal(h.run('token'),null);
  const p2=h.run('choose()');h.calls[1].resolve({token:'opaque'});await p2;
  const restore=h.run('restore()');h.calls[2].reject('failed');await restore;
  assert.equal(h.run('phase'),'failed');assert.equal(h.run('token'),null);assert.equal(h.calls.length,3);
});
test('recovery panel uses only actions returned by verified backend status',()=>{
  const panel=readFileSync(new URL('../src/lib/components/RecoveryPanel.svelte',import.meta.url),'utf8');
  compile(panel,{generate:'client'});assert.match(panel,/!actions.includes\(action\)/);assert.match(panel,/await invoke\("run_recovery"/);assert.match(panel,/await onfinished\(!previous, operation\)/);
  const boundary=readFileSync(new URL('../src-tauri/src/recovery.rs',import.meta.url),'utf8');
  assert.doesNotMatch(boundary,/classification::recovery|RESET|VACUUM/);
});

function recoveryHarness(mode = 'recovery', onFinished = async () => {}, prepared = false) {
  const panel=readFileSync(new URL('../src/lib/components/RecoveryPanel.svelte',import.meta.url),'utf8');
  const script=ts.transpileModule(panel.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
  let mount;const calls=[],finished=[];
  const context=vm.createContext({$state:v=>v,$props:()=>({mode,prepared,onfinished:async (value,operation)=>{finished.push(value);await onFinished(value,operation);}}),onMount:f=>mount=f,language:{current:'sv'},m:new Proxy({},{get:(_,key)=>()=>key}),invoke:(name,args)=>new Promise((resolve,reject)=>calls.push({name,args,resolve,reject}))});
  vm.runInContext(script,context);const dispose=mount();
  return {calls,finished,dispose,run:code=>vm.runInContext(code,context)};
}
test('recovery failure cannot report completion or run an unavailable action',async()=>{
  const h=recoveryHarness();h.calls[0].resolve({actions:['finish_new']});await settle();
  await h.run('run("continue")');assert.equal(h.calls.length,1);
  const p=h.run('run("finish_new")');h.calls[1].reject('blocked');await p;
  assert.equal(h.finished.length,0);assert.equal(h.run('failed'),true);
});
test('recovery reports restored or previous only after backend success',async()=>{
  for (const [action,restored] of [['finish_new',true],['keep_previous',false]]) {
    const h=recoveryHarness();h.calls[0].resolve({actions:[action]});await settle();
    const p=h.run(`run("${action}")`);assert.equal(h.finished.length,0);h.calls[1].resolve();await p;
    assert.equal(h.finished[0],restored);
  }
});
test('late recovery status after unmount is ignored',async()=>{
  const h=recoveryHarness();h.dispose();h.calls[0].resolve({actions:['finish_new']});await settle();assert.equal(h.run('actions.length'),0);
});

test('blocked shell waits for manual quit and never reports restore completion',async()=>{
  const h=recoveryHarness('blocked');assert.equal(h.calls.length,0,'mounting must not quit');
  const p=h.run('quit()');assert.equal(h.calls[0].name,'quit_after_restore');h.calls[0].resolve();await p;
  assert.equal(h.finished.length,0);assert.equal(h.calls.length,1);
});
test('failed quit keeps the restore page blocked and allows another manual attempt',async()=>{
  const h=harness();const p=h.run('choose()');h.calls[0].resolve({token:'opaque'});await p;
  const restore=h.run('restore()');h.calls[1].resolve();await restore;
  const quit=h.run('quit()');h.calls[2].reject('busy');await quit;
  assert.equal(h.run('phase'),'awaiting_exit');assert.equal(h.run('error'),'quit_failed');assert.equal(h.guard(),true);
});
test('app exit uses the normal Tauri event path without any restart or process launcher',()=>{
  const boundary=readFileSync(new URL('../src-tauri/src/restore_app.rs',import.meta.url),'utf8');
  assert.match(boundary,/app\.exit\(0\)/);
  assert.doesNotMatch(boundary,/app\.restart|request_restart|std::process::exit|Command::new|\.spawn\(/);
  const lib=readFileSync(new URL('../src-tauri/src/lib.rs',import.meta.url),'utf8');
  assert.match(lib,/restore_app::quit_after_restore/);assert.doesNotMatch(lib,/restart_after_restore/);
  assert.doesNotMatch(source,/restart_after_restore/);
  for(const locale of ['sv','en']) {
    const messages=JSON.parse(readFileSync(new URL(`../messages/${locale}.json`,import.meta.url),'utf8'));
    assert.ok(messages.restore_quit);assert.ok(messages.restore_reopen_instructions);assert.equal(messages.restore_restart,undefined);
  }
});

test('failed quit in blocked shell keeps the gate and does not report successful recovery',async()=>{
  const h=recoveryHarness('blocked');const p=h.run('quit()');h.calls[0].reject('busy');await p;
  assert.equal(h.run('quitFailed'),true);assert.equal(h.run('busy'),false);assert.equal(h.finished.length,0);
});


test('home back link is an ordinary keyboard-accessible link inside its state condition',()=>{
  const ast=parse(source,{modern:true});
  const main=ast.fragment.nodes.find(node=>node.type==='RegularElement' && node.name==='main');
  const condition=main.fragment.nodes.find(node=>node.type==='IfBlock');
  assert.equal(source.slice(condition.test.start,condition.test.end),'canReturnHome()');
  const link=condition.consequent.nodes.find(node=>node.type==='RegularElement' && node.name==='a');
  assert.equal(link.attributes.find(a=>a.name==='href').value[0].data,'/');
  assert.equal(link.attributes.some(a=>['onclick','tabindex'].includes(a.name)),false);
  for(const [locale,label] of [['sv','Tillbaka till startsidan'],['en','Back to home']]) {
    const messages=JSON.parse(readFileSync(new URL(`../messages/${locale}.json`,import.meta.url),'utf8'));
    assert.equal(messages.restore_back_to_home,label);
  }
});

test('home return remains available before activation but never during or after an activation attempt',async()=>{
  const h=harness();assert.equal(h.run('canReturnHome()'),true);assert.equal(h.guard(),false);
  const invalid=h.run('choose()');assert.equal(h.run('canReturnHome()'),true);
  h.calls[0].reject('invalid_backup');await invalid;
  assert.equal(h.run('canReturnHome()'),true);
  const selection=h.run('choose()');h.calls[1].resolve({token:'opaque'});await selection;
  assert.equal(h.run('canReturnHome()'),true);
  const restore=h.run('restore()');
  assert.equal(h.run('canReturnHome()'),false);assert.equal(h.guard(),true);
  h.calls[2].resolve();await restore;
  assert.equal(h.run('canReturnHome()'),false);assert.equal(h.guard(),true);
  const quit=h.run('quit()');h.calls[3].reject('busy');await quit;
  assert.equal(h.run('canReturnHome()'),false);assert.equal(h.guard(),true);
  const failed=harness();const pick=failed.run('choose()');failed.calls[0].resolve({token:'opaque'});await pick;
  const attempt=failed.run('restore()');failed.calls[1].reject('failed');await attempt;
  assert.equal(failed.run('canReturnHome()'),false,'an uncertain activation error must not offer home navigation');
});


function layoutHarness() {
  const layout = readFileSync(new URL('../src/routes/+layout.svelte',import.meta.url),'utf8');
  const script = ts.transpileModule(layout.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
  const calls=[],destinations=[]; let mount,event,block,afterNavigation;
  const page={url:new URL('https://example.invalid/')};
  async function navigate(path) {
    const from={url:page.url};page.url=new URL(path,page.url);
    afterNavigation({from,to:{url:page.url}});
  }
  const context=vm.createContext({$state:v=>v,$props:()=>({children(){}}), PLACE_MAP:'mno:place-map',createPlaceMap:()=>({}),setContext:(key,fn)=>{if(key!=='mno:place-map')block=fn;},onMount:fn=>mount=fn,
    initializeLanguage(){},page,afterNavigate:fn=>afterNavigation=fn,
    goto:async path=>{destinations.push(path);await navigate(path);},listen:(_name,fn)=>{event=fn;return Promise.resolve(()=>{});},
    invoke:name=>new Promise((resolve,reject)=>calls.push({name,resolve,reject}))});
  vm.runInContext(script,context);afterNavigation({from:null,to:{url:page.url}});const dispose=mount();
  return {calls,destinations,dispose,navigate,event:()=>event(),block:()=>block(),run:code=>vm.runInContext(code,context)};
}

test('both verified import outcomes return home; restore retains journal navigation',async()=>{
  for (const [operation,action,destination,result] of [
    ['import','finish_new','/','imported'], ['import','continue','/','imported'],
    ['restore','finish_new','/journal','restored'], ['import','keep_previous','/','previous'],
    ['restore','keep_previous','/journal','previous'], ['unknown','finish_new','/journal','restored'],
  ]) {
    const root=layoutHarness(); await settle();root.calls[0].resolve('recovery');await settle();
    const panel=recoveryHarness('recovery',(restored,kind)=>root.run(`recoveryFinished(${restored},${JSON.stringify(kind)})`));
    panel.calls[0].resolve({operation,actions:[action]}); await settle();
    const pending=panel.run(`run("${action}")`);
    assert.deepEqual(root.destinations,[]); assert.equal(root.run('result'),null);
    panel.calls[1].resolve();await pending;
    assert.deepEqual(root.destinations,[destination]);assert.equal(root.run('result'),result);
    assert.equal(root.run('mode'),'recovery','frontend completion does not grant Ready');
    root.event();await settle();root.calls[1].resolve('ready');await settle();
    assert.equal(root.run('mode'),'ready');assert.equal(root.destinations.length,1);
    root.dispose();panel.dispose();
  }
});

test('failed verification and blocked or unknown modes never navigate home',async()=>{
  const root=layoutHarness();await settle();root.calls[0].resolve('recovery');await settle();
  const panel=recoveryHarness('recovery',(restored,kind)=>root.run(`recoveryFinished(${restored},${JSON.stringify(kind)})`));
  panel.calls[0].resolve({operation:'import',actions:['finish_new']});await settle();
  const pending=panel.run('run("finish_new")');panel.calls[1].reject('failed');await pending;
  assert.deepEqual(root.destinations,[]);assert.equal(root.run('result'),null);assert.equal(root.run('mode'),'recovery');
  for (const mode of ['blocked','unknown']) {
    root.run(`mode = "${mode}"`);await root.run('recoveryFinished(true,"import")');
    assert.deepEqual(root.destinations,[]);assert.equal(root.run('result'),null);
  }
  root.run('mode = "ready"');root.block();await root.run('recoveryFinished(true,"import")');
  assert.deepEqual(root.destinations,[]);root.dispose();panel.dispose();
});

test('normal startup does not redirect or show an import confirmation',async()=>{
  const root=layoutHarness();await settle();root.calls[0].resolve('ready');await settle();
  assert.equal(root.run('mode'),'ready');assert.equal(root.run('result'),null);assert.deepEqual(root.destinations,[]);
  root.dispose();
});


test('Ready event before recovery reply preserves the import destination without granting Ready locally',async()=>{
  const root=layoutHarness();await settle();root.calls[0].resolve('recovery');await settle();
  const panel=recoveryHarness('recovery',(restored,kind)=>root.run(`recoveryFinished(${restored},${JSON.stringify(kind)})`));
  panel.calls[0].resolve({operation:'import',actions:['finish_new']});await settle();
  const pending=panel.run('run("finish_new")');
  root.event();await settle();root.calls[1].resolve('ready');await settle();
  panel.dispose();
  assert.deepEqual(root.destinations,[]);assert.equal(root.run('result'),null);
  panel.calls[1].resolve();await pending;
  assert.equal(root.run('mode'),'ready');assert.equal(root.run('result'),'imported');assert.deepEqual(root.destinations,['/']);
  root.dispose();
});


test('import confirmation is visible only on its first home visit and cannot return through late replies',async()=>{
  const root=layoutHarness();await settle();root.calls[0].resolve('recovery');await settle();
  // Begin away from home to check that a pending result is not shown there.
  await root.navigate('/settings/import');
  const panel=recoveryHarness('recovery',(restored,kind)=>root.run(`recoveryFinished(${restored},${JSON.stringify(kind)})`));
  panel.calls[0].resolve({operation:'import',actions:['finish_new']});await settle();
  const pending=panel.run('run("finish_new")');
  assert.equal(root.run('result'),null);
  panel.calls[1].resolve();await pending;
  assert.deepEqual(root.destinations,['/']);assert.equal(root.run('page.url.pathname'),'/');
  root.event();await settle();root.calls[1].resolve('ready');await settle();
  const visible=()=>root.run('mode === "ready" && result === "imported" && page.url.pathname === "/"');
  assert.equal(visible(),true);
  await root.navigate('/?view=home#top');assert.equal(visible(),true,'same home visit');
  await root.navigate('/journal');
  assert.equal(visible(),false);assert.equal(root.run('result'),null);
  assert.equal(root.run('importConfirmationConsumed'),true);
  await root.run('recoveryFinished(true,"import")');
  assert.equal(root.run('page.url.pathname'),'/journal','late completion must not redirect');
  assert.equal(root.run('result'),null);assert.equal(root.destinations.length,1);
  await root.navigate('/');assert.equal(visible(),false);assert.equal(root.run('result'),null);
  root.event();await settle();root.calls[2].resolve('ready');await settle();assert.equal(visible(),false);
  const layout=readFileSync(new URL('../src/routes/+layout.svelte',import.meta.url),'utf8');
  const ast=parse(layout,{modern:true});let bannerCondition;
  function walk(value,parent) {
    if(!value||typeof value!=='object')return;
    if(value.type==='IfBlock' && layout.slice(value.test.start,value.test.end).includes('page.url.pathname'))bannerCondition=value;
    for(const child of Object.values(value)) {
      if(Array.isArray(child))child.forEach(item=>walk(item,value));else if(child&&typeof child==='object')walk(child,value);
    }
  }
  walk(ast.fragment);
  assert.ok(bannerCondition,'actual banner must be restricted to home');
  assert.equal(layout.slice(bannerCondition.test.start,bannerCondition.test.end),'result && (result !== "imported" || page.url.pathname === "/")');
  root.dispose();panel.dispose();
});

test('leaving home does not consume restore or previous-journal confirmations',async()=>{
  for(const restored of [true,false]) {
    const root=layoutHarness();await settle();root.calls[0].resolve('recovery');await settle();
    await root.run(`recoveryFinished(${restored},"restore")`);
    assert.deepEqual(root.destinations,['/journal']);assert.equal(root.run('result'),restored?'restored':'previous');
    await root.navigate('/');await root.navigate('/settings/restore');
    assert.equal(root.run('result'),restored?'restored':'previous');assert.equal(root.run('importConfirmationConsumed'),false);
    root.dispose();
  }
});


const replacementView = (target='new',key='current-proof') => ({operation:'import',actions:target==='old'?['continue']:['continue','keep_previous'],replacement:{target,existing_entry_count:2,imported_entry_count:1,previous_confirmation:target==='old'?null:key}});
test('v2 previous journal needs an explicit second confirmation and sends the exact backend key', async()=>{
  const h=recoveryHarness();h.calls[0].resolve(replacementView());await settle();
  h.run('requestAction("keep_previous")');assert.equal(h.run('confirmingPrevious'),true);assert.equal(h.calls.length,1);
  h.run('confirmingPrevious = false');assert.equal(h.calls.length,1,'cancel has no backend effect');
  await h.run('run("keep_previous")');assert.equal(h.calls.length,1,'cannot bypass the confirmation');
  h.run('requestAction("keep_previous")');const p=h.run('run("keep_previous")');
  assert.equal(h.calls[1].args.previousConfirmation,'current-proof');
  assert.equal(h.calls[1].args.action,'keep_previous');
  await h.run('run("keep_previous")');assert.equal(h.calls.length,2,'double confirmation cannot run twice');
  h.calls[1].resolve();await p;assert.equal(h.finished[0],false);
});
test('durable old uses previous-journal labels and reports previous after Continue', async()=>{
  const h=recoveryHarness();h.calls[0].resolve(replacementView('old'));await settle();
  assert.equal(h.run('actionLabel("continue")'),'replacement_continue_previous');
  assert.equal(h.run('replacement.target'),'old');
  const p=h.run('run("continue")');assert.equal(h.calls[1].args.previousConfirmation,undefined);
  h.calls[1].resolve();await p;assert.deepEqual(h.finished,[false]);
});
test('new target stays uncompleted until backend succeeds; a late Ready event cannot change chosen journal', async()=>{
  const h=recoveryHarness();h.calls[0].resolve(replacementView());await settle();
  assert.equal(h.run('actionLabel("continue")'),'replacement_continue_imported');
  const p=h.run('run("continue")');assert.equal(h.finished.length,0);
  h.dispose();h.calls[1].resolve();await p;assert.deepEqual(h.finished,[true]);
});
test('stale confirmation clears offered actions and reload requires a new explicit decision', async()=>{
  const h=recoveryHarness();h.calls[0].resolve(replacementView());await settle();
  h.run('requestAction("keep_previous")');const p=h.run('run("keep_previous")');
  h.calls[1].reject('restore_blocked');await p;
  assert.equal(h.run('failed'),true);assert.equal(h.run('confirmingPrevious'),false);assert.equal(h.run('actions.length'),0);assert.equal(h.finished.length,0);
  await h.run('run("keep_previous")');assert.equal(h.calls.length,2);
  const refresh=h.run('refresh()');h.calls[2].resolve(replacementView('new','fresh-proof'));await refresh;
  assert.equal(h.run('failed'),false);await h.run('run("keep_previous")');assert.equal(h.calls.length,3);
  h.run('requestAction("keep_previous")');const retry=h.run('run("keep_previous")');
  assert.equal(h.calls[3].args.previousConfirmation,'fresh-proof');h.calls[3].resolve();await retry;
  assert.deepEqual(h.finished,[false]);
});
test('missing confirmation never dispatches v2 rollback and late status is ignored', async()=>{
  const h=recoveryHarness();h.calls[0].resolve(replacementView('new',null));await settle();
  h.run('requestAction("keep_previous")');await h.run('run("keep_previous")');assert.equal(h.calls.length,1);
  const pending=h.run('refresh()');h.dispose();h.calls[1].resolve(replacementView('old'));await pending;
  assert.equal(h.run('replacement.target'),'new');await h.run('run("continue")');assert.equal(h.calls.length,2);
});
test('prepared notice and v2 confirmation markup use i18n and ordinary accessible buttons',()=>{
  const panel=readFileSync(new URL('../src/lib/components/RecoveryPanel.svelte',import.meta.url),'utf8');
  compile(panel,{generate:'client'});
  assert.match(panel,/if prepared.*m.import_prepared/s);
  assert.match(panel,/onclick=\{\(\) => run\("keep_previous"\)\}/);
  assert.match(panel,/replacement.target === "old" \? m.replacement_target_previous/);
  assert.match(panel,/replacement_previous_count\(\{ count: replacement.existing_entry_count \}/);
  assert.match(panel,/import_incoming_count\(\{ count: replacement.imported_entry_count \}/);
  assert.match(panel,/m.replacement_safety/);
  for(const locale of ['sv','en']){
    const messages=JSON.parse(readFileSync(new URL(`../messages/${locale}.json`,import.meta.url),'utf8'));
    for(const key of ['import_prepared','replacement_target_previous','replacement_target_imported','replacement_previous_confirm','replacement_action_failed'])assert.ok(messages[key]);
    assert.match(messages.replacement_previous_count,/\{count\}/);
    assert.match(messages.import_incoming_count,/\{count\}/);
    assert.ok(messages.replacement_safety);
    assert.match(messages.replacement_previous_confirm,/\{oldCount\}/);assert.match(messages.replacement_previous_confirm,/\{newCount\}/);
  }
});


test('v2 completion navigates by selected journal, including a durable old Continue',async()=>{
  for(const [target,result,destination] of [['new','imported','/'],['old','previous','/']]) {
    const root=layoutHarness();await settle();root.calls[0].resolve('recovery');await settle();
    const panel=recoveryHarness('recovery',(restored,kind)=>root.run(`recoveryFinished(${restored},${JSON.stringify(kind)})`));
    panel.calls[0].resolve(replacementView(target));await settle();
    const pending=panel.run('run("continue")');assert.equal(root.run('result'),null);
    assert.deepEqual(root.destinations,[],'no navigation before successful backend completion');
    panel.calls[1].resolve();await pending;
    assert.equal(root.run('result'),result);assert.equal(root.destinations[0],destination);
    root.event();await settle();root.calls[1].resolve('ready');await settle();
    assert.equal(root.run('mode'),'ready');
    assert.equal(root.run('mode !== "ready" && mode !== "checking"'),false,'Recovery controls are not rendered after Ready');
    panel.dispose();root.dispose();
  }
});


test('prepared replacement shows concise instructions without generic blocked-state repetition', () => {
  const panel = readFileSync(new URL('../src/lib/components/RecoveryPanel.svelte', import.meta.url), 'utf8');
  const ast = parse(panel, { modern: true });
  const main = ast.fragment.nodes.find(node => node.type === 'RegularElement' && node.name === 'main');
  const generic = main.fragment.nodes.find(node => node.type === 'IfBlock' && panel.slice(node.start, node.end).includes('m.database_recovery_blocked'));
  const condition = panel.slice(generic.test.start, generic.test.end);
  for (const [mode, prepared, expected] of [['blocked', true, false], ['blocked', false, true], ['recovery', false, true]]) {
    assert.equal(vm.runInNewContext(condition, { mode, prepared, replacement: null, failed: false, loading: false }), expected);
  }
  const blocked = main.fragment.nodes.find(node => node.type === 'IfBlock' && panel.slice(node.test.start, node.test.end) === 'mode === "blocked"');
  const prepared = blocked.consequent.nodes.find(node => node.type === 'IfBlock');
  assert.equal(panel.slice(prepared.test.start, prepared.test.end), 'prepared');
  const normal = prepared.consequent.nodes.map(node => panel.slice(node.start, node.end)).join('');
  assert.match(normal, /m.import_prepared\(/);
  assert.match(normal, /m.import_prepared_reopen\(/);
  assert.doesNotMatch(normal, /restore_reopen_instructions/);
  assert.match(prepared.alternate.nodes.map(node => panel.slice(node.start, node.end)).join(''), /restore_reopen_instructions/);
  assert.match(panel, /if quitFailed.*m.restore_quit_failed/s);
  for (const locale of ['sv', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
    assert.ok(messages.import_prepared_reopen);
    if (locale === 'sv') {
      assert.equal(messages.import_prepared, 'Journalbytet är förberett. Din tidigare journal och dess skyddskopia finns kvar.');
      assert.equal(messages.import_prepared_reopen, 'Avsluta MNO och starta appen igen för att välja vilken journal du vill använda och slutföra bytet.');
    }
  }
});
