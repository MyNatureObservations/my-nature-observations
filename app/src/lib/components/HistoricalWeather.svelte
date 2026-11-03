<script lang="ts">
  import { onDestroy, untrack, tick } from "svelte";
  import { listPlaces, readJournalPlace, readWeatherDefaultPlace, filterPlaces, type Place } from "$lib/places";
  import { weatherPlaceDefaults, hasWeatherCoordinates, historicalError, previewHistoricalWeather, saveHistoricalWeather, type HistoricalPreview, type WeatherPlaceOption } from "$lib/weather-history";
  import WeatherDetails from "$lib/components/WeatherDetails.svelte";
  import { weatherLabels } from "$lib/weather-labels";
  import type { WeatherPart } from "$lib/weather";
  let {entryId,generation,date,locale,revision,existing,contextPending=false,blocked=false,onBusy=(_busy:boolean)=>{},onSaved=async()=>{},onConflict=async()=>{},onDone=()=>{},onStep=(_step:"overview"|"fetch"|"compare")=>{}}: {
    entryId:string|null;generation:number;date:string;locale:"sv"|"en";revision:string;existing:WeatherPart|null;blocked?:boolean;contextPending?:boolean;
    onDone?:()=>void;onBusy?:(busy:boolean)=>void;onSaved?:()=>Promise<void>;onConflict?:()=>Promise<void>;onStep?:(step:"overview"|"fetch"|"compare")=>void;
  } = $props();
  const uid=$props.id();
  const label=(key:string)=>weatherLabels[`history_${key}` as keyof typeof weatherLabels]?.({}, {locale}) ?? key;
  let options=$state<WeatherPlaceOption[]>([]), places=$state<Place[]>([]), selected=$state(""), query=$state("");
  let loading=$state(true),busy=$state(false),missingCoordinates=$state(false),error=$state(""),notice=$state("");
  let preview=$state<HistoricalPreview|null>(null);
  let step=$state<"overview"|"fetch"|"compare">("overview");
  function showStep(next:"overview"|"fetch"|"compare"){step=next;onStep(next);}
  let replacementTrigger=$state<HTMLButtonElement>();
  let sequence=0,disposed=false;
  const choice=$derived(options.find(option=>option.key===selected));
  const results=$derived(filterPlaces(places,query,locale).filter(hasWeatherCoordinates));
  function active(ticket:number,id:string|null,gen:number,day:string) {return !disposed && ticket===sequence && entryId===id && generation===gen && date===day;}
  function finish() { if(step==="overview" && notice==="saved" && !busy) onDone(); }
  function invalidate() { sequence++;preview=null;error="";notice=""; }
  // Changes to document identity, date or weather revision invalidate a preview.
  $effect(()=> { entryId;generation;date;revision;contextPending;untrack(()=>{invalidate();showStep("overview");void load();}); });
  onDestroy(()=>{disposed=true;sequence++;onBusy(false);});
  async function load() {
    const ticket=++sequence,id=entryId,gen=generation,day=date;
    loading=true;error="";
    try {
      const [journal,setting,saved]=await Promise.all([id ? readJournalPlace(id,gen) : Promise.resolve({place:null,revision:"0"}),readWeatherDefaultPlace(gen),listPlaces(gen)]);
      if(!active(ticket,id,gen,day))return;
      const defaults=weatherPlaceDefaults(journal,setting);
      options=[...defaults.options,...saved.filter(hasWeatherCoordinates).map(p=>({key:`saved:${p.id}`,name:p.name,latitude:p.latitude!,longitude:p.longitude!,source:{kind:"saved" as const,id:p.id}}))];
      places=saved;selected=defaults.selected;missingCoordinates=defaults.missingCoordinates;
    }catch(cause){if(active(ticket,id,gen,day))error=historicalError(cause);}
    finally{if(!disposed && ticket===sequence)loading=false;}
  }
  function select(key:string){invalidate();selected=key;}
  async function beginReplacement(){
    if(busy||blocked)return;
    invalidate();showStep("fetch");await load();
  }
  export async function discardPreview(){
    if(busy)return;
    invalidate();showStep("overview");
    await tick();replacementTrigger?.focus({preventScroll:true});
  }
  async function fetchPreview(){
    if(!entryId||busy||blocked||contextPending||!choice||loading||step!=="fetch")return;
    const source=choice.source,ticket=++sequence,id=entryId,gen=generation,day=date;
    preview=null;error="";notice="";busy=true;onBusy(true);
    try{
      const result=await previewHistoricalWeather(id,gen,source);
      if(!active(ticket,id,gen,day))return;
      if(result.fetched.period.kind!=="day"||result.fetched.period.date!==day||result.weatherRevision!==revision)throw "historical_weather_context_conflict";
      preview=result;showStep("compare");
    }catch(cause){if(active(ticket,id,gen,day)) {error=historicalError(cause, true);if(error==="conflict")await recover();}}
    finally{if(!disposed){busy=false;onBusy(false);}}
  }
  async function recover(){
    preview=null;showStep("fetch");
    try{await onConflict();await load();}catch{/* The parent retains its warning and write barrier. */}
    error="conflict";
  }
  async function save(confirmReplacement=false){
    if(!entryId||!preview||busy||blocked||contextPending||(existing&&!confirmReplacement))return;
    const value=preview,ticket=++sequence,id=entryId,gen=generation,day=date;
    busy=true;onBusy(true);error="";notice="";
    let committed=false;
    try{
      await saveHistoricalWeather(id,gen,value,existing!==null&&confirmReplacement);
      committed=true;
      if(disposed || entryId!==id || generation!==gen)return;
      preview=null;
      showStep("overview");
      // Parent invalidates its long-lived snapshot before attempting the reread.
      await onSaved();
      if(!disposed)notice="saved";
    }catch(cause){
      if(!disposed && entryId===id && generation===gen){
        if(committed){preview=null;showStep("overview");error="savedUnverified";}
        else{error=historicalError(cause);if(error==="conflict")await recover();}
      }
    }finally{if(!disposed){busy=false;onBusy(false);}}
  }
