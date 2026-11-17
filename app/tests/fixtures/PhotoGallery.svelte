<script lang="ts">
 import {onMount} from 'svelte';
 import type {BirdList} from '../../src/lib/birds';
 import JournalPhotoScope from '../../src/lib/components/JournalPhotoScope.svelte';
 import JournalContent from '../../src/lib/components/JournalContent.svelte';
 import BirdJournalLists from '../../src/lib/components/BirdJournalLists.svelte';
 import BirdListDetail from '../../src/lib/components/BirdListDetail.svelte';
 import {DEFAULT_BIRD_NAMES} from '../../src/lib/bird-names';
 let {lists,doc}:{lists:BirdList[];doc:string}=$props();
 onMount(()=>{(window as unknown as {bubbledThumbs:number}).bubbledThumbs=0;const handler=(event:Event)=>{if((event.target as Element).closest('button')?.querySelector('img'))(window as unknown as {bubbledThumbs:number}).bubbledThumbs++;};const node=document.querySelector('#journal-zone');node?.addEventListener('click',handler);return()=>node?.removeEventListener('click',handler);});
</script>
<section id="journal-zone"><JournalPhotoScope entryId="entry" generation={7} contentJson={doc} locale="en"><JournalContent contentJson={doc} documentVersion={2} entryId="entry" generation={7} locale="en"/><BirdJournalLists entryId="entry" generation={7} locale="en"/></JournalPhotoScope></section>
<section id="observation-zone"><BirdListDetail list={lists[1]} origin="/observations" locale="en" preferences={DEFAULT_BIRD_NAMES} onPreferencesChange={()=>{}}/></section>
