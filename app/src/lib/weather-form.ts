import { weatherConditionKey, type WeatherPart } from '$lib/weather';

export const commonWeatherCodes = [0, 2, 3, 45, 51, 61, 80, 71, 95] as const;
export const manualNumericFields = ['temperatureC','windSpeedMs','precipitationMm','cloudCoverPercent','relativeHumidityPercent','snowDepthMm'] as const;
export type NumericField = typeof manualNumericFields[number];
export type Draft = { values: Record<NumericField, string>; code: number | null; description: string };
export function makeDraft(part: WeatherPart | null, locale: 'sv' | 'en'): Draft {
  return { values: Object.fromEntries(manualNumericFields.map(key => [key, part?.[key] == null ? '' : String(part[key]).replace('.', locale === 'sv' ? ',' : '.')])) as Draft['values'], code: part?.weatherCode ?? null, description: part?.description ?? '' };
}
export function parseDraft(draft: Draft, original: WeatherPart | null, date: string, locale: 'sv' | 'en', baseline?: Draft) {
  const result: WeatherPart = original ? { ...original } : { period: { kind:'day', date } };
  const initial = baseline ?? makeDraft(original, locale);
  const errors: Partial<Record<NumericField | 'code' | 'empty', true>> = {};
  for (const key of manualNumericFields) {
    if (draft.values[key] === initial.values[key]) continue;
    const text = draft.values[key].trim();
    if (!text) { delete result[key]; continue; }
    const normal = locale === 'sv' ? text.replace(',', '.') : text;
    if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(normal)) { errors[key] = true; continue; }
    const n = Number(normal);
    if (!Number.isFinite(n) || (key === 'temperatureC' ? n < -273.15 : n < 0) || (['cloudCoverPercent','relativeHumidityPercent'].includes(key) && n > 100)) { errors[key] = true; continue; }
    result[key] = n;
  }
  if (draft.code != null && !weatherConditionKey(draft.code)) errors.code = true;
  if (draft.code !== initial.code) { if (draft.code == null) delete result.weatherCode; else result.weatherCode = draft.code; }
  if (draft.description !== initial.description) { if (draft.description.trim()) result.description = draft.description; else delete result.description; }
  if (!original && result.weatherCode == null && !result.description?.trim() && manualNumericFields.every(key => result[key] == null)) errors.empty = true;
  return { result, errors };
}
