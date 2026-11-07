<script lang="ts">
  import PlaceMapPanel from "$lib/components/PlaceMapPanel.svelte";
  import { getContext, onMount } from "svelte";
  import { PLACE_MAP, type PlaceMapController } from "$lib/place-map.svelte";
  const placeMap = getContext<PlaceMapController>(PLACE_MAP);
  import AboutMno from "$lib/components/AboutMno.svelte";
  import WeatherDefaultPlace from "$lib/components/WeatherDefaultPlace.svelte";
  import { createJournalReader } from "$lib/journal-data";
  import CalendarGrid from "$lib/components/CalendarGrid.svelte";
  import CalendarDayPanel from "$lib/components/CalendarDayPanel.svelte";
  import * as m from "$lib/paraglide/messages.js";
  import { language, changeLanguage } from "$lib/language.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";

  const readJournal = createJournalReader();
  type Panel = "left" | "top" | "right";

  let restoreAvailable = $state(false);
  let importAvailable = $state(false);
  onMount(() => {
    void invoke<boolean>("open_import_available").then((available) => { importAvailable = available; }).catch(() => {});
    void invoke<boolean>("restore_available").then((available) => { restoreAvailable = available; }).catch(() => {});
  });

  let isCreatingBackup = $state(false);
  let backupStatus = $state("idle");

  function suggestedBackupFilename(date = new Date()): string {
    const pad = (value: number) => String(value).padStart(2, "0");
    return `MNO-backup-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}_${pad(date.getHours())}${pad(date.getMinutes())}.mno-backup`;
  }

  async function createManualBackup() {
    if (isCreatingBackup || isExporting) return;
    clearCompletedStatuses();
    isCreatingBackup = true;
    backupStatus = "creating";
    try {
      const outcome = await invoke<"created" | "cancelled">("create_manual_backup", {
        suggestedName: suggestedBackupFilename(),
        dialogTitle: m.backup_create({}, { locale: language.current }),
      });
      backupStatus = outcome === "created" ? "created" : "idle";
    } catch (error) {
      backupStatus = ["already_exists", "invalid_destination", "busy"].includes(String(error))
        ? String(error)
        : "failed";
    } finally {
      isCreatingBackup = false;
    }
  }

  function backupStatusMessage(): string {
    const options = { locale: language.current };
    switch (backupStatus) {
      case "creating": return m.backup_creating({}, options);
      case "created": return m.backup_created({}, options);
      case "already_exists": return m.backup_already_exists({}, options);
      case "invalid_destination": return m.backup_invalid_destination({}, options);
      case "busy": return m.backup_busy({}, options);
      case "failed": return m.backup_failed({}, options);
      default: return "";
    }
  }


  let isExporting = $state(false);
  let exportStatus = $state("idle");

  function suggestedExportFilename(date = new Date()): string {
    const pad = (value: number) => String(value).padStart(2, "0");
    return `MNO-journal-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}_${pad(date.getHours())}${pad(date.getMinutes())}.mno-export`;
  }

  async function createOpenExport() {
    if (isExporting || isCreatingBackup) return;
    clearCompletedStatuses();
    isExporting = true;
    exportStatus = "creating";
    try {
      const outcome = await invoke<"created" | "cancelled">("create_open_export", {
        suggestedName: suggestedExportFilename(),
        dialogTitle: m.export_create({}, { locale: language.current }),
      });
      exportStatus = outcome === "created" ? "created" : "idle";
    } catch (error) {
      exportStatus = ["already_exists", "invalid_destination", "invalid_content", "busy"].includes(String(error))
        ? String(error) : "failed";
    } finally {
      isExporting = false;
    }
  }

  function exportStatusMessage(): string {
    const options = { locale: language.current };
    switch (exportStatus) {
      case "creating": return m.export_creating({}, options);
      case "created": return m.export_created({}, options);
      case "already_exists": return m.export_already_exists({}, options);
      case "invalid_destination": return m.export_invalid_destination({}, options);
      case "invalid_content": return m.export_invalid_content({}, options);
      case "busy": return m.export_busy({}, options);
      case "failed": return m.export_failed({}, options);
      default: return "";
    }
  }

  function clearCompletedStatuses() {
    if (backupStatus === "created") backupStatus = "idle";
    if (exportStatus === "created") exportStatus = "idle";
  }

  let activePanel = $state<Panel | null>(null);
  let mapTrigger = $state<HTMLButtonElement>();
  function closeMap() { placeMap.close(); if (!placeMap.request && activePanel === "top") activePanel = null; }

  let calendarMonth = $state(new Date());
  let selectedCalendarDate = $state<string | null>(null);
  let isCalendarMonthPickerOpen = $state(false);
  let markedCalendarDates = $state<string[]>([]);
  let calendarRequestId = 0;

  function togglePanel(panel: Panel) {
    if (placeMap.busy) return;
    if (panel === "top") {
      if (placeMap.request) { closeMap(); return; }
      activePanel = "top";
      placeMap.open({ mode: "orientation" });
      return;
    }
    placeMap.close();
    if (placeMap.request) return;
    activePanel = activePanel === panel ? null : panel;
  }

  function closeCalendarDayPanel() {
    selectedCalendarDate = null;
  }

  let searchText = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchNavigating = false;
  let searchDisposed = false;
  function scheduleSearch() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { void navigateSearch(); }, 300);
  }
  async function navigateSearch() {
    if (searchDisposed || searchNavigating) return;
    const query = searchText;
    const destination = `/search?q=${encodeURIComponent(query)}`;
    searchNavigating = true;
    try {
      await goto(destination, { keepFocus: true, noScroll: true, replaceState: page.url.pathname === "/search" });
      // A cancelled navigation must not repeatedly reopen the editor's guard.
      if (!searchDisposed && page.url.pathname === "/search" && page.url.searchParams.get("q") === query && searchText !== query) scheduleSearch();
    } catch { /* The existing navigation guard retains the current editor. */ }
    finally { searchNavigating = false; }
  }
  $effect(() => () => { searchDisposed = true; clearTimeout(searchTimer); });

  afterNavigate((navigation) => {
    clearCompletedStatuses();
    if (navigation?.to?.url.pathname !== "/search") {
      placeMap?.close();
      activePanel = null;
      clearTimeout(searchTimer);
    } else if (!searchNavigating) {
      searchText = navigation.to.url.searchParams.get("q") ?? "";
      const previous = navigation.from?.url;
      if (previous?.pathname.startsWith("/journal/entries/") &&
          previous.searchParams.get("from") === "search" &&
          (previous.searchParams.get("q") ?? "") === searchText) {
        activePanel = "left";
      }
    }
    selectedCalendarDate = null;
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    if (placeMap.request) { closeMap(); return; }

    if (activePanel === "left" && selectedCalendarDate !== null) {
      closeCalendarDayPanel();
    } else if (activePanel !== null) {
      if (activePanel === "top") closeMap();
      else activePanel = null;
    }
  }

  async function loadCalendarDates(year: number, month: number) {
    const requestId = ++calendarRequestId;

    markedCalendarDates = [];

    try {
      const dates = await readJournal<string[]>("list_journal_dates_for_month", {
        year,
        month,
      });

      if (requestId === calendarRequestId) {
        markedCalendarDates = dates.data;
      }
    } catch (error) {
      if (requestId === calendarRequestId) {
        console.error("Failed to load calendar dates:", error);
      }
    }
  }

  const calendarYear = $derived(calendarMonth.getFullYear());
  const calendarMonthNumber = $derived(calendarMonth.getMonth() + 1);

  $effect(() => {
    void loadCalendarDates(calendarYear, calendarMonthNumber);
  });

  function handleJournalChanged() {
    if (activePanel === "left") void refreshHistoryVisibility();
    void loadCalendarDates(calendarYear, calendarMonthNumber);
  }

  $effect(() => {
    window.addEventListener("mno:journal-changed", handleJournalChanged);

    return () => {
      window.removeEventListener("mno:journal-changed", handleJournalChanged);
    };
  });
  const readHistoryVisibility = createJournalReader();
  let historyVisible = $state(false);
  let historyVisibilityRequest = 0;
  let historyDisposed = false;

  async function refreshHistoryVisibility() {
    const request = ++historyVisibilityRequest;
    historyVisible = false;
    try {
      const result = await readHistoryVisibility<{ navigationVisible: boolean }>("journal_history_overview");
      if (!historyDisposed && request === historyVisibilityRequest && activePanel === "left") {
        historyVisible = result.data.navigationVisible;
      }
    } catch { /* Visibility is optional; a failed read must not reveal a stale link. */ }
  }

  $effect(() => {
    if (activePanel === "left") void refreshHistoryVisibility();
    return () => { historyVisibilityRequest++; };
  });
  $effect(() => () => { historyDisposed = true; historyVisibilityRequest++; });
</script>

<svelte:window onkeydown={handleKeydown} />
<nav
  class="app-navigation"
  aria-label={m.nav_main_navigation({}, { locale: language.current })}
>
  <button
    type="button"
    aria-label={m.nav_open_navigation({}, { locale: language.current })}
    aria-expanded={activePanel === "left"}
    onclick={() => togglePanel("left")}
  >
    ☰
  </button>

  <div class="app-title-navigation">
    <a href="/" class="app-home-link" onclick={() => (activePanel = null)}>
      <span class="app-name">MY NATURE OBSERVATIONS</span>
    </a>

    <button
      type="button"
      class="app-overview-button"
      bind:this={mapTrigger}
      aria-label={m.map_panel_title({}, { locale: language.current })}
      aria-controls="navigation-map"
      aria-expanded={placeMap.request !== null}
      onclick={() => togglePanel("top")}
    >
      <span aria-hidden="true">⌄</span>
    </button>
  </div>

  <button
    type="button"
    aria-label={m.nav_open_tools({}, { locale: language.current })}
    aria-expanded={activePanel === "right"}
    onclick={() => togglePanel("right")}
  >
    ⚙
  </button>
</nav>

{#if placeMap.request}
  {#key placeMap.request}<PlaceMapPanel request={placeMap.request} controller={placeMap} locale={language.current} onclose={closeMap} />{/key}
{/if}

{#if activePanel === "left"}
  <div
    class="panel-backdrop"
    class:search-docked={page.url.pathname === "/search"}
    onclick={() => (activePanel = null)}
    role="presentation"
  ></div>

  <aside
    class="left-panel"
    class:search-docked={page.url.pathname === "/search"}
    aria-label={m.nav_open_navigation({}, { locale: language.current })}
  >
    <div class="panel-header">
      <button
        type="button"
        class="panel-close"
        aria-label={m.nav_close_panel({}, { locale: language.current })}
        onclick={() => (activePanel = null)}
      >
        ×
      </button>
    </div>

    <div class="panel-section">
      <div class="navigation-search">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-4-4" />
        </svg>

        <input
          type="search"
          placeholder={m.nav_search_placeholder(
            {},
            { locale: language.current },
          )}
          aria-label={m.nav_search_placeholder(
            {},
            { locale: language.current },
          )}
          bind:value={searchText}
          oninput={scheduleSearch}
          onkeydown={(event) => { if (event.key === "Enter") { clearTimeout(searchTimer); void navigateSearch(); } }}
          maxlength="512"
        />
      </div>
    </div>
    <div class="panel-section calendar-section">
      {#if isCalendarMonthPickerOpen}
        <button
          type="button"
          class="calendar-back-heading"
          onclick={() => (isCalendarMonthPickerOpen = false)}
          aria-label={language.current === "sv"
            ? "Tillbaka till kalendern"
            : "Back to calendar"}
        >
          <span aria-hidden="true">‹</span>
          <span>{m.nav_calendar({}, { locale: language.current })}</span>
        </button>
      {:else}
        <h2 class="panel-section-heading">
          {m.nav_calendar({}, { locale: language.current })}
        </h2>
      {/if}

      <CalendarGrid
        bind:displayedMonth={calendarMonth}
        bind:isMonthPickerOpen={isCalendarMonthPickerOpen}
        markedDates={markedCalendarDates}
        selectedDate={selectedCalendarDate}
        onSelectDate={(date) => {
          selectedCalendarDate = date;
        }}
        onGoToToday={() => {
          selectedCalendarDate = null;
        }}
      />
    </div>
    {#if historyVisible}
      <a class="panel-section-heading history-link" href="/history">{m.history_title({}, { locale: language.current })}</a>
    {/if}
  </aside>

  {#if selectedCalendarDate !== null}
    <CalendarDayPanel
      date={selectedCalendarDate}
      onClose={closeCalendarDayPanel}
    />
  {/if}
{/if}

{#if activePanel === "right"}
  <div
    class="panel-backdrop"
    onclick={() => (activePanel = null)}
    role="presentation"
  ></div>

  <aside
    class="right-panel"
    aria-label={m.nav_tools_title({}, { locale: language.current })}
  >
    <div class="panel-header">
      <button
        type="button"
        class="panel-close"
        aria-label={m.nav_close_panel({}, { locale: language.current })}
        onclick={() => (activePanel = null)}
      >
        ×
      </button>
    </div>

    <section class="settings-group" aria-labelledby="settings-heading">
      <h2 id="settings-heading" class="settings-group-heading">
        {m.nav_settings_group({}, { locale: language.current })}
      </h2>
      <div class="panel-section">
        <details class="language-details">
          <summary class="panel-section-heading">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <circle cx="12" cy="12" r="9" />
              <path d="M3 12h18" />
              <path d="M12 3a15 15 0 0 1 0 18" />
              <path d="M12 3a15 15 0 0 0 0 18" />
            </svg>

            <span>{m.nav_language({}, { locale: language.current })}</span>
            <span class="language-chevron" aria-hidden="true">⌄</span>
          </summary>

          <div class="language-choices">
            <button
              type="button"
              class:language-selected={language.current === "sv"}
              aria-pressed={language.current === "sv"}
              onclick={() => changeLanguage("sv")}
            >
              Svenska
              {#if language.current === "sv"}
                <span aria-hidden="true">✓</span>
              {/if}
            </button>

            <button
              type="button"
              class:language-selected={language.current === "en"}
              aria-pressed={language.current === "en"}
              onclick={() => changeLanguage("en")}
            >
              English
              {#if language.current === "en"}
                <span aria-hidden="true">✓</span>
              {/if}
            </button>
          </div>
          <WeatherDefaultPlace />
        </details>
      </div>
    </section>

    <section class="settings-group" aria-labelledby="data-security-heading">
      <details class="data-security-details">
        <summary id="data-security-heading" class="panel-section-heading">
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6z" /><path d="m9 12 2 2 4-4" /></svg>
          <span>{m.nav_data_security_group({}, { locale: language.current })}</span>
          <span class="language-chevron" aria-hidden="true">⌄</span>
        </summary>
        <section class="panel-section backup-section" aria-labelledby="backup-heading">
          <h3 id="backup-heading" class="panel-section-heading">
            {m.backup_heading({}, { locale: language.current })}
          </h3>
          <p class="data-description">{m.settings_backup_description({}, { locale: language.current })}</p>
          <div class="data-actions">
            <button class="backup-button" type="button" onclick={createManualBackup} disabled={isCreatingBackup || isExporting}>
              {m.backup_create({}, { locale: language.current })}
            </button>
            {#if restoreAvailable}
              <a class="backup-button" href="/settings/restore" onclick={clearCompletedStatuses}>{m.settings_restore({}, { locale: language.current })}</a>
            {/if}
          </div>
          {#if backupStatus !== "idle"}
            <p class="backup-status" class:error={!["creating", "created"].includes(backupStatus)} role={["creating", "created"].includes(backupStatus) ? "status" : "alert"}>{backupStatusMessage()}</p>
          {/if}
        </section>
        <section class="panel-section backup-section transfer-section" aria-labelledby="transfer-heading">
          <h3 id="transfer-heading" class="panel-section-heading">
            {m.settings_transfer_heading({}, { locale: language.current })}
          </h3>
          <p class="data-description">{importAvailable ? m.settings_transfer_description({}, { locale: language.current }) : m.settings_export_only_description({}, { locale: language.current })}</p>
          <div class="data-actions">
            <button class="backup-button" type="button" onclick={createOpenExport} disabled={isExporting || isCreatingBackup} aria-describedby="open-export-description">
              {m.settings_export({}, { locale: language.current })}
            </button>
            {#if importAvailable}
              <a class="backup-button" href="/settings/import" onclick={clearCompletedStatuses}>{m.settings_import({}, { locale: language.current })}</a>
            {/if}
          </div>
          {#if exportStatus !== "idle"}
            <p class="backup-status" class:error={!["creating", "created"].includes(exportStatus)} role={["creating", "created"].includes(exportStatus) ? "status" : "alert"}>{exportStatusMessage()}</p>
          {/if}
          <p id="open-export-description" class="data-description data-note">{m.export_description({}, { locale: language.current })}</p>
        </section>
      </details>
    </section>
    <section class="settings-group" aria-labelledby="about-heading">
      <details class="about-details">
        <summary id="about-heading" class="panel-section-heading">
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 11v6m0-10v1" /></svg>
          <span>{m.about_heading({}, { locale: language.current })}</span>
          <span class="language-chevron" aria-hidden="true">⌄</span>
        </summary>
        <AboutMno />
      </details>
    </section>
  </aside>
{/if}

<style>
  .history-link {
    margin-top: 24px;
    text-transform: uppercase;
    text-decoration: none;
  }
  .history-link:hover { text-decoration: underline; }

  .data-actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: flex-start; }
  .data-description { font-size: 13px; line-height: 1.6; margin: 0 0 12px; }
  .data-note { font-size: 12px; margin: 12px 0 0; }
  .panel-section.transfer-section {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid rgb(217 216 209 / 60%);
  }
  .backup-status {
    font-size: 13px;
    line-height: 1.5;
    margin: 12px 0;
  }

  .backup-status.error {
    color: #a33b35;
  }

  .backup-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    min-width: 0;
    max-width: 100%;
    min-height: 36px;
    border: 1px solid #d9d8d1;
    border-radius: 6px;
    background: transparent;
    color: var(--accent, #435b47);
    font-family: inherit;
    font-size: 13px;
    font-weight: 400;
    line-height: 1.4;
    padding: 8px 12px;
    text-align: left;
    text-decoration: none;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .backup-button:hover { background: #eeeee8; }
  .backup-button:focus-visible { outline: 2px solid #788e7b; outline-offset: 2px; }

  .backup-button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .app-navigation {
    position: sticky;
    top: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 40px;
    padding: 0 24px;
    background: #f1f0eb;
    border-bottom: 1px solid #d9d8d1;
  }

  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-width: 36px;
    min-height: 36px;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #788e7b;
    cursor: pointer;
    font-size: 17px;
  }

  button:hover {
    background: #eeeee8;
  }

  button:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 2px;
  }

  .app-name {
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 2.5px;
    color: #788e7b;
  }

  @media (max-width: 600px) {
    .app-navigation {
      padding: 0 12px;
    }

    .app-name {
      font-size: 13px;
    }
  }

  .panel-backdrop {
    position: fixed;
    inset: 40px 0 0;
    z-index: 101;
    background: transparent;
  }

  .left-panel {
    box-sizing: border-box;
    overflow-y: auto;
    position: fixed;
    top: 40px;
    left: 0;
    bottom: 0;
    z-index: 102;
    width: min(360px, 100vw);
    padding: 28px;
    background: #f8f7f3;
    border-right: 1px solid #d9d8d1;
    box-shadow: 8px 0 24px rgb(52 53 48 / 5%);
  }

  .left-panel.search-docked { box-sizing: border-box; }

  @media (min-width: 1000px) {
    .panel-backdrop.search-docked { display: none; }
  }

  .right-panel {
    box-sizing: border-box;
    overflow-y: auto;
    overscroll-behavior: contain;
    position: fixed;
    top: 40px;
    right: 0;
    bottom: 0;
    z-index: 102;
    width: min(360px, 100vw);
    padding: 28px;
    background: #f8f7f3;
    border-left: 1px solid #d9d8d1;
    box-shadow: -8px 0 24px rgb(52 53 48 / 5%);
  }

  .settings-group {
    margin-top: 28px;
  }

  .settings-group-heading {
    margin: 0 0 8px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 1.4px;
    line-height: 1.5;
    text-transform: uppercase;
    color: var(--accent, #435b47);
  }

  .backup-section .panel-section-heading {
    margin-top: 0;
    cursor: default;
  }

  .panel-header {
    display: flex;
    justify-content: flex-end;
    align-items: center;
  }

  .panel-close {
    font-size: 16px;
    font-weight: 300;
    opacity: 0.65;
  }

  .panel-close:hover {
    opacity: 1;
  }

  .panel-section {
    margin-top: 8px;
    padding-top: 0;
    border-top: 0;
  }

  .panel-section-heading {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 0;
    padding: 8px 0;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    font-weight: 500;
    color: var(--accent, #435b47);
    cursor: pointer;
    list-style: none;
  }

  .panel-section-heading::-webkit-details-marker {
    display: none;
  }

  .panel-section-heading:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 3px;
    border-radius: 3px;
  }

  .panel-section-heading svg { flex-shrink: 0; }

  .language-chevron {
    margin-left: auto;
    font-size: 16px;
    transition: transform 150ms ease;
  }

  .language-details[open] .language-chevron,
  .data-security-details[open] .language-chevron,
  .about-details[open] > summary .language-chevron {
    transform: rotate(180deg);
  }

  .language-choices {
    display: flex;
    flex-direction: column;
    gap: 0px;
    margin: 12px 0 0 30px;
  }

  .language-choices button {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 7px 8px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    color: var(--text-muted, #686d63);
  }

  .language-choices button.language-selected {
    color: var(--accent, #435b47);
    font-weight: 500;
  }

  @media (prefers-reduced-motion: reduce) {
    .language-chevron {
      transition: none;
    }
  }

  .navigation-search {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
    border: 1px solid #d9d8d1;
    border-radius: 6px;
    color: #788e7b;
  }

  .navigation-search svg {
    flex-shrink: 0;
  }

  .navigation-search input {
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: #343530;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
  }

  .navigation-search input::placeholder {
    color: var(--text-muted, #686d63);
    opacity: 1;
  }

  .navigation-search input:disabled {
    cursor: not-allowed;
  }

  .calendar-section {
    margin-top: 28px;
  }

  .calendar-back-heading {
    display: inline-flex;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    min-height: 29px;
    padding: 0;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--accent, #435b47);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .calendar-back-heading:hover {
    background: transparent;
    color: #435b47;
  }

  .calendar-back-heading:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 3px;
  }

  .app-title-navigation {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .app-home-link {
    display: inline-flex;
    align-items: center;
    color: inherit;
    text-decoration: none;
    cursor: pointer;
  }

  .app-home-link:hover {
    color: var(--accent, #435b47);
  }

  .app-overview-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
  }
</style>
