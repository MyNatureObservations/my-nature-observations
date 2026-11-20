<script lang="ts">
 import {onMount} from "svelte";
 import {makeBirdReader,setBirdLink,entryHref,birdJournalLabel,type BirdJournalEntry} from "$lib/birds";
 import {formatBirdDate} from "$lib/bird-names";
 import DeleteEntryDialog from "$lib/components/DeleteEntryDialog.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {listId,generation,locale,disabled=false,onchanged,onbusy=()=>{}}:{listId:string;generation:number;locale:"sv"|"en";disabled?:boolean;onchanged:()=>Promise<void>;onbusy?:(busy:boolean)=>void}=$props();
 let entries=$state<BirdJournalEntry[]>([]),loading=$state(true),failed=$state(false),busy=$state(false),saveError=$state(false),choosing=$state(false),unlinkTarget=$state<BirdJournalEntry|null>(null);
 let disposed=false,revision=0;
 const label=(entry:BirdJournalEntry)=>birdJournalLabel(entry,m.search_untitled({}, {locale}));
 async function load(){
  const request=++revision;failed=false;
  try{const result=await makeBirdReader()<BirdJournalEntry[]>("read_bird_journal_entries",{generation,id:listId});if(!disposed&&request===revision)entries=result.data;}
  catch{if(!disposed&&request===revision)failed=true;}
  finally{if(!disposed&&request===revision)loading=false;}
 }
 onMount(()=>{void load();return()=>{disposed=true;};});
 function mountChooser(node:HTMLDialogElement){
  const previous=document.activeElement;node.showModal();node.querySelector<HTMLButtonElement>("footer button")?.focus();
  return {destroy(){node.close();if(previous instanceof HTMLElement&&previous.isConnected)previous.focus({preventScroll:true});}};
 }
 function cancelChoice(){if(!busy)choosing=false;}
 function requestUnlink(entry:BirdJournalEntry){if(!busy&&!disabled){unlinkTarget=entry;saveError=false;}}
 function cancelUnlink(){if(!busy)unlinkTarget=null;}
 async function change(entry:BirdJournalEntry,linked:boolean){
  if(busy||disabled||(linked&&entry.linked))return;
  busy=true;saveError=false;onbusy(true);
  try{
   await setBirdLink(generation,entry.id,listId,linked);
   if(disposed)return;
   choosing=false;unlinkTarget=null;
   await load();await onchanged();
   window.dispatchEvent(new Event("mno:journal-changed"));
  }catch{if(!disposed){saveError=true;unlinkTarget=null;}}
  finally{if(!disposed)busy=false;onbusy(false);}
 }
 async function confirmUnlink(){if(unlinkTarget)await change(unlinkTarget,false);}
 $effect(()=>{
  const beforeDisable=(event:Event)=>{if(busy)(event as CustomEvent<import("$lib/bird-module.svelte").DisableRequest>).detail.waitUntil(Promise.resolve(false));};
  window.addEventListener("mno:before-bird-disable",beforeDisable);
  return()=>window.removeEventListener("mno:before-bird-disable",beforeDisable);
 });
