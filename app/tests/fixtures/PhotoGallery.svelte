<script lang="ts">
 import {onMount} from 'svelte';
 import type {BirdList} from '../../src/lib/birds';
 import PlacePhotos from '../../src/lib/components/PlacePhotos.svelte';
 import JournalPhotoScope from '../../src/lib/components/JournalPhotoScope.svelte';
 import JournalContent from '../../src/lib/components/JournalContent.svelte';
 import BirdJournalLists from '../../src/lib/components/BirdJournalLists.svelte';
 import BirdListDetail from '../../src/lib/components/BirdListDetail.svelte';
 import {DEFAULT_BIRD_NAMES} from '../../src/lib/bird-names';
 let {lists,doc}:{lists:BirdList[];doc:string}=$props();
 const place = {id:'place',name:'Test lake',revision:'1',latitude:59,longitude:18};
 const entries = [{id:'entry',entryDate:'2026-11-18',title:'A journal note'}];
 let locale = $state<"en" | "sv">("en");
 let collection = $state("empty");
 const visits = $derived(collection === "mixed" ? [
  lists[0], lists[0], lists[1],
  ...Array.from({length:4}, (_, i) => ({...lists[0], id:'standalone-'+i})),
 ] : []);
 onMount(()=>{(window as unknown as {bubbledThumbs:number}).bubbledThumbs=0;const handler=(event:Event)=>{if((event.target as Element).closest('button')?.querySelector('img'))(window as unknown as {bubbledThumbs:number}).bubbledThumbs++;};const node=document.querySelector('#journal-zone');node?.addEventListener('click',handler);return()=>node?.removeEventListener('click',handler);});
</script>
<section id="journal-zone"><JournalPhotoScope entryId="entry" generation={7} contentJson={doc} locale="en"><JournalContent contentJson={doc} documentVersion={2} entryId="entry" generation={7} locale="en"/><BirdJournalLists entryId="entry" generation={7} locale="en"/></JournalPhotoScope></section>
<section id="observation-zone"><BirdListDetail list={lists[1]} origin="/observations" locale="en" preferences={DEFAULT_BIRD_NAMES} onPreferencesChange={()=>{}}/></section>

<section id="place-zone" hidden>
 <button id="gallery-locale" onclick={() => locale = locale === "en" ? "sv" : "en"}>Toggle language</button>
 <button id="gallery-small" onclick={() => collection = "small"}>Small</button>
 <button id="gallery-mixed" onclick={() => collection = "mixed"}>Mixed</button>
 <PlacePhotos place={{...place, entries:collection === "empty" ? [] : entries, visits}} generation={7} {locale}/>
</section>
