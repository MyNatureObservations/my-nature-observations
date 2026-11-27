import assert from 'node:assert/strict';import test from 'node:test';import vm from 'node:vm';import {render} from 'svelte/server';
import {source,transpile,gallery,viewerUrl,previewUrl,detailUrl,names} from './bird-test-modules.mjs';
const photo=id=>({caption:'Bird caption',row_id:`row-${id}`,asset_id:id,original_path:'/missing/original.png',mime:'image/png',width:32,height:32,preview:'AQID'});
const list=(id,date,start=null)=>({id,editToken:id,visitDate:date,visitStart:start,visitEnd:null,place:{id:'place',name:'Lake',latitude:59,longitude:18},rows:[{speciesId:'species',taxonomyVersion:'AviList v2026',rowId:`row-${id}`,swedish:'skata',english:'Eurasian Magpie',scientific:'Pica pica',count:'4',comment:'Keep',photo:photo(id)}]});
test('gallery keeps journal document order, captions and distinct image nodes',()=>{
 const doc={type:'doc',content:[{type:'journalImage',attrs:{nodeId:'first',assetId:'same',caption:'First caption',alt:'Independent alt'}},{type:'paragraph',content:[{type:'text',text:'Do not change prose'}]},{type:'journalImage',attrs:{nodeId:'second',assetId:'same',caption:'Other caption'}}]};
 const json=JSON.stringify(doc);assert.deepEqual(gallery.journalPhotoRefs(json).map(p=>[p.nodeId,p.assetId,p.caption]),[['first','same','First caption'],['second','same','Other caption']]);assert.equal(JSON.stringify(doc),json);assert.deepEqual(gallery.journalPhotoRefs('bad JSON'),[]);
});
test('linked observation images are unique per visit and stable by day/time/id then stored row order',()=>{
 const old=list('old','2026-11-16');const early=list('early','2026-11-17','08:00');const late=list('late','2026-11-17','18:00');const untimed=list('untimed','2026-11-17');early.rows.push({...early.rows[0],rowId:'other',photo:photo('other')});
 const items=gallery.observationPhotos([late,old,untimed,early,early],'sv');assert.deepEqual(items.map(i=>i.key),['bird:early:row-early:early','bird:early:other:other','bird:late:row-late:late','bird:untimed:row-untimed:untimed','bird:old:row-old:old']);assert.match(items[0].description,/Skata · Lake · 17 november 2026/);assert.ok(items.every(i=>i.src.startsWith('data:image/png;base64,')));assert.ok(items.every(i=>!i.src.includes('original.png')));
 assert.equal(gallery.observationPhotos([old],'en').length,1);assert.equal(gallery.observationPhotos([old],'en')[0].alt,'Eurasian Magpie');
});
test('cancelled gallery work cannot publish after close or a different request',()=>{const task=gallery.photoGalleryRequest();const old=task.start();assert.equal(old(),true);task.invalidate();assert.equal(old(),false);const next=task.start();const latest=task.start();assert.equal(next(),false);assert.equal(latest(),true);});
function loaderHarness({broken=false,linkedBroken=false,changed=false,enabled=true}={}){
 const calls=[];const l=list('visit','2026-11-17');const context=vm.createContext({journalPhotoRefs:gallery.journalPhotoRefs,observationPhotos:gallery.observationPhotos,Error,
 invoke:async(command,args)=>{calls.push(command);if(command==='bird_observations_available')return enabled;if(command==='new_journal_session')return {generation:changed?8:7};throw Error(command);},
 readPhotograph:async(_invoke,entry,g,id)=>{calls.push(`read:${id}`);if(broken)throw Error('missing stored asset');return {mime:'image/png',preview:'AQID',width:32,height:32};},photographSource:a=>`data:${a.mime};base64,${a.preview}`,
 createJournalReader:()=>async()=>{calls.push('read_journal_bird_lists');if(linkedBroken)throw Error('failed');return {generation:7,data:[l]};}});
 const src=source('../src/lib/journal-photo-gallery.ts').replace(/^import .*;\n/gm,'').replace('export async function','async function');vm.runInContext(transpile(src),context);return {calls,load:()=>vm.runInContext(`loadJournalGallery(${JSON.stringify(JSON.stringify({type:'doc',content:[{type:'journalImage',attrs:{nodeId:'node',assetId:'asset',caption:'Caption'}}]}))},'entry',7,'sv')`,context)};
}
test('real loader composes read-only scopes, preserves missing stored-image failures and rejects a changed generation',async()=>{
 const h=loaderHarness();const result=await h.load();assert.deepEqual(Array.from(result.items,i=>i.key),['journal:node','bird:visit:row-visit:visit']);assert.equal(result.items[0].caption,'Caption');assert.equal(result.items[1].caption,'Bird caption');assert.notEqual(result.items[1].caption,'Keep');assert.match(result.items[1].description,/Skata · Lake/);assert.equal(result.incomplete,false);assert.deepEqual(h.calls,['read:asset','bird_observations_available','read_journal_bird_lists','new_journal_session']);
 const missing=await loaderHarness({broken:true}).load();assert.equal(missing.items[0].src,null);assert.equal(missing.items.length,2);
 const partial=await loaderHarness({linkedBroken:true}).load();assert.equal(partial.incomplete,true);assert.equal(partial.items.length,1);
 const gated=loaderHarness({enabled:false});assert.equal((await gated.load()).items.length,1);assert.ok(!gated.calls.includes('read_journal_bird_lists'));await assert.rejects(loaderHarness({changed:true}).load(),/generation_conflict/);
});
test('shared viewer labels previews, preserves captions, and makes failures visible',async()=>{
 const {default:component}=await import(viewerUrl);for(const locale of ['sv','en']){const html=render(component,{props:{items:[{key:'one',src:'data:image/png;base64,AQID',alt:'Alt',caption:'Original <caption>',description:'Skata · Lake · date'}],initialKey:'one',trigger:null,locale,onclose:()=>{}}}).body;assert.match(html,/Original &lt;caption>/);assert.match(html,/aria-labelledby="photo-viewer-heading"/);assert.match(html,/<h2 class="visually-hidden[^"]*" id="photo-viewer-heading">/);assert.match(html,locale==='sv'?/>Fotografi<\//:/>Photograph<\//);assert.match(html,locale==='sv'?/Sparad förhandsbild/:/Stored preview/);assert.match(html,locale==='sv'?/Om bilden/:/About the photo/);}
 const html=render(component,{props:{items:[{key:'bad',src:null,alt:'',caption:'Caption',description:''}],initialKey:'bad',trigger:null,locale:'en',onclose:()=>{}}}).body;assert.match(html,/role="alert"/);assert.match(html,/Could not load the preview\./);
});
test('journal thumbnails are buttons outside visit links and detail layout scopes only its visit',async()=>{
 const l=list('visit','2026-11-17');let text=source('../src/lib/components/BirdJournalLists.svelte').replace('available=$state(false)','available=$state(true)').replace('$state<BirdList[]>([])',`$state<BirdList[]>(${JSON.stringify([l])})`);const {default:preview}=await import(previewUrl(text));const html=render(preview,{props:{entryId:'origin',generation:7,locale:'en'}}).body;assert.match(html,/bird-photo-thumbnails/);assert.match(html,/aria-haspopup="dialog"/);assert.ok(html.indexOf('</a>')<html.indexOf('aria-haspopup="dialog"'));
 const {default:detail}=await import(detailUrl());const body=render(detail,{props:{list:l,origin:'/journal/entries/origin',locale:'en',preferences:names.DEFAULT_BIRD_NAMES,onPreferencesChange:()=>{}}}).body;assert.match(body,/with-photo/);assert.match(body,/data-photo-scope/);assert.match(body,/species-text/);
});

test("place photos use exact assignments, preserve captions and ignore repeated journal links", async () => {
 const entries=[{id:"entry",title:"Journal"},{id:"moved",title:"Wrong place"}];
 const attached=list("linked","2026-11-18");
 attached.journalEntryIds=["entry","other"];
 const standalone=list("standalone","2026-11-17");
 const wrongPlace={...list("wrong","2026-11-17"),place:{...standalone.place,id:"another-place"}};
 const imported={...list("imported","2026-11-17"),source:{}};
 const assetId=attached.rows[0].photo.asset_id;
 const contentJson=JSON.stringify({type:"doc",content:[
  {type:"journalImage",attrs:{nodeId:"first",assetId,caption:"First caption",alt:"Journal alt"}},
  {type:"journalImage",attrs:{nodeId:"second",assetId,caption:"Another caption"}},
 ]});
 let enabled=true,generation=7;
 const calls=[];
 const ctx=vm.createContext({
  journalPhotoRefs:gallery.journalPhotoRefs,observationPhotos:gallery.observationPhotos,
  birdHref:(_entry,id)=>`/observations/${id}`,entryHref:id=>`/journal/entries/${id}`,
  photographSource:asset=>`data:${asset.mime};base64,${asset.preview}`,
  readPhotograph:async(_invoke,id,_generation,assetId)=>{calls.push([id,assetId]);return photo(assetId);},
  readJournalPlace:async id=>({place:{id:id==="moved"?"different":"place"}}),
  createJournalReader:()=>async()=>({data:{contentJson}}),
  invoke:async command=>command==="bird_observations_available"?enabled:{generation},
 });
 const code=source("../src/lib/place-photos.ts").replace(/^import .*;\n/gm,"").replace(/^export /gm,"");
 vm.runInContext(transpile(code),ctx);
 const load=vm.runInContext("loadPlacePhotos",ctx);
 const visits=[attached,standalone,attached,wrongPlace,imported];
 const result=await load({id:"place",entries,visits},7,"en",()=>true);
 assert.deepEqual(calls,[["entry",assetId]]);
 assert.deepEqual(Array.from(result.items,item=>item.parentHref),["/journal/entries/entry","/observations/linked","/observations/standalone"]);
 assert.equal(result.items[0].caption,"First caption\nAnother caption");
 assert.equal(result.items[0].alt,"Journal alt");
 assert.equal(result.items[1].caption,"Bird caption");
 assert.equal(new Set(Array.from(result.items,item=>item.key)).size,3);
 assert.equal((await load({id:"place",entries:[],visits},7,"sv",()=>true)).items.length,2);
 assert.equal((await load({id:"place",entries,visits:[]},7,"en",()=>true)).items.length,1);
 assert.equal((await load({id:"place",entries:[],visits:[]},7,"en",()=>true)).items.length,0);
 enabled=false;
 assert.equal((await load({id:"place",entries,visits},7,"en",()=>true)).items.length,1);
 generation=8;
 await assert.rejects(load({id:"place",entries,visits},7,"en",()=>true),/journal_generation_conflict/);
});
