<script lang="ts">
  import "$lib/journal-photographs.css";
  import ApplicationZoom from "$lib/components/ApplicationZoom.svelte";
  import DevelopmentIndicator from "$lib/components/DevelopmentIndicator.svelte";
  import { onMount, setContext } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";
  import { initializeLanguage } from "$lib/language.svelte";
  import RecoveryPanel from "$lib/components/RecoveryPanel.svelte";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import AppNavigation from "$lib/components/AppNavigation.svelte";
  import { createPlaceMap, PLACE_MAP } from "$lib/place-map.svelte";
  setContext(PLACE_MAP, createPlaceMap());

  let { children } = $props();

  let result = $state<"restored" | "previous" | "imported" | null>(null);
  let importConfirmationConsumed = false;
  afterNavigate(({ from, to }) => {
    if (result === "imported" && from?.url.pathname === "/" && to?.url.pathname !== "/") {
      result = null;
      importConfirmationConsumed = true;
    }
  });
  async function recoveryFinished(restored: boolean, operation: "import" | "restore" | null) {
    if (locallyBlocked || (mode !== "recovery" && mode !== "ready")) return;
    const imported = restored && operation === "import";
    if (imported && importConfirmationConsumed) return;
    result = imported ? "imported" : restored ? "restored" : "previous";
    await goto(operation === "import" ? "/" : "/journal");
  }
  let mode = $state<"checking" | "ready" | "recovery" | "blocked">("checking");
  let locallyBlocked = false;
  let replacementPrepared = $state(false);
  let request = 0;
  function blockJournal(prepared = false) {
    replacementPrepared = replacementPrepared || prepared;
    locallyBlocked = true;
    request++;
    mode = "blocked";
  }
  setContext("mno:block-journal", blockJournal);
  onMount(() => {
    initializeLanguage();
    let disposed = false;
    async function refreshMode() {
      if (locallyBlocked) return;
      const current = ++request;
      try {
        const next = await invoke<"ready" | "recovery" | "blocked">("database_mode");
        if (!disposed && !locallyBlocked && current === request) {
          if (next === "blocked") blockJournal();
          else mode = next;
        }
      } catch {
        if (!disposed && current === request) blockJournal();
      }
    }
    const unlisten = listen("mno:database-mode-changed", () => { void refreshMode(); });
    void unlisten.then(() => refreshMode()).catch(() => { if (!disposed) blockJournal(); });
    return () => { disposed = true; void unlisten.then((stop) => stop()).catch(() => {}); };
  });
</script>

<ApplicationZoom />
<DevelopmentIndicator />

{#if mode === "ready"}
  <AppNavigation />
  {#if result && (result !== "imported" || page.url.pathname === "/")}<p role="status" class="restore-result">{result === "imported" ? m.import_verified({}, { locale: language.current }) : result === "restored" ? m.restore_success({}, { locale: language.current }) : m.restore_previous({}, { locale: language.current })}</p>{/if}
  {@render children()}
{:else if mode === "checking"}
  <main aria-live="polite"><p>{m.database_checking({}, { locale: language.current })}</p></main>
{:else}
  {#key mode}<RecoveryPanel {mode} prepared={replacementPrepared} onfinished={recoveryFinished} />{/key}
{/if}

<style>
  .restore-result { margin: 20px auto; max-width: 640px; padding: 0 28px; color: #435b47; }
  :global(:focus-visible) {
    outline: 2px solid #788e7b;
    outline-offset: 3px;
  }
</style>
