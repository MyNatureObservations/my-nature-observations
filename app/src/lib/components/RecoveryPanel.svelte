<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";
  type Action = "finish_new" | "keep_previous" | "continue";
  let { mode, prepared = false, onfinished }: { mode: string; prepared?: boolean; onfinished: (restored: boolean, operation: "import" | "restore" | null) => Promise<void> } = $props();
  type Replacement = { target: "new" | "old"; existing_entry_count: number; imported_entry_count: number; previous_confirmation: string | null };
  let replacement = $state<Replacement | null>(null);
  let confirmingPrevious = $state(false);
  let actions = $state<Action[]>([]);
  let operation = $state<"import" | "restore" | null>(null);
  let busy = $state(false);
  let failed = $state(false);
  let loading = $state(false);
  let quitFailed = $state(false);
  let disposed = false;
  async function refresh() {
    if (disposed || busy || loading || mode !== "recovery") return;
    loading = true; actions = []; confirmingPrevious = false;
    try {
      const view = await invoke<{ actions: Action[]; operation?: string; replacement?: Replacement }>("recovery_status");
      if (!disposed) {
        operation = view.operation === "import" || view.operation === "restore" ? view.operation : null;
        replacement = view.replacement ?? null;
        actions = view.actions;
        failed = false;
      }
    } catch { if (!disposed) failed = true; }
    finally { if (!disposed) loading = false; }
  }
  onMount(() => {
    if (mode === "recovery") void refresh();
    return () => { disposed = true; };
  });
  function requestAction(action: Action) {
    if (disposed || busy || loading || !actions.includes(action)) return;
    if (replacement && action === "keep_previous") confirmingPrevious = true;
    else void run(action);
  }
  async function run(action: Action) {
    if (disposed || busy || loading || !actions.includes(action)) return;
    const previous = action === "keep_previous" || replacement?.target === "old";
    const key = replacement?.previous_confirmation;
    if (replacement && action === "keep_previous" && (!confirmingPrevious || !key)) return;
    busy = true; failed = false;
    confirmingPrevious = false;
    try {
      await invoke("run_recovery", replacement && action === "keep_previous"
        ? { action, previousConfirmation: key } : { action });
      // Successful completion is authoritative even if its Ready event unmounted
      // this panel before the command reply. Never infer completion from status.
      await onfinished(!previous, operation);
    } catch {
      if (!disposed) {
        failed = true;
        if (replacement) actions = []; // A retry needs fresh authoritative status.
      }
    } finally { if (!disposed) busy = false; }
  }
  function actionLabel(action: Action) {
    const options = { locale: language.current };
    if (replacement) return action === "keep_previous" ? m.replacement_choose_previous({}, options)
      : replacement.target === "old" ? m.replacement_continue_previous({}, options) : m.replacement_continue_imported({}, options);
    return action === "finish_new" ? m.restore_finish({}, options) : action === "continue" ? m.restore_continue({}, options) : m.restore_keep({}, options);
  }
  let backupToken = $state<string | null>(null);
  let windowsPhase = $state<"idle" | "checking" | "confirmed" | "publishing" | "pending">("idle");
  let windowsError = $state<"invalid" | "unavailable" | "publication" | null>(null);

  function windowsFailure(error: unknown) {
    windowsError = error === "invalid_backup" || error === "changed" ? "invalid"
      : error === "unsupported" || error === "busy" ? "unavailable" : "publication";
    windowsPhase = "idle";
  }

  async function chooseBackup() {
    if (mode !== "windows_recovery" || busy || windowsPhase !== "idle") return;
    busy = true;
    windowsError = null;
    windowsPhase = "checking";
    try {
      const selection = await invoke<{ token: string } | null>("choose_recovery_backup", {
        dialogTitle: m.restore_open({}, { locale: language.current })
      });
      if (!disposed) {
        backupToken = selection?.token ?? null;
        windowsPhase = backupToken ? "confirmed" : "idle";
      }
    } catch (error) {
      if (!disposed) windowsFailure(error);
    } finally { if (!disposed) busy = false; }
  }

  async function confirmBackup() {
    if (mode !== "windows_recovery" || busy || windowsPhase !== "confirmed" || !backupToken) return;
    const token = backupToken;
    backupToken = null;
    busy = true;
    windowsPhase = "publishing";
    try {
      await invoke("confirm_recovery_backup", { token });
      if (!disposed) windowsPhase = "pending";
    } catch (error) {
      if (!disposed) windowsFailure(error);
    } finally { if (!disposed) busy = false; }
  }

  async function quit() {
    if (busy) return;
    busy = true; quitFailed = false;
    try { await invoke("quit_after_restore"); } catch { quitFailed = true; busy = false; }
  }
