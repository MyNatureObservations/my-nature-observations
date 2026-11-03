import { createJournalReader } from "$lib/journal-data";

export type WeatherPeriod = { kind: "day"; date: string } | { kind: "hour"; start: string; endExclusive: string };
export type WeatherLocation = { name?: string | null; coordinates?: { latitude: number; longitude: number } | null };
export type WeatherPart = {
  period: WeatherPeriod; location?: WeatherLocation | null; timezone?: string | null;
  description?: string | null; temperatureC?: number | null; temperatureMeanC?: number | null;
  temperatureMinC?: number | null; temperatureMaxC?: number | null; windSpeedMs?: number | null;
  windSpeedMaxMs?: number | null; precipitationMm?: number | null; precipitationSumMm?: number | null;
  weatherCode?: number | null; relativeHumidityPercent?: number | null; cloudCoverPercent?: number | null;
  snowDepthMm?: number | null; retrievedAt?: string; sunrise?: string | null; sunset?: string | null;
  validAt?:string|null;currentIntervalSeconds?:number|null;
  source?: { provider: string; product: string; model?: string | null; dataKind: string;
    sampledCoordinates?: { latitude: number; longitude: number } | null;
    attribution: string; license: string; licenseUrl?: string | null };
};
export type WeatherView = { manual: WeatherPart | null; fetched: WeatherPart | null; revision: string };
export type WeatherStatus = { kind: "loading" } | { kind: "error" } | { kind: "ready" | "unverified"; data: WeatherView };
export const emptyWeather: WeatherStatus = { kind: "ready", data: { manual: null, fetched: null, revision: "0" } };
export function weatherIsCurrent(part:WeatherPart|null|undefined):boolean{return part?.source?.product === 'current-weather' && !!part.validAt;}

// Separate reader from journal loading; expected generation also ties weather to
// the displayed document. Batches avoid one database connection per reading tile.
export function createWeatherReader() {
  const read = createJournalReader();
  let request = 0;
  return async (ids: string[], generation: number): Promise<Record<string, WeatherView>> => {
    const ticket = ++request;
    const result: Record<string, WeatherView> = Object.create(null);
    for (let i = 0; i < ids.length; i += 100) {
      const batch = ids.slice(i, i + 100);
      const snapshot = await read<Record<string, WeatherView>>("read_journal_weather", { generation, ids: batch });
      if (ticket !== request) throw "journal_read_superseded";
      if (snapshot.generation !== generation) throw "journal_generation_conflict";
      for (const id of batch) {
        if (!Object.hasOwn(snapshot.data, id)) throw "weather_entry_missing";
        result[id] = snapshot.data[id];
      }
    }
    return result;
  };
}

export function weatherRows(part: WeatherPart, locale?: "sv" | "en"): [string, string][] {
  const rows: [string, string][] = [];
  const add = (key: string, value: string | number | null | undefined, unit = "") => {
    if (value != null) rows.push([key, `${typeof value === "number" && locale && key !== "weatherCode" ? weatherNumber(value, locale) : value}${unit}`]);
  };
  add("period", part.period.kind === "day" ? part.period.date : `${part.period.start} – ${part.period.endExclusive}`);
  add("location", part.location?.name);
  const coordinates = part.location?.coordinates;
  if (coordinates) add("coordinates", `${coordinates.latitude}, ${coordinates.longitude}`);
  add("timezone", part.timezone); add("description", part.description);
  for (const [field, unit] of [
    ["temperatureC", " °C"], ["temperatureMeanC", " °C"], ["temperatureMinC", " °C"], ["temperatureMaxC", " °C"],
    ["windSpeedMs", " m/s"], ["windSpeedMaxMs", " m/s"], ["precipitationMm", " mm"], ["precipitationSumMm", " mm"],
    ["weatherCode", ""], ["relativeHumidityPercent", " %"], ["cloudCoverPercent", " %"], ["snowDepthMm", " mm"],
  ] as const) add(field, part[field], unit);
  add("sunrise", part.sunrise); add("sunset", part.sunset); add("retrievedAt", part.retrievedAt);
  add("validAt",part.validAt);add("currentIntervalSeconds",part.currentIntervalSeconds," s");
  if (part.source) {
    const s = part.source;
    add("provider", s.provider); add("product", s.product); add("model", s.model); add("dataKind", s.dataKind);
    if (s.sampledCoordinates) add("sampledCoordinates", `${s.sampledCoordinates.latitude}, ${s.sampledCoordinates.longitude}`);
    add("attribution", s.attribution); add("license", s.license); add("licenseUrl", s.licenseUrl);
  }
  return rows;
}

// Display-only grouping. All original rows remain available; no stored part is changed.
export function weatherSections(part: WeatherPart, locale?: "sv" | "en") {
  const rows = weatherRows(part, locale);
  const order = [
    "temperatureC", "temperatureMeanC", "temperatureMinC", "temperatureMaxC",
    "description", "weatherCode", "windSpeedMs", "windSpeedMaxMs",
    "precipitationMm", "precipitationSumMm", "cloudCoverPercent",
    "relativeHumidityPercent", "snowDepthMm", "sunrise", "sunset",
  ];
  return {
    primary: order.flatMap(key => rows.filter(([field]) => field === key)),
    details: rows.filter(([key]) => !order.includes(key) || ["weatherCode", "sunrise", "sunset"].includes(key)),
  };
}

