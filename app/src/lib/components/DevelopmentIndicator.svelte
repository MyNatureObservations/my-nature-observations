<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  let development = $state(false);
  onMount(() => {
    let disposed = false;
    void invoke<boolean>("development_mode").then(value => { if (!disposed) development = value; }).catch(() => {});
    return () => { disposed = true; };
  });
</script>

{#if development}<span class="dev-indicator">DEV</span>{/if}

<style>
  .dev-indicator { position: fixed; right: 12px; bottom: 8px; z-index: 1000; padding: 2px 7px; border: 1px solid #d9d8d1; border-radius: 5px; background: #faf9f6; color: #435b47; font: 11px/1.5 system-ui, sans-serif; letter-spacing: .08em; pointer-events: none; }
</style>
