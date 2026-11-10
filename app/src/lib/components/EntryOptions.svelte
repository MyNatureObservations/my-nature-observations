<script lang="ts">
  import type { Snippet } from "svelte";
  import * as m from "$lib/paraglide/messages.js";

  let { locale, children }: { locale: "sv" | "en"; children: Snippet<[() => void]> } = $props();
  let open = $state(false);
  let trigger: HTMLButtonElement;
  const panelId = $props.id();

  function toggle() {
    trigger.focus();
    open = !open;
  }

  function closeAndFocus() {
    open = false;
    trigger.focus();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape" || !(event.target instanceof Node)
      || !trigger.parentElement?.contains(event.target)) return;
    event.preventDefault();
    event.stopPropagation();
    closeAndFocus();
  }

  function handleFocusout(event: FocusEvent) {
    if (event.relatedTarget instanceof Node && !(event.currentTarget as HTMLElement).contains(event.relatedTarget)) {
      open = false;
    }
  }

  function handleOutsidePointer(event: PointerEvent) {
    if (open && event.target instanceof Node && !trigger.parentElement?.contains(event.target)) {
      open = false;
    }
  }

  $effect(() => {
    if (!open) return;
    document.addEventListener("pointerdown", handleOutsidePointer);
    document.addEventListener("keydown", handleKeydown, true);
    return () => {
      document.removeEventListener("pointerdown", handleOutsidePointer);
      document.removeEventListener("keydown", handleKeydown, true);
    };
  });
</script>

<div class="entry-options" onfocusout={handleFocusout}>
  <button
    type="button"
    class="trigger"
    bind:this={trigger}
    aria-label={m.journal_entry_options({}, { locale })}
    aria-expanded={open}
    aria-controls={panelId}
    onclick={toggle}
  >…</button>
  <div id={panelId} class="actions" hidden={!open}>
    {@render children(closeAndFocus)}
  </div>
</div>

<style>
  .entry-options { position: relative; display: flex; justify-content: flex-end; }
  .trigger {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--accent, #435b47);
    font: 20px/1 system-ui, sans-serif;
    cursor: pointer;
  }
  .trigger:hover { background: #eef1eb; }
  .trigger:focus-visible, .actions :global(:focus-visible) { outline: 2px solid var(--accent, #435b47); outline-offset: 2px; }
  .actions {
    position: absolute;
    z-index: 2;
    right: 0;
    top: 100%;
    min-width: max-content;
    padding: 4px;
    border: 1px solid #d9d8d1;
    border-radius: 8px;
    background: var(--surface, #faf9f6);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  }
  .actions :global(a), .actions :global(button) {
    display: block;
    width: 100%;
    padding: 10px 12px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--accent, #435b47);
    font: 13px/1.4 system-ui, sans-serif;
    text-align: left;
    text-decoration: none;
    cursor: pointer;
  }
  .actions :global(a:hover), .actions :global(button:hover) { background: #eef1eb; }
</style>
