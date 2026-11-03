<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import icon from "../../../src-tauri/icons/128x128.png";
  import licenseText from "../../../../LICENSE?raw";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  let version = $state<string | null>(null);
  let versionFailed = $state(false);
  let opening = $state(false);
  let error = $state<"notices" | "repository" | null>(null);
  let disposed = false;
  const options = () => ({ locale: language.current });
  onMount(() => { void loadVersion(); });
  onDestroy(() => { disposed = true; });
  async function loadVersion() {
    try { const value = await getVersion(); if (!disposed) version = value; }
    catch { if (!disposed) versionFailed = true; }
  }
  async function openDocument(kind: "notices" | "repository") {
    if (opening || disposed) return;
    opening = true; error = null;
    try {
      if (kind === "notices") {
        await invoke("open_third_party_notices");
      } else await openUrl("https://github.com/MyNatureObservations/my-nature-observations");
    } catch { if (!disposed) error = kind; }
    finally { if (!disposed) opening = false; }
  }
</script>

<div class="about-mno">
  <img src={icon} alt="" width="64" height="64" />
  <h3>{m.about_app_name({}, options())}</h3>
  <p class="version" role="status">{version !== null ? m.about_version({ version }, options()) : versionFailed ? m.about_version_unavailable({}, options()) : m.about_version_loading({}, options())}</p>
  <p>{m.about_mit_description({}, options())}</p>
  <details class="license-text">
    <summary>{m.about_project_license({}, options())}</summary>
    <pre>{licenseText}</pre>
  </details>
  <div class="about-actions">
    <a href="https://github.com/MyNatureObservations/my-nature-observations" aria-disabled={opening} onclick={(event) => { event.preventDefault(); void openDocument("repository"); }}>{m.about_repository({}, options())}</a>
    <button type="button" disabled={opening} onclick={() => openDocument("notices")}>{m.about_third_party({}, options())}</button>
  </div>
  <p class="hint">{m.about_offline_hint({}, options())}</p>
  {#if error}<p class="error" role="alert">{error === "notices" ? m.about_notices_failed({}, options()) : m.about_repository_failed({}, options())}</p>{/if}
</div>

<style>
  .about-mno { margin-top: 12px; color: var(--text, #343530); font: 13px/1.6 system-ui,sans-serif; }
  img { display: block; border-radius: 12px; margin-bottom: 12px; }
  h3 { font-size: 14px; font-weight: 500; margin: 0; }
  p { margin: 8px 0; }
  .version, .hint { color: var(--text-muted, #686d63); font-size: 12px; }
  summary, button, a { cursor: pointer; color: var(--accent, #435b47); font: inherit; }
  summary { padding: 8px 0; min-height: 24px; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; font: 12px/1.6 system-ui,sans-serif; margin: 8px 0 16px; }
  .about-actions { display: grid; gap: 8px; margin-top: 8px; }
  a, button { box-sizing: border-box; text-decoration: none; text-align: left; padding: 8px 10px; min-height: 40px; border: 1px solid var(--border, #d9d8d1); border-radius: 6px; background: transparent; }
  :is(a, button):hover:not(:disabled):not([aria-disabled="true"]) { background: var(--surface, #eeeee8); }
  button:disabled, a[aria-disabled="true"] { opacity: .5; cursor: default; }
  :is(a, button, summary):focus-visible { outline: 2px solid var(--accent, #435b47); outline-offset: 2px; border-radius: 4px; }
  .error { color: #9a403b; }
</style>
