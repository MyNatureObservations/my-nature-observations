import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';
import {render} from 'svelte/server';
import {birds,source,previewUrl,creationUrl,names,detailUrl,overviewUrl,visitPreviewUrl,journalLinksUrl} from './bird-test-modules.mjs';
const place={id:'place',name:'Lake',latitude:59,longitude:18,revision:'1'};
const species={id:'avibase-DF1E2A1D',english:'Ruddy Duck',scientific:'Oxyura jamaicensis',swedish:null};
test('missing Swedish name keeps English/scientific fallback and registration identity',()=>{
 assert.equal(birds.speciesLabel(species,'sv'),'Ruddy Duck · Oxyura jamaicensis');assert.equal(birds.speciesLabel(species,'en'),'Ruddy Duck');
 assert.ok(birds.canSaveBirdList('2026-11-12',place,[{...species,count:'6'}],'08:15','09:30'));
 for(const count of ['','0','-1','1.5','01','9223372036854775808'])assert.equal(birds.canSaveBirdList('2026-11-12',place,[{...species,count}],'',''),false);
 assert.equal(birds.canSaveBirdList('2026-02-30',place,[{...species,count:'1'}],'',''),false);
 assert.equal(birds.canSaveBirdList('2026-11-12',{...place,latitude:null},[{...species,count:'1'}],'',''),false);
 assert.equal(birds.canSaveBirdList('2026-11-12',place,[{...species,count:'1'}],'24:30',''),false);
 assert.equal(birds.canSaveBirdList('2026-11-12',place,[],'',''),false);
});
test('one preview per list is below journal text, shows first three rows and preserves origin',async()=>{
 const lists=[{id:'same-list',place,visitDate:'2026-11-12',visitStart:'08:15',visitEnd:'09:30',rows:[1,2,3,4].map(n=>({...species,english:`Species ${n}`,count:String(n)}))}];
 const text=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)').replace('$state<BirdList[]>([])',`$state<BirdList[]>(${JSON.stringify(lists)})`);
 const {default:component}=await import(previewUrl(text));
 for(const locale of ['sv','en']){const html=render(component,{props:{entryId:'origin',generation:1,locale}}).body;assert.match(html,/Lake/);assert.match(html,/08:15/);assert.match(html,/Species 3/);assert.doesNotMatch(html,/Species 4/);assert.ok(html.includes('/journal/entries/origin/birds/same-list'));if(locale==='sv')assert.match(html,/Oxyura jamaicensis/);}
 const reading=source('../src/lib/components/JournalReading.svelte');assert.ok(reading.indexOf('<BirdJournalLists')>reading.indexOf('<JournalContent'));
 assert.equal(birds.birdHref('another-origin','same-list'),'/journal/entries/another-origin/birds/same-list');
});
function formHarness({entryId="origin",editId=null,editList=null,reject=false,rejectNavigation=false,rejectConfirmation=false,saveImplementation=null,journalPlace=place,entry=async()=>({generation:7,data:{id:"origin",entryDate:"2026-11-12"}}),failAt=null,enabled=true}={}){
 const calls=[],navigation=[],effects=[],mounts=[],initialization=[],focus=[],diagnostics=[],confirmations=[];let before;
 const check=stage=>{initialization.push(stage);if(failAt===stage)throw Error("injected "+stage);};
 const context=vm.createContext({$props:()=>({editId}),$state:v=>v,$derived:v=>v,$effect:fn=>effects.push(fn),page:{params:{id:entryId??undefined}},getContext:()=>({open:()=>()=>{}}),PLACE_MAP:'map',onMount:fn=>mounts.push(fn),invoke:async command=>{check(command);if(command==="bird_observations_available")return enabled;if(command==="new_journal_session")return {generation:7};if(command==="list_places")return {generation:7,data:[place]};throw Error("unexpected command "+command);},createJournalReader:()=>async(command)=>{check(command);if(command==="read_journal_bird_list")return {generation:7,data:editList};return entry();},readJournalPlace:async()=>{check("read_journal_place");return {place:journalPlace};},getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}}),tick:()=>Promise.resolve(),beforeNavigate:fn=>before=fn,goto:async href=>{navigation.push(href);if(rejectNavigation)throw Error("navigation failed");},localVisitDate:birds.localVisitDate,entryHref:birds.entryHref,birdHref:birds.birdHref,speciesLabel:birds.speciesLabel,capitalizeCommonName:names.capitalizeCommonName,validDate:birds.validDate,validTime:birds.validTime,validBirdCount:birds.validBirdCount,validBirdComment:birds.validBirdComment,DEFAULT_BIRD_NAMES:names.DEFAULT_BIRD_NAMES,loadBirdNamePreferences:()=>({...names.DEFAULT_BIRD_NAMES}),canSaveBirdList:birds.canSaveBirdList,
 reserveBirdRequest:async()=>"33333333-3333-4333-8333-333333333333",crypto:{randomUUID:()=>"33333333-3333-4333-8333-333333333333"},confirmSavedBirdList:async(...args)=>{confirmations.push(args);if(rejectConfirmation)throw Error("confirmation failed");},updateBirdList:async(...args)=>{calls.push(args);if(reject)throw Error("bird_edit_stale");return "b".repeat(64);},saveBirdList:async(...args)=>{calls.push(args);if(saveImplementation)return saveImplementation(...args);if(reject)throw typeof reject==='string'?reject:Error('lost response');return 'saved-list';},window:{dispatchEvent:()=>{},addEventListener:()=>{},removeEventListener:()=>{}},Event,JSON,Error,console:{error:(...args)=>diagnostics.push(args)},
 searchControl:{focus:()=>focus.push('search')},
 document:{getElementById:id=>{const i=Number(id.replace(/^bird-(?:count|comment)-/,''));return i>=0&&i<vm.runInContext('rows.length',context)?{focus:()=>focus.push(id),select:()=>{}}:null;},querySelector:()=>({focus:()=>focus.push('result-0')}),querySelectorAll:()=>[0,1,2].map(i=>({focus:()=>focus.push(`result-${i}`)}))}});
 const script=source('../src/lib/components/BirdListForm.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'').replace('const valid=$derived','let valid=$derived');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 const run=s=>vm.runInContext(s,context);return{run,calls,navigation,initialization,focus,diagnostics,confirmations,mount:()=>mounts[0](),before:()=>before};
}
test('cancellation creates neither list nor link and dirty navigation waits for confirmation',async()=>{
 const h=formHarness();h.run('dirty=true;cancel()');await Promise.resolve();assert.equal(h.run('leave'),true);assert.equal(h.calls.length,0);assert.equal(h.navigation.length,0);await h.run('discard()');assert.deepEqual(h.navigation,['/journal/entries/origin']);assert.equal(h.calls.length,0);
 const x=formHarness();x.run('dirty=true');let cancelled=false;x.before()({cancel:()=>cancelled=true,to:{url:new URL('https://example.invalid/')}});assert.equal(cancelled,true);assert.equal(x.calls.length,0);
});
function ready(h){h.run(`available=true;generation=7;date='2026-11-13';place=${JSON.stringify(place)};rows=${JSON.stringify([{...species,count:'6'}])};start='08:15';end='09:30';valid=true;dirty=true;`);}
test('save calls the guarded operation once with changed defaults and returns through the origin',async()=>{
 const h=formHarness();ready(h);await h.run('save()');assert.equal(h.calls.length,1);assert.equal(h.calls[0][0],7);assert.equal(h.calls[0][1],'origin');assert.equal(h.calls[0][2],'2026-11-13');assert.equal(h.calls[0][5],'08:15');assert.deepEqual(h.navigation,['/journal/entries/origin/birds/saved-list']);await h.run('save()');assert.equal(h.calls.length,1);
});
test('failed save retains draft and does not navigate or claim success',async()=>{
 const h=formHarness({reject:true});ready(h);await h.run('save()');assert.equal(h.calls.length,1);assert.equal(h.navigation.length,0);assert.equal(h.run('saved'),false);assert.equal(h.run('dirty'),true);assert.equal(h.run('rows[0].count'),'6');
});

