<script lang="ts">
  import BirdJournalLists from "$lib/components/BirdJournalLists.svelte";
  import { onMount, untrack, tick, type Snippet } from "svelte";
  import DeleteEntryDialog from "$lib/components/DeleteEntryDialog.svelte";
  import EntryOptions from "$lib/components/EntryOptions.svelte";
  import EntryWeather from "$lib/components/EntryWeather.svelte";
  import { Editor } from "@tiptap/core";
  import StarterKit from "@tiptap/starter-kit";
  import { JournalImage } from "$lib/journal-image";
  import { capturePhotoPosition, mapPhotoPosition, insertPhotograph, readPhotograph, type PhotoPosition, type PhotographAsset } from "$lib/journal-photographs";
  import { createJournalReader } from "$lib/journal-data";
  import { isDefinitelyEmptyJournalDocument } from "$lib/journal-empty";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { beforeNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  let {
    entryDate,
    isNew = false,
    journalEntryId = null,
    startInEditMode = false,
    returnToJournal = false,
    onEditingChange,
    placeDirty = false,
    placeBusy = false,
    onSessionChange,
    metadata,
  }: {
    metadata?: Snippet<[Snippet, Snippet | undefined]>;
    entryDate: string;
    isNew?: boolean;
    journalEntryId?: string | null;
    startInEditMode?: boolean;
    returnToJournal?: boolean;
    onEditingChange?: (editing: boolean) => void;
    placeDirty?: boolean;
    placeBusy?: boolean;
    onSessionChange?: (value: { entryId: string | null; generation: number | null; editing: boolean }) => void;
  } = $props();

  type JournalEntry = {
    id: string;
    entryDate: string;
    title: string;
    contentJson: string;
    documentVersion: number;
    editable: boolean;
  };

  let weatherControl = $state<EntryWeather>();
  async function ensureWeatherEntry(){if(!await saveEntry())throw "journal_save_failed";if(!entryId)throw "weather_new_entry_needs_text";await tick();}
  const readJournal = createJournalReader();
  let sessionGeneration = $state<number | null>(null);
  let generationConflict = $state(false);
  let documentProtected = $state(false);
  let documentVersion = 1;
  let draftEntryId: string | null = null;
  let photoBusy = $state(false);
  let photoDraft = $state(false);
  let focusPhotoDraft: (() => void) | undefined;
  let photoError = $state("");
  let photoEpoch = 0;
  let photoPosition: PhotoPosition | null = null;
  const preparedPhotographs = new Map<string, PhotographAsset>();
  const storedPhotographs = new Set<string>();

  function photographLabels() {
    const locale = language.current;
    return { photograph: m.photo_photograph({}, { locale }), loading: m.photo_loading({}, { locale }), unavailable: m.photo_unavailable({}, { locale }), edit: m.photo_edit({}, { locale }), caption: m.photo_caption({}, { locale }), alt: m.photo_alt({}, { locale }), remove: m.photo_remove({}, { locale }), done: m.photo_done({}, { locale }), cancel: m.photo_cancel({}, { locale }), left: m.photo_left({}, { locale }), right: m.photo_right({}, { locale }), row: m.photo_row({}, { locale }), small: m.photo_small({}, { locale }), medium: m.photo_medium({}, { locale }), large: m.photo_large({}, { locale }), placement: m.photo_placement({}, { locale }), size: m.photo_size({}, { locale }), changed: m.photo_changed({}, { locale }), tooLong: m.photo_too_long({}, { locale }) };
  }
  async function loadPhotograph(assetId: string): Promise<PhotographAsset> {
    const prepared = preparedPhotographs.get(assetId);
    if (prepared) return prepared;
    if (!entryId || sessionGeneration === null) throw new Error("photograph_missing_asset");
    return readPhotograph(invoke, entryId, sessionGeneration, assetId);
  }
  async function insertPhoto() {
    if (!editor || !isEditing || photoBusy || documentProtected || generationConflict || sessionGeneration === null || isLeavingWithoutSaving) return;
    const targetEditor = editor, generation = sessionGeneration, epoch = ++photoEpoch;
    const current = () => editor === targetEditor && photoEpoch === epoch && sessionGeneration === generation && !isLeavingWithoutSaving && isEditing;
    photoPosition = capturePhotoPosition(targetEditor.state);
    photoBusy = true; photoError = "";
    try {
      // Finish an earlier first save before assigning a stable owner to the asset.
      if (currentSave) await currentSave;
      if (!current()) return;
      const owner = entryId ?? (draftEntryId ??= crypto.randomUUID());
      const path = await invoke<string | null>("select_journal_photograph");
      if (!path || !current()) return;
      const asset = await invoke<PhotographAsset>("prepare_journal_photograph", { generation, path, entryId: owner });
      if (!current()) return;
      const latest = await invoke<{ generation: number }>("new_journal_session");
      if (!current()) return;
      if (latest.generation !== generation) throw "journal_generation_conflict";
      if ((entryId ?? draftEntryId) !== owner || asset.entry_id !== owner) return;
      if (!photoPosition?.valid) { photoError = m.photo_position_lost({}, { locale: language.current }); return; }
      const transaction = insertPhotograph(targetEditor.state.tr, photoPosition, { nodeId: crypto.randomUUID(), assetId: asset.asset_id, placement: "row", size: "large" });
      photoPosition = null;
      preparedPhotographs.set(asset.asset_id, asset);
      documentVersion = 2;
      targetEditor.view.dispatch(transaction);
      targetEditor.commands.focus();
    } catch (error) {
      if (current()) {
        if (String(error) === "journal_generation_conflict") { generationConflict = true; clearTimeout(autosaveTimer); }
        photoError = m.photo_failed({}, { locale: language.current });
      }
    } finally {
      if (photoEpoch === epoch) { photoBusy = false; photoPosition = null; }
    }
  }


  let editorElement: HTMLDivElement;
  let editor: Editor | null = $state(null);
  let isEditing = $state(false);
  let entryId: string | null = $state(null);
  let isSaving = $state(false);
  let weatherDirty = $state(false), weatherBusy = $state(false), weatherWarning = $state(false);
  let placeWarning = $state(false);
  let currentSave: Promise<boolean> | null = null;
  // Prevent new saves after the user chooses to discard changes.
  let isLeavingWithoutSaving = false;
  // Wait for a short writing pause before saving automatically.
  const AUTOSAVE_DELAY_MS = 2000;
  let autosaveTimer: ReturnType<typeof setTimeout> | undefined;
  let hasUnsavedChanges = $state(false);
  let showLeaveConfirmation = $state(false);
  let pendingClose = $state(false);
  let pendingNavigationUrl = $state<string | null>(null);
  let allowDiscardNavigation = false;
  let savedContentJson = "";
  let savedTitle = $state("");
  let showDeleteConfirmation = $state(false);
  let deletingEntry = $state(false);
  let entryDeleted = $state(false);
  let deleteFailed = $state(false);
  let savedEntryDate = $state("");
  let hasPendingChanges = $derived(
    generationConflict || hasUnsavedChanges ||
      (savedEntryDate !== "" && entryDate !== savedEntryDate),
  );

  function mountLeaveDialog(dialog: HTMLDialogElement) {
    const previous = document.activeElement;
    dialog.showModal();
    dialog.querySelector<HTMLButtonElement>("button")?.focus();
    return { destroy() {
      dialog.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus({ preventScroll: true });
    } };
  }

  function canDeleteEntry() {
    return !!journalEntryId && !!entryId && sessionGeneration !== null
      && !isLoading && !loadError && !isEditing && !entryDeleted
      && !hasPendingChanges && !currentSave && !isSaving && !photoDraft
      && !photoBusy && !placeDirty && !placeBusy && !weatherDirty && !weatherBusy;
  }

  async function deleteSavedEntry() {
    if (!showDeleteConfirmation || deletingEntry || !canDeleteEntry()) return;
    deletingEntry = true;
    deleteFailed = false;
    try {
      await invoke("delete_journal_entry", { id: entryId, generation: sessionGeneration });
      entryDeleted = true;
    } catch {
      deleteFailed = true;
    } finally {
      deletingEntry = false;
      showDeleteConfirmation = false;
    }
    if (entryDeleted) {
      window.dispatchEvent(new Event("mno:journal-changed"));
      await goto("/journal?latest=true");
    }
  }

  function cancelLeaveConfirmation() {
    pendingClose = false;
    showLeaveConfirmation = false;
  }

  function cancelLeaveDialog(event: Event) {
    event.preventDefault();
    // An already confirmed discard may be waiting for a write to finish.
    if (!isLeavingWithoutSaving) cancelLeaveConfirmation();
  }

  function leaveDialogKeys(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === "Escape") {
      cancelLeaveDialog(event);
    } else if (event.key === "Tab") {
      const buttons = Array.from((event.currentTarget as HTMLDialogElement).querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      event.preventDefault();
      const next = index < 0 ? (event.shiftKey ? buttons.length - 1 : 0)
        : (index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
  }

  beforeNavigate((navigation) => {
    if (deletingEntry) { navigation.cancel(); return; }
    if (photoDraft) { navigation.cancel(); focusPhotoDraft?.(); return; }
    // Do not leave while changes are unsaved or a database write is running.
    if ((!hasPendingChanges && !isSaving && !weatherDirty && !weatherBusy && !placeDirty && !placeBusy && !photoBusy) || allowDiscardNavigation) return;

    navigation.cancel();
    pendingNavigationUrl = navigation.to?.url.href ?? null;
    showLeaveConfirmation = true;
  });

  let saveMessage = $state("");
  let saveError = $state(false);
  let isRetrying = $state(false);
  let saveMessageTimer: ReturnType<typeof setTimeout> | undefined;
  let isSaveMessageFading = $state(false);
  let isLoading = $state(true);
  let loadError = $state("");

  let activeFormats = $state({
    bold: false,
    italic: false,
    heading: false,
    bulletList: false,
  });

  function generateTitle(): string {
    if (!editor) return "";

    const document = editor.getJSON();
    const blocks = document.content ?? [];

    function extractText(node: any): string {
      const ownText = node.text ?? "";
      const childText = (node.content ?? []).map(extractText).join("");

      return ownText + childText;
    }

// Use the first non-empty H2 heading anywhere in the entry.
for (const block of blocks) {
  if (block.type === "heading" && block.attrs?.level === 2) {
    const heading = extractText(block).trim();

    if (heading) return heading;
  }
}

    // Otherwise, derive the title from the first text in the entry.
    const text = blocks.map(extractText).join(" ").replace(/\s+/g, " ").trim();

    if (!text) return "";

    if (text.length <= 60) return text;

    const shortened = text.slice(0, 61);
    const lastSpace = shortened.lastIndexOf(" ");

    return (
      (lastSpace > 0 ? shortened.slice(0, lastSpace) : text.slice(0, 60)) + "…"
    );
  }

  function updateActiveFormats() {
    if (!editor) return;

    activeFormats = {
      bold: editor.isActive("bold"),
      italic: editor.isActive("italic"),
      heading: editor.isActive("heading", { level: 2 }),
      bulletList: editor.isActive("bulletList"),
    };
  }

  function updateUnsavedChanges(currentEditor: Editor) {
    const document = currentEditor.getJSON();
    hasUnsavedChanges = (!entryId && isDefinitelyEmptyJournalDocument(document))
      ? false
      : JSON.stringify(document) !== savedContentJson;
  }

  onMount(() => {
    let disposed = false;
    let unlistenClose: (() => void) | null = null;

    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        // Tauri otherwise calls destroy() after this listener, bypassing Rust's
        // CloseRequested guard. Only the reserved backend path may close us.
        event.preventDefault();
        if (deletingEntry) return;
        if (photoDraft) { focusPhotoDraft?.(); return; }
        if (hasPendingChanges || isSaving || weatherDirty || weatherBusy || placeDirty || placeBusy || photoBusy) {
          pendingClose = true;
          showLeaveConfirmation = true;
        } else {
          await closeJournalWindow();
        }
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          unlistenClose = unlisten;
        }
      });

    editor = new Editor({
      element: editorElement,
      editorProps: { attributes: () => ({
        "aria-label": m.journal_editor_label({}, { locale: language.current }),
        "aria-multiline": "true",
      }) },
      extensions: [StarterKit, JournalImage.configure({ loadAsset: loadPhotograph, labels: photographLabels, onDraft: (pending, focus) => { photoDraft = pending; focusPhotoDraft = focus; }, session: () => sessionGeneration, canEdit: () => isEditing && !generationConflict && !documentProtected && !isLeavingWithoutSaving })],
      content: "",
      editable: false,
      enableContentCheck: true,
      onSelectionUpdate: updateActiveFormats,
      onTransaction: ({ transaction }) => {
        if (photoPosition) photoPosition = mapPhotoPosition(photoPosition, transaction);
        updateActiveFormats();
      },
      onUpdate: ({ editor }) => {
        if (isEditing) {
          updateUnsavedChanges(editor);
          // Always cancel a previous timer, including when a new entry becomes empty.
          scheduleAutosave();
        }
      },
    });

    async function loadEntry() {
      try {
        const snapshot = isNew
          ? await readJournal<JournalEntry | null>("new_journal_session")
          : journalEntryId
          ? await readJournal<JournalEntry | null>("get_journal_entry", {
              id: journalEntryId,
            })
          : await readJournal<JournalEntry | null>("load_journal_entry", {
              entryDate,
            });

        if (disposed || !editor) return;
        const entry = snapshot.data;
        sessionGeneration = snapshot.generation;

        if (journalEntryId && !entry) {
          throw new Error(
            m.journal_entry_not_found({}, { locale: language.current }),
          );
        }

        if (isNew) {
          isEditing = true;
          editor.setEditable(true);
          editor.commands.focus("end");
        }

        if (entry) {
          if (![1, 2].includes(entry.documentVersion) || entry.editable !== true) {
            documentProtected = true;
            loadError = m.journal_document_protected({}, { locale: language.current });
            return;
          }
          documentVersion = entry.documentVersion;
          entryId = entry.id;
          const document = JSON.parse(entry.contentJson);

          editor.commands.setContent(document, { errorOnInvalidContent: true, emitUpdate: false });
          entryId = entry.id;
          savedContentJson = JSON.stringify(editor.getJSON());
          savedEntryDate = entry.entryDate;
          savedTitle = entry.title;
          isLoading = false;
          if (startInEditMode) startEditing();
        }
      } catch (error) {
        if (!disposed) {
          loadError = String(error) === "journal_generation_conflict"
            ? m.journal_generation_conflict({}, { locale: language.current })
            : m.journal_load_error(
                { error: String(error) },
                { locale: language.current },
              );
        }
      } finally {
        if (!disposed) {
          isLoading = false;
        }
      }
    }

    void loadEntry();
    if (!isNew && page.state.justSaved === true) {
      showSaveConfirmation();
    }

    return () => {
      disposed = true;
      photoEpoch++; photoPosition = null; photoBusy = false;
      preparedPhotographs.clear(); storedPhotographs.clear();
      unlistenClose?.();

      // Prevent a delayed autosave from running after the editor is destroyed.
      clearTimeout(autosaveTimer);
      clearTimeout(saveMessageTimer);

      editor?.destroy();
      editor = null;
    };
  });

  $effect(() => {
    onSessionChange?.({ entryId, generation: sessionGeneration, editing: isEditing });
    if (!placeDirty) placeWarning = false;
  });

  $effect(() => {
    const locale = language.current;
    const current = editor;
    if (locale && current) untrack(() => current.view.dispatch(current.state.tr.setMeta("addToHistory", false)));
  });

  function showSaveConfirmation() {
    clearTimeout(saveMessageTimer);

    saveError = false;
    isSaveMessageFading = false;
    saveMessage = m.journal_saved({}, { locale: language.current });

    saveMessageTimer = setTimeout(() => {
      isSaveMessageFading = true;

      saveMessageTimer = setTimeout(() => {
        saveMessage = "";
        isSaveMessageFading = false;
      }, 500);
    }, 3000);
  }

  function showAutosaveConfirmation() {
    clearTimeout(saveMessageTimer);

    // Never confirm a save while newer changes are still pending.
    if (hasPendingChanges) return;

    saveError = false;
    isSaveMessageFading = false;
    saveMessage = m.journal_autosaved({}, { locale: language.current });

    // Keep the confirmation visible briefly, then fade it away.
    saveMessageTimer = setTimeout(() => {
      isSaveMessageFading = true;

      saveMessageTimer = setTimeout(() => {
        saveMessage = "";
        isSaveMessageFading = false;
      }, 500);
    }, 3000);
  }

  function startEditing() {
    if (!editor || isLoading || loadError || documentProtected || deletingEntry || entryDeleted) return;
    saveMessage = "";
    saveError = false;

    isEditing = true;
    onEditingChange?.(true);
    editor?.setEditable(true);
    editor?.commands.focus("end");
  }

  $effect(() => {
    const currentDate = entryDate;
    const lastSavedDate = savedEntryDate;

    // Only autosave date changes on an existing journal entry.
    if (!editor || !isEditing || !entryId) return;

    if (currentDate !== lastSavedDate) {
      // Avoid tracking reactive state read by the scheduling function.
      untrack(() => scheduleAutosave());
    }
  });

  function scheduleAutosave() {
    clearTimeout(autosaveTimer);

    // Hide an outdated success message when new changes are pending.
    if (hasPendingChanges && !saveError) {
      clearTimeout(saveMessageTimer);
      saveMessage = "";
      isSaveMessageFading = false;
    }

    if (documentProtected || generationConflict || isLeavingWithoutSaving || !isEditing || !hasPendingChanges) return;
    // Restart the timer whenever the user makes another change.
    autosaveTimer = setTimeout(() => {
      void saveEntry();
    }, AUTOSAVE_DELAY_MS);
  }

  async function saveEntry(): Promise<boolean> {
    if (!editor || documentProtected || generationConflict || sessionGeneration === null || isLeavingWithoutSaving) return false;

    // Reserve a place in the queue before waiting for earlier saves.
    // This prevents concurrent writes and duplicate IDs for new entries.
    const previousSave = currentSave;

    const save = (async (): Promise<boolean> => {
      if (previousSave) {
        await previousSave;
      }

      if (!editor || documentProtected || generationConflict || sessionGeneration === null || isLeavingWithoutSaving) return false;

      // Check AFTER the queue wait: a previous first write may have returned an ID.
      // True means finishing is safe, not that an empty entry was written.
      if (!entryId && isDefinitelyEmptyJournalDocument(editor.getJSON())) {
        updateUnsavedChanges(editor);
        return true;
      }

      // Skip the database write when everything is already saved.
      if (entryId && !hasPendingChanges) {
        return true;
      }

      return performSave();
    })();

    currentSave = save;

    try {
      return await save;
    } finally {
      if (currentSave === save) {
        currentSave = null;
      }
    }
  }

  async function performSave(): Promise<boolean> {
    if (!editor || documentProtected || sessionGeneration === null) return false;
    const generationToSave = sessionGeneration;

    // Capture the exact content and date being saved.
    // The user may continue editing while the database is writing.
    const contentJson = JSON.stringify(editor.getJSON());
    const dateToSave = entryDate;
    const titleToSave = generateTitle();
    const versionToSave = documentVersion;
    const ownerToSave = entryId ?? draftEntryId ?? (versionToSave === 2 ? crypto.randomUUID() : null);
    const assetsToSave = [...preparedPhotographs.values()].filter(asset => !storedPhotographs.has(asset.asset_id));

    // Three attempts in total, with increasing delays.
    const retryDelays = [0, 2000, 5000];

    isSaving = true;
    isRetrying = false;
    clearTimeout(saveMessageTimer);
    saveMessage = "";
    isSaveMessageFading = false;

    try {
      for (let attempt = 0; attempt < retryDelays.length; attempt++) {
        // Stop retrying if the user has chosen to discard changes.
        if (!editor || documentProtected || generationConflict || isLeavingWithoutSaving) return false;

        if (retryDelays[attempt] > 0) {
          await new Promise<void>((resolve) =>
            setTimeout(resolve, retryDelays[attempt]),
          );
        }

        // The user may have chosen to leave during the delay.
        if (!editor || documentProtected || generationConflict || isLeavingWithoutSaving) return false;

        try {
          const result = versionToSave === 2
            ? await invoke("save_journal_with_photographs", { entryId: ownerToSave, entryDate: dateToSave, title: titleToSave, contentJson, assets: assetsToSave, generation: generationToSave, createEntry: entryId === null }).then(() => ({ id: ownerToSave! }))
            : await invoke<{ id: string }>("save_journal_entry", {
                id: entryId, newEntryId: entryId ? null : draftEntryId, entryDate: dateToSave, title: titleToSave, contentJson, generation: generationToSave,
              });
          if (versionToSave === 2) assetsToSave.forEach(asset => storedPhotographs.add(asset.asset_id));

          entryId = result.id;
          savedContentJson = contentJson;
          savedEntryDate = dateToSave;
          savedTitle = titleToSave;

          window.dispatchEvent(new Event("mno:journal-changed"));

          // New edits must remain marked as unsaved.
          const currentEditor = editor;

          if (currentEditor !== null) {
            updateUnsavedChanges(currentEditor);
          }

          // Keep an existing warning until the current changes are saved too.
          saveError = saveError && hasPendingChanges;

          // A change back to the previously saved content can cancel its timer
          // while this write is running. Ensure those changes are saved too.
          if (hasPendingChanges) {
            scheduleAutosave();
          }

          // Only confirm autosaving when the latest changes are saved.
          if (!hasPendingChanges && !isLeavingWithoutSaving) {
            showAutosaveConfirmation();
          }

          return true;
        } catch (error) {
          if (String(error) === "journal_document_incompatible") {
            documentProtected = true;
            editor?.setEditable(false);
            if (!weatherDirty && !weatherBusy) { isEditing = false; onEditingChange?.(false); }
            saveError = true;
            clearTimeout(autosaveTimer);
            return false;
          }
          if (String(error) === "journal_generation_conflict") {
            generationConflict = true;
            saveError = true;
            clearTimeout(autosaveTimer);
            return false;
          }
          // Only show an error after all three attempts have failed.
          if (attempt === retryDelays.length - 1) {
            // The user may have reverted to the saved version during retries.
            saveError = hasPendingChanges;
            return false;
          }
          isRetrying = true;
        }
      }

      return false;
    } finally {
      isSaving = false;
      isRetrying = false;
    }
  }

  async function retrySave() {
    // Ignore double clicks and leave active/queued writes in charge.
    if (!editor || documentProtected || generationConflict || currentSave || isLeavingWithoutSaving) return;
    clearTimeout(autosaveTimer);
    await saveEntry();
  }

  async function finishEditing() {
    if (photoDraft) { focusPhotoDraft?.(); return; }
    if (!editor || weatherBusy || placeBusy || photoBusy) return;
    if (weatherDirty && !isNew) { weatherWarning = true; return; }
    // A new journal must first be saved to obtain the ID required by the place link.
    if (placeDirty && entryId) { placeWarning = true; return; }

    // Stop a scheduled autosave; finishing will save immediately.
    clearTimeout(autosaveTimer);

    saveMessage = "";

    // Wait for any ongoing save, then save the latest changes.
    const saved = await saveEntry();

    if (!saved || !editor || photoBusy) return;
    if(hasPendingChanges){scheduleAutosave();return;}
    if(isNew && weatherDirty){await tick();if(!await weatherControl?.flushPending()){weatherWarning=true;return;}}
    // Weather editing or a weather write may have started while the journal saved.
    if (weatherBusy) return;
    if (weatherDirty) { weatherWarning = true; return; }
    if (placeBusy) return;
    if (placeDirty) { placeWarning = true; return; }

    // Changes made during the save must remain available for editing.
    if (hasPendingChanges) {
      scheduleAutosave();
      return;
    }

    if (returnToJournal && !isNew && journalEntryId && entryId) {
      // Only return after the queue succeeded and no newer edits remain.
      // Keep editing active until navigation succeeds; the usual guard still applies.
      await goto(`/journal?entry=${encodeURIComponent(entryId)}`);
      return;
    }

    isEditing = false;
    editor.setEditable(false);
    onEditingChange?.(false);
    if (!entryId) {
      // An empty new document is finished without a row or a saved notification.
      saveError = false;
      return;
    }
    showSaveConfirmation();

    if (isNew && entryId) {
      await goto(`/journal/entries/${entryId}`, {
        replaceState: true,
        state: {
          justSaved: true,
        },
      });
    }
  }

  async function closeJournalWindow(): Promise<boolean> {
    try {
      await invoke("close_journal_window");
      return true;
    } catch {
      // Busy or uncertain IPC: stay open and keep the editor usable.
      return false;
    }
  }

  async function leaveWithoutSaving() {
    if (weatherBusy || placeBusy) return;
    // Stop new saves before waiting for an existing database write.
    isLeavingWithoutSaving = true;
    photoEpoch++; photoPosition = null; photoBusy = false;

    // Cancel any autosave that has not started yet.
    clearTimeout(autosaveTimer);

    // Let an ongoing database write finish before leaving the editor.
    // Do not start another save: the user chose to discard later changes.
    if (currentSave) {
      await currentSave;
    }

    if (pendingClose) {
      pendingClose = false;
      showLeaveConfirmation = false;
      if (!(await closeJournalWindow())) {
        isLeavingWithoutSaving = false;
        if (editor && hasPendingChanges) scheduleAutosave();
      }
      return;
    }

    if (!pendingNavigationUrl) return;

    const destination = pendingNavigationUrl;

    showLeaveConfirmation = false;
    pendingNavigationUrl = null;
    allowDiscardNavigation = true;

    try {
      await goto(destination);
    } catch (error) {
      // Restore saving if navigation fails and the editor remains open.
      isLeavingWithoutSaving = false;

      if (editor && hasPendingChanges) {
        scheduleAutosave();
      }

      throw error;
    } finally {
      allowDiscardNavigation = false;
    }
  }
