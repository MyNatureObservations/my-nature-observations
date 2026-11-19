<script lang="ts">
 import {untrack,tick} from 'svelte';
 import BirdWeatherFetch from '$lib/components/BirdWeatherFetch.svelte';
 import {cancelVisitWeather,type VisitPreview,type WeatherOwner} from '$lib/bird-weather-fetch';
 import {formatBirdDate} from '$lib/bird-names';
 import WeatherIcon from '$lib/components/WeatherIcon.svelte';
 import WeatherOverview from '$lib/components/WeatherOverview.svelte';
 import WeatherForm from '$lib/components/WeatherForm.svelte';
 import BirdVisitWeather from '$lib/components/BirdVisitWeather.svelte';
 import {sameVisit,weatherStale,visitWeatherSummary,manualWeatherChange,type VisitWeather,type VisitContext,type WeatherChange} from '$lib/bird-weather';
 import type {WeatherPart} from '$lib/weather';
 import * as m from '$lib/paraglide/messages.js';
 let {records=[],context,locale,disabled=false,generation=null,getOwner=async()=>{throw Error("bird_weather_context_conflict")},onDirty}:{records?:VisitWeather[];context:VisitContext|null;locale:'sv'|'en';disabled?:boolean;generation?:number|null;getOwner?:()=>Promise<WeatherOwner>;onDirty:()=>void}=$props();
 const original=untrack(()=>records.find(r=>r.kind==='manual'));
 const manual:WeatherPart|null=original?JSON.parse(original.payload_json):null;
 let enabled=$state(!!original),removed=$state<string[]>([]),changed=$state(false);
 let captured=$state<VisitContext|null>(untrack(()=>original?JSON.parse(original.context_json):null));
 let confirmedContext=$state<VisitContext|null>(null),error=$state('');
 let invalidated=$state<string[]>([]);
 let panel:HTMLDialogElement|undefined=$state(),trigger:HTMLButtonElement|undefined=$state(),opened=$state(false);
 let preview=$state<VisitWeather|null>(null);
 let fetchedDraft=$state<VisitPreview|null>(null),fetchedInvalidated=$state(false),fetchBusy=$state(false),fetchReview=$state(false);
 let fetchControl:BirdWeatherFetch|undefined=$state();
 function fetchedApplied(value:VisitPreview){
  error='';if(disabled||weatherStale(value.record,context))throw 'bird_weather_context_conflict';
  // Review has succeeded. Validate pending manual input without changing it.
  const manualChange=readManualChange();
  if(manualChange?.action==='set')preview={list_id:original?.list_id??'draft',kind:'manual',context_json:JSON.stringify(manualChange.context),payload_json:JSON.stringify(manualChange.payload),stale:false};
  fetchedDraft=value;fetchedInvalidated=false;removed=removed.filter(k=>k!=='fetched');onDirty();
  // The child releases the accepted receipt after this callback; do not revoke it.
  snapshot=null;panel?.close();opened=false;trigger?.focus({preventScroll:true});
 }

 let snapshot: {enabled:boolean;removed:string[];changed:boolean;captured:VisitContext|null;confirmedContext:VisitContext|null;contextChanged:boolean;fetchedDraft:VisitPreview|null;fetchedInvalidated:boolean;fields:ReturnType<WeatherForm['draftSnapshot']>|null}|null=null;
 async function openPanel(){
  if(disabled||!panel||panel.open)return;
  snapshot={enabled,removed:[...removed],changed,captured:captured?{...captured}:null,confirmedContext:confirmedContext?{...confirmedContext}:null,contextChanged,fetchedDraft,fetchedInvalidated,fields:fields?.draftSnapshot()??null};
  panel.showModal();opened=true;await tick();panel.querySelector<HTMLButtonElement>('.cancel')?.focus();
 }
 async function cancelPanel(){
  if(!opened)return;
  fetchControl?.discard();
  if(fetchedDraft&&fetchedDraft.previewId!==snapshot?.fetchedDraft?.previewId)void cancelVisitWeather(fetchedDraft.previewId).catch(()=>{});

  if(snapshot){const saved=snapshot;enabled=saved.enabled;removed=saved.removed;changed=saved.changed;captured=saved.captured;confirmedContext=saved.confirmedContext;contextChanged=saved.contextChanged;fetchedDraft=saved.fetchedDraft;fetchedInvalidated=saved.fetchedInvalidated;error='';await tick();if(saved.fields)fields?.restoreDraft(saved.fields);}
  snapshot=null;panel?.close();opened=false;trigger?.focus({preventScroll:true});
 }
 function applyPanel(){
  if(fetchBusy)return;
  const changes=getChanges();if(changes===null)return;
  const manualChange=changes.find(c=>c.action==='set');
  if(manualChange?.action==='set')preview={list_id:original?.list_id??'draft',kind:'manual',context_json:JSON.stringify(manualChange.context),payload_json:JSON.stringify(manualChange.payload),stale:false};
  fetchControl?.discard();snapshot=null;panel?.close();opened=false;trigger?.focus({preventScroll:true});
 }
 function reuse(){if(context){confirmedContext={...context};onDirty();}}
 export function isOpen(){return opened;}
 export function fetchFailure(){fetchedInvalidated=true;error='fetch';void openPanel();}
 // Explicit navigation from the detail review opens/focuses choices, never applies them.
 export async function openForReview(action:'use'|'remove'|'fetch'){
  await openPanel();await tick();panel?.querySelector<HTMLButtonElement>(action==='use'?'.reuse':action==='fetch'?'.fetch':'.remove-weather')?.focus();
 }

 let fields:WeatherForm|undefined=$state();
 // Sticky within the draft, too: changing back does not silently reactivate it.
 let contextChanged=$state(false);
 $effect(()=>{if(captured&&!sameVisit(captured,context))contextChanged=true;});
 $effect(()=>{if(confirmedContext&&!sameVisit(confirmedContext,context))confirmedContext=null;});
 $effect(()=>{for(const record of records)if(weatherStale(record,context)&&!invalidated.includes(record.kind))invalidated=[...invalidated,record.kind];});
 const stale=$derived(!!original?.stale||contextChanged||!!captured&&!sameVisit(captured,context));
 const confirmed=$derived(sameVisit(confirmedContext,context));
 $effect(()=>{if(fetchedDraft&&weatherStale(fetchedDraft.record,context))fetchedInvalidated=true;});
 const visible=$derived(fetchedDraft?[...records.filter(r=>r.kind!=='fetched'),{...fetchedDraft.record,stale:fetchedInvalidated}]:records);
 const shown=$derived(visible.filter(r=>!removed.includes(r.kind)).map(r=>({...r,stale:r.stale||(!(r.kind==='fetched'&&fetchedDraft)&&invalidated.includes(r.kind))||weatherStale(r,context)})));
 const summaryRecords=$derived(preview&&!removed.includes('manual')?[...shown.filter(r=>r.kind!=='manual'),{...preview,stale:stale&&!confirmed||weatherStale(preview,context)}]:shown);
 const summary=$derived(visitWeatherSummary(summaryRecords,context));
 const needsReview=$derived(summaryRecords.some(r=>weatherStale(r,context))||enabled&&stale&&!confirmed);
 function remove(kind:string){removed=[...removed.filter(k=>k!==kind),kind];if(kind==='manual')enabled=false;error='';onDirty();}
 function add(){if(!context)return;removed=removed.filter(k=>k!=='manual');captured={...context};contextChanged=false;confirmedContext=null;enabled=true;changed=true;onDirty();}
 function readManualChange():WeatherChange|null {
  if(!enabled||removed.includes('manual')||original&&!changed&&!confirmed&&!preview)return null;
  if(!context||stale&&!confirmed){error='confirm';throw 'bird_weather_manual_review_required';}
  const part=fields?.getPart();if(!part){error='fields';throw 'bird_weather_manual_invalid';}
  return manualWeatherChange(part,context,confirmed);
 }
 export function getChanges():WeatherChange[]|null {
  error='';if(fetchBusy){error='fetch';return null;}const changes:WeatherChange[]=removed.filter(kind=>records.some(r=>r.kind===kind)).map(kind=>({action:'remove',kind:kind as 'manual'|'fetched'}));
  if(fetchedDraft&&!removed.includes('fetched')){
   if(fetchedInvalidated||weatherStale(fetchedDraft.record,context)){error='fetch';void openPanel();return null;}
   changes.push({action:'fetched',previewId:fetchedDraft.previewId});
  }
  try{const manualChange=readManualChange();if(manualChange)changes.push(manualChange);}
  catch{void openPanel();return null;}return changes;
 }
