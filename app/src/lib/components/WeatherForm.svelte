<script lang="ts">
  import { untrack } from 'svelte';
  import WeatherIcon from '$lib/components/WeatherIcon.svelte';
  import { weatherIcon, weatherConditionKey, type WeatherPart } from '$lib/weather';
  import { commonWeatherCodes, manualNumericFields, makeDraft, parseDraft, type NumericField } from '$lib/weather-form';
  import { weatherLabels } from '$lib/weather-labels';
  let { manual, date, locale, busy, onSave, onCancel, onDirty }: {manual: WeatherPart | null; date: string; locale:'sv'|'en'; busy:boolean; onSave:(part:WeatherPart)=>void; onCancel:()=>void; onDirty:(value:boolean)=>void} = $props();
  let draft = $state(untrack(() => makeDraft(manual, locale)));
  const initial = JSON.stringify(draft);
  const baseline = JSON.parse(initial);
  let errors = $state<Partial<Record<NumericField|'code'|'empty',true>>>({});
  const label = (key:string) => weatherLabels[key as keyof typeof weatherLabels]?.({}, {locale}) ?? key;
  $effect(() => { onDirty(JSON.stringify(draft) !== initial); });
  export function getPart(): WeatherPart|null { const parsed=parseDraft(draft,manual,date,locale,baseline);errors=parsed.errors;return Object.keys(errors).length ? null : parsed.result; }
  function save(event:SubmitEvent) { event.preventDefault();const part=getPart();if(part)onSave(part); }
  const mainFields: NumericField[] = ['temperatureC','windSpeedMs','precipitationMm'];
  const moreFields: NumericField[] = ['cloudCoverPercent','relativeHumidityPercent','snowDepthMm'];
  const errorLabel = (key:NumericField) => key === 'temperatureC' ? 'form_invalidTemperature' : ['cloudCoverPercent','relativeHumidityPercent'].includes(key) ? 'form_invalidPercent' : 'form_invalidNonnegative';
  const units: Record<NumericField,string> = {temperatureC:'°C',windSpeedMs:'m/s',precipitationMm:'mm',cloudCoverPercent:'%',relativeHumidityPercent:'%',snowDepthMm:'mm'};
</script>
<form onsubmit={save}>
  <fieldset disabled={busy}>
    <legend>{label('conditions')}</legend>
    <div class="choices">
      <label class:selected={draft.code === null}><input type="radio" name="condition" checked={draft.code === null} onchange={() => {draft.code = null;}} /><WeatherIcon name="neutral" size={23}/><span>{label('form_none')}</span></label>
      {#each commonWeatherCodes as code}
        <label class:selected={draft.code === code}><input type="radio" name="condition" checked={draft.code === code} onchange={() => {draft.code = code;}} /><WeatherIcon name={weatherIcon(code)} size={23}/><span>{label(weatherConditionKey(code)!)}</span></label>
      {/each}
      {#if baseline.code !== null && !commonWeatherCodes.some(code => code === baseline.code)}
        <label class:selected={draft.code === baseline.code}><input type="radio" name="condition" checked={draft.code === baseline.code} onchange={() => {draft.code = baseline.code;}} /><WeatherIcon name={weatherIcon(baseline.code)} size={23}/><span>{label(weatherConditionKey(baseline.code) ?? 'conditionUnknown')} · {label('form_existingCondition')}</span></label>
      {/if}
    </div>
    <div class="fields">
      {#each mainFields as key}
        <label>{label(key)} <span>({units[key]})</span><input type="text" inputmode="decimal" bind:value={draft.values[key]} aria-invalid={!!errors[key]} />{#if errors[key]}<small role="alert">{label(errorLabel(key))}</small>{/if}</label>
      {/each}
    </div>
    <div class="fields"><label class="description">{label('description')}<textarea rows="2" bind:value={draft.description}></textarea></label></div>
    <details><summary>{label('form_moreFields')}</summary><div class="fields">
      {#each moreFields as key}
        <label>{label(key)} <span>({units[key]})</span><input type="text" inputmode="decimal" bind:value={draft.values[key]} aria-invalid={!!errors[key]} />{#if errors[key]}<small role="alert">{label(errorLabel(key))}</small>{/if}</label>
      {/each}
    </div></details>
    {#if errors.empty}<p role="alert">{label('form_requiredObservation')}</p>{/if}
    <div class="actions"><button class="primary" type="submit">{label('form_save')}</button><button type="button" onclick={onCancel}>{label('form_cancel')}</button></div>
  </fieldset>
</form>
<style>
 fieldset { border:0; margin:0; padding:0; min-width:0; } legend { margin-bottom:10px; font-weight:600; }
 .choices { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:6px; }
 .choices label { min-width:0; display:flex; flex-direction:column; gap:4px; align-items:center; justify-content:center; text-align:center; border:1px solid #d9d8d1; border-radius:7px; padding:8px 3px; color:#435b47; font-size:10px; cursor:pointer; }
 .choices label.selected { border:2px solid #435b47; background:#edf1e9; padding:7px 2px; } .choices label:has(input:focus-visible) { outline:2px solid #435b47; outline-offset:2px; } .choices input { position:absolute; opacity:0; }
 .fields { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; margin-top:16px; } .fields label { display:block; font-size:12px; } .fields label span { color:var(--text-muted,#686d63); } .fields input,.fields textarea { display:block; width:100%; padding:7px; margin-top:4px; border:1px solid #c8ccc3; border-radius:5px; background:var(--surface, #faf9f6); color:var(--text, #252b27); font:inherit; } .fields textarea { resize:vertical; min-height:4em; } .description { grid-column:1/-1; } small,[role=alert] { color:#9d3b32; } details { margin-top:16px; } summary { cursor:pointer; color:var(--text-muted,#686d63); font-size:12px; } .actions { display:flex; gap:8px; margin-top:18px; } button { padding:7px 12px; cursor:pointer; font:inherit; border:1px solid #d9d8d1; border-radius:6px; background:transparent; color:var(--text, #252b27); }
 button.primary { background:#435b47; border-color:#435b47; color:var(--surface, #faf9f6); }
 button:hover:not(:disabled) { background:#eef1eb; } button.primary:hover:not(:disabled) { opacity:.88; background:#435b47; }
 button:disabled, fieldset:disabled button { opacity:.55; cursor:default; } button:focus-visible, .fields input:focus-visible, .fields textarea:focus-visible { outline:2px solid #435b47; outline-offset:2px; }
</style>
