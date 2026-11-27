<script lang="ts">
 import type {BirdList,EbirdRow} from '$lib/birds';
 import {formatSourceDate,formatSourceTime,sortSourceSpecies,sourceSpeciesName} from '$lib/ebird-presentation';
 import {mediaHref,mediaId} from '$lib/ebird-preview';
 import {openUrl} from '@tauri-apps/plugin-opener';
 import * as m from '$lib/paraglide/messages.js';
 let {list,origin,locale,links,mapAction}:{list:BirdList;origin:string;locale:'sv'|'en';links?:import('svelte').Snippet;mapAction?:import('svelte').Snippet}=$props();
 let failed=$state(false);
 async function external(url:string){
  failed=false;
  try{await openUrl(url);}
  catch{failed=true;}
 }
 function openMediaLink(event:MouseEvent){
  event.preventDefault();
  void external((event.currentTarget as HTMLAnchorElement).href);
 }
</script>
{#snippet rowContent(row:EbirdRow)}
 {@const ids=(row.fields['ML Catalog Numbers']??'').trim().split(/\s+/).filter(id=>mediaId(id))}
 <div class="identity"><strong>{row.fields['Common Name']}</strong><p class="scientific">{row.fields['Scientific Name']}</p></div>
 <span class="count">{row.fields.Count==='X'?m.ebird_unspecified({}, {locale}):row.fields.Count}</span>
 {#if row.fields['Observation Details']}<p class="extra">{m.ebird_species_comment({}, {locale})}: {row.fields['Observation Details']}</p>{/if}
 {#each row.additions as addition}<p class="extra">{m.ebird_own_addition({}, {locale})}: {addition}</p>{/each}
 {#if ids.length===1}
  <button type="button" class="card-media" aria-label={m.ebird_species_media({species:sourceSpeciesName(row)}, {locale})}
   onclick={()=>external(mediaHref(ids[0]))}>Media ↗</button>
 {:else if ids.length>1}
  <div class="media-line">
   {#each ids as id,i}
    <a href={mediaHref(id)} onclick={openMediaLink}
     onfocus={event=>event.currentTarget.scrollIntoView({block:'nearest',inline:'nearest'})}>
     {m.ebird_media_link({number:i+1}, {locale})} ↗
    </a>
   {/each}
  </div>
 {/if}
{/snippet}
{#if list.source}
 <a href={origin}>← {origin==='/observations'?m.bird_overview({}, {locale}):m.bird_back_entry({}, {locale})}</a>
 <p class="source-label">{m.ebird_from({}, {locale})}</p>
 <h1>{list.place.name}</h1>
 <p class="metadata">{formatSourceDate(list.visitDate,locale)}{#if list.source.fields.Time}{' · '}{formatSourceTime(list.source.fields.Time,locale)}{/if}</p>
 {@render mapAction?.()}
 {#if /^S[1-9]\d*$/.test(list.source.submissionId)}<button type="button" onclick={()=>external(`https://ebird.org/checklist/${list.source!.submissionId}`)}>{m.ebird_original({}, {locale})} ↗</button>{/if}
 {#if failed}<p role="alert">{m.ebird_link_failed({}, {locale})}</p>{/if}
 <ul>{#each sortSourceSpecies(list.source.rows,locale,sourceSpeciesName) as row}<li>{@render rowContent(row)}</li>{/each}</ul>
 {#if list.source.unresolved.length}<details class="review"><summary>{m.ebird_review_additions({}, {locale})} ({list.source.unresolved.length})</summary><p>{m.ebird_review_reason({}, {locale})}</p><ul>{#each sortSourceSpecies(list.source.unresolved,locale,sourceSpeciesName) as row}<li>{@render rowContent(row)}</li>{/each}</ul></details>{/if}
 {#each list.source.additions as addition}<p class="note">{m.ebird_own_addition({}, {locale})}: {addition}</p>{/each}
 {#if links}{@render links()}{/if}
 <details class="source-details"><summary>{m.ebird_source_details({}, {locale})}</summary><p>{m.ebird_source_locked({}, {locale})}</p><p>{m.ebird_source_place({}, {locale})}</p><p>{m.ebird_catalog_note({}, {locale})}</p><dl>{#each Object.entries(list.source.fields) as [key,value]}<dt>{key}</dt><dd>{value||'—'}</dd>{/each}</dl></details>
{/if}
<style>
 h1{font:normal 26px/1.4 Georgia,serif;margin:8px 0}.source-label{font:500 13px system-ui;color:#435b47;margin:10px 0 0}.metadata,.note,.source-details,.review{font:13px/1.6 system-ui;color:#686d63}.note{max-width:35rem}a,button{color:#435b47;min-height:40px;font:13px system-ui}button{border:0;background:transparent;text-decoration:underline;cursor:pointer}ul{list-style:none;padding:0}li{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:4px 18px;max-width:34rem;padding:12px 16px;margin:8px 0;background:#f5f5ef;border:1px solid #e0e0d7;border-radius:8px}.identity{min-width:0;overflow-wrap:anywhere}.identity strong{font:normal 17px Georgia,serif}.scientific{font:italic 12px system-ui;color:#686d63;margin:4px 0}.count{font:13px/1.6 system-ui;max-width:12ch;text-align:right}.extra{grid-column:1/-1;font:13px/1.6 system-ui;margin:4px 0;white-space:pre-wrap;overflow-wrap:anywhere}summary{cursor:pointer;padding:10px 0}dt{font-weight:500}dd{margin:0 0 6px;white-space:pre-wrap;overflow-wrap:anywhere}:is(a,button,summary):focus-visible{outline:2px solid #788e7b;outline-offset:3px}[role=alert]{color:#9a403b}
 li{position:relative}
 .card-media{grid-column:1/-1;justify-self:start;min-height:0;padding:3px 0;font-size:12px;text-decoration:none}
 .card-media::after{content:"";position:absolute;inset:0;border-radius:8px}
 .card-media:hover{text-decoration:underline}
 .card-media:focus-visible{outline:none}
 .card-media:focus-visible::after{outline:2px solid #788e7b;outline-offset:-2px}
 .media-line{grid-column:1/-1;display:flex;gap:12px;min-width:0;overflow-x:auto;scrollbar-width:thin}
 .media-line a{flex:none;min-height:0;padding:3px 2px;font-size:12px;white-space:nowrap;border-radius:4px}
 .media-line a:focus-visible{outline-offset:-2px}
</style>
