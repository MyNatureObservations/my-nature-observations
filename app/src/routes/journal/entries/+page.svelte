<script lang="ts">
  import { createJournalReader } from "$lib/journal-data";
  import WeatherOverview from "$lib/components/WeatherOverview.svelte";
  import {createWeatherReader,type WeatherView} from "$lib/weather";
  import { onMount } from "svelte";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  const readJournal = createJournalReader();
  const readWeather=createWeatherReader();
  let weather=$state<Record<string,WeatherView>>({});

  type JournalEntrySummary = {
    id: string;
    entryDate: string;
    title: string;
  };

  let entries = $state<JournalEntrySummary[]>([]);
  let isLoading = $state(true);
  let errorMessage = $state("");

  function formatDate(date: string) {
    // Use UTC to prevent date-only values from shifting between time zones.
    return new Intl.DateTimeFormat(
      language.current === "sv" ? "sv-SE" : "en-GB",
      {
        day: "numeric",
        month: "long",
        year: "numeric",
        timeZone: "UTC",
      },
    ).format(new Date(`${date}T12:00:00Z`));
  }

  onMount(() => {
    let disposed=false;
    async function loadEntries() {
      try {
        const snapshot=await readJournal<JournalEntrySummary[]>("list_journal_entries");
        if(disposed)return;
        entries=snapshot.data;
        void readWeather(entries.map(e=>e.id),snapshot.generation).then(data=>{if(!disposed)weather=data;}).catch(()=>{});
      } catch (error) {
        errorMessage = m.journal_list_error(
          { error: String(error) },
          { locale: language.current },
        );
      } finally {
        isLoading = false;
      }
    }

    void loadEntries();
    return ()=>{disposed=true;};
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
      <a href="/" class="back-link">← My Nature Observations</a>
      <span class="section-label">
        {m.journal_section({}, { locale: language.current })}
      </span>
    </header>

    <div class="page-heading">
      <h1>{m.journal_title({}, { locale: language.current })}</h1>
      <p>
        {m.journal_collection_description({}, { locale: language.current })}
      </p>
    </div>

    <a href="/journal/new" class="new-entry-link">
      + {m.journal_new_entry({}, { locale: language.current })}
    </a>

    {#if isLoading}
      <p class="message">
        {m.journal_list_loading({}, { locale: language.current })}
      </p>
    {:else if errorMessage}
      <p class="message error" role="alert">{errorMessage}</p>
    {:else if entries.length === 0}
      <p class="message">
        {m.journal_empty({}, { locale: language.current })}
      </p>
    {:else}
      <div class="entry-list">
        {#each entries as entry (entry.id)}
          <a href={`/journal/entries/${entry.id}`} class="entry-item">
            <div class="entry-date">{formatDate(entry.entryDate)} {#if weather[entry.id]?.manual || weather[entry.id]?.fetched}<WeatherOverview data={weather[entry.id]} locale={language.current} compact />{/if}</div>
            <h2>{entry.title}</h2>
          </a>
        {/each}
      </div>
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
    min-height: 100vh;
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

  .section-label {
    font-family: system-ui, sans-serif;
    font-size: 11px;
    letter-spacing: 2px;
    color: var(--accent, #435b47);
  }

  .page-heading {
    margin-bottom: 64px;
  }

  h1 {
    margin: 0 0 12px;
    font-size: 42px;
    font-weight: normal;
  }

  .page-heading p {
    color: var(--text-muted, #686d63);
    font-size: 17px;
    font-style: italic;
  }

  .entry-item {
    display: block;
    padding: 28px 0;
    border-bottom: 1px solid #d9d8d1;
    color: inherit;
    text-decoration: none;
    transition: padding-left 0.2s ease;
  }

  .entry-item:hover {
    padding-left: 8px;
  }

  .entry-item:hover h2 {
    color: #435b47;
  }

  .entry-item:focus-visible {
    outline: 2px solid #788e7b;
    outline-offset: 4px;
    border-radius: 4px;
  }

  .entry-date {
    display:flex;align-items:center;flex-wrap:wrap;gap:12px;
    margin-bottom: 12px;
    font-family: system-ui, sans-serif;
    font-size: 12px;
    letter-spacing: 1px;
    color: var(--text-muted, #686d63);
  }

  h2 {
    margin: 0;
    font-size: 26px;
    font-weight: normal;
    line-height: 1.3;
  }

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

  .new-entry-link {
    display: inline-block;
    margin-bottom: 36px;
    padding: 10px 16px;
    border: 1px solid #d9d8d1;
    border-radius: 8px;
    color: #435b47;
    font-family: system-ui, sans-serif;
    font-size: 13px;
    text-decoration: none;
  }

  .new-entry-link:hover {
    background: #e1e7df;
  }
</style>
