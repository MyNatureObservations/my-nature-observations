<script lang="ts">
 import {getContext,onMount,tick} from "svelte";
 import {page} from "$app/state";
 import {beforeNavigate,goto} from "$app/navigation";
 import {invoke} from "@tauri-apps/api/core";
 import {getCurrentWindow} from "@tauri-apps/api/window";
 import {language} from "$lib/language.svelte";
 import {createJournalReader} from "$lib/journal-data";
 import {readJournalPlace,type Place} from "$lib/places";
 import {PLACE_MAP,type PlaceMapController} from "$lib/place-map.svelte";
 import {entryHref,birdHref,speciesLabel,validDate,validTime,validBirdCount,validBirdComment,canSaveBirdList,saveBirdList,reserveBirdRequest,confirmSavedBirdList,updateBirdList,type BirdList,type Species,type BirdRow} from "$lib/birds";
 import {DEFAULT_BIRD_NAMES,loadBirdNamePreferences,capitalizeCommonName,type BirdNamePreferences} from "$lib/bird-names";
 import BirdSpeciesName from "$lib/components/BirdSpeciesName.svelte";
 let namePreferences=$state<BirdNamePreferences>({...DEFAULT_BIRD_NAMES});
 import VisitTimePicker from "$lib/components/VisitTimePicker.svelte";
 import DatePicker from "$lib/components/DatePicker.svelte";
 import BirdAttribution from "$lib/components/BirdAttribution.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let {editId=null}:{editId?:string|null}=$props();
 let expectedToken=$state("");
 const entryId=page.params.id!;const origin=$derived(editId?birdHref(entryId,editId):entryHref(entryId));const map=getContext<PlaceMapController>(PLACE_MAP);
 let initialized=$state(false);
 let proposedDate=$state(false),proposedPlace=$state(false);
 let searchInput=$state<HTMLInputElement|undefined>(undefined);
 let searching=$state(false);
 let saveUncertain=$state(false);
 let requestId=$state<string|null>(null);
 let generation=$state<number|null>(null),available=$state<boolean|null>(null),date=$state(""),place=$state<Place|null>(null),start=$state(""),end=$state(""),rows=$state<BirdRow[]>([]),query=$state(""),results=$state<Species[]>([]),busy=$state(false),dirty=$state(false),saved=$state(false),error=$state(""),searchError=$state(false),leave=$state(false),leaveUrl=$state<string|null>(null),pendingClose=false;
 let stayButton=$state<HTMLButtonElement|undefined>(undefined);let cancelMap:(()=>void)|undefined;let disposed=false;let baseline=$state("");let savedListId=$state<string|null>(null);
 const valid=$derived(canSaveBirdList(date,place,rows,start,end));
 onMount(()=>{
  namePreferences=loadBirdNamePreferences();
  let unlisten:(()=>void)|undefined;
  void(async()=>{try{
   const enabled=await invoke<boolean>("bird_observations_available");if(disposed)return;available=enabled;if(!enabled)return;
   if(editId){
    const session=await invoke<{generation:number}>("new_journal_session");
    const response=await createJournalReader()<BirdList>("read_journal_bird_list",{entryId,id:editId,generation:session.generation});if(disposed)return;
    generation=response.generation;const list=response.data;expectedToken=list.editToken;
    date=list.visitDate;place=list.place;start=list.visitStart??"";end=list.visitEnd??"";
    rows=list.rows.map(row=>({id:row.speciesId,taxonomyVersion:row.taxonomyVersion,english:row.english,swedish:row.swedish,scientific:row.scientific,count:row.count,comment:row.comment??"",commentOpen:!!row.comment}));
    baseline=JSON.stringify([date,place?.id,start,end,rows]);initialized=true;return;
   }
   const response=await createJournalReader()<{id:string;entryDate:string}|null>("get_journal_entry",{id:entryId});if(disposed)return;
   if(!response.data)throw Error("missing");generation=response.generation;proposedDate=validDate(response.data.entryDate);date=proposedDate?response.data.entryDate:"";
   const link=await readJournalPlace(entryId,response.generation);if(disposed)return;
   // Reuse the current registry object, not a stale journal snapshot.
   if(link.place){const places=await invoke<{generation:number;data:Place[]}>("list_places",{generation:response.generation});if(disposed)return;if(places.generation!==response.generation)throw Error("generation");const p=places.data.find(p=>p.id===link.place?.id);if(p&&p.latitude!==null&&p.longitude!==null){place=p;proposedPlace=true;}}
   baseline=JSON.stringify([date,place?.id,start,end,rows]);
   initialized=true;
  }catch{if(!disposed){generation=null;error="load";}}})();
  void getCurrentWindow().onCloseRequested(event=>{if(saved||(!dirty&&!busy))return;event.preventDefault();if(!busy){pendingClose=true;void askLeave(null);}}).then(stop=>{if(disposed)stop();else unlisten=stop;});
  const unload=(event:BeforeUnloadEvent)=>{if((dirty||busy)&&!saved){event.preventDefault();event.returnValue="";}};window.addEventListener("beforeunload",unload);
  return()=>{disposed=true;cancelMap?.();unlisten?.();window.removeEventListener("beforeunload",unload);};
 });
 $effect(()=>{
  const q=query;let stopped=false;
  searching=!!q.trim();
  const timer=setTimeout(()=>{
   if(!q.trim()){results=[];searchError=false;return;}
   void invoke<Species[]>("search_bird_species",{query:q})
    .then(value=>{if(!stopped){results=value;searchError=false;}})
    .catch(()=>{if(!stopped)searchError=true;})
    .finally(()=>{if(!stopped)searching=false;});
  },180);
  return()=>{stopped=true;clearTimeout(timer);};
 });
 $effect(()=>{if(baseline&&JSON.stringify([date,place?.id,start,end,rows])!==baseline)dirty=true;});
 beforeNavigate(nav=>{if(saved||(!dirty&&!busy))return;nav.cancel();if(!busy)void askLeave(nav.to?.url.href??origin);});
 function mountLeaveDialog(node:HTMLDialogElement){node.showModal();return {destroy(){node.close();}};}
 async function askLeave(url:string|null){leaveUrl=url;leave=true;await tick();stayButton?.focus();}
 function cancel(){if(busy)return;if(dirty)void askLeave(origin);else {saved=true;void goto(origin);}}
 async function discard(){saved=true;leave=false;if(pendingClose){await invoke("close_journal_window");}else await goto(leaveUrl??origin);}
 function choosePlace(){if(busy||generation===null)return;cancelMap=map.open({mode:"observation",generation,center:place?.latitude!==null&&place?.longitude!==null&&place?{latitude:place.latitude!,longitude:place.longitude!}:null,selectedId:place?.id,select:async p=>{if(p.latitude===null||p.longitude===null)return false;place=p;dirty=true;return true;}});}
 function focusSearch(){searchInput?.focus();}
 async function add(species:Species){
  const existing=rows.findIndex(row=>row.id===species.id);
  const index=existing<0?rows.length:existing;
  if(existing<0){rows=[...rows,{...species,count:"",comment:"",commentOpen:false}];dirty=true;}
  query="";results=[];
  await tick();if(disposed)return;
  const input=document.getElementById(`bird-count-${index}`) as HTMLInputElement|null;
  input?.focus();input?.select();
 }
 async function openComment(index:number){rows[index].commentOpen=true;await tick();if(!disposed)document.getElementById(`bird-comment-${index}`)?.focus();}
 async function removeRow(index:number){
  rows=rows.filter((_,i)=>i!==index);dirty=true;await tick();if(disposed)return;
  const next=document.getElementById(`bird-count-${Math.min(index,rows.length-1)}`);
  if(next)next.focus();else focusSearch();
 }
 function searchKeydown(event:KeyboardEvent){
  if(event.key==="Enter"){
   event.preventDefault();if(results[0])void add(results[0]);
  }else if(event.key==="ArrowDown"){
   event.preventDefault();document.querySelector<HTMLButtonElement>("#species-results button")?.focus();
  }else if(event.key==="Escape"){event.preventDefault();query="";results=[];}
 }
 function resultKeydown(event:KeyboardEvent,index:number){
  if(event.key==="Escape"){event.preventDefault();query="";results=[];focusSearch();}
  else if(event.key==="ArrowDown"||event.key==="ArrowUp"){
   event.preventDefault();
   const next=index+(event.key==="ArrowDown"?1:-1);
   if(next<0)focusSearch();else document.querySelectorAll<HTMLButtonElement>("#species-results button")[Math.min(next,results.length-1)]?.focus();
  }
 }
 function countKeydown(event:KeyboardEvent,count:string){
  if(event.key==="Enter"){event.preventDefault();if(validBirdCount(count))focusSearch();}
 }
 async function save(){
  if((editId&&saveUncertain)||available!==true||savedListId!==null||(error==="request"||error==="retired"||error==="legacy")||!valid||generation===null||!place||busy)return;
  busy=true;error="";
  try{
   if(editId){
    const token=await updateBirdList(generation,entryId,editId,expectedToken,date,place,rows,start,end);
    expectedToken=token;savedListId=editId;saved=true;saveUncertain=false;
    try {await confirmSavedBirdList(generation,entryId,editId);dirty=false;window.dispatchEvent(new Event("mno:journal-changed"));await goto(origin);}catch(cause){console.error("bird-list: edit saved, reopening failed",cause);error="saved";}
    return;
   }
   requestId??=await reserveBirdRequest(generation);
   const id=await saveBirdList(generation,entryId,date,place,rows,start,end,requestId);
   savedListId=id;saved=true;saveUncertain=false;
   let stage="confirmation";
   try{
    await confirmSavedBirdList(generation,entryId,id);dirty=false;
    stage="notification";window.dispatchEvent(new Event("mno:journal-changed"));
    stage="navigation";await goto(birdHref(entryId,id));
   }catch(cause){console.error(`bird-list: saved, ${stage} failed`,cause);error="saved";}
  }catch(cause){
   console.error("bird-list: save failed",cause);
   saveUncertain=true;error=editId?"edit":cause==="bird_request_retired"?"retired":cause==="bird_request_legacy"||cause==="bird_request_unreserved"?"legacy":cause==="bird_request_conflict"?"request":"save";
  }finally{busy=false;}
 }

