<script lang="ts">
 import {onDestroy} from 'svelte';
 import {sameVisit,type VisitContext} from '$lib/bird-weather';
 import {fetchVisitWeather,reviewVisitWeather,cancelVisitWeather,type WeatherOwner,type VisitRequest,type VisitPreview} from '$lib/bird-weather-fetch';
 import {historicalError} from '$lib/weather-history';
 import {weatherLabels} from '$lib/weather-labels';
 import BirdVisitWeather from '$lib/components/BirdVisitWeather.svelte';
 import * as m from '$lib/paraglide/messages.js';
 let {context,generation,getOwner,disabled=false,locale,onApply,onBusy,onReview=()=>{}}:{context:VisitContext|null;generation:number|null;getOwner:()=>Promise<WeatherOwner>;disabled?:boolean;locale:'sv'|'en';onApply:(preview:VisitPreview)=>void;onBusy:(busy:boolean)=>void;onReview?:(reviewing:boolean)=>void}=$props();
 let result=$state<VisitPreview|null>(null),request=$state<VisitRequest|null>(null),busy=$state(false),error=$state('');
 let serial=0,activeId:string|null=null,disposed=false;let requestGeneration:number|null=null;
 function fail(cause:unknown){const code=String(cause);error=/bird_weather_manual_/.test(code)?'manual':/conflict|expired|generation/.test(code)?'conflict':historicalError(cause,true);}
 function setBusy(value:boolean){busy=value;onBusy(value);}
 export function discard(){
  serial++;const id=activeId;activeId=null;result=null;request=null;error='';setBusy(false);
  if(id)void cancelVisitWeather(id).catch(()=>{});
 }
 $effect(()=>{if(request&&(!sameVisit(request.context,context)||generation!==requestGeneration)){discard();error='conflict';}});
 $effect(()=>{onReview(!!result);});
 onDestroy(()=>{disposed=true;discard();});
 async function fetchDaily(){
  if(disabled||busy||!context||generation===null)return;
  discard();const ticket=++serial;const g=generation,captured={...context};error='';setBusy(true);
  try{
   const owner=await getOwner();if(disposed||ticket!==serial)return;
   if(g!==generation||!sameVisit(captured,context))throw 'bird_weather_context_conflict';
   requestGeneration=g;request={...owner,context:captured};activeId=crypto.randomUUID();const id=activeId;
   const preview=await fetchVisitWeather(g,id,request);
   if(disposed||ticket!==serial)return;
   if(g!==generation||!sameVisit(captured,context))throw 'bird_weather_context_conflict';
   result=preview;
  }catch(cause){if(!disposed&&ticket===serial){fail(cause);const id=activeId;activeId=null;if(id)void cancelVisitWeather(id).catch(()=>{});}}
  finally{if(!disposed&&ticket===serial)setBusy(false);}
 }
 async function apply(){
  if(disabled||busy||!result||!request||generation===null)return;
  const ticket=serial,g=generation,binding=request,id=result.previewId;setBusy(true);error='';
  try{
   if(!sameVisit(binding.context,context))throw 'bird_weather_context_conflict';
   const preview=await reviewVisitWeather(g,id,binding);
   if(disposed||ticket!==serial)return;
   if(g!==generation||!sameVisit(binding.context,context))throw 'bird_weather_context_conflict';
   onApply(preview);activeId=null;result=null;request=null;
  }catch(cause){if(!disposed&&ticket===serial)fail(cause);}
  finally{if(!disposed&&ticket===serial)setBusy(false);}
 }
 const message=$derived(error==='manual'?m.bird_weather_manual_before_apply({}, {locale}):error==='conflict'?m.bird_weather_fetch_conflict({}, {locale}):error?weatherLabels[`history_${error}` as keyof typeof weatherLabels]?.({}, {locale})??m.weather_history_fetchFailed({}, {locale}):'');
</script>
<section class="daily-fetch" aria-label={m.bird_weather_fetch({}, {locale})}>
 <button class="fetch" type="button" disabled={disabled||busy||!context||generation===null} onclick={()=>void fetchDaily()}>{busy?m.weather_history_working({}, {locale}):m.bird_weather_fetch({}, {locale})}</button>
 {#if !result}<p class="hint">{m.bird_weather_daily_hint({}, {locale})}</p>{/if}
 {#if busy}<p role="status">{m.weather_history_working({}, {locale})}</p>{/if}
 {#if error}<p class="error" role="alert">{message}</p>{/if}
 {#if result}
  <div class="preview"><h3>{m.bird_weather_daily_preview({}, {locale})}</h3>
   <BirdVisitWeather records={[result.record]} {context} {locale} todayForecast={result.todayForecast} inline/>
   <p class="hint">{m.bird_weather_fetched_separate({}, {locale})}</p>
   <button class="use" type="button" disabled={disabled||busy} onclick={()=>void apply()}>{m.bird_weather_apply_daily({}, {locale})}</button>
   <button type="button" disabled={busy} onclick={discard}>{m.weather_history_discard({}, {locale})}</button>
  </div>
 {/if}
</section>
<style>
 .daily-fetch{margin:20px 0 4px;padding-top:16px;border-top:1px solid #e1e2d9;font:13px/1.5 system-ui,sans-serif}.hint{font-size:12px;color:var(--text-muted,#686d63)}button{min-height:44px;padding:8px 14px;margin:0 8px 4px 0;border:1px solid #cdd1c6;border-radius:6px;background:transparent;color:#435b47;font:inherit;cursor:pointer}.use{background:#435b47;color:#faf9f5;border-color:#435b47}.preview{margin-top:12px}h3{font:500 14px system-ui,sans-serif}.error{color:#9a403b}button:disabled{opacity:.55;cursor:default}button:focus-visible{outline:2px solid #788e7b;outline-offset:3px}
</style>
