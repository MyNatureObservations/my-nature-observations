import assert from 'node:assert/strict';
import test from 'node:test';
import { render } from 'svelte/server';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { weatherUrl, tileUrl, detailsUrl, overviewUrl } from './weather-test-modules.mjs';
const { weatherRows, weatherSections, weatherConditionKey, weatherIcon, weatherOverview, weatherClock } = await import(weatherUrl);
const { default: Tile } = await import(tileUrl);
const { default: Details } = await import(detailsUrl);
const { default: Overview } = await import(overviewUrl);
const part = {period:{kind:'day',date:'2026-10-25'},temperatureC:0,precipitationMm:0,weatherCode:0,description:'<script>not HTML</script>'};
const currentPart={period:{kind:'day',date:'2026-10-30'},temperatureC:16,windSpeedMs:0,precipitationMm:0,weatherCode:0,validAt:'2026-10-30T13:45:00+02:00',currentIntervalSeconds:900,retrievedAt:'2026-10-30T12:00:00Z',source:{provider:'open-meteo',product:'current-weather',model:'ECMWF IFS',dataKind:'forecast',attribution:'Open-Meteo; ECMWF',license:'CC BY 4.0'}};
const show = (status,locale='sv',editing=false) => render(Tile,{props:{status,locale,editing}}).body;
const empty = {kind:'ready',data:{manual:null,fetched:null,revision:'0'}};
const clean = html => html.replace(/<!--[\s\S]*?-->/g,'').trim();
test('current snapshots retain valid time, interval and model meaning without daily mean or sum labels',()=>{
 for(const locale of ['sv','en']){
  const html=render(Details,{props:{part:currentPart,locale}}).body;
  const primary=html.split('<details')[0];assert.match(primary,/13:45/);assert.match(primary,/15 min/);assert.match(primary,/16 °C/);assert.match(primary,/0 mm/);assert.match(primary,/0 m\/s/);
  assert.match(primary,locale==='sv'?/Aktuellt väder/:/Current weather/);assert.match(primary,locale==='sv'?/Modellberäknat väder/:/Modelled weather/);
  assert.doesNotMatch(primary,/Medeltemperatur|Mean temperature|Total nederbörd|Total precipitation/);
  const overview=render(Overview,{props:{data:{manual:null,fetched:currentPart,revision:'1'},locale,compact:true}}).body;
  assert.match(overview,/16 °C/);assert.match(overview,/13:45/);assert.doesNotMatch(overview,/Medeltemperatur|Mean temperature/);
  assert.match(html,/2026-10-30T12:00:00Z/);assert.match(html,/ECMWF IFS/);
 }
 const data={manual:{period:part.period,temperatureC:0,weatherCode:61,windSpeedMs:0},fetched:currentPart,revision:'1'};
 assert.equal(weatherOverview(data).temperature.value,0);assert.equal(weatherOverview(data).temperature.source,'manual');assert.equal(weatherOverview(data).condition.code,61);
 const html=render(Overview,{props:{data,locale:'sv',compact:true}}).body;assert.match(html,/0 °C/);assert.doesNotMatch(html,/13:45/);
});
test('stored current and completed model days remain distinct independently of the date viewed',()=>{
 const day={...currentPart,source:{...currentPart.source,product:'forecast-daily'},temperatureC:undefined,temperatureMeanC:12,windSpeedMs:undefined,windSpeedMaxMs:2,precipitationMm:undefined,precipitationSumMm:0,validAt:undefined,currentIntervalSeconds:undefined};
 const currentHtml=render(Details,{props:{part:currentPart,locale:'sv'}}).body;
 const dayHtml=render(Details,{props:{part:day,locale:'sv'}}).body;
 assert.match(currentHtml,/Aktuellt väder/);assert.doesNotMatch(currentHtml,/Avslutat modelldygn/);assert.match(dayHtml,/Avslutat modelldygn/);assert.doesNotMatch(dayHtml,/Aktuellt väder/);
 assert.equal(weatherOverview({manual:null,fetched:day,revision:'1'}).temperature.field,'temperatureMeanC');
 assert.equal(weatherOverview({manual:null,fetched:currentPart,revision:'1'}).temperature.field,'temperatureC');
});
test('zero values survive presentation; missing values remain absent',()=>{
 assert.equal(weatherOverview({manual:part,fetched:null,revision:'1'}).temperature.value,0);
 const rows=weatherRows(part);
 assert.ok(rows.some(([k,v])=>k==='precipitationMm'&&v==='0 mm'));
 assert.ok(rows.some(([k,v])=>k==='weatherCode'&&v==='0'));
 assert.ok(!rows.some(([k])=>k==='windSpeedMs'));
 assert.equal(weatherOverview({manual:{period:part.period},fetched:null,revision:'1'}).temperature,null);
});
test('empty weather has no reading markup and a compact neutral editing control',()=>{
 for (const locale of ['sv','en']) {
  assert.equal(clean(show(empty,locale)), '');
  assert.equal(clean(show({...empty,data:{...empty.data,revision:'9'}},locale)), '');
  const html=show(empty,locale,true);
  assert.match(html,/compact/); assert.match(html,/<svg[^>]*aria-hidden="true"/);
  assert.match(html,locale==='sv'?/>Väder</:/>Weather</);
  assert.doesNotMatch(html,/title="[^"]*(?:read-only|skrivskyddat)/);
  assert.doesNotMatch(html,/Lägg till|Add weather/);
 }
});
test('loading and read errors remain visible in reading and editing modes',()=>{
 for (const locale of ['sv','en']) for (const editing of [false,true]) {
  assert.match(show({kind:'loading'},locale,editing),locale==='sv'?/Läser väder/:/Loading weather/);
  assert.match(show({kind:'error'},locale,editing),locale==='sv'?/Vädret kunde inte läsas/:/Weather could not be read/);
  assert.doesNotMatch(show({kind:'error'},locale,editing),/Inget väder sparat|No weather saved/);
 }
});
test('details render actual primary measurements before collapsed metadata, without mutations',()=>{
 const full={...part,windSpeedMs:0,timezone:'Europe/Stockholm',location:{coordinates:{latitude:0,longitude:0}}};
 const before=JSON.stringify(full);
 const sections=weatherSections(full);
 assert.deepEqual(sections.primary.map(([k])=>k),['temperatureC','description','weatherCode','windSpeedMs','precipitationMm']);
 for(const [locale,condition,detailLabel] of [['sv','Klart','Fler detaljer'],['en','Clear','More details']]) {
  const html=render(Details,{props:{part:full,locale}}).body;
  const [primary,metadata]=html.split('<details');
  assert.match(primary,/0 °C/);assert.match(primary,/0 m\/s/);assert.match(primary,/0 mm/);
  assert.match(primary,new RegExp(condition)); assert.doesNotMatch(primary,/Europe\/Stockholm|Koordinater|Coordinates/);
  assert.match(metadata,new RegExp(detailLabel));assert.match(metadata,/Europe\/Stockholm/);assert.match(metadata,/0, 0/);
  assert.doesNotMatch(html,/<details[^>]*\bopen(?:=|\s|>)/);
  assert.doesNotMatch(primary,/&mdash;|>—</);
  assert.ok(!html.includes('<script>not HTML</script>'));assert.match(html,/&lt;script/);
 }
 assert.equal(JSON.stringify(full),before);
});
test('primary and details together retain all stored rows, including the exact condition code',()=>{
 const full={...part,temperatureMinC:-1,temperatureMaxC:1,relativeHumidityPercent:0,cloudCoverPercent:0,snowDepthMm:0,retrievedAt:'2026-10-26T09:00:00Z',sunrise:'2026-10-25T08:00:00+01:00',source:{provider:'open-meteo',product:'archive',model:'model',dataKind:'reanalysis',sampledCoordinates:{latitude:0,longitude:0},attribution:'Test',license:'CC BY',licenseUrl:'https://example.invalid/license'}};
 const sections=weatherSections(full);
 for(const [k,v] of weatherRows(full)) assert.ok([...sections.primary,...sections.details].some(([key,value])=>key===k&&value===v));
 assert.ok(sections.details.some(([k,v])=>k==='weatherCode'&&v==='0'));
 const noMeasurements=render(Details,{props:{part:{period:part.period},locale:'sv'}}).body;
 assert.doesNotMatch(noMeasurements,/°C|m\/s|0 mm|>—</);
 for(const code of [0,1,2,3,45,48,51,53,55,56,57,61,63,65,66,67,71,73,75,77,80,81,82,85,86,95,96,97,99]) assert.ok(weatherConditionKey(code));
 assert.equal(weatherConditionKey(255),undefined);
});
test('both sources retain independent summaries and escaped descriptions',()=>{
 const html=show({kind:'ready',data:{manual:part,fetched:{period:part.period,temperatureMeanC:12},revision:'9223372036854775807'}});
 assert.match(html,/0 °C/); assert.doesNotMatch(html,/12 °C/);
 assert.match(render(Details,{props:{part:{period:part.period,temperatureMeanC:12},locale:"sv"}}).body,/12 °C/);
 const escaped=show({kind:'ready',data:{manual:{...part,temperatureC:null,weatherCode:null},fetched:null,revision:'1'}});
 assert.ok(!escaped.includes('<script>not HTML</script>'));
 assert.doesNotMatch(escaped,/not HTML/);
 assert.match(render(Details,{props:{part,locale:"sv",manual:true}}).body,/&lt;script/);
 assert.match(html,/aria-haspopup="dialog"/); assert.match(html,/aria-expanded="false"/);
});
test('details preserve source timestamps, timezone, coordinates and attribution',()=>{
 const p={period:{kind:'hour',start:'2026-10-25T02:00:00+02:00',endExclusive:'2026-10-25T02:00:00+01:00'},timezone:'Europe/Stockholm',location:{coordinates:{latitude:0,longitude:0}},sunrise:'2026-10-25T08:00:00+01:00',source:{provider:'open-meteo',product:'archive',dataKind:'reanalysis',sampledCoordinates:{latitude:1,longitude:2},attribution:'Test source',license:'CC BY'}};
 const rows=Object.fromEntries(weatherRows(p)); assert.equal(rows.timezone,p.timezone); assert.equal(rows.coordinates,'0, 0'); assert.equal(rows.sunrise,p.sunrise); assert.equal(rows.attribution,'Test source'); assert.match(rows.period,/\+02:00.*\+01:00/);
});
function readerHarness(invoke){
 const context=vm.createContext({$effect:()=>{},untrack:f=>f(),invoke});
 let reader=readFileSync(new URL('../src/lib/journal-data.ts',import.meta.url),'utf8');
 let weather=readFileSync(new URL('../src/lib/weather.ts',import.meta.url),'utf8');
 const strip=s=>s.replace(/^import .*;\n/gm,'').replace(/^export /gm,'');
 vm.runInContext(ts.transpileModule(strip(reader)+'\n'+strip(weather),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return vm.runInContext('createWeatherReader()',context);
}
test('weather batch loads use expected generation and only read commands',async()=>{
 const calls=[]; const read=readerHarness(async(command,args)=>{calls.push({command,args});return command==='new_journal_session'?{generation:3,data:null}:{generation:3,data:Object.fromEntries(args.ids.map(id=>[id,{manual:null,fetched:null,revision:'9223372036854775807'}]))};});
 const ids=Array.from({length:201},(_,i)=>`id${i}`); const result=await read(ids,3);
 assert.equal(Object.keys(result).length,201); assert.equal(result.id200.revision,'9223372036854775807');
 assert.deepEqual(calls.map(c=>c.command),['read_journal_weather','new_journal_session','read_journal_weather','new_journal_session','read_journal_weather','new_journal_session']);
 assert.ok(calls.filter(c=>c.args).every(c=>c.args.generation===3&&c.args.ids.length<=100));
});
test('weather cannot attach to a journal from a different generation or missing entry',async()=>{
 const stale=readerHarness(async()=>({generation:2,data:{a:{}}}));
 await assert.rejects(stale(['a'],1),e=>e==='journal_generation_conflict');
 const missing=readerHarness(async()=>({generation:1,data:{}}));
 await assert.rejects(missing(['a'],1),e=>e==='weather_entry_missing');
});

test('explicit codes select sun, cloud, rain, snow and fog; measurements never infer a condition',()=>{
 for (const [code,icon] of [[0,'sun'],[1,'sun'],[2,'sun-cloud'],[3,'cloud'],[61,'rain'],[71,'snow'],[45,'fog'],[95,'thunder'],[null,'neutral'],[undefined,'neutral'],[255,'neutral']]) {
  assert.equal(weatherIcon(code),icon);
  const data={manual:{period:part.period,weatherCode:code,temperatureC:-5,precipitationMm:100,snowDepthMm:20},fetched:null,revision:'1'};
  const html=render(Overview,{props:{data,locale:'sv'}}).body;
  assert.match(html,new RegExp(`data-weather-icon="${icon}"`));
 }
});
test('manual observations win per field, including zero, and provider values only fill gaps',()=>{
 const data={manual:{period:part.period,weatherCode:61,temperatureC:0,precipitationMm:0,windSpeedMs:null,cloudCoverPercent:0},fetched:{period:part.period,weatherCode:0,temperatureMeanC:12,precipitationSumMm:10,windSpeedMaxMs:5,cloudCoverPercent:90},revision:'9'};
 const before=JSON.stringify(data); const v=weatherOverview(data);
 assert.equal(v.icon,'rain');assert.equal(v.condition.source,'manual');
 assert.deepEqual(v.temperature,{value:0,source:'manual',field:'temperatureC'});
 assert.equal(v.fetchedTemperature,12);
 assert.deepEqual(v.metrics.map(m=>m.measurement),[{value:5,source:'fetched',field:'windSpeedMaxMs'},{value:0,source:'manual',field:'precipitationMm'},{value:0,source:'manual',field:'cloudCoverPercent'}]);
 assert.equal(JSON.stringify(data),before);
 for(const locale of ['sv','en']) {
  const html=render(Overview,{props:{data,locale}}).body;
  assert.match(html,/<strong[^>]*>0 °C<\/strong>/);assert.match(html,/meteo/);assert.match(html,/12 °C/);
  assert.match(html,locale==='sv'?/Högsta vind/:/Maximum wind/);
  assert.doesNotMatch(html,/data-weather-icon="sun"/);
 }
});
test('uncoded manual prose stays primary and neutral rather than guessing or contradicting it',()=>{
 const data={manual:{period:part.period,description:'Soligt enligt min anteckning'},fetched:{period:part.period,weatherCode:61,temperatureMeanC:12},revision:'1'};
 const view=weatherOverview(data);
 assert.equal(view.icon,'neutral');assert.equal(view.condition.description,data.manual.description);
 assert.equal(view.temperature.source,'fetched');
 assert.equal(weatherOverview({...data,manual:{...data.manual,weatherCode:0}}).icon,'sun');
});
test('fetched-only values are marked meteo and no temperature or measurements are invented',()=>{
 const data={manual:null,fetched:{period:part.period,weatherCode:71,windSpeedMaxMs:0,precipitationSumMm:0},revision:'1'};
 const view=weatherOverview(data);assert.equal(view.temperature,null);assert.equal(view.icon,'snow');
 assert.equal(view.metrics.length,2);assert.ok(view.metrics.every(m=>m.measurement.source==='fetched'));
 for(const locale of ['sv','en']){
  const html=render(Overview,{props:{data,locale}}).body;
  assert.equal((html.match(/>meteo</g)||[]).length,1);assert.match(html,/0 m\/s/);assert.match(html,/0 mm/);
  assert.doesNotMatch(html,/°C|>—</);
  assert.match(html,locale==='sv'?/Snöfall/:/Snow/);
 }
 assert.equal(weatherOverview({manual:{period:part.period,temperatureC:0},fetched:{period:part.period,temperatureMeanC:0},revision:'1'}).fetchedTemperature,null);
});
test('manual overview has no visible Manual label and keeps accessible measurement names',()=>{
 const data={manual:{period:part.period,weatherCode:0,temperatureC:8,windSpeedMs:0},fetched:null,revision:'1'};
 for(const locale of ['sv','en']){
  const html=render(Overview,{props:{data,locale}}).body;
  const visible=clean(html).replace(/<span[^>]*class="[^"]*sr-only[^"]*"[^>]*>[\s\S]*?<\/span>/g,'').replace(/<[^>]+>/g,'');
  assert.doesNotMatch(visible,/Manuellt|Manual|meteo/);
  assert.match(html,locale==='sv'?/Temperatur/:/Temperature/);
  assert.match(html,/aria-hidden="true"/);
 }
});
test('sunrise and sunset have distinct icons and recorded wall-clock times, with exact timestamps in details',()=>{
 const full={period:part.period,sunrise:'2026-10-25T00:00:00+14:00',sunset:'2026-10-25T17:42:00-05:00'};
 assert.equal(weatherClock(full.sunrise),'00:00');assert.equal(weatherClock(full.sunset),'17:42');
 const before=JSON.stringify(full);
 for(const locale of ['sv','en']){
  const html=render(Details,{props:{part:full,locale}}).body;
  const [primary,details]=html.split('<details');
  assert.match(primary,/data-weather-icon="sunrise"/);assert.match(primary,/data-weather-icon="sunset"/);
  assert.match(primary,/>00:00<\/time>/);assert.match(primary,/>17:42<\/time>/);
  assert.match(details,/2026-10-25T00:00:00\+14:00/);assert.match(details,/2026-10-25T17:42:00-05:00/);
  assert.match(primary,locale==='sv'?/Soluppgång/:/Sunrise/);
 }
 assert.equal(JSON.stringify(full),before);
});

test('the icon accessible description follows its explicit code, while manual prose remains in details',()=>{
 const manual={period:part.period,weatherCode:0,description:'Regn enligt anteckningen',temperatureC:8};
 const html=render(Overview,{props:{data:{manual,fetched:null,revision:'1'},locale:'sv'}}).body;
 assert.match(html,/data-weather-icon="sun"/);assert.match(html,/title="Klart"/);
 const panel=render(Details,{props:{part:manual,locale:'sv'}}).body;
 assert.match(panel,/Regn enligt anteckningen/);
});
test('a fetched zero temperature is still a marked complement to a different manual value',()=>{
 const data={manual:{period:part.period,temperatureC:8,weatherCode:0},fetched:{period:part.period,temperatureMeanC:0},revision:'1'};
 assert.equal(weatherOverview(data).fetchedTemperature,0);
 const html=render(Overview,{props:{data,locale:'sv'}}).body;
 assert.match(html,/<strong[^>]*>8 °C<\/strong>/);assert.match(html,/meteo/);assert.match(html,/0 °C/);assert.match(html,/Medeltemperatur/);
});

test('floating panel is centered and stays within each viewport edge',async()=>{
 const {weatherPanelPosition:place}=await import((await import('./weather-test-modules.mjs')).positionUrl);
 assert.deepEqual(place({right:980,top:100,bottom:210},620,450,{width:1050,height:850}),{left:215,top:80});
 assert.deepEqual(place({right:100,top:100,bottom:210},620,450,{width:1050,height:850}),{left:215,top:80});
 assert.deepEqual(place({right:1100,top:700,bottom:810},620,450,{width:1050,height:850}),{left:215,top:80});
 for(const [width,height] of [[1050,850],[560,850],[420,600],[700,300]]){
  const w=Math.min(620,width-32),h=Math.min(600,height-32);
  for(const right of [40,width/2,width+100]) for(const top of [-100,20,height-100]){
   const p=place({right,top,bottom:top+110},w,h,{width,height});
   assert.ok(p.left>=16&&p.left+w<=width-16);
   assert.ok(p.top>=16&&p.top+h<=height-16);
  }
 }
});
test('source details have an independent hero, retain missing/zero values and put sun times below measurements',()=>{
 for(const locale of ['sv','en']){
  const manual={period:part.period,weatherCode:61,temperatureC:0,windSpeedMs:0,precipitationMm:0};
  const fetched={period:part.period,weatherCode:0,temperatureMeanC:12,windSpeedMaxMs:3,sunrise:'2026-10-25T08:00:00+01:00',sunset:'2026-10-25T16:30:00+01:00'};
  const own=render(Details,{props:{part:manual,locale}}).body;
  const meteo=render(Details,{props:{part:fetched,locale}}).body;
  assert.match(own,/data-weather-icon="rain"/);assert.match(own,/0 °C/);assert.match(own,/0 m\/s/);assert.match(own,/0 mm/);
  assert.doesNotMatch(own,/<time|12 °C/);
  assert.match(meteo,/data-weather-icon="sun"/);assert.match(meteo,/12 °C/);
  assert.ok(meteo.indexOf('3 m/s')<meteo.indexOf('<time'));
  const noTemp=render(Details,{props:{part:{period:part.period,windSpeedMs:0},locale}}).body;
  assert.doesNotMatch(noTemp,/°C|<time/);assert.match(noTemp,/data-weather-icon="neutral"/);
 }
});

test('overview and details format measurements consistently without rounding small values to zero',()=>{
 for(const [locale, decimal, small, negative, tiny] of [
  ['sv','2,5','0,04','−0,04','1×10^−8'],
  ['en','2.5','0.04','-0.04','1E-8'],
 ]) {
  const manual={period:part.period,temperatureC:2.5,precipitationMm:0.04,windSpeedMs:0,cloudCoverPercent:0.04};
  const data={manual,fetched:null,revision:'1'};
  const html=render(Overview,{props:{data,locale}}).body;
  const panel=render(Details,{props:{part:manual,locale}}).body;
  assert.ok(html.includes(`${decimal} °C`)); assert.ok(panel.includes(`${decimal} °C`));
  assert.ok(html.includes(`${small} mm`)); assert.ok(panel.includes(`${small} mm`));
  assert.ok(panel.includes('0 m/s')); assert.ok(panel.includes(`${small} %`));
  const cold={...manual,temperatureC:-0.04,precipitationMm:1e-8};
  const coldTile=render(Overview,{props:{data:{manual:cold,fetched:null,revision:'1'},locale}}).body;
  const coldPanel=render(Details,{props:{part:cold,locale}}).body;
  assert.ok(coldTile.includes(`${negative} °C`)); assert.ok(coldPanel.includes(`${negative} °C`));
  assert.ok(coldTile.includes(`${tiny} mm`)); assert.ok(coldPanel.includes(`${tiny} mm`));
 }
});

test('manual weather draft preserves metadata and codes; empty and zero differ in both languages',async()=>{
 const {formHelpersUrl}=await import('./weather-test-modules.mjs');
 const {makeDraft,parseDraft,commonWeatherCodes}=await import(formHelpersUrl);
 const original={period:{kind:'hour',start:'2026-10-25T02:00:00+02:00',endExclusive:'2026-10-25T02:00:00+01:00'},location:{name:'test'},timezone:'Europe/Stockholm',weatherCode:57,temperatureC:2.5,precipitationMm:0,description:'text',snowDepthMm:2};
 for(const locale of ['sv','en']) {
  const draft=makeDraft(original,locale);assert.equal(draft.code,57);assert.ok(!commonWeatherCodes.includes(57));
  draft.values.temperatureC=locale==='sv'?'-0,04':'-0.04';draft.values.precipitationMm='';draft.values.windSpeedMs='0';
  let parsed=parseDraft(draft,original,'2026-01-01',locale);assert.deepEqual(parsed.errors,{});
  assert.equal(parsed.result.temperatureC,-0.04);assert.equal(parsed.result.windSpeedMs,0);assert.equal(parsed.result.precipitationMm,undefined);assert.equal(parsed.result.weatherCode,57);assert.deepEqual(parsed.result.period,original.period);assert.deepEqual(parsed.result.location,original.location);assert.equal(parsed.result.timezone,original.timezone);assert.equal(parsed.result.snowDepthMm,2);
  draft.code=null;parsed=parseDraft(draft,original,'2026-01-01',locale);assert.equal(parsed.result.weatherCode,undefined);
  draft.values.windSpeedMs='-1';draft.values.cloudCoverPercent='101';parsed=parseDraft(draft,original,'2026-01-01',locale);assert.ok(parsed.errors.windSpeedMs&&parsed.errors.cloudCoverPercent);
  const single=makeDraft(null,locale);single.code=commonWeatherCodes[0];assert.deepEqual(parseDraft(single,null,'2026-10-25',locale).errors,{});
  single.code=null;assert.ok(parseDraft(single,null,'2026-10-25',locale).errors.empty);
 }
});

test('real panel handlers keep a dirty or busy draft until an explicit discard',()=>{
 let script=readFileSync(new URL('../src/lib/components/WeatherTile.svelte',import.meta.url),'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1];
 script=script.replace(/^\s*import .*;\s*$/gm,'');
 const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'0'}},locale:'sv'});props.id=()=> 'test';
 const context=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,tick:()=>Promise.resolve(),weatherLabels:{},weatherPanelPosition:()=>({left:16,top:16}),window:{},Node:class {},Element:class {}});
 vm.runInContext(ts.transpileModule(script.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 const run=code=>vm.runInContext(code,context);
 run('form = true; open = true; draftChanged(true)');
 assert.equal(run('close(true)'),false);assert.equal(run('pending'),'panel');assert.equal(run('open'),true);assert.equal(run('dirty'),true);
 run('pending = null; setBusy(true)');assert.equal(run('close()'),false);assert.equal(run('open'),true);
 run('setBusy(false); requestDiscard("form")');assert.equal(run('pending'),'form');assert.equal(run('dirty'),true);
 run('resolvePending()');assert.equal(run('form'),false);assert.equal(run('dirty'),false);assert.equal(run('open'),true);
});


