# Weather Bento stage 2A

Weather is a read-only sibling of the TipTap host, outside the editable DOM. The
entry contains the float with `flow-root`; the host and TipTap retain normal block
flow so text uses the width beside the tile and the full width below it. The
chronological reading renderer uses the same tile. Narrow windows stack it above
the text. Panel positioning never changes the float's dimensions.

## Native WebKit acceptance test

Use a separately identified debug app with both `appDirectoriesOverride` and
`MNO_TEST_DATA_DIR` pointing to a new synthetic-only directory. Never start the
ordinary application or copy real journals for this test. Keep artifacts.

1. Use two long paragraphs with Swedish characters and emoji, without a heading.
   Verify four or more text lines beside the tile, then full-width lines below.
2. Click beside the tile, move the caret across its bottom boundary, and extend a
   selection across that boundary with Shift+Down. Paste Swedish text and emoji
   using the native clipboard. Verify normal replacement and continued editing.
3. Toggle StarterKit bullet lists in the first paragraph. Verify wrapping beside
   and below the tile. Check the plain reading renderer too.
4. Open and close the panel; compare text range rectangles before and after.
   They must be identical. Check Escape, focus return, Tab departure and outside
   clicks without losing journal text.
5. Check empty/manual/fetched/both sources, loading and failure. Zero must display
   as zero; an absent measurement must not become zero. Source timestamps retain
   their recorded offsets and periods, even when the journal date differs.
6. At a window width below 600px, the tile stacks above the text and the panel
   remains within the viewport. Verify Swedish and English.
7. Compare saved journal JSON before and after opening/closing weather. There
   must be no document or weather writes from these interactions.

The initial native WKWebView probe used the installed TipTap core and StarterKit,
normal nested blocks, an external CSS float and native keyboard/clipboard input.
It required no editor extension, node, schema change or input override. Its range
measurements passed in editing and plain reading modes; panel opening produced
no text reflow. The probe's paste observer returned `false`, leaving normal
TipTap handling in control. Production UI verification is recorded in the delivery
report together with test logs; Linux rendering remains a separate platform check.

## Automated checks

`npm test`, `npm run check`, `npm run build`, and the Rust library suite. Weather
regressions cover source independence, zero versus missing, escaping, translation,
source metadata, generation checking, bounded batch reads, read-only SQLite,
legacy schema reads, invalid weather rejection and exact 64-bit revision strings.
No weather mutation command is registered. Backup/Recovery formats are unchanged.

## Verified on 2026-10-26

The production frontend and Rust read command were exercised together in a
separate native WKWebView app, with four synthetic entries (empty, manual,
fetched, both) and an i64::MAX revision. Swedish and English summaries matched
storage. Details retained zero coordinates, zero precipitation, UTC retrieval
time and sun times with their original offset. The weather period deliberately
differed from the journal date and was displayed unchanged.

In both the chronological renderer and the editor, text flowed left of the tile
and continued below it. The editor retains its existing toolbar, minimum height,
editable background/padding and list styling; the reading renderer retains its
existing paragraph/list styles. These are intentional existing mode differences.
Opening the panel did not move the underlying text in either view. Escape
returned focus to the trigger; Enter reopened it; Tab moved to the next edit link
and dismissed it; clicking the editable text dismissed it and placed the caret.

A second separately identified app at an initial width of 560 CSS pixels verified
stacking and a viewport-contained scrollable panel in both views. Both synthetic
database files remained byte-identical to their seeded baseline after all panel,
focus, mode and navigation interactions. Test apps were stopped and artifacts
retained. No weather network or geolocation code was added.

Local checks: 227 frontend tests, 808 Rust library tests and 18 selected Rust
integration tests passed; typecheck and production frontend build passed;
rustfmt and whitespace checks passed. Linux UI and VoiceOver were not executed.
The historical CI-excluded locking prototype was not rerun or changed.

## Visual refinement after acceptance

Ready weather with neither source renders no tile in reading mode, so it takes
no float or vertical space. Editing mode passes an explicit `editing` prop and
shows a compact cloud / Weather button instead. The neutral label and read-only
hint do not advertise an implemented add-weather action. Loading and error states
remain distinct from confirmed absence. Switching mode resets the open panel but
does not restart weather loading or change the editor document.

