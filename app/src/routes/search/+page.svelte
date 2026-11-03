<script lang="ts">
  import { page } from "$app/state";
  import { createJournalReader } from "$lib/journal-data";
  import { formatJournalDate } from "$lib/journal-reading";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  type Hit = { id: string; entryDate: string; title: string; excerpt: string };
  type Results = { entries: Hit[]; more: boolean };
  const readJournal = createJournalReader();
  let entries = $state<Hit[]>([]);
  let loading = $state(false);
  let failed = $state(false);
  let more = $state(false);
  let request = 0;
  let disposed = false;
  const query = $derived(page.url.searchParams.get("q") ?? "");
  async function load(text: string, append = false) {
    if (append && loading) return;
    const current = ++request;
    const offset = append ? entries.length : 0;
    if (!append) entries = [];
    failed = false; more = false;
    if (!text.trim()) { loading = false; return; }
    loading = true;
    try {
      const result = await readJournal<Results>("search_journal", { query: text, offset });
      if (!disposed && current === request) {
        entries = append ? [...entries, ...result.data.entries] : result.data.entries;
        more = result.data.more;
      }
    } catch {
      if (!disposed && current === request) { failed = true; entries = []; }
    } finally { if (!disposed && current === request) loading = false; }
  }
  $effect(() => { void load(query); });
  $effect(() => {
    const changed = () => { void load(page.url.searchParams.get("q") ?? ""); };
    window.addEventListener("mno:journal-changed", changed);
    return () => { disposed = true; request++; window.removeEventListener("mno:journal-changed", changed); };
  });
</script>
<svelte:head><title>{m.search_title({}, { locale: language.current })} — My Nature Observations</title></svelte:head>
<div class="search-space">
<main aria-busy={loading}>
  <a class="back-link" href="/">← {m.restore_back_to_home({}, { locale: language.current })}</a>
  <h1>{m.search_title({}, { locale: language.current })}</h1>
  {#if !query.trim()}
    <p>{m.search_instruction({}, { locale: language.current })}</p>
  {:else}
    <p class="query">{m.search_query({ query }, { locale: language.current })}</p>
    {#if loading}<p role="status">{m.search_loading({}, { locale: language.current })}</p>{/if}
    {#if failed}<p role="alert">{m.search_failed({}, { locale: language.current })}</p>
    {:else if !loading && entries.length === 0}<p role="status">{m.search_empty({}, { locale: language.current })}</p>{/if}
    <ul>
      {#each entries as entry (entry.id)}
        <li>
          <a href={`/journal/entries/${encodeURIComponent(entry.id)}?from=search&q=${encodeURIComponent(query)}`}>
            <h2>{entry.title || m.search_untitled({}, { locale: language.current })}</h2>
            <time datetime={entry.entryDate}>{formatJournalDate(entry.entryDate, language.current)}</time>
            <p class="excerpt">{entry.excerpt}</p>
          </a>
        </li>
      {/each}
    </ul>
    {#if more}<button disabled={loading} onclick={() => load(query, true)}>{m.search_more({}, { locale: language.current })}</button>{/if}
  {/if}
</main>
</div>
<style>
  main { box-sizing: border-box; max-width: 760px; margin: 0 auto; padding: 56px 28px; overflow-wrap: anywhere; color: #343530; }
  @media (min-width: 1000px) {
    :global(body:has(.left-panel.search-docked)) .search-space { padding-left: 360px; }
  }
  .back-link { display: inline-block; margin-bottom: 28px; color: var(--accent, #435b47); font: 13px system-ui, sans-serif; }
  .back-link:hover { text-decoration: underline; }
  h1 { font-size: 26px; font-weight: normal; margin: 0 0 24px; }
  .query, time { color: var(--accent, #435b47); font: 13px system-ui, sans-serif; }
  ul { list-style: none; padding: 0; }
  li { border-bottom: 1px solid #d9d8d1; padding: 20px 0; }
  a { display: block; color: inherit; text-decoration: none; }
  a:hover h2 { text-decoration: underline; }
  h2 { font-size: 20px; font-weight: normal; margin: 0 0 8px; }
  p { line-height: 1.6; }
  .excerpt { white-space: pre-line; margin: 12px 0 0; }
  button { background: transparent; color: #435b47; border: 1px solid #d9d8d1; padding: 10px 14px; border-radius: 4px; cursor: pointer; }
  @media (max-width: 600px) { main { padding: 40px 24px; } }
</style>
