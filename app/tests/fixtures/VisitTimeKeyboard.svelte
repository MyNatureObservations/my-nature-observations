<script lang="ts">
 import VisitTimePicker from "$lib/components/VisitTimePicker.svelte";
 let value=$state("");
</script>
<button id="before" type="button">Before</button>
<VisitTimePicker id="test-time" label="From" locale="en" bind:value/>
<button id="after" type="button">After</button>
<output id="current-time">{value}</output>
