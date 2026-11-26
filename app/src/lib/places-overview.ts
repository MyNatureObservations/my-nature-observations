import { invoke } from "@tauri-apps/api/core";
import { createJournalReader } from "$lib/journal-data";
import type { BirdList } from "$lib/birds";
import { mapDraftCoordinates, type Place } from "$lib/places";

export type JournalPlaceEntry = {
  place: Place;
  entry: { id: string; entryDate: string; title: string };
};

export type ContentPlace = Place & {
  source?: "ebird";
  entries: JournalPlaceEntry["entry"][];
  visits: BirdList[];
};

export function groupContentPlaces(journal: JournalPlaceEntry[], visits: BirdList[], locale: "sv" | "en") {
  const groups = new Map<string, ContentPlace>();
  function group(place: Place, source?: "ebird") {
    const existing = groups.get(place.id);
    if (existing) return existing;
    const item: ContentPlace = { ...place, source, entries: [], visits: [] };
    groups.set(place.id, item);
    return item;
  }
  for (const { place, entry } of journal) group(place).entries.push(entry);
  // Rust projects native place IDs for manual visits and namespaced source IDs
  // for eBird. Use those identities even when places share names or map points.
  const ordered = [...visits].sort((a, b) => b.visitDate.localeCompare(a.visitDate) || (b.visitStart ?? "").localeCompare(a.visitStart ?? "") || a.id.localeCompare(b.id));
  for (const visit of ordered) {
    const source = visit.source ? "ebird" : undefined;
    group(visit.place, source).visits.push(visit);
  }
  const collator = new Intl.Collator(locale, { sensitivity: "base" });
  return [...groups.values()].sort((a, b) => collator.compare(a.name, b.name) || a.id.localeCompare(b.id));
}

export type PlaceFilter = "all" | "personal" | "ebird";
export type PlaceSort = "name" | "latitude";
type BrowsablePlace = Place & { source?: "ebird" };

export function matchesPlaceFilter(place: BrowsablePlace, filter: PlaceFilter) {
  return filter === "all" || (filter === "ebird" ? place.source === "ebird" : !place.source);
}

export function sortPlaces<T extends BrowsablePlace>(places: T[], sort: PlaceSort, locale: "sv" | "en"): T[] {
  const names = new Intl.Collator(locale, { sensitivity: "base" });
  const latitude = (place: T) => mapDraftCoordinates(String(place.latitude), String(place.longitude))?.[0] ?? Infinity;
  return [...places].sort((a, b) =>
    (sort === "latitude" ? latitude(a) - latitude(b) : 0) ||
    names.compare(a.name, b.name) || a.id.localeCompare(b.id),
  );
}

export async function readPlaceContents(generation: number) {
  const enabled = await invoke<boolean>("bird_observations_available");
  const [journal, birds] = await Promise.all([
    createJournalReader()<JournalPlaceEntry[]>("read_journal_place_entries", { generation }),
    enabled ? createJournalReader()<BirdList[]>("read_bird_lists", { generation }) : null,
  ]);
  return { entries: journal.data, visits: birds?.data ?? [] };
}