</script>
<span class="weather-entry">
 <button bind:this={trigger} class="weather-control" type="button" {disabled} aria-haspopup="dialog" aria-expanded={opened} onclick={()=>void openPanel()}>
  {#if summary.manual||summary.fetched}<WeatherOverview data={summary} {locale} compact/>{:else}<WeatherIcon name="neutral" size={16}/>{/if}
  <span class:needs-review={needsReview} aria-live="polite">{needsReview?m.bird_weather_needs_review({}, {locale}):m.weather_heading({}, {locale})}</span>
 </button>
 {#if removed.length}<span class="feedback" role="status">{m.bird_weather_removed({}, {locale})}</span>{/if}
</span>
<dialog bind:this={panel} class="weather-panel" aria-label={m.weather_heading({}, {locale})} oncancel={event=>{event.preventDefault();void cancelPanel();}}>
 <header><h2>{m.weather_heading({}, {locale})}</h2><button class="close" type="button" aria-label={m.weather_close({}, {locale})} onclick={()=>void cancelPanel()}>×</button></header>
 <fieldset disabled={disabled}>
  {#if needsReview||enabled&&stale||error==='confirm'||error==='fetch'}
   <div class="weather-review" role="group" aria-label={m.bird_weather_needs_review({}, {locale})}>
    <p class="review-question">{m.bird_weather_stale_question({}, {locale})}</p>
    {#if captured}<p class="review-context">{m.bird_weather_original({}, {locale})}: {captured.place_name} · {formatBirdDate(captured.date,locale)}{#if captured.start||captured.end} · {captured.start??'…'}–{captured.end??'…'}{/if}</p>{/if}
    <div class="review-actions">
     {#if enabled&&stale}<button class="reuse" type="button" disabled={!context||confirmed} onclick={reuse}>{confirmed?m.bird_weather_reused({}, {locale}):m.bird_weather_use({}, {locale})}</button>{/if}
     {#each shown.filter(r=>weatherStale(r,context)) as record (record.kind)}<button type="button" class="remove-weather" onclick={()=>remove(record.kind)}>{m.bird_weather_remove({}, {locale})}{#if shown.length>1} · {record.kind==='manual'?m.weather_manual({}, {locale}):m.weather_fetched({}, {locale})}{/if}</button>{/each}
     {#if !original&&enabled&&stale}<button type="button" class="remove-weather" onclick={()=>remove('manual')}>{m.bird_weather_remove({}, {locale})}</button>{/if}
    </div>
    {#if error==='fetch'}<p class="error" role="alert">{m.bird_weather_fetch_conflict({}, {locale})}</p>{/if}
    {#if error==='confirm'}<p class="error" role="alert">{m.bird_weather_confirm_needed({}, {locale})}</p>{/if}
   </div>
  {/if}
  {#each shown.filter(r=>!weatherStale(r,context)) as record (record.kind)}<button type="button" class="secondary" onclick={()=>remove(record.kind)}>{m.bird_weather_remove({}, {locale})} · {record.kind==='manual'?m.weather_manual({}, {locale}):m.weather_fetched({}, {locale})}</button>{/each}
  {#if removed.length}<p role="status">{m.bird_weather_removed({}, {locale})}</p>{/if}
  {#if enabled&&!removed.includes('manual')}
   {#if !original&&!stale}<button type="button" class="secondary" onclick={()=>remove('manual')}>{m.bird_weather_remove({}, {locale})}</button>{/if}
   {#if error==='fields'}<p class="error" role="alert">{m.bird_weather_invalid({}, {locale})}</p>{/if}
   <WeatherForm bind:this={fields} {manual} date={captured?.date??context?.date??''} {locale} busy={disabled} embedded wide onDirty={value=>{if(value){changed=true;onDirty();}}}/>
  {:else if !original}<button type="button" class="secondary" onclick={add} disabled={!context}>{m.bird_weather_add({}, {locale})}</button>{#if !context}<p>{m.bird_weather_place_required({}, {locale})}</p>{/if}{/if}
 </fieldset>
 <BirdWeatherFetch bind:this={fetchControl} {context} {generation} {getOwner} {locale} disabled={disabled} onApply={fetchedApplied} onReview={value=>fetchReview=value} onBusy={value=>{fetchBusy=value;if(value)onDirty();}}/>
 {#if !fetchReview}<BirdVisitWeather records={shown} {context} {locale} inline/>{/if}
 <p class="hint">{m.bird_weather_draft_hint({}, {locale})}</p>
 <footer><button class="cancel" type="button" onclick={()=>void cancelPanel()}>{m.bird_cancel({}, {locale})}</button>{#if !fetchReview}<button type="button" class="primary" disabled={disabled||fetchBusy} onclick={applyPanel}>{m.bird_weather_done({}, {locale})}</button>{/if}</footer>
</dialog>
<style>
 .needs-review{color:#756146;font-weight:500}.weather-review{margin:6px 0 20px;padding:14px 16px;border:1px solid #ddd7c9;border-radius:8px;background:#f5f3eb}.review-question{margin:0 0 6px;font-weight:500}.review-context{font-size:12px;color:var(--text-muted,#686d63);margin:0 0 12px;overflow-wrap:anywhere}.review-actions{display:flex;align-items:center;gap:10px;flex-wrap:wrap}.review-actions button{min-height:44px;padding:9px 14px;font:inherit;border:1px solid #cdd1c6;border-radius:6px;cursor:pointer}.review-actions .reuse{background:#435b47;border-color:#435b47;color:#faf9f5}.remove-weather{background:transparent;color:#435b47}.weather-review .error{margin:10px 0 0}

 .weather-entry{display:inline-flex;align-items:center;flex-wrap:wrap;gap:4px;max-width:100%}.weather-control{display:inline-flex;align-items:center;gap:6px;padding:4px 6px;border:1px solid transparent;border-radius:6px;background:transparent;color:var(--accent,#435b47);font:12px/1.4 system-ui,sans-serif;min-height:36px;cursor:pointer}.weather-control:hover{border-color:#788e7b}.feedback{font:11px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63)}
 .weather-panel{box-sizing:border-box;width:min(780px,calc(100vw - 32px));max-height:calc(100dvh - 32px);overflow:auto;overscroll-behavior:contain;padding:18px 22px;border:1px solid #d9d8d1;border-radius:12px;background:#faf9f5;color:#343530;font:13px/1.5 system-ui,sans-serif}.weather-panel::backdrop{background:#34353026}header{display:flex;justify-content:space-between;align-items:center;gap:12px}h2{font:normal 22px Georgia,serif;margin:0}.close{border:0;background:transparent;font:24px system-ui,sans-serif;color:#435b47;min-width:40px;min-height:40px;cursor:pointer}fieldset{border:0;padding:0;margin:12px 0;min-width:0}.hint{color:var(--text-muted,#686d63);font-size:12px}.secondary{display:block;border:0;background:transparent;color:var(--accent,#435b47);font:inherit;min-height:36px;padding:6px 0;cursor:pointer}.error{color:#9a403b}footer{display:flex;justify-content:flex-end;gap:10px;margin-top:16px}footer button{border:1px solid #d9d8d1;border-radius:6px;background:transparent;color:#435b47;padding:8px 12px;min-height:40px;font:inherit;cursor:pointer}footer .primary{background:#435b47;color:#faf9f5;border-color:#435b47}button:disabled{opacity:.55;cursor:default}:focus-visible{outline:2px solid #788e7b;outline-offset:3px}
</style>