test('conflict reload preserves the draft while applying only explicitly changed fields',async()=>{
 const {formHelpersUrl}=await import('./weather-test-modules.mjs');
 const {makeDraft,parseDraft}=await import(formHelpersUrl);
 const old={period:part.period,temperatureC:5,windSpeedMs:1,weatherCode:61};
 const newer={period:part.period,temperatureC:8,windSpeedMs:9,weatherCode:71,timezone:'UTC'};
 const baseline=makeDraft(old,'sv');const draft=structuredClone(baseline);draft.values.temperatureC='6';
 const parsed=parseDraft(draft,newer,'2026-10-25','sv',baseline);
 assert.deepEqual(parsed.errors,{});assert.equal(parsed.result.temperatureC,6);assert.equal(parsed.result.windSpeedMs,9);assert.equal(parsed.result.weatherCode,71);assert.equal(parsed.result.timezone,'UTC');
});


test('the actual panel serializes saves and retains a draft on revision conflict',async()=>{
 let script=readFileSync(new URL('../src/lib/components/WeatherTile.svelte',import.meta.url),'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 let calls=0,release;let future=new Promise(resolve=>{release=resolve;});
 const props=()=>({status:{kind:'ready',data:{manual:null,fetched:null,revision:'1'}},locale:'sv',saveManual:async()=>{calls++;await future;}});props.id=()=> 'test';
 const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,tick:()=>Promise.resolve(),weatherLabels:{},weatherPanelPosition:()=>({left:16,top:16}),window:{},Node:class{},Element:class{}});
 vm.runInContext(ts.transpileModule(script.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 vm.runInContext('form=true; open=true; draftChanged(true)',c);
 const first=vm.runInContext('persist({})',c), second=vm.runInContext('persist({})',c);
 assert.equal(calls,1);assert.equal(vm.runInContext('busy',c),true);assert.equal(vm.runInContext('close()',c),false);
 release();await Promise.all([first,second]);assert.equal(vm.runInContext('busy',c),false);assert.equal(vm.runInContext('dirty',c),false);
 vm.runInContext('form=true; draftChanged(true); saveManual=async()=>{throw "weather_revision_conflict"}',c);await vm.runInContext('persist({})',c);
 assert.equal(vm.runInContext('failure',c),'conflict');assert.equal(vm.runInContext('dirty',c),true);assert.equal(vm.runInContext('form',c),true);
});

test('cloud cover stays in the optional section and keeps its saved value',async()=>{
 const {formUrl}=await import('./weather-test-modules.mjs');const {default:Form}=await import(formUrl);
 for(const [locale,more,cloud,temp] of [['sv','Fler väderuppgifter','Molntäcke','Temperatur'],['en','More weather details','Cloud cover','Temperature']]) {
  const manual={period:part.period,temperatureC:2,cloudCoverPercent:0};
  const html=render(Form,{props:{manual,date:'2026-10-25',locale,busy:false,onSave(){},onCancel(){},onDirty(){}}}).body;
  const split=html.indexOf(more);assert.ok(split>0);assert.ok(html.slice(0,split).includes(temp));assert.ok(!html.slice(0,split).includes(cloud));assert.ok(html.slice(split).includes(cloud));assert.match(html.slice(split),/value="0"/);
 }
});


test('condition chooser is concise and preserves a previously saved specific code', async()=>{
 const {formUrl,formHelpersUrl}=await import('./weather-test-modules.mjs');
 const {default:Form}=await import(formUrl);
 const {makeDraft,parseDraft,commonWeatherCodes}=await import(formHelpersUrl);
 assert.deepEqual([...commonWeatherCodes],[0,2,3,45,51,61,80,71,95]);
 for(const locale of ['sv','en']) {
  const original={period:part.period,weatherCode:57,temperatureC:2};
  const html=render(Form,{props:{manual:original,date:'2026-10-25',locale,busy:false,onSave:()=>{},onCancel:()=>{},onDirty:()=>{}}}).body;
  assert.match(html,locale==='sv'?/Tidigare valt/:/Previously selected/);
  assert.doesNotMatch(html,/Fler väderförhållanden|More weather conditions|\(57\)/);
  assert.equal((html.match(/type="radio"/g)||[]).length,11);
  const draft=makeDraft(original,locale);draft.values.temperatureC=locale==='sv'?'3,5':'3.5';
  const result=parseDraft(draft,original,'2026-10-25',locale);assert.deepEqual(result.errors,{});assert.equal(result.result.weatherCode,57);assert.equal(result.result.temperatureC,3.5);
  draft.code=61;assert.equal(parseDraft(draft,original,'2026-10-25',locale).result.weatherCode,61);
 }
});


test('manual description is in the main form and readable outside metadata, never in the tile', async()=>{
 const {formUrl}=await import('./weather-test-modules.mjs'); const {default:Form}=await import(formUrl);
 for(const locale of ['sv','en']) {
  const description='My own observation <not markup> '+ 'long unbroken text '.repeat(150);
  const manual={period:part.period,description};
  const form=render(Form,{props:{manual,date:'2026-10-25',locale,busy:false,onSave:()=>{},onCancel:()=>{},onDirty:()=>{}}}).body;
  assert.ok(form.indexOf('<textarea')<form.indexOf('<details'));
  assert.ok(form.indexOf('inputmode="decimal"')<form.indexOf('<textarea'));
  const details=render(Details,{props:{part:manual,locale,manual:true}}).body;
  assert.ok(details.indexOf('My own observation')<details.indexOf('<details'));
  assert.match(details,/tabindex="0" role="region"/); assert.match(details,/&lt;not markup/);
  for(const temperatureC of [undefined,2.5]) {
   const tile=render(Overview,{props:{data:{manual:{...manual,temperatureC},fetched:null,revision:'1'},locale}}).body;
   assert.doesNotMatch(tile,/My own observation|long unbroken text/);
  }
 }
});


test('confirmed weather write with failed or changed session is unverified, without publishing stale data', async()=>{
 let script=readFileSync(new URL('../src/lib/components/EntryWeather.svelte',import.meta.url),'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 for(const scenario of ['failure','changed','verified','write-failure']) {
  const calls=[];
  const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:()=>({entryId:'synthetic',generation:1,locale:'sv'}),$state:x=>x,$effect:()=>{},emptyWeather:{kind:'ready',data:{}},createWeatherReader:()=>()=>{},invoke:async(cmd)=>{
   calls.push(cmd);
   if(cmd==='update_manual_weather') {if(scenario==='write-failure') throw 'write-failed';return {manual:{temperatureC:5},fetched:null,revision:'2'};}
   if(scenario==='failure') throw 'session-unavailable';
   return {generation:scenario==='changed'?2:1};
  }});
  vm.runInContext(ts.transpileModule(script.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
  vm.runInContext('status={kind:"ready",data:{revision:"1"}}',c);
  if(scenario==='write-failure') await assert.rejects(vm.runInContext('saveManual(null)',c),e=>e==='write-failed');
  else assert.equal(await vm.runInContext('saveManual(null)',c),scenario==='verified'?'verified':'unverified');
  assert.equal(vm.runInContext('status.data.revision',c),scenario==='verified'?'2':'1');
  assert.equal(vm.runInContext('status.kind',c),['failure','changed'].includes(scenario)?'unverified':'ready');
  assert.deepEqual(calls,scenario==='write-failure'?['update_manual_weather']:['update_manual_weather','new_journal_session']);
 }
});

test('unverified successful write retains the draft and blocks another write until reload',async()=>{
 let script=readFileSync(new URL('../src/lib/components/WeatherTile.svelte',import.meta.url),'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 // Evaluate the derived state on each access, as Svelte does in the component.
 script=script.replace('const unverified = $derived(status.kind === "unverified");','');
 let writes=0,readFail=true;const state={kind:'ready',data:{manual:null,fetched:null,revision:'1'}};
 const props=()=>({status:state,locale:'sv',saveManual:async()=>{writes++;state.kind='unverified';return 'unverified'},reload:async()=>{if(readFail)throw 'stale';state.kind='ready'}});props.id=()=> 'test';
 const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:props,$state:x=>x,$derived:x=>x,tick:()=>Promise.resolve(),weatherLabels:{},weatherPanelPosition:()=>({left:16,top:16}),window:{},Node:class{},Element:class{}});
 Object.defineProperty(c,'unverified',{get:()=>state.kind==='unverified'});
 vm.runInContext(ts.transpileModule(script.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
 const run=x=>vm.runInContext(x,c);
 run('form=true; open=true; draftChanged(true)');await run('persist({})');
 assert.equal(run('unverified'),true);assert.equal(run('failure'),'unverified');assert.equal(run('dirty'),true);assert.equal(run('form'),true);
 await run('persist({})');assert.equal(writes,1);
 await run('refresh()');assert.equal(run('unverified'),true);assert.equal(run('failure'),'unverified');
 readFail=false;await run('refresh()');assert.equal(run('unverified'),false);assert.equal(run('dirty'),true);assert.equal(run('form'),true);assert.equal(run('reloaded'),true);
 state.kind='unverified';run('failure="unverified"');assert.equal(run('requestDiscard("panel")'),false);run('resolvePending()');assert.equal(run('failure'),'unverified');assert.equal(run('unverified'),true);
});

test('unverified write survives weather tile remount and recovers only after a verified reload',async()=>{
 const entryScript=readFileSync(new URL('../src/lib/components/EntryWeather.svelte',import.meta.url),'utf8').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\s*$/gm,'');
 for(const manual of [null,{period:part.period,temperatureC:5}]) {
  const old={kind:'ready',data:{manual:{period:part.period,temperatureC:17},fetched:null,revision:'1'}};
  const saved={manual,fetched:null,revision:'2'};let allowRead=false,writeCount=0;
  const c=vm.createContext({$effect:()=>{},untrack:f=>f(),$props:()=>({entryId:'synthetic',generation:1,locale:'sv'}),$state:x=>x,$effect:()=>{},emptyWeather:old,
   createWeatherReader:()=>async()=>{if(!allowRead)throw 'session-unavailable';return {synthetic:saved}},
   invoke:async cmd=>{if(cmd==='update_manual_weather'){writeCount++;return saved}throw 'session-unavailable'}});
  vm.runInContext(ts.transpileModule(entryScript.replace(/export /g,""),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,c);
  c.initial=old;c.savedManual=manual;vm.runInContext('status=initial',c);
  assert.equal(await vm.runInContext('saveManual(savedManual)',c),'unverified');
  let status=vm.runInContext('status',c);
  assert.equal(status.kind,'unverified');
  const remounted=show(status,'sv',false);
  assert.doesNotMatch(remounted,/17 °C/);assert.match(remounted,/kontrolleras/);
  await assert.rejects(vm.runInContext('saveManual(null)',c),e=>e==='weather_entry_missing');assert.equal(writeCount,1);
  await assert.rejects(vm.runInContext('reload()',c));assert.equal(vm.runInContext('status.kind',c),'unverified');
  allowRead=true;await vm.runInContext('reload()',c);status=vm.runInContext('status',c);
  assert.equal(status.kind,'ready');assert.equal(status.data.revision,'2');assert.deepEqual(status.data.manual,manual);
  const recovered=show(status,'sv',false);assert.doesNotMatch(recovered,/17 °C|behöver kontrolleras/);
 }
});

test('manual save is labelled Save observation in both languages',async()=>{
 const {formUrl}=await import('./weather-test-modules.mjs');const {default:Form}=await import(formUrl);
 for(const locale of ['sv','en']){
  const html=render(Form,{props:{manual:null,date:'2020-06-01',locale}}).body;
  assert.match(html,locale==='sv'?/Spara observation/:/Save observation/);
 }
});
test('ERA5 subtitle is shared by preview and stored weather',()=>{
 const era5={period:{kind:'day',date:'2020-06-01'},temperatureMeanC:12,source:{provider:'open-meteo',product:'historical-weather',model:'ERA5',license:'CC BY 4.0'}};
 for(const locale of ['sv','en']){
  const html=render(Details,{props:{part:era5,locale}}).body;
  assert.match(html,locale==='sv'?/Historiskt väder · Modellberäknat väder/:/Historical weather · Modelled weather/);
  assert.match(html,/ERA5/);assert.match(html,/CC BY 4.0/);
  assert.doesNotMatch(render(Details,{props:{part:era5,locale,manual:true}}).body,/snapshot-kind/);
 }
 const history=readFileSync(new URL('../src/lib/components/HistoricalWeather.svelte',import.meta.url),'utf8');
 assert.ok(history.includes('<WeatherDetails part={existing}'));assert.ok(history.includes('<WeatherDetails part={preview.fetched}'));
});

test('saved fetched summaries link Open-Meteo and CC BY in both languages outside the weather button', () => {
  for (const locale of ['sv', 'en']) {
    const historical = {period:{kind:'day',date:'2026-10-25'},temperatureMeanC:4,source:{...currentPart.source,product:'historical-weather',model:'ERA5'}};
    for (const fetched of [historical, currentPart]) for (const editing of [false, true]) {
      const html = show({kind:'ready',data:{manual:null,fetched,revision:'1'}},locale,editing);
      assert.match(html, /href="https:\/\/open-meteo\.com\/"/);
      assert.match(html, /href="https:\/\/creativecommons\.org\/licenses\/by\/4\.0\/"/);
      assert.match(html, locale==='sv' ? /Väderdata/ : /Weather data/);
      assert.doesNotMatch(html.match(/<button[\s\S]*?<\/button>/)?.[0] ?? '', /<a /);
    }
    const manual = show({kind:'ready',data:{manual:part,fetched:null,revision:'1'}},locale);
    assert.doesNotMatch(manual, /href="https:\/\/open-meteo/);
    const detail = render(Details,{props:{part:currentPart,locale}}).body;
    assert.match(detail, /href="https:\/\/open-meteo\.com\/"/);
    assert.match(detail, /href="https:\/\/creativecommons\.org\/licenses\/by\/4\.0\/"/);
  }
});

test('entry list places fetched-weather attribution outside the entry navigation link', () => {
  const source=readFileSync(new URL('../src/routes/journal/entries/+page.svelte',import.meta.url),'utf8');
  assert.match(source, /<\/a>\s*<WeatherAttribution part=\{weather\[entry.id\]\?\.fetched\}/);
});
