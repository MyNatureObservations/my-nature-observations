<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { beforeNavigate, goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import PlaceMap from "$lib/components/PlaceMap.svelte";
  import { readWeatherDefaultPlace, listPlaces, createPlace, updatePlace, deletePlace, readPlaceUsage, type PlaceUsage, parsePlaceCoordinates, filterPlaces, mapDraftCoordinates, type Place, type PlaceCoordinates } from "$lib/places";
  import type { MapRequest, PlaceMapController } from "$lib/place-map.svelte";
  import { formatJournalDate } from "$lib/journal-reading";
  import * as m from "$lib/paraglide/messages.js";

  let { locale = "sv", onclose, request = { mode: "orientation" }, controller }: { locale?: "sv" | "en"; onclose: () => void; request?: MapRequest; controller?: PlaceMapController } = $props();
  let generation = $state<number | null>(null);
  let places = $state<Place[]>([]);
  let query = $state("");
  let selected = $state<Place | null>(null);
  let initialSelection = $state<Place | null>(null);
  let name = $state("");
  let form = $state(false);
  let editing = $state<Place | null>(null);
  let baseline = $state("");
  let discardOpen = $state(false);
  let discardAction: (() => void) | null = null;
  let deleteTarget = $state<Place | null>(null);
  let blockedOpen = $state(false);
  let usage = $state<PlaceUsage | null>(null);
  let usageLoading = $state(false);
  let usageError = $state(false);
  let usagePlaceId: string | null = null;
  let usageSequence = 0;
  let blockedDialog = $state<HTMLDivElement>();
  async function loadUsage() {
    if (!blockedOpen || !usagePlaceId || generation === null || disposed) return;
    const ticket = ++usageSequence, gen = generation, id = usagePlaceId;
    usage = null; usageLoading = true; usageError = false;
    try {
      const result = await readPlaceUsage(gen, id);
      if (disposed || !blockedOpen || ticket !== usageSequence || gen !== generation || id !== usagePlaceId) return;
      if (result.placeId !== id) throw "place_usage_read_failed";
      usage = result;
    } catch {
      if (!disposed && blockedOpen && ticket === usageSequence && gen === generation) usageError = true;
    } finally {
      if (!disposed && ticket === usageSequence) usageLoading = false;
    }
  }
  async function openUsageEntry(id: string) {
    if (!blockedOpen || usageLoading || !usage?.entries.some(entry => entry.id === id) || generation === null) return;
    const ticket = usageSequence, gen = generation;
    usageLoading = true;
    try {
      const current = await invoke<{generation:number}>("new_journal_session");
      if (disposed || !blockedOpen || ticket !== usageSequence || gen !== generation) return;
      if (current.generation !== gen) throw "journal_generation_conflict";
      blockedOpen = false; usageSequence++; usage = null; usageLoading = false; error = "";
      onclose();
      await goto(`/journal/entries/${encodeURIComponent(id)}`);
    } catch {
      if (!disposed && blockedOpen && ticket === usageSequence) { usage = null; usageError = true; }
    } finally { if (!disposed && ticket === usageSequence) usageLoading = false; }
  }
  let blockedButton = $state<HTMLButtonElement>();
  let deleteButton = $state<HTMLButtonElement>();
  function dismissBlocked() {
    blockedOpen = false; usageSequence++; usage = null; usageLoading = false; usageError = false; error = "";
    void tick().then(() => { if (!disposed) (deleteButton ?? searchInput)?.focus(); });
  }
  function blockedKeys(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); dismissBlocked(); }
    else if (event.key === "Tab") {
      const controls = blockedDialog?.querySelectorAll<HTMLElement>('a[href],button:not([disabled])');
      if (!controls?.length) { event.preventDefault(); event.stopPropagation(); blockedButton?.focus(); return; }
      const first = controls[0], last = controls[controls.length - 1];
      const outside = !Array.from(controls).includes(document.activeElement as HTMLElement);
      if (event.shiftKey && (document.activeElement === first || outside)) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && (document.activeElement === last || outside)) { event.preventDefault(); first.focus(); }
      event.stopPropagation();
    }
  }
  let feedback = $state("");
  let releaseGuard: (() => void) | undefined;
  function dirty() { return form && JSON.stringify([name, latitude, longitude]) !== baseline; }
  function protect(action: () => void) {
    if (busy || blockedOpen) return false;
    if (dirty()) { discardAction = action; discardOpen = true; return false; }
    action(); return true;
  }
  function resetForm() {
    form = false; editing = null; discardOpen = false;
    name = selected?.name ?? "";
    latitude = selected?.latitude == null ? "" : String(selected.latitude);
    longitude = selected?.longitude == null ? "" : String(selected.longitude);
  }
  function discard() { const action = discardAction; discardAction = null; resetForm(); action?.(); }
  beforeNavigate((navigation) => {
    if (blockedOpen) { navigation.cancel(); blockedButton?.focus(); return; }
    if (busy || dirty()) { navigation.cancel(); if (!busy) protect(resetForm); }
  });
  let menu = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuElement = $state<HTMLDivElement>();
  let searchInput = $state<HTMLInputElement>();
  let nameInput = $state<HTMLInputElement>();
  let activeIndex = $state(-1);
  let busy = $state(false);
  let error = $state("");
  let notice = $state(false);
  const results = $derived(filterPlaces(places, query, locale));
  const eligible = (place: Place) => request.mode !== "default" || (place.latitude !== null && place.longitude !== null && mapDraftCoordinates(String(place.latitude), String(place.longitude)) !== null);
  let center = $state<PlaceCoordinates | null>(null);
  let loaded = $state(false);
  let failed = $state(false);
  let latitude = $state("");
  let longitude = $state("");
  let panel = $state<HTMLElement>();
  let disposed = false;
  onDestroy(() => { disposed = true; usageSequence++; releaseGuard?.(); });
  onMount(() => {
    releaseGuard = controller?.setCloseGuard(() => {
      if (busy || blockedOpen) return false;
      if (dirty()) { protect(() => { resetForm(); onclose(); }); return false; }
      return true;
    });
    center = request.center ?? null;
    panel?.focus();
    void loadCenter();
  });
  async function loadCenter() {
    try {
      const session = await invoke<{ generation: number }>("new_journal_session");
      if (disposed) return;
      if (request.generation !== undefined && request.generation !== session.generation) throw "journal_generation_conflict";
      generation = session.generation;
      try { places = await listPlaces(session.generation); } catch { if (!disposed) error = "load"; }
      if (disposed) return;
      selected = places.find(place => place.id === request.selectedId) ?? null;
      initialSelection = selected;
      const initialPoint = request.center ?? selected;
      if (initialPoint?.latitude != null && initialPoint?.longitude != null) {
        latitude = String(initialPoint.latitude); longitude = String(initialPoint.longitude);
      }
      const setting = await readWeatherDefaultPlace(session.generation);
      const current = await invoke<{ generation: number }>("new_journal_session");
      if (disposed) return;
      if (current.generation !== session.generation) throw "journal_generation_conflict";
      const point = setting.place;
      if (point && Number.isFinite(point.latitude) && Number.isFinite(point.longitude) && Math.abs(point.latitude) <= 90 && Math.abs(point.longitude) <= 180) {
        center = { latitude: point.latitude, longitude: point.longitude };
      }
      center = request.center ?? center;
    } catch { if (!disposed) failed = true; }
    finally { if (!disposed) loaded = true; }
  }
  function mark(lat: number, lon: number) {
    if (busy || disposed) return;
    latitude = String(lat); longitude = String(lon);
    if (!editing) selected = null; notice = false;
  }
  async function contextPoint(lat: number, lon: number, screen?: { x: number; y: number }) {
    if (busy || form) return;
    mark(lat, lon);
    menuX = Math.max(8, Math.min(screen?.x ?? 16, window.innerWidth - 190));
    menuY = Math.max(48, Math.min(screen?.y ?? 60, window.innerHeight - 60));
    menu = true;
    await tick();
    if (!disposed) menuElement?.querySelector<HTMLButtonElement>("button")?.focus();
  }
  async function startCreate(keepPoint = true) {
    protect(() => { if (!keepPoint) { latitude = ""; longitude = ""; } menu = false; editing = null; form = true; name = ""; error = ""; feedback = "";
      baseline = JSON.stringify([name, latitude, longitude]);
      void tick().then(() => { if (!disposed) nameInput?.focus(); });
    });
  }
  function startEdit() {
    if (!selected) return;
    const place = selected;
    protect(() => { editing = place; form = true; name = place.name;
      latitude = place.latitude === null ? "" : String(place.latitude);
      longitude = place.longitude === null ? "" : String(place.longitude);
      baseline = JSON.stringify([name, latitude, longitude]); error = ""; feedback = "";
      void tick().then(() => { if (!disposed) nameInput?.focus(); });
    });
  }
  async function mutationError(cause: unknown, placeId = selected?.id ?? null) {
    const code = String(cause);
    if (code === "place_registry_saved_unverified") { generation = null; places = []; selected = null; resetForm(); notice = false; error = "savedUnverified"; return; }
    if (/generation/.test(code)) { generation = null; error = "generation"; return; }
    error = code.includes("place_in_use") ? "inUse" : code.includes("place_coordinates_required_by_weather_default") ? "defaultCoordinates" : /revision_conflict|place_missing|place_not_found/.test(code) ? "conflict" : "save";
    if (error === "inUse") {
      blockedOpen = true; menu = false; usagePlaceId = placeId;
      void loadUsage();
      await tick();
      if (!disposed) blockedButton?.focus();
    }
    if (error === "conflict" && generation !== null) {
      try { const current = await listPlaces(generation); if (!disposed) { places = current; selected = current.find(p => p.id === selected?.id) ?? null; } } catch { /* Keep the draft and require reopening if reload fails. */ }
    }
  }
  async function verifySession(expected: number) {
    try {
      const current = await invoke<{generation:number}>("new_journal_session");
      if (current.generation !== expected) throw "journal_generation_conflict";
    } catch { throw "place_registry_saved_unverified"; }
  }
  async function erasePlace() {
    if (busy || disposed || generation === null || !deleteTarget) return;
    const target = deleteTarget, gen = generation;
    busy = true; controller?.setBusy(true); error = "";
    try {
      await deletePlace(gen, target.id); await verifySession(gen);
      if (disposed) return;
      places = places.filter(p => p.id !== target.id); if (selected?.id === target.id) { selected = null; latitude = ""; longitude = ""; }
      deleteTarget = null; feedback = "place_registry_deleted"; notice = false;
    } catch (cause) { if (!disposed) { deleteTarget = null; await mutationError(cause, target.id); } }
    finally { busy = false; if (!disposed) controller?.setBusy(false); }
  }
  async function registerPlace(useAfterSave = false) {
    if (busy || generation === null || disposed) return;
    const point = parsePlaceCoordinates(latitude, longitude);
    if (!name.trim()) { error = "invalid"; return; }
    if (!point) { error = "coordinates"; return; }
    busy = true; controller?.setBusy(true); error = "";
    try {
      const gen = generation;
      const saved = editing
        ? await updatePlace(gen, editing, name.trim(), point.latitude, point.longitude)
        : await createPlace(gen, name.trim(), point.latitude, point.longitude);
      await verifySession(gen);
      if (disposed) return;
      const wasEditing = editing !== null;
      places = wasEditing ? places.map(p => p.id === saved.id ? saved : p) : [...places, saved];
      selected = saved; resetForm(); notice = !wasEditing; feedback = wasEditing ? "place_registry_updated" : "";
      if (!wasEditing && useAfterSave && request.mode !== "orientation") {
        if (eligible(saved)) await applySelection(saved, true);
        else error = "weatherCoordinates";
      }
    } catch (cause) { if (!disposed) await mutationError(cause); }
    finally { busy = false; if (!disposed) controller?.setBusy(false); }
  }
  async function confirm() {
    if (form || generation === null || !selected || busy || disposed || !eligible(selected)) return;
    busy = true; controller?.setBusy(true); error = "";
    try {
      await applySelection(selected, notice);
    } catch { if (!disposed) error = "save"; }
    finally { busy = false; if (!disposed) controller?.setBusy(false); }
  }
  async function applySelection(place: Place, registered: boolean) {
    try {
      const saved = await request.select?.(place);
      if (disposed) return;
      if (saved) { busy = false; controller?.setBusy(false); onclose(); }
      else error = registered ? "link" : "save";
    } catch { if (!disposed) error = registered ? "link" : "save"; }
  }
  function close() { protect(onclose); }
  function pick(place: Place) {
    protect(() => { resetForm(); pickNow(place); });
  }
  function pickNow(place: Place) {
    selected = place; notice = false; menu = false; error = ""; feedback = ""; deleteTarget = null;
    latitude = place.latitude === null ? "" : String(place.latitude);
    longitude = place.longitude === null ? "" : String(place.longitude);
  }
  function searchKeys(event: KeyboardEvent) {
    const choices = results;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      activeIndex = choices.length ? (activeIndex + (event.key === "ArrowDown" ? 1 : choices.length - 1) + choices.length) % choices.length : -1;
      if (activeIndex >= 0) document.getElementById(`map-place-${choices[activeIndex].id}`)?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter" && choices[activeIndex]) { event.preventDefault(); pick(choices[activeIndex]); }
  }
  async function remove() {
    if (busy || disposed || !request.remove) return;
    busy = true; controller?.setBusy(true); error = "";
    try {
      const removed = await request.remove();
      if (disposed) return;
      if (removed) { busy = false; controller?.setBusy(false); onclose(); } else error = "save";
    } catch { if (!disposed) error = "save"; }
    finally { busy = false; if (!disposed) controller?.setBusy(false); }
  }
