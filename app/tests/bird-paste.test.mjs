import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import {render} from 'svelte/server';
import {paste as p,birds,names,source,transpile,pasteReviewUrl} from './bird-test-modules.mjs';
const tit={id:'tit',swedish:'talgoxe',english:'Great Tit',scientific:'Parus major'};
const blue={id:'blue',swedish:'blåmes',english:'Eurasian Blue Tit',scientific:'Cyanistes caeruleus'};
const review=(text,matches)=>p.prepareBirdPaste(p.parseBirdPaste(text).rows,matches);
test('LF/CRLF parsing preserves originals and supports optional st without changing invalid counts',()=>{
 for(const sep of ['\n','\r\n','\r']){
  const parsed=p.parseBirdPaste([' Talgoxe 6','Blåmes 4st','Nötväcka 1 st','Great Tit 2','Parus major 3'].join(sep));
  assert.deepEqual(parsed.rows.map(r=>r.count),['6','4','1','2','3']);assert.equal(parsed.rows[0].original,' Talgoxe 6');assert.equal(parsed.rows[4].name,'Parus major');
 }
 for(const count of ['0','-1','1.5','1,5','01','9223372036854775808','fast']){
  const rows=review(`Talgoxe ${count}`,[[tit]]);assert.equal(rows[0].count,count);assert.equal(p.planBirdPaste([],rows).checks[0].issue,'count');
 }
 assert.equal(p.parseBirdPaste('Talgoxe').rows[0].count,'');
 assert.equal(p.parseBirdPaste('x'.repeat(65537)).limited,true);assert.equal(p.parseBirdPaste(Array(201).fill('Talgoxe 1').join('\n')).limited,true);
 assert.deepEqual(p.pasteLookupNames([{name:'å'.repeat(101)}]),['']);
});
test('only unique exact non-referral candidates are interpreted automatically',()=>{
 const rows=review('Talgoxe 1\nUnknown 2\nShared 3\nGråkråka 4',[[tit],[],[tit,blue],[{...tit,referral:'gråkråka'}]]);
 assert.equal(rows[0].selected.id,'tit');for(const r of rows.slice(1))assert.equal(r.selected,null);
 assert.equal(p.planBirdPaste([],rows).canApply,false);assert.throws(()=>p.prepareBirdPaste([{}],[]));
});
test('duplicate decisions are explicit, retain comments and legacy rows, and never mutate input',()=>{
 const existing=[{...tit,count:'6',comment:'original',taxonomyVersion:'old'},{...tit,count:'2',comment:'legacy'}];const snapshot=JSON.stringify(existing);
 const rows=review('Talgoxe 3\nBlåmes 4\nBlåmes 2',[[tit],[blue],[blue]]);
 let plan=p.planBirdPaste(existing,rows);assert.equal(plan.canApply,false);assert.equal(plan.checks[0].targets.length,2);
 rows[0].combineInto='existing:0';rows[2].combineInto='paste:2';plan=p.planBirdPaste(existing,rows);assert.equal(plan.canApply,true);
 const applied=p.applyBirdPaste(existing,rows);assert.deepEqual(applied.map(r=>r.count),['9','2','6']);assert.equal(applied[0].comment,'original');assert.equal(applied[0].taxonomyVersion,'old');assert.equal(applied[1].comment,'legacy');assert.equal(JSON.stringify(existing),snapshot);
 rows[0].excluded=true;assert.equal(p.applyBirdPaste(existing,rows)[0].count,'6');
 rows[1].excluded=true;assert.equal(p.planBirdPaste(existing,rows).canApply,false); // stale target cannot silently become a new row
 rows[2].combineInto='';assert.equal(p.planBirdPaste(existing,rows).canApply,true);
});
test('overflow, invalid targets and all-excluded reviews cannot apply or lose data',()=>{
 const rows=review('Talgoxe 1',[[tit]]);rows[0].combineInto='existing:0';
 assert.equal(p.planBirdPaste([{...tit,count:'9223372036854775807'}],rows).checks[0].issue,'overflow');
 assert.equal(p.planBirdPaste([{...tit,count:'0'}],rows).checks[0].issue,'target');rows[0].excluded=true;assert.equal(p.planBirdPaste([],rows).canApply,false);assert.throws(()=>p.applyBirdPaste([],rows));
});
function harness({reject=false}={}){
 const mounts=[],calls=[],applied=[];let cancelled=0;const context=vm.createContext({$props:()=>({existingRows:[],locale:'sv',onapply:(rows)=>{applied.push(rows);return true;},oncancel:()=>cancelled++}),$state:x=>x,$derived:x=>x,onMount:fn=>mounts.push(fn),tick:async()=>{},invoke:async(command,args)=>{calls.push([command,args]);if(reject)throw Error('catalog unavailable');return [[tit]];},...p,speciesLabel:birds.speciesLabel,capitalizeCommonName:names.capitalizeCommonName,JSON,Error});
 const script=source('../src/lib/components/BirdPasteReview.svelte').match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'').replace('const plan=$derived','let plan=$derived').replace('const stale=$derived','let stale=$derived');
 vm.runInContext(transpile(script),context);const run=s=>vm.runInContext(s,context);mounts[0]();return {run,calls,applied,cancelled:()=>cancelled};
}
test('real review controller resolves without writes, cancel keeps draft, explicit apply runs once',async()=>{
 const h=harness();h.run("source='Talgoxe 6'");await h.run('interpret()');assert.equal(h.run('stage'),'review');assert.equal(h.calls[0][0],'match_bird_paste_names');assert.equal(h.applied.length,0);
 h.run('oncancel()');assert.equal(h.cancelled(),1);assert.equal(h.applied.length,0);
 const x=harness();x.run("source='Talgoxe 6'");await x.run('interpret()');x.run('plan=planBirdPaste(existingRows,review);apply();apply()');assert.equal(x.applied.length,1);assert.equal(x.applied[0][0].count,'6');assert.equal(x.calls.length,1);
 const f=harness({reject:true});f.run("source='Talgoxe 6'");await f.run('interpret()');assert.equal(f.run('loadError'),true);assert.equal(f.run('resolving'),false);assert.equal(f.run('source'),'Talgoxe 6');assert.equal(f.applied.length,0);
});
test('review renders original, localized corrections, explicit duplicate decision and accessible errors',async()=>{
 const rows=review('Talgoxe 6\nUnknown 0',[[tit],[]]);
 const text=source('../src/lib/components/BirdPasteReview.svelte').replace('review=$state<PasteRow[]>([])',`review=$state<PasteRow[]>(${JSON.stringify(rows)})`).replace('stage=$state<"input"|"review">("input")','stage=$state<"input"|"review">("review")');
 const {default:component}=await import(pasteReviewUrl(text));
 for(const locale of ['sv','en']){
  const html=render(component,{props:{existingRows:[{...tit,count:'2',comment:'kept'}],locale,onapply:()=>true,oncancel:()=>{}}}).body;
  assert.match(html,/Talgoxe 6/);assert.match(html,/Unknown 0/);assert.match(html,/aria-invalid="true"/);assert.match(html,/value="existing:0"/);assert.match(html,locale==='sv'?/Granska/:/Review/);assert.doesNotMatch(html,/<form/);
 }
});

