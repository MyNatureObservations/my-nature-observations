<script lang="ts">
 import PhotoThumbnail from "$lib/components/PhotoThumbnail.svelte";
 import {observationPhotos} from "$lib/photo-gallery";
 import {onMount} from "svelte";
 import { invoke } from "@tauri-apps/api/core";
 import { makeBirdReader, birdHref, entryHref, setBirdLink, type BirdList } from "$lib/birds";
 import {formatBirdDate} from "$lib/bird-names";
 import BirdVisitPreview from "$lib/components/BirdVisitPreview.svelte";
 import BirdLinkPicker from "$lib/components/BirdLinkPicker.svelte";
 import DeleteEntryDialog from "$lib/components/DeleteEntryDialog.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {entryId,generation,locale,allowCreate=false,journalDate=null}:{entryId:string;generation:number|null;locale:"sv"|"en";allowCreate?:boolean;journalDate?:string|null}=$props();
 let available=$state(false),failed=$state(false),lists=$state<BirdList[]>([]);
 const read=makeBirdReader();
 let revision=$state(0),linkPicker=$state(false),unlinkTarget=$state<BirdList|null>(null),unlinkBusy=$state(false),unlinkFailed=$state(false);
 async function unlink(){
  if(!unlinkTarget||generation===null||unlinkBusy)return;unlinkBusy=true;unlinkFailed=false;
  try{await setBirdLink(generation,entryId,unlinkTarget.id,false);unlinkTarget=null;window.dispatchEvent(new Event("mno:journal-changed"));}
  catch{unlinkFailed=true;unlinkTarget=null;}finally{unlinkBusy=false;}
 }
 function linked(){linkPicker=false;window.dispatchEvent(new Event("mno:journal-changed"));}
 onMount(()=>{const refresh=()=>revision++;window.addEventListener("mno:journal-changed",refresh);return()=>window.removeEventListener("mno:journal-changed",refresh);});
 $effect(()=>{revision;const id=entryId,g= generation;let cancelled=false;
  available=false;lists=[];failed=false;
  void (async()=>{try {const enabled=await invoke<boolean>("bird_observations_available");if(cancelled||!enabled||g===null)return;available=true;const response=await read<BirdList[]>("read_journal_bird_lists",{entryId:id,generation:g});if(!cancelled)lists=response.data;}catch{if(!cancelled)failed=true;}})();
  return ()=>{cancelled=true;};
 });
</script>
{#if available}
 <section class="bird-links" aria-label={m.bird_lists({}, {locale})}>
  {#if lists.length||allowCreate}<h2>{m.bird_lists({}, {locale})}</h2>{/if}
  {#if allowCreate}<p class="section-context">{m.bird_preview_context({}, {locale})}</p>{/if}
  {#if failed}<p role="alert">{m.bird_load_failed({}, {locale})}</p>{/if}
  {#each lists as list (list.id)}
   <div class="linked-visit"><BirdVisitPreview {list} {locale} {journalDate} href={birdHref(entryId,list.id)}/>
    {#if list.rows.some(row=>row.photo)}<div class="bird-photo-thumbnails">{#each observationPhotos([list],locale) as item (item.key)}<PhotoThumbnail {item} {locale} compact/>{/each}</div>{/if}
    {#if allowCreate}<button class="unlink" type="button" disabled={unlinkBusy} aria-label={m.bird_unlink_for({place:list.place.name,date:formatBirdDate(list.visitDate,locale)}, {locale})} onclick={()=>{unlinkTarget=list;unlinkFailed=false;}}>{m.bird_unlink({}, {locale})}</button>{/if}
   </div>
  {/each}
  {#if allowCreate&&generation!==null}<a class="create-birds" href={`${entryHref(entryId)}/birds/new`}>＋ {m.bird_create({}, {locale})}</a>{/if}
  {#if allowCreate&&generation!==null}<button class="create-birds" type="button" onclick={()=>linkPicker=true}>{m.bird_link_existing({}, {locale})}</button>{/if}
 </section>
 {#if linkPicker&&generation!==null}<BirdLinkPicker {entryId} {generation} {locale} linkedIds={lists.map(list=>list.id)} oncancel={()=>linkPicker=false} onlinked={linked}/>{/if}
 {#if unlinkTarget}<DeleteEntryDialog title={unlinkTarget.place.name} date={unlinkTarget.visitDate} {locale} busy={unlinkBusy} heading={m.bird_unlink({}, {locale})} description={m.bird_unlink_confirm({place:unlinkTarget.place.name,date:formatBirdDate(unlinkTarget.visitDate,locale)}, {locale})} oncancel={()=>{if(!unlinkBusy)unlinkTarget=null;}} onconfirm={()=>void unlink()}/>{/if}
 {#if unlinkFailed}<p role="alert">{m.bird_unlink_failed({}, {locale})}</p>{/if}
{/if}
<style>
 .bird-photo-thumbnails :global(button>img){max-height:76px;max-width:104px}
 .bird-photo-thumbnails{display:flex;flex-wrap:wrap;gap:10px;margin:6px 0 10px}
 .bird-links{margin-top:20px;color:var(--text-muted,#686d63);font:13px/1.5 system-ui,sans-serif}h2{font:12px system-ui,sans-serif;color:var(--accent,#435b47);margin:0 0 6px}.section-context{font:12px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin:0 0 6px}.linked-visit{margin:3px 0}.unlink{border:0;background:transparent;color:#686d63;font:11px system-ui,sans-serif;min-height:32px;padding:4px}.create-birds{display:inline-flex;align-items:center;min-height:40px;padding:0;margin-right:18px;border:0;background:transparent;text-decoration:underline;cursor:pointer;color:var(--accent,#435b47);font:12px system-ui,sans-serif}a:focus-visible,button:focus-visible{outline:2px solid #788e7b;outline-offset:3px}[role=alert]{color:#9a403b}
</style>
