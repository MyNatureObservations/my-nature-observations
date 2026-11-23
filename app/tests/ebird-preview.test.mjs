import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
import {compile} from 'svelte/compiler';
const read=p=>readFileSync(new URL(p,import.meta.url),'utf8');
const code=ts.transpileModule(read('../src/lib/ebird-preview.ts'),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext}}).outputText;
const api=await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const url=code=>'data:text/javascript;base64,'+Buffer.from(code).toString('base64');
const namesCode=ts.transpileModule(read('../src/lib/bird-names.ts'),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext}}).outputText;
const presentationCode=ts.transpileModule(read('../src/lib/ebird-presentation.ts'),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext}}).outputText.replace("'$lib/ebird-preview'",JSON.stringify(url(code))).replace("'$lib/bird-names'",JSON.stringify(url(namesCode)));
const presentation=await import(url(presentationCode));
const fixture=read('./fixtures/ebird/full.csv');
const species={id:'avibase-test',scientific:'Parus major',swedish:'talgoxe',english:'Great Tit'};
test('BOM, quoting, escaped quotes, CRLF, omitted optional fields and grouping preserve source data',()=>{
 const p=api.parseEbird('\ufeff'+fixture.replaceAll('\n','\r\n'));
 assert.deepEqual(p.issues,[]);assert.equal(p.visits.length,2);assert.equal(p.rows.length,3);assert.equal(p.places,2);
 assert.equal(p.unspecified,1);assert.equal(p.media,2);assert.equal(p.rows[1].count,null);assert.equal(p.rows[0].count,'6');
 assert.equal(p.visits[0].fields.Location,'Synthetic, lake');assert.equal(p.rows[0].fields['Checklist Comments'],'A "quoted" note');
 assert.equal(p.visits[0].rows.length,2);assert.notEqual(p.visits[0].fields['Location ID'],p.visits[1].fields['Location ID']);
 assert.deepEqual(p.rows[0].media,['10001','10002']);assert.equal(p.rows[1].fields['ML Catalog Numbers'],'');
});
test('quoted multiline content keeps correct physical line diagnostics',()=>{
 const records=api.csvRecords('a,b\r\n"line1\r\nline2",x\r\nz,q');assert.equal(records[1].values[0],'line1\r\nline2');assert.equal(records[2].line,4);
 for(const text of ['a,b\n"unterminated,x','a,b\na"b,c','a,b\n"a"junk,c'])assert.equal(api.parseEbird(text).issues[0].code,'csv');
});
test('single-checklist report is unsupported rather than guessed from filename',()=>{
 const p=api.parseEbird(read('./fixtures/ebird/checklist.csv'));assert.equal(p.unsupported,true);assert.equal(p.visits.length,0);
});
test('invalid counts, dates, times, IDs and coordinates are reported without dropping rows',()=>{
 for(const count of ['0','-1','1.5','abc','01','9223372036854775808']){
  const p=api.parseEbird(fixture.replace(',6,L100',`,${count},L100`));assert.equal(p.rows.length,3);assert.ok(p.issues.some(i=>i.field==='Count'));assert.equal(p.rows[0].fields.Count,count);
 }
 for(const [from,to,field] of [['2026-11-01','2026-02-30','Date'],['8:15 AM','13:15 PM','Time'],[',59,18,',',NaN,18,','Latitude'],[',59,18,',',91,18,','Latitude'],['S100,Great','invalid,Great','Submission ID']]){
  assert.ok(api.parseEbird(fixture.replace(from,to)).issues.some(i=>i.field===field),field);
 }
 const integer=api.parseEbird(fixture.replace(',6,L100',',9223372036854775807,L100'));assert.equal(integer.rows[0].count,'9223372036854775807');
});
test('conflicting checklist metadata, duplicate taxa and unexpected fields remain visible',()=>{
 const p=api.parseEbird(fixture.replace('59.0,18.0','58,18'));assert.ok(p.issues.some(i=>i.code==='metadata'&&i.field==='Latitude'));assert.equal(p.rows.length,3);
 const precise=api.parseEbird(fixture.replace('59.0,18.0','59.000000000000000000001,18.0'));assert.ok(precise.issues.some(i=>i.code==='metadata'&&i.field==='Latitude'));
 const duplicate=api.parseEbird(fixture.replace('Aves sp.','Parus major'));assert.ok(duplicate.issues.some(i=>i.code==='duplicate'));assert.equal(duplicate.visits[0].rows.length,2);
 assert.ok(api.parseEbird(fixture.replace('10001 10002','10001,10002')).issues.some(i=>i.code==='csv'));
 assert.ok(api.parseEbird(fixture.replace('10001 10002','10001 BAD')).issues.some(i=>i.code==='media'));
 assert.ok(api.parseEbird(fixture.replace('Submission ID','Something else')).issues.some(i=>i.code==='header'));
});
test('catalog candidates require an exact scientific name and do not overwrite source identities',()=>{
 const p=api.parseEbird(fixture);const mapped=api.applyCatalogMatches(p,[[species],[{...species,scientific:'Aves'}]],['Parus major','Aves sp.']);
 assert.deepEqual(mapped.unmatched,['Aves sp.']);assert.equal(mapped.rows[1].fields['Scientific Name'],'Aves sp.');assert.equal(mapped.rows[1].catalogMatch,undefined);
 assert.equal(mapped.rows[0].catalogMatch.id,species.id);assert.equal(p.rows[0].catalogMatch,undefined);
 assert.equal(api.sourceTime('12:01 AM'),'00:01');assert.equal(api.sourceTime('12:01 PM'),'12:01');assert.equal(api.sourceTime(''), '');
 assert.equal(api.mediaHref('10001'),'https://macaulaylibrary.org/asset/10001');assert.throws(()=>api.mediaHref('../bad'));
});
const component=read('../src/lib/components/EbirdPreview.svelte');
function harness({enabled=true,result=fixture,fail=null,confirmFailure=false,status="committed"}={}){
 let mount;const calls=[],events=new EventTarget();
 const context=vm.createContext({$state:v=>v,$derived:v=>v,groupPreviewVisits:presentation.groupPreviewVisits,onMount:f=>mount=f,language:{current:'sv'},window:events,Event,beforeNavigate:()=>{},
  m:new Proxy({},{get:()=>()=>''}),parseEbird:api.parseEbird,applyCatalogMatches:api.applyCatalogMatches,mediaId:api.mediaId,mediaHref:api.mediaHref,
  invoke:async(command,args)=>{calls.push(command);if(command==='ebird_preview_available')return enabled;if(command==='preview_ebird_export'){if(fail)throw fail;return result;}if(command==='new_journal_session')return {generation:1};if(command==='review_ebird_import')return {operationId:'review-1',generation:1,changes:args.visits.map(v=>({submissionId:v.submission_id,kind:'new',fields:v.rows[0]}))};if(command==='confirm_ebird_import'){if(confirmFailure)throw Error('lost reply');return 'committed';}if(command==='ebird_import_status')return status;if(command==='cancel_ebird_import')return null;if(command==='match_bird_paste_names')return args.names.map(name=>name==='Parus major'?[species]:[]);throw Error(command);},openUrl:async()=>{}});
 const script=component.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');
 vm.runInContext(ts.transpileModule(script,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText,context);
 return {calls,run:s=>vm.runInContext(s,context),mount:()=>mount(),events};
}
const settle=async()=>{for(let i=0;i<20;i++)await Promise.resolve();};
test('real preview controller loads read-only, cancellation preserves preview and no write command is used',async()=>{
 const h=harness();h.mount();await settle();await h.run('choose()');assert.equal(h.run('preview.visits.length'),2);assert.equal(h.run('preview.unmatched.length'),1);
 assert.deepEqual([...new Set(h.calls)].sort(),['ebird_preview_available','match_bird_paste_names','preview_ebird_export','new_journal_session','review_ebird_import'].sort());
 const cancel=harness({result:null});cancel.mount();await settle();cancel.run('preview={sentinel:true}');await cancel.run('choose()');assert.equal(cancel.run('preview.sentinel'),true);assert.equal(cancel.run('busy'),false);
});
test('disabled route cannot select files, read errors are explicit and disposed results are ignored',async()=>{
 const off=harness({enabled:false});off.mount();await settle();await off.run('choose()');assert.deepEqual(off.calls,['ebird_preview_available']);
 const failed=harness({fail:'ebird_unreadable'});failed.mount();await settle();await failed.run('choose()');assert.equal(failed.run('error'),'ebird_unreadable');assert.equal(failed.run('preview'),null);
 const late=harness();const dispose=late.mount();await settle();const pending=late.run('choose()');dispose();await pending;assert.equal(late.run('preview'),null);
});
test('localized markup compiles with accessible native disclosure and explicit source-ownership copy',()=>{
 assert.equal(compile(component,{generate:'client'}).warnings.length,0);
 for(const locale of ['sv','en']){
  const m=JSON.parse(read(`../messages/${locale}.json`));for(const key of [...component.matchAll(/m\.(ebird_\w+)/g)].map(m=>m[1]))assert.ok(m[key],key);
  assert.match(m.ebird_validation_only,locale==='sv'?/skrivskyddade/:/read-only/);
 }
 assert.match(component,/<details class="visit"/);assert.match(component,/role="alert"/);assert.doesNotMatch(component,/create_journal|reserve_bird|set_journal|update_.*list/);
 const backend=read('../src-tauri/src/ebird_preview.rs');assert.match(backend,/target_os = "macos"/);assert.match(backend,/module.command\(\)\?/);assert.doesNotMatch(backend,/DatabaseState|ready_database_path|Uuid/);
});

test('parsed chronology uses newest date, ascending local time, missing last and checklist tie-breaker without mutation',()=>{
 const visit=(id,date,time)=>({id,fields:{Date:date,Time:time},rows:[]});
 const visits=[visit('S9','2026-10-31',''),visit('S4','2026-11-01',''),visit('S3','2026-11-01','1:00 PM'),visit('S2','2026-11-01','9:00 AM'),visit('S1','2026-11-01','9:00 AM'),visit('S0','2026-11-01','12:01 AM')];
 const original=JSON.stringify(visits),days=presentation.groupPreviewVisits(visits);
 assert.deepEqual(days.map(d=>d.date),['2026-11-01','2026-10-31']);assert.deepEqual(days[0].visits.map(v=>v.id),['S0','S1','S2','S3','S4']);assert.equal(JSON.stringify(visits),original);
 const invalid=presentation.groupPreviewVisits([visit('S8','bad','invalid'),...visits]);assert.equal(invalid.at(-1).visits[0].id,'S8');
});
test('localized calendar dates and clock times never shift with the host timezone',()=>{
 const prior=process.env.TZ;
 try{for(const zone of ['Pacific/Honolulu','Pacific/Kiritimati']){process.env.TZ=zone;assert.equal(presentation.formatSourceDate('2026-11-01','sv'),'1 november 2026');assert.equal(presentation.formatSourceDate('2026-11-01','en'),'1 November 2026');for(const locale of ['sv','en'])assert.equal(presentation.formatSourceTime('12:01 AM',locale),'00:01');}}finally{if(prior===undefined)delete process.env.TZ;else process.env.TZ=prior;}
});
test('compact preview retains first three source names and counts and reports remaining birds',()=>{
 const p=api.parseEbird(fixture),visit=p.visits[0];visit.rows.push({...visit.rows[0],line:99},{...visit.rows[0],line:100});
 const compact=presentation.compactSpecies(visit);assert.equal(compact.more,1);assert.equal(compact.rows.length,3);assert.equal(presentation.sourceSpeciesName(compact.rows[0]),'Great Tit');assert.ok(compact.rows.every(row=>row.fields.Count==='6'));assert.equal(visit.rows.length,4);
 assert.ok(component.indexOf('<ul class="rows">')<component.indexOf('<details class="source-details">'));
});

test('explicit confirm resolves a lost reply by status and never blindly reapplies',async()=>{
 const h=harness({confirmFailure:true});h.mount();await settle();await h.run('choose()');assert.ok(h.run('comparison'));
 assert.equal(h.calls.filter(c=>c==='confirm_ebird_import').length,0);await h.run('confirm()');assert.equal(h.run('saved'),true);assert.equal(h.calls.filter(c=>c==='confirm_ebird_import').length,1);assert.ok(h.calls.includes('ebird_import_status'));
 const uncertain=harness({confirmFailure:true,status:'unknown'});uncertain.mount();await settle();await uncertain.run('choose()');await uncertain.run('confirm()');await uncertain.run('confirm()');assert.equal(uncertain.calls.filter(c=>c==='confirm_ebird_import').length,1);assert.equal(uncertain.run('error'),'uncertain');assert.ok(uncertain.run('preview'));
 const cancel=harness();cancel.mount();await settle();await cancel.run('choose()');await cancel.run('cancelReview()');assert.equal(cancel.run('comparison'),null);assert.ok(!cancel.calls.includes('confirm_ebird_import'));
});
test('source detail has locked fields, safe media references and preserved inactive additions',()=>{
 const detail=read('../src/lib/components/EbirdListDetail.svelte');assert.equal(compile(detail,{generate:'client'}).warnings.length,0);assert.match(detail,/ebird_source_locked/);assert.match(detail,/source.unresolved/);assert.match(detail,/mediaId\(id\)/);assert.doesNotMatch(detail,/update_journal|delete_journal|<input|<textarea/);
 const form=read('../src/lib/components/BirdListForm.svelte');assert.match(form,/if\(list.source\)\{sourceLocked=true;return;\}/);
});

test('replacing a reviewed file invalidates its capability before parsing a new file',async()=>{
 const h=harness();h.mount();await settle();await h.run('choose()');await h.run('choose()');
 assert.equal(h.calls.filter(c=>c==='cancel_ebird_import').length,1);
 assert.ok(h.calls.lastIndexOf('cancel_ebird_import')<h.calls.lastIndexOf('review_ebird_import'));
 assert.ok(!h.calls.includes('confirm_ebird_import'));
});

test('Swedish source species sorting is case-insensitive, stable and presentation-only',()=>{
 const names=['Örn','Ärla','Åda','Zebra','skata','Skata','albatross'];
 const rows=names.map((name,i)=>({fields:{'Common Name':name,'Scientific Name':`Taxon ${i}`,Count:String(i+1),'Observation Details':`Comment ${i}`,'ML Catalog Numbers':String(100+i)},additions:[`Addition ${i}`]}));
 const before=JSON.stringify(rows),sorted=presentation.sortSourceSpecies(rows,'sv',presentation.sourceSpeciesName);
 assert.deepEqual(sorted.map(r=>r.fields['Common Name']),['albatross','skata','Skata','Zebra','Åda','Ärla','Örn']);
 assert.equal(sorted[1],rows[4]);assert.equal(sorted[2],rows[5]);assert.equal(JSON.stringify(rows),before);
 assert.equal(new Set(sorted).size,rows.length);for(const row of sorted)assert.ok(rows.includes(row));
 assert.deepEqual(presentation.sortSourceSpecies(rows,'en',presentation.sourceSpeciesName).map(r=>r.fields['Common Name']),['Åda','albatross','Ärla','Örn','skata','Skata','Zebra']);
 const visit={rows};assert.deepEqual(presentation.compactSpecies(visit,'sv').rows,sorted.slice(0,3));
 assert.equal(JSON.stringify(rows),before);
});
test('all eBird species surfaces use the same locale-aware display sort and import naming',()=>{
 assert.match(component,/compactSpecies\(visit,language.current\)/);assert.match(component,/sortSourceSpecies\(visit.rows,language.current,sourceSpeciesName\)/);
 const detail=read('../src/lib/components/EbirdListDetail.svelte');assert.match(detail,/sortSourceSpecies\(list.source.rows,locale,sourceSpeciesName\)/);assert.match(detail,/sortSourceSpecies\(list.source.unresolved,locale,sourceSpeciesName\)/);
 assert.match(read('../src/lib/components/BirdVisitPreview.svelte'),/list.source\?sortSourceSpecies\(list.rows,locale/);
 assert.equal(JSON.parse(read('../messages/sv.json')).ebird_title,'Importera från eBird');assert.equal(JSON.parse(read('../messages/en.json')).ebird_title,'Import from eBird');
});

test('observation actions share a wrapping row and general source explanations are secondary',()=>{
 const overview=read('../src/routes/observations/+page.svelte');
 const actions=overview.match(/<div class="observation-actions">([\s\S]*?)<\/div>/)[1];
 assert.match(actions,/href="\/observations\/new"/);assert.match(actions,/href="\/observations\/ebird-preview"/);assert.doesNotMatch(actions,/<p>/);assert.match(overview,/\.observation-actions\{[^}]*flex-wrap:wrap/);
 const detail=read('../src/lib/components/EbirdListDetail.svelte');const source=detail.indexOf('<details class="source-details">');
 assert.ok(detail.indexOf('m.ebird_source_place')>source);assert.ok(detail.indexOf('m.ebird_catalog_note')>source);
 for(const key of ['m.ebird_source_locked','m.ebird_from','m.ebird_original','m.ebird_review_additions','role="alert"'])assert.ok(detail.indexOf(key)<source);
 assert.equal(JSON.parse(read('../messages/sv.json')).ebird_source_locked,'Uppgifterna hämtas från eBird och ändras där.');
 assert.equal(JSON.parse(read('../messages/en.json')).ebird_source_locked,'These details come from eBird and are edited there.');
});
