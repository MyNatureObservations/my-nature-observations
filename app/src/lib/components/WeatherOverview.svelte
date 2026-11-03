<script lang="ts">
  import WeatherIcon from "$lib/components/WeatherIcon.svelte";
  import { weatherOverview, weatherConditionKey, weatherNumber, weatherIsCurrent, weatherClock, type WeatherView } from "$lib/weather";
  import { weatherLabels } from "$lib/weather-labels";
  let { data, locale, compact=false }: { data: WeatherView; locale: "sv" | "en";compact?:boolean } = $props();
  const view = $derived(weatherOverview(data));
  const current=$derived(weatherIsCurrent(data.fetched));
  const currentShown=$derived(current && (view.temperature?.source==='fetched' || view.condition.source==='fetched'));
  const label = (key: string) => weatherLabels[key as keyof typeof weatherLabels]?.({}, { locale }) ?? key;
  const condition = $derived(view.condition.code != null && weatherConditionKey(view.condition.code) ? label(weatherConditionKey(view.condition.code)!) : label('conditionUnknown'));
</script>

{#snippet meteo()}
  <span class="meteo"><span aria-hidden="true">{label('meteo')}</span><span class="sr-only">{label('fetched')}</span></span>
{/snippet}
{#if compact}
  <span class="compact-summary"><WeatherIcon name={view.icon} size={18}/><span class="sr-only">{label('heading')} · {condition}{#if currentShown} · {label('currentHeading')}{/if}</span>{#if view.temperature}<span title={label(view.temperature.field)}>{weatherNumber(view.temperature.value,locale)} °C</span>{:else}<span>{condition}</span>{/if}{#if currentShown}<span title={label('validAt')}>{weatherClock(data.fetched!.validAt!)}</span>{/if}</span>
{:else}
<span class="overview">
  <span class="sr-only">{label('heading')}</span>
  <span class="hero">
    <span class="condition" title={condition}>
      <WeatherIcon name={view.icon} size={36} />
      <span class="sr-only">{condition}</span>
      {#if view.condition.source === 'fetched' && !view.onlyFetched}{@render meteo()}{/if}
    </span>
    {#if view.temperature}
      <span class="temperature" title={`${label(view.temperature.field)}: ${weatherNumber(view.temperature.value,locale)} °C`}>
        <span class="sr-only">{label(view.temperature.source)} · {label(view.temperature.field)}</span>
        <strong>{weatherNumber(view.temperature.value,locale)} °C</strong>
        {#if view.temperature.source === 'fetched' && !view.onlyFetched}{@render meteo()}{/if}
      </span>
    {:else}
      <span class="condition-text">{condition}</span>
    {/if}
  </span>
  <span class="supplement">
    {#if view.fetchedTemperature !== null}
      {@render meteo()} <span><span class="sr-only">{label(current?"temperatureC":"temperatureMeanC")}</span>{weatherNumber(view.fetchedTemperature,locale)} °C</span>
    {:else if view.onlyFetched}{@render meteo()}{/if}
  </span>
  <span class="metrics">
    {#each view.metrics as metric}
      {@const value = metric.measurement!}
      <span class="metric" title={`${label(value.field)}: ${weatherNumber(value.value,locale)} ${metric.unit} · ${label(value.source)}`}>
        <span class="sr-only">{label(value.source)} · {label(value.field)}</span>
        <span class="measurement"><WeatherIcon name={metric.icon} size={12} /><span>{weatherNumber(value.value,locale)}</span></span>
        <span class="metric-caption"><span>{metric.unit}</span>{#if value.source === 'fetched' && !view.onlyFetched}{@render meteo()}{/if}</span>
      </span>
    {/each}
  </span>
</span>
{/if}

<style>
  .overview { display: block; }
  .compact-summary {display:inline-flex;align-items:center;gap:6px;font:12px/1.4 system-ui,sans-serif;color:var(--accent,#435b47);}
  .hero { display: flex; align-items: center; gap: 8px; height: 44px; }
  .condition { display: flex; flex-direction: column; align-items: center; width: 40px; flex-shrink: 0; }
  .temperature { min-width: 0; display: flex; flex-direction: column; align-items: flex-start; }
  strong { display: block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 23px; font-weight: 400; letter-spacing: -1px; line-height: 1.2; }
  .condition-text { font-size: 11px; line-height: 1.3; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
  .meteo { font-size: 8px; line-height: 1.2; letter-spacing: .2px; color: var(--text-muted, #686d63); }
  .supplement { display: flex; align-items: baseline; justify-content: flex-end; gap: 4px; height: 15px; font-size: 10px; line-height: 1.3; color: var(--text-muted, #686d63); }
  .metrics { display: flex; gap: 6px; justify-content: space-between; }
  .metric { display: flex; min-width: 0; flex-direction: column; align-items: flex-start; font-size: 10.5px; line-height: 1.2; }
  .metric-caption { display: flex; align-items: baseline; gap: 3px; font-size: 9px; line-height: 1.1; padding-left: 15px; }
  .measurement { display: flex; align-items: center; gap: 3px; max-width: 100%; }
  .measurement > span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  @media (max-width: 600px) {
    .overview { display: grid; grid-template-columns: minmax(125px,1fr) minmax(0,1fr); column-gap: 12px; }
    .hero { grid-column: 1; grid-row: 1; height: 38px; }
    .condition :global(svg) { width: 28px; height: 28px; }
    .supplement { height: 12px; grid-column: 1; justify-content: flex-start; padding-left: 48px; }
    .metrics { grid-column: 2; grid-row: 1 / 3; align-items: center; }
  }
</style>
