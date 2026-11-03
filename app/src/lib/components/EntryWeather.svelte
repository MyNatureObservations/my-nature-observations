<script lang="ts">
  import { tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { WeatherPart, WeatherView } from "$lib/weather";
  import WeatherTile from "$lib/components/WeatherTile.svelte";
  import { createWeatherReader, emptyWeather, type WeatherStatus } from "$lib/weather";
  let { entryId, generation, locale, date = "", editing = false, contextPending = false, ensureEntry = async () => {}, onDraft = (_dirty:boolean, _busy:boolean) => {} }: { entryId: string | null; generation: number | null; locale: "sv" | "en"; date?: string; editing?: boolean; contextPending?:boolean; ensureEntry?:()=>Promise<void>; onDraft?: (dirty:boolean,busy:boolean)=>void } = $props();
  let tile = $state<WeatherTile>();
  export async function flushPending():Promise<boolean>{return await tile?.flushManual() ?? true;}
  const read = createWeatherReader();
  let readVersion=0;
  let loadedId:string|null=null,loadedGeneration:number|null=null;
  let status = $state<WeatherStatus>({ kind: "loading" });
  $effect(() => {
    const id = entryId; const expected = generation;
    const ticket=++readVersion;
    let cancelled = false;
    const previous=untrack(()=>status);
    const adoptingFirstId=loadedId===null && id!==null && loadedGeneration===expected;
    loadedId=id;loadedGeneration=expected;
    if(!adoptingFirstId || previous.kind!=="ready") status = id ? {kind:"loading"}:emptyWeather;
    if (id && expected !== null) {
      void read([id], expected).then(data => { if (!cancelled && ticket===readVersion) status = { kind: "ready", data: data[id] }; })
        .catch(() => { if (!cancelled && ticket===readVersion) status = adoptingFirstId && previous.kind==="ready" ? {kind:"unverified",data:previous.data} : { kind: "error" }; });
    }
    return () => { cancelled = true; };
  });
  async function saveManual(manual: WeatherPart | null): Promise<"verified" | "unverified"> {
    if(!entryId){await ensureEntry();await tick();}
    if (!entryId || generation === null || status.kind !== "ready") throw "weather_entry_missing";
    ++readVersion;
    const id = entryId, expected = generation;
    const data = await invoke<WeatherView>("update_manual_weather", { entryId:id, generation:expected, expectedRevision:status.data.revision, manual });
    // The write is confirmed at this point. An unavailable or changed session
    // must not be reported as a failed write or expose a stale snapshot.
    const invalidate = () => {
      if (entryId === id && generation === expected && status.kind === "ready") {
        status = {kind:"unverified", data:status.data};
      }
      return "unverified" as const;
    };
    try {
      const current = await invoke<{generation:number}>("new_journal_session");
      if (entryId !== id || generation !== expected || current.generation !== expected) return invalidate();
    } catch { return invalidate(); }
    status = {kind:"ready", data};
    return "verified";
  }
  async function reload(): Promise<void> {
    if (!entryId || generation === null) throw "weather_entry_missing";
    const id = entryId, expected = generation;
    const ticket=++readVersion;
    const data = await read([id], expected);
    if (entryId !== id || generation !== expected || ticket!==readVersion) throw "journal_generation_conflict";
    status = {kind:"ready",data:data[id]};
  }
  async function historicalSaved(): Promise<void> {
    // Invalidation belongs to this long-lived owner and survives Tile remounts.
    if(status.kind === "ready") status = {kind:"unverified",data:status.data};
    await reload();
  }
</script>
{#key `${generation}:${editing}`}<WeatherTile bind:this={tile} showEmpty {status} {locale} {editing} {date} {entryId} {generation} {contextPending} available={generation !== null} {onDraft} {saveManual} {reload} {historicalSaved} />{/key}
