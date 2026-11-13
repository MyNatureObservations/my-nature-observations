<script lang="ts">
 import {displayBirdNames,formatBirdCount,type BirdNamePreferences} from "$lib/bird-names";
 import type {Species} from "$lib/birds";
 import * as m from "$lib/paraglide/messages.js";
 let {species,locale,preferences,count=null}:{species:Pick<Species,"english"|"swedish"|"scientific">;locale:"sv"|"en";preferences:BirdNamePreferences;count?:string|null}=$props();
 const names=$derived(displayBirdNames(species,locale,preferences));
</script>
<div class="species-names" class:with-count={count!==null}>
 <div class="identity">
  <div class="common-name">{#if count!==null}<small class="field-label">{m.bird_species_label({}, {locale})}</small>{/if}<span>{names.primary}</span></div>
  {#if names.other}<div class="additional">{names.other}</div>{/if}
  {#if names.scientific}<div class="additional scientific">{names.scientific}</div>{/if}
  {#if names.missingSwedish}<small>{m.bird_no_swedish({}, {locale})}</small>{/if}
 </div>
 {#if count!==null}<div class="count"><small class="field-label">{m.bird_count({}, {locale})}</small><span>{count==="1"?m.bird_individual_one({count:formatBirdCount(count,locale)}, {locale}):m.bird_individual_many({count:formatBirdCount(count,locale)}, {locale})}</span></div>{/if}
</div>
<style>
 .species-names{min-width:0;overflow-wrap:anywhere}
 .with-count{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,7.5rem);align-items:start;column-gap:16px}
 .identity{min-width:0}
 .common-name{font:19px/1.4 Georgia,"Times New Roman",serif;color:#343530}
 .count{text-align:right;font:500 15px/1.775 system-ui,sans-serif;font-variant-numeric:tabular-nums;color:#435b47}
 .additional,small{display:block;color:var(--text-muted,#686d63);font:13px/1.45 system-ui,sans-serif}
 .field-label{margin:0 0 4px;color:var(--text-muted,#686d63);font:11px/1.4 system-ui,sans-serif;letter-spacing:.04em}
 .additional{margin-top:3px}.scientific{font-style:italic}small{font-size:11px}
 @media(max-width:380px){.with-count{grid-template-columns:minmax(0,1fr) minmax(0,6.75rem);column-gap:12px}.count{font-size:14px}}
</style>
