import { getSchema } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { JournalImage } from "$lib/journal-image";
import type { PhotographAttrs } from "$lib/journal-photographs";
import { isDefinitelyEmptyJournalJson } from "$lib/journal-empty";

export type ReadingEntry = {
  id: string;
  entryDate: string;
  title: string;
  contentJson: string;
  documentVersion?: number;
  editable?: boolean;
};

export type JournalMark = { type: string; attrs?: { href?: string } };
export type JournalNode = {
  type: string;
  text?: string;
  attrs?: { level?: number; start?: number } & Partial<PhotographAttrs>;
  marks?: JournalMark[];
  content?: JournalNode[];
};

// Match the existing editor's schema without creating an Editor or plugins.
const schema = getSchema([StarterKit]);
const schemaV2 = getSchema([StarterKit, JournalImage]);
const nodes = new Set([
  "doc", "paragraph", "heading", "bulletList", "orderedList", "listItem",
  "blockquote", "codeBlock", "hardBreak", "horizontalRule", "text",
]);
const marks = new Set(["bold", "italic", "strike", "underline", "code", "link"]);

export function readJournalDocument(contentJson: string, documentVersion = 1):
  | { document: JournalNode; error: null }
  | { document: null; error: string } {
  try {
    const document = JSON.parse(contentJson) as JournalNode;
    if (document?.type !== "doc") throw new Error("Invalid journal document");
    if (![1, 2].includes(documentVersion)) throw new Error("Unsupported journal version");
    (documentVersion === 2 ? schemaV2 : schema).nodeFromJSON(document).check();
    const check = (node: JournalNode, parent = ""): void => {
      if (node.type === "journalImage") {
        const attrs = node.attrs;
        const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
        if (documentVersion !== 2 || parent !== "doc" || !attrs || !uuid.test(attrs.nodeId ?? "") || !uuid.test(attrs.assetId ?? "") || !["left", "right", "row"].includes(attrs.placement ?? "") || !["small", "medium", "large"].includes(attrs.size ?? "") || [attrs.caption, attrs.alt].some(value => value !== undefined && typeof value !== "string")) throw new Error("Invalid photograph");
        return;
      }
      if (!nodes.has(node.type) || node.marks?.some(mark => !marks.has(mark.type))) {
        throw new Error("Unsupported journal content");
      }
      if (node.type === "heading" &&
        (!Number.isInteger(node.attrs?.level) || node.attrs!.level! < 1 || node.attrs!.level! > 6)) {
        throw new Error("Invalid heading level");
      }
      if (node.type === "orderedList" && node.attrs?.start !== undefined &&
        (!Number.isInteger(node.attrs.start) || node.attrs.start < 1)) {
        throw new Error("Invalid list start");
      }
      node.content?.forEach(child => check(child, node.type));
    };
    check(document);
    // Return the parsed input, never a normalized document for saving.
    return { document, error: null };
  } catch (error) {
    return { document: null, error: error instanceof Error ? error.message : String(error) };
  }
}

export function safeJournalHref(href: unknown): string | null {
  if (typeof href !== "string") return null;
  try {
    const url = new URL(href);
    return ["https:", "http:", "mailto:", "tel:"].includes(url.protocol) ? href : null;
  } catch {
    return null;
  }
}

export function visibleJournalEntries(entries: ReadingEntry[]): ReadingEntry[] {
  return entries.filter(entry => !isDefinitelyEmptyJournalJson(entry.contentJson));
}

export function groupJournalEntries(entries: ReadingEntry[]) {
  const groups: { date: string; entries: ReadingEntry[] }[] = [];
  // The API supplies date/creation/ID order; preserve its same-day ordering.
  for (const entry of entries) {
    let group = groups[groups.length - 1];
    if (!group || group.date !== entry.entryDate) {
      group = { date: entry.entryDate, entries: [] };
      groups.push(group);
    }
    group.entries.push(entry);
  }
  // Reverse only the new group array, never the entries within each day.
  return groups.reverse();
}

export function journalEntryAnchor(id: string): string {
  return `journal-entry-${id}`;
}

export function formatJournalDate(date: string, locale: string): string {
  const parsed = new Date(`${date}T12:00:00Z`);
  if (Number.isNaN(parsed.getTime())) return date;
  return new Intl.DateTimeFormat(locale === "sv" ? "sv-SE" : "en-GB", {
    day: "numeric", month: "long", year: "numeric", timeZone: "UTC",
  }).format(parsed);
}


export function journalDateAnchor(date: string): string {
  return `journal-date-${date}`;
}

// Explicit entry wins over date; latest is only a fallback selector.
export function resolveJournalTarget(entries: ReadingEntry[], params: URLSearchParams):
  { anchor: string | null; missing: "entry" | "date" | "latest" | null } {
  const visible = visibleJournalEntries(entries);
  if (params.has("entry")) {
    const entry = visible.find(entry => entry.id === params.get("entry"));
    return { anchor: entry ? journalEntryAnchor(entry.id) : null, missing: entry ? null : "entry" };
  }
  if (params.has("date")) {
    const date = params.get("date")!;
    const parsed = new Date(`${date}T12:00:00Z`);
    const valid = /^\d{4}-\d{2}-\d{2}$/.test(date) &&
      !Number.isNaN(parsed.getTime()) && parsed.toISOString().slice(0, 10) === date;
    const found = valid && visible.some(entry => entry.entryDate === date);
    return { anchor: found ? journalDateAnchor(date) : null, missing: found ? null : "date" };
  }
  if (params.get("latest") === "true") {
    const newestDay = groupJournalEntries(visible)[0];
    // Open the newest experience date from its heading and first visible entry.
    // Selecting the last-created article would skip earlier text within that day.
    return { anchor: newestDay ? journalDateAnchor(newestDay.date) : null, missing: newestDay ? null : "latest" };
  }
  return { anchor: null, missing: null };
}
