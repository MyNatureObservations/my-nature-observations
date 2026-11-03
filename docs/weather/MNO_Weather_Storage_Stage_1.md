# Weather storage, stage 1

> Historical stage-1 contract. Its schema, migration and scope descriptions below
> record that stage; they are not the current startup or archive contract. R1 now
> includes saved places, a weather default, manual editing and explicit Open-Meteo
> previews/saving. See [R1 – Places & Weather](../development/R1_Places_Weather.md)
> for current schema 3, content proof 4, archive format 4 and implementation links.

## Scope and integration contract

This stage adds persistent weather snapshots and portability, not a weather UI,
provider client, geocoder, device location or installation-specific default place.
Journal text validation, empty-entry rules and `document_version` remain unchanged.

`weather::storage::load` requires the current database generation and an existing
journal entry. It returns `Snapshot<StoredWeather>`: generation, optional original
manual/fetched JSON strings, and a weather revision. No row means revision 0.
`update` takes generation, expected weather revision, one selected source and an
optional JSON part. `None` clears only that source. The operation holds the normal
generation guard and one SQLite IMMEDIATE transaction through checked revision,
validation and commit. It never modifies the other source or the journal row.

An empty weather row is a revision tombstone. Never remove it merely because both
parts are NULL: removing it would let an old revision-0 writer recreate deleted
data. Weather revisions are separate from journal timestamps/document versions.
A schema-0 journal retained by a frozen legacy Recovery reads as absent weather.
A weather write on that schema returns `weather_schema_upgrade_requires_restart`;
the next normal startup upgrades through a private candidate. No live edit writes
migration DDL to the active database.

Missing entry, stale generation, stale revision, invalid part, resource limit and
revision overflow are errors. A weather-only entry cannot be created by this API.

Weather parts are bounded at 64 KiB of UTF-8 each. Oversized input is rejected
before opening SQLite. Direct-column metadata checks precede reading stored parts.
Each part is validated with the typed weather contract, including rejection of
unknown/duplicate fields. Storage and portable formats retain the original JSON
strings, rather than reserializing them or choosing meanings for duplicate keys.

## Data contract

- Independent optional manual and fetched snapshots; one fetched period per entry.
- Canonical Celsius, metres/second, millimetres (also snow depth), percentages
  for humidity/cloud cover, numeric weather codes. Missing values differ from 0.
- Finite metric values; nonnegative wind, precipitation and snow; percentages
  from 0 through 100; temperature no lower than absolute zero; consistent min,
  mean and max. Manual descriptions and location names are retained verbatim.
- WGS84 decimal-degree coordinates, latitude [-90,90], longitude [-180,180].
  A fetched location requires coordinates; manual location is optional. Places
  belong to each snapshot and are independent of future journal localities.
- Calendar day, or one-hour interval with RFC3339 start/end instants and explicit
  UTC offsets. Preserve the IANA-style timezone identifier independently: two
  occurrences of the same local DST hour remain distinguishable. Identifier
  syntax is validated here; a future acquisition adapter must resolve the actual
  IANA zone and check local dates/offsets against it before recording a request.
- Fetched sunrise/sunset use RFC3339 with offsets and ordered instants. Either
  may be absent, including polar conditions. Manual snapshots reject these fields.
- Provider, product/model, data kind, sampled coordinates, UTC retrieval instant,
  attribution, license and optional license URL remain part of the fetched
  snapshot. There is no automatic refresh or network dependency.

The stored weather format is 1. This contract was previously an unregistered
experiment; the new optional metrics/sun times are finalized before production
registration. Old experiment fixtures with absent optional fields still parse.

## Schema and preservation

Production schema 1 adds only `journal_weather` to the frozen journal schema:
entry ID primary key and RESTRICT foreign key, weather format 1, nullable source
JSON, and positive INTEGER revision. Startup holds the existing installation
lease before migrating 0 -> 1 in an IMMEDIATE transaction. Migration is idempotent,
checks exact supported schema before/after, and never parses or rewrites journal
rows, user dates, titles or timestamps. Unknown or partial schemas are not repaired.

The old test-only `database/schema_v1.rs` is not activated. Its strict document/date
validation would have narrowed the lossless preservation contract. Production uses
`database/weather_schema.rs` instead. The frozen schema-0 initializer/validator
remain available to the old readers and their independent fixtures.

New preservation verification accepts exactly the journal records already accepted
by open import: bounded UTF-8 TEXT metadata, INTEGER document version 1 and bounded
syntactically valid JSON objects. It preserves unusual dates, unknown document
nodes and duplicate document keys verbatim. This does not grant editing permission:
`backup::restore::document_is_editable` and the journal write gate retain the
existing strict editor contract. Invalid storage types, malformed JSON, unsupported
versions, schema changes, broken weather references and invalid weather are rejected.

