<script lang="ts">
 import {onMount,tick} from "svelte";
 import {invoke} from "@tauri-apps/api/core";
 import {validBirdPhotoCaption,speciesLabel,type BirdRow,type BirdPhoto} from "$lib/birds";
 import {birdPhotoTask} from "$lib/bird-photo";
 import PhotoAbout from "$lib/components/PhotoAbout.svelte";
 import BirdPhotoView from "$lib/components/BirdPhotoView.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {row,generation,locale,disabled=false,onchange,onpending}:{row:BirdRow;generation:number|null;locale:"sv"|"en";disabled?:boolean;onchange:()=>void;onpending:(amount:number)=>void}=$props();
 let busy=$state(false),error=$state(false);let pickerButton=$state<HTMLButtonElement>();let disposed=false;let task=birdPhotoTask();let release:(()=>void)|null=null;
 onMount(()=>()=>{disposed=true;task.dispose();release?.();});
 async function choose(){
  if(disabled||busy||generation===null)return;
  const target=row,g=generation,owner=row.rowId??crypto.randomUUID(),valid=task.start();
  const current=()=>valid()&&row===target&&generation===g&&!disabled;
  busy=true;error=false;onpending(1);let released=false;
  const done=()=>{if(!released){released=true;onpending(-1);busy=false;}};release=done;
  try{
   const path=await invoke<string|null>("select_journal_photograph",{locale:locale});if(!path||!current())return;
   const photo=await invoke<BirdPhoto>("prepare_bird_photograph",{generation:g,path,rowId:owner});if(!current())return;
   const latest=await invoke<{generation:number}>("new_journal_session");if(!current())return;
   if(latest.generation!==g||photo.row_id!==owner)throw Error("bird_photo_session_changed");
   photo.caption=row.photo?.caption??null;row.rowId=owner;row.photo=photo;onchange();
  }catch{if(current())error=true;}finally{done();await tick();if(current())pickerButton?.focus({preventScroll:true});}
 }
 function setCaption(value:string){if(disabled||!row.photo)return;row.photo.caption=value||null;onchange();}
 function remove(){if(disabled)return;task.cancel();release?.();row.photo=null;error=false;onchange();void tick().then(()=>{if(!disposed)pickerButton?.focus({preventScroll:true});});}
</script>
<div class="row-photo" role="group" aria-label={m.bird_photo_alt({species:speciesLabel(row,locale)},{locale})} aria-busy={busy}>
 {#if row.photo}<BirdPhotoView photo={row.photo} species={speciesLabel(row,locale)} {locale} compact showAbout={false}/>{/if}
 <div class="photo-actions">
  <button bind:this={pickerButton} type="button" disabled={disabled||busy} onclick={()=>void choose()}>{row.photo?m.bird_photo_replace({}, {locale}):m.bird_photo_add({}, {locale})}</button>
  {#if row.photo}<button type="button" {disabled} onclick={remove}>{m.bird_photo_remove({}, {locale})}</button>{/if}
 </div>
 {#if row.photo}
 <label for={`bird-caption-${row.rowId}`}>{m.photo_caption({}, {locale})}</label>
 <textarea id={`bird-caption-${row.rowId}`} value={row.photo.caption??""} {disabled} rows="2" maxlength="4000" aria-invalid={!validBirdPhotoCaption(row.photo.caption)} aria-describedby={!validBirdPhotoCaption(row.photo.caption)?`bird-caption-error-${row.rowId}`:undefined} oninput={(event)=>setCaption(event.currentTarget.value)}></textarea>
 {#if !validBirdPhotoCaption(row.photo.caption)}<p class="error" id={`bird-caption-error-${row.rowId}`} role="alert">{m.bird_photo_caption_invalid({}, {locale})}</p>{/if}
 <PhotoAbout {locale}/>
 {/if}
 {#if busy}<p role="status">{m.photo_processing({}, {locale})}</p>{/if}
 {#if error}<p class="error" role="alert">{m.bird_photo_failed({}, {locale})}</p>{/if}
</div>
<style>label{display:block;font:12px system-ui,sans-serif;color:var(--text-muted,#686d63);margin:8px 0 4px}textarea{display:block;box-sizing:border-box;width:100%;max-width:36rem;padding:8px;border:1px solid var(--border,#d9d9cd);border-radius:5px;background:var(--surface,#faf9f5);color:inherit;font:inherit;resize:vertical}.row-photo{margin-top:8px}.photo-actions{display:flex;flex-wrap:wrap;gap:14px}button{border:0;background:transparent;padding:4px 0;min-height:36px;color:var(--accent,#435b47);text-decoration:underline;font:12px system-ui,sans-serif;cursor:pointer}p{font:12px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin:4px 0}.error{color:#9a403b}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}:disabled{opacity:.6;cursor:default}</style>
