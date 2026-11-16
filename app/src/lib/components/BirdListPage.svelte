<script lang="ts">
 import {onMount} from "svelte";
 import {goto} from "$app/navigation";
 import {invoke} from "@tauri-apps/api/core";
 import {language} from "$lib/language.svelte";
 import {makeBirdReader,entryHref,birdHref,deleteBirdList,birdListExists,type BirdList} from "$lib/birds";
 import {formatBirdDate,DEFAULT_BIRD_NAMES,loadBirdNamePreferences,saveBirdNamePreferences,type BirdNamePreferences} from "$lib/bird-names";
 import BirdJournalLinks from "$lib/components/BirdJournalLinks.svelte";
 import BirdListDetail from "$lib/components/BirdListDetail.svelte";
 import DeleteEntryDialog from "$lib/components/DeleteEntryDialog.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {entryId,listId}:{entryId:string|null;listId:string}=$props();
 let linkBusy=$state(false),linksRefreshing=$state(false),linksRefreshFailed=$state(false);
 let preferences=$state<BirdNamePreferences>({...DEFAULT_BIRD_NAMES}),preferencesSaved=$state(true);
 let available=$state<boolean|null>(null),list=$state<BirdList|null>(null),failed=$state(false),generation=$state<number|null>(null);
 let deleteDialog=$state(false),deleting=$state(false),deleteAttempted=$state(false),deleted=$state(false),deleteError=$state("");
 let returnDestination=$state("/journal?latest=true");let disposed=false;
 const origin=$derived(entryId?entryHref(entryId):"/observations");
 function setPreferences(p:BirdNamePreferences){preferences=p;preferencesSaved=saveBirdNamePreferences(p);}
 onMount(()=>{
  preferences=loadBirdNamePreferences();
  void(async()=>{try{
   const enabled=await invoke<boolean>("bird_observations_available");if(disposed)return;available=enabled;if(!enabled)return;
   const session=await invoke<{generation:number}>("new_journal_session");
   const response=await makeBirdReader()<BirdList>("read_journal_bird_list",{entryId:null,id:listId,generation:session.generation});
   if(!disposed){list=response.data;generation=response.generation;}
  }catch{if(!disposed)failed=true;}})();
  return()=>{disposed=true;};
 });
 async function linksChanged(){
  if(generation===null||disposed)return;linksRefreshing=true;linksRefreshFailed=false;
  try{const response=await makeBirdReader()<BirdList>("read_journal_bird_list",{entryId:null,id:listId,generation});if(!disposed)list=response.data;}
  catch{if(!disposed)linksRefreshFailed=true;}finally{if(!disposed)linksRefreshing=false;}
 }
 function requestDeletion(){if(list&&generation!==null&&!deleting&&!deleteAttempted&&!linkBusy&&!linksRefreshing&&!linksRefreshFailed)deleteDialog=true;}
 function cancelDeletion(){if(!deleting)deleteDialog=false;}
 async function finishDeletion(){
  deleted=true;if(!entryId)returnDestination="/observations";window.dispatchEvent(new Event("mno:journal-changed"));
  // A deleted originating entry is not a destination; use the existing reader.
  if(entryId)try {const entry=await makeBirdReader()<{id:string}|null>("get_journal_entry",{id:entryId});if(entry.data)returnDestination=origin;}catch{/* Safe journal-reader fallback. */}
  if(disposed)return;
  try{await goto(returnDestination);}catch{deleteError="navigation";}
 }
 async function verifyDeletion(){
  if(!list||generation===null)return;
  const exists=await birdListExists(generation,list.id);
  if(disposed)return;
  if(exists){deleteError="failed";return;}
  await finishDeletion();
 }
 async function confirmDeletion(){
  if(!deleteDialog||deleting||deleteAttempted||!list||generation===null)return;
  deleting=true;deleteAttempted=true;deleteError="";
  try{
   try{await deleteBirdList(generation,list.id,list.editToken);}catch(cause){console.error("bird-list: deletion response uncertain",cause);}
   // Read-only confirmation also resolves a lost response; never repeat DELETE.
   await verifyDeletion();
  }catch(cause){console.error("bird-list: deletion confirmation failed",cause);if(!disposed)deleteError="failed";}
  finally{if(!disposed){deleting=false;deleteDialog=false;}}
 }
 async function checkDeletion(){
  if(!deleteAttempted||deleting||deleted)return;deleting=true;
  try{await verifyDeletion();}catch{if(!disposed)deleteError="failed";}finally{if(!disposed)deleting=false;}
 }