</script>
<section class="journal-links" aria-label={m.bird_journal_links({}, {locale})}>
 <h2>{m.bird_journal_links({}, {locale})}</h2>
 {#if loading}<p role="status">{m.bird_loading({}, {locale})}</p>
 {:else if failed}<p role="alert">{m.bird_links_load_failed({}, {locale})}</p>
 {:else}
  <ul>{#each entries.filter(entry=>entry.linked) as entry (entry.id)}<li>
   <a href={entryHref(entry.id)}><time datetime={entry.entryDate}>{formatBirdDate(entry.entryDate,locale)}</time><span>{label(entry)}</span></a>
   <button class="unlink" type="button" disabled={disabled||busy} aria-label={m.bird_unlink_entry_for({title:label(entry),date:formatBirdDate(entry.entryDate,locale)}, {locale})} onclick={()=>requestUnlink(entry)}>{m.bird_unlink({}, {locale})}</button>
  </li>{/each}</ul>
  {#if !entries.some(entry=>entry.linked)}<p>{m.bird_no_journal_links({}, {locale})}</p>{/if}
 {/if}
 <button class="link-entry" type="button" disabled={disabled||busy||loading||failed} onclick={()=>{choosing=true;saveError=false;}}>{m.bird_link_entry({}, {locale})}</button>
 {#if saveError}<p role="alert">{m.bird_link_failed({}, {locale})}</p>{/if}
</section>
{#if choosing}
 <dialog use:mountChooser aria-labelledby="bird-journal-chooser-heading" oncancel={event=>{event.preventDefault();cancelChoice();}}>
  <h2 id="bird-journal-chooser-heading">{m.bird_link_entry({}, {locale})}</h2>
  <p>{m.bird_choose_journal_entry({}, {locale})}</p>
  <ul class="entry-choices">{#each entries as entry (entry.id)}<li><button type="button" class="entry-choice" disabled={busy||disabled||entry.linked} onclick={()=>void change(entry,true)}>
   <time datetime={entry.entryDate}>{formatBirdDate(entry.entryDate,locale)}</time><span>{label(entry)}</span>{#if entry.linked}<small>{m.bird_linked({}, {locale})}</small>{/if}
  </button></li>{/each}</ul>
  {#if !entries.length}<p>{m.bird_no_journal_entries({}, {locale})}</p>{/if}
  {#if saveError}<p role="alert">{m.bird_link_failed({}, {locale})}</p>{/if}
  <footer><button type="button" disabled={busy} onclick={cancelChoice}>{m.bird_cancel({}, {locale})}</button></footer>
 </dialog>
{/if}
{#if unlinkTarget}
 <DeleteEntryDialog title={label(unlinkTarget)} date={unlinkTarget.entryDate} {locale} {busy} heading={m.bird_unlink({}, {locale})} description={m.bird_unlink_entry_confirm({title:label(unlinkTarget),date:formatBirdDate(unlinkTarget.entryDate,locale)}, {locale})} oncancel={cancelUnlink} onconfirm={()=>void confirmUnlink()}/>
{/if}
<style>
 .journal-links{max-width:560px;margin:24px 0;color:var(--text-muted,#686d63);font:12px/1.5 system-ui,sans-serif}h2{font:13px system-ui,sans-serif;color:var(--accent,#435b47);margin:0 0 8px}ul{list-style:none;padding:0;margin:0}li{display:flex;align-items:center;gap:12px;padding:4px 0}a{flex:1;min-width:0;color:#435b47;text-decoration:none;min-height:36px}a:hover{text-decoration:underline}time,li span{display:block}time{font-size:11px;color:#686d63}li span{overflow-wrap:anywhere}.unlink,.link-entry{border:0;background:transparent;color:#435b47;font:12px system-ui,sans-serif;min-height:36px;padding:4px 0;cursor:pointer}.link-entry{text-decoration:underline;margin-top:6px}.unlink{flex:none;color:#686d63}.entry-choices{max-height:45vh;overflow:auto}.entry-choices li{display:block;border-bottom:1px solid #e2e3dc}.entry-choice{display:block;width:100%;border:0;border-radius:4px;background:transparent;padding:10px 8px;text-align:left;color:#343530;font:13px/1.5 system-ui,sans-serif;cursor:pointer}.entry-choice:hover{background:#eef1eb}.entry-choice:disabled{opacity:.65;cursor:default}small{color:#686d63}dialog{box-sizing:border-box;width:min(560px,calc(100vw - 40px));max-height:80vh;padding:24px;border:1px solid #d9d8d1;border-radius:12px;background:#faf9f6;color:#343530;font:13px/1.5 system-ui,sans-serif}dialog::backdrop{background:rgba(39,43,38,.25)}dialog h2{font:normal 23px Georgia,serif}footer{display:flex;justify-content:flex-end;margin-top:16px}footer button{min-height:44px;border:1px solid #d3d4ce;border-radius:6px;background:transparent;padding:8px 14px;color:#435b47;font:13px system-ui,sans-serif}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}button:disabled{cursor:default;opacity:.65}[role=alert]{color:#9a403b}
</style>
