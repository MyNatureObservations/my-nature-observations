// Explicit rendered paste test: node --test tests/bird-paste.browser.mjs
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
test('rendered paste review supports keyboard cancel and draft-only apply',async t=>{
 const modules={'paste-api':`export async function invoke(command,args){window.calls??=[];window.calls.push(command);if(command!=='match_bird_paste_names')throw Error(command);return args.names.map(()=>[{id:'tit',swedish:'talgoxe',english:'Great Tit',scientific:'Parus major'}]);}`, 'bird-main':`import {mount} from 'svelte';import Fixture from '${root}/tests/fixtures/BirdPasteReview.svelte';mount(Fixture,{target:document.getElementById('app')});`};
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'paste-review-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/paste-review-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'paste-api'},{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-paste-review-'));
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
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/paste-review-test`});await wait('document.querySelector("#open")');
 const click=async expression=>{const point=await evaluate(`(()=>{const node=(${expression});node.scrollIntoView({block:'center'});const r=node.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1});};
 await click('document.querySelector("#open")');await wait('document.querySelector("#bird-paste-source")');
 assert.equal(await evaluate('document.activeElement.id'),'bird-paste-source');
 await send('Input.insertText',{text:'Talgoxe 6\nTalgoxe 4st'});
 await click('document.querySelector("dialog .primary")');await wait('document.querySelectorAll(".review-rows li").length===2');
 assert.equal(await evaluate('document.querySelector("#rows").textContent'),'[]');
 assert.equal(await evaluate('document.querySelector("dialog .primary").disabled'),true);
 await evaluate(`const select=document.querySelector('.duplicate select');select.value='paste:1';select.dispatchEvent(new Event('change',{bubbles:true}));`);
 await wait('!document.querySelector("dialog .primary").disabled');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
 await wait('!document.querySelector("dialog")');assert.equal(await evaluate('document.querySelector("#rows").textContent'),'[]');assert.equal(await evaluate('document.activeElement.id'),'open');
 await click('document.querySelector("#open")');await wait('document.querySelector("#bird-paste-source")');await send('Input.insertText',{text:'Talgoxe 6'});await click('document.querySelector("dialog .primary")');await wait('document.querySelector(".review-rows li")');
 await click('document.querySelector("dialog .primary")');await wait('!document.querySelector("dialog")');
 const rows=JSON.parse(await evaluate('document.querySelector("#rows").textContent'));assert.equal(rows.length,1);assert.equal(rows[0].count,'6');assert.deepEqual(await evaluate('window.calls'),['match_bird_paste_names','match_bird_paste_names']);
});
