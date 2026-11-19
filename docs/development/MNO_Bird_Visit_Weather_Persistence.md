# Independent visit weather — persistence foundation

Base: public main `e9e5ce6810f17d81a3501d6ddd037c9228b29a90`.
Branch: `codex/mac-bird-visit-weather-persistence`. Local checkpoint; UI/network
activation remains deferred.
Product decisions are recorded in [the requirements](../requirements/Egna_fagelobservationer.md#besökets-väder--beslut-19-november-2026) and [implementation plan](../requirements/Egna_fagelobservationer_implementation.md#4-listans-eget-frivilliga-väder-och-samlad-acceptans).

## Model and boundaries

Schema 12 adds only `bird_visit_weather`, keyed by list ID and `manual`/`fetched`.
Each part stores its original JSON value, original visit context (date, place ID,
name and coordinates, optional start/end), and a sticky stale flag. Original
context does not depend on a historical place still existing. No list ID is put
into `journal_weather`. List deletion cascades to owned weather; journal deletion
only removes journal links. Existing journal text, weather and photos remain owned
by the journal.

The existing strict weather-value validator is reused. Manual values describe a
visit; fetched values must be daily and use the existing `historical-weather`
reanalysis or `forecast-daily` source contracts. Provenance, model, attribution,
retrieval time, timezone and daily aggregates are retained in the fetched JSON.
Resolution and forecast status are derived from the validated part, never guessed
from visit times. Today's data cannot be labelled completed historical weather.
Hourly and current-weather snapshots are not accepted as fetched visit weather.

Internal create/update variants use the existing list write/generation guard,
transaction and edit token. Weather is part of the token; nonempty weather is also
bound to creation receipts, retaining the old request digest for no-weather calls.
Invalid parts roll back the list, rows, links and receipts together. Late writes
must still match generation, list existence, edit token and captured context.
There is no weather upsert that recreates a missing list.

Ordinary list edits and shared-place edits invalidate weather inside their existing
transactions. Staleness is sticky: reverting date/place/times or coordinates does
not reactivate old values. Reads also compare the captured context. The current
summary excludes stale parts. Manual reuse and replacing stale manual data require
an explicit confirmation; reuse rebinds the manually confirmed values to the new
context. Fetched parts remain independent and stale until replaced/removed.

## Upgrade and archives

Ordinary macOS startup uses the existing protected candidate upgrade to schema 12,
retaining the independently verified source copy. Pending Restore/Import/Recovery
keeps its existing startup barrier. Full schema validators recognize only the exact
new layout. Existing journal-weather schema-10/11 compatibility remains, and the
same strict reader also recognizes schema 12 on macOS.

Backup format 13 uses the verified SQLite snapshot. Export format 13 adds only
`bird_visit_weather.jsonl` to the exact ZIP inventory, using existing checksums,
row counts and limits. Content proof 13 includes source JSON, context and stale
state alongside the frozen previous content proof. Import rejects invalid sources,
contexts, references, duplicate parts and malformed inventory/records before
activation. Older supported production exports (formats 5–12) are first verified
using their frozen contracts, normalized privately to schema 12 with no visit
weather, then verified again. Frozen older readers are retained.

New restore marker 13 and import markers 42–50 explicitly bind source format to
schema 12/proof 13; older marker contracts remain unchanged. Previous journals are
still protected before replacement. No second recovery authority is introduced.
Older app versions must reject the new schema/archives; they are not forward
compatible. Windows/Linux startup and feature gates remain unchanged.

## Remaining work

No visit-weather UI or Tauri creation/fetching commands are registered. No network
service path has been added. UI staging, daily-service selection (including today's
forecast), localized stale/reuse controls and macOS DEV acceptance belong to later
work. This foundation has no manual or packaged acceptance claim. Existing test
profiles, user journals and archives were not used or modified.

The internal fetched-value input is not a public command boundary. A future fetch
command must reuse the existing server-held preview/receipt pattern, bound to list
generation, edit token and visit context, rather than accepting provider JSON from
the frontend. No generic weather framework or hourly selection is introduced.

## Local validation — 19 November 2026

All Rust checks use the unchanged CI pin, **1.99.1**.

- Complete macOS library suite: **1,021 passed, 0 failed, 2 existing ignored**.
- Final strengthened visit-weather suite: **10 passed**, including atomic create,
  receipt retry/conflict/deleted-request replay, independent journal weather,
  explicit manual confirmation, sticky date/time/place staleness, coordinate-edit
  invalidation, rollback after partial weather/cascade work, expired generations
  and late writes to missing/stale lists.
- Archive coverage includes linked/unlinked/stale parts, multiple journal links,
  full format-13 Restore and Open Import activation/admission/restart, restoration
  of a schema-4 backup and normalization/activation of a format-12 export. The
  replaced journal is deliberately different and its weather is verified in the
  protection archive. Each source export format 5–12 is separately normalized and
  compared with its frozen original content proof. Rehashed invalid weather
  payloads are rejected for semantic/reference reasons, not merely checksums.
- Protected schema-11 → 12 upgrade: **2 passed**, covering interruption/retry at
  every existing publication checkpoint, exact retained source bytes, original
  journal/photo/weather content and invalid-candidate rejection.
- Affected weather filter: **83 passed, 1 existing ignored**. The final stronger
  schema-10/11 compatibility regression also passed separately. Existing photo
  suite: **5 passed**; current overview archive regression: **1 passed**.
- All-targets compilation and pinned formatting passed. No frontend code changed,
  so frontend/Svelte tests were not repeated. Documentation links and
  `git diff --check` passed.

The complete suite was run before the last test-only strengthening; those changed
tests were then rerun successfully. The final place regression also explicitly
passed switching to a different place ID at identical coordinates, retaining the
original context and stale status. The first broad run exposed current-schema
expectations still pinned to schema 11 and two unchanged subprocess race failures
(`startup_success_and_retry_matrix`, `early_pending_validation_keeps_live_b_c_locks`).
Current expectations were updated only after identifying their ordinary-startup
fixtures; frozen older contracts were retained explicitly. Both subprocess tests
passed in isolation, and the final full run passed without relaxing assertions.
No root cause for the initial subprocess races is claimed.

Native Windows/Linux tests and manual macOS visit-weather acceptance were not run.
The UI/network gates remain closed; the existing synthetic user profile was not
opened. The next ordinary macOS startup with this branch upgrades an eligible
profile to schema 12 through the protected path, so any further DEV review should
continue using disposable data, not the production journal.

## Review correction — numeric coordinates

Review found that comparing raw JSON numbers rejected valid integer coordinates
such as `59` when the captured context serialized the same f64 value as `59.0`.
The comparison now uses the validated numeric values, with exact f64 equality and
no rounding or geographic tolerance. Existing finite-number, range, type and
required-field checks still run before comparison. Schema 12, archive format 13
and the stored JSON/proof contracts are unchanged.

After this correction, **11 focused visit-weather tests passed** with CI-pinned
Rust 1.99.1. New cases cover integer/decimal/mixed forms for manual and fetched
weather, tiny genuine differences, missing/non-numeric coordinates, out-of-range
and non-finite values. The production guarded creation and Backup/Restore plus
Export/Open Import candidate paths preserve integer-form coordinates unchanged.
Rehashed export payloads with differing or invalid coordinates are rejected without
changing the active journal. All-targets compilation, pinned formatting and
`git diff --check` passed again; unaffected full-suite evidence above was reused.

The reviewed coordinate blocker is resolved. No further concrete correctness or
data-preservation blocker was identified. Test profiles and existing archives were
not opened or modified; no visit-weather UI or network path was activated.
