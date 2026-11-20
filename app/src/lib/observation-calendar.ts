import { invoke } from '@tauri-apps/api/core';
import { createJournalReader } from '$lib/journal-data';
import { groupBirdVisits, type BirdList } from '$lib/birds';

// One visit per identity, independently of its journal links or import source.
export function calendarVisits(lists: BirdList[], date?: string): BirdList[] {
  const unique = [...new Map(lists.map(list => [list.id, list])).values()];
  return groupBirdVisits(unique).flatMap(group => group.visits).filter(list => !date || list.visitDate === date);
}
export function observationDates(lists: BirdList[], year: number, month: number): string[] {
  const prefix = `${year}-${String(month).padStart(2, '0')}-`;
  return [...new Set(calendarVisits(lists).map(list => list.visitDate).filter(date => date.startsWith(prefix)))].sort();
}
export function createObservationCalendarReader() {
  const read = createJournalReader();
  return async () => {
    if (!await invoke<boolean>('bird_observations_available')) return null;
    const session = await invoke<{generation:number}>('new_journal_session');
    return read<BirdList[]>('read_bird_lists', {generation:session.generation});
  };
}
