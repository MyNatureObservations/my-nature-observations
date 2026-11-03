import { invoke } from "@tauri-apps/api/core";
import type { WeatherPart } from "$lib/weather";
import type { JournalPlace, WeatherDefaultPlace, PlaceCoordinates } from "$lib/places";
export type WeatherPlaceSource = { kind: "journal" } | { kind: "default" } | { kind: "saved"; id: string };
export type HistoricalPreview = { previewId: string; entryId: string; weatherRevision: string; fetched: WeatherPart };
export type WeatherPlaceOption = { key: string; name: string; latitude: number; longitude: number; source: WeatherPlaceSource };
export function hasWeatherCoordinates(place: {latitude:number|null;longitude:number|null}|null|undefined): place is PlaceCoordinates {
  return !!place && place.latitude !== null && place.longitude !== null && Number.isFinite(place.latitude) && Number.isFinite(place.longitude) && Math.abs(place.latitude) <= 90 && Math.abs(place.longitude) <= 180;
}
export function weatherPlaceDefaults(journal: JournalPlace, setting: WeatherDefaultPlace): {options: WeatherPlaceOption[]; selected:string; missingCoordinates:boolean} {
  const options: WeatherPlaceOption[] = [];
  if (hasWeatherCoordinates(journal.place)) options.push({key:"journal",...journal.place!,source:{kind:"journal"}});
  if (hasWeatherCoordinates(setting.place)) options.push({key:"default",...setting.place!,source:{kind:"default"}});
  const missingCoordinates = journal.place !== null && !hasWeatherCoordinates(journal.place);
  return { options, selected: missingCoordinates ? "" : options[0]?.key ?? "", missingCoordinates };
}
export async function previewHistoricalWeather(entryId:string, generation:number, source:WeatherPlaceSource): Promise<HistoricalPreview> {
  const snapshot = await invoke<{generation:number;data:HistoricalPreview}>("preview_historical_weather",{entryId,generation,source});
  if (snapshot.generation !== generation || snapshot.data.entryId !== entryId) throw "journal_generation_conflict";
  return snapshot.data;
}
// Return the confirmed write directly. Follow-up read failures must never turn
// a successful commit into a reported save failure.
export function saveHistoricalWeather(entryId:string,generation:number,preview:HistoricalPreview,replaceConfirmed:boolean) {
  return invoke<{generation:number;data:{revision:string}}>("save_historical_weather",{entryId,generation,previewId:preview.previewId,expectedRevision:preview.weatherRevision,replaceConfirmed});
}
export function historicalError(cause:unknown, fetching = false): string {
  const code = String(cause);
  if (/conflict|preview_expired/.test(code)) return "conflict";
  if (/date_unavailable|invalid_date/.test(code)) return "dateError";
  if (/timeout/.test(code)) return "timeout";
  if (/service_unavailable/.test(code)) return "serviceUnavailable";
  if (/rate_limited/.test(code)) return "rateLimited";
  if (/network/.test(code)) return "network";
  if (/^http$|^api$/.test(code)) return "fetchFailed";
  if (/no_data|empty_preview/.test(code)) return "noData";
  if (/place|coordinates/.test(code)) return "placeError";
  if (/entry_missing/.test(code)) return "needEntry";
  if (/invalid_response|invalid_preview/.test(code)) return "invalidResponse";
  return fetching ? "fetchFailed" : "failed";
}