## Version matrix

| Contract | Previous version | New version |
|---|---|---|
| Database | schema 0 | schema 1 |
| Weather row | experimental only | format 1 |
| Backup | v1, schema 0, original strict restore rules | v2, schema 1, preservation validation |
| Open export/import | v1, schema 0, seven exact journal fields | v2, schema 1, optional weather record on each JSONL row |
| Restore activation | v1, schema 0, strict content proof v1 | v2, schema-1 table contract, preservation proof v2 on both sides |
| Import replacement activation | v1/v2 with their original proof/transition rules | v3, schema 1, `mno-preserved-content` v2 |
| Legacy pending/completion | frozen v1 | unchanged; no new producer is connected |

Backups still contain exactly `manifest.json` and the SQLite snapshot. Snapshot
size, checksum, secure extraction, archive bounds, no-clobber publication and
durability barriers are reused. A schema-1 archive cannot masquerade as v1.

Open v2 keeps the same four fixed ZIP entries and manifest checksums. Each
`journal.jsonl` row has the original seven fields, with optional `weather`:
`format_version`, `manual_json`, `fetched_json`, `revision`. Tombstones are included.
The Markdown reading view also contains the weather record as safely fenced JSON.
No setting or external file is exported. V1 rows still reject additional fields.

`build_current` first verifies the original archive under its advertised format,
then migrates only its owned, private candidate to schema 1 and verifies it with
proof v2. It never changes the source archive or active journal during preview.
Legacy `build` retains its schema-0/proof-v1 behavior for v1 archives.

A v1 backup is validated under its original strict restore rules. During a new v2
restore into an upgraded installation, only the private SQLite writer is migrated
to schema 1. The incoming archive and its verified snapshot are untouched. Logical
proof v2 treats schema 0 as an empty weather section, so the conversion is checked
without changing journal values. Previously imported unusual entries should first
be migrated normally and then backed up as v2; v1 restore semantics are not widened.

## Content proof and Recovery

`storage_content.rs` owns preservation proof v2. It hashes a fixed domain and two
explicit named sections, `journal_entries` and `journal_weather`, with explicit
column order, BINARY ID order, SQL type tags, big-endian integer/byte lengths and
row counts. Both source strings, weather revision and every journal field are
covered. No document or weather JSON normalization occurs. An independent frozen
empty vector guards the wire definition; schema 0 and an empty schema-1 weather
table have the same logical proof. Exact schema/integrity/reference checks remain
separate prerequisites. The original strict and open-import v1 hashes are frozen.

New restore v2 uses the existing closed-candidate copy, local atomic replacement
and verified startup completion. New import v3 uses the existing parked-old/new
candidate transition, including empty installations. It can finish new or explicitly
retain previous journal/weather content. Version dispatch selects the verifier;
recognition of a marker alone never grants Ready. The existing abort/checked-close,
installation lease, generation, namespace, sidecar, archive hash and fsync rules
remain in force. Corrupted new content can never be installed; a separately verified
old restore may still be retained where the original restore flow allows it.

Established schema-0 journals are upgraded only in a private candidate under
`backups/schema-migration/<uuid>/`. Startup holds the existing DirectoryLease and
opens the original read-only. The existing SQLite backup copier builds a candidate;
its transaction applies the additive DDL. The preservation proof must match before
and after migration. A v2 safety archive is durably published and strictly restored
for proof comparison before any replacement. It contains the migrated schema with
exactly the original journal values; it does not claim to be a schema-0 archive.

All SQLite owners close with the existing checked-close policy. Mandatory file and
directory barriers precede a single same-volume rename, followed by barriers and
verification of the active file. No intermediate schema is written to the active
inode. An interruption before rename leaves the original; after rename it leaves
the verified schema-1 candidate. Restart reaches Ready through the normal startup
checks. No new Recovery contract or recovery-marker cleanup is needed because
neither complete side of this single-file publication requires journal replay.

Interrupted private candidates, including their journals, are retained unopened;
subsequent attempts use fresh UUID directories. They are never replayed or promoted
on restart. This deliberately uses extra disk space after interrupted attempts.
Unknown active journals/WAL/SHM and Recovery artifacts still block startup and are
never deleted. Old journals from versions that migrated in place remain unknown
sidecars and require the previously documented controlled recovery; this fix does
not invent retroactive ownership of them.

