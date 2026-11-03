import { photographs } from "./photo-test-modules.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { compile, parse } from "svelte/compiler";

// Execute the component's actual script, with editor, Tauri and timers replaced.
// Runes are synchronous here; the derived value is evaluated on every read.
const source = readFileSync(new URL("../src/lib/components/JournalEditor.svelte", import.meta.url), "utf8")
  .match(/<script lang="ts">([\s\S]*?)<\/script>/)[1]
  .replace(/^  import .*;\n/gm, "")
  .replace(/  let hasPendingChanges = \$derived\([\s\S]*?\n  \);/, "");
const script = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None },
}).outputText;

const readerSource = readFileSync(new URL("../src/lib/journal-data.ts", import.meta.url), "utf8")
  .replace(/^import .*;\n/gm, "").replace(/^export /gm, "");
const readerScript = ts.transpileModule(readerSource, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;

const emptySource = readFileSync(new URL("../src/lib/journal-empty.ts", import.meta.url), "utf8").replace(/^export /gm, "");
const emptyScript = ts.transpileModule(emptySource, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;

// Exercise Tauri's actual automatic-destroy fallback, not just our callback.
const windowApi = readFileSync(new URL('../node_modules/@tauri-apps/api/window.js', import.meta.url), 'utf8');
const closeEventClass = windowApi.match(/class CloseRequestedEvent \{[\s\S]*?\n\}/)[0];
const closeListenerBody = windowApi.match(/async onCloseRequested\(handler\) \{([\s\S]*?)\n    \}/)[1];
const registerClose = new Function('TauriEvent', `${closeEventClass}; return async function(handler) {${closeListenerBody}};`)({ WINDOW_CLOSE_REQUESTED: 'close' });

export async function harness({
  isNew = false,
  journalEntryId = null,
  entryDate = "2026-10-17",
  initialText = "A",
  initialDocument = null,
  loadResponse = null,
  startInEditMode = false,
  returnToJournal = false,
  manuallyStartEditing = true,
  editable = true,
  documentVersion = 1,
  initialJson = null,
  onGoto = async () => {},
  photoInvoke = null,
  realState = null,
  browserGlobals = {},
  messages = new Proxy({}, { get: () => () => "status" }),
} = {}) {
  let mount;
  let navigate;
  let closeRequested;
  let destroyed = 0;
  let reserved = false;
  let closeFailure = false;
  let closeCalls = 0;
  let cleanup;
  let update;
  let document = initialDocument ?? { text: initialText };
  let persistedContent = initialJson ?? JSON.stringify(document);
  const loadedDocuments = [];
  let editorOptions;
  let editingChanges = [];
  let nextTimer = 0;
  const timers = new Map();
  const writes = [];
  const context = vm.createContext({
    $props: () => ({ entryDate, isNew, journalEntryId, startInEditMode, returnToJournal, onEditingChange: value => editingChanges.push(value) }),
    goto: onGoto,
    $state: (value) => value,
    $effect: () => {},
    untrack: (callback) => callback(),
    tick: async () => {},
    onMount: (callback) => { mount = callback; },
    beforeNavigate: (callback) => { navigate = callback; },
    page: { state: {} },
    language: { current: "sv" },
    m: messages,
    ...browserGlobals,
    StarterKit: {},
    JournalImage: { configure: () => ({}) },
    crypto: globalThis.crypto,
    ...photographs,
    Event,
    window: { dispatchEvent: () => {} },
    getCurrentWindow: () => ({
      onCloseRequested: (callback) => registerClose.call({
        listen: async (_name, listener) => { closeRequested = listener; return () => {}; },
        destroy: async () => { destroyed++; },
      }, callback),
    }),
    setTimeout: (callback, delay) => {
      const id = ++nextTimer;
      timers.set(id, { callback, delay });
      return id;
    },
    clearTimeout: (id) => timers.delete(id),
    Editor: class {
      constructor(options) { update = options.onUpdate; editorOptions = options; this.state = realState; }
      view = { dispatch: transaction => { this.state = this.state.apply(transaction); document = this.state.doc.toJSON(); editorOptions.onTransaction({ transaction }); update({ editor: this }); } };
      commands = { setContent: (value, options) => { loadedDocuments.push({value, options}); document = value; }, focus: () => {} };
      isActive() { return false; }
      getJSON() { return document; }
      getText() { return document.text; }
      setEditable() {}
      destroy() {}
    },
    invoke: (command, args) => {
      if (photoInvoke) { const result = photoInvoke(command, args); if (result !== undefined) return result; }
      if (command === "close_journal_window") {
        closeCalls++;
        // Rust exercises the real shared-reservation decision separately.
        if (reserved || closeFailure) return Promise.reject(reserved ? "busy" : "failed");
        destroyed++;
        return Promise.resolve();
      }
      if (command === "new_journal_session") return Promise.resolve({ generation: 1, data: null });
      if (command === "load_journal_entry" || command === "get_journal_entry") {
        if (command === "get_journal_entry") assert.equal(args.id, journalEntryId);
        return loadResponse ?? Promise.resolve({ generation: 1, data: {
          id: journalEntryId ?? "entry", entryDate, contentJson: initialJson ?? JSON.stringify(document), editable, documentVersion,
        } });
      }
      assert.ok(["save_journal_entry", "save_journal_with_photographs"].includes(command));
      return new Promise((resolve, reject) => writes.push({ ...args, id: command === "save_journal_with_photographs" ? args.entryId : args.id, command, resolve, reject }));
    },
  });
  const run = (code) => vm.runInContext(code, context);
  Object.defineProperty(context, "hasPendingChanges", {
    get: () => run('generationConflict || hasUnsavedChanges || (savedEntryDate !== "" && entryDate !== savedEntryDate)'),
  });
  run(emptyScript);
  run(readerScript);
  run(script);
  cleanup = mount();
  const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
  await flush();
  if (manuallyStartEditing) run("startEditing()");
  return {
    writes, run, flush, loadedDocuments, editingChanges, editorOptions,
    edit(text) { document = { text }; update({ editor: run("editor") }); },
    editDocument(value) { document = value; update({ editor: run("editor") }); },
    async tick(delay = 2000) {
      const due = [...timers].filter(([, timer]) => timer.delay === delay);
      for (const [id, timer] of due) { timers.delete(id); timer.callback(); }
      await flush();
    },
    async succeed(index) { persistedContent = writes[index].contentJson; writes[index].resolve({ id: writes[index].id ?? "entry" }); await flush(); },
    persisted: () => persistedContent,
    destroyed: () => destroyed,
    pending: () => run("hasPendingChanges"),
    text: () => document.text,
    retryDelays: () => [...timers.values()].map((timer) => timer.delay),
    reserve(value = true) { reserved = value; },
    failClose(value = true) { closeFailure = value; },
    closeCalls: () => closeCalls,
    async canClose() {
      const before = destroyed;
      await closeRequested({ event: "close", id: 1 });
      return destroyed > before;
    },
    canLeave(destination = "https://example.invalid/journal") {
      let cancelled = false;
      navigate({ cancel: () => { cancelled = true; }, to: { url: new URL(destination) } });
      return !cancelled;
    },
    cleanup: () => cleanup(),
  };
}
