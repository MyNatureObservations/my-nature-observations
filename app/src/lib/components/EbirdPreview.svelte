<script lang="ts">
 import {onMount} from 'svelte';
 import {invoke} from '@tauri-apps/api/core';
 import {openUrl} from '@tauri-apps/plugin-opener';
 import {language} from '$lib/language.svelte';
 import {parseEbird,applyCatalogMatches,mediaId,mediaHref,type Preview,type Issue} from '$lib/ebird-preview';
 import {groupPreviewVisits,formatSourceDate,formatSourceTime,compactSpecies,sourceSpeciesName,displaySourceCount} from '$lib/ebird-presentation';
 import type {Species} from '$lib/birds';
 import * as m from '$lib/paraglide/messages.js';
 let available=$state<boolean|null>(null),busy=$state(false),error=$state(''),preview=$state<Preview|null>(null),selected=$state<string|null>(null);
 const days=$derived(groupPreviewVisits(preview?.visits??[]));
 let disposed=false,serial=0;
 const options=()=>({locale:language.current});
 const sourceFields=['Submission ID','Date','Time','Location ID','Location','Latitude','Longitude','State/Province','County','Protocol','Duration (Min)','Distance Traveled (km)','Area Covered (ha)','Number of Observers','All Obs Reported','Checklist Comments'];
 onMount(()=>{
  void invoke<boolean>('ebird_preview_available').then(value=>{if(!disposed)available=value;}).catch(()=>{if(!disposed)error='failed';});
  const beforeDisable=(event:Event)=>{if(busy)(event as CustomEvent<import('$lib/bird-module.svelte').DisableRequest>).detail.waitUntil(Promise.resolve(false));};
  window.addEventListener('mno:before-bird-disable',beforeDisable);
  return()=>{disposed=true;serial++;window.removeEventListener('mno:before-bird-disable',beforeDisable);};
 });
 async function choose(){
  if(!available||busy)return;busy=true;error='';const ticket=++serial;
  try{
   const text=await invoke<string|null>('preview_ebird_export',{locale:language.current});if(disposed||ticket!==serial||text===null)return;
   let result=parseEbird(text);
   if(!result.unsupported){
    const names=[...new Set(result.rows.map(r=>r.fields['Scientific Name']).filter(Boolean))];let matches:Species[][]=[];
    for(let i=0;i<names.length;i+=200){const batch=await invoke<Species[][]>('match_bird_paste_names',{names:names.slice(i,i+200)});if(disposed||ticket!==serial)return;matches.push(...batch);}
    result=applyCatalogMatches(result,matches,names);
   }
   if(!await invoke<boolean>('ebird_preview_available'))throw 'ebird_preview_unavailable';
   if(!disposed&&ticket===serial){preview=result;selected=null;}
  }catch(cause){if(!disposed&&ticket===serial){error=String(cause);preview=null;}}
  finally{if(!disposed&&ticket===serial)busy=false;}
 }
 function issueText(code:Issue['code']):string{
  return ({csv:m.ebird_issue_csv,header:m.ebird_issue_header,required:m.ebird_issue_required,count:m.ebird_issue_count,date:m.ebird_issue_date,time:m.ebird_issue_time,coordinate:m.ebird_issue_coordinate,number:m.ebird_issue_number,metadata:m.ebird_issue_metadata,media:m.ebird_issue_media,duplicate:m.ebird_issue_duplicate,limit:m.ebird_too_large}[code])({},options());
 }
 function sourceFieldLabel(field:string):string{return ({"Submission ID":m.ebird_field_checklist_id,"Date":m.ebird_field_date,"Time":m.ebird_field_time,"Location ID":m.ebird_field_location_id,"Location":m.ebird_field_place,"Latitude":m.ebird_field_latitude,"Longitude":m.ebird_field_longitude,"State/Province":m.ebird_field_region,"County":m.ebird_field_county,"Protocol":m.ebird_field_protocol,"Duration (Min)":m.ebird_field_duration,"Distance Traveled (km)":m.ebird_field_distance,"Area Covered (ha)":m.ebird_field_area,"Number of Observers":m.ebird_field_observers,"All Obs Reported":m.ebird_field_complete,"Checklist Comments":m.ebird_field_visit_comment} as Record<string,typeof m.ebird_field_date>)[field]?.({},options())??field;}
 function quantity(count:number,one:typeof m.ebird_visit_one,many:typeof m.ebird_visit_many){return (count===1?one:many)({count},options());}
 async function external(url:string){try{await openUrl(url);}catch{error='link';}}
