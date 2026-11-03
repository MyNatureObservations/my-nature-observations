import { invoke } from "@tauri-apps/api/core";
import { createJournalReader, type JournalSnapshot } from "$lib/journal-data";

export type Place = {
  id: string;
  name: string;
  latitude: number | null;
  longitude: number | null;
  revision: string;
};

export type PlaceSnapshot = {
  id: string;
  name: string;
  latitude: number | null;
  longitude: number | null;
  placeRevision: string;
};

export type JournalPlace = {
  place: PlaceSnapshot | null;
  revision: string;
};

export type PlaceCoordinates = { latitude: number; longitude: number };

export function groupMappedPlaces(places: Place[]): Place[][] {
  const groups = new Map<string, Place[]>();
  for (const place of places) {
    if (place.latitude === null || place.longitude === null || !mapDraftCoordinates(String(place.latitude), String(place.longitude))) continue;
    const key = `${place.latitude},${place.longitude}`;
    const group = groups.get(key) ?? [];
    group.push(place);
    groups.set(key, group);
  }
  return [...groups.values()];
}

export type WeatherDefaultPlace = {
  place: (PlaceCoordinates & { id: string; name: string }) | null;
  revision: string;
};

export async function readWeatherDefaultPlace(generation: number): Promise<WeatherDefaultPlace> {
  return requireGeneration(await invoke<JournalSnapshot<WeatherDefaultPlace>>(
    "read_weather_default_place", { generation }), generation).data;
}

export async function setWeatherDefaultPlace(generation: number, expectedRevision: string, placeId: string | null): Promise<WeatherDefaultPlace> {
  return requireGeneration(await invoke<JournalSnapshot<WeatherDefaultPlace>>(
    "set_weather_default_place", { generation, expectedRevision, placeId }), generation).data;
}

export function mapDraftCoordinates(latitude: string, longitude: string): [number, number] | null {
  const latText = latitude.trim().replace(",", ".");
  const lonText = longitude.trim().replace(",", ".");
  if (!latText || !lonText) return null;
  const lat = Number(latText), lon = Number(lonText);
  return Number.isFinite(lat) && Number.isFinite(lon) && lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180
    ? [lat, lon] : null;
}

export function mapStartCoordinates(
  journal: PlaceCoordinates | null,
  defaultPlace: PlaceCoordinates | null,
): PlaceCoordinates | null {
  return journal ?? defaultPlace;
}

export function filterPlaces(places: Place[], query: string, locale: "sv" | "en"): Place[] {
  const needle = query.trim().toLocaleLowerCase(locale === "sv" ? "sv-SE" : "en");
  if (!needle) return places;
  return places.filter((place) =>
    place.name.toLocaleLowerCase(locale === "sv" ? "sv-SE" : "en").includes(needle),
  );
}

export function movePlaceIndex(current: number, length: number, direction: 1 | -1): number {
  if (length <= 0) return -1;
  if (current < 0) return direction === 1 ? 0 : length - 1;
  return (current + direction + length) % length;
}

export function parsePlaceCoordinates(
  latitude: string,
  longitude: string,
): { latitude: number | null; longitude: number | null } | null {
  const latText = latitude.trim().replace(",", ".");
  const lonText = longitude.trim().replace(",", ".");
  if (!latText && !lonText) return { latitude: null, longitude: null };
  if (!latText || !lonText) return null;
  const lat = Number(latText), lon = Number(lonText);
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || lat < -90 || lat > 90 || lon < -180 || lon > 180) return null;
  return { latitude: lat, longitude: lon };
}

function requireGeneration<T>(snapshot: JournalSnapshot<T>, generation: number): JournalSnapshot<T> {
  if (snapshot.generation !== generation) throw "journal_generation_conflict";
  return snapshot;
}

export async function listPlaces(generation: number): Promise<Place[]> {
  return requireGeneration(
    await invoke<JournalSnapshot<Place[]>>("list_places", { generation }),
    generation,
  ).data;
}

export async function readJournalPlace(entryId: string, generation: number): Promise<JournalPlace> {
  return requireGeneration(
    await invoke<JournalSnapshot<JournalPlace>>("read_journal_place", { entryId, generation }),
    generation,
  ).data;
}

export async function createPlace(
  generation: number,
  name: string,
  latitude: number | null,
  longitude: number | null,
): Promise<Place> {
  return requireGeneration(
    await invoke<JournalSnapshot<Place>>("create_place", {
      generation,
      name,
      latitude,
      longitude,
    }),
    generation,
  ).data;
}

export async function setJournalPlace(
  generation: number,
  entryId: string,
  expectedRevision: string,
  placeId: string | null,
): Promise<JournalPlace> {
  return requireGeneration(
    await invoke<JournalSnapshot<JournalPlace>>("set_journal_place", {
      generation,
      entryId,
      expectedRevision,
      placeId,
    }),
    generation,
  ).data;
}

export async function updatePlace(generation: number, place: Place, name: string, latitude: number | null, longitude: number | null): Promise<Place> {
  return requireGeneration(await invoke<JournalSnapshot<Place>>("update_place", {
    generation, id: place.id, expectedRevision: place.revision, name, latitude, longitude,
  }), generation).data;
}

export async function deletePlace(generation: number, id: string): Promise<void> {
  requireGeneration(await invoke<JournalSnapshot<null>>("delete_place", { generation, id }), generation);
}

export type PlaceUsage = {
  placeId: string;
  isWeatherDefault: boolean;
  totalCount: number;
  entries: { id: string; entryDate: string; title: string }[];
};

export async function readPlaceUsage(generation: number, placeId: string): Promise<PlaceUsage> {
  const read = createJournalReader();
  return requireGeneration(await read<PlaceUsage>("read_place_usage", {generation, placeId}), generation).data;
}
