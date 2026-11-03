import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import {compile} from 'svelte/compiler';
import {render} from 'svelte/server';
import {historyUrl} from './weather-test-modules.mjs';
const read=path=>readFileSync(new URL(path,import.meta.url),'utf8');
const transpile=s=>ts.transpileModule(s.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
const helper=read('../src/lib/weather-history.ts').replace(/^import .*;$/gm,'').replace(/export /g,'');
const helpers=vm.createContext({tick:async()=>{},$effect:()=>{},untrack:f=>f(),invoke:async()=>{throw 'no automatic network'}});
vm.runInContext(transpile(helper),helpers);
const runHelper=code=>vm.runInContext(code,helpers);
test('place defaults prefer journal, then setting; missing coordinates require explicit selection including zero',()=>{
 for(const [journal,setting,key,missing] of [
  [{place:{id:'j',name:'J',latitude:0,longitude:0}},{place:{id:'d',name:'D',latitude:59,longitude:18}},'journal',false],
  [{place:null},{place:{id:'d',name:'D',latitude:59,longitude:18}},'default',false],
  [{place:{id:'j',name:'J',latitude:null,longitude:null}},{place:{id:'d',name:'D',latitude:59,longitude:18}},'',true],
  [{place:null},{place:null},'',false],
 ]){
  helpers.j=journal;helpers.d=setting;const result=runHelper('weatherPlaceDefaults(j,d)');assert.equal(result.selected,key);assert.equal(result.missingCoordinates,missing);
 }
 for(const p of [{latitude:NaN,longitude:0},{latitude:91,longitude:0},{latitude:0,longitude:null}]){helpers.p=p;assert.equal(runHelper('hasWeatherCoordinates(p)'),false);}
});
test('new unsaved journal proposes the default without linking an observation or fetching',async()=>{
 const h=harness();h.run('entryId=null;readWeatherDefaultPlace=async()=>({place:{id:"default",name:"Default lake",latitude:0,longitude:0},revision:"1"})');
 await h.run('load()');assert.equal(h.run('selected'),'default');h.run('choice=options[0]');await h.run('fetchPreview()');assert.deepEqual(h.calls,[]);
});
const source=read('../src/lib/components/HistoricalWeather.svelte');
const fetched={period:{kind:'day',date:'2020-06-01'},temperatureMeanC:12,precipitationSumMm:0,source:{provider:'open-meteo',model:'ERA5'}};
function harness({existing=null,previewFailure=null,saveFailure=null,readFailure=false}={}){
 const calls=[],busy=[],effects=[];let destroy, deferred=null;
 const props=()=>({entryId:'one',generation:7,date:'2020-06-01',revision:'1',locale:'sv',existing,onDone:()=>calls.push('done'),onBusy:value=>busy.push(value),onSaved:async()=>{calls.push('reload');if(readFailure)throw 'read failed'},onConflict:async()=>{calls.push('conflictReload')}});props.id=()=> 'history';
 const c=vm.createContext({tick:async()=>{},$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,$effect:f=>effects.push(f),untrack:f=>f(),onDestroy:f=>destroy=f,weatherLabels:{},
  weatherPlaceDefaults:runHelper('weatherPlaceDefaults'),hasWeatherCoordinates:runHelper('hasWeatherCoordinates'),historicalError:runHelper('historicalError'),filterPlaces:p=>p,
  readJournalPlace:async()=>({place:{id:'j',name:'Lake',latitude:0,longitude:0},revision:'1'}),readWeatherDefaultPlace:async()=>({place:null,revision:'0'}),listPlaces:async()=>[],
  previewHistoricalWeather:async(...args)=>{calls.push(['preview',...args]);if(deferred)return deferred;if(previewFailure)throw previewFailure;return {entryId:'one',previewId:'p',weatherRevision:'1',fetched}},
  saveHistoricalWeather:async(...args)=>{calls.push(['save',...args]);if(saveFailure)throw saveFailure;return {generation:7,data:{revision:'2'}}},
 });
 const script=source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'').replace('const choice=$derived(options.find(option=>option.key===selected));','let choice:any;');
 vm.runInContext(transpile(script),c);
 const run=s=>vm.runInContext(s,c);
 return {run,calls,busy,setLabels:labels=>Object.assign(c.weatherLabels,labels),destroy:()=>destroy(),effect:()=>effects.forEach(f=>f()),delay:()=>{let resolve;deferred=new Promise(r=>resolve=r);return ()=>resolve({entryId:'one',previewId:'p',weatherRevision:'1',fetched})},prepare:async()=>{await run('beginReplacement()');run('choice=options[0]');}};
}
test('real handlers fetch only on request, preview without saving, then save explicitly and reload',async()=>{
 const h=harness();await h.prepare();assert.deepEqual(h.calls,[]);
 await h.run('fetchPreview()');assert.equal(h.run('preview.previewId'),'p');assert.equal(h.calls.length,1);
 await h.run('save()');assert.equal(h.calls[1][0],'save');assert.equal(h.calls[1][4],false);assert.equal(h.calls[2],'reload');assert.equal(h.run('notice'),'saved');assert.equal(h.run('preview'),null);
});
test('replacement needs its own confirmation; discard never writes and manual weather is never sent',async()=>{
 const h=harness({existing:fetched});await h.prepare();await h.run('fetchPreview()');await h.run('save()');assert.equal(h.calls.length,1);
 await h.run('save(true)');assert.equal(h.calls[1][4],true);assert.equal(h.calls[1][3].previewId,'p');
 assert.doesNotMatch(JSON.stringify(h.calls),/manual/);
 const cancel=harness();await cancel.prepare();await cancel.run('fetchPreview()');cancel.run('invalidate()');await cancel.run('save()');assert.equal(cancel.calls.length,1);
});
test('fetch errors retain stored data and conflicts discard stale preview and reload',async()=>{
 for(const code of ['timeout','network','no_data','invalid_response','date_unavailable','historical_weather_context_conflict']){
  const h=harness({previewFailure:code,existing:fetched});await h.prepare();await h.run('fetchPreview()');assert.equal(h.run('preview'),null);assert.ok(h.run('error'));assert.equal(h.calls.filter(x=>x[0]==='save').length,0);
 }
 const h=harness({saveFailure:'weather_revision_conflict'});await h.prepare();await h.run('fetchPreview()');await h.run('save()');assert.ok(h.calls.includes('conflictReload'));assert.equal(h.run('error'),'conflict');assert.equal(h.run('preview'),null);
});
test('confirmed save plus failed reread is reported as saved but unverified',async()=>{
 const h=harness({readFailure:true});await h.prepare();await h.run('fetchPreview()');await h.run('save()');assert.equal(h.run('error'),'savedUnverified');assert.equal(h.run('preview'),null);assert.ok(h.calls.includes('reload'));
});
test('double actions, late responses, date and selection changes cannot save a stale preview',async()=>{
 for(const action of ['select("default")','date="2020-06-02";invalidate()','generation=8;invalidate()','entryId="two";invalidate()','destroy']){
  const h=harness();await h.prepare();const resolve=h.delay();const request=h.run('fetchPreview()');await h.run('fetchPreview()');assert.equal(h.calls.length,1);
  if(action==='destroy')h.destroy();else h.run(action);
  resolve();await request;assert.equal(h.run('preview'),null);
 }
 const h=harness();await h.prepare();await h.run('fetchPreview()');h.run('blocked=true');await h.run('save()');assert.equal(h.calls.length,1);
 const pending=harness();await pending.prepare();pending.run('contextPending=true');await pending.run('fetchPreview()');assert.equal(pending.calls.length,0);
});
test('component compiles and SSR does not fetch; both languages identify unsaved preview and privacy',async()=>{
 assert.equal(compile(source,{generate:'client'}).warnings.length,0);
 const {default:History}=await import(historyUrl);
 for(const locale of ['sv','en']){
  const html=render(History,{props:{entryId:'one',generation:7,date:'2020-06-01',revision:'1',existing:null,locale}}).body;
  assert.match(html,locale==='sv'?/Hämtat väder/:/Fetched weather/);
  const messages=JSON.parse(read(`../messages/${locale}.json`));for(const key of ['preview','privacy','replaceConfirm','savedUnverified'])assert.ok(messages[`weather_history_${key}`]);
 }
});

