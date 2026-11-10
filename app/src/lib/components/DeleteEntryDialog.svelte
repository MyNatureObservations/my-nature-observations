<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";

  let { title, date, locale, busy, oncancel, onconfirm }: {
    title: string; date: string; locale: "sv" | "en"; busy: boolean;
    oncancel: () => void; onconfirm: () => void;
  } = $props();
  const id = $props.id();
  const titleId = `${id}-title`;
  const descriptionId = `${id}-description`;

  function mountDialog(dialog: HTMLDialogElement) {
    const previous = document.activeElement;
    dialog.showModal();
    dialog.querySelector<HTMLButtonElement>("button")?.focus();
    return { destroy() {
      dialog.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus({ preventScroll: true });
    } };
  }
</script>

<dialog
  class="delete-dialog"
  use:mountDialog
  oncancel={(event) => { event.preventDefault(); if (!busy) oncancel(); }}
  aria-labelledby={titleId}
  aria-describedby={descriptionId}
>
  <h2 id={titleId}>{m.journal_delete_entry({}, { locale })}</h2>
  <p id={descriptionId}>
    {m.journal_delete_confirm({
      title: title || m.search_untitled({}, { locale }),
      date,
    }, { locale })}
  </p>
  <div class="delete-dialog-actions">
    <button type="button" disabled={busy} onclick={oncancel}>
      {m.journal_cancel({}, { locale })}
    </button>
    <button type="button" disabled={busy} onclick={onconfirm}>
      {m.journal_delete_entry({}, { locale })}
    </button>
  </div>
</dialog>

<style>
  .delete-dialog::backdrop {
    background: rgba(0, 0, 0, 0.35);
  }

  .delete-dialog {
    box-sizing: border-box;
    width: min(calc(100vw - 48px), 420px);
    max-height: calc(100dvh - 48px);
    margin: auto;
    padding: 28px;
    border: 0;
    border-radius: 14px;
    background: var(--surface, #faf9f6);
    color: var(--text, #252b27);
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.18);
  }

  .delete-dialog h2 {
    margin: 0 0 12px;
    font-size: 1.2rem;
  }

  .delete-dialog p {
    margin: 0;
    line-height: 1.6;
  }

  .delete-dialog-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 24px;
  }

  .delete-dialog-actions button {
    padding: 10px 14px;
    border: 1px solid #d9d8d1;
    background: transparent;
    color: inherit;
    font-family: system-ui, sans-serif;
    border-radius: 8px;
    cursor: pointer;
  }

  .delete-dialog-actions button:disabled { opacity: 0.6; }
</style>
