import { Selection } from "@tiptap/pm/state";
import type { NodeViewRendererProps } from "@tiptap/core";
import { photographSource, type PhotographAsset } from "./journal-photographs";

import type { PhotographLabels, PhotographTarget, ControlOptions } from "./photograph-controls";
export type { PhotographLabels } from "./photograph-controls";
export type PhotographOptions = { loadAsset: (id: string) => Promise<PhotographAsset>; labels: () => PhotographLabels; openControls?: (target: PhotographTarget, point?: { x: number; y: number }) => void } & ControlOptions;

export function createPhotographView({ node: initialNode, editor, getPos }: NodeViewRendererProps, options: PhotographOptions) {
  let node = initialNode;
  let destroyed = false, request = 0, assetId = "";
  const dom = document.createElement("figure");
  dom.className = "mno-photograph";
  dom.contentEditable = "false";
  dom.tabIndex = 0;
  const image = document.createElement("img");
  image.hidden = true;
  const status = document.createElement("span");
  status.setAttribute("role", "status");
  const caption = document.createElement("figcaption");
  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "mno-photo-options";
  edit.setAttribute("aria-haspopup", "menu");
  dom.append(image, status, caption, edit);
  function open(point?: { x: number; y: number }) {
    if (editor.isEditable) options.openControls?.({ nodeId: node.attrs.nodeId, getPos, element: dom }, point);
  }
  function refresh() {
    const labels = options.labels();
    dom.setAttribute("aria-label", labels.photograph);
    image.alt = node.attrs.alt ?? "";
    caption.textContent = node.attrs.caption ?? ""; caption.hidden = !caption.textContent;
    // Keep the pointer target stable during focus/selection transactions.
    if (edit.textContent !== labels.edit) edit.textContent = labels.edit;
    edit.hidden = !editor.isEditable;
    dom.tabIndex = editor.isEditable ? 0 : -1;
    dom.dataset.placement = node.attrs.placement;
    dom.dataset.size = node.attrs.size;
    if (!status.hidden) status.textContent = status.dataset.failed ? labels.unavailable : labels.loading;
    if (assetId !== node.attrs.assetId) {
      assetId = node.attrs.assetId;
      dom.style.removeProperty("--photo-natural-width");
      image.hidden = true; image.removeAttribute("src"); status.hidden = false; delete status.dataset.failed;
      const current = ++request;
      void options.loadAsset(assetId).then(asset => {
        if (destroyed || current !== request) return;
        dom.style.setProperty("--photo-natural-width", `${asset.width}px`);
        image.src = photographSource(asset); image.width = asset.width; image.height = asset.height;
        image.hidden = false; status.hidden = true;
      }).catch(() => { if (!destroyed && current === request) { status.dataset.failed = "true"; status.textContent = options.labels().unavailable; } });
    }
  }
  edit.onclick = () => open();
  dom.oncontextmenu = event => {
    if (!editor.isEditable) return;
    event.preventDefault(); event.stopPropagation(); open({ x: event.clientX, y: event.clientY });
  };
  dom.onkeydown = event => {
    if (editor.isEditable && event.target === dom && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      const pos = getPos();
      if (typeof pos === "number") {
        const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
        const selection = Selection.near(editor.state.doc.resolve(forward ? pos + node.nodeSize : pos), forward ? 1 : -1);
        event.preventDefault(); event.stopPropagation();
        editor.view.dispatch(editor.state.tr.setSelection(selection).setMeta("addToHistory", false));
        editor.view.focus();
      }
      return;
    }
    if (editor.isEditable && (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))) {
      event.preventDefault(); event.stopPropagation(); open();
    }
  };
  dom.onfocus = () => { const pos = getPos(); if (editor.isEditable && typeof pos === "number") editor.commands.setNodeSelection(pos); };
  editor.on("update", refresh); editor.on("transaction", refresh);
  refresh();
  return {
    dom,
    update(next: typeof node) { if (next.type !== node.type) return false; node = next; refresh(); return true; },
    selectNode() { dom.classList.add("ProseMirror-selectednode"); },
    deselectNode() { dom.classList.remove("ProseMirror-selectednode"); },
    stopEvent(event: Event) { return event.target instanceof Element && !!event.target.closest("button, input, textarea, label"); },
    ignoreMutation() { return true; },
    destroy() { destroyed = true; request++; editor.off("update", refresh); editor.off("transaction", refresh); image.removeAttribute("src"); },
  };
}
