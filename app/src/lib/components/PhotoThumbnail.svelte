<script lang="ts">
 import type {GalleryPhoto} from "$lib/photo-gallery";
 import {PHOTO_OPEN_EVENT} from "$lib/photo-gallery";
 import PhotoViewer from "$lib/components/PhotoViewer.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {item,locale,compact=false,journal=false}:{journal?:boolean;item:GalleryPhoto;locale:"sv"|"en";compact?:boolean}=$props();
 let failed=$state(false),shown=$state(false),trigger=$state<HTMLElement|null>(null);
 $effect(()=>{item.key;item.src;failed=false;shown=false;});
 function activate(node:HTMLButtonElement){node.addEventListener("click",open);return {destroy(){node.removeEventListener("click",open);}};}
 function open(event:MouseEvent){event.preventDefault();event.stopPropagation();const node=event.currentTarget as HTMLElement;if(node.closest("[data-photo-scope]")){node.dispatchEvent(new CustomEvent(PHOTO_OPEN_EVENT,{bubbles:true,detail:{key:item.key,trigger:node}}));}else{trigger=node;shown=true;}}
</script>
{#if !item.src||failed}<p role="status">{m.photo_unavailable({}, {locale})}</p>{:else}<button type="button" class:compact class:journal aria-haspopup="dialog" aria-label={m.photo_viewer_open({description:item.description||item.caption||item.alt||m.photo_photograph({}, {locale})},{locale})} use:activate><img src={item.src} width={item.width} height={item.height} alt={item.alt} onerror={()=>failed=true}/></button>{/if}
{#if shown}<PhotoViewer items={[item]} initialKey={item.key} {trigger} {locale} onclose={()=>shown=false}/>{/if}
<style>button{display:block;max-width:100%;padding:0;border:0;background:transparent;border-radius:6px;cursor:zoom-in}img{display:block;width:auto;height:auto;max-width:100%;max-height:340px;object-fit:contain;border-radius:6px}.journal{width:100%}.journal img{width:100%;max-height:none}.compact img{max-height:130px;max-width:180px}button:focus-visible{outline:2px solid #788e7b;outline-offset:4px}p{font:12px system-ui,sans-serif;color:#9a403b}</style>
