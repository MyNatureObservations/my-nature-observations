import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import ts from 'typescript';
import { compile } from 'svelte/compiler';
import { render } from 'svelte/server';
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const nav = read('../src/lib/components/AppNavigation.svelte');
const pageSource = read('../src/routes/search/+page.svelte');
const entry = read('../src/routes/journal/entries/[id]/+page.svelte');
const translations = Object.fromEntries(['sv','en'].map(locale=>[locale,JSON.parse(read(`../messages/${locale}.json`))]));
const script = source => ts.transpileModule(source.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm,''), {compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.None}}).outputText;
const flush = async () => {for(let i=0;i<12;i++)await Promise.resolve();};
function navHarness({blocked=false, initial='https://mno.invalid/journal'}={}) {
  const calls=[],timers=new Map(),effects=[]; let navigate; let next=0;
  const context=vm.createContext({$state:v=>v,$derived:v=>v,$effect:f=>effects.push(f),
    PLACE_MAP: 'mno:place-map', getContext: () => ({ close() {} }),
    createJournalReader:()=>()=>Promise.resolve({data:[]}),language:{current:'sv'},m:new Proxy({},{get:()=>()=>''}),
    afterNavigate:f=>navigate=f,page:{url:new URL(initial)},
    setTimeout:(fn,ms)=>{assert.equal(ms,300);timers.set(++next,fn);return next;},clearTimeout:id=>timers.delete(id),
    goto:async (url,options)=>{calls.push({url,options});if(blocked)throw Error('navigation cancelled');context.page.url=new URL(url,initial);navigate({to:{url:context.page.url}});},
  });
  vm.runInContext(script(nav),context); const dispose=effects[0]();
  return {calls,run:code=>vm.runInContext(code,context),timers,
    async tick(){const pending=[...timers.values()];timers.clear();pending.forEach(f=>f());await flush();},dispose,
    returnTo(url){const from={url:context.page.url};context.page.url=new URL(url,initial);navigate({from,to:{url:context.page.url}});}};
}
function resultsHarness() {
  const effects=[],calls=[];let changed,cleanup;
  const context=vm.createContext({$state:v=>v,$derived:v=>v,$effect:f=>effects.push(f),page:{url:new URL('https://mno.invalid/search?q=hackspett')},
    createJournalReader:()=> (command,args)=>new Promise((resolve,reject)=>calls.push({command,args,resolve,reject})),
    window:{addEventListener:(_name,f)=>changed=f,removeEventListener:()=>{}},
  });
  vm.runInContext(script(pageSource),context);cleanup=effects[1]();
  return {calls,run:code=>vm.runInContext(code,context),changed:()=>changed(),dispose:cleanup};
}
test('search components compile, search is enabled and uses ordinary result links',()=>{
  for(const source of [nav,pageSource,entry])assert.equal(compile(source,{generate:'client'}).warnings.length,0);
  assert.match(nav,/bind:value=\{searchText\}/);assert.doesNotMatch(nav,/nav_search_unavailable/);
  assert.match(pageSource,/href=\{`\/journal\/entries\/\$\{encodeURIComponent\(entry.id\)\}\?from=search&q=/);
  assert.doesNotMatch(pageSource,/@html|edit=true/);
  assert.match(entry,/search_back/);assert.match(entry,/\/search\?q=/);
});
test('typing waits 300ms, coalesces keystrokes and retains focus and panel',async()=>{
  const h=navHarness();h.run('activePanel="left"; searchText="hack"; scheduleSearch()');
  h.run('searchText="hackspett"; scheduleSearch()');assert.equal(h.calls.length,0);assert.equal(h.timers.size,1);
  await h.tick();assert.equal(h.calls.length,1);assert.equal(h.calls[0].url,'/search?q=hackspett');
  assert.equal(h.calls[0].options.keepFocus,true);assert.equal(h.run('activePanel'),'left');
  h.run('searchText="örn"; scheduleSearch()');await h.tick();assert.equal(h.calls[1].options.replaceState,true);
  h.dispose();h.run('scheduleSearch()');await h.tick();assert.equal(h.calls.length,2);
});
test('blocked navigation does not repeatedly reopen editor confirmation',async()=>{
  const h=navHarness({blocked:true});h.run('searchText="draft"; scheduleSearch()');await h.tick();
  assert.equal(h.calls.length,1);assert.equal(h.timers.size,0);assert.equal(h.run('page.url.pathname'),'/journal');
  assert.equal(h.run('searchNavigating'),false);h.dispose();
});
test('returning to search restores the encoded query in the existing field',()=>{
  const h=navHarness();h.returnTo('/search?q=%C3%B6rn%20och%20hackspett');
  assert.equal(h.run('searchText'),'örn och hackspett');h.returnTo('/journal/entries/abc');
  assert.equal(h.run('activePanel'),null);h.dispose();
});
test('only latest search reply can replace results, including failure and disposal',async()=>{
  const h=resultsHarness();const old=h.run('load("old")');const current=h.run('load("new")');
  assert.equal(h.calls[1].command,'search_journal');assert.deepEqual({...h.calls[1].args},{query:'new',offset:0});
  h.calls[1].resolve({generation:1,data:{entries:[{id:'new'}],more:false}});await current;
  h.calls[0].resolve({generation:1,data:{entries:[{id:'old'}],more:true}});await old;
  assert.equal(h.run('entries[0].id'),'new');assert.equal(h.run('loading'),false);
  const late=h.run('load("late")');h.dispose();h.calls[2].reject('closed');await late;
  assert.equal(h.run('failed'),false);assert.equal(h.run('entries.length'),0);
});
test('clearing a query invalidates an in-flight reply; errors are visible and can retry',async()=>{
  const h=resultsHarness();const pending=h.run('load("old")');await h.run('load("")');
  h.calls[0].resolve({data:{entries:[{id:'old'}],more:false}});await pending;
  assert.equal(h.run('entries.length'),0);assert.equal(h.run('loading'),false);
  const fail=h.run('load("bad")');h.calls[1].reject('blocked');await fail;assert.equal(h.run('failed'),true);
  const retry=h.run('load("valid")');h.calls[2].resolve({data:{entries:[],more:false}});await retry;
  assert.equal(h.run('failed'),false);h.dispose();
});
test('journal change refreshes current query and pagination is bounded',async()=>{
  const h=resultsHarness();h.changed();assert.equal(h.calls[0].args.query,'hackspett');
  h.calls[0].resolve({data:{entries:[{id:'one'}],more:true}});await flush();
  const next=h.run('load("hackspett",true)');await h.run('load("hackspett",true)');assert.equal(h.calls.length,2);assert.equal(h.calls[1].args.offset,1);
  h.calls[1].resolve({data:{entries:[{id:'two'}],more:false}});await next;
  assert.equal(h.run('entries.length'),2);assert.equal(h.run('more'),false);h.dispose();
});
const moduleUrl = code => `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
const resolveImports = (code,local) => code.replace(/from (["'])([^"']+)\1/g,(_m,q,name)=>`from ${q}${local[name]??import.meta.resolve(name)}${q}`);
test('actual result markup escapes excerpts, labels links and supplies both languages',async()=>{
  const seeded=pageSource.replace('let entries = $state<Hit[]>([])',`let entries = $state<Hit[]>(${JSON.stringify([{id:'a/b',entryDate:'2026-10-26',title:'Örn',excerpt:'<img src=x onerror=attack()> & text'}])})`);
  const compiled=compile(seeded,{filename:'Search.svelte',generate:'server'});
  const imports={'$app/state':moduleUrl('export const page={url:new URL("https://mno.invalid/search?q=%C3%B6rn")};'),
    '$lib/journal-data':moduleUrl('export const createJournalReader=()=>()=>{};'),
    '$lib/journal-reading':moduleUrl('export const formatJournalDate=(date)=>date;'),
    '$lib/language.svelte':moduleUrl('export const language={current:"sv"};'),
    '$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href};
  const component=await import(moduleUrl(resolveImports(compiled.js.code,imports)));
  const html=render(component.default).body;
  assert.match(html,/a%2Fb\?from=search&amp;q=%C3%B6rn/);assert.match(html,/&lt;img/);assert.doesNotMatch(html,/<img /);
  assert.match(html,/<time datetime="2026-10-26"/);assert.match(html,/Örn/);
  assert.match(html,/<a[^>]*class="back-link[^"]*"[^>]*href="\/"[^>]*>← Tillbaka till startsidan<\/a>/);
  assert.equal(translations.en.restore_back_to_home,'Back to home');
  assert.match(pageSource,/m\.restore_back_to_home/);
  for(const locale of ['sv','en'])for(const key of ['search_title','search_instruction','search_loading','search_failed','search_empty','search_back','search_more'])assert.equal(typeof translations[locale][key],'string');
});

test('entry back link retains the exact search text and does not start editing',()=>{
  const context=vm.createContext({$state:v=>v,$derived:v=>v,onMount(){},createJournalReader:()=>()=>{},
    page:{params:{id:'entry'},url:new URL('https://mno.invalid/journal/entries/entry?from=search&q=hackspett%20%26%20%C3%B6rn')},language:{current:'sv'}});
  vm.runInContext(script(entry),context);
  assert.equal(vm.runInContext('backHref',context),'/search?q=hackspett%20%26%20%C3%B6rn');
  assert.equal(vm.runInContext('isEditing',context),false);
});

test('only an open left panel on search receives the docked layout and desktop click-through',async()=>{
  async function panelHtml(path,open) {
    const seeded=nav.replace('let activePanel = $state<Panel | null>(null)',`let activePanel = $state<Panel | null>(${open?'"left"':'null'})`);
    const compiled=compile(seeded,{filename:'AppNavigation.svelte',generate:'server'});
    const imports={
      '$lib/journal-data':moduleUrl('export const createJournalReader=()=>()=>{};'),
      '$lib/components/CalendarGrid.svelte':moduleUrl('export default function() {}'),
      '$lib/components/CalendarDayPanel.svelte':moduleUrl('export default function() {}'),
      '$lib/components/PlaceMapPanel.svelte':moduleUrl('export default function() {}'),
      '$lib/components/WeatherDefaultPlace.svelte':moduleUrl('export default function() {}'),
      '$lib/components/AboutMno.svelte':moduleUrl('export default function() {}'),
      '$lib/place-map.svelte':moduleUrl('export const PLACE_MAP="mno:place-map";'),
      '$lib/language.svelte':moduleUrl('export const language={current:"sv"};export function changeLanguage() {}'),
      '$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href,
      '@tauri-apps/api/core':moduleUrl('export function invoke() {}'),
      '$app/navigation':moduleUrl('export function afterNavigate() {} export function goto() {}'),
      '$app/state':moduleUrl(`export const page={url:new URL(${JSON.stringify('https://mno.invalid'+path)})};`),
    };
    const component=await import(moduleUrl(resolveImports(compiled.js.code,imports)));
    return render(component.default, { context: new Map([['mno:place-map', { request:null, close() {} }]]) }).body;
  }
  const open=await panelHtml('/search?q=hackspett',true);
  assert.match(open,/<aside[^>]*class="[^"]*left-panel[^"]*search-docked/);
  assert.match(open,/<div[^>]*class="[^"]*panel-backdrop[^"]*search-docked/);
  assert.doesNotMatch(await panelHtml('/journal',true),/class="[^"]*search-docked/);
  assert.doesNotMatch(await panelHtml('/search?q=hackspett',false),/<aside|class="[^"]*search-docked/);
  const searchCss=compile(pageSource,{generate:'client'}).css.code;
  const navCss=compile(nav,{generate:'client'}).css.code;
  assert.match(searchCss,/@media\s*\(min-width:\s*1000px\)[\s\S]*body:has\(\.left-panel\.search-docked\)[^{]*\.search-space[^{}]*\{\s*padding-left:\s*360px/);
  assert.match(navCss,/@media\s*\(min-width:\s*1000px\)[\s\S]*\.panel-backdrop[^{}]*\.search-docked[^{}]*\{\s*display:\s*none/);
  assert.match(searchCss,/main[^{}]*\{\s*box-sizing:\s*border-box/);
});


test('return from a search hit reopens only that search panel with the original query',()=>{
  const h=navHarness({initial:'https://mno.invalid/search?q=%C3%B6rn%20%26%20skog'});
  h.run('activePanel="left"');
  h.returnTo('/journal/entries/entry?from=search&q=%C3%B6rn%20%26%20skog');
  assert.equal(h.run('activePanel'),null);
  h.returnTo('/search?q=%C3%B6rn%20%26%20skog');
  assert.equal(h.run('activePanel'),'left');assert.equal(h.run('searchText'),'örn & skog');
  h.returnTo('/journal/entries/entry');h.returnTo('/search?q=%C3%B6rn%20%26%20skog');
  assert.equal(h.run('activePanel'),null,'ordinary entry navigation must not open the panel');
  h.returnTo('/journal/entries/entry?from=search&q=other');h.returnTo('/search?q=new');
  assert.equal(h.run('activePanel'),null,'a different search is not the return link');h.dispose();
});
test('closing the panel and clearing the search never navigate home or reopen the panel',async()=>{
  const h=navHarness({initial:'https://mno.invalid/search?q=hackspett'});
  h.run('activePanel="left"; searchText="hackspett"; togglePanel("left")');
  assert.equal(h.calls.length,0);assert.equal(h.run('activePanel'),null);
  h.run('searchText=""; scheduleSearch()');await h.tick();
  assert.equal(h.calls.length,1);assert.equal(h.calls[0].url,'/search?q=');assert.equal(h.run('activePanel'),null);h.dispose();
});
