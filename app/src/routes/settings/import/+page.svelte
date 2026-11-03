<script lang="ts">
  import { getContext, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { beforeNavigate } from "$app/navigation";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  type Selection = { token: string; format_version: number; entry_count: number; existing_entry_count: number; existing_place_count: number; imported_place_count: number; existing_setting_count: number; imported_setting_count: number };
  type ActivationResult = { activated: boolean; prepared: boolean; error: string | null; mode: "ready" | "recovery" | "blocked" | "unknown" };
  const blockJournal = getContext<(prepared?: boolean) => void>("mno:block-journal");
  let phase = $state<"idle" | "checking" | "confirmed" | "cancelling" | "writing" | "blocked">("idle");
  let token = $state<string | null>(null);
  let entryCount = $state(0);
  let existingEntryCount = $state(0);
  let existingPlaceCount = $state(0);
  let importedPlaceCount = $state(0);
  let existingSettingCount = $state(0);
  let importedSettingCount = $state(0);
  function hasExistingData() {
    return existingEntryCount > 0 || existingPlaceCount > 0 || existingSettingCount > 0;
  }
  let formatVersion = $state(0);
  let error = $state("");
  let notice = $state("");
  let disposed = false;
  let revision = 0;

  beforeNavigate((navigation) => {
    if (phase === "writing" || phase === "blocked") navigation.cancel();
  });
  async function release(selected: string) {
    await invoke("cancel_open_import", { token: selected });
  }
  onDestroy(() => {
    disposed = true;
    revision++;
    if (token) void release(token).catch(() => {});
  });
  async function choose() {
    if (disposed || phase !== "idle") return;
    const current = ++revision;
    phase = "checking"; error = ""; notice = "";
    try {
      const selection = await invoke<Selection | null>("choose_open_import", {
        dialogTitle: m.import_open({}, { locale: language.current }),
      });
      if (disposed || current !== revision) {
        if (selection) void release(selection.token).catch(() => {});
        return;
      }
      if (!selection) { phase = "idle"; notice = "cancelled"; return; }
      token = selection.token;
      entryCount = selection.entry_count;
      existingEntryCount = selection.existing_entry_count ?? 0;
      existingPlaceCount = selection.existing_place_count ?? 0;
      importedPlaceCount = selection.imported_place_count ?? 0;
      existingSettingCount = selection.existing_setting_count ?? 0;
      importedSettingCount = selection.imported_setting_count ?? 0;
      formatVersion = selection.format_version;
      phase = "confirmed";
    } catch (cause) {
      if (disposed || current !== revision) return;
      phase = "idle";
      error = errorCode(cause);
    }
  }
  function errorCode(cause: unknown) {
    return typeof cause === "string" && ["occupied", "invalid_package", "changed", "busy"].includes(cause)
      ? cause : "failed";
  }
  async function cancel() {
    if (disposed || phase !== "confirmed" || !token) return;
    const selected = token;
    const current = ++revision;
    phase = "cancelling"; error = "";
    try {
      await release(selected);
      if (disposed || current !== revision) return;
      token = null; phase = "idle"; notice = "cancelled";
    } catch {
      if (disposed || current !== revision) return;
      phase = "confirmed"; error = "cancel_failed";
    }
  }
  function block() {
    phase = "blocked";
    blockJournal(); // Restrictive only: this page never grants Ready.
  }
  async function activate() {
    if (disposed || phase !== "confirmed" || !token) return;
    const selected = token;
    token = null; // Consumed by the backend; unmount must not cancel activation.
    const current = ++revision;
    phase = "writing"; error = ""; notice = "";
    try {
      const result = await invoke<ActivationResult>("activate_open_import", { token: selected });
      // The mode event may already have unmounted this page. A successful
      // preparation reply can only add a notice to the still-blocked app shell.
      if (result?.prepared === true && result.activated === false && result.error === null
          && (result.mode === "blocked" || result.mode === "recovery")) {
        blockJournal(true);
        if (!disposed && current === revision) phase = "blocked";
        return;
      }
      if (disposed || current !== revision) return;
      // Only an explicit pre-activation error with Ready permits another attempt.
      // Malformed replies and contradictory success/Ready combinations stay closed.
      if (result && result.mode === "ready" && result.activated === false && result.prepared !== true && typeof result.error === "string") {
        phase = "idle"; error = errorCode(result.error);
      } else {
        block();
      }
    } catch {
      if (!disposed && current === revision) block();
    }
  }
  function errorMessage() {
    const options = { locale: language.current };
    switch (error) {
      case "occupied": return m.import_occupied({}, options);
      case "invalid_package": return m.import_invalid({}, options);
      case "changed": return m.import_changed({}, options);
      case "busy": return m.import_busy({}, options);
      case "cancel_failed": return m.import_cancel_failed({}, options);
      default: return m.import_failed({}, options);
    }
  }
</script>

<main>
  {#if phase !== "writing" && phase !== "blocked"}
    <a class="back-link" href="/">← {m.restore_back_to_home({}, { locale: language.current })}</a>
  {/if}
  <h1>{phase === "confirmed" && hasExistingData() ? m.import_replace_title({}, { locale: language.current }) : m.import_open({}, { locale: language.current })}</h1>
  {#if phase !== "confirmed" || !hasExistingData()}
    <p>{m.import_intro({}, { locale: language.current })}</p>
  {/if}
  {#if phase === "confirmed"}
    {#if hasExistingData()}
      <p role="status">{m.import_replace_valid({}, { locale: language.current })}</p>
      <p>{m.import_current_count({ count: existingEntryCount }, { locale: language.current })}<br />{m.import_incoming_count({ count: entryCount }, { locale: language.current })}</p>
      {#if existingPlaceCount > 0 || importedPlaceCount > 0}<p>{m.import_current_places({ count: existingPlaceCount }, { locale: language.current })}<br />{m.import_incoming_places({ count: importedPlaceCount }, { locale: language.current })}</p>{/if}
      <p>{m.import_replace_confirm({}, { locale: language.current })}</p>
      {#if existingPlaceCount > 0}<p>{m.import_replace_places({}, { locale: language.current })}</p>{/if}
      <p>{formatVersion >= 3 ? m.import_replace_startup_v3({}, { locale: language.current }) : m.import_replace_startup({}, { locale: language.current })}</p>
    {:else}
      <p role="status">{m.import_preview({ count: entryCount }, { locale: language.current })}</p>
      {#if importedPlaceCount > 0}<p>{m.import_incoming_places({ count: importedPlaceCount }, { locale: language.current })}</p>{/if}
      {#if importedSettingCount > 0}<p>{m.import_replace_valid({}, { locale: language.current })}</p>{/if}
      <p>{m.import_confirm({}, { locale: language.current })}</p>
    {/if}
    <button onclick={activate}>{hasExistingData() ? (formatVersion >= 3 ? m.import_replace_button_v3({}, { locale: language.current }) : m.import_replace_button({}, { locale: language.current })) : m.import_confirm_button({}, { locale: language.current })}</button>
    <button onclick={cancel}>{m.restore_cancel({}, { locale: language.current })}</button>
  {:else if phase === "checking" || phase === "cancelling" || phase === "writing"}
    <p role="status">{phase === "checking" ? m.import_checking({}, { locale: language.current }) : phase === "cancelling" ? m.import_cancelling({}, { locale: language.current }) : m.import_working({}, { locale: language.current })}</p>
  {:else if phase === "idle"}
    <button onclick={choose}>{m.import_choose({}, { locale: language.current })}</button>
  {/if}
  {#if notice}<p role="status">{m.import_cancelled({}, { locale: language.current })}</p>{/if}
  {#if error}<p role="alert">{errorMessage()}</p>{/if}
</main>

<style>
  main { max-width: 640px; margin: 64px auto; padding: 0 28px; color: #343530; line-height: 1.7; }
  .back-link { color: var(--accent, #435b47); font-family: -apple-system, BlinkMacSystemFont, sans-serif; font-size: 13px; text-decoration: none; }
  .back-link:hover { text-decoration: underline; }
  .back-link:focus-visible { outline: 2px solid #788e7b; outline-offset: 3px; }
  h1 { font-size: 24px; font-weight: 400; }
  button { background: transparent; color: #435b47; border: 1px solid #d9d8d1; padding: 10px 14px; margin: 8px 12px 8px 0; border-radius: 4px; cursor: pointer; }
</style>
