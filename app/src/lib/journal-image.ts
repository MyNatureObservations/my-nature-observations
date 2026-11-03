import { createPhotographControls } from "./photograph-controls";
import { Plugin, NodeSelection } from "@tiptap/pm/state";
import { Node } from "@tiptap/core";
import { createPhotographView, type PhotographOptions } from "./journal-image-view";

declare module "@tiptap/core" {
  interface Storage { journalImage: { controls: ReturnType<typeof createPhotographControls> | null } }
}

export const JournalImage = Node.create<PhotographOptions>({
  name: "journalImage",
  group: "block",
  atom: true,
  selectable: true,
  addOptions() {
    return { loadAsset: async () => { throw new Error("photograph_missing_asset"); }, labels: () => ({ photograph: "Photograph", loading: "Loading photograph…", unavailable: "Photograph unavailable", edit: "Photograph options", caption: "Caption", alt: "Alternative text", remove: "Remove from journal", done: "Apply", cancel: "Cancel", left: "Left", right: "Right", row: "Centered", small: "Small", medium: "Medium", large: "Large", placement: "Placement", size: "Size", changed: "This text changed. Cancel and reopen to edit the latest version.", tooLong: "Use at most 4,000 characters." }) };
  },
  addStorage() { return { controls: null as ReturnType<typeof createPhotographControls> | null }; },
  addProseMirrorPlugins() {
    return [new Plugin({ props: { handleKeyDown: (view, event) => {
      if (!this.editor.isEditable || !(event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))) return false;
      const selection = view.state.selection;
      if (!(selection instanceof NodeSelection) || selection.node.type.name !== "journalImage") return false;
      const nodeId = selection.node.attrs.nodeId;
      const element = view.nodeDOM(selection.from);
      if (!(element instanceof HTMLElement)) return false;
      this.storage.controls?.open({ nodeId, element, getPos: () => {
        let found: number | undefined;
        this.editor.state.doc.forEach((node, pos) => { if (node.type.name === "journalImage" && node.attrs.nodeId === nodeId) found = pos; });
        return found;
      } });
      event.preventDefault(); return true;
    } } })];
  },
  onCreate() { this.storage.controls = createPhotographControls(this.editor, this.options); },
  onDestroy() { this.storage.controls?.destroy(); },
  renderHTML() { return ["figure", { "data-journal-image": "" }]; },
  addNodeView() { return props => createPhotographView(props, { ...this.options, openControls: (target, point) => this.storage.controls?.open(target, point) }); },
  addAttributes() {
    return {
      nodeId: { default: null },
      assetId: { default: null },
      placement: { default: "row" },
      size: { default: "large" },
      caption: { default: undefined },
      alt: { default: undefined },
    };
  },
});