</script>
<main aria-live="polite">
  <h1>{replacement ? m.replacement_recovery_title({}, { locale: language.current }) : m.restore_recovery_title({}, { locale: language.current })}</h1>
  {#if !(mode === "blocked" && prepared) && (!replacement || failed || loading) && mode !== "windows_recovery" && mode !== "windows_restore_pending" && mode !== "windows_restore_uncertain"}<p>{m.database_recovery_blocked({}, { locale: language.current })}</p>{/if}
  {#if mode === "windows_restore_pending" || mode === "windows_restore_uncertain"}
    {#if mode === "windows_restore_pending"}
      <p role="status">{m.windows_restore_pending({}, { locale: language.current })}</p>
    {:else}
      <p role="alert">{m.windows_restore_uncertain({}, { locale: language.current })}</p>
    {/if}
    <button disabled={busy} onclick={quit}>{m.restore_quit({}, { locale: language.current })}</button>
  {/if}
  {#if mode === "windows_recovery"}
    {#if windowsPhase === "pending"}
      <p role="status">{m.restore_reopen_instructions({}, { locale: language.current })}</p>
    {:else if windowsPhase === "confirmed"}
      <p>{m.windows_recovery_confirm({}, { locale: language.current })}</p>
      <button onclick={confirmBackup}>{m.windows_recovery_confirm_button({}, { locale: language.current })}</button>
      <button onclick={() => { backupToken = null; windowsPhase = "idle"; }}>{m.restore_cancel({}, { locale: language.current })}</button>
    {:else if windowsPhase === "checking" || windowsPhase === "publishing"}
      <p role="status">{windowsPhase === "checking" ? m.restore_checking({}, { locale: language.current }) : m.restore_working({}, { locale: language.current })}</p>
    {:else}
      <p>{m.database_recovery_blocked({}, { locale: language.current })}</p>
      <button disabled={busy} onclick={chooseBackup}>{m.restore_choose({}, { locale: language.current })}</button>
    {/if}
    {#if windowsError}
      <p role="alert">{windowsError === "invalid" ? m.windows_recovery_invalid({}, { locale: language.current })
        : windowsError === "unavailable" ? m.windows_recovery_unavailable({}, { locale: language.current })
        : m.windows_recovery_failed({}, { locale: language.current })}</p>
    {/if}
    <button disabled={busy} onclick={quit}>{m.restore_quit({}, { locale: language.current })}</button>
  {/if}
  {#if loading}<p>{m.restore_checking({}, { locale: language.current })}</p>{/if}
  {#if replacement && !failed && !loading}
    <p>{m.replacement_previous_count({ count: replacement.existing_entry_count }, { locale: language.current })}<br />{m.import_incoming_count({ count: replacement.imported_entry_count }, { locale: language.current })}</p>
    <p>{replacement.target === "old" ? m.replacement_target_previous({}, { locale: language.current }) : m.replacement_target_imported({}, { locale: language.current })}</p>
    <p>{m.replacement_safety({}, { locale: language.current })}</p>
  {/if}
  {#if confirmingPrevious && replacement}
    <section aria-label={m.replacement_choose_previous({}, { locale: language.current })}>
      <p>{m.replacement_previous_confirm({ oldCount: replacement.existing_entry_count, newCount: replacement.imported_entry_count }, { locale: language.current })}</p>
      <button disabled={busy || !replacement.previous_confirmation} onclick={() => run("keep_previous")}>{m.replacement_confirm_previous({}, { locale: language.current })}</button>
      <button disabled={busy} onclick={() => { confirmingPrevious = false; }}>{m.restore_cancel({}, { locale: language.current })}</button>
    </section>
  {:else}
    {#each actions as action}
      <button disabled={busy} onclick={() => requestAction(action)}>{actionLabel(action)}</button>
    {/each}
  {/if}
  {#if mode === "blocked"}
    {#if prepared}
      <p role="status">{m.import_prepared({}, { locale: language.current })}</p>
      <p>{m.import_prepared_reopen({}, { locale: language.current })}</p>
    {:else}
      <p>{m.restore_reopen_instructions({}, { locale: language.current })}</p>
    {/if}
    <button disabled={busy} onclick={quit}>{m.restore_quit({}, { locale: language.current })}</button>
  {/if}
  {#if quitFailed}<p>{m.restore_quit_failed({}, { locale: language.current })}</p>{/if}
  {#if failed && replacement}
    <p role="alert">{m.replacement_action_failed({}, { locale: language.current })}</p>
    <button disabled={busy || loading} onclick={refresh}>{m.replacement_refresh({}, { locale: language.current })}</button>
  {/if}
  {#if (failed && !replacement) || (mode === "recovery" && !loading && !failed && actions.length === 0)}<p>{m.restore_recovery_help({}, { locale: language.current })}</p>{/if}
</main>
<style>
  main { max-width: 640px; margin: 64px auto; padding: 0 28px; overflow-wrap: anywhere; color: #343530; line-height: 1.7; }
  section { margin: 20px 0; }
  h1 { font-size: 24px; font-weight: 400; }
  button { background: transparent; color: #435b47; border: 1px solid #d9d8d1; padding: 10px 14px; margin: 8px 12px 8px 0; border-radius: 4px; cursor: pointer; }
</style>
