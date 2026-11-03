import { invoke } from "@tauri-apps/api/core";

export type JournalSnapshot<T> = { generation: number; data: T };
let observedGeneration = 0;

// One reader per consumer: a late response cannot replace a newer request.
export function createJournalReader() {
  let request = 0;
  return async function read<T>(command: string, args?: Record<string, unknown>): Promise<JournalSnapshot<T>> {
    const currentRequest = ++request;
    const result = await invoke<JournalSnapshot<T>>(command, args);
    // Recheck against Rust, including when no newer response has reached this webview.
    const current = await invoke<JournalSnapshot<null>>("new_journal_session");
    observedGeneration = Math.max(observedGeneration, current.generation);
    if (currentRequest !== request) throw "journal_read_superseded";
    if (result.generation !== observedGeneration) throw "journal_generation_conflict";
    return result;
  };
}
