// Explicit rendered time test: node --test tests/visit-time-keyboard.browser.mjs
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
test('real time field allows Tab/Shift+Tab between segments and out, direct HH:mm typing and clearing',async t=>{
 const modules={'bird-main':`import {mount} from 'svelte';import Fixture from '${root}/tests/fixtures/VisitTimeKeyboard.svelte';mount(Fixture,{target:document.getElementById('app')});`};
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'time-keyboard-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/time-keyboard-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-time-keyboard-'));
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
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/time-keyboard-test`});await wait('document.querySelector("#test-time")');

 const key=async(key,code,modifiers=0,text=undefined)=>{for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key,code:key,windowsVirtualKeyCode:code,modifiers,...(type==='keyDown'&&text?{text}:{})});};
 const active=()=>evaluate('document.activeElement.id');
 const value=()=>evaluate('document.querySelector("#current-time").textContent');
 await evaluate('document.querySelector("#before").focus()');await key('Tab',9);assert.equal(await active(),'test-time');
 await send('Input.insertText',{text:'08'});assert.equal(await value(),'08:');
 await key('Tab',9);assert.equal(await active(),'test-time-minute');await send('Input.insertText',{text:'07'});assert.equal(await value(),'08:07');
 await key('Tab',9,8);assert.equal(await active(),'test-time');await key('Tab',9,8);assert.equal(await active(),'before');
 await key('Tab',9);await key('Tab',9);assert.equal(await active(),'test-time-minute');await key('Tab',9);
 assert.equal(await evaluate('document.activeElement===document.querySelector("summary")'),true);await key('Tab',9);assert.equal(await active(),'after');
 await evaluate('document.querySelector("summary").click()');await wait('document.querySelector("details").open');
 await evaluate('document.querySelector(".time-panel button").click()');assert.equal(await value(),'');assert.equal(await active(),'test-time');
 await send('Input.insertText',{text:'12'});await key(':',186,0,':');assert.equal(await active(),'test-time-minute');await send('Input.insertText',{text:'43'});assert.equal(await value(),'12:43');
 await evaluate(`const input=document.querySelector('#test-time');const data=new DataTransfer();data.setData('text/plain','25:99');input.dispatchEvent(new ClipboardEvent('paste',{clipboardData:data,bubbles:true,cancelable:true}));`);
 assert.equal(await value(),'25:99');assert.equal(await evaluate('document.querySelector("#test-time").getAttribute("aria-invalid")'),'true');
 assert.equal(await evaluate('document.querySelector("#test-time-minute").value'),'99');
 await evaluate('document.querySelector("summary").click();document.querySelector(".time-panel button").click()');assert.equal(await value(),'');
});
