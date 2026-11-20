import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import {compile} from 'svelte/compiler';
import {render} from 'svelte/server';
import {source,url,transpile,birdsUrl,visitPreviewUrl} from './bird-test-modules.mjs';
const messages=new URL('../src/lib/paraglide/messages.js',import.meta.url).href;
const resolve=(text,aliases)=>text.replace(/from (["'])([^"']+)\1/g,(_,q,n)=>`from ${q}${aliases[n]??import.meta.resolve(n)}${q}`);
const calendarSource=source('../src/lib/observation-calendar.ts');
const calendarUrl=url(resolve(transpile(calendarSource),{'$lib/birds':birdsUrl,'$lib/journal-data':url('export const createJournalReader=()=>async()=>{throw Error("SSR read")};')}));
const calendar=await import(calendarUrl);
const visit=(id,date,start=null)=>({id,visitDate:date,visitStart:start,visitEnd:null,place:{name:'Gläntan',id:'place',latitude:59,longitude:18},rows:[{english:'Great Tit',swedish:'talgoxe',scientific:'Parus major',count:'4'}]});
async function html(text,props={},locale='sv'){
 const compiled=compile(text,{generate:'server'});assert.deepEqual(compiled.warnings,[]);
 const mod=await import(url(resolve(compiled.js.code,{'$lib/language.svelte':url(`export const language={current:${JSON.stringify(locale)}}`),'$lib/paraglide/messages.js':messages,'$lib/birds':birdsUrl,'$lib/observation-calendar':calendarUrl,'$lib/components/BirdVisitPreview.svelte':visitPreviewUrl})));
 return render(mod.default,{props}).body;
}
test('visit identity/date drives one marker per day independently of journal links, edits and deletion',()=>{
 const lists=[visit('evening','2026-11-19','18:00'),visit('morning','2026-11-19','07:00'),visit('standalone','2026-11-20'),visit('morning','2026-11-19','07:00')];
 assert.deepEqual(calendar.observationDates(lists,2026,11),['2026-11-19','2026-11-20']);
 assert.deepEqual(calendar.calendarVisits(lists,'2026-11-19').map(v=>v.id),['morning','evening']);
 assert.deepEqual(calendar.calendarVisits(lists,'2026-11-21'),[]);
 lists[0]={...lists[0],visitDate:'2026-12-01'};
 assert.deepEqual(calendar.observationDates(lists.filter(v=>v.id!=='morning'),2026,11),['2026-11-20']);
 assert.deepEqual(calendar.observationDates([],2026,11),[]);
});
test('rendered calendar labels journal-only, bird-only, mixed and empty days in both languages',async()=>{
 for(const locale of ['sv','en']){
  const markup=await html(source('../src/lib/components/CalendarGrid.svelte'),{displayedMonth:new Date(2026,10,1),markedDates:['2026-11-18','2026-11-20'],observationDates:['2026-11-19','2026-11-20'],observationsAvailable:true},locale);
  const button=day=>markup.match(new RegExp(`<button[^>]*aria-label="${day} (?:november|November) 2026[^>]*>[\\s\\S]*?<\\/button>`))?.[0];
  assert.match(button(18),/day-content-dot/);assert.doesNotMatch(button(18),/observation-dot/);
  assert.match(button(19),/observation-dot/);assert.doesNotMatch(button(19),/day-content-dot/);
  assert.match(button(20),/day-content-dot/);assert.match(button(20),/observation-dot/);
  assert.doesNotMatch(button(21),/day-content-dot|observation-dot/);
  assert.match(button(20),locale==='sv'?/Journalanteckningar · Fågelobservationer/:/Journal entries · Bird observations/);
  assert.match(button(21),locale==='sv'?/Inget innehåll/:/No content/);
  assert.match(markup,/calendar-legend/);assert.match(button(20),/type="button"/);
 }
});
test('rendered day observations link directly to standalone detail and show an explicit empty state',async()=>{
 const original=source('../src/lib/components/CalendarObservations.svelte');
 const seeded=rows=>original.replace('visits=$state<BirdList[]>([])',`visits=$state<BirdList[]>(${JSON.stringify(rows)})`).replace('available=$state(false)','available=$state(true)').replace('loading=$state(true)','loading=$state(false)');
 const result=await html(seeded([visit('one','2026-11-19'),visit('two','2026-11-19')]),{date:'2026-11-19'});
 assert.match(result,/aria-label="Observationer"/);assert.match(result,/href="\/observations\/one"/);assert.match(result,/href="\/observations\/two"/);assert.match(result,/Gläntan/);assert.doesNotMatch(result,/journal\/entries/);
 assert.match(await html(seeded([]),{date:'2026-11-19'}),/Inga observationer den här dagen/);
 assert.doesNotMatch(await html(original,{date:'2026-11-19'}),/href=|<section/);
});
test('calendar reader checks platform availability before invoking guarded list reads',async()=>{
 let enabled=false,calls=[];
 const sandbox=vm.createContext({invoke:async(command)=>{calls.push(command);return command==='bird_observations_available'?enabled:{generation:8}},createJournalReader:()=>async(command,args)=>{calls.push([command,{...args}]);return {generation:8,data:[]}},groupBirdVisits:()=>[]});
 vm.runInContext(transpile(calendarSource.replace(/^import .*;\n/gm,'').replace(/^export /gm,'')),sandbox);
 assert.equal(await vm.runInContext('createObservationCalendarReader()()',sandbox),null);assert.deepEqual(calls,['bird_observations_available']);
 enabled=true;calls=[];assert.equal((await vm.runInContext('createObservationCalendarReader()()',sandbox)).generation,8);
 assert.deepEqual(calls,['bird_observations_available','new_journal_session',['read_bird_lists',{generation:8}]]);
});
test('day content refresh rejects old dates/late replies and reflects edits/deletion on journal-change events',async()=>{
 const effects=[],listeners=new Map(),pending=[];
 const context=vm.createContext({$props:()=>({date:'2026-11-19'}),$state:v=>v,$effect:f=>effects.push(f),createObservationCalendarReader:()=>()=>new Promise((resolve,reject)=>pending.push({resolve,reject})),calendarVisits:calendar.calendarVisits,window:{addEventListener:(n,f)=>listeners.set(n,f),removeEventListener:n=>listeners.delete(n)}});
 const script=source('../src/lib/components/CalendarObservations.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^ import .*;\n/gm,'');
 vm.runInContext(transpile(script),context);const run=s=>vm.runInContext(s,context);const cleanups=effects.map(f=>f());
 const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve()};
 run('date="2026-11-20";void load(date)');pending[1].resolve({generation:2,data:[visit('new','2026-11-20')]});await flush();
 pending[0].resolve({generation:2,data:[visit('old','2026-11-19')]});await flush();assert.equal(run('visits[0].id'),'new');
 listeners.get('mno:journal-changed')();pending[2].resolve({generation:2,data:[]});await flush();assert.equal(run('visits.length'),0);
 listeners.get('mno:journal-changed')();cleanups.forEach(f=>f?.());pending[3].resolve({generation:2,data:[visit('late','2026-11-20')]});await flush();assert.equal(run('visits.length'),0);assert.equal(listeners.size,0);
});
test('Home keeps journal and Continue primary, with gated integrated observation entry',async()=>{
 const text=source('../src/routes/+page.svelte');
 const enabled=await html(text.replace('observationsAvailable=$state(false)','observationsAvailable=$state(true)'));
 assert.match(enabled,/Skriv journalanteckning/);assert.match(enabled,/href="\/observations\/new"/);assert.match(enabled,/Registrera en observation/);assert.doesNotMatch(enabled,/href="\/observations"/);assert.match(enabled,/href="\/journal\?latest=true"/);
 assert.ok(enabled.indexOf('/journal/new')<enabled.indexOf('/observations/new'));
 assert.doesNotMatch(await html(text),/href="\/observations/);
 const journal=source('../src/lib/components/BirdJournalLists.svelte');
 for(const key of ['observation_add','observation_create_new','observation_link_existing'])assert.ok(journal.includes(`m.${key}`));
 assert.match(journal,/\$\{entryHref\(entryId\)\}\/birds\/new/);assert.match(journal,/allowCreate&&generation!==null/);assert.equal(compile(journal,{generate:'client'}).warnings.length,0);
});

test('actual navigation refreshes markers on changes, ignores superseded months and hides failures',async()=>{
 const effects=[],listeners=new Map(),pending=[];
 const context=vm.createContext({$state:v=>v,$derived:v=>v,$effect:f=>effects.push(f),createObservationCalendarReader:()=>()=>new Promise((resolve,reject)=>pending.push({resolve,reject})),observationDates:calendar.observationDates,createJournalReader:()=>async()=>({data:[]}),getContext:()=>({close(){}}),PLACE_MAP:'map',onMount(){},afterNavigate(){},language:{current:'sv'},window:{addEventListener:(n,f)=>listeners.set(n,f),removeEventListener:n=>listeners.delete(n)},clearTimeout(){},console});
 const script=source('../src/lib/components/AppNavigation.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,'').replace('calendarMonth.getFullYear()', '2026').replace('calendarMonth.getMonth() + 1', '11');
 vm.runInContext(transpile(script),context);const run=s=>vm.runInContext(s,context);const flush=async()=>{for(let i=0;i<10;i++)await Promise.resolve()};
 // Run the real event-listener effect without unrelated map/history effects.
 const listenerEffect=effects.find(f=>f.toString().includes('addEventListener'));const cleanup=listenerEffect();
 run('handleJournalChanged()');
 pending[0].resolve({generation:1,data:[visit('linked','2026-11-19'),visit('linked','2026-11-19'),visit('standalone','2026-11-19')]});await flush();
 assert.equal(run('markedObservationDates.join()'),'2026-11-19');
 listeners.get('mno:journal-changed')();pending[1].resolve({generation:1,data:[visit('linked','2026-11-20')]});await flush();assert.equal(run('markedObservationDates.join()'),'2026-11-20');
 listeners.get('mno:journal-changed')();pending[2].resolve({generation:1,data:[]});await flush();assert.equal(run('markedObservationDates.length'),0);
 run('void loadObservationDates(2026,10);void loadObservationDates(2026,11)');pending[4].resolve({generation:1,data:[visit('current','2026-11-21')]});await flush();pending[3].resolve({generation:1,data:[visit('old','2026-10-01')]});await flush();assert.equal(run('markedObservationDates.join()'),'2026-11-21');
 run('void loadObservationDates(2026,11)');pending[5].reject('journal_generation_conflict');await flush();assert.equal(run('markedObservationDates.length'),0);assert.equal(run('observationCalendarError'),true);
 run('void loadObservationDates(2026,11)');effects.at(-1)()();pending[6].resolve({generation:1,data:[visit('late','2026-11-19')]});await flush();assert.equal(run('markedObservationDates.length'),0);cleanup();
});

test('native date buttons preserve selectDay destination and journal actions use a keyboard disclosure',async()=>{
 const selected=[];
 const context=vm.createContext({$props:()=>({displayedMonth:new Date(2026,10,1),onSelectDate:date=>selected.push(date)}),$bindable:v=>v,$state:v=>v,$derived:Object.assign(v=>v,{by:()=>[]}),language:{current:'sv'},Intl,Date});
 const script=source('../src/lib/components/CalendarGrid.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,'');
 vm.runInContext(transpile(script),context);vm.runInContext('selectDay(19)',context);assert.deepEqual(selected,['2026-11-19']);
 const text=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)');
 const compiled=compile(text,{generate:'server'});assert.deepEqual(compiled.warnings,[]);
 const blank=url('export default function(){}');
 const component=await import(url(resolve(compiled.js.code,{'$lib/birds':birdsUrl,'$lib/bird-names':url('export const formatBirdDate=()=>"date"'),'$lib/photo-gallery':url('export const observationPhotos=()=>[]'),'$lib/components/PhotoThumbnail.svelte':blank,'$lib/components/BirdVisitPreview.svelte':visitPreviewUrl,'$lib/components/BirdLinkPicker.svelte':blank,'$lib/components/DeleteEntryDialog.svelte':blank,'$lib/paraglide/messages.js':messages})));
 const result=render(component.default,{props:{entryId:'entry/a',generation:7,locale:'sv',allowCreate:true}}).body;
 assert.match(result,/<details[^>]*class="observation-actions/);assert.match(result,/<summary[^>]*>Lägg till observation<\/summary>/);assert.match(result,/href="\/journal\/entries\/entry%2Fa\/birds\/new"/);assert.match(result,/>Skapa ny<\/a>/);assert.match(result,/>Koppla befintlig<\/button>/);
 assert.doesNotMatch(render(component.default,{props:{entryId:'entry',generation:7,locale:'sv',allowCreate:false}}).body,/<details/);
});


test('Home preserves two main cards and one integrated compartment in both languages',async()=>{
 const text=source('../src/routes/+page.svelte').replace('observationsAvailable=$state(false)','observationsAvailable=$state(true)');
 for(const locale of ['sv','en']){
  const result=await html(text,{},locale);
  assert.equal((result.match(/class="journal-card(?:\s[^"]*)?"/g)||[]).length,2);
  assert.equal((result.match(/class="card-arrow(?:\s[^"]*)?"/g)||[]).length,3);
  assert.equal((result.match(/class="observation-entry(?:\s[^"]*)?"/g)||[]).length,1);
  assert.ok(result.includes(locale==='sv'?'Registrera en observation':'Record an observation'));
  assert.doesNotMatch(result,/observations-entry|observation-cards/);
 }
 assert.match(text,/@media \(max-width: 600px\)[\s\S]*\.journal-cards[^{]*\{[^}]*grid-template-columns: 1fr/);
 const calendar=source('../src/lib/components/CalendarGrid.svelte');assert.match(calendar,/background:#376b9b/);assert.match(calendar,/gap:5px/);assert.match(calendar,/width:6px;height:6px/);
});

 test('sidebar starts closed and offers one diary route plus the gated observations home',()=>{
  const navigation=source('../src/lib/components/AppNavigation.svelte');
  assert.match(navigation,/activePanel = \$state<Panel \| null>\(null\)/);
  assert.match(navigation,/aria-expanded=\{activePanel === "left"\}/);
  assert.match(navigation,/href="\/journal\?latest=true"[^>]*onclick/);
  assert.match(navigation,/\{#if birdsAvailable\}[\s\S]*href="\/observations"/);
  assert.equal((navigation.match(/<CalendarDayPanel/g)||[]).length,1);
 });


test('Continue describes the combined diary and observation type text uses a quieter shade than markers',async()=>{
 const text=source('../src/routes/+page.svelte');
 assert.match(await html(text,{},'sv'),/Återvänd till dina dagar i naturen\./);
 assert.match(await html(text,{},'en'),/Return to your days in nature\./);
 assert.match(source('../src/lib/components/JournalReading.svelte'),/color:#345f80/);
 assert.match(source('../src/lib/components/CalendarGrid.svelte'),/grid-template-rows: 18px 6px/);
 assert.doesNotMatch(source('../src/lib/components/CalendarGrid.svelte'),/day-markers\{position:absolute/);
});


test('sidebar destinations remain quiet after the calendar while preserving semantic location and keyboard focus',()=>{
 const navigation=source('../src/lib/components/AppNavigation.svelte');
 assert.ok(navigation.indexOf('class="navigation-search"')<navigation.indexOf('<CalendarGrid'));
 assert.ok(navigation.indexOf('<CalendarGrid')<navigation.indexOf('class="panel-section destination-navigation"'));
 assert.doesNotMatch(navigation,/\.destination-link\[aria-current\]/);
 assert.match(navigation,/aria-current=/);
 assert.match(navigation,/\.destination-link:hover\{background:/);
 assert.match(navigation,/\.destination-link:focus-visible\{outline:2px/);
 assert.doesNotMatch(navigation,/autofocus/);
 const toggle=navigation.split('function togglePanel')[1].split('function closeCalendarDayPanel')[0];assert.doesNotMatch(toggle,/\.focus\(/);
 const reading=source('../src/lib/components/JournalReading.svelte');assert.match(reading,/observation-type\{font:500 13px\/1.5 system-ui,sans-serif;color:#345f80;letter-spacing:\.25px/);
 const luminance=hex=>{const rgb=hex.match(/../g).map(n=>parseInt(n,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;};
 assert.ok((luminance('f8f7f3')+.05)/(luminance('345f80')+.05)>=4.5);
});
