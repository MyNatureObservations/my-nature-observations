<script lang="ts">
 import {forecastOnRetrievalDay} from "$lib/bird-weather-fetch";
 import {tick} from "svelte";
 import WeatherIcon from "$lib/components/WeatherIcon.svelte";
 import {visitWeatherSummary,weatherStale,type VisitWeather,type VisitContext} from '$lib/bird-weather';
 import {formatBirdDate} from '$lib/bird-names';
 import WeatherOverview from '$lib/components/WeatherOverview.svelte';
 import WeatherDetails from '$lib/components/WeatherDetails.svelte';
 import * as m from '$lib/paraglide/messages.js';
 let {records,context,locale,inline=false,editHref=null,todayForecast=null}:{records:VisitWeather[];context:VisitContext|null;locale:'sv'|'en';inline?:boolean;editHref?:string|null;todayForecast?:boolean|null}=$props();
 const needsReview=$derived(records.some(r=>weatherStale(r,context)));
 const previous=$derived(records.find(r=>weatherStale(r,context)));
 const summary=$derived(visitWeatherSummary(records,context));
 let panel:HTMLDialogElement|undefined=$state(),trigger:HTMLButtonElement|undefined=$state(),opened=$state(false);
 async function show(){panel?.showModal();opened=true;await tick();panel?.querySelector<HTMLButtonElement>('button')?.focus();}
 function close(){panel?.close();opened=false;trigger?.focus({preventScroll:true});}
