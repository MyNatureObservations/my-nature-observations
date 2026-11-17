<script lang="ts">
 import {tick} from "svelte";
 import type {GalleryPhoto} from "$lib/photo-gallery";
 import PhotoAbout from "$lib/components/PhotoAbout.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {items,initialKey,trigger,locale,loading=false,error=false,incomplete=false,onclose}:{items:GalleryPhoto[];initialKey:string;trigger:HTMLElement|null;locale:"sv"|"en";loading?:boolean;error?:boolean;incomplete?:boolean;onclose:()=>void}=$props();
 let index=$state(0),failed=$state(false);
 const current=$derived(items[index]);
 $effect(()=>{index=Math.max(0,items.findIndex(item=>item.key===initialKey));});
 $effect(()=>{current?.key;failed=false;});
 function mount(node:HTMLDialogElement){node.showModal();node.querySelector<HTMLButtonElement>(".close")?.focus();return {destroy(){node.close();void tick().then(()=>{if(trigger?.isConnected)trigger.focus({preventScroll:true});});}};}
 function move(direction:number){index=Math.max(0,Math.min(items.length-1,index+direction));}
 function keydown(event:KeyboardEvent){
  if(event.key==="Tab"){
   const node=event.currentTarget as HTMLDialogElement;
   const controls=Array.from(node.querySelectorAll<HTMLElement>("button:not(:disabled),summary,a[href],input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex='0']")).filter(control=>control.getClientRects().length>0);
   const first=controls[0],last=controls[controls.length-1];
   if(first&&last&&((event.shiftKey&&document.activeElement===first)||(!event.shiftKey&&document.activeElement===last))){event.preventDefault();(event.shiftKey?last:first).focus();}return;
  }
  if(event.altKey||event.ctrlKey||event.metaKey||event.shiftKey)return;if(event.key==="ArrowLeft"||event.key==="ArrowRight"){event.preventDefault();event.stopPropagation();move(event.key==="ArrowLeft"?-1:1);}}
</script>
<dialog class="photo-viewer" use:mount aria-labelledby="photo-viewer-heading" onkeydown={keydown} oncancel={event=>{event.preventDefault();event.stopPropagation();onclose();}}>
 <header><h2 class="visually-hidden" id="photo-viewer-heading">{m.photo_photograph({}, {locale})}</h2><button type="button" class="close" onclick={onclose}>{m.photo_viewer_close({}, {locale})}</button></header>
 {#if loading}<p role="status">{m.photo_loading({}, {locale})}</p>
 {:else if error}<p role="alert">{m.photo_viewer_failed({}, {locale})}</p>
 {:else if current}
  <figure>{#if !current.src||failed}<p role="alert">{m.photo_unavailable({}, {locale})}</p>{:else}<img src={current.src} alt={current.alt} width={current.width} height={current.height} onerror={()=>failed=true}/>{/if}
   <figcaption>{#if current.caption}<p class="caption">{current.caption}</p>{/if}{#if current.description}<p>{current.description}</p>{/if}<small>{m.photo_stored_preview({}, {locale})}</small></figcaption>
  </figure>
  {#if incomplete}<p role="alert">{m.photo_viewer_incomplete({}, {locale})}</p>{/if}
  <nav aria-label={m.photo_viewer_navigation({}, {locale})}><button type="button" disabled={index===0} onclick={()=>move(-1)}>{m.photo_viewer_previous({}, {locale})}</button><span role="status" aria-live="polite">{m.photo_viewer_position({position:index+1,total:items.length},{locale})}</span><button type="button" disabled={index>=items.length-1} onclick={()=>move(1)}>{m.photo_viewer_next({}, {locale})}</button></nav>
  <PhotoAbout {locale}/>
 {/if}
</dialog>
<style>dialog{box-sizing:border-box;width:min(960px,calc(100vw - 40px));max-height:92vh;overflow:auto;padding:20px 24px;border:1px solid #d7d9d0;border-radius:10px;background:#faf9f6;color:#343530}dialog::backdrop{background:rgba(25,31,26,.65)}header{display:flex;align-items:center;justify-content:flex-end;gap:16px}.visually-hidden{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}button{min-height:40px;padding:7px 12px;border:1px solid #d3d7ce;border-radius:6px;background:transparent;color:#435b47;font:13px system-ui,sans-serif;cursor:pointer}button:disabled{opacity:.45;cursor:default}figure{margin:16px 0 0;text-align:center}img{display:block;margin:auto;max-width:100%;width:auto;height:auto;max-height:65vh;object-fit:contain}figcaption{font:13px/1.5 system-ui,sans-serif;color:#686d63;overflow-wrap:anywhere}.caption{font:17px/1.5 Georgia,serif;color:#343530;white-space:pre-wrap}figcaption p{margin:8px 0 0}small{display:block;margin-top:8px;font-size:11px}nav{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:16px;font:12px system-ui,sans-serif}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}[role=alert]{color:#9a403b}@media(max-width:540px){dialog{padding:16px;width:calc(100vw - 20px)}img{max-height:55vh}nav{gap:6px}button{padding:7px 9px}}</style>