test('adapters use the registered command contract, never send manual data, and reject a wrong preview identity',async()=>{
 const calls=[];let response={generation:7,data:{entryId:'one',previewId:'p',weatherRevision:'1',fetched}};
 const c=vm.createContext({tick:async()=>{},$effect:()=>{},untrack:f=>f(),invoke:async(command,args)=>{calls.push({command,args});return response}});
 vm.runInContext(transpile(helper),c);
 const run=s=>vm.runInContext(s,c);
 await run('previewHistoricalWeather("one",7,{kind:"default"})');
 assert.equal(calls[0].command,'preview_historical_weather');assert.equal(calls[0].args.source.kind,'default');
 c.p=response.data;response={generation:7,data:{revision:'2'}};
 await run('saveHistoricalWeather("one",7,p,true)');assert.equal(calls[1].command,'save_historical_weather');assert.equal(calls[1].args.replaceConfirmed,true);assert.equal(calls[1].args.expectedRevision,'1');assert.equal(calls[1].args.previewId,'p');assert.ok(!('manual' in calls[1].args));assert.ok(!('fetched' in calls[1].args));
 for(const bad of [{generation:8,data:{entryId:'one'}},{generation:7,data:{entryId:'two'}}]){response=bad;await assert.rejects(run('previewHistoricalWeather("one",7,{kind:"journal"})'));}
});