test('paste count copy distinguishes missing/nonnumeric input without changing validation',()=>{
 for(const count of ['', ' ', 'abc','six','NaN','Infinity'])assert.equal(p.pasteCountError(count),'required',count);
 for(const count of ['0','-1','1.5','1,5','0.5','01','9223372036854775808']){
  assert.equal(p.pasteCountError(count),'minimum',count);
  assert.equal(p.planBirdPaste([],review(`Talgoxe ${count}`,[[tit]])).canApply,false,count);
 }
 for(const count of ['1','6','9223372036854775807'])assert.equal(p.pasteCountError(count),null,count);
});

test('localized paste errors sit in the relevant field and remain associated with its control',async()=>{
 for(const locale of ['sv','en']){
  for(const [count,expected] of [['','required'],['abc','required'],['0','minimum'],['-2','minimum'],['1.5','minimum']]){
   const rows=[{...review('Unknown 1',[[]])[0],count}];
   const text=source('../src/lib/components/BirdPasteReview.svelte').replace('review=$state<PasteRow[]>([])',`review=$state<PasteRow[]>(${JSON.stringify(rows)})`).replace('stage=$state<"input"|"review">("input")','stage=$state<"input"|"review">("review")');
   const {default:component}=await import(pasteReviewUrl(text));
   const html=render(component,{props:{existingRows:[],locale,onapply:()=>true,oncancel:()=>{}}}).body;
   const countField=html.match(/<label class="count[^]*?<\/label>/)?.[0];
   const speciesField=html.match(/<label class="species-choice[^]*?<\/label>/)?.[0];
   assert.ok(countField);assert.ok(speciesField);
   assert.match(countField,/aria-describedby="paste-count-help-1"/);assert.match(countField,/id="paste-count-help-1"/);assert.match(countField,/aria-invalid="true"/);
   assert.ok(countField.includes(locale==='sv'?(expected==='required'?'Ange ett antal.':'Ange minst 1, utan decimaler.'):(expected==='required'?'Enter a count.':'Enter at least 1, without decimals.')));
   assert.match(speciesField,/aria-describedby="paste-species-help-1"/);assert.match(speciesField,/id="paste-species-help-1"/);
   assert.ok(speciesField.includes(locale==='sv'?'Välj en art eller uteslut raden.':'Choose a species or exclude the row.'));
  }
 }
});
