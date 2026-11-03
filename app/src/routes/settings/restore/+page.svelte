<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { beforeNavigate } from "$app/navigation";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";
  let token = $state<string | null>(null);
  let phase = $state<"idle" | "checking" | "confirmed" | "writing" | "awaiting_exit" | "failed">("idle");
  let error = $state("");
  beforeNavigate((navigation) => { if (phase === "writing" || phase === "awaiting_exit") navigation.cancel(); });
  function canReturnHome() {
    return phase === "idle" || phase === "checking" || phase === "confirmed" ||
      (phase === "failed" && error === "invalid");
  }
  async function choose() {
    if (phase === "checking" || phase === "writing" || phase === "awaiting_exit") return;
    token = null; error = ""; phase = "checking";
    try {
      const selection = await invoke<{ token: string } | null>("choose_restore_backup", { dialogTitle: m.restore_open({}, { locale: language.current }) });
      token = selection?.token ?? null;
      phase = token ? "confirmed" : "idle";
    } catch { phase = "failed"; error = "invalid"; }
  }
  async function restore() {
    if (phase !== "confirmed" || !token) return;
    phase = "writing";
    try {
      await invoke("restore_selected_backup", { token });
      token = null;
      phase = "awaiting_exit";
    } catch { phase = "failed"; error = "failed"; token = null; }
  }
  async function quit() {
    if (phase !== "awaiting_exit") return;
    try { await invoke("quit_after_restore"); }
    catch { error = "quit_failed"; }
  }
</script>
<main>
  {#if canReturnHome()}
    <a class="back-link" href="/">← {m.restore_back_to_home({}, { locale: language.current })}</a>
  {/if}
  <h1>{m.restore_open({}, { locale: language.current })}</h1>
  <p>{m.restore_intro({}, { locale: language.current })}</p>
  {#if phase === "confirmed"}
    <p>{m.restore_confirm({}, { locale: language.current })}</p>
    <button onclick={restore}>{m.restore_confirm_button({}, { locale: language.current })}</button>
    <button onclick={() => { token = null; phase = "idle"; }}>{m.restore_cancel({}, { locale: language.current })}</button>
  {:else if phase === "awaiting_exit"}
    <p role="status">{m.restore_reopen_instructions({}, { locale: language.current })}</p>
    <button onclick={quit}>{m.restore_quit({}, { locale: language.current })}</button>
  {:else if phase === "writing" || phase === "checking"}
    <p role="status">{phase === "writing" ? m.restore_working({}, { locale: language.current }) : m.restore_checking({}, { locale: language.current })}</p>
  {:else}
    <button onclick={choose}>{m.restore_choose({}, { locale: language.current })}</button>
  {/if}
  {#if error}<p role="alert">{error === "invalid" ? m.restore_invalid({}, { locale: language.current }) : error === "quit_failed" ? m.restore_quit_failed({}, { locale: language.current }) : m.restore_failed({}, { locale: language.current })}</p>{/if}
</main>
<style>
  main { max-width: 640px; margin: 64px auto; padding: 0 28px; color: #343530; line-height: 1.7; }
  .back-link { color: var(--accent, #435b47); font-family: -apple-system, BlinkMacSystemFont, sans-serif; font-size: 13px; text-decoration: none; }
  .back-link:hover { text-decoration: underline; }
  .back-link:focus-visible { outline: 2px solid #788e7b; outline-offset: 3px; }
  h1 { font-size: 24px; font-weight: 400; }
  button { background: transparent; color: #435b47; border: 1px solid #d9d8d1; padding: 10px 14px; margin: 8px 12px 8px 0; border-radius: 4px; cursor: pointer; }
</style>