test('long-lived EntryWeather invalidates before reread; a failed reread cannot resurrect old weather on remount',async()=>{
 const script=read('../src/lib/components/EntryWeather.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 let allow=false;
 const old={manual:{period:fetched.period,temperatureC:17},fetched,revision:'1'};
 const next={manual:old.manual,fetched:{...fetched,temperatureMeanC:4},revision:'2'};
 const c=vm.createContext({tick:async()=>{},$effect:()=>{},untrack:f=>f(),$props:()=>({entryId:'one',generation:7,locale:'sv'}),$state:x=>x,$effect:()=>{},emptyWeather:{kind:'ready',data:old},createWeatherReader:()=>async()=>{if(!allow)throw 'unavailable';return {one:next}}});
 vm.runInContext(transpile(script),c);const run=s=>vm.runInContext(s,c);c.old=old;run('status={kind:"ready",data:old}');
 await assert.rejects(run('historicalSaved()'));assert.equal(run('status.kind'),'unverified');
 assert.match(read('../src/lib/components/WeatherTile.svelte'),/blocked=\{busy\|\|unverified\|\|contextPending\}/);
 allow=true;await run('reload()');assert.equal(run('status.kind'),'ready');assert.equal(run('status.data.revision'),'2');assert.equal(run('status.data.manual.temperatureC'),17);
});

test('historical request blocks panel closure and manual writes without discarding the manual draft',async()=>{
 let script=read('../src/lib/components/WeatherTile.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 let writes=0;const drafts=[];const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'1'}},locale:'sv',saveManual:async()=>{writes++},onDraft:(...args)=>drafts.push(args)});props.id=()=> 'tile';
 const c=vm.createContext({tick:async()=>{},$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,weatherLabels:{},tick:async()=>{}});
 vm.runInContext(transpile(script),c);const run=s=>vm.runInContext(s,c);
 run('open=true;form=true;draftChanged(true);setHistoryBusy(true)');assert.equal(run('close()'),false);await run('persist({})');assert.equal(writes,0);assert.equal(run('dirty'),true);assert.equal(run('form'),true);
 run('setHistoryBusy(false)');assert.deepEqual(drafts.at(-1),[true,false]);assert.equal(run('requestDiscard("panel")'),false);
});
test('a current preview switches the whole panel step without viewport scrolling; late responses cannot switch it',async()=>{
 for(const stale of [false,true]){
  const h=harness();await h.prepare();
  h.run('let scrollCalls=[];let focusCalls=0;previewElement={scrollIntoView:options=>scrollCalls.push(options),focus:()=>focusCalls++}');
  const resolve=h.delay();const pending=h.run('fetchPreview()');if(stale)h.run('select("default")');resolve();await pending;
  assert.equal(h.run('scrollCalls.length'),0);assert.equal(h.run('step'),stale?'fetch':'compare');assert.equal(h.run('focusCalls'),0);
 }
});
test('wide panel keeps history and its preview in the right column and stacks on narrow screens',()=>{
 const tile=read('../src/lib/components/WeatherTile.svelte');
 const right=tile.slice(tile.indexOf('<section class="historical-column"'),tile.indexOf('{#if unverified}<p role='));
 assert.match(right,/<HistoricalWeather/);assert.match(right,/stored-weather/);
 assert.match(tile,/width: min\(1000px, calc\(100vw - 32px\)\)/);
 assert.match(tile,/@media \(max-width: 800px\)/);assert.match(tile,/grid-template-columns: minmax\(0,1fr\)/);
 assert.match(tile,/position:sticky;top:0/);assert.match(tile,/if\(open\) void reposition\(\)/);
 assert.match(source,/step === "compare" && preview/);
 assert.ok(source.indexOf('<div class="actions">')>source.indexOf('<WeatherDetails part={preview.fetched}'));
});
test('saved weather renders first, with attribution and replacement button but no fetch controls',async()=>{
 const {default:History}=await import(historyUrl);
 const saved={...fetched,source:{...fetched.source,license:'CC BY 4.0',attribution:'Open-Meteo'}};
 for(const locale of ['sv','en']){
  const html=render(History,{props:{entryId:'one',generation:7,date:'2020-06-01',revision:'1',existing:saved,locale}}).body;
  assert.match(html,/12 °C/);assert.match(html,/https:\/\/open-meteo.com\//);assert.match(html,/ERA5/);assert.match(html,/CC BY 4.0/);
  assert.match(html,locale==='sv'?/Ersätt hämtat väder/:/Replace fetched weather/);
  assert.doesNotMatch(html,/<select|type="search"|Hämta historiskt väder|Fetch historical weather/);
  assert.ok(html.indexOf('12 °C')<html.indexOf('replace-control'));
 }
});
test('replacement opens explicitly, cancellation restores display without writing and comparison keeps both snapshots',async()=>{
 const original=structuredClone(fetched),h=harness({existing:original});
 assert.equal(h.run('step'),'overview');
 await h.run('load()');h.run('choice=options[0]');await h.run('fetchPreview()');assert.deepEqual(h.calls,[]);
 await h.run('beginReplacement()');h.run('choice=options[0]');assert.equal(h.run('step'),'fetch');await h.run('fetchPreview()');assert.equal(h.run('preview.previewId'),'p');
 await h.run('discardPreview()');assert.equal(h.run('preview'),null);assert.equal(h.run('step'),'overview');await h.run('save()');assert.equal(h.calls.length,1);
 assert.deepEqual(original,fetched);
 assert.match(source,/class="comparison"/);assert.match(source,/<WeatherDetails part=\{existing\}/);assert.match(source,/<WeatherDetails part=\{preview.fetched\}/);
 assert.match(source,/@media\(max-width:600px\)/);
});
test('replacement uses three exclusive steps and a single explicit confirmation, without a checkbox',async()=>{
 const h=harness({existing:fetched});assert.equal(h.run('step'),'overview');
 await h.run('beginReplacement()');h.run('choice=options[0]');assert.equal(h.run('step'),'fetch');
 await h.run('fetchPreview()');assert.equal(h.run('step'),'compare');
 await h.run('save()');assert.equal(h.calls.filter(c=>c[0]==='save').length,0);
 await h.run('save(true)');assert.equal(h.calls.filter(c=>c[0]==='save').length,1);assert.equal(h.run('step'),'overview');
 assert.equal(h.calls.find(c=>c[0]==='save')[4],true);
 assert.doesNotMatch(source,/type="checkbox"/);
 assert.match(source,/{:else if step === "fetch"}/);assert.match(source,/{:else if step === "compare" && preview}/);
});
test('one common panel scroll surface replaces column scrolls, while the manual form remains mounted',async()=>{
 const tile=read('../src/lib/components/WeatherTile.svelte');
 assert.match(tile,/\.weather-panel \{[^}]*overflow: auto/);
 assert.doesNotMatch(tile,/\.sources[^}]*overflow:\s*auto/);
 assert.match(tile,/hidden=\{historyStep !== 'overview'\}/);
 assert.doesNotMatch(tile,/{#if historyStep[^}]*}[\s\S]*?<WeatherForm/);
 assert.match(tile,/panel.scrollTop=0/);assert.doesNotMatch(source,/scrollIntoView/);
 assert.doesNotMatch(source,/max-height:10rem;overflow:auto/);
 const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'1'}},locale:'sv'});props.id=()=> 'step';
 const c=vm.createContext({$props:props,$state:x=>x,$derived:x=>x,$effect:()=>{},untrack:f=>f(),tick:async()=>{},weatherLabels:{},document:{activeElement:{},body:{}}});
 const tileScript=tile.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 vm.runInContext(transpile(tileScript),c);
 vm.runInContext('open=true;form=true;dirty=true;weatherForm={getPart:()=>null};const originalForm=weatherForm;panel={contains:()=>false,scrollTop:300}',c);
 for(const step of ['fetch','compare','overview']){
  await vm.runInContext(`setHistoryStep("${step}")`,c);
  assert.equal(vm.runInContext('weatherForm===originalForm && dirty && form',c),true);assert.equal(vm.runInContext('panel.scrollTop',c),0);
 }
 vm.runInContext('let cancellations=0;historyStep="compare";historyControl={discardPreview:async()=>{cancellations++}}',c);
 vm.runInContext('keydown({key:"Escape",preventDefault(){},stopPropagation(){}})',c);
 assert.equal(vm.runInContext('cancellations',c),1);assert.equal(vm.runInContext('dirty',c),true);assert.equal(vm.runInContext('open',c),true);
});

