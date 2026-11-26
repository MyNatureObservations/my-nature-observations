<script lang="ts">
  import { getContext } from "svelte";
  import { PLACE_MAP, type PlaceMapController } from "$lib/place-map.svelte";
  import * as m from "$lib/paraglide/messages.js";

  let { placeId, generation, locale }: { placeId: string; generation: number; locale: "sv" | "en" } = $props();
  const map = getContext<PlaceMapController | undefined>(PLACE_MAP);
</script>

{#if map && placeId}
  <button type="button" aria-haspopup="dialog" aria-controls="navigation-map"
    disabled={map.busy}
    onclick={(event) => {
      event.currentTarget.focus({ preventScroll: true });
      map.open({ mode: "orientation", generation, selectedId: placeId });
    }}>{m.places_show_map({}, { locale })}</button>
{/if}

<style>
  button { border: 0; padding: 6px 4px; min-height: 36px; font: 13px system-ui,sans-serif; color: #435b47; background: transparent; text-decoration: underline; cursor: pointer; }
  button:focus-visible { outline: 2px solid #788e7b; outline-offset: 2px; }
</style>