</script>
<section class="history" aria-labelledby={`${uid}-title`}>
  <h3 id={`${uid}-title`}>{label('title')}</h3>
  {#if step === "overview"}
    {#if existing}<WeatherDetails part={existing} {locale}/>{/if}
    <button bind:this={replacementTrigger} class="replace-control" type="button" disabled={busy||blocked} onclick={beginReplacement}>{label(existing?'replaceSave':'fetch')}</button>
  {:else if step === "fetch"}
  <div id={`${uid}-controls`} class="fetch-controls">
  <p>{label('date')}: <time>{date}</time></p>
  {#if !entryId}<p role="status">{label('needEntry')}</p>{/if}
  {#if contextPending}<p role="status">{label('waitForJournal')}</p>{/if}
  {#if loading}<p role="status">{label('loadingPlaces')}</p>{:else}
    {#if missingCoordinates}<p>{label('missingCoordinates')}</p>{/if}
    <label for={`${uid}-place`}>{label('place')}</label>
    <select id={`${uid}-place`} value={selected} disabled={busy||blocked} onchange={event=>select(event.currentTarget.value)}>
      <option value="">{label('choose')}</option>
      {#each options as option}<option value={option.key}>{option.name} · {option.latitude}, {option.longitude}{option.key==='journal'?` · ${label('journal')}`:option.key==='default'?` · ${label('default')}`:''}</option>{/each}
    </select>
    <details><summary>{label('search')}</summary>
      <label for={`${uid}-search`}>{label('search')}</label><input id={`${uid}-search`} type="search" bind:value={query} disabled={busy||blocked}/>
      <ul>{#each results as place}<li><button type="button" disabled={busy||blocked} aria-pressed={selected===`saved:${place.id}`} onclick={()=>select(`saved:${place.id}`)}>{place.name} · {place.latitude}, {place.longitude}</button></li>{/each}</ul>
      {#if !results.length}<p>{label('noPlaces')}</p>{/if}
    </details>
    {#if choice}<p>{choice.name} · {choice.latitude}, {choice.longitude}</p>{/if}
    {#if !options.length}<p>{label('noPlaces')}</p>{/if}
    <p class="hint">{label('privacy')}</p>
    <button type="button" disabled={!entryId||busy||blocked||!choice} onclick={fetchPreview}>{label(busy?'working':existing?'fetchNew':'fetch')}</button>
    <button type="button" disabled={busy} onclick={discardPreview}>{label('back')}</button>
  {/if}
  </div>
  {:else if step === "compare" && preview}
    <div class="preview" aria-label={label('preview')}>
      <div class="preview-summary">
      <h4>{label('preview')}</h4><p>{preview.fetched.location?.name} · {date}</p>
      {#if existing}<p>{label('replaceHint')}</p>{/if}

      </div>
      {#if existing}
        <div class="comparison">
          <section aria-label={label('current')}><h4>{label('current')}</h4><WeatherDetails part={existing} {locale}/></section>
          <section aria-label={label('new')}><h4>{label('new')}</h4><WeatherDetails part={preview.fetched} {locale}/></section>
        </div>
      {:else}<WeatherDetails part={preview.fetched} {locale}/>{/if}
      <div class="actions"><button type="button" disabled={busy} onclick={discardPreview}>{label(existing?'keepExisting':'back')}</button><button type="button" class="primary" disabled={busy||blocked} onclick={()=>save(true)}>{label(existing?'confirmReplace':'save')}</button></div>
    </div>
  {/if}
  {#if error}<p role="alert">{label(error)}</p>{#if !busy}<button type="button" disabled={blocked} onclick={()=>{invalidate();void load();}}>{label('retry')}</button>{/if}{/if}
  {#if notice}<div class="saved-result"><p role="status">{label(notice)}</p>{#if notice === "saved" && step === "overview"}<button type="button" disabled={busy} onclick={finish}>{label("done")}</button>{/if}</div>{/if}
</section>
<style>
.comparison{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:16px}.comparison section{min-width:0}.comparison section+section{border-left:1px solid var(--border,#d9d8d1);padding-left:16px}.fetch-controls{margin-top:16px}.replace-control{margin-top:16px}@media(max-width:600px){.comparison{grid-template-columns:minmax(0,1fr)}.comparison section+section{border-left:0;border-top:1px solid var(--border,#d9d8d1);padding:16px 0 0}}
 .history{min-width:0}h3,h4{font-size:13px;margin:0 0 12px;color:var(--accent,#435b47)}p{overflow-wrap:anywhere}.hint{font-size:12px;color:var(--text-muted,#686d63)}select,input[type=search]{box-sizing:border-box;width:100%;padding:7px;margin:5px 0 8px;border:1px solid var(--border,#c8ccc3);border-radius:5px;background:var(--surface,#faf9f6);color:var(--text,#252b27);font:inherit}button{padding:7px 12px;border:1px solid var(--border,#d9d8d1);border-radius:6px;background:transparent;color:var(--text,#252b27);font:inherit;cursor:pointer}button.primary{background:var(--accent,#435b47);color:var(--surface,#faf9f6)}button:disabled{opacity:.55;cursor:default}button:focus-visible,input:focus-visible,select:focus-visible,summary:focus-visible{outline:2px solid var(--accent,#435b47);outline-offset:3px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:12px}ul{list-style:none;padding:0}li button{width:100%;text-align:left}details{margin:10px 0}summary{cursor:pointer}.preview{scroll-margin:16px}.preview .actions{margin-bottom:16px}.preview-summary{scroll-margin:72px 0 16px}
</style>
