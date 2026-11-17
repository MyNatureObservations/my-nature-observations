<script lang="ts">
 import {onMount,tick} from "svelte";
 import {invoke} from "@tauri-apps/api/core";
 import {speciesLabel,type Species,type BirdRow} from "$lib/birds";
 import {capitalizeCommonName} from "$lib/bird-names";
 import {pasteCountError,parseBirdPaste,pasteLookupNames,prepareBirdPaste,planBirdPaste,applyBirdPaste,type PasteRow,type PasteTarget} from "$lib/bird-paste";
 import * as m from "$lib/paraglide/messages.js";
 let {existingRows,locale,onapply,oncancel}:{existingRows:BirdRow[];locale:"sv"|"en";onapply:(rows:BirdRow[],baseline:string)=>boolean;oncancel:()=>void}=$props();
 let source=$state(""),review=$state<PasteRow[]>([]),stage=$state<"input"|"review">("input"),resolving=$state(false),loadError=$state(false),limitError=$state(false),applyError=$state(false),applied=$state(false),blanks=$state(0),baseline=$state("");
 let textInput=$state<HTMLTextAreaElement>();let reviewHeading=$state<HTMLHeadingElement>();let disposed=false;
 const plan=$derived(planBirdPaste(existingRows,review));
 const stale=$derived(!!baseline&&baseline!==JSON.stringify(existingRows));
 const label=(species:Species)=>capitalizeCommonName(speciesLabel(species,locale),locale);
 function targetLabel(target:PasteTarget){
  const args={row:target.existingRow??target.sourceLine??0,species:label(target.species),count:target.count,version:target.taxonomyVersion};
  return target.existingRow!==null?m.bird_paste_existing_target(args,{locale}):m.bird_paste_incoming_target(args,{locale});
 }
 onMount(()=>{baseline=JSON.stringify(existingRows);return()=>{disposed=true;};});
 function mount(node:HTMLDialogElement){const previous=document.activeElement;node.showModal();textInput?.focus();return {destroy(){node.close();if(previous instanceof HTMLElement&&previous.isConnected)previous.focus({preventScroll:true});}};}
 async function interpret(){
  if(resolving)return;loadError=false;limitError=false;applyError=false;
  const parsed=parseBirdPaste(source);if(parsed.limited){limitError=true;return;}if(!parsed.rows.length)return;
  resolving=true;
  try{
   const matches=await invoke<Species[][]>("match_bird_paste_names",{names:pasteLookupNames(parsed.rows)});
   if(disposed)return;review=prepareBirdPaste(parsed.rows,matches);blanks=parsed.blanks;stage="review";
   await tick();if(!disposed)reviewHeading?.focus();
  }catch{if(!disposed)loadError=true;}finally{if(!disposed)resolving=false;}
 }
 function editName(row:PasteRow,name:string){row.name=name;row.selected=null;row.candidates=[];row.combineInto="";}
 async function search(row:PasteRow){
  if(resolving||row.excluded)return;const query=row.name;resolving=true;loadError=false;
  try{const matches=await invoke<Species[]>("search_bird_species",{query});if(!disposed&&row.name===query){row.candidates=matches;row.selected=null;}}
  catch{if(!disposed)loadError=true;}finally{if(!disposed)resolving=false;}
 }
 function choose(row:PasteRow,id:string){row.selected=row.candidates.find(species=>species.id===id)??null;row.combineInto="";}
 function duplicate(row:PasteRow,value:string){if(value==="exclude"){row.excluded=true;row.combineInto="";}else row.combineInto=value;}
 function apply(){
  if(resolving||stale||applied||!plan.canApply)return;
  applied=true;applyError=false;
  if(!onapply(applyBirdPaste(existingRows,review),baseline)){applied=false;applyError=true;}
 }
