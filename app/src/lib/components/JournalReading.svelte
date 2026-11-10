<script lang="ts">
  import { tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import DeleteEntryDialog from "$lib/components/DeleteEntryDialog.svelte";
  import EntryOptions from "$lib/components/EntryOptions.svelte";
  import EntryPlace from "$lib/components/EntryPlace.svelte";
  import WeatherTile from "$lib/components/WeatherTile.svelte";
  import { createWeatherReader, type WeatherView } from "$lib/weather";
  import JournalContent from "$lib/components/JournalContent.svelte";
  import { groupJournalEntries, visibleJournalEntries, journalEntryAnchor, journalDateAnchor, formatJournalDate, type ReadingEntry } from "$lib/journal-reading";
  import * as m from "$lib/paraglide/messages.js";

  let { entries, isLoading, errorMessage, locale, generation = null, onEntryDeleted }: {
    entries: ReadingEntry[]; isLoading: boolean; errorMessage: string; locale: "sv" | "en"; generation?: number | null;
    onEntryDeleted: (id: string) => void;
  } = $props();
  const visibleEntries = $derived(visibleJournalEntries(entries));
  const groups = $derived(groupJournalEntries(visibleEntries));
  const readWeather = createWeatherReader();
  let weather = $state<Record<string, WeatherView>>(Object.create(null));
  let weatherError = $state(false);
  let deleteTarget = $state<{ id: string; title: string; date: string; generation: number } | null>(null);
  let deletingEntry = $state(false);
  let deleteFailedId = $state<string | null>(null);

  function requestDelete(entry: ReadingEntry, closeAndFocus: () => void) {
    if (generation === null || deleteTarget || entry.editable !== true) return;
    closeAndFocus();
    deleteFailedId = null;
    deleteTarget = { id: entry.id, title: entry.title, date: entry.entryDate, generation };
  }

  function cancelDelete() {
    if (!deletingEntry) deleteTarget = null;
  }

  async function confirmDelete() {
    const target = deleteTarget;
    if (!target || deletingEntry) return;
    deletingEntry = true;
    try {
      await invoke("delete_journal_entry", { id: target.id, generation: target.generation });
    } catch {
      deleteFailedId = target.id;
      return;
    } finally {
      deletingEntry = false;
      deleteTarget = null;
    }

    const ordered = groups.flatMap(group => group.entries);
    const index = ordered.findIndex(entry => entry.id === target.id);
    const neighbor = ordered[index + 1] ?? ordered[index - 1];
    onEntryDeleted(target.id);
    window.dispatchEvent(new Event("mno:journal-changed"));
    await tick();
    if (neighbor) {
      document.getElementById(journalEntryAnchor(neighbor.id))
        ?.querySelector<HTMLAnchorElement>(".entry-open")?.focus();
    }
  }

  function openEntryFromClick(event: MouseEvent) {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    if (!(event.target instanceof Element) || event.target.closest("a, button, input, select, textarea, label, summary, .mno-photograph, .entry-metadata, .entry-options")) return;
    const article = event.currentTarget as HTMLElement;
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed && selection.containsNode(article, true)) return;
    article.querySelector<HTMLAnchorElement>(".entry-open")?.click();
  }

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

{#if deleteTarget}
  <DeleteEntryDialog title={deleteTarget.title} date={deleteTarget.date} {locale} busy={deletingEntry}
    oncancel={cancelDelete} onconfirm={confirmDelete} />
{/if}

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
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (The native entry link provides keyboard navigation without wrapping interactive content.) -->
        <article id={journalEntryAnchor(entry.id)} aria-label={entry.title || formatJournalDate(group.date, locale)} onclick={openEntryFromClick}>
          {#if entry.editable === true}
            <div class="entry-actions">
              <a class="entry-open" href={`/journal/entries/${encodeURIComponent(entry.id)}`}>
                {m.journal_view_entry({}, { locale })} · {entry.title || formatJournalDate(group.date, locale)}
              </a>
              <EntryOptions {locale}>
                {#snippet children(closeAndFocus)}
                  <a href={`/journal/entries/${encodeURIComponent(entry.id)}?edit=true&from=journal`}>{m.journal_edit({}, { locale })}</a>
                  {#if generation !== null}
                    <button type="button" onclick={() => requestDelete(entry, closeAndFocus)}>
                      {m.journal_delete_entry({}, { locale })}
                    </button>
                  {/if}
                {/snippet}
              </EntryOptions>
            </div>
          {/if}
          <div class="entry-metadata">
            {#key `${entry.id}:${generation}`}
              <EntryPlace entryId={entry.id} {generation} editing={false} {locale} />
              <WeatherTile status={weatherError ? { kind: "error" } : weather[entry.id] ? { kind: "ready", data: weather[entry.id] } : { kind: "loading" }} {locale} />
            {/key}
          </div>
          {#if deleteFailedId === entry.id}
            <p class="message error" role="alert">{m.journal_delete_failed({}, { locale })}</p>
          {/if}
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
  .entry-actions { position: relative; }
  .entry-open {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 3;
    color: var(--accent, #435b47);
    background: var(--surface, #faf9f6);
    padding: 8px 12px;
  }
  .entry-open:not(:focus) { clip-path: inset(50%); width: 1px; height: 1px; padding: 0; overflow: hidden; }
  .entry-metadata { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; min-width: 0; }
  .entry-metadata > :global(*) { min-width: 0; max-width: 100%; }
  .message { color: var(--text-muted, #686d63); font-size: 16px; }
  .message.error { color: #a33b35; }
</style>
