import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
const read=p=>readFileSync(new URL(p,import.meta.url),'utf8');
function lifecycle(fail=false){
 const events=new EventTarget(),calls=[];
 const context=vm.createContext({$state:v=>v,window:events,CustomEvent,Event,Promise,invoke:async(command,args)=>{calls.push({command,args});if(fail)throw Error('disk');}});
 const source=read('../src/lib/bird-module.svelte.ts').replace(/^import .*;\n/gm,'').replace(/^export /gm,'');
 vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {events,calls,run:s=>vm.runInContext(s,context)};
}
test('module lifecycle waits for safeguards and broadcasts only after authoritative success',async()=>{
 const h=lifecycle();let finish;h.events.addEventListener('mno:before-bird-disable',e=>e.detail.waitUntil(new Promise(r=>finish=r)));
 const pending=h.run('approveBirdDisable()');finish(false);assert.equal(await pending,false);assert.equal(h.calls.length,0);
 let changed=0;h.events.addEventListener('mno:journal-changed',()=>changed++);
 await h.run('setBirdModule(true)');assert.equal(h.calls[0].command,'set_bird_module_enabled');assert.equal(h.run('birdModule.revision'),1);assert.equal(changed,1);
 const failed=lifecycle(true);await assert.rejects(failed.run('setBirdModule(false)'));assert.equal(failed.run('birdModule.revision'),0);
});
test('every registered bird command has an authoritative activation guard',()=>{
 for(const file of ['bird_app.rs','bird_photos.rs','bird_weather_fetch.rs']){
  const source=read(`../src-tauri/src/${file}`);
  const commands=[...source.matchAll(/#\[tauri::command\]\s*pub\(crate\) (?:async )?fn (\w+)/g)];
  for(let i=0;i<commands.length;i++){
   const name=commands[i][1],body=source.slice(commands[i].index,commands[i+1]?.index??source.length);
   if(name==='bird_observations_available')assert.match(body,/module.enabled\(\)/);
   else if(['search_bird_species','match_bird_paste_names'].includes(name))assert.match(body,/_with_module\(&module/);
   else assert.match(body,/module.command\(\)\?/ ,name);
  }
 }
});
test('preference stays outside journal contracts and production/DEV identifiers differ',()=>{
 const lib=read('../src-tauri/src/lib.rs');assert.match(lib,/preference_path\(\s*&app.path\(\).config_dir\(\)\?,\s*&app.config\(\).identifier,?\s*\)/);
 const prod=JSON.parse(read('../src-tauri/tauri.conf.json')),dev=JSON.parse(read('../src-tauri/tauri.dev.conf.json'));
 assert.notEqual(prod.identifier,dev.identifier);
 const setting=read('../src/lib/components/BirdModuleSetting.svelte');assert.match(setting,/approveBirdDisable/);assert.match(setting,/currentTarget.checked=enabled/);
 for(const locale of ['sv','en']){const m=JSON.parse(read(`../messages/${locale}.json`));for(const k of ['modules_heading','modules_birds','modules_birds_explanation','modules_pending','modules_failed'])assert.ok(m[k]);}
});

test('module refresh preserves core editor instances and journal gallery keeps core photos',()=>{
 const layout=read('../src/routes/+layout.svelte');assert.doesNotMatch(layout,/#key birdModule/);
 const home=read('../src/routes/+page.svelte');assert.match(home,/mno:bird-module-changed/);
 const gallery=read('../src/lib/journal-photo-gallery.ts');assert.match(gallery,/items:\[\.\.\.journal,\.\.\.observationPhotos\(linked,locale\)\]/);
 const form=read('../src/lib/components/BirdListForm.svelte');assert.match(form,/available===false/);assert.match(form,/entryId\?entryHref\(entryId\):"\/"/);
 const detail=read('../src/lib/components/BirdListPage.svelte');assert.match(detail,/available===false/);assert.match(detail,/restore_back_to_home/);
});