</script>
<dialog class="paste-review" use:mount aria-labelledby="bird-paste-heading" oncancel={event=>{event.preventDefault();oncancel();}}>
 <h2 id="bird-paste-heading" bind:this={reviewHeading} tabindex="-1">{m.bird_paste_title({}, {locale})}</h2>
 <p class="intro">{m.bird_paste_intro({}, {locale})}</p>
 {#if stage==="input"}
  <label for="bird-paste-source">{m.bird_paste_source({}, {locale})}</label>
  <textarea id="bird-paste-source" bind:this={textInput} bind:value={source} rows="7" disabled={resolving} placeholder={'Talgoxe 6\nBlåmes 4st\nNötväcka 1 st'}></textarea>
  {#if limitError}<p class="error" role="alert">{m.bird_paste_limit({}, {locale})}</p>{/if}
 {:else}
  {#if blanks}<p class="note">{m.bird_paste_blanks({count:blanks},{locale})}</p>{/if}
  <ol class="review-rows">
   {#each review as row,i (row.line)}
    {@const countError=pasteCountError(row.count)}
    <li class:excluded={row.excluded}>
     <h3>{m.bird_paste_line({line:row.line},{locale})}</h3>
     <p class="original"><span>{m.bird_paste_original({}, {locale})}</span> <samp>{row.original}</samp></p>
     <label class="exclude"><input type="checkbox" bind:checked={row.excluded}/>{m.bird_paste_exclude({}, {locale})}</label>
     <div class="row-fields">
      <label>{m.bird_paste_name({}, {locale})}<input type="text" value={row.name} disabled={row.excluded} oninput={event=>editName(row,event.currentTarget.value)} aria-label={m.bird_paste_name_line({line:row.line},{locale})}/></label>
      <button class="quiet" type="button" disabled={row.excluded||resolving||!row.name.trim()} onclick={()=>void search(row)}>{m.bird_paste_find({}, {locale})}</button>
      <label class="species-choice">{m.bird_paste_species({}, {locale})}<select value={row.selected?.id??""} disabled={row.excluded||resolving} aria-label={m.bird_paste_species_line({line:row.line},{locale})} aria-invalid={!row.excluded&&!row.selected} aria-describedby={!row.excluded&&!row.selected?`paste-species-help-${row.line}`:undefined} onchange={event=>choose(row,event.currentTarget.value)}>
       <option value="">{m.bird_paste_choose({}, {locale})}</option>
       {#each row.candidates as species (species.id)}<option value={species.id}>{label(species)} · {species.scientific}</option>{/each}
      </select>
       {#if !row.excluded&&!row.selected}<span class="error" id={`paste-species-help-${row.line}`}>{m.bird_paste_species_unresolved({}, {locale})}</span>{/if}
      </label>
      <label class="count">{m.bird_count({}, {locale})}<input type="text" inputmode="numeric" bind:value={row.count} disabled={row.excluded} aria-label={m.bird_paste_count_line({line:row.line},{locale})} aria-invalid={!row.excluded&&countError!==null} aria-describedby={!row.excluded&&countError!==null?`paste-count-help-${row.line}`:undefined}/>
       {#if !row.excluded&&countError}<span class="error" id={`paste-count-help-${row.line}`}>{countError==="required"?m.bird_paste_count_missing({}, {locale}):m.bird_paste_count_invalid({}, {locale})}</span>{/if}
      </label>
     </div>
     {#each row.candidates.filter(candidate=>candidate.referral&&(!row.selected||candidate.id===row.selected.id)) as referral}<p class="note">{m.bird_species_referral({name:capitalizeCommonName(referral.referral!,locale),species:label(referral),scientific:referral.scientific},{locale})}</p>{/each}
     {#if !row.excluded&&(plan.checks[i].targets.length||row.combineInto)}
      <label class="duplicate">{m.bird_paste_duplicate({}, {locale})}<select value={row.combineInto} aria-label={m.bird_paste_duplicate_line({line:row.line},{locale})} aria-invalid={plan.checks[i].issue==="duplicate"} onchange={event=>duplicate(row,event.currentTarget.value)}>
       <option value="">{plan.checks[i].targets.length?m.bird_paste_duplicate_choose({}, {locale}):m.bird_paste_new_row({}, {locale})}</option>
       {#each plan.checks[i].targets as target (target.key)}<option value={target.key}>{targetLabel(target)}</option>{/each}
       <option value="exclude">{m.bird_paste_exclude({}, {locale})}</option>
      </select></label>
     {/if}
     {#if !row.excluded&&plan.checks[i].issue==="duplicate"}<p class="error">{m.bird_paste_duplicate_help({}, {locale})}</p>{/if}
     {#if !row.excluded&&(plan.checks[i].issue==="overflow"||plan.checks[i].issue==="target")}<p class="error" role="alert">{m.bird_paste_sum_invalid({}, {locale})}</p>{/if}
     {#if !row.excluded&&plan.checks[i].resultCount}<p class="note">{m.bird_paste_total({count:plan.checks[i].resultCount!},{locale})}</p>{/if}
    </li>
   {/each}
  </ol>
 {/if}
 {#if resolving}<p role="status">{m.bird_loading({}, {locale})}</p>{/if}
 {#if loadError}<p class="error" role="alert">{m.bird_paste_failed({}, {locale})}</p>{/if}
 {#if stale||applyError}<p class="error" role="alert">{m.bird_paste_stale({}, {locale})}</p>{/if}
 <footer>
  <button type="button" onclick={oncancel}>{m.bird_cancel({}, {locale})}</button>
  {#if stage==="input"}<button class="primary" type="button" disabled={resolving||!source.trim()} onclick={()=>void interpret()}>{m.bird_paste_review({}, {locale})}</button>
  {:else}<button class="primary" type="button" disabled={resolving||stale||applied||!plan.canApply} onclick={apply}>{m.bird_paste_apply({}, {locale})}</button>{/if}
 </footer>
</dialog>
<style>
 dialog{box-sizing:border-box;width:min(740px,calc(100vw - 40px));max-height:85vh;overflow:auto;padding:26px;border:1px solid #d9d8d1;border-radius:12px;background:#faf9f6;color:#343530;font:13px/1.5 system-ui,sans-serif}dialog::backdrop{background:rgba(39,43,38,.25)}h2{font:normal 25px/1.3 Georgia,serif;margin:0 0 12px}h3{font:12px system-ui,sans-serif;color:#686d63;margin:0 0 8px}.intro{margin:0 0 18px;color:#686d63}label{display:grid;gap:6px;font-size:12px;color:#5d625a}input,textarea,select{box-sizing:border-box;max-width:100%;min-width:0;border:1px solid #cccec7;border-radius:6px;background:#fffefa;color:#343530;padding:8px 10px;font:14px system-ui,sans-serif}input,select,button{min-height:38px}textarea{display:block;width:100%;margin-top:7px;resize:vertical;line-height:1.6}.review-rows{list-style:none;padding:0;margin:16px 0}.review-rows>li{border-top:1px solid #dedfd8;padding:16px 0}.row-fields{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:end;margin-top:10px}.species-choice{grid-column:1}.count{max-width:110px}.original{margin:0 0 8px;overflow-wrap:anywhere}.original span{font-size:11px;color:#686d63}.original samp{white-space:pre-wrap;font:14px/1.5 Georgia,serif}.exclude{display:flex;align-items:center;gap:7px}.exclude input{min-height:0}.excluded .row-fields{opacity:.6}.duplicate{margin-top:10px}.note{font-size:12px;color:#686d63;margin:8px 0 0}.error{color:#9a403b;font-size:12px;margin:8px 0}button{border:1px solid #d3d4ce;border-radius:6px;background:transparent;color:#435b47;padding:8px 12px;font:12px system-ui,sans-serif;cursor:pointer}.quiet{padding:8px}.primary{background:#607763;border-color:#607763;color:white}footer{position:sticky;bottom:-26px;display:flex;justify-content:flex-end;gap:10px;padding:16px 0 0;background:#faf9f6;margin-top:16px}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}:disabled{opacity:.6;cursor:default}@media(max-width:540px){dialog{padding:20px}.row-fields{grid-template-columns:minmax(0,1fr)}.species-choice{grid-column:auto}.count{max-width:100%}footer{bottom:-20px}}
</style>
