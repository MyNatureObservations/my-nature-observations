<script lang="ts">
 import type {Snippet} from "svelte";
 import PhotoScope from "$lib/components/PhotoScope.svelte";
 import {loadJournalGallery} from "$lib/journal-photo-gallery";
 let {entryId,generation,contentJson,locale,enabled=true,children}:{entryId:string;generation:number|null;contentJson:string;locale:"sv"|"en";enabled?:boolean;children:Snippet}=$props();
</script>
<PhotoScope {locale} {enabled} version={`${entryId}:${generation}:${contentJson}`} load={()=>loadJournalGallery(contentJson,entryId,generation,locale)}>{@render children()}</PhotoScope>
