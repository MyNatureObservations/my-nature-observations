import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8');
function luminance(hex) {
  const channels = hex.slice(1).match(/../g).map(byte => parseInt(byte, 16) / 255)
    .map(c => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return channels.reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i], 0);
}
function contrast(a, b) {
  const [dark, light] = [luminance(a), luminance(b)].sort((a, b) => a - b);
  return (light + 0.05) / (dark + 0.05);
}
function rule(path, selector) {
  const css = read(path).split('<style>')[1];
  const matches = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter(match => match[1].trim() === selector);
  assert.ok(matches.length, `${path}: missing ${selector}`);
  return matches.map(match => match[2]).join('\n');
}
function color(body, property) {
  const value = body.match(new RegExp(`(?:^|[;\\s])${property}:\\s*([^;]+)`))?.[1];
  assert.ok(value, `missing ${property}`);
  // Current light appearance has no token overrides. Resolve the actual CSS fallback.
  const hex = value.match(/#[\da-f]{6}/i)?.[0];
  assert.ok(hex, `unsupported color: ${value}`);
  return hex;
}

// Each entry is an affected text rule, including hover and selected states.
const affected =
[
  ["lib/components/WeatherDetails.svelte", "dt"],
  ["lib/components/WeatherDetails.svelte", "summary"],
  ["lib/components/WeatherOverview.svelte", ".meteo"],
  ["lib/components/WeatherOverview.svelte", ".supplement"],
  ["lib/components/AboutMno.svelte", ".version, .hint"],
  ["lib/components/AboutMno.svelte", "summary, button, a"],
  ["lib/components/AppNavigation.svelte", ".backup-button"],
  ["lib/components/AppNavigation.svelte", ".settings-group-heading"],
  ["lib/components/AppNavigation.svelte", ".panel-section-heading"],
  ["lib/components/AppNavigation.svelte", ".language-choices button"],
  ["lib/components/AppNavigation.svelte", ".language-choices button.language-selected"],
  ["lib/components/AppNavigation.svelte", ".navigation-search input::placeholder"],
  ["lib/components/AppNavigation.svelte", ".calendar-back-heading"],
  ["lib/components/AppNavigation.svelte", ".app-home-link:hover"],
  ["lib/components/CalendarDayPanel.svelte", "h3"],
  ["lib/components/CalendarDayPanel.svelte", "p"],
  ["lib/components/CalendarDayPanel.svelte", ".entry-title:hover"],
  ["lib/components/CalendarDayPanel.svelte", ".day-reading-link"],
  ["lib/components/CalendarDayPanel.svelte", ".new-entry-link"],
  ["lib/components/CalendarGrid.svelte", ".weekday"],
  ["lib/components/CalendarGrid.svelte", ".month-title:hover"],
  ["lib/components/CalendarGrid.svelte", ".month-picker-today"],
  ["lib/components/DatePicker.svelte", ".date-display"],
  ["lib/components/DatePicker.svelte", ".weekday"],
  ["lib/components/EntryPlace.svelte", ".place-trigger"],
  ["lib/components/EntryPlace.svelte", ".place-read"],
  ["lib/components/EntryPlace.svelte", ".place-result small,.form p,.empty"],
  ["lib/components/HistoricalWeather.svelte", "h3,h4"],
  ["lib/components/HistoricalWeather.svelte", ".hint"],
  ["lib/components/JournalEditor.svelte", ".load-status"],
  ["lib/components/JournalReading.svelte", ".entry-date"],
  ["lib/components/JournalReading.svelte", ".message"],
  ["lib/components/PlaceMap.svelte", ".map-help"],
  ["lib/components/PlaceMapPanel.svelte", "p"],
  ["lib/components/PlaceMapPanel.svelte", ".action-group h3"],
  ["lib/components/PlaceMapPanel.svelte", ".usage-list a"],
  ["lib/components/PlaceMapPanel.svelte", ".usage-list a span:first-child"],
  ["lib/components/WeatherDetails.svelte", ".snapshot-kind"],
  ["lib/components/WeatherDetails.svelte", ".attribution"],
  ["lib/components/WeatherForm.svelte", ".fields label span"],
  ["lib/components/WeatherForm.svelte", "summary"],
  ["routes/+page.svelte", ".section-label"],
  ["routes/+page.svelte", ".card-label"],
  ["routes/+page.svelte", ".journal-card p"],
  ["routes/+page.svelte", "footer"],
  ["routes/history/+page.svelte", ".back-link"],
  ["routes/journal/+page.svelte", ".navigation-message"],
  ["routes/journal/+page.svelte", ".back-link"],
  ["routes/journal/+page.svelte", ".section-label"],
  ["routes/journal/+page.svelte", ".journal-footer"],
  ["routes/journal/entries/+page.svelte", ".back-link"],
  ["routes/journal/entries/+page.svelte", ".section-label"],
  ["routes/journal/entries/+page.svelte", ".page-heading p"],
  ["routes/journal/entries/+page.svelte", ".entry-date"],
  ["routes/journal/entries/+page.svelte", ".message"],
  ["routes/journal/entries/+page.svelte", ".journal-footer"],
  ["routes/journal/entries/[id]/+page.svelte", ".back-link"],
  ["routes/journal/entries/[id]/+page.svelte", ".section-label"],
  ["routes/journal/entries/[id]/+page.svelte", ".entry-date"],
  ["routes/journal/entries/[id]/+page.svelte", ".message"],
  ["routes/journal/entries/[id]/+page.svelte", ".journal-footer"],
  ["routes/journal/new/+page.svelte", ".back-link"],
  ["routes/journal/new/+page.svelte", ".section-label"],
  ["routes/journal/new/+page.svelte", ".journal-footer"],
  ["routes/search/+page.svelte", ".back-link"],
  ["routes/search/+page.svelte", ".query, time"],
  ["routes/settings/import/+page.svelte", ".back-link"],
  ["routes/settings/restore/+page.svelte", ".back-link"],
];
for (const [path, selector] of affected) {
  test(`AX-02 normal/hover/focus text: ${path} ${selector}`, () => {
    const foreground = color(rule(path, selector), 'color');
    for (const background of ['#f8f7f3', '#f1f0eb', '#eeeee8', '#eef1eb', '#fbfaf7', '#fdfcf9', '#fffefa', '#faf9f5', '#faf9f6']) {
      assert.ok(contrast(foreground, background) >= 4.5,
        `${foreground} on ${background}: ${contrast(foreground, background)}`);
    }
  });
}
for (const [path, selector] of [
  ['lib/components/CalendarGrid.svelte', '.calendar-day.selected'],
  ['lib/components/CalendarGrid.svelte', '.month-picker-option.current-month'],
  ['lib/components/DatePicker.svelte', '.calendar-day.selected'],
]) {
  test(`AX-02 selected calendar label: ${path} ${selector}`, () => {
    const body = rule(path, selector);
    assert.ok(contrast(color(body, 'color'), color(body, 'background')) >= 4.5);
  });
}
test('search text and placeholder are not faded by wrapper opacity', () => {
  const path = 'lib/components/AppNavigation.svelte';
  assert.doesNotMatch(rule(path, '.navigation-search'), /opacity\s*:/);
  assert.match(rule(path, '.navigation-search input::placeholder'), /opacity:\s*1/);
});
test('contrast calculation reproduces the original AX-02 measurements', () => {
  assert.ok(Math.abs(contrast('#788e7b', '#f8f7f3') - 3.290) < 0.001);
  assert.ok(Math.abs(contrast('#888b82', '#f8f7f3') - 3.231) < 0.001);
});
