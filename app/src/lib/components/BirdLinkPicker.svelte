<script lang="ts">
 import {onMount} from "svelte";
 import {makeBirdReader,setBirdLink,type BirdList} from "$lib/birds";
 import BirdVisitPreview from "$lib/components/BirdVisitPreview.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {entryId,generation,linkedIds,locale,oncancel,onlinked}:{entryId:string;generation:number;linkedIds:string[];locale:"sv"|"en";oncancel:()=>void;onlinked:()=>void}=$props();
 let lists=$state<BirdList[]>([]),loading=$state(true),failed=$state(false),busy=$state(false),saveError=$state(false);
 let cancelButton:HTMLButtonElement;let disposed=false;
 function mount(node:HTMLDialogElement){const previous=document.activeElement;node.showModal();cancelButton?.focus();return {destroy(){node.close();if(previous instanceof HTMLElement&&previous.isConnected)previous.focus({preventScroll:true});}};}
 onMount(()=>{void(async()=>{try{
  const result=await makeBirdReader()<BirdList[]>("read_bird_lists",{generation});if(!disposed)lists=result.data;
 }catch{if(!disposed)failed=true;}finally{if(!disposed)loading=false;}})();return()=>{disposed=true;};});
 async function link(list:BirdList){
  if(busy||linkedIds.includes(list.id))return;busy=true;saveError=false;
  try{await setBirdLink(generation,entryId,list.id,true);if(!disposed)onlinked();}
  catch{if(!disposed)saveError=true;}finally{if(!disposed)busy=false;}
 }
</script>
<dialog use:mount aria-labelledby="bird-link-heading" oncancel={event=>{event.preventDefault();if(!busy)oncancel();}}>
 <h2 id="bird-link-heading">{m.bird_link_existing({}, {locale})}</h2>
 <p>{m.bird_link_choose({}, {locale})}</p>
 {#if loading}<p role="status">{m.bird_loading({}, {locale})}</p>
 {:else if failed}<p role="alert">{m.bird_load_failed({}, {locale})}</p>
 {:else}<ul>{#each lists as list (list.id)}<li><BirdVisitPreview {list} {locale} onSelect={()=>void link(list)} linked={linkedIds.includes(list.id)} disabled={busy||linkedIds.includes(list.id)}/></li>{/each}</ul>{#if !lists.length}<p>{m.bird_no_lists({}, {locale})}</p>{/if}{/if}
 {#if saveError}<p role="alert">{m.bird_link_failed({}, {locale})}</p>{/if}
 <footer><button bind:this={cancelButton} type="button" disabled={busy} onclick={oncancel}>{m.bird_cancel({}, {locale})}</button></footer>
</dialog>
<style>
 dialog{box-sizing:border-box;width:min(580px,calc(100vw - 40px));max-height:80vh;padding:24px;border:1px solid #d9d8d1;border-radius:12px;background:#faf9f6;color:#343530;font:13px/1.5 system-ui,sans-serif}dialog::backdrop{background:rgba(39,43,38,.25)}h2{font:normal 23px Georgia,serif;margin:0 0 12px}ul{list-style:none;padding:0;max-height:45vh;overflow:auto}li{border-bottom:1px solid #e2e3dc}footer{display:flex;justify-content:flex-end;margin-top:16px}footer button{min-height:44px;padding:8px 14px;border:1px solid #d3d4ce;border-radius:6px;background:transparent;color:#435b47;font:13px system-ui,sans-serif}button:focus-visible{outline:2px solid #788e7b;outline-offset:3px}[role=alert]{color:#9a403b}
</style>
