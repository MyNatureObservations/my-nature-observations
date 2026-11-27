// Explicit rendered module test: node --test tests/ebird-preview.browser.mjs
// Installed Chrome or CHROME_BINARY; no downloads, backend or real journal.
import assert from 'node:assert/strict';
import test from 'node:test';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
const root=fileURLToPath(new URL('..',import.meta.url));
test('rendered eBird import and imported species media preserve source details and keyboard access',async t=>{
 const modules={
 'ebird-api':'window.nextCsv='+JSON.stringify(await readFile(join(root,'tests/fixtures/ebird/full.csv'),'utf8'))+';'+`export async function invoke(command,args){window.calls??=[];window.calls.push(command);if(command==='ebird_preview_available')return true;if(command==='ebird_import_epoch')return 1;if(command==='preview_ebird_export')return window.nextCsv;if(command==='new_journal_session')return {generation:1};if(command==='review_ebird_import')return {operationId:'synthetic-review',generation:1,changes:args.visits.map(v=>({submissionId:v.submission_id,kind:'new',fields:v.rows[0]}))};if(command==='confirm_ebird_import'){window.imported=true;if(window.loseReply)throw Error('lost reply');return 'committed';}if(command==='ebird_import_status')return window.imported?'committed':'unknown';if(command==='cancel_ebird_import')return null;if(command==='match_bird_paste_names')return args.names.map(name=>name==='Parus major'?[{id:'synthetic-tit',scientific:name,english:'Great Tit',swedish:'Talgoxe'}]:[]);throw Error(command);}`,
 'ebird-opener':`export async function openUrl(url){window.opened=url;}`,
 'module-navigation':`export function beforeNavigate(){} export async function goto(path){window.destination=path;}`,
 'module-page':`export const page={url:new URL('https://mno.invalid/observations/ebird-preview')};`,
 'bird-main':`import {setLocale} from '${root}/src/lib/paraglide/runtime.js';setLocale('sv',{reload:false});import {language} from '${root}/src/lib/language.svelte.ts';language.current='sv';window.setTestLanguage=locale=>language.current=locale;import {mount,unmount} from 'svelte';let mediaDetail;import Detail from '${root}/src/lib/components/EbirdListDetail.svelte';window.mountMediaDetail=locale=>{const target=document.querySelector('#media-detail');if(mediaDetail)unmount(mediaDetail);const row=(name,ids)=>({fields:{'Common Name':name,'Scientific Name':'Synthetic taxon',Count:'2','ML Catalog Numbers':ids},additions:[]});mediaDetail=mount(Detail,{target,props:{locale,origin:'/observations',list:{place:{name:'Test place'},visitDate:'2026-11-01',source:{submissionId:'S123',fields:{},additions:[],unresolved:[],rows:[row('No media',''),row('One record','12345'),row('Several records','invalid 23456 34567 45678 56789 67890 78901 89012')]}}}});};import Preview from '${root}/src/lib/components/EbirdPreview.svelte';mount(Preview,{target:document.getElementById('app')});`
 };
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'ebird-preview-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/ebird-preview-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'ebird-api'},{find:'@tauri-apps/plugin-opener',replacement:'ebird-opener'},{find:'$app/navigation',replacement:'module-navigation'},{find:'$app/state',replacement:'module-page'},{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-ebird-preview-'));
 const binary=process.env.CHROME_BINARY??(process.platform==='darwin'?'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome':'/usr/bin/google-chrome');
 const chrome=spawn(binary,['--headless=new','--disable-gpu','--no-first-run','--disable-background-networking','--remote-debugging-port=0',`--user-data-dir=${directory}`,'about:blank'],{stdio:'ignore'});
 let launchError;chrome.on('error',error=>launchError=error);const closed=new Promise(r=>chrome.once('close',r));
 t.after(async()=>{chrome.kill();await closed;await rm(directory,{recursive:true,force:true});});
 let port;const end=Date.now()+15000;
 while(!port){if(launchError)throw launchError;if(Date.now()>end)throw Error('Chrome failed to start');try{port=(await readFile(join(directory,'DevToolsActivePort'),'utf8')).split('\n')[0];}catch{await new Promise(r=>setTimeout(r,25));}}
 const target=await(await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'})).json();
 const socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise((r,j)=>{socket.onopen=r;socket.onerror=j;});t.after(()=>socket.close());
 let sequence=0;const pending=new Map();socket.onmessage=event=>{const m=JSON.parse(event.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
 const send=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert.equal(result.exceptionDetails,undefined,JSON.stringify(result.exceptionDetails));return result.result.value;};
 const wait=condition=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+15000;function check(){if(${condition})resolve(true);else if(performance.now()>end)reject(Error('Form readiness timed out'));else requestAnimationFrame(check);}check();})`);
 await send('Page.enable');await send('Emulation.setDeviceMetricsOverride',{width:1100,height:900,deviceScaleFactor:1,mobile:false});
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/ebird-preview-test`});await wait('document.querySelector("button.choose")');
 const click=async expression=>{const point=await evaluate(`(()=>{const node=(${expression});node.scrollIntoView({block:'center'});const r=node.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1});};


 await click('document.querySelector("button.choose")');await wait('document.querySelectorAll(".visit").length===2');
 assert.equal(await evaluate('document.querySelector("main").textContent.includes("skrivskyddade")'),true);
 assert.equal(await evaluate('document.querySelector(".species-preview").textContent'),'Great Tit 6 · Unidentified bird Antal ej angivet');
 assert.equal(await evaluate('getComputedStyle(document.querySelector(".species-separator")).whiteSpace'),'pre');
 await evaluate('document.querySelector(".visit summary").focus()');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(type==='keyDown'?{text:'\r'}:{})});
 await wait('document.querySelectorAll(".rows li").length===2');
 assert.equal(await evaluate('document.querySelector(".rows").textContent.includes("Antal ej angivet")'),true);
 assert.equal(await evaluate('document.querySelector(".rows").textContent.includes("Källans namn visas")'),true);
 assert.equal(await evaluate('document.querySelector(".source-details").open'),false);
 assert.equal(await evaluate('Boolean(document.querySelector(".rows").compareDocumentPosition(document.querySelector(".source-details")) & Node.DOCUMENT_POSITION_FOLLOWING)'),true);
 await click('document.querySelector(".rows .text-link")');await wait('window.opened');
 assert.equal(await evaluate('window.opened'),'https://macaulaylibrary.org/asset/10001');
 // Sorting must move the complete row, keeping X and media on their species.
 const alphabetCsv=(await readFile(join(root,'tests/fixtures/ebird/full.csv'),'utf8')).replaceAll('Great Tit','Örn').replaceAll('Unidentified bird','Åda');
 await evaluate('window.nextCsv='+JSON.stringify(alphabetCsv));await click('document.querySelector("button.choose")');
 await wait('document.querySelector(".species-preview").textContent.includes("Åda Antal ej angivet · Örn 6")');
 await click('document.querySelector(".visit summary")');await wait('document.querySelectorAll(".rows li").length===2');
 assert.equal(await evaluate('document.querySelectorAll(".rows li")[0].querySelector("strong").textContent'),'Åda');
 assert.equal(await evaluate('document.querySelectorAll(".rows li")[0].querySelectorAll(".media-links").length'),0);
 assert.equal(await evaluate('document.querySelectorAll(".rows li")[1].querySelectorAll(".media-links button").length'),2);
 const extraCsv=(await readFile(join(root,'tests/fixtures/ebird/full.csv'),'utf8')).replace('ML Catalog Numbers\n','ML Catalog Numbers,Observation Details\n').replace('10001 10002\n','10001 10002,synthetic comment\n');
 const extraLine=extraCsv.split('\n').find(line=>line.startsWith('S100,Great'));
 await evaluate('window.nextCsv='+JSON.stringify(extraCsv.trimEnd()+'\n'+extraLine.replace('Great Tit','Synthetic bird three').replace('Parus major','Synthetic taxon three')+'\n'+extraLine.replace('Great Tit','Synthetic bird four').replace('Parus major','Synthetic taxon four')+'\n'));
 await click('document.querySelector("button.choose")');await wait('document.querySelector(".species-preview").textContent.includes("och 1 till")');
 assert.equal(await evaluate('document.querySelector(".visit summary").textContent.includes("S100")'),false);
 await click('document.querySelector(".visit summary")');await wait('document.querySelectorAll(".rows li").length===4');
 assert.equal(await evaluate('document.querySelectorAll(".rows li")[0].textContent.includes("synthetic comment")'),true);
 await evaluate("window.setTestLanguage('en')");await wait('document.querySelector(".species-preview").textContent.includes("and 1 more")');
 assert.equal(await evaluate('document.querySelector(".source-details summary").textContent'),'Source details');
 await evaluate("window.setTestLanguage('sv')");
 await evaluate('window.loseReply=true');
 await click('document.querySelector(".comparison>button")');await wait('document.querySelector("dialog[open]")');assert.equal(await evaluate('document.activeElement.textContent'),'Avbryt');
 await click('document.querySelector("dialog footer button")');await wait('!document.querySelector("dialog")');assert.equal(await evaluate('window.imported===undefined'),true);
 await click('document.querySelector(".comparison>button")');await click('document.querySelector("dialog footer button:last-child")');await wait('window.imported && !document.querySelector("dialog")');
 assert.equal(await evaluate('window.calls.filter(c=>c==="confirm_ebird_import").length'),1);
 await evaluate('window.nextCsv=null');await click('document.querySelector("button.choose")');
 await wait('!document.querySelector("button.choose").disabled');assert.equal(await evaluate('document.querySelectorAll(".visit").length'),2);
 await evaluate('window.nextCsv='+"\"Species,Count,Location,Observation Date,Start Time\\nGreat Tit (Parus major),2,Synthetic location,\\\"Nov 1, 2026\\\",8:15 AM\\n\"");await click('document.querySelector("button.choose")');
 await wait('document.querySelector("[role=alert]")?.textContent.includes("enskild checklista")');
 await evaluate("window.setTestLanguage('en')");await wait('document.querySelector("[role=alert]").textContent.includes("single-checklist")');
 assert.deepEqual(await evaluate('[...new Set(window.calls)].sort()'),['ebird_preview_available','match_bird_paste_names','ebird_import_epoch','preview_ebird_export','new_journal_session','review_ebird_import','confirm_ebird_import','ebird_import_status','cancel_ebird_import'].sort());
 await evaluate(`document.body.insertAdjacentHTML('beforeend','<section id="media-detail"></section>');document.querySelector('#app').hidden=true`);
 const card = name => `[...document.querySelectorAll('#media-detail li')].find(row=>row.querySelector('strong').textContent===${JSON.stringify(name)})`;
 const key = async (key,code,windowsVirtualKeyCode) => {
  for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key,code,windowsVirtualKeyCode,...(key==='Enter'&&type==='keyDown'?{text:'\r'}:{})});
 };
 for(const [locale,width] of [['sv',340],['sv',900],['en',340],['en',900]]){
  await send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});
  await evaluate(`window.mountMediaDetail('${locale}');window.opened=null`);
  await wait('document.querySelectorAll("#media-detail li").length===3');
  assert.equal(await evaluate(`(${card('No media')}).querySelector('button')===null`),true);
  await click(`(${card('One record')}).querySelector('strong')`);
  await wait('window.opened==="https://macaulaylibrary.org/asset/12345"');
  const one=`(${card('One record')}).querySelector('button')`;
  const multi=`(${card('Several records')})`;
  const line=`${multi}.querySelector('.media-line')`;
  const links=`${line}.querySelectorAll('a')`;
  assert.equal(await evaluate(`${multi}.querySelector('button')===null`),true);
  assert.deepEqual(await evaluate(`[...${links}].map(a=>a.textContent.trim())`),Array.from({length:7},(_,i)=>`Media ${i+1} ↗`));
  assert.equal(await evaluate(`new Set([...${links}].map(a=>a.offsetTop)).size`),1);
  assert.equal(await evaluate(`${line}.scrollWidth>${line}.clientWidth`),width===340);
  await evaluate('window.opened=null');
  await click(`${multi}.querySelector('strong')`);
  assert.equal(await evaluate('window.opened'),null);
  await click(`${links}[1]`);
  await wait('window.opened==="https://macaulaylibrary.org/asset/34567"');
  await evaluate(`window.opened=null;${multi}.closest('ul').previousElementSibling.focus()`);
  await key('Tab','Tab',9);
  assert.equal(await evaluate(`document.activeElement===${one}`),true);
  assert.equal(await evaluate(`getComputedStyle(${one},'::after').outlineStyle`),'solid');
  await key('Enter','Enter',13);
  await wait('window.opened==="https://macaulaylibrary.org/asset/12345"');
  for(let i=0;i<7;i++){
   await key('Tab','Tab',9);
   assert.equal(await evaluate(`document.activeElement===${links}[${i}]`),true);
  }
  assert.equal(await evaluate(`getComputedStyle(${links}[6]).outlineStyle`),'solid');
  await wait(`${links}[6].getBoundingClientRect().right<=${line}.getBoundingClientRect().right+1`);
  await key('Enter','Enter',13);
  await wait('window.opened==="https://macaulaylibrary.org/asset/89012"');
 }
});
