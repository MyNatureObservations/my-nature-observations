import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const read=p=>readFileSync(new URL(p,import.meta.url),'utf8');
const script=p=>read(p).match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'').replace(/export /g,'');
const execute=(s,c)=>vm.runInContext(ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
test('new entry weather waits for the existing journal save and preserves data on failure',async()=>{
 for(const failure of ['journal','weather',null]){
  const calls=[];let c;
  const props=()=>({entryId:null,generation:7,locale:'sv',ensureEntry:async()=>{calls.push('journal');if(failure==='journal')throw 'journal-save-failed';vm.runInContext('entryId="created"',c)}});
  c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$effect:()=>{},untrack:f=>f(),tick:async()=>{},createWeatherReader:()=>async()=>({}),emptyWeather:{kind:'ready',data:{manual:null,fetched:null,revision:'0'}},invoke:async(cmd,args)=>{
    calls.push([cmd,args]);if(cmd==='update_manual_weather'){if(failure==='weather')throw 'weather-save-failed';return {manual:args.manual,fetched:null,revision:'1'}}return {generation:7};
  }});
  execute(script('../src/lib/components/EntryWeather.svelte'),c);
  vm.runInContext('status=emptyWeather',c);
  const value={period:{kind:'day',date:'2020-06-01'},temperatureC:16,weatherCode:0};c.value=value;
  if(failure)await assert.rejects(vm.runInContext('saveManual(value)',c));else assert.equal(await vm.runInContext('saveManual(value)',c),'verified');
  assert.equal(calls[0],'journal');
  if(failure==='journal')assert.equal(calls.length,1);else{assert.equal(calls[1][1].entryId,'created');assert.equal(calls[1][1].manual.temperatureC,16);assert.equal(calls[1][1].expectedRevision,'0');}
 }
});
test('weather form survives the first journal ID and exposes validated submission without an extra button click',()=>{
 const source=read('../src/lib/components/EntryWeather.svelte');
 assert.match(source, /#key `\$\{generation\}:\$\{editing\}`/);
 assert.doesNotMatch(source, /#key `\$\{entryId\}/);
 assert.match(source,/previous=untrack\(\(\)=>status\)/);
 const form=read('../src/lib/components/WeatherForm.svelte');assert.match(form,/export function getPart/);assert.match(form,/errors=parsed.errors/);
 const editor=read('../src/lib/components/JournalEditor.svelte');assert.ok(editor.indexOf('const saved = await saveEntry();')<editor.indexOf('weatherControl?.flushPending()'));
 assert.match(editor,/weatherDirty && !isNew/);
});
test('flush submits the actual weather form and a failed write retains the dirty draft',async()=>{
 for(const failure of [false,true]){
  let writes=0;const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'0'}},locale:'sv',editing:true,saveManual:async()=>{writes++;if(failure)throw 'failed'}});props.id=()=> 'test';
  const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,weatherLabels:{},tick:async()=>{}});
  execute(script('../src/lib/components/WeatherTile.svelte'),c);
  vm.runInContext('form=true;dirty=true;weatherForm={getPart:()=>({period:{kind:"day",date:"2020-06-01"},temperatureC:16})}',c);
  assert.equal(await vm.runInContext('flushManual()',c),!failure);assert.equal(writes,1);assert.equal(vm.runInContext('dirty',c),failure);
 }
});
test('header renders the sole weather control next to date/place and keeps it outside editor JSON',()=>{
 const source=read('../src/lib/components/JournalEditor.svelte');assert.equal((source.match(/<EntryWeather\s/g)||[]).length,1);
 assert.ok(source.indexOf('<EntryWeather')<source.indexOf('<article class="journal-entry">'));
 for(const p of ['../src/routes/journal/new/+page.svelte','../src/routes/journal/entries/[id]/+page.svelte']){const s=read(p);assert.match(s,/snippet metadata\(weather: Snippet\)/);assert.match(s,/@render weather\(\)/);}
 assert.match(read('../src/lib/components/WeatherTile.svelte'),/if\(editing && status.kind==='ready' && !status.data.manual\)form=true/);
 assert.match(read('../src/lib/components/WeatherTile.svelte'),/position:sticky;top:0/);
});
test('overview uses batched generation-checked weather and leaves weatherless entries alone',()=>{
 const source=read('../src/routes/journal/entries/+page.svelte');assert.match(source,/readWeather\(entries.map\(e=>e.id\),snapshot.generation\)/);
 assert.match(source, /weather\[entry.id\]\?\.manual \|\| weather\[entry.id\]\?\.fetched/);
 assert.doesNotMatch(source,/<EntryWeather|<WeatherTile/);
});
test('a late initial weather read cannot overwrite a confirmed first manual write',async()=>{
 let effect,resolve;
 const pending=new Promise(r=>resolve=r);
 const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:()=>({entryId:null,generation:7,locale:'sv'}),$state:x=>x,$effect:f=>effect=f,untrack:f=>f(),tick:async()=>{},emptyWeather:{kind:'ready',data:{manual:null,fetched:null,revision:'0'}},createWeatherReader:()=>()=>pending,
 invoke:async(cmd,args)=>cmd==='update_manual_weather'?{manual:args.manual,fetched:null,revision:'1'}:{generation:7}});
 execute(script('../src/lib/components/EntryWeather.svelte'),c);effect();vm.runInContext('entryId="created"',c);effect();
 await vm.runInContext('saveManual({period:{kind:"day",date:"2020-06-01"},temperatureC:16})',c);
 resolve({created:{manual:null,fetched:null,revision:'0'}});await Promise.resolve();await Promise.resolve();
 assert.equal(vm.runInContext('status.data.revision',c),'1');assert.equal(vm.runInContext('status.data.manual.temperatureC',c),16);
});
test('weather alone does not create an empty journal and failed submission keeps the inputs',async()=>{
 const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'0'}},locale:'sv',saveManual:async()=>{throw 'weather_new_entry_needs_text'}});props.id=()=> 'test';
 const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,weatherLabels:{},tick:async()=>{}});execute(script('../src/lib/components/WeatherTile.svelte'),c);
 vm.runInContext('form=true;dirty=true;weatherForm={getPart:()=>({period:{kind:"day",date:"2020-06-01"},temperatureC:16})}',c);
 assert.equal(await vm.runInContext('flushManual()',c),false);assert.equal(vm.runInContext('failure',c),'needText');assert.equal(vm.runInContext('dirty',c),true);assert.equal(vm.runInContext('form',c),true);
});
test('opening before the read completes still reveals the empty manual form, without writes',()=>{
 for(const manual of [null,{period:{kind:'day',date:'2020-06-01'},temperatureC:16}]){
  let effect,writes=0;const props=()=>({status:{kind:'loading'},locale:'sv',editing:true,saveManual:async()=>{writes++}});props.id=()=> 'test';
  const c=vm.createContext({$props:props,$state:x=>x,$derived:x=>x,$effect:f=>effect=f,untrack:f=>f(),weatherLabels:{},tick:async()=>{}});
  execute(script('../src/lib/components/WeatherTile.svelte'),c);vm.runInContext('open=true',c);effect();assert.equal(vm.runInContext('form',c),false);
  c.manual=manual;vm.runInContext('status={kind:"ready",data:{manual,fetched:null,revision:"0"}}',c);effect();assert.equal(vm.runInContext('form',c),manual===null);assert.equal(writes,0);
 }
});
