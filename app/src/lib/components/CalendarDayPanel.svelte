<script lang="ts">
  import { createJournalReader } from "$lib/journal-data";
  import { visibleJournalEntries, type ReadingEntry } from "$lib/journal-reading";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  const readJournal = createJournalReader();

  let {
    date,
    onClose,
  }: {
    date: string;
    onClose: () => void;
  } = $props();

  let entries = $state<ReadingEntry[]>([]);
  let isLoading = $state(true);
  let loadError = $state(false);

  let requestId = 0;
  let disposed = false;

  const formattedDate = $derived(
    new Intl.DateTimeFormat(language.current === "sv" ? "sv-SE" : "en-GB", {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: "numeric",
      timeZone: "UTC",
    }).format(new Date(`${date}T12:00:00Z`)),
  );

  async function loadEntries(selectedDate: string) {
    const currentRequest = ++requestId;

    isLoading = true;
    loadError = false;

    try {
      const result = await readJournal<ReadingEntry[]>("read_journal_entries");

      if (disposed || currentRequest !== requestId || selectedDate !== date) return;

      // Match the reading view's visibility; keep the panel's newest-created-first order.
      entries = visibleJournalEntries(result.data)
        .filter(entry => entry.entryDate === selectedDate).reverse();
    } catch (error) {
      if (disposed || currentRequest !== requestId || selectedDate !== date) return;

      console.error("Failed to load journal entries:", error);
      loadError = true;
      entries = [];
    } finally {
      if (!disposed && currentRequest === requestId && selectedDate === date) {
        isLoading = false;
      }
    }
  }

  $effect(() => {
    void loadEntries(date);
  });

  function handleJournalChanged() {
    void loadEntries(date);
  }

  $effect(() => {
    window.addEventListener("mno:journal-changed", handleJournalChanged);

    return () => {
      disposed = true;
      requestId++;
      window.removeEventListener("mno:journal-changed", handleJournalChanged);
    };
  });
</script>

<aside class="calendar-day-panel" aria-label={formattedDate}>
  <div class="day-panel-header">
    <h2>{formattedDate}</h2>

    <button
      type="button"
      class="day-panel-close"
      aria-label={m.calendar_day_close({}, { locale: language.current })}
      onclick={onClose}
    >
      ×
    </button>
  </div>

  <div class="day-panel-content">
    <h3>{m.calendar_day_entries({}, { locale: language.current })}</h3>

    {#if isLoading}
      <p>{m.calendar_day_loading({}, { locale: language.current })}</p>
    {:else if loadError}
      <p role="alert">
        {m.calendar_day_error({}, { locale: language.current })}
      </p>
    {:else if entries.length === 0}
      <p>{m.calendar_day_empty({}, { locale: language.current })}</p>
    {:else}
      <a href={`/journal?date=${encodeURIComponent(date)}`} class="day-reading-link">
        {language.current === "sv" ? "Visa dagen i journalen" : "View day in journal"}
      </a>
      <ul class="entry-list">
        {#each entries as entry (entry.id)}
          <li class="entry-item">
            <a href={`/journal?entry=${encodeURIComponent(entry.id)}`} class="entry-title">
              {entry.title}
            </a>
          </li>
        {/each}
      </ul>
    {/if}

    <a href={`/journal/new?date=${date}`} class="new-entry-link">
      <span aria-hidden="true">＋</span>
      {m.home_card_new_title({}, { locale: language.current })}
    </a>
  </div>
</aside>

<style>
  .calendar-day-panel {
    position: fixed;
    top: 40px;
    bottom: 0;
    left: min(360px, 100vw);
    z-index: 102;
    width: min(360px, calc(100vw - 360px));
    box-sizing: border-box;
    padding: 28px;
    overflow-y: auto;
    background: #f8f7f3;
    border-right: 1px solid #d9d8d1;
    box-shadow: 8px 0 24px rgb(52 53 48 / 5%);
  }

  .day-panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  h2 {
    margin: 4px 0 0;
    color: #343530;
    font-family: Georgia, serif;
    font-size: 20px;
    font-weight: 400;
    line-height: 1.4;
    text-transform: capitalize;
  }

  .day-panel-close {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #788e7b;
    cursor: pointer;
    font-size: 20px;
  }

  .day-panel-close:hover {
    background: #eeeee8;
  }

  .day-panel-content {
    margin-top: 24px;
  }

  h3 {
    margin: 0 0 16px;
    color: var(--accent, #435b47);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    font-weight: 500;
  }

  p {
    color: var(--text-muted, #686d63);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    line-height: 1.7;
  }

  @media (max-width: 720px) {
    .calendar-day-panel {
      left: 0;
      width: 100vw;
    }
  }

  .entry-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry-item {
    padding: 12px 0;
    border-bottom: 1px solid #d9d8d1;
  }

  .entry-title {
    color: #343530;
    font-family: Georgia, serif;
    font-size: 16px;
    line-height: 1.5;
    text-decoration: none;
  }

  .entry-title:hover {
    color: var(--accent, #435b47);
    text-decoration: underline;
  }

  .day-reading-link {
    display: inline-block;
    margin-bottom: 12px;
    color: var(--accent, #435b47);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    text-decoration: none;
  }

  .day-reading-link:hover { text-decoration: underline; }

  .new-entry-link {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-top: 28px;
    color: var(--accent, #435b47);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    font-weight: 500;
    text-decoration: none;
  }

  .new-entry-link:hover {
    text-decoration: underline;
  }
</style>