</script>

<svelte:window onpointerdown={(event) => { if (event.button === 0 && menu && !menuElement?.contains(event.target as Node)) menu = false; }} />

<div id="navigation-map" class="map-panel" bind:this={panel} tabindex="-1" role="dialog" aria-label={m.map_panel_title({}, { locale })}
  onkeydown={(event) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if (menu) { menu = false; searchInput?.focus(); } else if (discardOpen) { discardOpen = false; } else if (deleteTarget) { if (!busy) deleteTarget = null; } else if (form && !busy) { protect(() => { resetForm(); searchInput?.focus(); }); } else close(); } }}>
  <header inert={blockedOpen}><h2>{m.map_panel_title({}, { locale })} · {request.mode === "journal" ? m.place_choose({}, { locale }) : request.mode === "default" ? m.weather_default_title({}, { locale }) : m.map_orientation({}, { locale })}</h2><button type="button" disabled={busy} onclick={close} aria-label={m.nav_close_panel({}, { locale })}>×</button></header>
  {#if failed}<p role="status">{m.map_panel_read_failed({}, { locale })}</p>{/if}
  <div class="workspace" inert={blockedOpen}>
  <aside aria-label={m.place_saved({}, { locale })}>
    <label class="search">{m.place_search({}, { locale })}<input bind:this={searchInput} bind:value={query} disabled={busy} oninput={() => activeIndex = -1} onkeydown={searchKeys} aria-controls="map-place-results" /></label>
    <div class="results" id="map-place-results">{#each results as place (place.id)}
      <button id={`map-place-${place.id}`} class:keyboard-active={results[activeIndex]?.id === place.id} type="button" disabled={busy} aria-pressed={selected?.id === place.id} onclick={() => pick(place)}>
        {place.name}{selected?.id === place.id ? " ✓" : ""}<small>{place.latitude !== null ? `${place.latitude}, ${place.longitude}` : m.place_no_coordinates({}, { locale })}</small>
        {#if request.currentId === place.id}<small>{m.map_current_journal_place({}, { locale })}</small>{/if}
      </button>
    {:else}<p>{m.place_no_matches({}, { locale })}</p>{/each}</div>
    {#if selected}<p>{m.map_selected_place({}, { locale })}: {selected.name}</p>
    {:else if latitude && longitude}<p>{m.map_local_point({}, { locale })}: {latitude}, {longitude}</p>{/if}
    {#if !form}
      <div class="primary-actions">
        {#if request.mode !== "orientation"}<button class="primary" type="button" disabled={busy || generation === null || !selected || !eligible(selected)} onclick={confirm}>{m.map_use_place({}, { locale })}</button>{/if}
        <button type="button" disabled={busy} onclick={close}>{m.place_cancel({}, { locale })}</button>
      </div>
    {/if}
    <section class="action-group" aria-label={m.place_manage_heading({}, { locale })}>
    <h3>{m.place_manage_heading({}, { locale })}</h3>
    {#if latitude && longitude && !menu && !form}<button type="button" disabled={busy} onclick={() => startCreate()}>{m.map_create_here({}, { locale })}</button>{/if}
    {#if !form}<button type="button" disabled={busy || generation === null} onclick={() => startCreate(false)}>{m.place_new({}, { locale })}</button>{/if}
    {#if selected && !eligible(selected) && !form}<p>{m.weather_default_coordinates({}, { locale })}</p>{/if}
    {#if selected && !form}
      <button type="button" disabled={busy || generation === null} onclick={startEdit}>{m.place_registry_edit({}, { locale })}</button>
      <button bind:this={deleteButton} type="button" disabled={busy || generation === null} onclick={() => { deleteTarget = selected; error = ""; }}>{m.place_registry_delete({}, { locale })}</button>
    {/if}
    {#if form}
      <form onsubmit={(event) => { event.preventDefault(); void registerPlace(!editing && request.mode !== "orientation"); }}>
        <label>{m.place_name({}, { locale })}<input bind:this={nameInput} bind:value={name} maxlength="512" disabled={busy} /></label>
        <label>{m.place_latitude({}, { locale })}<input bind:value={latitude} inputmode="decimal" disabled={busy} /></label>
        <label>{m.place_longitude({}, { locale })}<input bind:value={longitude} inputmode="decimal" disabled={busy} /></label>
        <p>{m.place_coordinates_optional({}, { locale })}</p>
        <button disabled={busy || generation === null}>{editing ? m.place_registry_save({}, { locale }) : request.mode === "journal" ? m.place_save_use({}, { locale }) : request.mode === "default" ? m.map_save_default({}, { locale }) : m.map_save_place({}, { locale })}</button><button type="button" disabled={busy} onclick={() => protect(resetForm)}>{m.place_cancel({}, { locale })}</button>
      </form>
    {/if}
    </section>
    {#if discardOpen}<section class="decision" aria-label={m.place_discard_title({}, { locale })}>
      <p>{m.place_registry_discard({}, { locale })}</p>
      <button type="button" onclick={() => { discardOpen = false; discardAction = null; }}>{m.place_continue({}, { locale })}</button>
      <button type="button" onclick={discard}>{m.place_discard({}, { locale })}</button>
    </section>{/if}
    {#if deleteTarget}<section class="decision" aria-label={m.place_registry_delete({}, { locale })}>
      <p>{m.place_registry_delete_confirm({ name: deleteTarget.name }, { locale })}</p>
      <button type="button" disabled={busy} onclick={() => deleteTarget = null}>{m.place_cancel({}, { locale })}</button>
      <button type="button" disabled={busy || generation === null} onclick={erasePlace}>{m.place_registry_delete({}, { locale })}</button>
    </section>{/if}
    {#if feedback}<p role="status">{feedback === "place_registry_updated" ? m.place_registry_updated({}, { locale }) : m.place_registry_deleted({}, { locale })}</p>{/if}
    {#if notice}<p role="status">{m.map_place_saved({}, { locale })}</p>{/if}
    {#if error && error !== "inUse"}<p role="alert">{error === "savedUnverified" ? m.place_registry_saved_unverified({}, { locale }) : error === "weatherCoordinates" ? m.weather_default_coordinates({}, { locale }) : error === "defaultCoordinates" ? m.place_registry_default_coordinates({}, { locale }) : error === "conflict" ? m.place_registry_conflict({}, { locale }) : error === "generation" ? m.place_generation_conflict({}, { locale }) : error === "link" ? m.map_place_use_failed({}, { locale }) : error === "invalid" ? m.place_name_required({}, { locale }) : error === "coordinates" ? m.place_coordinates_invalid({}, { locale }) : m.place_save_failed({}, { locale })}</p>{/if}
    {#if error === "conflict" && selected}<button type="button" disabled={busy} onclick={startEdit}>{m.place_registry_reload({}, { locale })}</button>{/if}
    {#if !form && request.mode === "journal" && request.remove}
      <section class="action-group" aria-label={m.place_journal_link_heading({}, { locale })}>
        <h3>{m.place_journal_link_heading({}, { locale })}</h3>
        <button type="button" disabled={busy} onclick={remove}>{m.place_remove({}, { locale })}</button>
      </section>
    {/if}
  </aside>
  <div class="map-content" inert={busy}>
    {#if loaded}<PlaceMap {latitude} {longitude} {places} selectedPlaceId={selected?.id ?? null} onSelectPlace={pick} focusCenter={selected && selected.latitude !== null && selected.longitude !== null ? (selected === initialSelection && request.center ? request.center : { latitude: selected.latitude, longitude: selected.longitude }) : null} journalCenter={request.center} defaultCenter={center} {locale} onCoordinates={mark} onCreateHere={contextPoint} />
    {:else}<p role="status">{m.place_loading({}, { locale })}</p>{/if}
  </div>
  </div>
  {#if menu}<div class="context-menu" bind:this={menuElement} role="menu" aria-label={m.map_create_here({}, { locale })} style:left={`${menuX}px`} style:top={`${menuY}px`}>
    <button type="button" role="menuitem" onclick={() => startCreate()}>{m.map_create_here({}, { locale })}</button>
  </div>{/if}
  {#if blockedOpen}
    <div class="information-overlay">
      <div bind:this={blockedDialog} class="information-dialog" role="dialog" aria-modal="true" aria-labelledby="place-blocked-title" aria-describedby="place-blocked-description" tabindex="-1" onkeydown={blockedKeys}>
        <h2 id="place-blocked-title">{m.place_registry_blocked_title({}, { locale })}</h2>
        <p id="place-blocked-description">{m.place_registry_blocked_description({}, { locale })}</p>
        {#if usageLoading}<p role="status">{m.place_usage_loading({}, { locale })}</p>
        {:else if usageError}<p role="alert">{m.place_usage_failed({}, { locale })}</p><button type="button" onclick={loadUsage}>{m.place_usage_retry({}, { locale })}</button>
        {:else if usage}
          <p class="usage-caption">{m.place_usage_as_read({}, { locale })}</p>
          {#if usage.isWeatherDefault}<p>{m.place_usage_default({}, { locale })}</p>{/if}
          {#if usage.totalCount > 0}
            <p>{m.place_usage_count({ count: usage.totalCount }, { locale })}</p>
            <ul class="usage-list">{#each usage.entries as entry (entry.id)}<li>
              <a href={`/journal/entries/${encodeURIComponent(entry.id)}`} onclick={(event) => { event.preventDefault(); void openUsageEntry(entry.id); }}>
                <span>{formatJournalDate(entry.entryDate, locale)}</span><span>{entry.title.trim() || m.search_untitled({}, { locale })}</span>
              </a>
            </li>{/each}</ul>
            <p>{m.place_usage_journal_hint({}, { locale })}</p>
          {/if}
          {#if !usage.isWeatherDefault && usage.totalCount === 0}<p>{m.place_usage_changed({}, { locale })}</p>{/if}
        {/if}
        <button bind:this={blockedButton} type="button" onclick={dismissBlocked}>{m.place_registry_close({}, { locale })}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .map-panel { position: fixed; inset: 40px 0 auto; z-index: 150; box-sizing: border-box; height: calc(100dvh - 40px); display: flex; flex-direction: column; padding: 12px 20px; background: #f8f7f3; border-bottom: 1px solid #d9d8d1; box-shadow: 0 8px 24px rgb(52 53 48 / 8%); animation: unfold 180ms ease-out; }
  header { display: flex; align-items: center; justify-content: space-between; flex: 0 0 auto; }
  h2 { margin: 0; color: #435b47; font: 16px Georgia,serif; }
  button { background: transparent; border: 0; color: #435b47; font-size: 24px; padding: 4px 10px; cursor: pointer; }
  button:focus-visible { outline: 2px solid #788e7b; outline-offset: 2px; }
  p { color: var(--text-muted, #686d63); font: 12px system-ui,sans-serif; }
  .workspace { display: flex; flex: 1; min-height: 0; gap: 16px; }
  aside { width: 260px; flex: 0 0 auto; overflow: auto; min-height: 0; font: 13px system-ui,sans-serif; }
  .search { position: sticky; top: 0; z-index: 1; background: #f8f7f3; }
  aside button { font: inherit; border: 1px solid #d9d8d1; border-radius: 4px; padding: 8px; margin: 4px 0; }
  .action-group { border-top: 1px solid var(--border,#d9d8d1); margin-top: 12px; padding-top: 8px; }
  .action-group h3 { font: 12px system-ui,sans-serif; color: var(--text-muted, #686d63); margin: 0 0 6px; }
  .primary-actions { display: flex; flex-wrap: wrap; gap: 8px; margin: 8px 0; }
  aside .primary-actions .primary { background: var(--accent,#435b47); color: var(--surface,#faf9f6); border-color: var(--accent,#435b47); }
  aside button { max-width: 100%; overflow-wrap: anywhere; }
  .information-overlay { position: fixed; inset: 0; z-index: 170; display: grid; place-items: center; padding: 20px; background: rgb(39 43 38 / 18%); }
  .information-dialog { box-sizing: border-box; width: min(440px,100%); max-height: calc(100dvh - 40px); overflow: auto; padding: 24px; border: 1px solid var(--border,#d9d8d1); border-radius: 12px; background: var(--surface,#f8f7f3); box-shadow: 0 12px 36px rgb(35 39 34 / 12%); }
  .information-dialog p { font-size: 14px; line-height: 1.6; }
  .usage-list { max-height: min(35dvh,280px); overflow-y: auto; margin: 12px 0; padding: 0; list-style: none; }
  .usage-list a { display: grid; gap: 3px; padding: 8px 4px; color: var(--accent,#435b47); overflow-wrap: anywhere; }
  .usage-list a:focus-visible { outline: 2px solid var(--accent,#435b47); outline-offset: -2px; }
  .usage-list a span:first-child { font-size: 12px; color: var(--text-muted, #686d63); }
  .information-dialog button { display: block; margin-left: auto; font: 13px system-ui,sans-serif; padding: 8px 14px; border: 1px solid var(--border,#d9d8d1); border-radius: 6px; }
  .decision { border: 1px solid #d9d8d1; border-radius: 6px; padding: 8px; margin: 8px 0; }
  aside button:disabled { opacity: .45; }
  label, small { display: block; margin: 4px 0; }
  input { appearance: none; box-sizing: border-box; display: block; width: 100%; min-width: 0; min-height: 40px; padding: 8px; background: #fffefa; border: 1px solid #d9d8d1; border-radius: 6px; box-shadow: none; color: inherit; font: inherit; }
  input:focus, input:focus-visible { outline: 2px solid #788e7b; outline-offset: -2px; }
  form, label { min-width: 0; }
  .results { height: min(30vh,240px); min-height: 0; overflow-y: auto; overscroll-behavior: contain; flex-shrink: 0; }
  .results button { display: block; width: 100%; text-align: left; }
  .results button[aria-pressed="true"] { background: #eef1eb; }
  .results button.keyboard-active { outline: 2px solid #788e7b; outline-offset: -2px; }
  .context-menu { position: fixed; z-index: 160; background: #f8f7f3; border: 1px solid #d9d8d1; border-radius: 6px; box-shadow: 0 4px 15px rgb(52 53 48 / 12%); }
  .context-menu button { font: 13px system-ui,sans-serif; padding: 10px 14px; }
  @media(max-width:650px) { .workspace { flex-direction: column-reverse; } aside { width: auto; max-height: 42vh; } .map-content { min-height: 160px; } }
  .map-content { isolation: isolate; display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .map-content :global(.map-shell) { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .map-content :global(.map) { flex: 1; height: 100%; min-height: 0; }
  @keyframes unfold { from { clip-path: inset(0 0 100% 0); } to { clip-path: inset(0); } }
  @media (prefers-reduced-motion: reduce) { .map-panel { animation: none; } }
  @media (max-width: 600px) { .map-panel { padding: 8px; } }
</style>
