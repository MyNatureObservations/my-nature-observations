<script lang="ts">
 import type {BirdList} from "$lib/birds";
 import {speciesLabel} from "$lib/birds";
 import {capitalizeCommonName,formatBirdDate} from "$lib/bird-names";
 import * as m from "$lib/paraglide/messages.js";
 let {list,locale,href=null,journalDate=null,onSelect=null,disabled=false,linked=false}:{list:BirdList;locale:"sv"|"en";href?:string|null;journalDate?:string|null;onSelect?:(()=>void)|null;disabled?:boolean;linked?:boolean}=$props();
</script>
{#snippet content()}
 <span class="visit"><strong>{list.place.name}</strong>{#if list.visitStart||list.visitEnd}<span>{' · '}{list.visitStart??"…"}–{list.visitEnd??"…"}</span>{/if}{#if list.visitDate!==journalDate}<span>{' · '}{formatBirdDate(list.visitDate,locale)}</span>{/if}</span>
 <span class="species-preview">{#each list.rows.slice(0,3) as row,i}{#if i>0}{' · '}{/if}{capitalizeCommonName(speciesLabel(row,locale),locale)} <b>{row.count}</b>{/each}{#if list.rows.length>3}{' · '}{m.bird_more_rows({count:list.rows.length-3}, {locale})}{/if}</span>
 {#if linked}<span class="linked">{m.bird_linked({}, {locale})}</span>{/if}
{/snippet}
{#if onSelect}<button class="bird-preview" type="button" {disabled} onclick={onSelect}>{@render content()}</button>
{:else if href}<a class="bird-preview" {href}>{@render content()}</a>{/if}
<style>
 .bird-preview{box-sizing:border-box;display:block;width:100%;padding:8px 4px;color:var(--text-muted,#686d63);text-decoration:none;text-align:left;border:0;border-radius:4px;background:transparent;font:13px/1.5 system-ui,sans-serif;cursor:pointer}.bird-preview:hover{background:#eef1eb}.visit,.species-preview,.linked{display:block}.visit strong{font-weight:500;color:var(--accent,#435b47)}.species-preview{margin-top:2px}b{font-weight:500;font-variant-numeric:tabular-nums;margin-left:2px}.linked{font-size:11px;margin-top:4px}.bird-preview:focus-visible{outline:2px solid #788e7b;outline-offset:3px}.bird-preview:disabled{cursor:default;opacity:.65}
</style>
