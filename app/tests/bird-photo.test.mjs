import test from 'node:test';import assert from 'node:assert/strict';import vm from 'node:vm';import {render} from 'svelte/server';
import {source,transpile,photo,photoViewUrl,rowPhotoUrl,birds} from './bird-test-modules.mjs';
const original={caption:'Quiet light',row_id:'row',asset_id:'old',original_path:'/synthetic/original.png',mime:'image/png',width:8,height:6,preview:'AQID'};
function harness({cancel=false,fail=false,wait=null,latest=7}={}){
 const row={id:'avibase-test',english:'Species',swedish:'Art',scientific:'Genus species',count:'4',comment:'Preserve',rowId:'row',photo:{...original}};const calls=[],mounts=[],pending=[];let changes=0;
 const context=vm.createContext({$props:()=>({row,generation:7,locale:'en',disabled:false,onchange:()=>changes++,onpending:n=>pending.push(n)}),$state:x=>x,tick:async()=>{},onMount:f=>mounts.push(f),birdPhotoTask:photo.birdPhotoTask,crypto:{randomUUID:()=> 'new-row'},Error,
 invoke:async(command,args)=>{calls.push([command,args]);if(command==='select_journal_photograph')return cancel?null:'/synthetic/new.png';if(command==='prepare_bird_photograph'){if(wait)await wait;if(fail)throw Error('invalid photo');return {...original,row_id:args.rowId,asset_id:'new'};}if(command==='new_journal_session')return {generation:latest};throw Error(command);}});
 const script=source('../src/lib/components/BirdRowPhoto.svelte').match(/<script lang="ts">([^]*?)<\/script>/)[1].replace(/^\s*import .*;\n/gm,'');vm.runInContext(transpile(script),context);const run=s=>vm.runInContext(s,context);const dispose=mounts[0]();return {row,calls,pending,run,dispose,changes:()=>changes};
}
test('photo replacement and removal update only the draft, keeping comments/count/row identity',async()=>{
 const h=harness();await h.run('choose()');assert.equal(h.row.photo.asset_id,'new');assert.equal(h.row.count,'4');assert.equal(h.row.comment,'Preserve');assert.equal(h.row.rowId,'row');assert.equal(h.changes(),1);assert.deepEqual(h.pending,[1,-1]);
 h.run('remove()');assert.equal(h.row.photo,null);assert.equal(h.changes(),2);assert.deepEqual(h.calls.map(c=>c[0]),['select_journal_photograph','prepare_bird_photograph','new_journal_session']);
});
test('cancelled selection and failed preparation retain the existing attachment',async()=>{
 for(const option of [{cancel:true},{fail:true}]){const h=harness(option);await h.run('choose()');assert.deepEqual(h.row.photo,original);assert.equal(h.changes(),0);assert.deepEqual(h.pending,[1,-1]);assert.equal(h.run('error'),!!option.fail);}
});
test('late photo replies cannot restore removed attachments or mutate removed/replaced rows or generations',async()=>{
 for(const boundary of ['remove','dispose','replace','generation']){
  let release;const wait=new Promise(r=>release=r);const h=harness({wait});const result=h.run('choose()');await Promise.resolve();await Promise.resolve();
  if(boundary==='remove')h.run('remove()');else if(boundary==='dispose')h.dispose();else if(boundary==='replace')h.run('row={...row,photo:null}');else h.run('generation=8');
  release();await result;assert.notEqual(h.row.photo?.asset_id,'new');assert.equal(h.changes(),boundary==='remove'?1:0);assert.equal(h.pending.reduce((a,b)=>a+b,0),0);
 }
 const changed=harness({latest:8});await changed.run('choose()');assert.deepEqual(changed.row.photo,original);assert.equal(changed.run('error'),true);assert.equal(changed.changes(),0);
});
test('saved photo renders its stored preview without consulting the original file',async()=>{
 const {default:component}=await import(photoViewUrl);for(const locale of ['sv','en']){const html=render(component,{props:{photo:original,species:'Skata',locale}}).body;assert.match(html,/data:image\/png;base64,AQID/);assert.match(html,/width="8"/);assert.match(html,/Skata/);assert.doesNotMatch(html,/src="\/synthetic/);assert.match(html,locale==='sv'?/Originalfilen ändras inte/:/original file is not changed/);}
 assert.equal(photo.birdPhotoUrl(original),'data:image/png;base64,AQID');
});

test('caption edits remain draft-only, replacement retains the caption and removal clears ownership',async()=>{
 const h=harness();h.run('setCaption("New caption")');assert.equal(h.row.photo.caption,'New caption');assert.equal(original.caption,'Quiet light');assert.equal(h.calls.length,0);await h.run('choose()');assert.equal(h.row.photo.caption,'New caption');h.run('remove()');assert.equal(h.row.photo,null);assert.equal(h.row.comment,'Preserve');
 assert.equal(birds.validBirdPhotoCaption('å'.repeat(4000)),true);assert.equal(birds.validBirdPhotoCaption('a'.repeat(4001)),false);assert.equal(birds.validBirdPhotoCaption('bad\\0'),true);assert.equal(birds.validBirdPhotoCaption(String.fromCharCode(0)),false);
});

test('attached-photo caption field is localized, labelled and distinct from the row comment',async()=>{
 const {default:component}=await import(rowPhotoUrl);
 for(const locale of ['sv','en']){const html=render(component,{props:{row:{id:'taxon',rowId:'row',english:'Species',swedish:'Art',scientific:'Genus species',count:'4',comment:'Separate comment',photo:original},generation:7,locale,onchange:()=>{},onpending:()=>{}}}).body;assert.match(html,/for="bird-caption-row"/);assert.match(html,/id="bird-caption-row"/);assert.match(html,/Quiet light/);assert.match(html,locale==='sv'?/Bildtext/:/Caption/);assert.doesNotMatch(html,/Separate comment/);const image=html.indexOf('<img '),actions=html.indexOf('class="photo-actions'),caption=html.indexOf('<textarea '),about=html.indexOf('<details ');assert.ok(image<actions&&actions<caption&&caption<about,'preview → photo actions → caption → About matches keyboard order');assert.equal((html.match(/<details /g)||[]).length,1);}
});
