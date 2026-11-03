<script lang="ts">
  import WeatherIcon from "$lib/components/WeatherIcon.svelte";
  import { weatherSections, weatherConditionKey, weatherIcon, weatherFieldIcon, weatherClock, weatherNumber, weatherIsCurrent, type WeatherPart } from "$lib/weather";
  import { weatherLabels } from "$lib/weather-labels";
  let { part, locale, manual = false }: { part: WeatherPart; locale: "sv" | "en"; manual?: boolean } = $props();
  const rows = $derived(weatherSections(part, locale));
  const label = (key: string) => weatherLabels[key as keyof typeof weatherLabels]?.({}, { locale }) ?? key;
  const temperature = $derived(rows.primary.find(([key]) => key === 'temperatureC') ?? rows.primary.find(([key]) => key === 'temperatureMeanC'));
  const metrics = $derived(rows.primary.filter(([key]) => key !== temperature?.[0] && !['weatherCode', 'description', 'sunrise', 'sunset'].includes(key)));
  const sunTimes = $derived(rows.primary.filter(([key]) => key === 'sunrise' || key === 'sunset'));
  const condition = $derived(part.weatherCode != null ? weatherConditionKey(part.weatherCode) : undefined);
  const current=$derived(!manual && weatherIsCurrent(part));
</script>

{#if current}
  <p class="snapshot-kind">{label('currentHeading')} · {label('modelWeather')}</p>
  <p>{label('validAt')}: <time datetime={part.validAt!}>{part.validAt!.slice(0,10)} · {weatherClock(part.validAt!)}</time></p>
{:else if !manual && part.source?.product==='forecast-daily'}<p class="snapshot-kind">{label('modelDay')} · {label('modelWeather')}</p>
{:else if !manual && part.source?.product==='historical-weather' && part.source?.model==='ERA5'}<p class="snapshot-kind">{label('historicalDay')} · {label('modelWeather')}</p>{/if}
<div class="hero">
  <WeatherIcon name={weatherIcon(part.weatherCode)} size={48} />
  <div>
    {#if temperature}<div class="temperature"><span class="sr-only">{label(temperature[0])}: </span>{temperature[1]}</div>{/if}
    <div class="condition">{condition ? label(condition) : part.weatherCode != null ? `WMO ${part.weatherCode}` : label('conditionUnknown')}</div>
  </div>
</div>
{#if part.description != null}
  {#if manual}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to scroll long descriptions.) -->
    <div class="description manual" tabindex="0" role="region" aria-label={label('description')}>{part.description}</div>
  {:else}<p class="description">{part.description}</p>{/if}
{/if}
<dl class="primary">
  {#each metrics as [key, value]}
    <div class="field">
      <dt><WeatherIcon name={weatherFieldIcon(key)} size={18} /><span>{label(current && key==='precipitationMm'?'currentPrecipitation':key)}{#if current && key==='precipitationMm' && part.currentIntervalSeconds} ({weatherNumber(part.currentIntervalSeconds/60,locale)} min){/if}</span></dt>
      <dd>{value}</dd>
    </div>
  {/each}
</dl>
{#if sunTimes.length}
  <dl class="sun-times">
    {#each sunTimes as [key, value]}
      <div class="field">
        <dt><WeatherIcon name={weatherFieldIcon(key)} size={20} /><span>{label(key)}</span></dt>
        <dd><time datetime={value} title={value}>{weatherClock(value)}</time></dd>
      </div>
    {/each}
  </dl>
{/if}
{#if !manual && part.source?.provider === 'open-meteo'}
  <p class="attribution">{label('history_source')}: <a href="https://open-meteo.com/" target="_blank" rel="noreferrer">Open-Meteo</a>{#if part.source.model} · {part.source.model}{/if} · <a href="https://creativecommons.org/licenses/by/4.0/" target="_blank" rel="noreferrer">{part.source.license}</a></p>
{/if}
{#if rows.details.length}
  <details>
    <summary>{label('details')}</summary>
    <dl class="metadata">{#each rows.details as [key, value]}<dt>{label(key)}</dt><dd>{value}</dd>{/each}</dl>
  </details>
{/if}

<style>
  dl { margin: 0; }
  .snapshot-kind {font-size:12px;color:var(--text-muted,#686d63);margin:0 0 8px;}
  .attribution { margin:16px 0 0; font-size:11px; color:var(--text-muted,#686d63); overflow-wrap:anywhere; } .attribution a { color:inherit; }
  .hero { display: flex; align-items: center; gap: 16px; color: #435b47; min-height: 64px; }
  .hero > div { min-width: 0; }
  .temperature { font-size: 30px; line-height: 1.2; letter-spacing: -.7px; overflow-wrap: anywhere; }
  .condition { font-size: 13px; margin-top: 4px; overflow-wrap: anywhere; }
  .description { margin: 12px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .description.manual { max-height:9em; overflow:auto; }
  .description.manual:focus-visible { outline:2px solid #435b47; outline-offset:3px; }
  .primary, .sun-times { display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 16px; }
  .primary:not(:empty) { margin-top: 22px; }
  .field { min-width: 0; }
  dt { color: var(--text-muted, #686d63); font-size: 11px; display: flex; align-items: center; gap: 6px; }
  dd { margin: 4px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 16px; }
  .sun-times { margin-top: 22px; padding-top: 16px; border-top: 1px solid #e2e2d9; }
  .metadata dt { margin-top: 10px; }
  .metadata dd { font-size: 12px; }
  details { margin-top: 20px; }
  summary { color: var(--text-muted, #686d63); font-size: 12px; cursor: pointer; }
  summary:focus-visible { outline: 2px solid #435b47; outline-offset: 3px; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
</style>
