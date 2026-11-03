<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { readPhotograph, photographSource, type PhotographAsset, type PhotographAttrs } from "$lib/journal-photographs";
  import * as m from "$lib/paraglide/messages.js";
  let { attrs, entryId, generation, locale }: { attrs: PhotographAttrs; entryId: string; generation: number | null; locale: "sv" | "en" } = $props();
  let asset = $state<PhotographAsset | null>(null);
  let failed = $state(false);
  $effect(() => {
    const id = attrs.assetId, entry = entryId, expected = generation;
    let cancelled = false;
    asset = null; failed = false;
    if (expected !== null) {
      void readPhotograph(invoke, entry, expected, id)
        .then(value => { if (!cancelled) asset = value; })
        .catch(() => { if (!cancelled) failed = true; });
    }
    return () => { cancelled = true; };
  });
</script>

<figure data-placement={attrs.placement} data-size={attrs.size} style:--photo-natural-width={asset ? `${asset.width}px` : undefined} class="mno-photograph" aria-label={m.photo_photograph({}, { locale })}>
  {#if asset}
    <img src={photographSource(asset)} width={asset.width} height={asset.height} alt={attrs.alt ?? ""} />
  {:else}
    <span role="status">{failed ? m.photo_unavailable({}, { locale }) : m.photo_loading({}, { locale })}</span>
  {/if}
  {#if attrs.caption}<figcaption>{attrs.caption}</figcaption>{/if}
</figure>
