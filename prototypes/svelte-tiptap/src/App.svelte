<script lang="ts">
  import { onMount } from "svelte";
  import { Editor } from "@tiptap/core";
  import StarterKit from "@tiptap/starter-kit";

  let editorElement: HTMLDivElement;
  let editor: Editor | null = null;

  let isEditing = $state(false);
  let saveStatus = $state("Sparad");
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let activeFormats = $state({
    bold: false,
    italic: false,
    heading: false,
    bulletList: false,
  });

  const storageKey = "mno-tiptap-journal-test-v1";
  const initialContent = `
    <h2>En stilla morgon vid sjön</h2>
    <p>Dimman låg fortfarande över vattnet när jag kom fram till stranden.</p>
    <p>En rödhake sjöng från buskarna och längre bort hördes en spillkråka.
    Det var en sådan morgon då naturen kändes alldeles nära.</p>
  `;
  function updateActiveFormats() {
    if (!editor) return;

    activeFormats = {
      bold: editor.isActive("bold"),
      italic: editor.isActive("italic"),
      heading: editor.isActive("heading", { level: 2 }),
      bulletList: editor.isActive("bulletList"),
    };
  }
  onMount(() => {
    let savedContent = null;

    try {
      const stored = localStorage.getItem(storageKey);
      if (stored) {
        savedContent = JSON.parse(stored);
      }
    } catch (error) {
      console.error("Kunde inte läsa sparad anteckning:", error);
    }
    editor = new Editor({
      element: editorElement,
      extensions: [StarterKit],
      content: savedContent ?? initialContent,
      editable: false,
      onSelectionUpdate: updateActiveFormats,
      onTransaction: updateActiveFormats,
      onUpdate: () => {
        saveStatus = "Osparade ändringar";

        clearTimeout(saveTimer);
        saveTimer = setTimeout(() => {
          try {
            const document = editor?.getJSON();

            if (document) {
              localStorage.setItem(storageKey, JSON.stringify(document));

              saveStatus = "Sparad lokalt";
            }
          } catch (error) {
            console.error("Kunde inte spara anteckningen:", error);
            saveStatus = "Kunde inte spara";
          }
        }, 1200);
      },
    });

    return () => {
      clearTimeout(saveTimer);
      editor?.destroy();
    };
  });

  function startEditing() {
    isEditing = true;
    editor?.setEditable(true);
    editor?.commands.focus("end");
  }

  function finishEditing() {
    if (!editor) return;

    clearTimeout(saveTimer);

    try {
      localStorage.setItem(storageKey, JSON.stringify(editor.getJSON()));
      saveStatus = "Sparad lokalt";
    } catch (error) {
      console.error("Kunde inte spara anteckningen:", error);
      saveStatus = "Kunde inte spara";
    }

    isEditing = false;
    editor.setEditable(false);
  }
</script>

<main class="journal-page">
  <header class="journal-header">
    <div>
      <p class="eyebrow">MY NATURE OBSERVATIONS</p>
      <p class="date">12 oktober 2026</p>
    </div>

    {#if isEditing}
      <button onclick={finishEditing}>✓ Klar</button>
    {:else}
      <button onclick={startEditing}>Redigera</button>
    {/if}
  </header>

  {#if isEditing && editor}
    <nav class="editor-toolbar" aria-label="Textformatering">
      <button
        class:active={activeFormats.bold}
        aria-label="Fetstil"
        aria-pressed={editor.isActive("bold")}
        onclick={() => editor?.chain().focus().toggleBold().run()}
      >
        <strong>B</strong>
      </button>

      <button
        class:active={activeFormats.italic}
        aria-label="Kursiv stil"
        aria-pressed={editor.isActive("italic")}
        onclick={() => editor?.chain().focus().toggleItalic().run()}
      >
        <em>I</em>
      </button>

      <button
        class:active={activeFormats.heading}
        aria-label="Rubrik"
        aria-pressed={editor.isActive("heading", { level: 2 })}
        onclick={() =>
          editor?.chain().focus().toggleHeading({ level: 2 }).run()}
      >
        H2
      </button>

      <button
        class:active={activeFormats.bulletList}
        aria-label="Punktlista"
        aria-pressed={editor.isActive("bulletList")}
        onclick={() => editor?.chain().focus().toggleBulletList().run()}
      >
        ☷
      </button>

      <span class="toolbar-divider"></span>

      <button
        aria-label="Ångra"
        onclick={() => editor?.chain().focus().undo().run()}
      >
        ↶
      </button>

      <button
        aria-label="Gör om"
        onclick={() => editor?.chain().focus().redo().run()}
      >
        ↷
      </button>
    </nav>
  {/if}

  <article class="journal-entry">
    <div bind:this={editorElement}></div>
  </article>

  <footer class="journal-footer">
    <span>{saveStatus}</span>
  </footer>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #f8f7f3;
    color: #343530;
    font-family: Georgia, serif;
  }

  :global(button) {
    font-family: system-ui, sans-serif;
    cursor: pointer;
  }

  .journal-page {
    max-width: 760px;
    margin: 0 auto;
    padding: 72px 32px;
  }

  .journal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 56px;
  }

  .eyebrow {
    font-family: system-ui, sans-serif;
    font-size: 11px;
    letter-spacing: 0.16em;
    color: #788e7b;
  }

  .date {
    font-family: system-ui, sans-serif;
    font-size: 14px;
    color: #777970;
  }

  button {
    border: 1px solid #d9d8d1;
    border-radius: 8px;
    padding: 10px 16px;
    background: transparent;
    color: #343530;
  }

  .journal-entry {
    font-size: 19px;
    line-height: 1.5;
  }

  .journal-entry :global(h2) {
    font-size: 32px;
    font-weight: normal;
    line-height: 1.3;
    margin-bottom: 32px;
  }

  .journal-entry :global(p) {
    margin-bottom: 16px;
  }

  .journal-entry :global(.tiptap) {
    outline: none;
    min-height: 200px;
    border-radius: 6px;
    transition: background-color 0.2s ease;
  }

  .journal-entry :global(.tiptap[contenteditable="true"]) {
    padding: 10px;
    margin: -10px;
  }

  .journal-entry :global(.tiptap[contenteditable="true"]:focus) {
    background-color: rgba(120, 142, 123, 0.035);
  }

  .journal-footer {
    margin-top: 60px;
    font-family: system-ui, sans-serif;
    font-size: 12px;
    color: #788e7b;
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 24px;
    padding-bottom: 14px;
    border-bottom: 1px solid #d9d8d1;
  }

  .editor-toolbar button {
    min-width: 38px;
    min-height: 38px;
    padding: 6px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 14px;
  }

  .editor-toolbar button:hover {
    background: #eeede7;
  }

  .editor-toolbar button.active {
    background: #e1e7df;
    color: #435b47;
  }

  .toolbar-divider {
    width: 1px;
    height: 22px;
    margin: 0 8px;
    background: #d9d8d1;
  }

  .journal-entry :global(ul) {
    padding-left: 28px;
    margin: 16px 0 24px;
  }

  .journal-entry :global(li) {
    margin-bottom: 8px;
  }
</style>
