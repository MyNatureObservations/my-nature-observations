<script lang="ts">
  import { tick, getContext, onDestroy, untrack } from "svelte";
  import { PLACE_MAP, type PlaceMapController } from "$lib/place-map.svelte";
  const placeMap = getContext<PlaceMapController>(PLACE_MAP);
  let cancelMap = () => {};
  onDestroy(() => cancelMap());
  import PlaceMap from "$lib/components/PlaceMap.svelte";
  import {
    createPlace,
    filterPlaces,
    listPlaces,
    movePlaceIndex,
    parsePlaceCoordinates,
    readJournalPlace,
    setJournalPlace,
    type JournalPlace,
    type Place,
    type PlaceCoordinates,
  } from "$lib/places";
  import * as m from "$lib/paraglide/messages.js";

  let {
    entryId,
    generation,
    editing,
    locale = "sv",
    defaultCoordinates = null,
    onDraft = () => {},
  }: {
    entryId: string | null;
    generation: number | null;
    editing: boolean;
    locale?: "sv" | "en";
    /** Reserved for R1E3's persisted default place. It only controls map centering. */
    defaultCoordinates?: PlaceCoordinates | null;
    onDraft?: (dirty: boolean, busy: boolean) => void;
  } = $props();

  const emptyLink = (): JournalPlace => ({ place: null, revision: "0" });
  let link = $state<JournalPlace>(emptyLink());
  let places = $state<Place[]>([]);
  let open = $state(false);
  let mode = $state<"list" | "new">("list");
  let query = $state("");
  let selectedId = $state<string | null>(null);
  let activeIndex = $state(-1);
  let name = $state("");
  let latitude = $state("");
  let longitude = $state("");
  let busy = $state(false);
  let dirty = $state(false);
  let loading = $state(false);
  let error = $state("");
  let discardOpen = $state(false);
  let trigger = $state<HTMLButtonElement | undefined>();
  let panel = $state<HTMLDivElement | undefined>();
  let loadSequence = 0;
  let writeSequence = 0;
  let autoAttemptedFor: string | null = null;
  let pending:
    | { kind: "link"; placeId: string }
    | { kind: "create"; name: string; latitude: number | null; longitude: number | null }
    | null = $state(null);
  let pendingPlace = $state<Place | null>(null);

  const filteredPlaces = $derived.by(() => {
    return filterPlaces(places, query, locale);
  });
  const journalCoordinates = $derived.by<PlaceCoordinates | null>(() =>
    link.place && link.place.latitude !== null && link.place.longitude !== null
      ? { latitude: link.place.latitude, longitude: link.place.longitude }
      : null,
  );

  $effect(() => onDraft(dirty || pending !== null, busy));

  $effect(() => {
    const id = entryId;
    const currentGeneration = generation;
    if (!id || currentGeneration === null) {
      link = emptyLink();
      return;
    }
    // Never display a snapshot from another entry or database generation.
    link = emptyLink();
    selectedId = null;
    untrack(() => { void loadLink(id, currentGeneration); });
  });

  $effect(() => {
    const id = entryId;
    const currentGeneration = generation;
    const intent = pending;
    if (!id || currentGeneration === null || !intent || busy || autoAttemptedFor === id) return;
    autoAttemptedFor = id;
    untrack(() => { void applyPending(intent, id, currentGeneration); });
  });

  async function loadLink(id: string, currentGeneration: number) {
    if (busy) return;
    const beforeWrite = writeSequence;
    const sequence = ++loadSequence;
    loading = true;
    if (!pending) error = "";
    try {
      const loaded = await readJournalPlace(id, currentGeneration);
      if (sequence !== loadSequence || id !== entryId || currentGeneration !== generation || beforeWrite !== writeSequence || busy) return;
      link = loaded;
      selectedId = loaded.place?.id ?? null;
    } catch (cause) {
      if (sequence === loadSequence) error = messageFor(cause);
    } finally {
      if (sequence === loadSequence) loading = false;
    }
  }

  async function loadPlaces() {
    if (generation === null) return;
    const currentGeneration = generation;
    const sequence = ++loadSequence;
    loading = true;
    error = "";
    try {
      const loaded = await listPlaces(currentGeneration);
      if (sequence !== loadSequence || currentGeneration !== generation) return;
      places = loaded;
      activeIndex = -1;
    } catch (cause) {
      if (sequence === loadSequence) error = messageFor(cause);
    } finally {
      if (sequence === loadSequence) loading = false;
    }
  }

  async function showPicker() {
    if (!editing || generation === null) return;
    if (placeMap) {
      const id = entryId, currentGeneration = generation;
      cancelMap = placeMap.open({ mode: "journal", generation: currentGeneration, center: journalCoordinates, selectedId: pendingPlace?.id ?? link.place?.id, currentId: link.place?.id,
        remove: link.place || pending ? async () => {
          if ((id !== null && entryId !== id) || generation !== currentGeneration || !editing) return false;
          return removeLink();
        } : undefined,
        select: async (place) => {
          if ((id !== null && entryId !== id) || generation !== currentGeneration || !editing) return false;
          return selectPlace(place);
        },
      });
      return;
    }
    open = true;
    mode = "list";
    query = "";
    activeIndex = -1;
    selectedId = pending?.kind === "link" ? pending.placeId : link.place?.id ?? null;
    error = "";
    await tick();
    panel?.querySelector<HTMLInputElement>("input")?.focus();
    void loadPlaces();
  }

  function startNew() {
    mode = "new";
    selectedId = null;
    name = "";
    latitude = "";
    longitude = "";
    dirty = false;
    error = "";
  }

  function setCoordinates(lat: number, lon: number) {
    latitude = String(lat);
    longitude = String(lon);
    dirty = true;
    error = "";
  }

  async function selectPlace(place: Place) {
    if (busy) return false;
    selectedId = place.id;
    const intent = { kind: "link", placeId: place.id } as const;
    pending = intent;
    pendingPlace = place;
    dirty = true;
    if (!entryId || generation === null) {
      autoAttemptedFor = null;
      error = m.place_waiting_for_entry({}, { locale });
      return true;
    }
    return applyPending(intent, entryId, generation);
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      activeIndex = movePlaceIndex(activeIndex, filteredPlaces.length, event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key === "Enter" && activeIndex >= 0 && filteredPlaces[activeIndex]) {
      event.preventDefault();
      void selectPlace(filteredPlaces[activeIndex]);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closePicker();
    }
  }

  async function saveNew() {
    const trimmedName = name.trim();
    const coordinates = parsePlaceCoordinates(latitude, longitude);
    if (!trimmedName) { error = m.place_name_required({}, { locale }); return; }
    if (!coordinates) { error = m.place_coordinates_invalid({}, { locale }); return; }
    const intent = { kind: "create", name: trimmedName, ...coordinates } as const;
    if (!entryId || generation === null) {
      pending = intent;
      autoAttemptedFor = null;
      dirty = true;
      open = false;
      trigger?.focus();
      return;
    }
    await applyPending(intent, entryId, generation);
  }

  async function applyPending(
    intent: NonNullable<typeof pending>,
    id: string,
    currentGeneration: number,
  ) {
    if (busy) return false;
    autoAttemptedFor = id;
    const sequence = ++writeSequence;
    busy = true;
    error = "";
    try {
      let placeId: string;
      if (intent.kind === "create") {
        const created = await createPlace(
          currentGeneration,
          intent.name,
          intent.latitude,
          intent.longitude,
        );
        placeId = created.id;
        // If linking fails, retry the existing place instead of creating a duplicate.
        pending = { kind: "link", placeId };
        selectedId = placeId;
        if (sequence !== writeSequence || id !== entryId || currentGeneration !== generation) {
          pending = null;
          dirty = false;
          return false;
        }
        places = [...places, created].sort((a, b) => a.name.localeCompare(b.name, locale));
      } else {
        placeId = intent.placeId;
        pending = intent;
      }
      const saved = await setJournalPlace(currentGeneration, id, link.revision, placeId);
      if (sequence !== writeSequence || id !== entryId || currentGeneration !== generation) {
        pending = null;
        dirty = false;
        return false;
      }
      link = saved;
      pending = null;
      pendingPlace = null;
      autoAttemptedFor = null;
      dirty = false;
      open = false;
      mode = "list";
      trigger?.focus();
      return true;
    } catch (cause) {
      if (sequence === writeSequence) {
        error = messageFor(cause);
        if (String(cause) === "place_revision_conflict" && id === entryId && currentGeneration === generation) {
          try {
            const current = await readJournalPlace(id, currentGeneration);
            if (sequence === writeSequence && id === entryId && currentGeneration === generation) {
              link = current;
              selectedId = pending?.kind === "link" ? pending.placeId : current.place?.id ?? null;
            }
          } catch {
            // Keep the original conflict visible and retain the user's pending choice.
          }
        }
      }
      return false;
    } finally {
      if (sequence === writeSequence) busy = false;
    }
  }

  async function removeLink() {
    if (busy) return false;
    if (!entryId || generation === null) {
      pending = null;
      pendingPlace = null;
      dirty = false;
      closePicker(true);
      return true;
    }
    const id = entryId;
    const currentGeneration = generation;
    const sequence = ++writeSequence;
    busy = true;
    error = "";
    try {
      const saved = await setJournalPlace(currentGeneration, id, link.revision, null);
      if (sequence !== writeSequence || entryId !== id || generation !== currentGeneration) return false;
      link = saved;
      pending = null;
      pendingPlace = null;
      selectedId = null;
      dirty = false;
      open = false;
      trigger?.focus();
      return true;
    } catch (cause) {
      if (sequence === writeSequence) error = messageFor(cause);
      return false;
    } finally {
      if (sequence === writeSequence) busy = false;
    }
  }

  function closePicker(force = false) {
    if (busy) return;
    if (!force && dirty) { discardOpen = true; return; }
    open = false;
    discardOpen = false;
    dirty = pending !== null;
    selectedId = link.place?.id ?? null;
    mode = "list";
    error = "";
    trigger?.focus();
  }

  function messageFor(cause: unknown): string {
    const code = String(cause);
    if (code === "place_revision_conflict") return m.place_revision_conflict({}, { locale });
    if (code === "journal_generation_conflict") return m.place_generation_conflict({}, { locale });
    if (code === "place_coordinates_invalid") return m.place_coordinates_invalid({}, { locale });
    return m.place_save_failed({}, { locale });
  }

  function coordinates(value: { latitude: number | null; longitude: number | null }): string {
    return value.latitude === null || value.longitude === null
      ? m.place_no_coordinates({}, { locale })
      : `${value.latitude}, ${value.longitude}`;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      closePicker();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if editing || link.place || error}
