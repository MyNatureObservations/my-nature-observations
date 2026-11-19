<script lang="ts">
 import type {BirdPhoto} from "$lib/birds";
 import PhotoThumbnail from "$lib/components/PhotoThumbnail.svelte";
 import PhotoAbout from "$lib/components/PhotoAbout.svelte";
 import type {GalleryPhoto} from "$lib/photo-gallery";
 import {birdPhotoUrl} from "$lib/bird-photo";
 import * as m from "$lib/paraglide/messages.js";
 let {photo,species,locale,compact=false,showAbout=true,item=null}:{item?:GalleryPhoto|null;photo:BirdPhoto;species:string;locale:"sv"|"en";compact?:boolean;showAbout?:boolean}=$props();
 const displayed=$derived(item??{key:`bird-photo:${photo.asset_id}`,src:birdPhotoUrl(photo),alt:m.bird_photo_alt({species},{locale}),caption:photo.caption??"",description:species,width:photo.width,height:photo.height});
</script>
<figure class:compact><PhotoThumbnail item={displayed} {locale} {compact}/>{#if showAbout}<PhotoAbout {locale}/>{/if}</figure>
<style>figure{margin:0;min-width:0}figure.compact{margin-top:10px}figure :global(img){max-height:200px}figure.compact :global(img){max-height:130px}</style>