</script>
<svelte:head><title>{m.ebird_title({},options())} — MNO DEV</title></svelte:head>
<main>
 <a class="back" href={available?'/observations':'/'}>← {available?m.bird_overview({},options()):m.restore_back_to_home({},options())}</a>
 <h1>{m.ebird_title({},options())}</h1>
 <p class="intro">{m.ebird_intro({},options())}</p>
 {#if available===false}<p role="status">{m.ebird_unavailable({},options())}</p>
 {:else if available===null&&!error}<p role="status">{m.bird_loading({},options())}</p>
 {:else if available}
  <button class="choose" type="button" disabled={busy} onclick={choose}>{busy?m.bird_loading({},options()):m.ebird_choose({},options())}</button>
 {/if}
 {#if error}<p role="alert">{error==='ebird_unreadable'?m.ebird_unreadable({},options()):error==='ebird_encoding'?m.ebird_encoding({},options()):error==='ebird_too_large'?m.ebird_too_large({},options()):error==='ebird_preview_unavailable'||error==='bird_module_disabled'?m.ebird_unavailable({},options()):error==='link'?m.ebird_link_failed({},options()):m.ebird_failed({},options())}</p>{/if}
 {#if preview?.unsupported}<p role="alert">{m.ebird_unsupported({},options())}</p>
 {:else if preview}
  <section aria-label={m.ebird_summary_heading({},options())}>
   <h2>{m.ebird_summary_heading({},options())}</h2>
   <p class="file-overview">{quantity(preview.visits.length,m.ebird_visit_one,m.ebird_visit_many)} · {quantity(preview.places,m.ebird_place_one,m.ebird_place_many)} · {quantity(preview.rows.length,m.ebird_record_one,m.ebird_record_many)}{#if preview.unspecified} · {m.ebird_unspecified_total({count:preview.unspecified},options())}{/if}{#if preview.media} · {quantity(preview.media,m.ebird_media_one,m.ebird_media_many)}{/if}</p>
   <p class="note">{m.ebird_validation_only({},options())}</p>
   {#if preview.issues.length}
    <h3>{m.ebird_issues({count:preview.issues.length},options())}</h3>
    <ul class="issues" role="alert">{#each preview.issues.slice(0,100) as issue}<li>{issue.field?m.ebird_issue_line({line:issue.line,field:sourceFieldLabel(issue.field)},options()):m.ebird_issue_row({line:issue.line},options())} {issueText(issue.code)}</li>{/each}</ul>
    {#if preview.issues.length>100}<p>{m.ebird_first_issues({},options())}</p>{/if}
   {:else}<p role="status">{m.ebird_valid({},options())}</p>{/if}
   {#if preview.unmatched.length}<details><summary>{quantity(preview.unmatched.length,m.ebird_unmatched_one,m.ebird_unmatched_many)}</summary><p class="note">{m.ebird_catalog_note({},options())}</p><ul>{#each preview.unmatched as name}<li>{name}</li>{/each}</ul></details>{/if}
  </section>
  {#each days as day (day.date)}
   <section class="day-group" aria-label={formatSourceDate(day.date,language.current)||m.ebird_date_unknown({},options())}>
    <h2>{formatSourceDate(day.date,language.current)||m.ebird_date_unknown({},options())}</h2>
    {#each day.visits as visit (visit.id)}
     {@const compact=compactSpecies(visit)}
     <details class="visit" open={selected===visit.id} ontoggle={event=>{if(event.currentTarget.open)selected=visit.id;else if(selected===visit.id)selected=null;}}>
      <summary class="visit-summary">
       <span class="visit-line"><strong>{visit.fields.Location}</strong>{#if visit.fields.Time}<span> · {formatSourceTime(visit.fields.Time,language.current)}</span>{/if}</span>
       <span class="species-preview">{#each compact.rows as row,i}{#if i}<span class="species-separator">{' · '}</span>{/if}{sourceSpeciesName(row)} <b>{row.fields.Count==='X'?m.ebird_unspecified({},options()):displaySourceCount(row.fields.Count,language.current)}</b>{/each}{#if compact.more}<span class="species-separator">{' · '}</span>{m.ebird_more_species({count:compact.more},options())}{/if}</span>
      </summary>
      {#if selected===visit.id}
       {#if /^S[1-9]\d*$/.test(visit.id)}<p class="visit-actions"><button class="text-link" onclick={()=>external(`https://ebird.org/checklist/${visit.id}`)}>{m.ebird_original({},options())} ↗</button></p>{/if}
       <ul class="rows">{#each visit.rows as row (row.line)}<li class="species-card">
        <div class="species-identity"><strong>{sourceSpeciesName(row)}</strong><p class="scientific">{row.fields['Scientific Name']}</p></div>
        <span class="species-count">{row.fields.Count==='X'?m.ebird_unspecified({},options()):row.count!==null?(row.count==='1'?m.bird_individual_one:m.bird_individual_many)({count:displaySourceCount(row.count,language.current)},options()):row.fields.Count}</span>
        {#if !row.catalogMatch}<p class="note species-addition">{m.ebird_source_taxon({},options())}</p>{/if}
        {#if row.fields['Observation Details']}<div class="species-addition source-comment"><span class="note">{m.ebird_species_comment({},options())}</span><p>{row.fields['Observation Details']}</p></div>{/if}
        {#if row.media.length}<p class="species-addition media-links">{#each row.media as id,i}{#if mediaId(id)}<button class="text-link" onclick={()=>external(mediaHref(id))}>{m.ebird_media_link({number:i+1},options())} ↗</button>{:else}<span>{id}</span>{/if}{#if i<row.media.length-1} · {/if}{/each}</p>{/if}
       </li>{/each}</ul>
       {#if visit.fields['Checklist Comments']}<div class="visit-comment"><h3>{m.ebird_visit_comment({},options())}</h3><p>{visit.fields['Checklist Comments']}</p></div>{/if}
       <details class="source-details">
        <summary>{m.ebird_source_details({},options())}</summary>
        <p class="note">{m.ebird_source_place({},options())}</p>
        <dl>{#each sourceFields as field}{#if visit.fields[field]}<div><dt>{sourceFieldLabel(field)}</dt><dd>{visit.fields[field]}</dd></div>{/if}{/each}</dl>
        <details class="raw-fields"><summary>{m.ebird_raw_fields({},options())}</summary>
         {#each visit.rows as row (row.line)}<h3>{sourceSpeciesName(row)}</h3><dl>{#each Object.entries(row.fields) as [field,value]}<div><dt>{field}</dt><dd>{value||'—'}</dd></div>{/each}</dl>{/each}
        </details>
       </details>
      {/if}
     </details>
    {/each}
   </section>
  {/each}
 {/if}
 <p><a href={available?'/observations':'/'}>{m.ebird_close({},options())}</a></p>
</main>
<style>
 main{box-sizing:border-box;max-width:824px;margin:0 auto;padding:24px 32px 60px;color:#343530;font:17px/1.5 Georgia,serif}h1{font:normal 28px/1.3 Georgia,serif;margin:16px 0 14px}h2{font:normal 21px/1.4 Georgia,serif;margin:24px 0 8px;color:#435b47}h3{font:500 13px system-ui,sans-serif}a,button{color:var(--accent,#435b47)}button{font:13px system-ui,sans-serif;min-height:40px;cursor:pointer}.choose{background:#eeeee8;border:1px solid #d9d8d1;border-radius:6px;padding:8px 14px}.back,.note,.file-overview,dl,.issues{font:13px/1.6 system-ui,sans-serif}.intro,.note{color:#686d63}.day-group{margin-top:28px}.visit{border-bottom:1px solid #e2e3dc}.visit-summary{position:relative;display:block;list-style:none;padding:10px 28px 10px 4px;border-radius:4px;cursor:pointer;font:13px/1.5 system-ui,sans-serif}.visit-summary::-webkit-details-marker{display:none}.visit-summary::after{content:'⌄';position:absolute;right:6px;top:10px;color:#686d63}.visit[open]>.visit-summary::after{transform:rotate(180deg)}.visit-summary:hover{background:#eef1eb}.visit-line,.species-preview{display:block}.visit-line strong{color:#435b47;font-weight:500}.species-preview{color:#686d63;margin-top:2px}.species-separator{white-space:pre}.species-preview b{font-weight:500;margin-left:2px}.visit-actions{margin:0 0 4px}.rows{list-style:none;padding:0;margin:0 0 16px}.species-card{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:5px 18px;max-width:34rem;box-sizing:border-box;margin:8px 0;padding:12px 16px;border:1px solid #e0e0d7;border-radius:8px;background:#f5f5ef}.species-identity{min-width:0;overflow-wrap:anywhere}.species-identity strong{font-size:16px;font-weight:normal}.scientific{font:italic 12px/1.5 system-ui,sans-serif;color:#686d63;margin:2px 0}.species-count{font:13px/1.6 system-ui,sans-serif;font-variant-numeric:tabular-nums;text-align:right;max-width:12ch}.species-addition{grid-column:1/-1;min-width:0;margin:2px 0;font:13px/1.6 system-ui,sans-serif}.source-comment p,.visit-comment p{margin:2px 0;white-space:pre-wrap;overflow-wrap:anywhere}.visit-comment{max-width:34rem}.media-links{display:flex;flex-wrap:wrap;gap:0 8px;align-items:center}.source-details{font:13px/1.6 system-ui,sans-serif;color:#686d63;margin:8px 0 18px}.source-details summary{padding:8px 0;cursor:pointer}.raw-fields{margin-top:8px}dl{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:8px 20px}dt{color:#686d63}dd{margin:0;color:#343530;white-space:pre-wrap;overflow-wrap:anywhere}.text-link{background:none;border:0;padding:2px 0;text-decoration:underline}button:disabled{opacity:.5;cursor:default}[role=alert]{color:#9a403b}:is(a,button,summary):focus-visible{outline:2px solid #788e7b;outline-offset:3px}@media(max-width:600px){main{padding:20px 24px 40px}.species-card{gap:5px 10px;padding:10px 12px}.species-count{max-width:10ch}}
</style>
