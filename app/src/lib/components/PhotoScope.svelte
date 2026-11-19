<script lang="ts">
 import {onMount,type Snippet} from "svelte";
 import {PHOTO_OPEN_EVENT,photoGalleryRequest,type GalleryPhoto} from "$lib/photo-gallery";
 import PhotoViewer from "$lib/components/PhotoViewer.svelte";
 let {load,version,locale,enabled=true,children}:{load:()=>Promise<{items:GalleryPhoto[];incomplete:boolean}>;version:string;locale:"sv"|"en";enabled?:boolean;children:Snippet}=$props();
 let shown=$state(false),loading=$state(false),error=$state(false),incomplete=$state(false),items=$state<GalleryPhoto[]>([]),initialKey=$state(""),trigger=$state<HTMLElement|null>(null);const request=photoGalleryRequest();
 function close(){request.invalidate();shown=false;items=[];}
 $effect(()=>{version;enabled;locale;close();});
 onMount(()=>{const changed=()=>close();window.addEventListener("mno:journal-changed",changed);return()=>{request.invalidate();window.removeEventListener("mno:journal-changed",changed);};});
 function mount(node:HTMLDivElement){const open=(event:Event)=>{const detail=(event as CustomEvent<{key:string;trigger:HTMLElement}>).detail;if(!enabled||!detail||!node.contains(detail.trigger))return;event.stopPropagation();const valid=request.start();initialKey=detail.key;trigger=detail.trigger;items=[];error=false;incomplete=false;loading=true;shown=true;void load().then(result=>{if(!valid())return;if(!result.items.some(item=>item.key===detail.key)){error=true;return;}items=result.items;incomplete=result.incomplete;}).catch(()=>{if(valid())error=true;}).finally(()=>{if(valid())loading=false;});};node.addEventListener(PHOTO_OPEN_EVENT,open);return {destroy(){node.removeEventListener(PHOTO_OPEN_EVENT,open);request.invalidate();}};}
</script>
<div class="photo-scope" data-photo-scope={enabled?"":undefined} use:mount>{@render children()}{#if shown}<PhotoViewer {items} {initialKey} {trigger} {locale} {loading} {error} {incomplete} onclose={close}/>{/if}</div>
<style>.photo-scope{display:contents}</style>
