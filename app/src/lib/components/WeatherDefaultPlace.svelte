<script lang="ts">
  import { onMount, onDestroy, tick, getContext } from "svelte";
  import { PLACE_MAP, type PlaceMapController } from "$lib/place-map.svelte";
  const placeMap = getContext<PlaceMapController>(PLACE_MAP);
  let cancelMap = () => {};
  import { invoke } from "@tauri-apps/api/core";
  import { language } from "$lib/language.svelte";
  import { filterPlaces, listPlaces, mapDraftCoordinates, readWeatherDefaultPlace, setWeatherDefaultPlace, type Place, type WeatherDefaultPlace } from "$lib/places";
  import * as m from "$lib/paraglide/messages.js";
  let value = $state<WeatherDefaultPlace>({ place: null, revision: "0" });
  let generation = $state<number | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let open = $state(false);
  let query = $state("");
  let places = $state<Place[]>([]);
  let error = $state("");
  let input = $state<HTMLInputElement>();
  let trigger = $state<HTMLButtonElement>();
  let sequence = 0;
  let disposed = false;
  const options = () => ({ locale: language.current });
  const results = $derived(filterPlaces(places, query, language.current));
  const eligible = (place: Place) => place.latitude !== null && place.longitude !== null && mapDraftCoordinates(String(place.latitude), String(place.longitude)) !== null;
  onMount(() => { void load(); });
  onDestroy(() => { disposed = true; sequence++; cancelMap(); });
  async function load(conflict = false) {
    const request = ++sequence;
    loading = true;
    try {
      const session = await invoke<{ generation: number }>("new_journal_session");
      const next = await readWeatherDefaultPlace(session.generation);
      const current = await invoke<{ generation: number }>("new_journal_session");
      if (disposed || request !== sequence) return;
      if (session.generation !== current.generation) throw "journal_generation_conflict";
      value = next; generation = session.generation;
      error = conflict ? "conflict" : "";
    } catch { if (!disposed && request === sequence) { generation = null; error = "failed"; } }
    finally { if (!disposed && request === sequence) loading = false; }
  }
  async function choose() {
    if (generation === null || busy || loading) return;
    if (placeMap) {
      const currentGeneration = generation;
      cancelMap = placeMap.open({ mode: "default", generation, center: value.place, selectedId: value.place?.id,
        select: async (place) => {
          if (disposed || generation !== currentGeneration) return false;
          await save(place.id);
          return !error && value.place?.id === place.id;
        },
      });
      return;
    }
    const request = ++sequence;
    loading = true; error = "";
    try {
      const next = await listPlaces(generation);
      if (disposed || request !== sequence) return;
      places = next; query = ""; open = true;
      await tick(); input?.focus();
    } catch { if (!disposed && request === sequence) error = "failed"; }
    finally { if (!disposed && request === sequence) loading = false; }
  }
  async function save(id: string | null) {
    if (generation === null || busy || loading) return;
    busy = true; error = "";
    const request = ++sequence;
    try {
      const next = await setWeatherDefaultPlace(generation, value.revision, id);
      if (disposed || request !== sequence) return;
      value = next; open = false;
      await tick(); trigger?.focus();
    } catch (cause) {
      if (disposed || request !== sequence) return;
      if (typeof cause === "string" && /revision_conflict|generation/.test(cause)) {
        open = false; await load(true);
      } else error = "failed";
    } finally { if (!disposed) busy = false; }
  }
  function close() { if (!busy) { open = false; trigger?.focus(); } }
</script>

<section aria-labelledby="weather-default-heading">
  <h3 id="weather-default-heading">{m.weather_default_title({}, options())}</h3>
  <p>{m.weather_default_description({}, options())}</p>
  {#if !loading}<p class="value">{value.place?.name ?? m.weather_default_none({}, options())}</p>{/if}
  <button class="location-action" bind:this={trigger} disabled={busy || loading || generation === null} onclick={choose}>{value.place ? m.weather_default_change({}, options()) : m.weather_default_choose({}, options())}</button>
  {#if value.place}<button class="location-action" disabled={busy || loading || generation === null} onclick={() => save(null)}>{m.weather_default_clear({}, options())}</button>{/if}
  {#if generation === null && !loading}<button onclick={() => load()}>{m.weather_default_retry({}, options())}</button>{/if}
  {#if open}
    <div class="picker" tabindex="-1" onkeydown={(event) => { if (event.key === "Escape") { event.stopPropagation(); close(); } }} role="dialog" aria-label={m.weather_default_choose({}, options())}>
      <input bind:this={input} bind:value={query} disabled={busy} aria-label={m.place_search({}, options())} />
      <div class="results">
        {#each results as place (place.id)}
          <button disabled={busy || !eligible(place)} onclick={() => save(place.id)}>
            {place.name} {#if place.id === value.place?.id}✓{/if}
            <small>{eligible(place) ? `${place.latitude}, ${place.longitude}` : m.weather_default_coordinates({}, options())}</small>
          </button>
        {:else}<p>{m.weather_default_empty({}, options())}</p>{/each}
      </div>
      <button disabled={busy} onclick={close}>{m.weather_default_cancel({}, options())}</button>
    </div>
  {/if}
  {#if error}<p role="status">{error === "conflict" ? m.weather_default_conflict({}, options()) : m.weather_default_failed({}, options())}</p>{/if}
</section>

<style>
  section { margin-top: 1rem; color: inherit; }
  h3 { font-size: .9rem; font-weight: 500; margin: 0 0 .5rem; }
  p { font-size: .8rem; line-height: 1.5; opacity: .85; }
  .value { font-weight: 500; opacity: 1; }
  button, input { font: inherit; color: inherit; background: transparent; border: 1px solid currentColor; border-radius: 4px; padding: .5rem .65rem; }
  button { cursor: pointer; margin: .2rem .3rem .2rem 0; }
  .location-action { display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box; min-width: 40px; min-height: 40px; padding: 4px 8px; border-color: var(--border, #d9d8d1); border-radius: 6px; color: var(--accent, #435b47); font-size: 13px; font-weight: 400; line-height: 1.4; }
  .location-action:hover:not(:disabled) { background: var(--surface, #eeeee8); }
  .location-action:focus-visible { outline: 2px solid var(--accent, #435b47); outline-offset: 2px; }
  button:disabled { opacity: .45; cursor: default; }
  :is(button,input):focus-visible { outline: 2px solid currentColor; outline-offset: 3px; }
  .picker { margin-top: .7rem; }
  input { width: 100%; box-sizing: border-box; }
  .results { max-height: 220px; overflow-y: auto; }
  .results button { display: block; width: 100%; text-align: left; }
  small { display: block; font-size: .75rem; opacity: .75; }
</style>