async function creationHtml(h,locale='sv') {
 let text=source('../src/lib/components/BirdListForm.svelte');
 for(const [name,initial] of Object.entries({initialized:'$state(false)',available:'$state<boolean|null>(null)',generation:'$state<number|null>(null)',date:'$state("")',error:'$state("")',rows:'$state<BirdRow[]>([])',saveUncertain:'$state(false)',savedListId:'$state<string|null>(null)',proposedDate:'$state(false)',proposedPlace:'$state(false)',query:'$state("")',results:'$state<Species[]>([])'})) {
  text=text.replace(`${name}=${initial}`,`${name}=$state(${JSON.stringify(h.run(name))})`);
 }
 const {default:component}=await import(creationUrl(text,locale,h.run('entryId')));
 return render(component).body;
}
const settle=()=>new Promise(resolve=>setImmediate(resolve));
test('initialization waits for the entry and place before mounting the real date picker',async()=>{
 let resolveEntry;const pending=new Promise(resolve=>resolveEntry=resolve);
 const h=formHarness({entry:()=>pending});h.mount();await settle();
 assert.equal(h.run('available'),true);assert.equal(h.run('initialized'),false);
 // Previously availability alone mounted DatePicker with date="", throwing
 // RangeError: Invalid time value and leaving the old loading DOM in place.
 const loading=await creationHtml(h);assert.match(loading,/Läser in/);assert.doesNotMatch(loading,/<form/);
 resolveEntry({generation:7,data:{id:'origin',entryDate:'2026-11-12'}});await settle();
 assert.deepEqual(h.initialization,['bird_observations_available','get_journal_entry','read_journal_place','list_places']);
 assert.equal(h.run('initialized'),true);assert.equal(h.run('generation'),7);assert.equal(h.run('place.id'),'place');
 const html=await creationHtml(h);assert.match(html,/<form/);assert.match(html,/november/);assert.doesNotMatch(html,/Läser in/);
 assert.equal(h.calls.length,0);
});
test('every initialization failure displays a localized actionable error and a way back',async()=>{
 for(const failAt of ['bird_observations_available','get_journal_entry','read_journal_place','list_places']){
  const h=formHarness({failAt});h.mount();await settle();
  assert.equal(h.run('error'),'load');assert.equal(h.run('generation'),null);
  for(const locale of ['sv','en']){
   const html=await creationHtml(h,locale);assert.match(html,/role="alert"/);assert.doesNotMatch(html,/<form|role="status"/);
   assert.match(html,locale==='sv'?/försök igen/:/try again/);assert.match(html,locale==='sv'?/Tillbaka till anteckningen/:/Back to journal entry/);
  }
  h.run('cancel()');await settle();assert.deepEqual(h.navigation,['/journal/entries/origin']);assert.equal(h.calls.length,0);
 }
});
test('unsupported platform stops initialization and an invalid legacy date remains editable',async()=>{
 const unsupported=formHarness({enabled:false});unsupported.mount();await settle();
 assert.deepEqual(unsupported.initialization,['bird_observations_available']);
 assert.doesNotMatch(await creationHtml(unsupported),/<form|role="status"/);
 const h=formHarness({entry:async()=>({generation:7,data:{id:'origin',entryDate:'invalid'}})});h.mount();await settle();
 const html=await creationHtml(h);assert.match(html,/<form/);assert.match(html,/type="date"/);assert.equal(h.run('date'),'');
});

test('keyboard species selection focuses count, Enter returns to search and removal keeps focus nearby',async()=>{
 const h=formHarness();h.run(`searchInput=searchControl;results=${JSON.stringify([species])};query='Ruddy';`);
 let prevented=false;h.run('searchKeydown')({key:'Enter',preventDefault:()=>prevented=true});await settle();
 assert.equal(prevented,true);assert.equal(h.run('rows.length'),1);assert.equal(h.run('rows[0].count'),'');assert.deepEqual(h.focus,['bird-count-0']);
 h.run('countKeydown')({key:'Enter',preventDefault:()=>{}},'0');assert.deepEqual(h.focus,['bird-count-0']);
 h.run('countKeydown')({key:'Enter',preventDefault:()=>{}},'6');assert.equal(h.focus.at(-1),'search');
 h.run('searchKeydown')({key:'ArrowDown',preventDefault:()=>{}});assert.equal(h.focus.at(-1),'result-0');
 h.run('resultKeydown')({key:'ArrowUp',preventDefault:()=>{}},0);assert.equal(h.focus.at(-1),'search');
 h.run('resultKeydown')({key:'Escape',preventDefault:()=>{}},0);assert.equal(h.run('query'),'');assert.equal(h.focus.at(-1),'search');
 await h.run(`add(${JSON.stringify({...species,id:'second'})})`);assert.equal(h.focus.at(-1),'bird-count-1');
 await h.run('removeRow(0)');assert.equal(h.focus.at(-1),'bird-count-0');assert.equal(h.run('rows[0].id'),'second');
 await h.run('removeRow(0)');assert.equal(h.focus.at(-1),'search');assert.equal(h.calls.length,0);
});
test('compact rows keep individual accessible count labels and localized adjacent validation',async()=>{
 const h=formHarness();ready(h);h.run('initialized=true;rows[0].count=""');
 for(const locale of ['sv','en']){
  const html=await creationHtml(h,locale);
  assert.match(html,/class="row-headings/);assert.match(html,/aria-label="(?:Antal för|Count for) Ruddy Duck/);
  assert.match(html,/aria-describedby="bird-count-help-0"/);assert.match(html,/aria-invalid="true"/);
  assert.match(html,locale==='sv'?/Ange ett heltal större än noll/:/Enter a whole number greater than zero/);
  assert.match(html,locale==='sv'?/Besökstider \(frivilligt\)/:/Visit times \(optional\)/);
  assert.doesNotMatch(html,/<label[^>]*>Antal<input|<label[^>]*>Count<input/);
 }
 const page=source('../src/lib/components/BirdListForm.svelte');
 assert.match(page,/fieldset \{ border:0/);assert.match(page,/max-width:824px/);assert.match(page,/:focus-visible/);
});

test('real invoke helper emits the shared Rust command-contract payload',async()=>{
 const expected=JSON.parse(source('./fixtures/bird-create-command.json'));
 const catalog=JSON.parse(source('../src-tauri/catalog/avilist-v2026.json'));
 const rows=expected.rows.map(r=>({...catalog.find(s=>s.id===r.speciesId),count:r.count,comment:r.comment??""}));
 const previous=globalThis.window;const calls=[];
 globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return '33333333-3333-4333-8333-333333333333';}}};
 try{
  const id=await birds.saveBirdList(expected.generation,expected.entryId,expected.visitDate,{...place,id:expected.placeId},rows,expected.visitStart,expected.visitEnd,expected.requestId);
  assert.equal(id,'33333333-3333-4333-8333-333333333333');assert.deepEqual(calls,[{command:'create_journal_bird_list',args:expected}]);
 }finally{if(previous===undefined)delete globalThis.window;else globalThis.window=previous;}
});
test('reused request with different content remains blocked and preserves the draft',async()=>{
 const h=formHarness({reject:'bird_request_conflict'});ready(h);h.run('initialized=true');await h.run('save()');
 assert.equal(h.run('error'),'request');assert.equal(h.run('saveUncertain'),true);assert.equal(h.run('dirty'),true);assert.equal(h.run('rows[0].count'),'6');await h.run('save()');assert.equal(h.calls.length,1);
 const html=await creationHtml(h);assert.match(html,/Inget skrevs över/);
});
test('lost committed response reuses the exact request and confirms before opening',async()=>{
 let committed=0;const requests=new Map();let lose=true;
 const h=formHarness({saveImplementation:async(...args)=>{
  const id=args[7];if(!requests.has(id)){requests.set(id,JSON.stringify(args));committed++;}
  else assert.equal(JSON.stringify(args),requests.get(id));
  if(lose){lose=false;throw Error('response lost after commit');}return id;
 }});ready(h);h.run('initialized=true');await h.run('save()');
 assert.equal(h.run('saveUncertain'),true);assert.equal(h.run('rows[0].count'),'6');assert.equal(h.run('start'),'08:15');assert.equal(h.run('dirty'),true);assert.equal(h.navigation.length,0);
 for(const locale of ['sv','en']){const html=await creationHtml(h,locale);assert.match(html,locale==='sv'?/Kontrollera sparningen/:/Check save/);}
 await h.run('save()');assert.equal(committed,1);assert.equal(h.calls.length,2);assert.equal(h.calls[0][7],h.calls[1][7]);assert.equal(h.confirmations.length,1);assert.equal(h.run('saveUncertain'),false);assert.equal(h.run('saved'),true);
 assert.deepEqual(h.navigation,['/journal/entries/origin/birds/33333333-3333-4333-8333-333333333333']);
});
test('unconfirmed response keeps inputs and request identity on repeated check',async()=>{
 const h=formHarness({reject:true});ready(h);h.run('initialized=true');await h.run('save()');await h.run('save()');
 assert.equal(h.calls.length,2);assert.equal(h.calls[0][7],h.calls[1][7]);assert.equal(h.run('rows[0].count'),'6');assert.equal(h.run('end'),'09:30');assert.equal(h.navigation.length,0);
 h.run('cancel()');await settle();assert.equal(h.run('leave'),true);
});
test('post-commit confirmation failure retains saved identity without resubmission',async()=>{
 const h=formHarness({rejectConfirmation:true});ready(h);h.run('initialized=true');await h.run('save()');await h.run('save()');
 assert.equal(h.calls.length,1);assert.equal(h.confirmations.length,1);assert.equal(h.diagnostics[0][0],'bird-list: saved, confirmation failed');assert.equal(h.run('savedListId'),'saved-list');assert.equal(h.run('error'),'saved');assert.equal(h.navigation.length,0);assert.equal(h.run('rows[0].count'),'6');
});
test('opening failure after a committed response keeps the saved identity and never saves twice',async()=>{
 const h=formHarness({rejectNavigation:true});ready(h);h.run('initialized=true');await h.run('save()');
 assert.equal(h.run('savedListId'),'saved-list');assert.equal(h.run('saved'),true);assert.equal(h.run('error'),'saved');assert.equal(h.diagnostics[0][0],'bird-list: saved, navigation failed');await h.run('save()');assert.equal(h.calls.length,1);
 const html=await creationHtml(h);assert.match(html,/Observationen sparades men kunde inte öppnas/);assert.match(html,/href="\/journal\/entries\/origin\/birds\/saved-list"/);
});

