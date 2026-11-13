<script lang="ts">
 import {onMount} from "svelte";
 import { invoke } from "@tauri-apps/api/core";
 import { makeBirdReader, birdHref, entryHref, speciesLabel, type BirdList } from "$lib/birds";
 import {capitalizeCommonName,formatBirdDate} from "$lib/bird-names";
 import * as m from "$lib/paraglide/messages.js";
 let {entryId,generation,locale,allowCreate=false,journalDate=null}:{entryId:string;generation:number|null;locale:"sv"|"en";allowCreate?:boolean;journalDate?:string|null}=$props();
 let available=$state(false),failed=$state(false),lists=$state<BirdList[]>([]);
 const read=makeBirdReader();
 let revision=$state(0);
 onMount(()=>{const refresh=()=>revision++;window.addEventListener("mno:journal-changed",refresh);return()=>window.removeEventListener("mno:journal-changed",refresh);});
 $effect(()=>{revision;const id=entryId,g= generation;let cancelled=false;
  available=false;lists=[];failed=false;
  void (async()=>{try {const enabled=await invoke<boolean>("bird_observations_available");if(cancelled||!enabled||g===null)return;available=true;const response=await read<BirdList[]>("read_journal_bird_lists",{entryId:id,generation:g});if(!cancelled)lists=response.data;}catch{if(!cancelled)failed=true;}})();
  return ()=>{cancelled=true;};
 });
</script>
{#if available}
 <section class="bird-links" aria-label={m.bird_lists({}, {locale})}>
  {#if lists.length||allowCreate}<h2>{m.bird_lists({}, {locale})}</h2>{/if}
  {#if allowCreate}<p class="section-context">{m.bird_preview_context({}, {locale})}</p>{/if}
  {#if failed}<p role="alert">{m.bird_load_failed({}, {locale})}</p>{/if}
  {#each lists as list (list.id)}
   <a class="bird-preview" href={birdHref(entryId,list.id)}>
    <span class="visit"><strong>{list.place.name}</strong>{#if list.visitStart||list.visitEnd}<span>{' · '}{list.visitStart??"…"}–{list.visitEnd??"…"}</span>{/if}{#if list.visitDate!==journalDate}<span>{' · '}{formatBirdDate(list.visitDate,locale)}</span>{/if}</span>
    <span class="species-preview">{#each list.rows.slice(0,3) as row,i}{#if i>0}{' · '}{/if}{capitalizeCommonName(speciesLabel(row,locale),locale)} <b>{row.count}</b>{/each}{#if list.rows.length>3}{' · '}{m.bird_more_rows({count:list.rows.length-3}, {locale})}{/if}</span>
   </a>
  {/each}
  {#if allowCreate&&generation!==null}<a class="create-birds" href={`${entryHref(entryId)}/birds/new`}>＋ {m.bird_create({}, {locale})}</a>{/if}
 </section>
{/if}
<style>
 .bird-links{margin-top:20px;color:var(--text-muted,#686d63);font:13px/1.5 system-ui,sans-serif}h2{font:12px system-ui,sans-serif;color:var(--accent,#435b47);margin:0 0 6px}.section-context{font:12px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin:0 0 6px}.bird-preview{display:block;padding:7px 4px;margin:3px -4px;color:inherit;text-decoration:none;border-radius:4px}.bird-preview:hover{background:#eef1eb}.visit,.species-preview{display:block}.visit strong{font-weight:500;color:var(--accent,#435b47)}.species-preview{margin-top:2px}b{font-weight:500;font-variant-numeric:tabular-nums;margin-left:2px}.create-birds{display:inline-flex;align-items:center;min-height:40px;color:var(--accent,#435b47);font-size:12px}a:focus-visible{outline:2px solid #788e7b;outline-offset:3px}
</style>