</script>

{#snippet weatherHeader()}
  <EntryWeather bind:this={weatherControl} ensureEntry={ensureWeatherEntry} {entryId} contextPending={hasPendingChanges || isSaving || placeDirty || placeBusy || photoBusy} editing={isEditing} date={entryDate} generation={sessionGeneration} locale={language.current} onDraft={(dirty, busy) => { weatherDirty = dirty; weatherBusy = busy; if (!dirty) weatherWarning = false; if (!dirty && !busy && documentProtected && isEditing) { isEditing = false; onEditingChange?.(false); } }} />
{/snippet}
{#snippet savedEntryOptions()}
  <EntryOptions locale={language.current}>
    {#snippet children(closeAndFocus)}
      <button type="button" onclick={() => { closeAndFocus(); startEditing(); }} disabled={deletingEntry}>
        {m.journal_edit({}, { locale: language.current })}
      </button>
      {#if canDeleteEntry()}
        <button type="button" onclick={() => { closeAndFocus(); deleteFailed = false; showDeleteConfirmation = true; }}>
          {m.journal_delete_entry({}, { locale: language.current })}
        </button>
      {/if}
    {/snippet}
  </EntryOptions>
{/snippet}
{#if metadata}
  {@render metadata(weatherHeader,
    journalEntryId && !isEditing && !isLoading && !loadError && !documentProtected && !entryDeleted
      ? savedEntryOptions : undefined)}
{:else}
  {@render weatherHeader()}
{/if}

<div class="editor">
  {#if isLoading}
    <p class="load-status">
      {m.journal_loading({}, { locale: language.current })}
    </p>
  {:else if loadError}
    <p class="load-status error" role="alert">{loadError}</p>
  {/if}

  {#if deleteFailed}<p role="alert">{m.journal_delete_failed({}, { locale: language.current })}</p>{/if}

  {#if weatherWarning}<p role="alert">{m.weather_form_finishFirst({}, { locale: language.current })}</p>{/if}
  {#if placeWarning}<p role="alert">{m.place_finish_first({}, { locale: language.current })}</p>{/if}

  {#if documentProtected && !loadError}
    <p class="load-status error" role="alert">{m.journal_document_protected({}, { locale: language.current })}</p>
  {/if}

  {#if !isEditing && (!journalEntryId || saveMessage)}
    <div class="editor-actions">
      {#if saveMessage}
        <p
          class="save-status"
          class:error={saveError}
          class:fading={isSaveMessageFading}
          role="status"
        >
          {saveMessage}
        </p>
      {/if}

      {#if !journalEntryId}
        <button type="button" onclick={startEditing} disabled={isLoading || !!loadError || documentProtected}>
          {m.journal_edit({}, { locale: language.current })}
        </button>
      {/if}
    </div>
  {/if}

  {#if isEditing && editor}
    <nav
      class="editor-toolbar"
      aria-label={m.editor_toolbar({}, { locale: language.current })}
    >
      <button
        class:active={activeFormats.bold}
        aria-label={m.editor_bold({}, { locale: language.current })}
        aria-pressed={activeFormats.bold}
        onclick={() => editor?.chain().focus().toggleBold().run()}
        ><strong>B</strong></button
      >

      <button
        class:active={activeFormats.italic}
        aria-label={m.editor_italic({}, { locale: language.current })}
        aria-pressed={activeFormats.italic}
        onclick={() => editor?.chain().focus().toggleItalic().run()}
        ><em>I</em></button
      >

      <button
        class:active={activeFormats.heading}
        aria-label={m.editor_heading({}, { locale: language.current })}
        aria-pressed={activeFormats.heading}
        onclick={() =>
          editor?.chain().focus().toggleHeading({ level: 2 }).run()}>H2</button
      >

      <button
        class:active={activeFormats.bulletList}
        aria-label={m.editor_bullet_list({}, { locale: language.current })}
        aria-pressed={activeFormats.bulletList}
        onclick={() => editor?.chain().focus().toggleBulletList().run()}
        >☷</button
      >

      <button type="button" onclick={insertPhoto} disabled={photoBusy || documentProtected || generationConflict || sessionGeneration === null}>
        {m.photo_insert({}, { locale: language.current })}
      </button>
      <span class="toolbar-divider"></span>

      <button
        aria-label={m.editor_undo({}, { locale: language.current })}
        onclick={() => editor?.chain().focus().undo().run()}>↶</button
      >

      <button
        aria-label={m.editor_redo({}, { locale: language.current })}
        onclick={() => editor?.chain().focus().redo().run()}>↷</button
      >
      <div class="toolbar-save">
        <span
          class="save-status"
          class:fading={isSaveMessageFading}
          role="status"
        >
          {#if isRetrying}
            {m.journal_save_retrying({}, { locale: language.current })}
          {:else if isSaving}
            {m.journal_saving({}, { locale: language.current })}
          {:else}
            {saveMessage}
          {/if}
        </span>

        <button type="button" onclick={finishEditing} disabled={photoBusy}>
          {m.journal_done({}, { locale: language.current })}
        </button>
      </div>
    </nav>
  {/if}

  {#if generationConflict || (saveError && hasPendingChanges)}
    <div class="save-warning">
      <div role="alert">
        <strong>{m.journal_save_failed_title({}, { locale: language.current })}</strong>
        <p>{generationConflict
          ? m.journal_generation_conflict({}, { locale: language.current })
          : m.journal_save_failed_description({}, { locale: language.current })}</p>
      </div>
      <button type="button" onclick={retrySave} disabled={isSaving || generationConflict || documentProtected}>
        {m.journal_save_retry({}, { locale: language.current })}
      </button>
    </div>
  {/if}

  {#if photoBusy}<p class="load-status" role="status">{m.photo_processing({}, { locale: language.current })}</p>{/if}
  {#if photoError}<p class="load-status error" role="alert">{photoError}</p>{/if}

  <article class="journal-entry">

    <div bind:this={editorElement}></div>
  </article>
  {#if canDeleteEntry() && !documentProtected && !deletingEntry && entryId && sessionGeneration!==null}
    <BirdJournalLists entryId={entryId} generation={sessionGeneration} locale={language.current} journalDate={savedEntryDate} allowCreate={true}/>
  {/if}

</div>

{#if showLeaveConfirmation}
  <!-- Keep the marker used by the existing weather-panel focus guard. -->
  <dialog
      class="leave-dialog leave-overlay"
      use:mountLeaveDialog
      oncancel={cancelLeaveDialog}
      onkeydown={leaveDialogKeys}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="leave-dialog-title"
      aria-describedby="leave-dialog-description"
    >
      <h2 id="leave-dialog-title">
        {m.journal_unsaved_title({}, { locale: language.current })}
      </h2>

      <p id="leave-dialog-description">
        {m.journal_unsaved_description({}, { locale: language.current })}
      </p>

      <div class="leave-dialog-actions">
        <button
          type="button"
          onclick={cancelLeaveConfirmation}
        >
          {m.journal_continue_editing({}, { locale: language.current })}
        </button>

        <button
          type="button"
          onclick={leaveWithoutSaving}
          disabled={!pendingNavigationUrl && !pendingClose}
        >
          {m.journal_discard_changes({}, { locale: language.current })}
        </button>
      </div>
  </dialog>
{/if}

{#if showDeleteConfirmation}
  <DeleteEntryDialog title={savedTitle} date={savedEntryDate} locale={language.current} busy={deletingEntry}
    oncancel={() => { showDeleteConfirmation = false; }} onconfirm={deleteSavedEntry} />
{/if}

<style>
  .editor {
    position: relative;
  }

  .editor-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 24px;
    margin-bottom: 24px;
  }

  button {
    border: 1px solid #d9d8d1;
    border-radius: 8px;
    padding: 10px 16px;
    background: transparent;
    color: #343530;
    font-family: system-ui, sans-serif;
    cursor: pointer;
  }

  .journal-entry {
    display: flow-root;
    font-family: Georgia, serif;
    font-size: 19px;
    line-height: 1.5;
  }

  .journal-entry :global(.tiptap) {
    outline: none;
    min-height: 200px;
    border-radius: 6px;
  }

  .journal-entry :global(.tiptap[contenteditable="true"]) {
    padding: 10px;
    margin: -10px;
    background: rgba(120, 142, 123, 0.035);
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


  .save-status {
    margin: 0;
    color: #435b47;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 13px;
    text-align: right;
    pointer-events: none;
    transition: opacity 0.5s ease;
  }

  .save-status.error {
    color: #a33b35;
  }

  .save-warning {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 24px;
    padding: 14px 16px;
    border-left: 2px solid #a33b35;
    background: rgba(163, 59, 53, 0.04);
    font-family: system-ui, sans-serif;
    font-size: 13px;
    line-height: 1.5;
  }

  .save-warning p {
    margin: 4px 0 0;
  }

  .save-warning button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .load-status {
    margin: 0 0 16px;
    color: var(--accent, #435b47);
    font-family: system-ui, sans-serif;
    font-size: 13px;
  }

  .load-status.error {
    color: #a33b35;
  }

  .toolbar-save {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    margin-left: auto;
  }

  .toolbar-save .save-status {
    display: inline-block;
    min-width: 75px;
    max-width: 240px;
    text-align: right;
    white-space: nowrap;
  }

  .toolbar-save button {
    flex-shrink: 0;
  }

  .toolbar-save button {
    min-width: auto;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: #435b47;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 15px;
    cursor: pointer;
  }

  .toolbar-save button:hover {
    background: #eef1eb;
  }

  .toolbar-save button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .save-status {
    transition: opacity 0.5s ease;
  }

  .save-status.fading {
    opacity: 0;
  }

  .leave-dialog::backdrop {
    background: rgba(0, 0, 0, 0.35);
  }

  .leave-dialog {
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

  .leave-dialog h2 {
    margin: 0 0 12px;
    font-size: 1.2rem;
  }

  .leave-dialog p {
    margin: 0;
    line-height: 1.6;
  }

  .leave-dialog-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 24px;
  }

  .leave-dialog-actions button {
    padding: 10px 14px;
    border-radius: 8px;
    cursor: pointer;
  }

  .leave-dialog-actions button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