<div class="entry-place">
  {#if editing}
    <button bind:this={trigger} class="place-trigger" type="button" onclick={showPicker} disabled={generation === null || busy || loading} aria-haspopup="dialog" aria-expanded={placeMap?.request?.mode === "journal" || open}>
      <span aria-hidden="true">⌖</span>
      {pendingPlace?.name ?? link.place?.name ?? m.place_choose({}, { locale })}
    </button>
  {:else if link.place}
    <span class="place-read"><span aria-hidden="true">⌖</span>{link.place.name}</span>
  {/if}
  {#if loading && !open}<span class="sr-only">{m.place_loading({}, { locale })}</span>{/if}
  {#if error && !open}<span class="inline-error" role="alert">{error}</span>{/if}
</div>
{/if}

{#if open}
  <div class="overlay" role="button" tabindex="-1" aria-label={m.place_close({}, { locale })} onclick={(event) => event.target === event.currentTarget && closePicker()} onkeydown={(event) => event.key === "Escape" && closePicker()}>
    <div bind:this={panel} class="panel" class:map-mode={mode === "new"} role="dialog" aria-modal="true" aria-labelledby="place-title">
      <header><h2 id="place-title">{m.place_panel_title({}, { locale })}</h2><button type="button" class="close" onclick={() => closePicker()} aria-label={m.place_close({}, { locale })}>×</button></header>
      <div class="panel-content">
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        {#if mode === "list"}
        <label class="search-label">{m.place_search({}, { locale })}<input type="search" role="combobox" aria-autocomplete="list" aria-expanded="true" aria-controls="place-results" aria-activedescendant={activeIndex >= 0 && filteredPlaces[activeIndex] ? `place-option-${filteredPlaces[activeIndex].id}` : undefined} bind:value={query} oninput={() => { activeIndex = -1; error = ""; }} onkeydown={handleSearchKeydown} /></label>
        <div id="place-results" class="place-list" role="listbox" aria-label={m.place_saved({}, { locale })} aria-busy={busy}>
          {#each filteredPlaces as place, index (place.id)}
            <button id={`place-option-${place.id}`} type="button" role="option" aria-selected={link.place?.id === place.id} class:active={activeIndex === index} disabled={busy} onmousemove={() => activeIndex = index} onclick={() => selectPlace(place)}>
              <span class="place-result"><strong>{place.name}</strong><small>{coordinates(place)}</small></span>
              {#if link.place?.id === place.id}<span class="check" aria-label={m.place_current({}, { locale })}>✓</span>{/if}
            </button>
          {:else}
            <p class="empty">{m.place_no_matches({}, { locale })}</p>
          {/each}
        </div>
        <div class="actions split"><button type="button" onclick={startNew} disabled={busy}>{m.place_new({}, { locale })}</button><button type="button" onclick={() => closePicker()} disabled={busy}>{m.place_cancel({}, { locale })}</button></div>
        {#if link.place}<button class="remove" type="button" onclick={removeLink} disabled={busy}>{m.place_remove({}, { locale })}</button>{/if}
        {:else}
          <div class="form">
          <PlaceMap {latitude} {longitude} {places} {locale} journalCenter={journalCoordinates} defaultCenter={defaultCoordinates} onCoordinates={setCoordinates} />
          <label>{m.place_name({}, { locale })}<input bind:value={name} maxlength="512" oninput={() => dirty = true} /></label>
          <div class="coordinates">
            <label>{m.place_latitude({}, { locale })}<input inputmode="decimal" bind:value={latitude} oninput={() => dirty = true} /></label>
            <label>{m.place_longitude({}, { locale })}<input inputmode="decimal" bind:value={longitude} oninput={() => dirty = true} /></label>
          </div>
          <p>{m.place_coordinates_optional({}, { locale })}</p>
          </div>
          <div class="actions"><button type="button" onclick={() => { mode = "list"; dirty = false; error = ""; }}>{m.place_back({}, { locale })}</button><button class="primary" type="button" onclick={saveNew} disabled={busy}>{m.place_save_use({}, { locale })}</button></div>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if discardOpen}
  <div class="discard" role="alertdialog" aria-modal="true" aria-labelledby="place-discard-title">
    <div><h2 id="place-discard-title">{m.place_discard_title({}, { locale })}</h2><p>{m.place_discard_description({}, { locale })}</p><footer><button type="button" onclick={() => discardOpen = false}>{m.place_continue({}, { locale })}</button><button type="button" onclick={() => closePicker(true)}>{m.place_discard({}, { locale })}</button></footer></div>
  </div>
{/if}

<style>
  .place-read { min-width: 0; overflow-wrap: anywhere; }
  .place-read > span { flex-shrink: 0; }
  .entry-place{min-height:28px;display:flex;align-items:center;gap:8px}.place-trigger{border:0;background:transparent;color:var(--accent,#435b47);padding:5px 8px;border-radius:6px;font:13px system-ui,sans-serif;cursor:pointer}.place-trigger:hover,.place-trigger:focus-visible{background:#eef1eb}.place-read{display:flex;gap:6px;align-items:center;color: var(--text-muted, #686d63);font:12px system-ui,sans-serif;letter-spacing:.02em}.inline-error{max-width:260px;color:#9a403b;font:11px system-ui,sans-serif}.overlay{position:fixed;inset:0;z-index:40;background:rgba(39,43,38,.18);display:flex;align-items:center;justify-content:center;padding:16px}.panel{display:flex;flex-direction:column;width:min(780px,calc(100vw - 32px));height:min(820px,calc(100vh - 32px));max-height:calc(100vh - 32px);overflow:hidden;background:#f8f7f3;border:1px solid #d9d8d1;border-radius:14px;box-shadow:0 18px 50px rgba(35,39,34,.16);padding:22px}.panel header{flex:0 0 auto;display:flex;align-items:center;justify-content:space-between;padding-bottom:10px;border-bottom:1px solid #e2e1da}.panel-content{min-height:0;overflow:auto;padding:0 3px 3px}.panel h2,.discard h2{font:normal 22px Georgia,serif;margin:0}.close{border:0;background:transparent;font-size:24px}.panel label{display:grid;gap:6px;margin:16px 0 10px;color:#5d625a;font:12px system-ui,sans-serif}.panel label.search-label{gap:6px;margin-bottom:0}.panel input{width:100%;border:1px solid #cccec7;border-radius:8px 8px 0 0;background:#fffefa;padding:10px;color:#343530;font:15px system-ui,sans-serif}.place-list{display:grid;max-height:min(45vh,360px);overflow:auto;border:1px solid #cccec7;border-top:0;border-radius:0 0 8px 8px;background:#fffefa}.place-list button{display:flex;align-items:center;justify-content:space-between;gap:12px;text-align:left;border:0;border-bottom:1px solid #e4e3dc;background:transparent;padding:10px;color:#343530}.place-list button:last-of-type{border-bottom:0}.place-list button:hover,.place-list button.active,.place-list button:focus-visible{background:#eef1eb;outline:2px solid #788e7b;outline-offset:-2px}.place-list button:disabled{cursor:wait;opacity:.65}.place-result{display:grid;gap:2px}.place-result small,.form p,.empty{color: var(--text-muted, #686d63);font:12px system-ui,sans-serif}.check{color:#607763;font:16px system-ui,sans-serif}.empty{margin:0;padding:14px}.actions{position:sticky;bottom:-3px;z-index:2;display:flex;justify-content:space-between;gap:10px;margin-top:20px;padding:12px 0 2px;background:linear-gradient(to bottom,rgba(248,247,243,0),#f8f7f3 12px)}.actions button,.remove,.discard button{border:1px solid #d3d4ce;border-radius:8px;background:#f8f7f3;padding:9px 13px;color:#435b47}.actions .primary{background:#607763;color:white;border-color:#607763}.remove{margin-top:10px;color:#8b4b47}.coordinates{display:grid;grid-template-columns:1fr 1fr;gap:12px}.error{color:#9a403b;font:13px system-ui,sans-serif}.discard{position:fixed;inset:0;z-index:50;background:rgba(39,43,38,.3);display:grid;place-items:center;padding:20px}.discard>div{max-width:430px;background:#f8f7f3;border-radius:12px;padding:24px}.discard footer{display:flex;justify-content:flex-end;gap:10px}.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)}@media(max-width:560px),(max-height:680px){.overlay{padding:8px}.panel{width:calc(100vw - 16px);height:calc(100vh - 16px);max-height:calc(100vh - 16px);padding:16px}.coordinates{grid-template-columns:1fr}.actions.split{align-items:flex-start;flex-direction:column}}
  .panel-content:has(.form) { display: flex; flex-direction: column; }
  .overlay { z-index: 200; }
  .discard { z-index: 201; }
  .form { display: flex; flex-direction: column; }
  .form :global(.map-shell) { flex: 1; }
  .form label { margin: 7px 0 3px; gap: 3px; }
  .form p { margin: 3px 0; }
  .form .coordinates { gap: 10px; }
  .panel-content:has(.form) .actions { margin-top: 6px; padding-top: 7px; }
  @media (max-height: 680px) { .form label { margin: 4px 0 2px; } }
  .panel.map-mode { width: 95vw; height: 95vh; max-height: 95vh; padding: 16px; }
  .panel.map-mode .panel-content { display: flex; flex-direction: column; flex: 1; }
  .panel.map-mode .form { flex: 1 1 auto; min-height: 0; }
  .panel.map-mode .form :global(.map-shell) { display: flex; flex-direction: column; min-height: 180px; margin-top: 8px; }
  .panel.map-mode .form :global(.map) { flex: 1 1 auto; height: 100%; min-height: 180px; }
  .panel.map-mode .coordinates { display: contents; }
  .panel.map-mode .form { display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1fr) minmax(0, 1fr); grid-template-rows: minmax(180px, 1fr) auto auto auto; column-gap: 12px; align-content: stretch; }
  .panel.map-mode .form :global(.map-shell), .panel.map-mode .form :global(.map-help), .panel.map-mode .form > p { grid-column: 1 / -1; }
  .panel.map-mode .actions { flex: 0 0 auto; margin-top: 4px; }
  @media (max-width: 650px) { .panel.map-mode .form { display: flex; flex-direction: column; min-height: 360px; } .panel.map-mode .coordinates { display: grid; } .panel.map-mode .form :global(.map-shell) { flex: 1 1 auto; min-height: 180px; } }
  @media (max-height: 560px) { .panel.map-mode .form { min-height: 280px; } .panel.map-mode .form :global(.map-shell), .panel.map-mode .form :global(.map) { min-height: 160px; } }
</style>