`WeatherDetails` presents temperature, description / condition, wind and
precipitation first, excluding missing measurements while retaining zero values.
Each source has a native, initially collapsed `details` / `summary` for period,
location, exact condition code, timezone, provider metadata and remaining fields.
Condition names are coarse display-only categories from the [documented WMO
codes](https://open-meteo.com/en/docs#weather-variable-documentation); the exact
code remains visible in details. All original values remain stored unchanged.

Regressions render the actual details component and verify ordering, collapsed
metadata markup, zero versus missing, source-value preservation, escaping and
both languages. Empty weather is checked in both modes, including a tombstone
revision; read failures are still visible. These checks extend the frontend suite
to 230 tests. Native focus / layout checks are recorded with the delivery.

The refined release builds were verified in native WebKit at 1050px and 560px.
The no-weather reading entry had no weather button and used the full text width;
editing showed only the small cloud / Weather control. The empty panel stated
that weather is read-only. Temperature, conditions, zero wind/precipitation and
separate sources preceded the collapsed metadata. Tab + Space opened each
source's details; Escape closed the dialog and returned focus. Swedish and
English were checked. Both synthetic SQLite files remained byte-identical after
mode, panel, disclosure and language interactions. Original data, main and the
four protected untracked files were unchanged. VoiceOver/Linux UI remain untested.

## Icon Bento and manual-first overview

`WeatherIcon` is a local inline SVG component in the same stroke style as the
existing controls; no icon library or design framework was added. The overview
uses only explicit condition codes for sun/cloud/rain/snow/fog/thunder icons.
Missing or unknown codes remain neutral. Temperature, precipitation, snow depth
and cloud percentage never infer a condition.

Overview fields select manual values first, then the corresponding fetched field
(temperatureC/temperatureMeanC, windSpeedMs/windSpeedMaxMs,
precipitationMm/precipitationSumMm, cloudCoverPercent). Null checks retain zero.
Each selected field retains its origin; fetched-only overviews use one meteo tag,
while mixed overviews identify fetched complements individually. A differing
fetched mean temperature appears as a secondary meteo value. Both original
sources are still presented independently in the panel and remain unchanged.

Manual free text without a condition code cannot safely select an icon. It stays
primary with a neutral icon, rather than guessing from prose or displaying a
potentially contradictory provider condition. When an explicit code exists,
the icon's accessible description follows that code; prose is preserved in the
panel. Sunrise/sunset use dedicated icons and the recorded wall-clock portion,
with full original timestamps retained in details. No machine-timezone conversion.

The overview keeps the existing 168x110 float and 74px stacked mobile footprint.
Metric values and their units use separate compact lines to fit three ordinary
measurements without truncating units. Labels remain available to assistive
technology and via tooltips; the panel also keeps visible field labels and
separate source headings. Reading/editing no-weather behavior is unchanged.

Regressions cover each icon category, manual/fetched/mixed sources, conflicting
conditions and temperatures, manual and fetched zero values, missing temperature,
manual prose, unknown conditions, source preservation and recorded sun times.

Native release acceptance for the icon presentation used 11 synthetic cases in
two independent databases (normal 1050px and narrow 560px). Manual rain + 0°C
won over provider sun + 12°C; the provider temperature stayed a meteo complement.
Sun, partly cloudy, cloud, rain, snow, fog and neutral views loaded correctly.
Missing temperature stayed absent. Native inspection found ordinary metric units
could initially truncate; splitting values and units corrected that without
changing the float dimensions. Final normal/narrow layouts were checked again.
The panel kept source separation, icons with visible field labels, recorded sun
clocks, collapsed metadata and working Tab/Space/Escape focus behavior. English
and Swedish and editing/reading were checked. Both SQLite files stayed exactly
byte-identical after UI testing. The final frontend suite has 238 passing tests,
with clean typecheck/build. VoiceOver and Linux UI are still not executed.

## Wide source panel

The overlay is up to 620px wide, right-aligned with the tile and clamped to a
16px viewport margin. Opening measures its actual rendered height before choosing
below, above, or a clamped position. Its remaining viewport height constrains
scrolling even after metadata is expanded. It remains fixed outside document
flow; the tile and editor layout are unchanged.

Manual and fetched parts appear in independent columns, headed Own observations
and meteo. A single source fills the available width; below 640px the sources
stack. Each part has its own explicit-code icon and temperature, measurement
grid, recorded sun clocks, and collapsed technical details. No source values
are combined in the panel. Overview priority and all storage remain unchanged.

Regression coverage includes viewport edges/low height, independent source heroes,
missing and zero values, sun placement, timestamps and both languages. Native
acceptance and current CI results are recorded in the delivery report.