</script>
{#snippet content()}
 <section aria-label={m.bird_weather_heading({}, {locale})}>
 {#if !inline&&previous}
  {@const original=JSON.parse(previous.context_json) as VisitContext}
  <div class="weather-review" role="group" aria-label={m.bird_weather_needs_review({}, {locale})}>
   <p>{m.bird_weather_stale_question({}, {locale})}</p>
   <p class="review-context">{m.bird_weather_original({}, {locale})}: {original.place_name} · {formatBirdDate(original.date,locale)}{#if original.start||original.end} · {original.start??'…'}–{original.end??'…'}{/if}</p>
   {#if editHref}<div class="review-actions">{#if records.some(r=>r.kind==='manual'&&weatherStale(r,context))}<a class="reuse" href={`${editHref}?weatherReview=use`}>{m.bird_weather_use({}, {locale})}</a>{/if}{#if records.some(r=>r.kind==='fetched'&&weatherStale(r,context))}<a href={`${editHref}?weatherReview=fetch`}>{m.bird_weather_fetch({}, {locale})}</a>{/if}<a href={`${editHref}?weatherReview=remove`}>{m.bird_weather_remove({}, {locale})}</a></div><p class="review-context">{m.bird_weather_review_in_edit({}, {locale})}</p>{/if}
  </div>
 {/if}
 <div class="weather-parts" class:paired={records.length>1}>
 {#each records as record (record.kind)}
  {@const stale=weatherStale(record,context)}
  {@const original=JSON.parse(record.context_json) as VisitContext}
  {@const part=JSON.parse(record.payload_json)}
  <section class="weather-part" class:stale>
   <div class="part-heading"><h3>{record.kind==='manual'?m.bird_weather_manual({}, {locale}):m.bird_weather_fetched_heading({}, {locale})}</h3>{#if stale}<span class="stale-label">{m.bird_weather_previous({}, {locale})}</span>{/if}</div>
   {#if record.kind==='fetched'}
    <p class="scope">{m.bird_weather_daily_date({date:formatBirdDate(original.date,locale)}, {locale})} · {m.bird_weather_not_visit_time({}, {locale})}</p>
    {#if todayForecast??forecastOnRetrievalDay(part)}<p class="forecast">{m.bird_weather_today_forecast({}, {locale})}</p>{/if}
   {/if}
   <WeatherDetails {part} manual={record.kind==='manual'} {locale} compact detailsLabel={m.bird_weather_source_details({}, {locale})}>
    {#snippet contextDetails()}{#if stale}<p class="original">{m.bird_weather_original({}, {locale})}: {original.place_name} · {formatBirdDate(original.date,locale)}{#if original.start||original.end} · {original.start??'…'}–{original.end??'…'}{/if}<br/>{m.bird_weather_coordinates({}, {locale})}: {original.latitude}, {original.longitude}</p>{/if}{/snippet}
   </WeatherDetails>
  </section>
 {/each}
 </div>
 </section>
{/snippet}
{#if records.length}
 {#if inline}{@render content()}
 {:else}
  <button bind:this={trigger} type="button" class="weather-control" aria-haspopup="dialog" aria-expanded={opened} onclick={()=>void show()}>{#if summary.manual||summary.fetched}<WeatherOverview data={summary} {locale} compact/>{:else}<WeatherIcon name="neutral" size={16}/>{/if}<span class:needs-review={needsReview}>{needsReview?m.bird_weather_needs_review({}, {locale}):m.weather_heading({}, {locale})}</span></button>
  <dialog bind:this={panel} class="weather-panel" aria-label={m.weather_heading({}, {locale})} oncancel={event=>{event.preventDefault();close();}}><header><h2>{m.weather_heading({}, {locale})}</h2><button type="button" aria-label={m.weather_close({}, {locale})} onclick={close}>×</button></header>{@render content()}</dialog>
 {/if}
{/if}
<style>
 .needs-review{color:#756146;font-weight:500}.weather-review{margin:6px 0 18px;padding:14px 16px;border:1px solid #ddd7c9;border-radius:8px;background:#f5f3eb}.weather-review p{margin:0 0 10px}.review-context{font-size:12px;color:var(--text-muted,#686d63);overflow-wrap:anywhere}.review-actions{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-bottom:10px}.review-actions a{box-sizing:border-box;display:inline-flex;align-items:center;min-height:44px;padding:9px 14px;border:1px solid #cdd1c6;border-radius:6px;color:#435b47;text-decoration:none;font:13px system-ui,sans-serif}.review-actions .reuse{background:#435b47;color:#faf9f5;border-color:#435b47}.review-actions a:focus-visible{outline:2px solid #788e7b;outline-offset:3px}

 .weather-control{display:inline-flex;align-items:center;gap:6px;padding:4px 6px;border:1px solid transparent;border-radius:6px;background:transparent;color:var(--accent,#435b47);font:12px/1.4 system-ui,sans-serif;min-height:36px;cursor:pointer}.weather-control:hover{border-color:#788e7b}.weather-control:focus-visible{outline:2px solid #788e7b;outline-offset:3px}.weather-panel{box-sizing:border-box;width:min(780px,calc(100vw - 32px));max-height:calc(100dvh - 32px);overflow:auto;padding:18px 22px;border:1px solid #d9d8d1;border-radius:12px;background:#faf9f5;color:#343530;font:13px/1.5 system-ui,sans-serif}.weather-panel::backdrop{background:#34353026}header{display:flex;align-items:center;justify-content:space-between;gap:12px}header h2{font:normal 22px Georgia,serif;margin:0}header button{min-height:40px;min-width:40px;border:0;background:transparent;color:#435b47;font:24px system-ui,sans-serif;cursor:pointer}header button:focus-visible{outline:2px solid #788e7b;outline-offset:3px}

 section{font:13px/1.5 system-ui,sans-serif;color:var(--text-muted,#686d63)}.weather-parts{display:grid;gap:20px;margin:14px 0}.weather-parts.paired{grid-template-columns:repeat(2,minmax(0,1fr))}.weather-part{min-width:0}.part-heading{display:flex;align-items:baseline;gap:8px;flex-wrap:wrap;margin:0 0 8px}h3{font:500 13px system-ui,sans-serif;color:#435b47;margin:0}.scope,.forecast{font-size:12px;line-height:1.5;margin:0 0 10px}.original{font-size:12px;overflow-wrap:anywhere}.stale-label{color:#756146;font-size:11px}.stale h3{color:#756146}@media(max-width:680px){.weather-parts.paired{grid-template-columns:minmax(0,1fr)}}
</style>
