<script lang="ts">
  import EntryPlace from "$lib/components/EntryPlace.svelte";
  import WeatherTile from "$lib/components/WeatherTile.svelte";
  import { createWeatherReader, type WeatherView } from "$lib/weather";
  import JournalContent from "$lib/components/JournalContent.svelte";
  import { groupJournalEntries, visibleJournalEntries, journalEntryAnchor, journalDateAnchor, formatJournalDate, type ReadingEntry } from "$lib/journal-reading";
  import * as m from "$lib/paraglide/messages.js";

  let { entries, isLoading, errorMessage, locale, generation = null }: {
    entries: ReadingEntry[]; isLoading: boolean; errorMessage: string; locale: "sv" | "en"; generation?: number | null;
  } = $props();
  const visibleEntries = $derived(visibleJournalEntries(entries));
  const groups = $derived(groupJournalEntries(visibleEntries));
  const readWeather = createWeatherReader();
  let weather = $state<Record<string, WeatherView>>(Object.create(null));
  let weatherError = $state(false);
  $effect(() => {
    const expected = generation;
    const ids = visibleEntries.map(entry => entry.id);
    let cancelled = false;
    weather = Object.create(null); weatherError = false;
    if (expected !== null && ids.length) {
      void readWeather(ids, expected).then(data => { if (!cancelled) weather = data; })
        .catch(() => { if (!cancelled) weatherError = true; });
    }
    return () => { cancelled = true; };
  });
</script>

{#if isLoading}
  <p class="message" role="status">{m.journal_list_loading({}, { locale })}</p>
{:else if errorMessage}
  <p class="message error" role="alert">{errorMessage}</p>
{:else if visibleEntries.length === 0}
  <p class="message">{m.journal_empty({}, { locale })}</p>
{:else}
  {#each groups as group (group.date)}
    <section id={journalDateAnchor(group.date)} class="journal-day" aria-label={formatJournalDate(group.date, locale)}>
      <h2 class="entry-date"><time datetime={group.date}>{formatJournalDate(group.date, locale)}</time></h2>
      {#each group.entries as entry (entry.id)}
        <article id={journalEntryAnchor(entry.id)} aria-label={entry.title || formatJournalDate(group.date, locale)}>
          {#if entry.editable === true}
            <div class="entry-actions">
              <a href={`/journal/entries/${encodeURIComponent(entry.id)}?edit=true&from=journal`}>{m.journal_edit({}, { locale })}</a>
            </div>
          {/if}
          <div class="entry-metadata">
            {#key `${entry.id}:${generation}`}
              <EntryPlace entryId={entry.id} {generation} editing={false} {locale} />
              <WeatherTile status={weatherError ? { kind: "error" } : weather[entry.id] ? { kind: "ready", data: weather[entry.id] } : { kind: "loading" }} {locale} />
            {/key}
          </div>
          <JournalContent contentJson={entry.contentJson} entryId={entry.id} documentVersion={entry.documentVersion ?? 1} {generation} {locale} editable={entry.editable === true} />
        </article>
      {/each}
    </section>
  {/each}
{/if}

<style>
  .journal-day { scroll-margin-top: 64px; }
  .journal-day + .journal-day { margin-top: 72px; }
  .entry-date {
    display: flex;
    align-items: center;
    gap: 16px;
    margin: 0 0 36px;
    font-family: system-ui, sans-serif;
    font-size: 12px;
    font-weight: normal;
    letter-spacing: 1px;
    color: var(--text-muted, #686d63);
  }
  .entry-date::before, .entry-date::after {
    content: "";
    flex: 1;
    height: 1px;
    background: #d9d8d1;
  }
  .entry-date time { flex-shrink: 0; }
  article + article { margin-top: 32px; position: relative; }
  article + article::before {
    content: "";
    display: block;
    width: 132px;
    height: 1px;
    margin: 0 auto 32px;
    background: linear-gradient(to right,
      #d9d8d1 0 calc(50% - 10px),
      transparent calc(50% - 10px) calc(50% + 10px),
      #d9d8d1 calc(50% + 10px) 100%);
  }
  article + article::after {
    content: "";
    position: absolute;
    top: -1.5px;
    left: 50%;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    transform: translateX(-50%);
    background: #d9d8d1;
  }
  article { scroll-margin-top: 64px; display: flow-root; }
  .entry-metadata { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; min-width: 0; }
  .entry-metadata > :global(*) { min-width: 0; max-width: 100%; }
  .entry-actions { text-align: right; margin-bottom: 12px; }
  .entry-actions a { color: #435b47; font-size: 13px; text-decoration: none; }
  .entry-actions a:hover { text-decoration: underline; }
  .message { color: var(--text-muted, #686d63); font-size: 16px; }
  .message.error { color: #a33b35; }
</style>
