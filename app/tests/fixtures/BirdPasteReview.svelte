<script lang="ts">
 import BirdPasteReview from '../../src/lib/components/BirdPasteReview.svelte';
 import type {BirdRow} from '../../src/lib/birds';
 let open=$state(false), rows=$state<BirdRow[]>([]);
</script>
<button id="open" onclick={()=>open=true}>Paste</button>
<output id="rows">{JSON.stringify(rows)}</output>
{#if open}<BirdPasteReview existingRows={rows} locale="en" oncancel={()=>open=false} onapply={next=>{rows=next;open=false;return true;}}/>{/if}