test('real saved-list confirmation checks the read contract, current generation and returned identity',async()=>{
 const code=p=>ts.transpileModule(source(p).replace(/^import .*;\n/gm,'').replace(/^export /gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
 const id='33333333-3333-4333-8333-333333333333';
 const payload={id,visitDate:'2026-11-12',visitStart:null,visitEnd:null,place,rows:[{...species,speciesId:species.id,taxonomyVersion:'AviList v2026',count:'6'}]};
 const calls=[];let generation=1,current=1,returned=id;
 const context=vm.createContext({invoke:async(command,args)=>{calls.push({command,args});return command==='new_journal_session'?{generation:current,data:null}:{generation,data:{...payload,id:returned}};},BigInt,Date,Error,Number,Math,RegExp});
 vm.runInContext(code('../src/lib/journal-data.ts')+'\n'+code('../src/lib/birds.ts'),context);
 const confirm=()=>vm.runInContext(`confirmSavedBirdList(1,'origin','${id}')`,context);
 assert.equal((await confirm()).id,id);assert.deepEqual(calls.map(c=>c.command),['read_journal_bird_list','new_journal_session']);
 returned='unexpected';await assert.rejects(confirm(),/bird_save_confirmation_invalid/);
 returned=id;current=2;await assert.rejects(confirm(),e=>e==='journal_generation_conflict');
});

test('name view preferences are shared between lists, remembered and preserve missing-name fallback',()=>{
 const values=new Map();const storage={getItem:k=>values.get(k)??null,setItem:(k,v)=>values.set(k,v)};
 assert.deepEqual(names.loadBirdNamePreferences(storage),{showOtherLanguage:false,showScientific:false});
 assert.equal(names.saveBirdNamePreferences({showOtherLanguage:true,showScientific:true},storage),true);
 assert.deepEqual(names.loadBirdNamePreferences(storage),{showOtherLanguage:true,showScientific:true});
 const s={english:'Great Tit',swedish:'talgoxe',scientific:'Parus major'};
 assert.deepEqual(names.displayBirdNames(s,'sv',names.loadBirdNamePreferences(storage)),{primary:'Talgoxe',other:'Great Tit',scientific:'Parus major',missingSwedish:false});
 assert.equal(names.displayBirdNames(s,'en',names.loadBirdNamePreferences(storage)).other,'Talgoxe');
 assert.deepEqual(names.displayBirdNames(s,'sv',names.DEFAULT_BIRD_NAMES),{primary:'Talgoxe',other:null,scientific:null,missingSwedish:false});
 assert.deepEqual(names.displayBirdNames(species,'sv',names.DEFAULT_BIRD_NAMES),{primary:'Ruddy Duck',other:null,scientific:'Oxyura jamaicensis',missingSwedish:true});
 assert.equal(names.saveBirdNamePreferences(names.DEFAULT_BIRD_NAMES,{getItem:()=>null,setItem:()=>{throw Error('denied');}}),false);
 assert.deepEqual(names.loadBirdNamePreferences({getItem:()=>'{bad',setItem:()=>{}}),names.DEFAULT_BIRD_NAMES);
});
test('reselecting the same concept focuses its count and preserves its comment and count',async()=>{
 const h=formHarness();ready(h);h.run('rows[0].comment="Satt i björken";rows[0].commentOpen=true');
 await h.run(`add(${JSON.stringify(species)})`);
 assert.equal(h.run('rows.length'),1);assert.equal(h.run('rows[0].count'),'6');assert.equal(h.run('rows[0].comment'),'Satt i björken');assert.equal(h.focus.at(-1),'bird-count-0');
 await h.run('openComment(0)');assert.equal(h.focus.at(-1),'bird-comment-0');assert.equal(h.calls.length,0);
 assert.equal(birds.validBirdComment('x'.repeat(8193)),false);assert.equal(birds.validBirdComment('plain\ntext'),true);assert.equal(birds.validBirdComment('bad\0text'),false);
});
test('compact separate visit previews hide same journal date and do not expose comments or optional names',async()=>{
 const rows=[{...species,english:'Great Tit',swedish:'talgoxe',scientific:'Parus major',count:'6',comment:'PRIVATE COMMENT'}];
 const lists=[{id:'morning',place,visitDate:'2026-11-12',visitStart:'08:00',visitEnd:null,rows},{id:'evening',place,visitDate:'2026-11-13',visitStart:'18:00',visitEnd:null,rows}];
 const text=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)').replace('$state<BirdList[]>([])',`$state<BirdList[]>(${JSON.stringify(lists)})`);
 const {default:c}=await import(previewUrl(text));const html=render(c,{props:{entryId:'origin',generation:1,locale:'sv',journalDate:'2026-11-12'}}).body;
 assert.match(html,/08:00/);assert.match(html,/18:00/);assert.doesNotMatch(html,/2026-11-12|PRIVATE COMMENT|Great Tit|Parus major/);assert.match(html,/13 november 2026/);assert.match(html,/Talgoxe/);
 assert.match(html,/origin\/birds\/morning/);assert.match(html,/origin\/birds\/evening/);
});
test('detail cards preserve legacy duplicate rows and render comments as plain text with origin navigation',async()=>{
 const row={...species,swedish:'talgoxe',english:'Great Tit',scientific:'Parus major',count:'6',comment:'I björken.\n<script>not markup</script>'};
 const list={id:'list',visitDate:'2026-11-12',visitStart:null,visitEnd:null,place,rows:[row,{...row,count:'4',comment:'Another saved row'}]};
 const {default:c}=await import(detailUrl());
 for(const locale of ['sv','en']){
  const html=render(c,{props:{list,origin:'/journal/entries/origin',locale,preferences:{showOtherLanguage:true,showScientific:true},onPreferencesChange:()=>{}}}).body;
  assert.match(html,/href="\/journal\/entries\/origin"/);assert.match(html,/Parus major/);assert.match(html,/Great Tit/);assert.match(html,/Talgoxe/);assert.match(html,/I björken/);assert.match(html,/Another saved row/);assert.doesNotMatch(html,/<script>/);assert.match(html,/&lt;script>/);
  assert.equal((html.match(/class="species-names/g)??[]).length,2);
 }
});

test('display helpers capitalize common names only and keep exact counts',()=>{
 const s=Object.freeze({english:'Great Tit',swedish:'talgoxe',scientific:'Parus major'});
 const display=names.displayBirdNames(s,'sv',{showOtherLanguage:true,showScientific:true});
 assert.equal(display.primary,'Talgoxe');assert.equal(display.scientific,'Parus major');assert.equal(s.swedish,'talgoxe');
 assert.equal(names.capitalizeCommonName('ängspiplärka','sv'),'Ängspiplärka');
 assert.equal(names.capitalizeCommonName('örnfalk','sv'),'Örnfalk');assert.equal(names.capitalizeCommonName('årta','sv'),'Årta');assert.equal(names.capitalizeCommonName('avibase-FBBBA943','sv'),'avibase-FBBBA943');
 assert.equal(names.formatBirdCount('9223372036854775807','en'),'9,223,372,036,854,775,807');
});
test('localized date-only display remains the same day across distant time zones',async()=>{
 const {execFileSync}=await import('node:child_process');
 const {namesUrl}=await import('./bird-test-modules.mjs');
 for(const TZ of ['America/Los_Angeles','Pacific/Kiritimati']){
  const code=`import {formatBirdDate} from ${JSON.stringify(namesUrl)};console.log(JSON.stringify([formatBirdDate('2026-11-12','sv'),formatBirdDate('2026-11-12','en'),formatBirdDate('2026-01-01','sv')]))`;
  const actual=JSON.parse(execFileSync(process.execPath,['--input-type=module','-e',code],{env:{...process.env,TZ},encoding:'utf8'}));
  assert.deepEqual(actual,['12 november 2026','12 November 2026','1 januari 2026']);
 }
});
test('detail cards label species/count/comment and localize singular and plural without changing preferences',async()=>{
 const {default:c}=await import(detailUrl());
 const list={id:'one',place,visitDate:'2026-11-12',visitStart:'08:00',visitEnd:null,rows:[{...species,swedish:'skata',english:'Eurasian Magpie',scientific:'Pica pica',count:'1',comment:'Ensam vid sjön'},{...species,swedish:'blåmes',english:'Eurasian Blue Tit',scientific:'Cyanistes caeruleus',count:'4',comment:null}]};
 for(const locale of ['sv','en']){
  const html=render(c,{props:{list,origin:'/journal/entries/origin',locale,preferences:{showOtherLanguage:true,showScientific:true},onPreferencesChange:()=>{}}}).body;
  assert.match(html,locale==='sv'?/>Art</:/>Species</);assert.match(html,locale==='sv'?/>Antal</:/>Count</);
  assert.match(html,locale==='sv'?/1 individ</:/1 individual</);assert.match(html,locale==='sv'?/4 individer</:/4 individuals</);
  assert.match(html,locale==='sv'?/Skata/:/Eurasian Magpie/);assert.match(html,/Pica pica/);assert.doesNotMatch(html,/Pica Pica/);
  assert.equal((html.match(locale==='sv'?/Din kommentar/g:/Your comment/g)??[]).length,1);
  assert.match(html,locale==='sv'?/Fåglar du noterade under besöket/:/Birds you recorded during your visit/);
  assert.match(html,locale==='sv'?/>12 november 2026</:/>12 November 2026</);
 }
});
test('creation contextual copy reflects only values actually proposed from the journal',async()=>{
 const cases=[{options:{},sv:'Datum och plats är hämtade',en:'The date and place come'},
 {options:{journalPlace:null},sv:'Datumet är hämtat',en:'The date comes'},
 {options:{entry:async()=>({generation:7,data:{id:'origin',entryDate:'invalid'}})},sv:'Platsen är hämtad',en:'The place comes'}];
 for(const {options,sv,en} of cases){const h=formHarness(options);h.mount();await settle();for(const locale of ['sv','en']){
  const html=await creationHtml(h,locale);assert.ok(html.includes(locale==='sv'?sv:en));assert.match(html,locale==='sv'?/Vilka fåglar såg du under besöket/:/Which birds did you see during your visit/);
 }}
 const h=formHarness({journalPlace:null,entry:async()=>({generation:7,data:{id:'origin',entryDate:'invalid'}})});h.mount();await settle();assert.doesNotMatch(await creationHtml(h),/hämtad(?:e|t)? från anteckningen/);
});
test('journal preview spacing and context are compact and appear once only in the detail section',async()=>{
 const lists=[{id:'one',place,visitDate:'2026-11-12',visitStart:null,visitEnd:null,rows:[{...species,swedish:'skata',count:'4'},{...species,swedish:'blåmes',count:'2'}]},{id:'two',place,visitDate:'2026-11-12',rows:[]}];
 const text=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)').replace('$state<BirdList[]>([])',`$state<BirdList[]>(${JSON.stringify(lists)})`);
 const {default:c}=await import(previewUrl(text));
 for(const allowCreate of [false,true]){
  const html=render(c,{props:{entryId:'origin',generation:1,locale:'sv',journalDate:'2026-11-12',allowCreate}}).body;
  const plain=html.replace(/<!--[\s\S]*?-->/g,'').replace(/<[^>]+>/g,'').replace(/\s+/g,' ');
  assert.match(plain,/Skata 4 · Blåmes 2/);
  assert.equal((html.match(/Fåglar du noterade under besöket\./g)??[]).length,allowCreate?1:0);
 }
});

test('detail presentation preserves bilingual identity and comments across name preferences',async()=>{
 const {default:c}=await import(detailUrl());
 const comment='Min egen kommentar.\nSame text in either language.';
 const rows=[{...species,swedish:'skata',english:'Eurasian Magpie',scientific:'Pica pica',count:'1',comment:null},{...species,swedish:'svartvit stenskvätta med ett långt visningsnamn',english:'Eastern Black-eared Wheatear with a long display name',scientific:'Oenanthe melanoleuca',count:'14',comment}];
 for(const locale of ['sv','en'])for(const showOtherLanguage of [false,true])for(const showScientific of [false,true]){
  const html=render(c,{props:{list:{id:'layout',place,visitDate:'2026-11-12',rows},origin:'/journal/entries/origin',locale,preferences:{showOtherLanguage,showScientific},onPreferencesChange:()=>{}}}).body;
  assert.equal((html.match(/class="identity /g)??[]).length,2);
  assert.equal((html.match(/class="count /g)??[]).length,2);
  assert.ok(html.includes(locale==='sv'?'Svartvit stenskvätta':'Eastern Black-eared Wheatear'));
  assert.equal(html.includes('Oenanthe melanoleuca'),showScientific);
  assert.equal(html.includes(locale==='sv'?'Eastern Black-eared Wheatear':'Svartvit stenskvätta'),showOtherLanguage);
  assert.ok(html.includes(comment));
  assert.equal((html.match(/class="comment-section /g)??[]).length,1);
 }
});


test('one collapsed shared name display follows all cards and precedes attribution',async()=>{
 const {default:c}=await import(detailUrl());
 for(const locale of ['sv','en']){
  const html=render(c,{props:{list:{id:'placement',place,visitDate:'2026-11-12',rows:[{...species,count:'1'},{...species,count:'4'}]},origin:'/journal/entries/origin',locale,preferences:{showOtherLanguage:true,showScientific:true},onPreferencesChange:()=>{}}}).body;
  const controls=[...html.matchAll(/<details class="view-options[^>]*>/g)];
  assert.equal(controls.length,1);assert.doesNotMatch(controls[0][0],/\bopen\b/);
  assert.ok(controls[0].index>html.indexOf('</ul>'));
  const label=locale==='sv'?'Namnvisning':'Name display';
  assert.ok(html.includes(label));
  const attribution=locale==='sv'?'Artregister och källor':'Species catalog and attribution';
  assert.ok(html.indexOf(attribution)>html.indexOf(label));
  assert.equal((html.match(/type="checkbox"[^>]*checked/g)??[]).length,2);
 }
});


test('editing loads every legacy row and preserves recorded identities; cancel never saves',async()=>{
 const row={...species,speciesId:species.id,taxonomyVersion:'Older catalog',count:'3',comment:'Keep this'};
 const list={id:'existing',editToken:'a'.repeat(64),place,visitDate:'2026-11-12',visitStart:'08:00',visitEnd:null,rows:[row,{...row,count:'7'}]};
 const h=formHarness({editId:'existing',editList:list});h.mount();await settle();
 assert.equal(h.run('initialized'),true);assert.equal(h.run('rows.length'),2);assert.equal(h.run('rows[1].count'),'7');assert.equal(h.run('rows[0].taxonomyVersion'),'Older catalog');
 h.run('cancel()');await settle();assert.deepEqual(h.navigation,['/journal/entries/origin/birds/existing']);assert.equal(h.calls.length,0);
 const x=formHarness({editId:'existing',editList:list});x.mount();await settle();x.run('rows[0].count="8";dirty=true;cancel()');await settle();assert.equal(x.run('leave'),true);assert.equal(x.calls.length,0);await x.run('discard()');assert.deepEqual(x.navigation,['/journal/entries/origin/birds/existing']);
});
test('edit success refreshes previews and returns to detail; stale or uncertain save retains input without recreating',async()=>{
 for(const reject of [false,true]){
  const h=formHarness({editId:'existing',reject});ready(h);h.run('expectedToken="old-token"');await h.run('save()');
  assert.equal(h.calls.length,1);assert.equal(h.calls[0][2],'existing');assert.equal(h.calls[0][3],'old-token');assert.equal(h.calls[0][6][0].count,'6');
  if(reject){assert.equal(h.run('error'),'edit');assert.equal(h.run('rows[0].count'),'6');assert.equal(h.run('dirty'),true);assert.equal(h.navigation.length,0);await h.run('save()');assert.equal(h.calls.length,1);}
  else {assert.deepEqual(h.navigation,['/journal/entries/origin/birds/existing']);assert.equal(h.run('savedListId'),'existing');assert.equal(h.confirmations.length,1);}
 }
 const detail=source('../src/lib/components/BirdListDetail.svelte');assert.match(detail,/<EntryOptions/);assert.ok(detail.indexOf('<EntryOptions')<detail.indexOf('<ul class="species-cards"'));
 const previews=source('../src/lib/components/BirdJournalLists.svelte');assert.match(previews,/addEventListener\("mno:journal-changed",refresh\)/);assert.match(previews,/removeEventListener\("mno:journal-changed",refresh\)/);
});


test('real edit invoke preserves identity, recorded taxonomy and optimistic token in the command contract',async()=>{
 const expected=JSON.parse(source('./fixtures/bird-create-command.json'));delete expected.requestId;expected.id='existing';expected.expectedToken='a'.repeat(64);
 const catalog=JSON.parse(source('../src-tauri/catalog/avilist-v2026.json'));
 const rows=expected.rows.map(r=>({...catalog.find(s=>s.id===r.speciesId),taxonomyVersion:r.taxonomyVersion,count:r.count,comment:r.comment??''}));
 const previous=globalThis.window;const calls=[];
 globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return 'b'.repeat(64);}}};
 try{assert.equal(await birds.updateBirdList(expected.generation,expected.entryId,expected.id,expected.expectedToken,expected.visitDate,{...place,id:expected.placeId},rows,expected.visitStart,expected.visitEnd),'b'.repeat(64));assert.deepEqual(calls,[{command:'update_journal_bird_list',args:expected}]);}
 finally{if(previous===undefined)delete globalThis.window;else globalThis.window=previous;}
});

test('mounted journal previews reread edited content on the shared change event and clean up listeners',async()=>{
 const effects=[],mounts=[],listeners=new Map();let count='2',reads=0;
 const context=vm.createContext({$props:()=>({entryId:'origin',generation:7,locale:'sv'}),$state:v=>v,$effect:fn=>effects.push(fn),onMount:fn=>mounts.push(fn),invoke:async()=>true,makeBirdReader:()=>async()=>{reads++;return {generation:7,data:[{id:'same-list',place,visitDate:'2026-11-12',rows:[{...species,count}]}]};},window:{addEventListener:(name,fn)=>listeners.set(name,fn),removeEventListener:(name)=>listeners.delete(name)}});
 const script=source('../src/lib/components/BirdJournalLists.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 const unmount=mounts[0]();let cleanup=effects[0]();await settle();assert.equal(vm.runInContext('lists[0].rows[0].count',context),'2');
 count='9';listeners.get('mno:journal-changed')();assert.equal(vm.runInContext('revision',context),1);cleanup();cleanup=effects[0]();await settle();assert.equal(reads,2);assert.equal(vm.runInContext('lists[0].rows[0].count',context),'9');
 cleanup();unmount();assert.equal(listeners.size,0);
});

function deletionHarness({mode='success',originExists=true,rejectNavigation=false,entryId='origin',refreshToken=null,refreshFails=false}={}){
 let listReads=0;const list={id:'visit',editToken:'a'.repeat(64),visitDate:'2026-11-12',visitStart:'08:00',visitEnd:'09:00',place,rows:[{...species,count:'4',comment:'Keep until confirmed'}]};
 const mounts=[],calls=[],checks=[],navigation=[],events=[];let exists=true,failStatus=mode==='unknown';
 const context=vm.createContext({$props:()=>({entryId,listId:'visit'}),$derived:v=>v,$state:v=>v,onMount:fn=>mounts.push(fn),page:{params:{id:'origin',listId:'visit'}},localVisitDate:birds.localVisitDate,entryHref:birds.entryHref,birdHref:birds.birdHref,DEFAULT_BIRD_NAMES:names.DEFAULT_BIRD_NAMES,loadBirdNamePreferences:()=>({...names.DEFAULT_BIRD_NAMES}),saveBirdNamePreferences:()=>true,
 invoke:async cmd=>{if(cmd==='bird_observations_available')return true;if(cmd==='new_journal_session')return {generation:7};throw Error(cmd);},
 makeBirdReader:()=>async cmd=>{if(cmd==='read_journal_bird_list'){listReads++;if(listReads>1&&refreshFails)throw Error('refresh failed');return {generation:7,data:refreshToken&&listReads>1?{...list,editToken:refreshToken}:list};}return {generation:7,data:originExists?{id:'origin'}:null};},
 deleteBirdList:async(...args)=>{calls.push(args);if(mode==='failed')throw Error('journal_generation_conflict');exists=false;if(mode==='lost'||mode==='unknown')throw Error('response lost');},
 birdListExists:async(...args)=>{checks.push(args);if(failStatus)throw Error('status unavailable');return exists;},
 goto:async href=>{navigation.push(href);if(rejectNavigation)throw Error('navigation failed');},window:{dispatchEvent:e=>events.push(e.type)},Event,console:{error:()=>{}}});
 const script=source('../src/lib/components/BirdListPage.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {run:s=>vm.runInContext(s,context),mount:()=>mounts[0](),calls,checks,navigation,events,resolveStatus:()=>{failStatus=false;}};
}
test('whole-list cancellation or unconfirmed calls do not invoke deletion',async()=>{
 const h=deletionHarness();h.mount();await settle();await h.run('confirmDeletion()');assert.equal(h.calls.length,0);
 h.run('requestDeletion()');assert.equal(h.run('deleteDialog'),true);h.run('cancelDeletion()');assert.equal(h.run('deleteDialog'),false);assert.equal(h.calls.length,0);assert.equal(h.checks.length,0);assert.equal(h.navigation.length,0);
});
test('whole-list deletion refreshes previews and selects the originating entry or safe reader fallback',async()=>{
 for(const originExists of [true,false]){
  const h=deletionHarness({originExists});h.mount();await settle();h.run('requestDeletion()');await h.run('confirmDeletion()');
  assert.deepEqual(h.calls,[[7,'visit','a'.repeat(64)]]);assert.deepEqual(h.checks,[[7,'visit']]);assert.deepEqual(h.events,['mno:journal-changed']);assert.deepEqual(h.navigation,[originExists?'/journal/entries/origin':'/journal?latest=true']);
  await h.run('confirmDeletion()');assert.equal(h.calls.length,1);
 }
});
test('lost response uses a read-only check; stale, uncertain and navigation failures never repeat deletion',async()=>{
 for(const mode of ['lost','failed','unknown']){
  const h=deletionHarness({mode});h.mount();await settle();h.run('requestDeletion()');await h.run('confirmDeletion()');
  assert.equal(h.calls.length,1);assert.equal(h.run('list.rows[0].comment'),'Keep until confirmed');
  if(mode==='lost'){assert.equal(h.run('deleted'),true);assert.equal(h.navigation.length,1);}
  else{assert.equal(h.run('deleteError'),'failed');assert.equal(h.navigation.length,0);assert.equal(h.run('deleteDialog'),false);h.run('requestDeletion()');await h.run('confirmDeletion()');assert.equal(h.calls.length,1);h.resolveStatus();await h.run('checkDeletion()');assert.equal(h.calls.length,1);assert.equal(h.navigation.length,mode==='unknown'?1:0);}
 }
 const h=deletionHarness({rejectNavigation:true});h.mount();await settle();h.run('requestDeletion()');await h.run('confirmDeletion()');assert.equal(h.run('deleted'),true);assert.equal(h.run('deleteError'),'navigation');assert.equal(h.calls.length,1);
});
test('real delete invoke sends exact identity, generation, token and explicit confirmation',async()=>{
 const previous=globalThis.window;const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});}}};
 try{await birds.deleteBirdList(7,'visit','a'.repeat(64));assert.deepEqual(calls,[{command:'delete_journal_bird_list',args:{generation:7,id:'visit',expectedToken:'a'.repeat(64),confirmed:true}}]);}
 finally{if(previous===undefined)delete globalThis.window;else globalThis.window=previous;}
});
test('whole-list confirmation identifies the visit and retained data, with Cancel focused and focus restored',async()=>{
 const {deleteDialogUrl}=await import('./bird-test-modules.mjs');const {default:Dialog}=await import(deleteDialogUrl);const m=await import('../src/lib/paraglide/messages.js');
 for(const locale of ['sv','en']){
  const heading=m.bird_delete({}, {locale});const description=m.bird_delete_confirm({place:'Lake',date:names.formatBirdDate('2026-11-12',locale),times:' · 08:00–09:00'},{locale});
  const html=render(Dialog,{props:{title:'Lake',date:'2026-11-12',locale,busy:false,heading,description,oncancel:()=>{},onconfirm:()=>{}}}).body;
  assert.ok(html.includes(heading));assert.ok(html.includes('Lake'));assert.ok(html.includes('08:00–09:00'));assert.match(html,locale==='sv'?/Journaltexten och den sparade platsen finns kvar/:/Journal text and the saved place will remain/);assert.match(html,locale==='sv'?/Originalbilder påverkas inte/:/Original photos are kept/);
  assert.ok(html.indexOf(locale==='sv'?'Avbryt':'Cancel')<html.lastIndexOf(heading));assert.match(html,/aria-describedby=/);assert.match(html,/aria-labelledby=/);
 }
 let focused=0,restored=0,opened=0,closed=0;class Element{isConnected=true;focus(){restored++;}}
 const script=source('../src/lib/components/DeleteEntryDialog.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');
 const context=vm.createContext({$props:Object.assign(()=>({title:'Lake',date:'date',locale:'sv',busy:false,oncancel(){},onconfirm(){}}),{id:()=> 'test'}),document:{activeElement:new Element()},HTMLElement:Element});
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 context.dialog={showModal:()=>opened++,close:()=>closed++,querySelector:()=>({focus:()=>focused++})};
 vm.runInContext('const action=mountDialog(dialog);action.destroy()',context);assert.equal(opened,1);assert.equal(focused,1);assert.equal(restored,1);assert.equal(closed,1);
 const {default:Detail}=await import(detailUrl());const html=render(Detail,{props:{list:{id:'visit',visitDate:'2026-11-12',place,rows:[]},origin:'/journal/entries/origin',editHref:'/journal/entries/origin/birds/visit/edit',onDelete:()=>{},locale:'en',preferences:names.DEFAULT_BIRD_NAMES,onPreferencesChange:()=>{}}}).body;
 assert.match(html,/>Edit</);assert.match(html,/>Delete bird observation</);assert.match(html,/aria-expanded="false"/);
});

test('real read-only deletion status rejects malformed data and changed generations',async()=>{
 const code=p=>ts.transpileModule(source(p).replace(/^import .*;\n/gm,'').replace(/^export /gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
 const calls=[];let data=false,current=7;
 const context=vm.createContext({invoke:async(command,args)=>{calls.push({command,args});return command==='new_journal_session'?{generation:current,data:null}:{generation:7,data};},BigInt,Date,Error,Number,Math,RegExp});
 vm.runInContext(code('../src/lib/journal-data.ts')+'\n'+code('../src/lib/birds.ts'),context);
 const check=()=>vm.runInContext("birdListExists(7,'visit')",context);
 assert.equal(await check(),false);data=true;assert.equal(await check(),true);
 assert.deepEqual(calls.map(c=>c.command),['journal_bird_list_exists','new_journal_session','journal_bird_list_exists','new_journal_session']);
 data='false';await assert.rejects(check(),/bird_delete_confirmation_invalid/);
 data=false;current=8;await assert.rejects(check(),e=>e==='journal_generation_conflict');
 assert.ok(calls.every(c=>c.command!=='delete_journal_bird_list'));
});

function timeHarness(value='',disabled=false){
 let focused=0,triggerFocused=0;class Node{}
 const context=vm.createContext({$props:()=>({value,id:'visit-time',label:'Start',locale:'en',disabled}),$bindable:v=>v,$state:v=>v,$derived:v=>v,$effect:()=>{},validTime:birds.validTime,Node,focusInput:()=>focused++,focusTrigger:()=>triggerFocused++});
 const script=source('../src/lib/components/VisitTimePicker.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {run:s=>vm.runInContext(s,context),focused:()=>focused,triggerFocused:()=>triggerFocused};
}
test('visit time picker supports every exact minute, direct typing, clearing and unchanged invalid input',()=>{
 const h=timeHarness('08:17');assert.equal(h.run('value'),'08:17');
 h.run('selectPart("hour","23")');assert.equal(h.run('value'),'23:17');
 for(let minute=0;minute<60;minute++){const text=String(minute).padStart(2,'0');h.run(`selectPart("minute","${text}")`);assert.equal(h.run('value'),`23:${text}`);}
 h.run('typeTime({currentTarget:{value:"06:43"}})');assert.equal(h.run('value'),'06:43');assert.equal(h.run('validTime(value)'),true);
 h.run('typeTime({currentTarget:{value:"25:99"}})');assert.equal(h.run('value'),'25:99');assert.equal(h.run('validTime(value)'),false);
 h.run('selectPart("hour","24")');assert.equal(h.run('value'),'25:99');
 h.run('open=true;input={focus:focusInput};clear()');assert.equal(h.run('open'),false);assert.equal(h.run('value'),'');assert.equal(h.run('validTime(value)'),true);assert.equal(h.focused(),1);
 h.run('selectPart("minute","07")');assert.equal(h.run('value'),'00:07');
 const disabled=timeHarness('09:13',true);disabled.run('typeTime({currentTarget:{value:"10:00"}});selectPart("minute","14");clear()');assert.equal(disabled.run('value'),'09:13');
});
test('visit time popup closes on Escape and restores focus to its control',()=>{
 const h=timeHarness('08:17');h.run('picker={contains:()=>true,querySelector:()=>({focus:focusTrigger}),parentElement:{contains:()=>true}};open=true;keydown({key:"Escape",target:new Node(),preventDefault(){},stopPropagation(){}})');
 assert.equal(h.run('open'),false);assert.equal(h.triggerFocused(),1);
 h.run('picker.parentElement.contains=()=>false;open=true;outside({target:new Node()})');assert.equal(h.run('open'),false);
 h.run('open=true;leaveControl({relatedTarget:new Node(),currentTarget:{contains:()=>false}})');assert.equal(h.run('open'),false);
});
test('real time control has localized labels, complete hour/minute selectors and validation associations',async()=>{
 const {timePickerUrl}=await import('./bird-test-modules.mjs');const {default:Picker}=await import(timePickerUrl);
 for(const locale of ['sv','en']){
  for(const value of ['08:17','','25:99']){
   const html=render(Picker,{props:{id:'bird-start',value,locale,label:locale==='sv'?'Starttid':'Start time',descriptionId:'bird-start-help'}}).body;
   assert.match(html,/id="bird-start"/);assert.match(html,/type="text"/);assert.match(html,/aria-describedby="bird-start-help"/);assert.match(html,new RegExp(`aria-invalid="${!birds.validTime(value)}"`));
   assert.match(html,locale==='sv'?/>Timmar</:/>Hours</);assert.match(html,locale==='sv'?/>Minuter</:/>Minutes</);
   assert.equal((html.match(/<option /g)??[]).length,86);assert.match(html,/>59</);assert.match(html,/>23</);assert.match(html,locale==='sv'?/>Rensa tid</:/>Clear time</);assert.equal((html.match(/size="5"/g)??[]).length,2);assert.match(html,/<label[^>]*for="bird-start"/);
   const [hours,minutes]=value.split(":");assert.match(html,new RegExp(`id="bird-start"[^>]*value="${hours}"`));assert.match(html,new RegExp(`id="bird-start-minute"[^>]*value="${minutes??""}"`));
  }
 }
});


test('shared visit form clearly labels From/To and keeps clear actions inside the clock pickers',async()=>{
 const h=formHarness();ready(h);h.run('initialized=true');
 for(const locale of ['sv','en']){
  const html=await creationHtml(h,locale);
  assert.match(html,locale==='sv'?/<label[^>]*for="bird-start"[^>]*>Från</:/<label[^>]*for="bird-start"[^>]*>From</);
  assert.match(html,locale==='sv'?/<label[^>]*for="bird-end"[^>]*>Till</:/<label[^>]*for="bird-end"[^>]*>To</);
  const controls=[...html.matchAll(/<div class="time-control[\s\S]*?<\/details>/g)];assert.equal(controls.length,2);
  for(const [control] of controls){assert.match(control,/<svg[^>]*aria-hidden="true"/);assert.equal((control.match(/<button/g)??[]).length,1);assert.ok(control.indexOf('<button')>control.indexOf('class="time-panel'));assert.match(control,locale==='sv'?/>Rensa tid</:/>Clear time</);}
 }
});


test('sourced crow name/referral is explicit in search and selection keeps its species identity',async()=>{
 const row=JSON.parse(source('./fixtures/bird-crow-view.json'));
 const species={id:row.speciesId,english:row.english,swedish:row.swedish,scientific:row.scientific,referral:'gråkråka'};
 const h=formHarness();ready(h);h.run(`initialized=true;rows=[];query='gråkråka';results=${JSON.stringify([species])}`);
 for(const locale of ['sv','en']){
  const html=await creationHtml(h,locale);assert.match(html,locale==='sv'?/>Kråka</:/>Carrion Crow</);assert.match(html,/Gråkråka/);assert.match(html,locale==='sv'?/registreras arten, inte underarten/:/records the species, not the subspecies/);assert.doesNotMatch(html,/Gråkråka \(kråka\)/);
 }
 await h.run(`add(${JSON.stringify(species)})`);
 assert.equal(h.run('rows.length'),1);assert.equal(h.run('rows[0].id'),'avibase-E4EE9AC4');assert.equal(h.run('rows[0].swedish'),'Kråka');
 assert.match(await creationHtml(h,'sv'),/>Kråka</);assert.match(await creationHtml(h,'en'),/>Carrion Crow</);
 h.run('rows[0].count="4"');await h.run('save()');assert.equal(h.calls[0][4][0].id,'avibase-E4EE9AC4');
});
test('reopened crow wire displays Kråka in previews/detail and keeps English and optional names unchanged',async()=>{
 const row=JSON.parse(source('./fixtures/bird-crow-view.json'));const list={id:'crow-list',visitDate:'2026-11-12',place,rows:[row]};
 const original=JSON.parse(source('../src-tauri/catalog/avilist-v2026.json')).find(s=>s.id===row.speciesId);assert.equal(original.swedish,null);
 const supplement=JSON.parse(source('../src-tauri/catalog/name-supplements.json')).find(s=>s.id===row.speciesId);assert.equal(supplement.name,row.swedish);assert.equal(supplement.taxonomyVersion,row.taxonomyVersion);assert.ok(supplement.source.url.startsWith('https://'));
 const {default:Detail}=await import(detailUrl());
 const seeded=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)').replace('$state<BirdList[]>([])',`$state<BirdList[]>(${JSON.stringify([list])})`);
 const {default:Preview}=await import(previewUrl(seeded));
 for(const locale of ['sv','en']){
  const preview=render(Preview,{props:{entryId:'origin',generation:7,locale}}).body;assert.ok(preview.includes(locale==='sv'?'Kråka':'Carrion Crow'));assert.doesNotMatch(preview,/Gråkråka/);
  for(const showOtherLanguage of [false,true])for(const showScientific of [false,true]){
   const html=render(Detail,{props:{list,origin:'/journal/entries/origin',locale,preferences:{showOtherLanguage,showScientific},onPreferencesChange:()=>{}}}).body;
   assert.ok(html.includes(locale==='sv'?'Kråka':'Carrion Crow'));assert.equal(html.includes(locale==='sv'?'Carrion Crow':'Kråka'),showOtherLanguage);assert.equal(html.includes('Corvus corone'),showScientific);assert.ok(html.includes(row.comment));assert.doesNotMatch(html,/Gråkråka/);
  }
 }
});

test('backend reservations precede creation and retired/unreserved attempts retain data without retry',async()=>{
 for(const reject of ['bird_request_retired','bird_request_legacy','bird_request_unreserved']){
  const h=formHarness({reject});ready(h);await h.run('save()');assert.equal(h.calls.length,1);assert.equal(h.run('rows[0].count'),'6');await h.run('save()');assert.equal(h.calls.length,1);
 }
 const previous=globalThis.window;const calls=[];globalThis.window={__TAURI_INTERNALS__:{invoke:async(command,args)=>{calls.push({command,args});return '33333333-3333-4333-8333-333333333333';}}};
 try{assert.equal(await birds.reserveBirdRequest(7),'33333333-3333-4333-8333-333333333333');assert.deepEqual(calls,[{command:'reserve_bird_creation',args:{generation:7}}]);}
 finally{if(previous===undefined)delete globalThis.window;else globalThis.window=previous;}
});


test('standalone creation initializes without reading/creating an entry, cancels cleanly and saves a null origin',async()=>{
 const h=formHarness({entryId:null});h.mount();await settle();
 assert.equal(h.run('initialized'),true);assert.equal(h.run('proposedDate'),false);assert.equal(h.run('proposedPlace'),false);
 assert.deepEqual(h.initialization,['bird_observations_available','new_journal_session']);
 assert.equal(h.run('origin'),'/observations');h.run('cancel()');await settle();assert.deepEqual(h.navigation,['/observations']);assert.equal(h.calls.length,0);
 const draft=formHarness({entryId:null});draft.mount();await settle();ready(draft);draft.run('cancel()');await settle();assert.equal(draft.run('leave'),true);await draft.run('discard()');assert.equal(draft.calls.length,0);
 const saved=formHarness({entryId:null});saved.mount();await settle();ready(saved);await saved.run('save()');assert.equal(saved.calls[0][1],null);assert.deepEqual(saved.navigation,['/observations/saved-list']);
 const failed=formHarness({entryId:null,reject:true});failed.mount();await settle();ready(failed);await failed.run('save()');assert.equal(failed.run('rows[0].count'),'6');assert.equal(failed.navigation.length,0);
 assert.equal(birds.birdHref(null,'one'),'/observations/one');assert.equal(birds.birdHref('journal-2','one'),'/journal/entries/journal-2/birds/one');
});
test('standalone editing returns to the same independent detail on save or cancel',async()=>{
 const list={id:'existing',editToken:'a'.repeat(64),visitDate:'2026-11-12',visitStart:null,visitEnd:null,place,rows:[{...species,speciesId:species.id,taxonomyVersion:'AviList v2026',count:'3',comment:'Keep'}]};
 for(const save of [true,false]){const h=formHarness({entryId:null,editId:'existing',editList:list});h.mount();await settle();assert.equal(h.run('origin'),'/observations/existing');if(save){h.run('valid=true');await h.run('save()');assert.equal(h.calls[0][1],null);}else{h.run('cancel()');await settle();assert.equal(h.calls.length,0);}assert.deepEqual(h.navigation,['/observations/existing']);}
});
test('link response loss is confirmed read-only; unlink never sends a list deletion',async()=>{
 const code=p=>ts.transpileModule(source(p).replace(/^import .*;\n/gm,'').replace(/^export /gm,''),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
 let linked=false,stale=false;const commands=[];
 const context=vm.createContext({invoke:async(command,args)=>{
  commands.push({command,args});if(command==='set_journal_bird_link'){if(stale)throw Error('stale generation');linked=args.linked;throw Error('lost response');}
  if(command==='read_journal_bird_lists')return {generation:7,data:linked?[{id:'one'}]:[]};if(command==='new_journal_session')return {generation:7,data:null};throw Error(command);
 },Error});
 vm.runInContext(code('../src/lib/journal-data.ts')+'\n'+code('../src/lib/birds.ts'),context);
 const change=value=>vm.runInContext(`setBirdLink(7,'origin','one',${value})`,context);
 await change(true);await change(true);await change(false);assert.equal(linked,false);assert.equal(commands.filter(c=>c.command==='set_journal_bird_link').length,3);assert.equal(commands.some(c=>c.command.includes('delete')),false);
 stale=true;await assert.rejects(()=>change(true),/stale generation/);
});


test('overview shows each provided list once, dates and visits, and always opens standalone context',async()=>{
 const data=['unlinked','shared'].map((id,i)=>({id,place,visitDate:'2026-11-13',visitStart:i?'18:07':null,visitEnd:null,rows:[{...species,count:'4'}]}));
 const text=source('../src/routes/observations/+page.svelte').replace('available=$state<boolean|null>(null)','available=$state<boolean|null>(true)').replace('lists=$state<BirdList[]>([])',`lists=$state<BirdList[]>(${JSON.stringify(data)})`).replace('loading=$state(true)','loading=$state(false)');
 for(const locale of ['sv','en']){const {default:component}=await import(overviewUrl(text,locale));const html=render(component).body;assert.equal((html.match(/class="bird-preview/g)||[]).length,2);assert.ok(html.includes('href="/observations/unlinked"'));assert.ok(html.includes('href="/observations/shared"'));assert.match(html,/18:07/);assert.match(html,/november|November/);assert.ok(html.includes('/observations/new'));}
 const unavailable=source('../src/routes/observations/+page.svelte').replace('available=$state<boolean|null>(null)','available=$state<boolean|null>(false)');const {default:component}=await import(overviewUrl(unavailable));assert.doesNotMatch(render(component).body,/href="\/observations\/new"/);
 const nav=source('../src/lib/components/AppNavigation.svelte');assert.match(nav,/invoke<boolean>\("bird_observations_available"\)/);assert.match(nav,/\{#if birdsAvailable\}[\s\S]*?href="\/observations"/);
});
test('standalone detail keeps the overview return context even when the list has journal links',async()=>{
 const {default:component}=await import(detailUrl());const html=render(component,{props:{list:{id:'visit',place,visitDate:'2026-11-13',rows:[{...species,count:'1',comment:null}]},origin:'/observations',standalone:true,locale:'sv',preferences:{showOtherLanguage:false,showScientific:false},onPreferencesChange:()=>{}}}).body;
 assert.match(html,/href="\/observations"/);assert.match(html,/Observationer/);assert.doesNotMatch(html,/Tillbaka till anteckningen/);
});
function linkPickerHarness({reject=false}={}){
 const mounts=[],calls=[],events=[],focus=[];let props;
 const context=vm.createContext({$props:()=>props,$state:v=>v,onMount:fn=>mounts.push(fn),makeBirdReader:()=>async()=>({generation:7,data:[{id:'linked'},{id:'free'}]}),setBirdLink:async(...args)=>{calls.push(args);if(reject)throw Error('stale');}});
 props={entryId:'origin',generation:7,linkedIds:['linked'],locale:'sv',oncancel:()=>events.push('cancel'),onlinked:()=>events.push('linked')};
 const script=source('../src/lib/components/BirdLinkPicker.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {run:s=>vm.runInContext(s,context),mount:()=>mounts[0](),calls,events,focus,props};
}
test('link picker cancellation writes nothing, existing links cannot duplicate and errors retain the picker',async()=>{
 const h=linkPickerHarness();h.mount();await settle();h.props.oncancel();assert.deepEqual(h.events,['cancel']);assert.equal(h.calls.length,0);
 await h.run('link({id:"linked"})');assert.equal(h.calls.length,0);await h.run('link({id:"free"})');assert.deepEqual(h.calls,[[7,'origin','free',true]]);assert.deepEqual(h.events,['cancel','linked']);
 const fail=linkPickerHarness({reject:true});fail.mount();await settle();await fail.run('link({id:"free"})');assert.equal(fail.run('saveError'),true);assert.equal(fail.run('lists.length'),2);assert.deepEqual(fail.events,[]);
 const sourceText=source('../src/lib/components/BirdLinkPicker.svelte');assert.match(sourceText,/cancelButton\?\.focus\(\)/);assert.match(sourceText,/oncancel=\{event=>\{event.preventDefault\(\);if\(!busy\)oncancel\(\);\}\}/);
});
test('unlink cancellation does not write; confirmation changes only the selected journal link and refreshes previews',async()=>{
 const calls=[],events=[];let reject=false;
 const context=vm.createContext({$props:()=>({entryId:'origin',generation:7,locale:'sv',allowCreate:true}),$state:v=>v,$effect:()=>{},onMount:()=>{},makeBirdReader:()=>()=>{},setBirdLink:async(...args)=>{calls.push(args);if(reject)throw Error('failed');},window:{dispatchEvent:e=>events.push(e.type)},Event});
 const script=source('../src/lib/components/BirdJournalLists.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 const run=s=>vm.runInContext(s,context);await run('unlink()');assert.equal(calls.length,0);run('unlinkTarget={id:"second"};unlinkTarget=null');await run('unlink()');assert.equal(calls.length,0);
 run('unlinkTarget={id:"second"}');await run('unlink()');assert.deepEqual(calls,[[7,'origin','second',false]]);assert.deepEqual(events,['mno:journal-changed']);assert.equal(run('unlinkTarget'),null);
 reject=true;run('unlinkTarget={id:"first"}');await run('unlink()');assert.equal(run('unlinkFailed'),true);assert.equal(events.length,1);
});

test('deleting from standalone detail returns to overview, regardless of journal links',async()=>{
 const h=deletionHarness({entryId:null});h.mount();await settle();h.run('requestDeletion()');await h.run('confirmDeletion()');assert.deepEqual(h.navigation,['/observations']);assert.deepEqual(h.calls,[[7,'visit','a'.repeat(64)]]);assert.equal(h.run('origin'),'/observations');
});


test('date grouping uses newest days, ascending starts, untimed last and binary identity ties without mutation',()=>{
 const visit=(id,date,start)=>({id,visitDate:date,visitStart:start,visitEnd:null,place,rows:[]});
 const data=[visit('z','2026-11-13',null),visit('c','2026-11-13','08:15'),visit('d','2026-11-12','00:01'),visit('b','2026-11-13','18:07'),visit('a','2026-11-13','08:15')];
 const before=JSON.stringify(data),groups=birds.groupBirdVisits(data);assert.deepEqual(groups.map(g=>g.date),['2026-11-13','2026-11-12']);assert.deepEqual(groups[0].visits.map(v=>v.id),['a','c','b','z']);assert.equal(groups[0].visits[3].visitStart,null);assert.equal(JSON.stringify(data),before);
});
test('hour and minute typing keeps partial values invalid, colon advances, and clearing both is optional',()=>{
 const h=timeHarness('');h.run('typePart("hour",{currentTarget:{value:"08"}})');assert.equal(h.run('value'),'08:');assert.equal(h.run('validTime(value)'),false);
 h.run('typePart("minute",{currentTarget:{value:"07"}})');assert.equal(h.run('value'),'08:07');
 h.run('typePart("hour",{currentTarget:{value:""}})');assert.equal(h.run('value'),':07');assert.equal(h.run('validTime(value)'),false);h.run('typePart("minute",{currentTarget:{value:""}})');assert.equal(h.run('value'),'');
 h.run('minuteInput={focus:focusTrigger,select(){}};hourKeydown({key:":",preventDefault(){}})');assert.equal(h.triggerFocused(),1);
 h.run('pasteTime({clipboardData:{getData:()=>"23:59"},preventDefault(){}})');assert.equal(h.run('value'),'23:59');
 const text=source('../src/lib/components/VisitTimePicker.svelte');assert.doesNotMatch(text,/event.key==="Tab"/);assert.ok(text.indexOf('class="hour-entry"')<text.indexOf('class="minute-entry"'));
});

function journalLinksHarness({reject=false,refreshReject=false}={}){
 const mounts=[],calls=[],events=[];let changed=0;let data=[{id:'a',entryDate:'2026-11-13',title:'A',preview:'',linked:true},{id:'b',entryDate:'2026-11-12',title:'',preview:'Useful text',linked:false}];
 const context=vm.createContext({$props:()=>({listId:'visit',generation:7,locale:'sv',onchanged:async()=>{changed++;if(refreshReject)throw Error('refresh');},onbusy:()=>{}}),$state:v=>v,onMount:fn=>mounts.push(fn),makeBirdReader:()=>async()=>({generation:7,data:JSON.parse(JSON.stringify(data))}),setBirdLink:async(...args)=>{calls.push(args);if(reject)throw Error('stale');data=data.map(e=>e.id===args[1]?{...e,linked:args[3]}:e);},window:{dispatchEvent:e=>events.push(e.type)},Event});
 const script=source('../src/lib/components/BirdJournalLinks.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {run:s=>vm.runInContext(s,context),mount:()=>mounts[0](),calls,events,changed:()=>changed};
}
test('detail links choose an exact entry, prevent duplicates and unlink only that entry',async()=>{
 const h=journalLinksHarness();h.mount();await settle();h.run('choosing=true;cancelChoice()');assert.equal(h.calls.length,0);assert.equal(h.run('choosing'),false);
 await h.run('change(entries[0],true)');assert.equal(h.calls.length,0);await h.run('change(entries[1],true)');assert.deepEqual(h.calls,[[7,'b','visit',true]]);assert.equal(h.run('entries.filter(e=>e.linked).length'),2);
 h.run('requestUnlink(entries[0]);cancelUnlink()');await h.run('confirmUnlink()');assert.equal(h.calls.length,1);
 h.run('requestUnlink(entries[0])');await h.run('confirmUnlink()');assert.deepEqual(h.calls[1],[7,'a','visit',false]);assert.equal(h.run('entries.find(e=>e.id==="b").linked'),true);assert.equal(h.run('entries.find(e=>e.id==="a").linked'),false);assert.equal(h.changed(),2);assert.deepEqual(h.events,['mno:journal-changed','mno:journal-changed']);
 const fail=journalLinksHarness({reject:true});fail.mount();await settle();await fail.run('change(entries[1],true)');assert.equal(fail.run('saveError'),true);assert.equal(fail.run('entries[0].linked'),true);assert.equal(fail.changed(),0);
 assert.equal(birds.birdJournalLabel({title:'',preview:'Useful text'},'Untitled'),'Useful text');assert.equal(birds.birdJournalLabel({title:'Named',preview:'Other'},'Untitled'),'Named');
});
test('link refresh preserves both return contexts and reloads independent observation access',async()=>{
 for(const entryId of ['origin',null]){const h=deletionHarness({entryId});h.mount();await settle();await h.run('linksChanged()');assert.equal(h.run('origin'),entryId?'/journal/entries/origin':'/observations');assert.equal(h.run('linksRefreshFailed'),false);assert.equal(h.navigation.length,0);}
 const h=formHarness({editId:'existing'});ready(h);await h.run('save()');assert.equal(h.calls[0][1],null);assert.deepEqual(h.navigation,['/journal/entries/origin/birds/existing']);
});

test('detail journal chooser renders localized dates, title or safe preview, and targeted unlink confirmation',async()=>{
 const entries=[{id:'a',entryDate:'2026-11-13',title:'Morning birds',preview:'Text',linked:true},{id:'b',entryDate:'2026-11-12',title:'',preview:'Useful <birds>',linked:false}];
 for(const locale of ['sv','en']){
  let text=source('../src/lib/components/BirdJournalLinks.svelte').replace('entries=$state<BirdJournalEntry[]>([])',`entries=$state<BirdJournalEntry[]>(${JSON.stringify(entries)})`).replace('loading=$state(true)','loading=$state(false)').replace('choosing=$state(false)','choosing=$state(true)');
  let {default:component}=await import(journalLinksUrl(text));let html=render(component,{props:{listId:'visit',generation:7,locale,onchanged:async()=>{}}}).body;
  assert.match(html,/href="\/journal\/entries\/a"/);assert.match(html,/Morning birds/);assert.match(html,/Useful &lt;birds>/);assert.doesNotMatch(html,/<birds>/);assert.match(html,/november|November/);assert.match(html,/aria-labelledby="bird-journal-chooser-heading"/);assert.match(html,locale==='sv'?/Koppla till journalanteckning/:/Link to journal entry/);assert.match(html,locale==='sv'?/Redan kopplad/:/Already linked/);
  text=text.replace('choosing=$state(true)','choosing=$state(false)').replace('unlinkTarget=$state<BirdJournalEntry|null>(null)',`unlinkTarget=$state<BirdJournalEntry|null>(${JSON.stringify(entries[0])})`);({default:component}=await import(journalLinksUrl(text)));html=render(component,{props:{listId:'visit',generation:7,locale,onchanged:async()=>{}}}).body;
  assert.match(html,locale==='sv'?/Bara denna koppling tas bort/:/Only this link is removed/);assert.match(html,/Morning birds/);assert.match(html,locale==='sv'?/alla andra kopplingar finns kvar/:/all other links are kept/);
 }
});
test('after link changes a refreshed token is used and failed refresh blocks destructive actions',async()=>{
 const h=deletionHarness({refreshToken:'b'.repeat(64)});h.mount();await settle();await h.run('linksChanged()');assert.equal(h.run('list.editToken'),'b'.repeat(64));h.run('requestDeletion()');await h.run('confirmDeletion()');assert.equal(h.calls[0][2],'b'.repeat(64));
 const fail=deletionHarness({refreshFails:true});fail.mount();await settle();await fail.run('linksChanged()');assert.equal(fail.run('linksRefreshFailed'),true);fail.run('requestDeletion()');assert.equal(fail.run('deleteDialog'),false);assert.equal(fail.calls.length,0);
});

// The shared production form serves both create and edit; paste only changes its draft.
test('paste apply and cancel preserve create/edit drafts until normal guarded Save',async()=>{
 for(const editId of [null,'existing']){
  const h=formHarness({editId});ready(h);const before=h.run('JSON.stringify(rows)');
  h.run('pasteOpen=true');await h.run('save()');assert.equal(h.calls.length,0);
  h.run('pasteOpen=false');assert.equal(h.run('JSON.stringify(rows)'),before);
  assert.equal(h.run(`applyPaste([{...rows[0],count:'9',comment:'kept'}],JSON.stringify(rows))`),true);
  assert.equal(h.run('rows[0].count'),'9');assert.equal(h.run('dirty'),true);assert.equal(h.calls.length,0);
  assert.equal(h.run(`applyPaste([],${JSON.stringify(before)})`),false);assert.equal(h.run('rows.length'),1);
  await h.run('save()');assert.equal(h.calls.length,1);
 }
});

test('pending photo work blocks normal save and cancelling an attachment edit never writes',async()=>{
 for(const editId of [null,'existing']){
  const h=formHarness({editId});ready(h);h.run('photoPending=1');await h.run('save()');assert.equal(h.calls.length,0);h.run('photoPending=0');
  h.run('rows[0].rowId="stored-row";rows[0].photo={asset_id:"stored-photo",preview:"AQID",caption:"Stored caption"};const storedPhoto=JSON.stringify(rows[0].photo);rows[0].photo=null;dirty=true;cancel()');assert.equal(h.run('leave'),true);assert.equal(h.calls.length,0);await h.run('discard()');assert.equal(h.calls.length,0);
 }
});
