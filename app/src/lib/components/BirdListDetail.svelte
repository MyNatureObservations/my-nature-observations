<script lang="ts">
 import type {BirdList} from "$lib/birds";
 import {formatBirdDate,type BirdNamePreferences} from "$lib/bird-names";
 import BirdSpeciesName from "$lib/components/BirdSpeciesName.svelte";
 import EntryOptions from "$lib/components/EntryOptions.svelte";
 import BirdAttribution from "$lib/components/BirdAttribution.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {list,origin,locale,preferences,preferencesSaved=true,editHref=null,onDelete=null,actionsDisabled=false,onPreferencesChange}:{list:BirdList;origin:string;locale:"sv"|"en";preferences:BirdNamePreferences;preferencesSaved?:boolean;editHref?:string|null;onDelete?:(()=>void)|null;actionsDisabled?:boolean;onPreferencesChange:(p:BirdNamePreferences)=>void}=$props();
</script>
<header><a href={origin}>← {m.bird_back_entry({}, {locale})}</a><span class="section-label">{m.bird_lists({}, {locale})}</span></header>
<h1>{list.place.name}</h1>
<div class="metadata"><time datetime={list.visitDate}>{formatBirdDate(list.visitDate,locale)}</time>{#if list.visitStart||list.visitEnd}<span>{list.visitStart??"…"}–{list.visitEnd??"…"}</span>{/if}{#if editHref||onDelete}<div class="list-options"><EntryOptions {locale}>{#snippet children(close)}{#if editHref&&!actionsDisabled}<a href={editHref} onclick={close}>{m.journal_edit({}, {locale})}</a>{/if}{#if onDelete}<button type="button" disabled={actionsDisabled} onclick={()=>{close();onDelete?.();}}>{m.bird_delete({}, {locale})}</button>{/if}{/snippet}</EntryOptions></div>{/if}</div>
<p class="visit-context">{m.bird_visit_context({}, {locale})}</p>
<ul class="species-cards" aria-label={m.bird_species({}, {locale})}>
 {#each list.rows as row,i (i)}<li>
  <BirdSpeciesName species={row} {locale} {preferences} count={row.count}/>
  {#if row.comment}<div class="comment-section"><span class="comment-label">{m.bird_your_comment({}, {locale})}</span><p class="comment">{row.comment}</p></div>{/if}
 </li>{/each}
</ul>
<details class="view-options"><summary>{m.bird_name_display({}, {locale})}</summary>
 <label><input type="checkbox" checked={preferences.showOtherLanguage} onchange={e=>onPreferencesChange({...preferences,showOtherLanguage:e.currentTarget.checked})}/>{locale==="sv"?m.bird_show_english({}, {locale}):m.bird_show_swedish({}, {locale})}</label>
 <label><input type="checkbox" checked={preferences.showScientific} onchange={e=>onPreferencesChange({...preferences,showScientific:e.currentTarget.checked})}/>{m.bird_show_scientific({}, {locale})}</label>
 <small>{m.bird_names_all_lists({}, {locale})}</small>
 {#if !preferencesSaved}<p role="status">{m.bird_preferences_unsaved({}, {locale})}</p>{/if}
</details>
<BirdAttribution {locale}/>
<style>
 header{display:flex;align-items:center;justify-content:space-between;gap:20px;margin-bottom:32px}a{color:var(--accent,#435b47);font:13px system-ui,sans-serif;text-decoration:none;display:inline-flex;align-items:center;min-height:44px}a:hover{text-decoration:underline}.section-label{font:11px system-ui,sans-serif;letter-spacing:2px;color:var(--accent,#435b47)}h1{font:normal 28px/1.3 Georgia,"Times New Roman",serif;margin:0 0 14px}.list-options{margin-left:auto}.metadata{display:flex;align-items:center;gap:14px;flex-wrap:wrap;color:var(--text-muted,#686d63);font:12px system-ui,sans-serif;margin-bottom:20px}.view-options{font:12px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin:0 0 20px}.view-options summary{cursor:pointer;min-height:36px;display:list-item;align-content:center}.view-options label{display:flex;align-items:center;gap:8px;min-height:36px}.view-options input{accent-color:#607763}.view-options small{display:block;margin-top:4px}.species-cards{max-width:560px;list-style:none;padding:0;margin:0;display:grid;gap:10px}.species-cards>li{border:1px solid #e0e2da;background:#fafaf6;border-radius:10px;padding:14px 16px;min-width:0}.visit-context{color:var(--text-muted,#686d63);font:13px/1.5 system-ui,sans-serif;margin:0 0 12px}.comment-section{margin-top:12px;max-width:42ch}.comment-label{color:var(--text-muted,#686d63);font:11px system-ui,sans-serif;letter-spacing:.04em}.comment{font:17px/1.55 Georgia,"Times New Roman",serif;margin:5px 0 0;white-space:pre-wrap;overflow-wrap:anywhere;color:#4c5048}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}@media(max-width:600px){.section-label{display:none}}
</style>
