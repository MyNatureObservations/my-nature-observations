<script lang="ts">
 import Home from '../../src/routes/+page.svelte';
 import CalendarGrid from '../../src/lib/components/CalendarGrid.svelte';
 import CalendarDayPanel from '../../src/lib/components/CalendarDayPanel.svelte';
 import BirdJournalLists from '../../src/lib/components/BirdJournalLists.svelte';
 let month=$state(new Date(2026,10,1)),selected=$state<string|null>(null);
</script>
<div style="width:300px"><CalendarGrid bind:displayedMonth={month} selectedDate={selected} markedDates={['2026-11-18','2026-11-20']} observationDates={['2026-11-19','2026-11-20']} observationsAvailable onSelectDate={date=>selected=date}/></div>
<output id="selected">{selected}</output>
{#if selected}<CalendarDayPanel date={selected} onClose={()=>selected=null}/>{/if}
<div id="journal-actions"><BirdJournalLists entryId="entry" generation={7} locale="sv" allowCreate/></div>

<div id="home-preview"><Home/></div>
