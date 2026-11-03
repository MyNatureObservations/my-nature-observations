import { closeHistory } from "@tiptap/pm/history";
import { NodeSelection, TextSelection, type EditorState, type Transaction } from "@tiptap/pm/state";

export type PhotographAsset = {
  asset_id: string; entry_id: string; original_path: string;
  mime: "image/jpeg" | "image/png"; width: number; height: number; preview: string;
};
export type PhotographAttrs = {
  nodeId: string; assetId: string; placement: "left" | "right" | "row";
  size: "small" | "medium" | "large"; caption?: string; alt?: string;
};
export type PhotoPosition = { pos: number; splitParagraph: boolean; valid: boolean };
export function capturePhotoPosition(state: EditorState): PhotoPosition {
  const $pos = state.doc.resolve(state.selection.from);
  if ($pos.depth === 1 && $pos.parent.type.name === "paragraph") {
    return { pos: $pos.pos, splitParagraph: true, valid: true };
  }
  return { pos: $pos.depth ? $pos.after(1) : state.selection.to, splitParagraph: false, valid: true };
}
export function mapPhotoPosition(position: PhotoPosition, transaction: Transaction): PhotoPosition {
  const mapped = transaction.mapping.mapResult(position.pos, 1);
  const $pos = transaction.doc.resolve(mapped.pos);
  const splitParagraph = position.splitParagraph && $pos.depth === 1 && $pos.parent.type.name === "paragraph";
  return { pos: !splitParagraph && $pos.depth ? $pos.after(1) : mapped.pos, splitParagraph, valid: position.valid && !mapped.deletedAcross };
}
export function insertPhotograph(transaction: Transaction, position: PhotoPosition, attrs: PhotographAttrs): Transaction {
  if (!position.valid) throw new Error("photograph_position_lost");
  closeHistory(transaction);
  const image = transaction.doc.type.schema.nodes.journalImage.create(attrs);
  if (position.splitParagraph) {
    transaction.setSelection(TextSelection.create(transaction.doc, position.pos));
    transaction.replaceSelectionWith(image, false);
  } else {
    transaction.insert(position.pos, image);
    transaction.setSelection(NodeSelection.create(transaction.doc, position.pos));
  }
  // Leave a text cursor after the image, including at the end of an entry.
  let imagePos = -1;
  transaction.doc.forEach((node, offset) => { if (node.type.name === "journalImage" && node.attrs.nodeId === attrs.nodeId) imagePos = offset; });
  if (imagePos < 0) throw new Error("photograph_position_lost");
  const after = imagePos + image.nodeSize;
  if (transaction.doc.nodeAt(after)?.type.name !== "paragraph") transaction.insert(after, transaction.doc.type.schema.nodes.paragraph.create());
  return transaction.setSelection(TextSelection.create(transaction.doc, after + 1)).scrollIntoView();
}
export function photographSource(asset: PhotographAsset): string {
  if (!["image/jpeg", "image/png"].includes(asset.mime)) throw new Error("photograph_format");
  return `data:${asset.mime};base64,${asset.preview}`;
}
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export async function readPhotograph(invoke: Invoke, entryId: string, generation: number, assetId: string): Promise<PhotographAsset> {
  const asset = await invoke<PhotographAsset | null>("get_journal_photograph", { entryId, generation, assetId });
  const current = await invoke<{ generation: number }>("new_journal_session");
  if (current.generation !== generation) throw new Error("journal_generation_conflict");
  if (!asset || asset.entry_id !== entryId || asset.asset_id !== assetId) throw new Error("photograph_missing_asset");
  return asset;
}
