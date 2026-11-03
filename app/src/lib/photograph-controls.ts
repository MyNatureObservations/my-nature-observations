import type { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";
import { closeHistory } from "@tiptap/pm/history";

export type PhotographLabels = Record<"photograph" | "loading" | "unavailable" | "edit" | "caption" | "alt" | "remove" | "done" | "cancel" | "left" | "right" | "row" | "small" | "medium" | "large" | "placement" | "size" | "changed" | "tooLong", string>;
export type PhotographTarget = { nodeId: string; getPos: () => number | undefined; element: HTMLElement };
export type ControlOptions = {
  labels: () => PhotographLabels;
  onDraft?: (pending: boolean, focus?: () => void) => void;
  session?: () => unknown;
  canEdit?: () => boolean;
};

// One overlay host per editor. No overlay participates in the journal's flow.
export function createPhotographControls(editor: Editor, options: ControlOptions) {
  let target: PhotographTarget | null = null;
  let session: unknown;
  let overlay: HTMLElement | null = null;
  let field: "caption" | "alt" | null = null;
  let original: unknown;
  let input: HTMLTextAreaElement | null = null;
  let error: HTMLElement | null = null;
  function resolve() {
    if (!target || editor.isDestroyed || !editor.isEditable || options.canEdit?.() === false || session !== options.session?.()) return null;
    let pos: number | undefined;
    try { pos = target.getPos(); } catch { return null; }
    const node = typeof pos === "number" ? editor.state.doc.nodeAt(pos) : null;
    return node?.type.name === "journalImage" && node.attrs.nodeId === target.nodeId ? { pos: pos!, node } : null;
  }
  function close(restore = true) {
    const previous = target;
    if (overlay instanceof HTMLDialogElement && overlay.open) overlay.close();
    overlay?.remove(); overlay = null; field = null; input = null; error = null;
    target = null; options.onDraft?.(false);
    if (restore && previous?.element.isConnected && editor.isEditable) previous.element.focus();
  }
  function commit(key: string, value: string) {
    const current = resolve();
    if (!current) { close(false); return; }
    if (key === "size" && value === "large" && current.node.attrs.placement !== "row") { close(); return; }
    const attrs = { ...current.node.attrs, [key]: value };
    if (key === "placement" && value !== "row" && attrs.size === "large") attrs.size = "medium";
    const after = current.pos + current.node.nodeSize;
    const following = editor.state.doc.nodeAt(after);
    // Insertion already leaves a paragraph here. Resume writing, not atom replacement.
    const resumeWriting = (key === "placement" || key === "size") && following?.type.name === "paragraph" && following.content.size === 0;
    if (current.node.attrs[key] !== value || current.node.attrs.size !== attrs.size) {
      const transaction = closeHistory(editor.state.tr.setNodeMarkup(current.pos, undefined, attrs));
      if (resumeWriting) transaction.setSelection(TextSelection.create(transaction.doc, after + 1));
      editor.view.dispatch(transaction);
      // Separate following prose edits from this deliberate property action too.
      editor.view.dispatch(closeHistory(editor.state.tr).setMeta("addToHistory", false));
    } else if (resumeWriting) {
      editor.view.dispatch(editor.state.tr.setSelection(TextSelection.create(editor.state.doc, after + 1)).setMeta("addToHistory", false));
    }
    close(!resumeWriting);
    if (resumeWriting) editor.view.focus();
  }
  function button(text: string, action: () => void) {
    const element = document.createElement("button"); element.type = "button"; element.textContent = text; element.onclick = action;
    return element;
  }
  function edit(key: "caption" | "alt") {
    const current = resolve(), savedTarget = target;
    if (!current || !savedTarget) { close(false); return; }
    overlay?.remove();
    const labels = options.labels();
    field = key; original = current.node.attrs[key];
    const dialog = document.createElement("dialog"); dialog.className = "mno-photo-dialog";
    overlay = dialog;
    const label = document.createElement("label"), name = document.createElement("span");
    name.textContent = labels[key];
    input = document.createElement("textarea"); input.rows = key === "caption" ? 4 : 3; input.maxLength = 4000; input.value = String(original ?? "");
    label.append(name, input);
    dialog.setAttribute("aria-label", labels[key]);
    error = document.createElement("p"); error.setAttribute("role", "alert"); error.hidden = true;
    const actions = document.createElement("div"); actions.className = "mno-photo-dialog-actions";
    actions.append(button(labels.cancel, () => close()), button(labels.done, () => {
      const latest = resolve();
      if (!latest || !input || !error) { close(false); return; }
      if (latest.node.attrs[key] !== original) { error.textContent = options.labels().changed; error.hidden = false; return; }
      if (input.value.length > 4000 || new TextEncoder().encode(input.value).length > 16384) { error.textContent = options.labels().tooLong; error.hidden = false; return; }
      // An untouched absent field stays absent.
      if (input.value === String(original ?? "")) { close(); return; }
      commit(key, input.value);
    }));
    dialog.append(label, error, actions);
    dialog.oncancel = event => { event.preventDefault(); close(); };
    document.body.append(dialog); dialog.showModal(); input.focus();
    options.onDraft?.(true, () => input?.focus());
  }
  function open(next: PhotographTarget, point?: { x: number; y: number }) {
    // A modal draft must be resolved before selecting another photograph.
    if (field) { input?.focus(); return; }
    close(false); target = next; session = options.session?.();
    const current = resolve(); if (!current) { close(false); return; }
    editor.commands.setNodeSelection(current.pos);
    const labels = options.labels(), menu = document.createElement("div");
    menu.className = "mno-photo-menu"; menu.setAttribute("role", "menu"); menu.setAttribute("aria-label", labels.edit);
    overlay = menu;
    const items: HTMLButtonElement[] = [];
    function item(text: string, action: () => void, checked?: boolean) {
      const element = button(text, action); element.tabIndex = -1;
      element.setAttribute("role", checked === undefined ? "menuitem" : "menuitemradio");
      if (checked !== undefined) element.setAttribute("aria-checked", String(checked));
      items.push(element); return element;
    }
    menu.append(item(labels.caption, () => edit("caption")), item(labels.alt, () => edit("alt")));
    const sizes = current.node.attrs.placement === "row" ? ["small", "medium", "large"] as const : ["small", "medium"] as const;
    const selectedSize = current.node.attrs.placement !== "row" && current.node.attrs.size === "large" ? "medium" : current.node.attrs.size;
    for (const [key, values] of [["placement", ["left", "right", "row"]], ["size", sizes]] as const) {
      const group = document.createElement("div"); group.setAttribute("role", "group"); group.setAttribute("aria-label", labels[key]);
      for (const value of values) group.append(item(labels[value], () => commit(key, value), (key === "size" ? selectedSize : current.node.attrs.placement) === value));
      menu.append(group);
    }
    menu.append(item(labels.remove, () => {
      const latest = resolve();
      if (latest) {
        editor.view.dispatch(closeHistory(editor.state.tr.delete(latest.pos, latest.pos + latest.node.nodeSize)));
        editor.view.dispatch(closeHistory(editor.state.tr).setMeta("addToHistory", false));
      }
      close(false); editor.commands.focus();
    }));
    menu.onkeydown = event => {
      let index = items.indexOf(document.activeElement as HTMLButtonElement);
      if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault();
        index = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
        items[index].focus();
      } else if (event.key === "Escape" || event.key === "Tab") { event.preventDefault(); close(); }
    };
    document.body.append(menu);
    const anchor = next.element.getBoundingClientRect();
    const rect = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(point?.x ?? anchor.left, window.innerWidth - rect.width - 8))}px`;
    menu.style.top = `${Math.max(8, Math.min(point?.y ?? anchor.top, window.innerHeight - rect.height - 8))}px`;
    items[0].focus();
  }
  function outside(event: PointerEvent) { if (!field && overlay && !overlay.contains(event.target as Node)) close(); }
  function validate() { if (target && !resolve()) close(false); }
  document.addEventListener("pointerdown", outside);
  editor.on("transaction", validate);
  editor.on("update", validate);
  return { open, close, destroy() { close(false); document.removeEventListener("pointerdown", outside); editor.off("transaction", validate); editor.off("update", validate); } };
}
