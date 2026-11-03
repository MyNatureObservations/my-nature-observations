<script lang="ts">
  import JournalPhotograph from "$lib/components/JournalPhotograph.svelte";
  import type { PhotographAttrs } from "$lib/journal-photographs";
  import { readJournalDocument, safeJournalHref, type JournalNode, type JournalMark } from "$lib/journal-reading";
  import * as m from "$lib/paraglide/messages.js";

  let { contentJson, locale, editable = true, entryId = "", generation = null, documentVersion = 1 }: { contentJson: string; locale: "sv" | "en"; editable?: boolean; entryId?: string; generation?: number | null; documentVersion?: number } = $props();
  const result = $derived(readJournalDocument(contentJson, documentVersion));
</script>

{#snippet markedText(text: string, marks: JournalMark[], index: number)}
  {#if index === marks.length}
    {text}
  {:else}
    {@const mark = marks[index]}
    {#if mark.type === "bold"}
      <strong>{@render markedText(text, marks, index + 1)}</strong>
    {:else if mark.type === "italic"}
      <em>{@render markedText(text, marks, index + 1)}</em>
    {:else if mark.type === "strike"}
      <s>{@render markedText(text, marks, index + 1)}</s>
    {:else if mark.type === "underline"}
      <u>{@render markedText(text, marks, index + 1)}</u>
    {:else if mark.type === "code"}
      <code>{@render markedText(text, marks, index + 1)}</code>
    {:else if mark.type === "link"}
      {@const href = safeJournalHref(mark.attrs?.href)}
      {#if href}
        <a {href} target="_blank" rel="noopener noreferrer">{@render markedText(text, marks, index + 1)}</a>
      {:else}
        {@render markedText(text, marks, index + 1)}
      {/if}
    {/if}
  {/if}
{/snippet}

{#snippet blocks(content: JournalNode[])}
  {#each content as node}
    {#if node.type === "journalImage"}
      <JournalPhotograph attrs={node.attrs as PhotographAttrs} {entryId} {generation} {locale} />
    {:else if node.type === "text"}
      {@render markedText(node.text ?? "", node.marks ?? [], 0)}
    {:else if node.type === "paragraph"}
      <p>{@render blocks(node.content ?? [])}</p>
    {:else if node.type === "heading"}
      <svelte:element this={`h${node.attrs!.level}`}>{@render blocks(node.content ?? [])}</svelte:element>
    {:else if node.type === "bulletList"}
      <ul>{@render blocks(node.content ?? [])}</ul>
    {:else if node.type === "orderedList"}
      <ol start={node.attrs?.start ?? 1}>{@render blocks(node.content ?? [])}</ol>
    {:else if node.type === "listItem"}
      <li>{@render blocks(node.content ?? [])}</li>
    {:else if node.type === "blockquote"}
      <blockquote>{@render blocks(node.content ?? [])}</blockquote>
    {:else if node.type === "codeBlock"}
      <pre><code>{@render blocks(node.content ?? [])}</code></pre>
    {:else if node.type === "hardBreak"}
      <br />
    {:else if node.type === "horizontalRule"}
      <hr />
    {/if}
  {/each}
{/snippet}

<div class="journal-text">
  {#if !editable}<p class="content-error" role="status">{m.journal_document_incomplete({}, { locale })}</p>{/if}
  {#if result.document}
    {@render blocks(result.document.content ?? [])}
  {:else}
    <p class="content-error" role="alert">{m.journal_entry_load_error({ error: result.error }, { locale })}</p>
  {/if}
</div>

<style>
  .content-error { color: #a33b35; font-size: 15px; }
</style>