The narrow candidate publisher lives beside Recovery and uses the existing
macOS/Linux source ownership and durability mechanisms. It does not widen the
macOS-only Recovery activation API. Other platforms retain startup's existing
unsupported-durability gate; direct schema-0 initialization also fails closed.
The new publisher is tested on macOS; Linux execution needs its own CI coverage.
Crash tests cover copy, open migration transaction, committed migration, closed
owners, before/after rename and final verification, then require production startup
to reach Ready with all journal values unchanged. The active-sidecar rejection
regression also covers case aliases.

Physical preservation now accepts legacy metadata fields up to the frozen v1
64 MiB value capacity while retaining v2's existing 69 MiB + 1024 total row budget.
These bounds are independent of portable export's 1 MiB metadata limit. No old
reader, content-proof encoding or format version tuple changes. Backup and restore
regressions compare ID and title over 1 MiB before and after migration. Weather
bounds, UTF-8/type checks, integrity checks and foreign keys remain unchanged.

## Future modules

There is no module registry, dynamic table enumeration or parallel backup system.
The new storage proof centralizes the explicit persistent-content inventory. A
future localities/observations module needs:

1. An additive schema migration and an exact supported schema definition.
2. A new proof version adding that module's named, typed table sections, with
   frozen vectors and legacy readers retained.
3. An explicit portable format mapping plus round-trip/reference tests.
4. New accepted version tuples in the existing activation contracts and verifier
   dispatch. No redesign of leases, file swaps, staging, abort policy or Ready
   admission should be necessary for an ordinary additive table module.

Old proof v2 must never be silently extended to cover new tables. Unknown tables
are rejected rather than exported incompletely. Backup snapshots already copy the
whole database; explicit schema/content validation supplies the completeness proof.
This makes module additions bounded and reviewable while keeping future format
changes intentional.

## Verification and test isolation

All tests use temporary databases/archives or subprocess-owned directories. The
original working tree, its untracked Recovery files and real journal data are not
used as test inputs. The code is implemented in a separate clone/working branch.

Old protocol tests use `initialize_legacy_startup`, a test-only frozen initializer
through the existing startup probe. Real locks/artifact/sidecar/ownership paths
remain active. This models an installation interrupted before this upgrade rather
than deriving old protocol fixtures from the new schema creation policy. New tests
call the actual current startup/migration path.

Coverage includes exact preservation of unusual documents/dates, rollback and
idempotence, zero/missing values, independent source updates, concurrent revision
conflicts, generation conflicts, limits/overflow, DST offsets, sun times, offline
reopen, backup/export round trips, legacy formats, rehashed invalid weather, mixed
marker contracts, changed-weather proofs, and process interruptions before/after
publication, parking, installation, SQLite close and replacement. Recovery
activation remains macOS-only, as in the existing application architecture.

The documented historical `managed_target_probe` is excluded from the project's
selected CI integration targets because its old standalone POSIX-lock model has a
known failing case. It is not the production Recovery engine and is not changed by
this stage. See `docs/development/testing.md`; do not report an unrestricted Cargo
run as passing if that historical target fails.

### Initial delivery verification on macOS, 2026-10-26

- `cargo test --offline --locked --lib`: 803 passed.
- Selected CI integration targets (`database_crash`, `database_safety`,
  `descriptor_owner`, `recovery_handoff_prototype`): 19 passed.
- `cargo check --offline --locked`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `npm test`: 221 passed. `npm run check`: 0 errors, 0 warnings.
- An unrestricted run also exercised the excluded historical managed-target
  prototype: 2 passed, 4 failed with `Identity`. Running the original repository's
  unchanged cached test executable reproduced exactly the same 4 failures. The
  historical model and its assertions were not changed or included in CI claims.

### Review corrections, 2026-10-26

The two blocking findings are covered by candidate-migration crash tests and
physical backup/restore of both ID and title at 1,048,577 bytes. Negative tests
keep unknown sidecars, malformed JSON and wrong SQL types blocked. The historical
schema prototype still tests rollback on its disposable fixture; actual production
startup/Ready assertions now live in the candidate migration tests. The standalone
crash integration imports the identical frozen schema-0 initializer from its own
small source file rather than depending on the full startup publisher.

- Rust library: 806 passed; selected integrations: 19 passed.
- Frontend: 221 passed; Svelte/TypeScript: no errors or warnings; build passed.
- Formatting and whitespace checks passed.
- Independent focused static review found no remaining blockers in these fixes.
- Linux shares the supported publisher but was not executed in this macOS run.
- The historical excluded lock prototype is unchanged; its initial-delivery
  results above are not presented as newly rerun correction results.
