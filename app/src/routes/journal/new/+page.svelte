<script lang="ts">
  import type { Snippet } from "svelte";
  import JournalEditor from "$lib/components/JournalEditor.svelte";
  import DatePicker from "$lib/components/DatePicker.svelte";
  import EntryPlace from "$lib/components/EntryPlace.svelte";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";
  import { page } from "$app/state";

  // Use the calendar's selected date when provided.
  // Otherwise, default to the computer's local date.
  function getInitialEntryDate(): string {
    const requestedDate = page.url.searchParams.get("date");

    if (requestedDate && /^\d{4}-\d{2}-\d{2}$/.test(requestedDate)) {
      const parsedDate = new Date(`${requestedDate}T12:00:00Z`);

      if (
        !Number.isNaN(parsedDate.getTime()) &&
        parsedDate.toISOString().slice(0, 10) === requestedDate
      ) {
        return requestedDate;
      }
    }

    const now = new Date();

    return [
      now.getFullYear(),
      String(now.getMonth() + 1).padStart(2, "0"),
      String(now.getDate()).padStart(2, "0"),
    ].join("-");
  }

  let entryDate = $state(getInitialEntryDate());
  let entryId = $state<string | null>(null);
  let generation = $state<number | null>(null);
  let placeDirty = $state(false);
  let placeBusy = $state(false);
</script>

<svelte:head>
  <title
    >{m.journal_new_entry({}, { locale: language.current })} — My Nature Observations</title
  >
</svelte:head>

<main class="journal">
  <div class="journal-content">
    <header class="journal-header">
      <a href="/journal/entries" class="back-link">
        ← {m.journal_all_entries({}, { locale: language.current })}
      </a>

      <span class="section-label">
        {m.journal_section({}, { locale: language.current })}
      </span>
    </header>

    <section class="journal-entry">
      {#snippet metadata(weather: Snippet)}
      <div class="entry-metadata">
        <DatePicker bind:value={entryDate} />
        <EntryPlace {entryId} {generation} editing={true} locale={language.current} onDraft={(dirty, busy) => { placeDirty = dirty; placeBusy = busy; }} />
        {@render weather()}
      </div>
      {/snippet}
      <JournalEditor {metadata} {entryDate} isNew={true} {placeDirty} {placeBusy} onSessionChange={(session) => { entryId = session.entryId; generation = session.generation; }} />
    </section>

    <footer class="journal-footer">
      <span>My Nature Observations</span>
      <span>{m.journal_title({}, { locale: language.current })}</span>
    </footer>
  </div>
</main>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    background: #f8f7f3;
    color: #343530;
    font-family: Georgia, "Times New Roman", serif;
  }

  :global(*) {
    box-sizing: border-box;
  }

  .journal {
    min-height: calc(100vh - 40px);
    padding: 24px 32px 40px;
  }

  .journal-content {
    max-width: 760px;
    margin: 0 auto;
  }

  .journal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 32px;
  }

  .back-link {
    color: var(--accent, #435b47);
    font-family: system-ui, sans-serif;
    font-size: 13px;
    text-decoration: none;
  }

  .back-link:hover {
    text-decoration: underline;
  }

  .section-label {
    font-family: system-ui, sans-serif;
    font-size: 11px;
    letter-spacing: 2px;
    color: var(--accent, #435b47);
  }

  .journal-footer {
    display: flex;
    justify-content: space-between;
    margin-top: 120px;
    padding-top: 20px;
    border-top: 1px solid #d9d8d1;
    font-family: system-ui, sans-serif;
    font-size: 11px;
    color: var(--text-muted, #686d63);
  }

  .entry-metadata {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    margin-bottom: 20px;
  }

  .entry-metadata :global(.date-picker) { margin-bottom: 0; }

  @media (max-width: 600px) {
    .journal {
      padding: 20px 24px;
    }

    .journal-header {
      margin-bottom: 28px;
    }
  }
</style>