// Coarse, translated display names; the exact WMO code stays in the details.
// https://open-meteo.com/en/docs#weather-variable-documentation
const weatherConditionKeys: Record<number, string> = {
    0: "clear", 1: "mainlyClear", 2: "partlyCloudy", 3: "overcast",
    45: "fog", 48: "fog", 51: "drizzle", 53: "drizzle", 55: "drizzle",
    56: "freezingDrizzle", 57: "freezingDrizzle", 61: "rain", 63: "rain", 65: "rain",
    66: "freezingRain", 67: "freezingRain", 71: "snow", 73: "snow", 75: "snow",
    77: "snowGrains", 80: "rainShowers", 81: "rainShowers", 82: "rainShowers",
    85: "snowShowers", 86: "snowShowers", 95: "thunderstorm", 97: "thunderstorm",
    96: "thunderstormHail", 99: "thunderstormHail",
};
export const weatherConditionCodes = Object.keys(weatherConditionKeys).map(Number);
export function weatherConditionKey(code: number): string | undefined {
  return weatherConditionKeys[code];
}

export type WeatherIconName = "sun" | "sun-cloud" | "cloud" | "rain" | "snow" | "fog" | "thunder" | "neutral" | "temperature" | "wind" | "drop" | "humidity" | "sunrise" | "sunset";

// Only explicit condition codes select a weather icon. Measurements and free text
// never imply a condition. Unknown codes deliberately remain neutral.
export function weatherIcon(code: number | null | undefined): WeatherIconName {
  switch (weatherConditionKey(code ?? -1)) {
    case "clear": case "mainlyClear": return "sun";
    case "partlyCloudy": return "sun-cloud";
    case "overcast": return "cloud";
    case "fog": return "fog";
    case "drizzle": case "freezingDrizzle": case "rain": case "freezingRain": case "rainShowers": return "rain";
    case "snow": case "snowGrains": case "snowShowers": return "snow";
    case "thunderstorm": case "thunderstormHail": return "thunder";
    default: return "neutral";
  }
}
export function weatherFieldIcon(key: string): WeatherIconName {
  if (key.startsWith("temperature")) return "temperature";
  if (key.startsWith("wind")) return "wind";
  if (key.startsWith("precipitation")) return "drop";
  if (key === "cloudCoverPercent") return "cloud";
  if (key === "relativeHumidityPercent") return "humidity";
  if (key === "snowDepthMm") return "snow";
  if (key === "sunrise" || key === "sunset") return key;
  return "neutral";
}

type OverviewValue = { value: number; source: "manual" | "fetched"; field: string };
export function weatherOverview(data: WeatherView) {
  const {manual, fetched} = data;
  const pick = (manualValue: number | null | undefined, fetchedValue: number | null | undefined, manualField: string, fetchedField = manualField): OverviewValue | null =>
    manualValue != null ? {value:manualValue,source:"manual",field:manualField} :
    fetchedValue != null ? {value:fetchedValue,source:"fetched",field:fetchedField} : null;
  const current=weatherIsCurrent(fetched);
  const fetchedTemperature=current?fetched?.temperatureC:fetched?.temperatureMeanC;
  const temperature = pick(manual?.temperatureC, fetchedTemperature, "temperatureC", current?"temperatureC":"temperatureMeanC");
  // A manual observation in prose is primary too, but cannot safely select an
  // icon. Keep it neutral instead of letting a provider code contradict the text.
  const description = manual?.description?.trim() || null;
  const condition = manual?.weatherCode != null ? {code:manual.weatherCode,source:"manual" as const,description} :
    description ? {code:null,source:"manual" as const,description} :
    fetched?.weatherCode != null ? {code:fetched.weatherCode,source:"fetched" as const,description:null} :
    {code:null,source:null,description:null};
  const metrics = [
    {measurement:pick(manual?.windSpeedMs, current?fetched?.windSpeedMs:fetched?.windSpeedMaxMs, "windSpeedMs", current?"windSpeedMs":"windSpeedMaxMs"),unit:"m/s",icon:"wind" as const},
    {measurement:pick(manual?.precipitationMm, current?fetched?.precipitationMm:fetched?.precipitationSumMm, "precipitationMm", current?"precipitationMm":"precipitationSumMm"),unit:"mm",icon:"drop" as const},
    {measurement:pick(manual?.cloudCoverPercent, fetched?.cloudCoverPercent, "cloudCoverPercent"),unit:"%",icon:"cloud" as const},
  ].filter(metric => metric.measurement !== null);
  const origins = [condition.source, temperature?.source, ...metrics.map(metric => metric.measurement?.source)].filter(Boolean);
  return {
    temperature, condition, icon:weatherIcon(condition.code), metrics,
    onlyFetched: origins.length > 0 && origins.every(origin => origin === "fetched"),
    fetchedTemperature: manual?.temperatureC != null && fetchedTemperature != null && manual.temperatureC !== fetchedTemperature ? fetchedTemperature : null,
  };
}

// Display the wall-clock portion recorded with the sun time. Never reinterpret it
// using the computer timezone. The original full timestamp also stays in details.
export function weatherClock(value: string): string {
  return /^\d{4}-\d{2}-\d{2}T(\d{2}:\d{2})/.exec(value)?.[1] ?? value;
}
export function weatherNumber(value: number, locale: "sv" | "en"): string {
  const magnitude = Math.abs(value);
  // Small non-zero measurements keep two significant digits; scientific notation
  // prevents extremely small valid values from becoming long strings of zeros.
  const options: Intl.NumberFormatOptions = magnitude > 0 && magnitude < 0.1
    ? { maximumSignificantDigits: 2, ...(magnitude < 0.0001 ? { notation: "scientific" } : {}) }
    : { maximumFractionDigits: 1 };
  return new Intl.NumberFormat(locale, options).format(value);
}