test('Done closes only after confirmed save and reread, without another write or fetch',async()=>{
 const h=harness();h.run('finish()');assert.deepEqual(h.calls,[]);
 await h.prepare();await h.run('fetchPreview()');await h.run('save(true)');
 assert.equal(h.run('notice'),'saved');assert.equal(h.run('step'),'overview');
 const before=h.calls.length;h.run('finish()');assert.equal(h.calls.length,before+1);assert.equal(h.calls.at(-1),'done');
 const failed=harness({readFailure:true});await failed.prepare();await failed.run('fetchPreview()');await failed.run('save(true)');failed.run('finish()');assert.ok(!failed.calls.includes('done'));
 assert.match(source,/notice === "saved" && step === "overview"/);
 assert.ok(read('../src/lib/components/WeatherTile.svelte').includes('onDone={()=>close(true)}'));
});

test('service, rate-limit, network and unknown fetch failures select distinct localized messages', async () => {
 const messages=await import('../src/lib/paraglide/messages.js');
 const labelSource=read('../src/lib/weather-labels.ts').replace(/^import .*;$/gm,'');
 const labelContext=vm.createContext({m:messages});vm.runInContext(transpile(labelSource),labelContext);
 const labels=vm.runInContext('weatherLabels',labelContext);
 const cases=[
  ['service_unavailable','serviceUnavailable',
   'Vädertjänsten är tillfälligt otillgänglig. Försök igen senare.',
   'The weather service is temporarily unavailable. Try again later.'],
  ['rate_limited','rateLimited',
   'Vädertjänsten tar inte emot fler förfrågningar just nu. Försök igen senare.',
   'The weather service is not accepting more requests right now. Try again later.'],
  ['network','network',
   'Kunde inte ansluta till vädertjänsten. Kontrollera internetanslutningen.',
   'Could not connect to the weather service. Check your internet connection.'],
  ['http','fetchFailed',
   'Väderuppgifterna kunde inte hämtas. Försök igen senare.',
   'The weather data could not be fetched. Try again later.'],
 ];
 for(const [code,key,sv,en] of cases){
  helpers.cause=code;assert.equal(runHelper('historicalError(cause, true)'),key);
  const h=harness({previewFailure:code,existing:fetched});await h.prepare();await h.run('fetchPreview()');
  assert.equal(h.run('error'),key);assert.equal(h.run('preview'),null);
  h.setLabels(labels);
  for(const [locale,expected] of [['sv',sv],['en',en]]){
   assert.equal(h.run(`locale="${locale}"; label(error)`),expected);
  }
  assert.equal(h.run('existing'),fetched);
  assert.equal(h.calls.filter(call=>call[0]==='save').length,0);
 }
});

test('unknown fetch errors do not blame connectivity or change generic save/load failures',()=>{
 for(const cause of ['api','http','historical_weather_preview_unavailable','something unexpected']){
  helpers.cause=cause;assert.equal(runHelper('historicalError(cause, true)'),'fetchFailed');
 }
 helpers.cause='something unexpected';assert.equal(runHelper('historicalError(cause)'),'failed');
 helpers.cause='timeout';assert.equal(runHelper('historicalError(cause, true)'),'timeout');
 helpers.cause='invalid_response';assert.equal(runHelper('historicalError(cause, true)'),'invalidResponse');
});
