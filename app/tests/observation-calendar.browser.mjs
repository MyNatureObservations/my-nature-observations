// Explicit rendered time test: node --test tests/observation-calendar.browser.mjs
// Installed Chrome or CHROME_BINARY; no downloads, backend or real journal.
import assert from 'node:assert/strict';
import test from 'node:test';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
const root=fileURLToPath(new URL('..',import.meta.url));
test('rendered calendar keyboard activation opens observation-only/mixed days and journal disclosure retains context',async t=>{
 const modules={'calendar-api':`export async function invoke(command,args){window.calls??=[];window.calls.push(command);if(command==='bird_observations_available')return true;if(command==='new_journal_session')return {generation:7};if(command==='read_journal_bird_lists')return {generation:7,data:[]};if(command==='read_journal_entries')return {generation:7,data:[{id:'entry',title:'Journal',entryDate:'2026-11-20',contentJson:JSON.stringify({type:'doc',content:[{type:'paragraph',content:[{type:'text',text:'Journal text'}]}]})}]};if(command==='read_bird_lists')return {generation:7,data:[{id:'visit',visitDate:args?.date??'2026-11-19',visitStart:null,visitEnd:null,place:{id:'p',name:'Gläntan',latitude:59,longitude:18},rows:[{english:'Great Tit',swedish:'talgoxe',scientific:'Parus major',count:'4'}]},{id:'mixed-visit',visitDate:'2026-11-20',visitStart:null,visitEnd:null,place:{id:'p',name:'Gläntan',latitude:59,longitude:18},rows:[{english:'Great Tit',swedish:'talgoxe',scientific:'Parus major',count:'4'}]}]};throw Error(command);}`,'bird-main':`import {setLocale} from '${root}/src/lib/paraglide/runtime.js';setLocale('sv',{reload:false});import {language} from '${root}/src/lib/language.svelte.ts';language.current='sv';window.setTestLanguage=locale=>{language.current=locale;};import {mount} from 'svelte';import Fixture from '${root}/tests/fixtures/ObservationCalendar.svelte';mount(Fixture,{target:document.getElementById('app')});`};
 const server=await createServer({root,configFile:false,logLevel:'error',plugins:[{name:'observation-calendar-test',resolveId(id){if(id in modules)return '\0'+id;},load(id){if(id.startsWith('\0'))return modules[id.slice(1)];},configureServer(server){server.middlewares.use('/observation-calendar-test',(_request,response)=>{response.setHeader('Content-Type','text/html');response.end('<!doctype html><meta charset="utf-8"><div id="app"></div><script type="module" src="/@id/__x00__bird-main"></script>');});}},svelte({configFile:false})],resolve:{alias:[{find:'@tauri-apps/api/core',replacement:'calendar-api'},{find:'$lib',replacement:root+'/src/lib'}]},server:{host:'127.0.0.1',port:0,fs:{allow:[root]}}});
 t.after(()=>server.close());await server.listen();
 const directory=await mkdtemp(join(tmpdir(),'mno-observation-calendar-'));
 const binary=process.env.CHROME_BINARY??(process.platform==='darwin'?'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome':'/usr/bin/google-chrome');
 const chrome=spawn(binary,['--headless=new','--disable-gpu','--no-first-run','--disable-background-networking','--remote-debugging-port=0',`--user-data-dir=${directory}`,'about:blank'],{stdio:'ignore'});
 let launchError;chrome.on('error',error=>launchError=error);const closed=new Promise(r=>chrome.once('close',r));
 t.after(async()=>{chrome.kill();await closed;await rm(directory,{recursive:true,force:true});});
 let port;const end=Date.now()+15000;
 while(!port){if(launchError)throw launchError;if(Date.now()>end)throw Error('Chrome failed to start');try{port=(await readFile(join(directory,'DevToolsActivePort'),'utf8')).split('\n')[0];}catch{await new Promise(r=>setTimeout(r,25));}}
 const target=await(await fetch(`http://127.0.0.1:${port}/json/new?about:blank`,{method:'PUT'})).json();
 const socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise((r,j)=>{socket.onopen=r;socket.onerror=j;});t.after(()=>socket.close());
 let sequence=0;const pending=new Map();const errors=[];socket.onmessage=event=>{const m=JSON.parse(event.data);if(m.method==='Runtime.exceptionThrown')errors.push(m.params);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}};
 const send=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));});
 const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert.equal(result.exceptionDetails,undefined,JSON.stringify(result.exceptionDetails));return result.result.value;};
 const wait=condition=>evaluate(`new Promise((resolve,reject)=>{const end=performance.now()+15000;function check(){if(${condition})resolve(true);else if(performance.now()>end)reject(Error('Form readiness timed out'));else requestAnimationFrame(check);}check();})`);
 await send('Page.enable');await send('Runtime.enable');await send('Emulation.setDeviceMetricsOverride',{width:1100,height:900,deviceScaleFactor:1,mobile:false});
 await send('Page.navigate',{url:`http://127.0.0.1:${server.httpServer.address().port}/observation-calendar-test`});await wait('document.querySelector(".calendar-day")');

 const key=async(key,code,modifiers=0,text=undefined)=>{for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key,code:key,windowsVirtualKeyCode:code,modifiers,...(type==='keyDown'&&text?{text}:{})});};


 await evaluate(`document.querySelector('[aria-label="19 november 2026 · Fågelobservationer"]').focus()`);
 await key('Enter',13,0,'\r');await wait('document.querySelector("#selected").textContent==="2026-11-19"');
 await wait(`document.querySelector('.calendar-day-panel a[href="/observations/visit"]')`);
 assert.equal(await evaluate(`document.querySelector('.calendar-day-panel a[href="/observations/visit"]').textContent.includes('Gläntan')`),true);
 await evaluate('document.querySelector(".day-panel-close").click()');
 await evaluate(`document.querySelector('[aria-label="20 november 2026 · Journalanteckningar · Fågelobservationer"]').focus()`);
 await key(' ',32,0,' ');await wait('document.querySelector("#selected").textContent==="2026-11-20"');
 await wait(`document.querySelector('.calendar-day-panel a[href="/journal?entry=entry"]')`);
 await wait(`document.querySelector('.calendar-day-panel a[href="/observations/mixed-visit"]')`);
 await evaluate('document.querySelector(".day-panel-close").click()');
 await wait('document.querySelector("#journal-actions summary")');await evaluate('document.querySelector("#journal-actions summary").focus()');await key('Enter',13,0,'\r');
 assert.equal(await evaluate('document.querySelector("#journal-actions details").open'),true);
 await key('Tab',9);assert.equal(await evaluate('document.activeElement.getAttribute("href")'),'/journal/entries/entry/birds/new');
 await key('Tab',9);assert.equal(await evaluate('document.activeElement.textContent'),'Koppla befintlig');
 await key('Enter',13,0,'\r');await wait('document.querySelector("dialog[open]")');
 await key('Escape',27);await wait('!document.querySelector("dialog[open]")');
 assert.equal(await evaluate('document.activeElement===document.querySelector("#journal-actions summary")'),true);

 // Compare real card geometry and 300px calendar markers, not enlarged swatches.
 for(const locale of ['sv','en'])for(const width of [1100,420]){
  await send('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});
  await evaluate(`window.setTestLanguage('${locale}')`);
  await wait(`document.querySelector('#home-preview .observation-entry strong')?.textContent===${JSON.stringify(locale==='sv'?'Registrera en observation':'Record an observation')}`);
  const geometry=await evaluate(`JSON.stringify((()=>{
   const cards=[...document.querySelectorAll('#home-preview .journal-card')];
   const bounds=cards.map(node=>{const r=node.getBoundingClientRect(),s=getComputedStyle(node);return {x:r.x,y:r.y,width:r.width,height:r.height,border:s.border,font:getComputedStyle(node.querySelector('h2')).font,padding:s.padding,overflow:node.scrollWidth>node.clientWidth+1};});
   const dots=[...document.querySelectorAll('[aria-label*="20 "] .day-markers span')].map(node=>{const r=node.getBoundingClientRect();return {x:r.x,width:r.width,color:getComputedStyle(node).backgroundColor}});
   const compartment=document.querySelector('#home-preview .observation-entry');const cr=compartment.getBoundingClientRect();const rows=[...document.querySelectorAll('.calendar-day')].map(button=>{const b=button.getBoundingClientRect(),n=button.querySelector('.day-number').getBoundingClientRect(),m=button.querySelector('.day-markers').getBoundingClientRect();return {gap:m.top-n.bottom,bottom:b.bottom-m.bottom,center:(m.left+m.right-b.left-b.right)/2,height:b.height}});return {rows,compartment:{x:cr.x,y:cr.y,width:cr.width,height:cr.height,border:getComputedStyle(compartment).border,overflow:compartment.scrollWidth>compartment.clientWidth+1},bounds,dots,legend:[...document.querySelectorAll('.calendar-legend i')].map(n=>getComputedStyle(n).backgroundColor),pageOverflow:document.documentElement.scrollWidth>innerWidth};
  })())`);
  const {rows,compartment,bounds,dots,legend,pageOverflow}=JSON.parse(geometry);
  assert.equal(bounds.length,2);assert.equal(pageOverflow,false);assert.ok(bounds.every(card=>!card.overflow));
  assert.equal(bounds[0].border,compartment.border);assert.equal(compartment.overflow,false);assert.ok(compartment.y>bounds[1].y);assert.ok(compartment.height<bounds[0].height);
  assert.ok(Math.abs(compartment.width-bounds[0].width)<1);assert.ok(Math.abs(compartment.x-bounds[0].x)<1);
  assert.ok(rows.every(row=>row.gap>=4&&row.bottom>=5&&Math.abs(row.center)<1));assert.equal(new Set(rows.map(row=>row.height)).size,1);
  if(width>600){assert.equal(bounds[0].y,bounds[1].y);}else{assert.ok(bounds.every((card,i)=>i===0||card.y>bounds[i-1].y));}
  assert.equal(dots.length,2);assert.equal(dots[0].width,6);assert.equal(dots[1].width,6);assert.ok(dots[1].x-dots[0].x>=11);
  assert.equal(dots[0].color,'rgb(120, 142, 123)');assert.equal(dots[1].color,'rgb(55, 107, 155)');assert.deepEqual(legend,['rgb(120, 142, 123)','rgb(55, 107, 155)']);
  if(process.env.MNO_NAV_SCREENSHOTS){
   await evaluate('document.querySelector("#home-preview").scrollIntoView()');
   const shot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await writeFile(`${process.env.MNO_NAV_SCREENSHOTS}-${locale}-${width}.png`,Buffer.from(shot.data,'base64'));
   await evaluate('window.scrollTo(0,0)');const calendarShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await writeFile(`${process.env.MNO_NAV_SCREENSHOTS}-calendar-${locale}-${width}.png`,Buffer.from(calendarShot.data,'base64'));
  }
 }
 assert.deepEqual(errors,[]);
 assert.equal(await evaluate('window.calls.every(command=>["bird_observations_available","new_journal_session","read_journal_entries","read_bird_lists","read_journal_bird_lists"].includes(command))'),true);
});
