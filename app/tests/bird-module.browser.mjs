// Explicit rendered module test: node --test tests/bird-module.browser.mjs
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
test('rendered module toggle is default-off, keyboard accessible and failure-safe',async t=>{
 const modules={
 'module-api':`export async function invoke(command,args){window.calls??=[];window.calls.push(command);if(command==='bird_module_supported')return true;if(command==='ebird_connection_enabled')return window.connectionEnabled??false;if(command==='set_ebird_connection_enabled'){window.connectionEnabled=args.enabled;return;}if(command==='bird_module_enabled')return window.enabled??false;if(command==='set_bird_module_enabled'){if(window.failSetting)throw Error('disk');window.enabled=args.enabled;if(window.loseResponse)throw Error('lost response');return;}throw Error(command);}`,
 'module-navigation':`export async function goto(path){window.destination=path;}`,
 'module-page':`export const page={url:new URL('https://mno.invalid/')};`,
 'bird-main':`import {setLocale} from '${root}/src/lib/paraglide/runtime.js';setLocale('sv',{reload:false});import {language} from '${root}/src/lib/language.svelte.ts';language.current='sv';window.setTestLanguage=locale=>language.current=locale;import {mount} from 'svelte';import Fixture from '${root}/tests/fixtures/BirdModuleSetting.svelte';mount(Fixture,{target:document.getElementById('app')});`
 };
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'bird-module-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/bird-module-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'module-api'},{find:'$app/navigation',replacement:'module-navigation'},{find:'$app/state',replacement:'module-page'},{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-bird-module-'));
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
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/bird-module-test`});await wait('document.querySelector("input[type=checkbox]")');
 const click=async expression=>{const point=await evaluate(`(()=>{const node=(${expression});node.scrollIntoView({block:'center'});const r=node.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);for(const type of ['mousePressed','mouseReleased'])await send('Input.dispatchMouseEvent',{type,...point,button:'left',clickCount:1});};

 assert.equal(await evaluate('document.querySelector("details").open'),false);
 await evaluate('document.querySelector("summary").focus()');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(type==='keyDown'?{text:'\r'}:{})});
 await wait('document.querySelector("details").open');
 assert.equal(await evaluate('window.calls.filter(c=>c==="set_bird_module_enabled").length'),0);
 assert.equal(await evaluate('document.querySelector("input").checked'),false);
 const accessibility=await send('Accessibility.getFullAXTree');
 const control=accessibility.nodes.find(node=>node.role?.value==='switch');
 assert.equal(control.name.value,'Fågelobservationer');
 assert.equal(control.properties.find(p=>p.name==='checked').value.value,'false');
 assert.equal(await evaluate('document.querySelector("#bird-module-description").textContent'),'Registrera fåglar från dina besök. Dina uppgifter finns kvar när modulen är avstängd.');
 assert.equal(await evaluate('document.querySelectorAll(".setting-help[aria-expanded=true]").length'),0);
 assert.equal(await evaluate('document.querySelector(".setting-help").ariaLabel'),'Om Fågelobservationer');
 await evaluate('document.querySelector(".setting-help").focus()');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(type==='keyDown'?{text:'\r'}:{})});
 await wait('document.querySelector(".setting-help").ariaExpanded==="true"');
 assert.notEqual(await evaluate('getComputedStyle(document.activeElement).outlineStyle'),'none');
 await click('document.querySelector(".source-setting .setting-help")');
 await wait('document.querySelector(".source-setting .setting-help").ariaExpanded==="true"');
 assert.equal(await evaluate('document.querySelector("#ebird-connection-dependency").textContent'),'Aktivera Fågelobservationer först.');
 assert.equal(await evaluate('window.calls.filter(c=>c.startsWith("set_")).length'),0);
 await evaluate('document.querySelector("input").focus()');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32,...(type==='keyDown'?{text:' '}:{} )});
 await wait('document.querySelector("input").checked');
 assert.equal(await evaluate('document.activeElement.tagName'),'INPUT');
 assert.equal(await evaluate('window.enabled'),true);
 assert.equal(await evaluate('getComputedStyle(document.querySelector("input")).backgroundColor'),'rgb(67, 91, 71)');
 assert.notEqual(await evaluate('getComputedStyle(document.querySelector("input")).outlineStyle'),'none');
 await evaluate(`window.veto=e=>e.detail.waitUntil(Promise.resolve(false));window.addEventListener('mno:before-bird-disable',window.veto)`);
 await click('document.querySelector("input")');await wait('document.querySelector("[role=alert]")');
 assert.equal(await evaluate('document.querySelector("input").checked'),true);
 assert.equal(await evaluate('window.enabled'),true);
 await evaluate(`window.removeEventListener('mno:before-bird-disable',window.veto);window.failSetting=true`);
 await click('document.querySelector("input")');await wait('document.querySelector("[role=alert]").textContent.includes("kunde inte")');
 assert.equal(await evaluate('document.querySelector("input").checked'),true);
 await evaluate('window.failSetting=false;window.loseResponse=true');await click('document.querySelector("input")');
 await wait('!document.querySelector("input").checked');assert.equal(await evaluate('window.enabled'),false);
 assert.equal(await evaluate('document.querySelector("#revision").textContent'),'2');
 await evaluate("window.setTestLanguage('en')");await wait('document.querySelector("label").textContent.includes("Bird observations")');
 assert.equal(await evaluate('document.querySelector("#bird-module-description").textContent'),'Record birds from your visits. Your data is kept when the module is turned off.');
 await send('Emulation.setDeviceMetricsOverride',{width:320,height:700,deviceScaleFactor:1,mobile:false});
 const helpFits=()=>evaluate(`[...document.querySelectorAll('.setting-row')].every(row=>{
  const label=row.querySelector('label').getBoundingClientRect(), help=row.querySelector('.help-text').getBoundingClientRect(), toggle=row.querySelector('input').getBoundingClientRect();
  return Math.abs(help.left-label.left)<1 && help.top>=label.bottom && help.right>=toggle.right-1;
 })`);
 assert.equal(await helpFits(),true);
 assert.equal(await evaluate('document.documentElement.scrollWidth<=320'),true);
 assert.equal(await helpFits(),true);
 await evaluate("window.setTestLanguage('sv')");
 await wait('document.querySelector(".setting-help").ariaLabel==="Om Fågelobservationer"');
 assert.equal(await helpFits(),true);
 const writesBeforeCollapse=await evaluate('window.calls.filter(c=>c==="set_bird_module_enabled").length');
 await evaluate('document.querySelector("summary").focus()');
 for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,...(type==='keyDown'?{text:'\r'}:{})});
 await wait('!document.querySelector("details").open');
 assert.equal(await evaluate('window.calls.filter(c=>c==="set_bird_module_enabled").length'),writesBeforeCollapse);
 assert.equal(await evaluate('window.enabled'),false);
 await evaluate('document.querySelector("summary").click();window.loseResponse=false');
 const sourceSwitch='document.querySelector(".source-setting input")';
 assert.equal(await evaluate(sourceSwitch+'.disabled'),true);
 await click('document.querySelector("input")');await wait('!'+sourceSwitch+'.disabled');
 assert.equal(await evaluate(sourceSwitch+'.checked'),false);
 await click(sourceSwitch);await wait(sourceSwitch+'.checked');
 await click('document.querySelector("input")');await wait(sourceSwitch+'.disabled');
 assert.equal(await evaluate(sourceSwitch+'.checked'),true);
 assert.equal(await evaluate('window.connectionEnabled'),true);
});
