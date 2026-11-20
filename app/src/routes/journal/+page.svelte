<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import {createObservationCalendarReader} from "$lib/observation-calendar";
  import type {BirdList} from "$lib/birds";
  const readObservations=createObservationCalendarReader();
  import JournalReading from "$lib/components/JournalReading.svelte";
  import { createJournalReader } from "$lib/journal-data";
  import { resolveJournalTarget, type ReadingEntry } from "$lib/journal-reading";
  import { page } from "$app/state";
  import { afterNavigate } from "$app/navigation";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  const readJournal = createJournalReader();
  let observations=$state<BirdList[]>([]);
  let generation = $state<number | null>(null);
  let entries = $state<ReadingEntry[]>([]);
  let isLoading = $state(true);
  let errorMessage = $state("");

  let navigationMessage = $state("");
  let navigationRevision = $state(0);

  afterNavigate(() => {
    // Reapply the target after SvelteKit has finished its own scroll handling.
    navigationRevision++;
  });

  $effect(() => {
    const url = page.url;
    const revision = navigationRevision;
    const loading = isLoading;
    const error = errorMessage;
    // Reapply URL targets on navigation or initial load, while an in-place
    // deletion keeps the reader near its current position.
    const currentEntries = untrack(() => entries);
    const currentObservations = untrack(() => observations);
    const locale = language.current;
    let cancelled = false;
    navigationMessage = "";
    if (loading || error) return;
    const target = resolveJournalTarget(currentEntries, url.searchParams,currentObservations);
    void tick().then(() => {
      if (cancelled || revision !== navigationRevision || url !== page.url) return;
      if (target.missing) {
        const sv = {
          entry: "Anteckningen finns inte i journalens läsvy.",
          date: "Det finns inget synligt innehåll för det valda datumet.",
          latest: "Dagboken har inget synligt innehåll ännu.",
        };
        const en = {
          entry: "The entry is not available in the journal reading view.",
          date: "There is no visible content for the selected date.",
          latest: "The diary has no visible content yet.",
        };
        navigationMessage = (locale === "sv" ? sv : en)[target.missing];
      } else if (target.anchor) {
        document.getElementById(target.anchor)?.scrollIntoView({ block: "start", behavior: "instant" });
      }
    });
    return () => { cancelled = true; };
  });

  onMount(() => {
    let disposed = false, request=0;
    async function load(initial=false){
      const ticket=++request;
      try{
        const [snapshot,visits]=await Promise.all([readJournal<ReadingEntry[]>("read_journal_entries"),readObservations()]);
        if(disposed||ticket!==request)return;
        if(visits&&visits.generation!==snapshot.generation)throw "journal_generation_conflict";
        entries=snapshot.data;observations=visits?.data??[];generation=snapshot.generation;errorMessage="";
      }catch(error){if(!disposed&&ticket===request)errorMessage=m.journal_list_error({error:String(error)},{locale:language.current});}
      finally{if(!disposed&&ticket===request)isLoading=false;}
    }
    const refresh=()=>void load();window.addEventListener("mno:journal-changed",refresh);void load(true);
    return () => { disposed=true;request++;window.removeEventListener("mno:journal-changed",refresh); };
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

    {#if navigationMessage}
      <p class="navigation-message" role="status">{navigationMessage}</p>
    {/if}

    <JournalReading {generation} {entries} {observations} {isLoading} {errorMessage} locale={language.current}
      onEntryDeleted={(id) => { entries = entries.filter(entry => entry.id !== id); }} />

    <footer class="journal-footer">
      <span>My Nature Observations</span>
      <span>{m.journal_title({}, { locale: language.current })}</span>
    </footer>
  </div>
</main>

<style>
  .navigation-message { color: var(--text-muted, #686d63); font-size: 16px; }
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
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 13px;
    text-decoration: none;
  }

  .back-link:hover {
    text-decoration: underline;
  }

  .section-label {
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
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
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
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
</style>
