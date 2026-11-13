// Explicit rendered-form regression: node --test tests/bird-species-hit-area.browser.mjs
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
const species=[{id:'a',english:'Magpie',swedish:'skata',scientific:'Pica pica'},{id:'b',english:'Blue Tit',swedish:'blåmes',scientific:'Cyanistes caeruleus'},{id:'c',english:'Great Tit',swedish:'talgoxe',scientific:'Parus major'}];
test('real form selects from blank right side and padding once, including existing species',async t=>{
 const modules={
  'bird-state':'export const page={params:{id:"origin"}};',
  'bird-nav':'export function beforeNavigate(){};export async function goto(){};',
  'bird-window':'export function getCurrentWindow(){return {onCloseRequested:async()=>()=>{}};}',
  'bird-language':'export const language={current:"sv"};',
  'bird-main':`import {mount} from 'svelte';import Form from '${root}/src/lib/components/BirdListForm.svelte';
   window.__commands=[];window.__TAURI_INTERNALS__={invoke:async(command)=>{
    window.__commands.push(command);
    if(command==='bird_observations_available')return true;
    if(command==='get_journal_entry')return {generation:7,data:{id:'origin',entryDate:'2026-11-13'}};
    if(command==='new_journal_session')return {generation:7,data:null};
    if(command==='read_journal_place')return {generation:7,data:{place:null}};
    if(command==='search_bird_species')return ${JSON.stringify(species)};
    throw Error('Unexpected command '+command);
   }};mount(Form,{target:document.getElementById('app')});`
 };
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'bird-hit-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/bird-hit-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'$app/state',replacement:'bird-state'},{find:'$app/navigation',replacement:'bird-nav'},{find:'@tauri-apps/api/window',replacement:'bird-window'},{find:'$lib/language.svelte',replacement:'bird-language'},{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-bird-hit-'));
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
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/bird-hit-test`});await wait('document.querySelector("#species-search")');
 const search=async()=>{await evaluate('document.querySelector("#species-search").focus()');await send('Input.insertText',{text:'mes'});await wait('document.querySelectorAll("#species-results button").length===3');};
 const clickBlank=async(index,corner)=>{
  const point=await evaluate(`(()=>{const button=document.querySelectorAll('#species-results button')[${index}];const row=button.parentElement;row.scrollIntoView({block:'center'});const r=row.getBoundingClientRect(),b=button.getBoundingClientRect(),text=button.querySelector('span').getBoundingClientRect();const x=r.right-4,y=${corner?'r.bottom-3':'r.top+r.height/2'};return {x,y,row:[r.left,r.top,r.right,r.bottom],button:[b.left,b.top,b.right,b.bottom],blank:x>text.right,hit:document.elementFromPoint(x,y)===button};})()`);
  assert.deepEqual(point.button,point.row,'button fills entire row');assert.ok(point.blank,'click is beyond the name');assert.ok(point.hit,'blank space hits the actual button');
  for(const type of ['mouseMoved','mousePressed','mouseReleased']){await send('Input.dispatchMouseEvent',{type,x:point.x,y:point.y,button:type==='mouseMoved'?'none':'left',clickCount:type==='mouseMoved'?0:1});if(type==='mousePressed')await evaluate('new Promise(requestAnimationFrame)');}
  await wait('document.querySelector("#species-search").value===""');
 };
 await search();await clickBlank(1,false);
 assert.deepEqual(await evaluate('({rows:document.querySelectorAll(".rows li").length,focus:document.activeElement.id,name:document.querySelector(".species-name").textContent.trim()})'),{rows:1,focus:'bird-count-0',name:'Blåmes'});
 await evaluate(`const c=document.querySelector('#bird-count-0');c.value='4';c.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('.add-comment').click();`);await wait('document.querySelector("#bird-comment-0")');
 await evaluate(`const comment=document.querySelector('#bird-comment-0');comment.value='Keep my comment';comment.dispatchEvent(new Event('input',{bubbles:true}));`);
 await search();await clickBlank(1,true);
 assert.deepEqual(await evaluate('({rows:document.querySelectorAll(".rows li").length,focus:document.activeElement.id,count:document.querySelector("#bird-count-0").value,comment:document.querySelector("#bird-comment-0").value})'),{rows:1,focus:'bird-count-0',count:'4',comment:'Keep my comment'});
 await send('Emulation.setDeviceMetricsOverride',{width:480,height:900,deviceScaleFactor:1,mobile:false});await search();await clickBlank(2,true);
 assert.equal(await evaluate('document.querySelectorAll(".rows li").length'),2);assert.equal(await evaluate('document.activeElement.id'),'bird-count-1');
 const key=async(key,code)=>{for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key,code:key,windowsVirtualKeyCode:code,...(type==='keyDown'&&key==='Enter'?{text:'\r'}:{})});};
 await search();await key('ArrowDown',40);assert.equal(await evaluate('document.activeElement===document.querySelector("#species-results button")'),true);await key('Enter',13);
 await wait('document.activeElement.id==="bird-count-2"');assert.equal(await evaluate('document.querySelectorAll(".rows li").length'),3);
 await search();await key('Escape',27);await wait('document.querySelector("#species-search").value===""');
 assert.equal(await evaluate('window.__commands.some(c=>/create|update|delete|reserve/.test(c))'),false);
});