</script>
<svelte:head><title>{m.bird_list({}, {locale:language.current})} — MNO</title></svelte:head>
<main>
 {#if available===false}<a href={origin}>← {entryId?m.bird_back_entry({}, {locale:language.current}):m.bird_overview({}, {locale:language.current})}</a><p>{m.bird_unavailable({}, {locale:language.current})}</p>
 {:else if failed}<a href={origin}>← {entryId?m.bird_back_entry({}, {locale:language.current}):m.bird_overview({}, {locale:language.current})}</a><p role="alert">{m.bird_load_failed({}, {locale:language.current})}</p>
 {:else if list}
  <BirdListDetail {list} {origin} standalone={!entryId} editHref={`${birdHref(entryId,list.id)}/edit`} onDelete={requestDeletion} actionsDisabled={deleting||deleteAttempted||linkBusy||linksRefreshing||linksRefreshFailed} locale={language.current} {preferences} {preferencesSaved} onPreferencesChange={setPreferences}>
   {#snippet links()}{#if generation!==null}<BirdJournalLinks {listId} {generation} locale={language.current} disabled={deleting||deleteAttempted||linksRefreshing||linksRefreshFailed} onchanged={linksChanged} onbusy={busy=>linkBusy=busy}/>{/if}{/snippet}
  </BirdListDetail>
  {#if linksRefreshFailed}<p role="alert">{m.bird_link_refresh_failed({}, {locale:language.current})}</p>{/if}
  {#if deleteError}<p role="alert">{deleted?m.bird_deleted_navigation_failed({}, {locale:language.current}):m.bird_delete_failed({}, {locale:language.current})}</p>
   {#if deleted}<a href={returnDestination}>{entryId?m.bird_back_entry({}, {locale:language.current}):m.bird_overview({}, {locale:language.current})}</a>
   {:else}<button type="button" disabled={deleting} onclick={()=>void checkDeletion()}>{m.bird_check_deletion({}, {locale:language.current})}</button>{/if}
  {/if}
 {:else}<a href={origin}>← {entryId?m.bird_back_entry({}, {locale:language.current}):m.bird_overview({}, {locale:language.current})}</a><p role="status">{m.bird_loading({}, {locale:language.current})}</p>{/if}
</main>
{#if deleteDialog&&list}
 <DeleteEntryDialog title={list.place.name} date={list.visitDate} locale={language.current} busy={deleting}
  heading={m.bird_delete({}, {locale:language.current})}
  description={m.bird_delete_confirm({place:list.place.name,date:formatBirdDate(list.visitDate,language.current),times:list.visitStart||list.visitEnd?` · ${list.visitStart??"…"}–${list.visitEnd??"…"}`:""}, {locale:language.current})}
  oncancel={cancelDeletion} onconfirm={()=>void confirmDeletion()}/>
{/if}
<style>main{box-sizing:border-box;max-width:824px;margin:0 auto;padding:24px 32px 60px;color:#343530;font-family:Georgia,"Times New Roman",serif}a{color:#435b47;font:13px system-ui,sans-serif;display:inline-flex;align-items:center;min-height:44px}a:focus-visible{outline:2px solid #788e7b;outline-offset:3px}@media(max-width:600px){main{padding:20px 24px 40px}}</style>
