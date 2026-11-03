<script lang="ts">
  import { onMount } from "svelte";
  import { afterNavigate, replaceState } from "$app/navigation";
  import { page } from "$app/state";
  import { createJournalReader } from "$lib/journal-data";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  type Overview = {
    totalEntries: number;
    years: { year: number; count: number; months: { month: number; count: number }[] }[];
    undatedCount: number;
  };
  type Entry = { id: string; entryDate: string; title: string };
  type Bucket = {
    year?: number; month?: number; open: boolean; entries: Entry[];
    loaded: boolean; loading: boolean; failed: boolean; hasMore: boolean; request: number;
  };
  const readOverview = createJournalReader();
  // Separate readers allow different months to load concurrently without superseding each other.
  const readers = new Map<string, ReturnType<typeof createJournalReader>>();
  let overview = $state<Overview | null>(null);
  let buckets = $state<Record<string, Bucket>>({});
  let openYears = $state<Record<number, boolean>>({});
  let loading = $state(true);
  let failed = $state(false);
  let epoch = 0;
  let disposed = false;

  function periodKey(year: number, month: number) { return `${year}-${month}`; }
  function makeBucket(year?: number, month?: number): Bucket {
    return { year, month, open: false, entries: [], loaded: false, loading: false, failed: false, hasMore: false, request: 0 };
  }
  async function loadOverview() {
    const current = ++epoch;
    loading = true;
    failed = false;
    overview = null;
    buckets = {};
    readers.clear();
    try {
      const result = await readOverview<Overview>("journal_history_overview");
      if (disposed || current !== epoch) return;
      const next: Record<string, Bucket> = {};
      for (const year of result.data.years) {
        for (const month of year.months) next[periodKey(year.year, month.month)] = makeBucket(year.year, month.month);
      }
      if (result.data.undatedCount) next.undated = makeBucket();
      buckets = next;
      openYears = result.data.years.length ? { [result.data.years[0].year]: true } : {};
      overview = result.data;
      restoreSelection();
    } catch {
      if (!disposed && current === epoch) failed = true;
    } finally {
      if (!disposed && current === epoch) loading = false;
    }
  }
  function restoreSelection() {
    const year = Number(page.url.searchParams.get("year"));
    const month = Number(page.url.searchParams.get("month"));
    const key = page.url.searchParams.get("undated") === "true" ? "undated" : periodKey(year, month);
    if (buckets[key]) {
      if (key !== "undated") openYears[year] = true;
      buckets[key].open = true;
      if (!buckets[key].loaded) void loadEntries(key);
    }
  }
  function rememberSelection(bucket: Bucket) {
    const url = new URL(page.url);
    url.search = bucket.year === undefined ? "?undated=true" : `?year=${bucket.year}&month=${bucket.month}`;
    replaceState(url, page.state);
  }
  function toggleMonth(key: string, open: boolean) {
    const bucket = buckets[key];
    if (!bucket || bucket.open === open) return;
    bucket.open = open;
    if (open) {
      rememberSelection(bucket);
      if (!bucket.loaded) void loadEntries(key);
    } else {
      bucket.request++;
      bucket.loading = false;
    }
  }
  async function loadEntries(key: string, more = false) {
    const bucket = buckets[key];
    if (!bucket || bucket.loading || (more && !bucket.hasMore)) return;
    const currentEpoch = epoch;
    const request = ++bucket.request;
    bucket.loading = true;
    bucket.failed = false;
    if (!readers.has(key)) readers.set(key, createJournalReader());
    try {
      const result = await readers.get(key)!<{ entries: Entry[]; hasMore: boolean }>("journal_history_entries", {
        year: bucket.year ?? null, month: bucket.month ?? null, offset: more ? bucket.entries.length : 0,
      });
      if (disposed || currentEpoch !== epoch || request !== bucket.request) return;
      bucket.entries = more ? [...bucket.entries, ...result.data.entries] : result.data.entries;
      bucket.hasMore = result.data.hasMore;
      bucket.loaded = true;
    } catch {
      if (!disposed && currentEpoch === epoch && request === bucket.request) bucket.failed = true;
    } finally {
      if (!disposed && currentEpoch === epoch && request === bucket.request) bucket.loading = false;
    }
  }
  function entryHref(entry: Entry, bucket: Bucket) {
    const selection = bucket.year === undefined ? "undated=true" : `year=${bucket.year}&month=${bucket.month}`;
    return `/journal/entries/${encodeURIComponent(entry.id)}?from=history&${selection}`;
  }
  function countLabel(count: number) {
    return count === 1
      ? m.history_count_one({}, { locale: language.current })
      : m.history_count({ count }, { locale: language.current });
  }
  function monthName(month: number) {
    return new Intl.DateTimeFormat(language.current === "sv" ? "sv-SE" : "en-GB", { month: "long", timeZone: "UTC" }).format(new Date(Date.UTC(2000, month - 1, 1)));
  }
  afterNavigate(() => { if (overview) restoreSelection(); });
  onMount(() => {
    void loadOverview();
    const changed = () => { void loadOverview(); };
    window.addEventListener("mno:journal-changed", changed);
    return () => {
      disposed = true;
      epoch++;
      window.removeEventListener("mno:journal-changed", changed);
    };
  });
