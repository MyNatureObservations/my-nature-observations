<script lang="ts">
  import type { Snippet } from "svelte";
  import { createJournalReader } from "$lib/journal-data";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import JournalEditor from "$lib/components/JournalEditor.svelte";
  import DatePicker from "$lib/components/DatePicker.svelte";
  import EntryPlace from "$lib/components/EntryPlace.svelte";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  const readJournal = createJournalReader();

  type JournalEntry = {
    id: string;
    entryDate: string;
    title: string;
    contentJson: string;
  };

  const journalEntryId = page.params.id;
  const fromSearch = $derived(page.url.searchParams.get("from") === "search");
  const fromHistory = $derived(page.url.searchParams.get("from") === "history");
  function historyBackHref() {
    const year = page.url.searchParams.get("year");
    const month = page.url.searchParams.get("month");
    if (year && month && /^\d{1,4}$/.test(year) && Number(year) >= 1 && Number(year) <= 9999 && /^\d{1,2}$/.test(month) && Number(month) >= 1 && Number(month) <= 12) {
      return `/history?year=${Number(year)}&month=${Number(month)}`;
    }
    return page.url.searchParams.get("undated") === "true" ? "/history?undated=true" : "/history";
  }
  const backHref = $derived(fromHistory ? historyBackHref() : fromSearch ? `/search?q=${encodeURIComponent(page.url.searchParams.get("q") ?? "")}` : "/journal?latest=true");

  let entryDate = $state("");
  let isEditing = $state(false);
  let isLoading = $state(true);
  let errorMessage = $state("");
  let editorEntryId = $state<string | null>(journalEntryId ?? null);
  let generation = $state<number | null>(null);
  let placeDirty = $state(false);
  let placeBusy = $state(false);

  function isValidEntryDate(value: string): boolean {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith("0000")) return false;
    const date = new Date(`${value}T12:00:00Z`);
    return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
  }
  const validEntryDate = $derived(isValidEntryDate(entryDate));
  const formattedDate = $derived(
    validEntryDate
      ? new Intl.DateTimeFormat(language.current === "sv" ? "sv-SE" : "en-GB", {
          day: "numeric", month: "long", year: "numeric", timeZone: "UTC",
        }).format(new Date(`${entryDate}T12:00:00Z`))
      : isLoading ? "" : m.history_invalid_date({}, { locale: language.current }),
  );

  onMount(() => {
    async function loadEntryDetails() {
      try {
        const { data: entry } = await readJournal<JournalEntry | null>("get_journal_entry", {
          id: journalEntryId,
        });

        if (!entry) {
          errorMessage = m.journal_entry_not_found(
            {},
            { locale: language.current },
          );
          return;
        }

        entryDate = entry.entryDate;
      } catch (error) {
        errorMessage = m.journal_entry_load_error(
          { error: String(error) },
          { locale: language.current },
        );
      } finally {
        isLoading = false;
      }
    }

    void loadEntryDetails();
  });
</script>

<svelte:head>
  <title
    >{m.journal_title({}, { locale: language.current })} — My Nature Observations</title
  >
</svelte:head>

<main class="journal">
  <div class="journal-content">
    <header class="journal-header">
      <a href={backHref} class="back-link">
        ← {fromHistory ? m.history_back({}, { locale: language.current }) : fromSearch ? m.search_back({}, { locale: language.current }) : m.journal_all_entries({}, { locale: language.current })}
      </a>

      <span class="section-label">
        {m.journal_section({}, { locale: language.current })}
      </span>
    </header>

    {#if isLoading}
      <p class="message">
        {m.journal_entry_loading({}, { locale: language.current })}
      </p>
    {:else if errorMessage}
      <p class="message error" role="alert">{errorMessage}</p>
    {:else}
      <section class="journal-entry">
        {#snippet metadata(weather: Snippet, options: Snippet | undefined)}
        <div class="entry-metadata">
          {#if isEditing && validEntryDate}
            <DatePicker bind:value={entryDate} />
          {:else}
            <div class="entry-date">{formattedDate}</div>
          {/if}
          <EntryPlace entryId={editorEntryId} {generation} editing={isEditing} locale={language.current} onDraft={(dirty, busy) => { placeDirty = dirty; placeBusy = busy; }} />
          {@render weather()}
          {#if options}
            <div class="entry-options-slot">{@render options()}</div>
          {/if}
        </div>
        {/snippet}

        <JournalEditor
          {metadata}
          {entryDate}
          {journalEntryId}
          startInEditMode={page.url.searchParams.get("edit") === "true"}
          returnToJournal={page.url.searchParams.get("from") === "journal"}
          {placeDirty}
          {placeBusy}
          onSessionChange={(session) => {
            editorEntryId = session.entryId;
            generation = session.generation;
          }}
          onEditingChange={(editing) => {
            isEditing = editing;
          }}
        />
      </section>
    {/if}

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

  .entry-date {
    font-family: system-ui, sans-serif;
    font-size: 12px;
    letter-spacing: 1px;
    color: var(--text-muted, #686d63);
  }

  .entry-metadata {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    margin-bottom: 20px;
  }

  .entry-options-slot { margin-inline-start: auto; flex: none; }

  .entry-metadata :global(.date-picker) { margin-bottom: 0; }

  .message {
    color: var(--text-muted, #686d63);
    font-size: 16px;
  }

  .message.error {
    color: #a33b35;
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

  @media (max-width: 600px) {
    .journal {
      padding: 20px 24px;
    }

    .journal-header {
      margin-bottom: 28px;
    }
  }

  .journal-entry {
    position: relative;
  }
</style>
