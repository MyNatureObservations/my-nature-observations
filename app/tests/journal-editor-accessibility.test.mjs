import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { compile, parse } from 'svelte/compiler';
import { harness } from './journal-editor-test-harness.mjs';
import * as messages from '../src/lib/paraglide/messages.js';
const source=readFileSync(new URL('../src/lib/components/JournalEditor.svelte',import.meta.url),'utf8');

async function focusHarness() {
  const doc={activeElement:null};
  class Element {
    isConnected=true;
    disabled=false;
    focuses=0;
    focus(){this.focuses++;doc.activeElement=this;}
  }
  const previous=new Element(), keep=new Element(), discard=new Element();
  previous.focus();
  class Dialog extends Element {
    open=false;
    shows=0;
    closes=0;
    showModal(){this.open=true;this.shows++;}
    close(){this.open=false;this.closes++;}
    querySelector(){return keep;}
    querySelectorAll(){return [keep,discard].filter(b=>!b.disabled);}
  }
  const dialog=new Dialog();
  const h=await harness({browserGlobals:{document:doc,HTMLElement:Element,fixtureDialog:dialog},messages});
  function key(key,shiftKey=false) {
    const event={key,shiftKey,currentTarget:dialog,prevented:false,stopped:false,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;}};
    // The real handler runs in the component's VM, with this event passed through a global.
    dialog.event=event;
    h.run('leaveDialogKeys(fixtureDialog.event)');
    return event;
  }
  const mount=()=>h.run('globalThis.focusLifecycle = mountLeaveDialog(fixtureDialog)');
  const unmount=()=>h.run('focusLifecycle.destroy()');
  return {h,doc,previous,keep,discard,dialog,key,mount,unmount};
}

test('native modal semantics and labels remain on the actual leave dialog',()=>{
  compile(source,{generate:'client'});
  const ast=parse(source,{modern:true}),nodes=[];
  function walk(v){if(!v||typeof v!=='object')return;if(v.type==='RegularElement')nodes.push(v);for(const c of Object.values(v))if(Array.isArray(c))c.forEach(walk);else if(c&&typeof c==='object')walk(c);}
  walk(ast.fragment);
  const dialog=nodes.find(n=>n.name==='dialog');assert.ok(dialog);
  const html=source.slice(dialog.start,dialog.end);
  for(const value of ['use:mountLeaveDialog','role="alertdialog"','aria-modal="true"','aria-labelledby="leave-dialog-title"','aria-describedby="leave-dialog-description"','oncancel={cancelLeaveDialog}','onkeydown={leaveDialogKeys}','onclick={cancelLeaveConfirmation}','onclick={leaveWithoutSaving}'])assert.ok(html.includes(value),value);
  assert.match(html,/class="leave-dialog leave-overlay"/);
  assert.doesNotMatch(html,/autofocus/);
});
test('opening focuses the safe choice; Tab and Shift+Tab cycle only enabled dialog buttons',async()=>{
  const f=await focusHarness();f.mount();assert.equal(f.dialog.open,true);assert.equal(f.dialog.shows,1);assert.equal(f.doc.activeElement,f.keep);
  for(const [shift,target] of [[false,f.discard],[false,f.keep],[true,f.discard],[true,f.keep]]){
    const event=f.key('Tab',shift);assert.ok(event.prevented&&event.stopped);assert.equal(f.doc.activeElement,target);
  }
  f.discard.disabled=true;f.key('Tab');assert.equal(f.doc.activeElement,f.keep);f.key('Tab',true);assert.equal(f.doc.activeElement,f.keep);
  f.unmount();f.h.cleanup();
});
test('closing restores a connected prior control; a removed prior control is not focused',async()=>{
  for(const connected of [true,false]){
    const f=await focusHarness();f.mount();f.previous.isConnected=connected;const count=f.previous.focuses;
    f.unmount();assert.equal(f.dialog.open,false);assert.equal(f.dialog.closes,1);assert.equal(f.previous.focuses,count+(connected?1:0));
    if(connected)assert.equal(f.doc.activeElement,f.previous);
    f.h.cleanup();
  }
});
test('Escape cancels navigation/close confirmation without discarding text, writes or history',async()=>{
  const f=await focusHarness();f.h.edit('B');assert.equal(f.h.canLeave(),false);await f.h.canClose();f.mount();
  const event=f.key('Escape');assert.ok(event.prevented&&event.stopped);assert.equal(f.h.run('showLeaveConfirmation'),false);assert.equal(f.h.run('pendingClose'),false);assert.equal(f.h.run('isLeavingWithoutSaving'),false);
  assert.equal(f.h.text(),'B');assert.equal(f.h.destroyed(),0);assert.equal(f.h.closeCalls(),0);
  f.unmount();assert.equal(f.doc.activeElement,f.previous);
  await f.h.tick();assert.equal(f.h.writes.length,1);await f.h.succeed(0);assert.equal(f.h.pending(),false);f.h.cleanup();
});
test('Escape does not revoke an explicit discard already waiting for a save',async()=>{
  const f=await focusHarness();f.h.edit('B');await f.h.tick();assert.equal(await f.h.canClose(),false);f.mount();
  const leaving=f.h.run('leaveWithoutSaving()');assert.equal(f.h.run('isLeavingWithoutSaving'),true);
  const event=f.key('Escape');assert.ok(event.prevented);assert.equal(f.h.run('showLeaveConfirmation'),true);assert.equal(f.h.run('pendingClose'),true);
  await f.h.succeed(0);await leaving;assert.equal(f.h.destroyed(),1);assert.equal(f.h.writes.length,1);f.unmount();f.h.cleanup();
});
test('native cancel uses the same safe cancellation as Continue editing',async()=>{
  const f=await focusHarness();f.h.edit('B');await f.h.canClose();f.mount();
  f.dialog.event={prevented:false,preventDefault(){this.prevented=true;}};
  f.h.run('cancelLeaveDialog(fixtureDialog.event)');assert.ok(f.dialog.event.prevented);assert.equal(f.h.run('showLeaveConfirmation'),false);
  f.h.run('showLeaveConfirmation=true;pendingClose=true;cancelLeaveConfirmation()');assert.equal(f.h.run('showLeaveConfirmation'),false);assert.equal(f.h.run('pendingClose'),false);
  f.unmount();f.h.cleanup();
});
test('actual TipTap attributes provide a single localized editable-surface name in sv/en',async()=>{
  const h=await harness({messages});
  for(const [locale,label] of [['sv','Anteckningens text'],['en','Journal entry text'],['sv','Anteckningens text']]){
    h.run(`language.current=${JSON.stringify(locale)}`);
    assert.deepEqual({...h.editorOptions.editorProps.attributes()},{'aria-label':label,'aria-multiline':'true'});
    assert.equal(h.writes.length,0);assert.equal(h.text(),'A');assert.equal(h.pending(),false);
  }
  assert.doesNotMatch(source,/<article[^>]*aria-label/);
  assert.match(source,/const locale = language.current;[\s\S]*current\.view\.dispatch\(current\.state\.tr\.setMeta\("addToHistory", false\)\)/);
  h.cleanup();
});