</script>
<svelte:head><title>{editId?m.bird_edit({}, {locale:language.current}):m.bird_create({}, {locale:language.current})} — MNO</title></svelte:head>
<main class="bird-page">
 <header class="page-header">
  <button class="back" type="button" onclick={cancel} disabled={busy}>← {editId?m.bird_back_list({}, {locale:language.current}):m.bird_back_entry({}, {locale:language.current})}</button>
  <span class="section-label">{m.bird_lists({}, {locale:language.current})}</span>
 </header>
 {#if error==="load"}<p class="error" role="alert">{m.bird_initialization_failed({}, {locale:language.current})}</p>
 {:else if available===false}<p>{m.bird_unavailable({}, {locale:language.current})}</p>
 {:else if !initialized}<p role="status">{m.bird_loading({}, {locale:language.current})}</p>
 {:else}
 <h1>{editId?m.bird_edit({}, {locale:language.current}):m.bird_create({}, {locale:language.current})}</h1>
 <p class="intro">{m.bird_creation_intro({}, {locale:language.current})}</p>
 {#if proposedDate||proposedPlace}<p class="proposed-context">{proposedDate&&proposedPlace?m.bird_proposed_both({}, {locale:language.current}):proposedDate?m.bird_proposed_date({}, {locale:language.current}):m.bird_proposed_place({}, {locale:language.current})}</p>{/if}
 <form onsubmit={event=>{event.preventDefault();void save();}} oninput={()=>dirty=true} novalidate>
  <fieldset class="visit" disabled={busy||saveUncertain||generation===null||savedListId!==null}>
   <legend class="sr-only">{m.bird_visit({}, {locale:language.current})}</legend>
   <div class="visit-metadata">
    <div class="date-control">
     <span class="sr-only">{m.bird_date({}, {locale:language.current})}</span>
     {#if validDate(date)}<DatePicker bind:value={date}/>{:else}<input type="date" bind:value={date} aria-label={m.bird_date({}, {locale:language.current})} aria-invalid="true" aria-describedby="bird-date-help"/>{/if}
     {#if !validDate(date)}<small id="bird-date-help" class="error">{m.bird_date_required({}, {locale:language.current})}</small>{/if}
    </div>
    <div class="place-control">
     <button class="place-trigger" type="button" onclick={choosePlace} aria-haspopup="dialog" aria-describedby={!place?'bird-place-help':undefined}>
      <span aria-hidden="true">⌖</span> {place?.name??m.bird_choose_place({}, {locale:language.current})}
     </button>
     {#if !place}<small id="bird-place-help">{m.bird_place_required({}, {locale:language.current})}</small>{/if}
    </div>
   </div>
   <div class="times">
    <span class="optional-label">{m.bird_visit_times({}, {locale:language.current})}</span>
    <VisitTimePicker id="bird-start" bind:value={start} label={m.bird_start({}, {locale:language.current})} locale={language.current} disabled={busy||saveUncertain||generation===null||savedListId!==null} descriptionId={!validTime(start)?'bird-start-help':undefined}/>
    <VisitTimePicker id="bird-end" bind:value={end} label={m.bird_end({}, {locale:language.current})} locale={language.current} disabled={busy||saveUncertain||generation===null||savedListId!==null} descriptionId={!validTime(end)?'bird-end-help':undefined}/>
   </div>
   {#if !validTime(start)}<small id="bird-start-help" class="error">{m.bird_time_invalid({}, {locale:language.current})}</small>{/if}
   {#if !validTime(end)}<small id="bird-end-help" class="error">{m.bird_time_invalid({}, {locale:language.current})}</small>{/if}
  </fieldset>
  <fieldset class="species" disabled={busy||saveUncertain||generation===null||savedListId!==null}>
   <legend class="sr-only">{m.bird_species({}, {locale:language.current})}</legend>
   {#if rows.length}
    <div class="row-headings" aria-hidden="true"><span>{m.bird_species({}, {locale:language.current})}</span><span>{m.bird_count({}, {locale:language.current})}</span><span></span></div>
    <ol class="rows">
     {#each rows as row,i}
      <li>
       <div class="species-name"><BirdSpeciesName species={row} locale={language.current} preferences={namePreferences}/></div>
       <div class="count-control">
        <input id={`bird-count-${i}`} inputmode="numeric" pattern="[1-9][0-9]*" required bind:value={row.count} aria-label={m.bird_count_for({species:speciesLabel(row,language.current)}, {locale:language.current})} aria-invalid={!validBirdCount(row.count)} aria-describedby={!validBirdCount(row.count)?`bird-count-help-${i}`:undefined} onkeydown={event=>countKeydown(event,row.count)}/>
       </div>
       <button class="remove-row" type="button" onclick={()=>void removeRow(i)} aria-label={m.bird_remove_species({species:speciesLabel(row,language.current)}, {locale:language.current})}>×</button>
       {#if !validBirdCount(row.count)}<small id={`bird-count-help-${i}`} class="row-error error">{m.bird_count_required({}, {locale:language.current})}</small>{/if}
       <div class="row-comment">
        {#if row.commentOpen||row.comment}<label class="sr-only" for={`bird-comment-${i}`}>{m.bird_comment_for({species:speciesLabel(row,language.current)}, {locale:language.current})}</label><textarea id={`bird-comment-${i}`} rows="2" maxlength="8192" bind:value={row.comment} aria-invalid={!validBirdComment(row.comment)} aria-describedby={!validBirdComment(row.comment)?`bird-comment-help-${i}`:undefined}></textarea>{#if !validBirdComment(row.comment)}<small id={`bird-comment-help-${i}`} class="error">{m.bird_comment_invalid({}, {locale:language.current})}</small>{/if}
        {:else}<button class="add-comment" type="button" onclick={()=>void openComment(i)} aria-label={m.bird_comment_for({species:speciesLabel(row,language.current)}, {locale:language.current})}>{m.bird_add_comment({}, {locale:language.current})}</button>{/if}
       </div>
      </li>
     {/each}
    </ol>
   {/if}
   <div class="add-species">
    <label for="species-search">{m.bird_search({}, {locale:language.current})}</label>
    <input id="species-search" type="search" bind:this={searchInput} bind:value={query} autocomplete="off" aria-controls={query.trim()?'species-results':undefined} aria-describedby={rows.length?"bird-search-help":"bird-search-help bird-species-required"} oninput={()=>{results=[];searchError=false;}} onkeydown={searchKeydown}/>
    <small id="bird-search-help">{m.bird_search_help({}, {locale:language.current})}</small>
    {#if !rows.length}<small id="bird-species-required">{m.bird_species_required({}, {locale:language.current})}</small>{/if}
    {#if query.trim()}
     <ul id="species-results" class="results" aria-label={m.bird_species({}, {locale:language.current})} aria-busy={searching}>
      {#each results as species,i (species.id)}<li><button type="button" onclick={()=>void add(species)} onkeydown={event=>resultKeydown(event,i)}><span>{capitalizeCommonName(language.current==="sv"&&species.swedish?species.swedish:species.english,language.current)}</span><small>{species.scientific}{#if language.current==="sv"&&!species.swedish} · {m.bird_no_swedish({}, {locale:language.current})}{/if}</small>{#if species.referral}<small class="referral">{m.bird_species_referral({name:capitalizeCommonName(species.referral,language.current),species:speciesLabel(species,language.current),scientific:species.scientific},{locale:language.current})}</small>{/if}</button></li>{/each}
     </ul>
     {#if searching}<p class="search-status" role="status">{m.bird_loading({}, {locale:language.current})}</p>{:else if !results.length&&!searchError}<p class="search-status">{m.bird_no_results({}, {locale:language.current})}</p>{/if}
    {/if}
    {#if searchError}<p class="error" role="alert">{m.bird_load_failed({}, {locale:language.current})}</p>{/if}
   </div>
  </fieldset>
  {#if savedListId}<p><a href={birdHref(entryId,savedListId)}>{m.bird_open_saved({}, {locale:language.current})}</a></p>{/if}
  {#if error}<p class="error" role="alert">{error==="edit"?m.bird_edit_failed({}, {locale:language.current}):error==="retired"?m.bird_request_retired({}, {locale:language.current}):error==="legacy"?m.bird_request_legacy({}, {locale:language.current}):error==="request"?m.bird_request_conflict({}, {locale:language.current}):error==="saved"?m.bird_saved_open_failed({}, {locale:language.current}):m.bird_save_failed({}, {locale:language.current})}</p>{/if}
  <footer class="actions"><button type="button" onclick={cancel} disabled={busy}>{m.bird_cancel({}, {locale:language.current})}</button><button class="primary" type="submit" disabled={(!!editId&&saveUncertain)||!valid||busy||(error==="request"||error==="retired"||error==="legacy")||generation===null||savedListId!==null}>{busy?m.bird_saving({}, {locale:language.current}):editId?m.bird_save_changes({}, {locale:language.current}):saveUncertain?m.bird_retry_save({}, {locale:language.current}):m.bird_save({}, {locale:language.current})}</button></footer>
 </form><BirdAttribution locale={language.current}/>
 {/if}
</main>
{#if leave}<dialog class="leave-dialog" use:mountLeaveDialog aria-labelledby="bird-leave-title" oncancel={event=>{event.preventDefault();leave=false;pendingClose=false;}}><h2 id="bird-leave-title">{m.bird_discard_title({}, {locale:language.current})}</h2><p>{saveUncertain?m.bird_uncertain_leave({}, {locale:language.current}):m.bird_discard_text({}, {locale:language.current})}</p><button bind:this={stayButton} onclick={()=>{leave=false;pendingClose=false;}}>{m.bird_stay({}, {locale:language.current})}</button><button onclick={()=>void discard()}>{m.bird_discard({}, {locale:language.current})}</button></dialog>{/if}

<style>
 .bird-page { max-width:824px; margin:0 auto; padding:24px 32px 60px; color:#343530; font-family:Georgia,"Times New Roman",serif; box-sizing:border-box; }
 .page-header { display:flex; justify-content:space-between; align-items:center; gap:20px; margin-bottom:32px; }
 .back { border:0; border-radius:6px; background:transparent; padding:8px 0; color:var(--accent,#435b47); font:13px system-ui,sans-serif; }
 .back:hover { text-decoration:underline; }
 .section-label { font:11px system-ui,sans-serif; letter-spacing:2px; color:var(--accent,#435b47); }
 h1 { font:normal 28px/1.25 Georgia,"Times New Roman",serif; margin:0 0 14px; }
 .intro{font:17px/1.5 Georgia,"Times New Roman",serif;margin:0 0 14px;color:#4c5048}.proposed-context{font:12px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63);margin:0 0 20px}
 fieldset { border:0; padding:0; margin:0; min-width:0; }
 .visit { margin-bottom:32px; }
 .visit-metadata { display:flex; align-items:flex-start; gap:14px 24px; flex-wrap:wrap; }
 .date-control, .place-control { min-width:0; }
 .date-control :global(.date-picker) { margin-bottom:0; min-height:36px; display:flex; align-items:center; }
 .place-trigger { display:inline-flex; align-items:center; gap:6px; padding:5px 8px; border:0; min-height:36px; background:transparent; border-radius:6px; color:var(--accent,#435b47); font:13px system-ui,sans-serif; max-width:100%; text-align:left; overflow-wrap:anywhere; }
 .place-trigger:hover { background:#eef1eb; }
 .times { display:flex; align-items:center; flex-wrap:wrap; gap:8px; margin-top:12px; color:var(--text-muted,#686d63); font:12px system-ui,sans-serif; }
 .optional-label { margin-right:4px; }
 input { box-sizing:border-box; max-width:100%; border:1px solid #cccec7; border-radius:6px; background:#fffefa; color:#343530; padding:8px 10px; font:15px system-ui,sans-serif; min-height:40px; }
 small { display:block; color:var(--text-muted,#686d63); font:12px/1.5 system-ui,sans-serif; }
 .row-headings,.rows li { display:grid; grid-template-columns:minmax(0,1fr) 76px 36px; column-gap:12px; align-items:center; }
 .row-headings { padding:0 0 8px; font:11px system-ui,sans-serif; letter-spacing:1px; color:var(--text-muted,#686d63); border-bottom:1px solid #d9d8d1; }
 .row-headings>span:nth-child(2) { text-align:center; }
 .rows { list-style:none; padding:0; margin:0 0 24px; }
 .rows li { padding:12px 0; border-bottom:1px solid #e2e1da; }
 .species-name { font-size:19px; line-height:1.4; min-width:0; overflow-wrap:anywhere; }
 .row-comment{grid-column:1/-1;margin-top:4px}.add-comment{border:0;background:transparent;padding:6px 0;min-height:32px;color:var(--accent,#435b47);font:12px system-ui,sans-serif}.row-comment textarea{box-sizing:border-box;display:block;width:100%;resize:vertical;margin-top:6px;padding:10px 12px;border:1px solid #d9dcd2;border-radius:6px;background:#fffefa;color:#343530;font:16px/1.5 Georgia,serif}
 .count-control input { width:100%; text-align:center; font-variant-numeric:tabular-nums; }
 .remove-row { min-width:36px; min-height:44px; padding:0; border:0; border-radius:6px; background:transparent; color:var(--text-muted,#686d63); font:22px system-ui,sans-serif; }
 .remove-row:hover { background:#eeede7; color:#9a403b; }
 .row-error { grid-column:1/-1; margin-top:5px; }
 .add-species label { display:block; margin-bottom:7px; font:13px system-ui,sans-serif; color:#5d625a; }
 #species-search { width:100%; }
 #bird-search-help { margin-top:6px; }
 .results { list-style:none; padding:0; margin:8px 0 0; max-height:280px; overflow:auto; background:#fffefa; border-radius:6px; }
 .results:empty { display:none; }
 .results li { margin:0; padding:0; }
 .results button { appearance:none; display:grid; box-sizing:border-box; margin:0; width:100%; min-height:48px; text-align:left; padding:10px 12px; border:0; border-bottom:1px solid #e4e3dc; background:transparent; color:#343530; font:17px/1.4 Georgia,serif; }
 .results .referral{margin-top:6px;line-height:1.5}
 .results button:hover,.results button:focus-visible { background:#eef1eb; }
 .search-status { font:13px/1.5 system-ui,sans-serif; color:var(--text-muted,#686d63); }
 .actions { display:flex; align-items:center; justify-content:flex-end; gap:12px; margin-top:32px; padding-top:20px; border-top:1px solid #d9d8d1; }
 .actions button,.leave-dialog button { border:1px solid #d3d4ce; border-radius:8px; padding:10px 16px; background:transparent; color:var(--accent,#435b47); font:13px system-ui,sans-serif; min-height:44px; }
 .actions .primary { background:#607763; color:white; border-color:#607763; }
 button { cursor:pointer; }
 button:disabled { cursor:default; opacity:.55; }
 .error { color:#9a403b; font:12px/1.5 system-ui,sans-serif; }
 input[aria-invalid="true"] { border-color:#b4756f; }
 .leave-dialog { background:#f8f7f3; color:#343530; padding:24px; max-width:430px; border:1px solid #d9d8d1; border-radius:12px; }
 .leave-dialog h2 { font:normal 22px Georgia,serif; }
 .leave-dialog::backdrop { background:rgba(39,43,38,.3); }
 .leave-dialog button { margin-right:10px; }
 :focus-visible { outline:2px solid #788e7b; outline-offset:3px; }
 .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0; }
 @media(max-width:600px) { .bird-page { padding:20px 24px 40px; } .section-label { display:none; } .row-headings,.rows li { grid-template-columns:minmax(0,1fr) 64px 32px; column-gap:8px; } .species-name { font-size:17px; } }
</style>
