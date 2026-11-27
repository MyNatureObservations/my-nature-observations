import { invoke } from "@tauri-apps/api/core";
import { birdHref, entryHref } from "$lib/birds";
import { createJournalReader } from "$lib/journal-data";
import { readPhotograph, photographSource } from "$lib/journal-photographs";
import { journalPhotoRefs, observationPhotos, type GalleryPhoto } from "$lib/photo-gallery";
import { readJournalPlace } from "$lib/places";
import type { ContentPlace } from "$lib/places-overview";

export type PlacePhoto = GalleryPhoto & { parentHref: string; parentKind: "journal" | "observation" };

export async function loadPlacePhotos(
  place: ContentPlace, generation: number, locale: "sv" | "en", current: () => boolean,
): Promise<{ items: PlacePhoto[]; incomplete: boolean }> {
  const items: PlacePhoto[] = [];
  let incomplete = false;
  const seen = new Set<string>();
  const read = createJournalReader();
  for (const entry of place.entries) {
    if (!current()) break;
    try {
      const { place: assigned } = await readJournalPlace(entry.id, generation);
      if (assigned?.id !== place.id || !current()) continue;
      const { data } = await read<{ contentJson: string } | null>("get_journal_entry", { id: entry.id });
      if (!data) { incomplete = true; continue; }
      const refs = new Map<string, ReturnType<typeof journalPhotoRefs>>();
      for (const ref of journalPhotoRefs(data.contentJson)) {
        const nodes = refs.get(ref.assetId);
        if (nodes) nodes.push(ref);
        else refs.set(ref.assetId, [ref]);
      }
      for (const [assetId, nodes] of refs) {
        if (!current()) break;
        const key = `journal:${assetId}`;
        if (seen.has(key)) continue;
        seen.add(key);
        const photo: PlacePhoto = {
          key, src: null, alt: nodes[0].alt ?? "",
          caption: [...new Set(nodes.map(node => node.caption).filter(Boolean))].join("\n"),
          description: entry.title,
          parentHref: entryHref(entry.id), parentKind: "journal",
        };
        try {
          const asset = await readPhotograph(invoke, entry.id, generation, assetId);
          photo.src = photographSource(asset);
          photo.width = asset.width;
          photo.height = asset.height;
        } catch { incomplete = true; }
        items.push(photo);
      }
    } catch { incomplete = true; }
  }

  const visits = place.visits.filter(visit => !visit.source && visit.place.id === place.id);
  if (current() && visits.length && await invoke<boolean>("bird_observations_available")) {
    const uniqueVisits = new Map(visits.map(visit => [visit.id, visit]));
    for (const visit of uniqueVisits.values()) {
      for (const photo of observationPhotos([visit], locale)) {
        if (seen.has(photo.key)) continue;
        seen.add(photo.key);
        items.push({ ...photo, parentHref: birdHref(null, visit.id), parentKind: "observation" });
      }
    }
  }
  if (current() && (await invoke<{ generation: number }>("new_journal_session")).generation !== generation) {
    throw Error("journal_generation_conflict");
  }
  return { items, incomplete };
}
