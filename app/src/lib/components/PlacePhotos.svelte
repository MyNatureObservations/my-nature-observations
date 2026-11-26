<script lang="ts">
  import PhotoScope from "$lib/components/PhotoScope.svelte";
  import PhotoThumbnail from "$lib/components/PhotoThumbnail.svelte";
  import { loadPlacePhotos, type PlacePhoto } from "$lib/place-photos";
  import type { ContentPlace } from "$lib/places-overview";
  import * as m from "$lib/paraglide/messages.js";

  let { place, generation, locale }: { place: ContentPlace; generation: number; locale: "sv" | "en" } = $props();
  const initialCount = 6;
  let open = $state(false);
  let expanded = $state(false);
  let loading = $state(false);
  let failed = $state(false);
  let incomplete = $state(false);
  let items = $state<PlacePhoto[]>([]);

  $effect(() => {
    if (!open) return;
    let active = true;
    items = [];
    loading = true;
    failed = false;
    incomplete = false;
    void loadPlacePhotos(place, generation, locale, () => active).then(result => {
      if (!active) return;
      items = result.items;
      incomplete = result.incomplete;
    }).catch(() => {
      if (active) failed = true;
    }).finally(() => {
      if (active) loading = false;
    });
    return () => {
      active = false;
      items = [];
    };
  });
</script>

<details class="place-photos" ontoggle={(event) => { open = event.currentTarget.open; if (!open) expanded = false; }}>
  <summary>{m.place_photos_heading({}, { locale })}</summary>
  {#if open}
    {#if loading}<p role="status">{m.place_photos_loading({}, { locale })}</p>
    {:else if failed}<p role="alert">{m.photo_viewer_failed({}, { locale })}</p>
    {:else}
      {#if incomplete}<p role="status">{m.photo_viewer_incomplete({}, { locale })}</p>{/if}
      {#if !items.length && !incomplete}<p>{m.place_photos_empty({}, { locale })}</p>{/if}
      {#if items.length}
        <PhotoScope version={`${place.id}:${generation}`} {locale} load={async () => ({ items, incomplete })}>
          <ul>
            {#each (expanded ? items : items.slice(0, initialCount)) as item (item.key)}
              <li>
                <PhotoThumbnail {item} {locale} compact />
                {#if item.caption}<p class="caption">{item.caption}</p>{/if}
                <a href={item.parentHref}>{item.parentKind === "journal"
                  ? m.place_photos_open_journal({}, { locale })
                  : m.place_photos_open_observation({}, { locale })}
                  {#if item.description}<span>{item.description}</span>{/if}
                </a>
              </li>
            {/each}
          </ul>
        </PhotoScope>
        {#if items.length > initialCount}
          <button type="button" aria-expanded={expanded} onclick={() => expanded = !expanded}>
            {expanded ? m.place_photos_show_fewer({}, { locale }) : m.places_show_all({ count: items.length }, { locale })}
          </button>
        {/if}
      {/if}
    {/if}
  {/if}
</details>

<style>
  .place-photos { border-top: 1px solid #d9d8d1; margin-top: 12px; padding-top: 8px; }
  summary, a, button { color: #435b47; }
  summary, button { cursor: pointer; padding: 6px 4px; }
  button { background: transparent; border: 1px solid #d9d8d1; border-radius: 4px; font: inherit; }
  ul { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; margin: 8px 0; padding: 0; list-style: none; }
  li { min-width: 0; overflow-wrap: anywhere; }
  .place-photos :global(li img) { width: auto; max-width: 100%; height: auto; max-height: 88px; }
  p, a { font: 12px/1.45 system-ui,sans-serif; }
  .caption { margin: 4px 0; white-space: pre-wrap; }
  a { display: block; padding: 4px 0; }
  a span { display: block; color: #686d63; }
  summary, a { border-radius: 4px; }
  .place-photos :global(:is(a, button, summary):focus-visible) { outline: 2px solid #788e7b; outline-offset: -2px; }
</style>