</script>

<svelte:head><title>{m.history_title({}, { locale: language.current })} — My Nature Observations</title></svelte:head>

{#snippet entries(key: string)}
  {@const bucket = buckets[key]}
  <div class="entries" aria-busy={bucket.loading}>
    {#if bucket.entries.length}
      <ul>
        {#each bucket.entries as entry (entry.id)}
          <li><a href={entryHref(entry, bucket)}>
            <span>{entry.title || m.history_untitled({}, { locale: language.current })}</span>
            <span class="date">{bucket.year === undefined ? m.history_invalid_date({}, { locale: language.current }) : entry.entryDate}</span>
          </a></li>
        {/each}
      </ul>
    {:else if bucket.loaded && !bucket.loading}
      <p role="status">{m.history_empty({}, { locale: language.current })}</p>
    {/if}
    {#if bucket.loading}<p role="status">{m.history_loading({}, { locale: language.current })}</p>{/if}
    {#if bucket.failed}
      <p role="alert">{m.history_error({}, { locale: language.current })}</p>
      <button onclick={() => loadEntries(key, bucket.loaded)}>{m.history_retry({}, { locale: language.current })}</button>
    {:else if bucket.hasMore}
      <button disabled={bucket.loading} onclick={() => loadEntries(key, true)}>{m.history_more({}, { locale: language.current })}</button>
    {/if}
  </div>
{/snippet}

<main class="history">
  <a class="back-link" href="/">← {m.restore_back_to_home({}, { locale: language.current })}</a>
  <h1>{m.history_title({}, { locale: language.current })}</h1>
  <p class="intro">{m.history_intro({}, { locale: language.current })}</p>
  {#if loading}<p role="status">{m.history_loading({}, { locale: language.current })}</p>
  {:else if failed}
    <p role="alert">{m.history_error({}, { locale: language.current })}</p>
    <button onclick={() => loadOverview()}>{m.history_retry({}, { locale: language.current })}</button>
  {:else if overview}
    {#if overview.totalEntries === 0}<p>{m.history_empty({}, { locale: language.current })}</p>{/if}
    {#each overview.years as year (year.year)}
      <details class="year" open={openYears[year.year] ?? false} ontoggle={(event) => { openYears[year.year] = event.currentTarget.open; }}>
        <summary><span>{year.year}</span><span class="count">{countLabel(year.count)}</span></summary>
        <div class="months">
          {#each year.months as month (month.month)}
            {@const key = periodKey(year.year, month.month)}
            <details open={buckets[key].open} ontoggle={(event) => toggleMonth(key, event.currentTarget.open)}>
              <summary><span>{monthName(month.month)}</span><span class="count">{countLabel(month.count)}</span></summary>
              {@render entries(key)}
            </details>
          {/each}
        </div>
      </details>
    {/each}
    {#if overview.undatedCount}
      <details class="year" open={buckets.undated.open} ontoggle={(event) => toggleMonth("undated", event.currentTarget.open)}>
        <summary><span>{m.history_undated({}, { locale: language.current })}</span><span class="count">{countLabel(overview.undatedCount)}</span></summary>
        {@render entries("undated")}
      </details>
    {/if}
  {/if}
</main>

<style>
  .history { max-width: 760px; margin: 0 auto; padding: 56px 28px 40px; color: #343530; }
  .back-link { color: var(--accent, #435b47); font: 13px system-ui, sans-serif; text-decoration: none; }
  .back-link:hover { text-decoration: underline; }
  h1 { font-size: 30px; font-weight: 400; margin: 36px 0 12px; }
  .intro { color: #72766d; margin-bottom: 32px; }
  .year { border-top: 1px solid #d9d8d1; }
  summary { cursor: pointer; padding: 16px 0; overflow-wrap: anywhere; }
  summary > span:first-of-type { margin-left: 8px; }
  .count { display: inline-block; margin-left: 16px; color: #72766d; font: 12px system-ui, sans-serif; }
  .months { margin-left: 20px; }
  .months summary { text-transform: capitalize; }
  .entries { padding: 0 0 16px 24px; font: 14px system-ui, sans-serif; }
  ul { list-style: none; margin: 0; padding: 0; }
  li a { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px 20px; padding: 12px 0; color: #435b47; text-decoration: none; overflow-wrap: anywhere; }
  li a:hover { text-decoration: underline; }
  .date { color: #72766d; font-size: 12px; }
  button { background: transparent; color: #435b47; border: 1px solid #d9d8d1; border-radius: 6px; padding: 9px 14px; cursor: pointer; font: 13px system-ui, sans-serif; }
  button:disabled { opacity: .6; cursor: default; }
  @media (max-width: 600px) { .history { padding: 40px 20px; } .months { margin-left: 12px; } .entries { padding-left: 16px; } }
</style>
