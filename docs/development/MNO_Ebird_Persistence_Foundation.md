# Protected eBird persistence foundation — internal macOS work

Base: public main `a24fb8b27948a911087c4d3351cc734aeaf61b80`.
Branch: `codex/ebird-persistence-foundation`. Local, uncommitted implementation.
[Requirements](../requirements/Import_fran_eBird.md).

## Ownership and identity

Schema **13** adds four source-specific tables; it does not reinterpret manual
`bird_lists`, invent catalog concepts, or attach source snapshots to MNO places.

- `ebird_visits`: opaque local UUID, unique Submission ID in the implicit eBird
  namespace, and the current source-owned visit/place metadata snapshot.
- `ebird_rows`: opaque local row UUID, visit, source ordering, active status and
  complete source fields. `Count` remains its exact decimal string or explicit
  `X` (unspecified); it is never coerced to zero/one. Common/scientific names and
  all Macaulay IDs remain source values, without a guessed AviList identity or
  taxonomy version. Unknown extra CSV columns are retained too.
- `ebird_additions`: separate MNO-authored plain text, owned by the visit and
  optionally referring to one preserved source row. This is a minimal internal
  preservation anchor, not an exposed editing/attachment feature.
- `journal_ebird_visits`: separate journal links. Journal deletion removes its
  links, not visits, source rows or additions. No imported journal entry is created.

Location ID/name/coordinates belong to each visit snapshot. There is no automatic
name/proximity association, global source-location merge or update to user-owned
`places`. Source snapshots are locked against ordinary manual bird edit/delete
paths by separate ownership/tables, not merely by disabled frontend controls.

## Bounded row matching and reimport

The CSV has neither stable row IDs nor an explicit taxonomy version. A scientific
name, common name or taxonomic order is **not** treated as a permanent identity.
Match exact unchanged complete source-field maps first. Then consider only the
remaining active old rows and incoming rows within the same Submission ID. A
correction retains its local row UUID only when the remaining match is uniquely
one-to-one and all fields except these three remain exactly unchanged:

- `Count` — the numeric count or explicit X.
- `Observation Details` — the source observation comment, not checklist comments.
- `ML Catalog Numbers` — the complete source media-reference field.

There is no case folding, name substitution or numeric normalization in this
continuity comparison. All other source fields, including common/scientific
names, taxonomic order, visit metadata and unknown extra columns, must be equal.
Taxonomic order is not used as an identity; a change is conservatively excluded.
No taxonomy version is inferred. Exact matching runs first, and neither phase
resolves ambiguity by row order. Reordering unchanged rows changes positions only.
Source values remain exact; field maps are canonical JSON (sorted keys), not the
original CSV quoting/BOM/column order/line endings.

Accepted count/comment/media corrections update the source snapshot under the
same row UUID; MNO additions remain attached to it. This is bounded source-row
continuity, not permanent taxon identity or a claim of taxonomic equivalence.
Other changed or disappearing rows become inactive and remain stored. Incoming
unmatched rows receive fresh UUIDs. Their old additions remain on the retained
rows for review; nothing is guessed, merged or transferred. Inactive rows never
auto-reactivate. Duplicate taxon input still rejects the batch under existing
strict validation for explicit review; it is not silently accepted as new data.

Missing visits in a later export are retained. Identical input leaves content,
identities and links unchanged. Duplicate visit IDs, duplicate scientific labels
within a visit, conflicting metadata and invalid values reject the entire batch.
These are validation conflicts, not invented matching decisions. Counts remain
bounded positive i64 integers or X. Optional trailing fields can remain absent;
metadata comparison accepts equivalent numeric/time spellings without rounding
away different coordinates, while preserving the original strings in snapshots.

`ebird_persistence::import` uses the module permit, search write serialization,
expected generation permit and one SQLite transaction, followed by complete
schema/content/FK verification before commit. A replaced generation rejects late
requests; normal bird operations cannot upsert an imported source visit. There is
no persistent creation-ID reservation or user-facing save command for this work.

## Migration, proofs and archives

The protected candidate mechanism supports schema 12 → 13 (and earlier supported
sources): immutable source verification, SQLite copy, additive upgrade, old-content
proof comparison, independent retained source, barriers, single publication rename,
post-publication verification and safe retry. The schema-12 validator and proof
remain frozen; schema 13 has its own exact schema validator and content proof **14**.

Backup format **14** carries schema 13. Open-export format **14** adds `ebird.jsonl`
to the prior exact inventory, including active/inactive rows, additions and links.
Checksums, byte sizes, row counts, references, canonical snapshot JSON and content
proofs are checked before activation. Photos/previews/captions, journal text,
manual birds, places and both journal/visit weather retain their existing formats.
Original photo files are never copied or modified by this feature.

Open Import normalizes supported modern formats 5–13 into private schema-13
candidates after verifying their original frozen contracts, then verifies preserved
old content and the new proof. Format 14 is verified directly. Existing legacy
readers remain present; no new cross-schema combinations for formats 1–4 are added.
Schema-13 Restore/Import activation and Recovery bind schema/proof/source-format
explicitly, retain the replaced journal in protection, and reopen through startup.
Older Restore sources retain their old schema until the supported startup upgrade;
there is no migration underneath unfinished Recovery. New restore markers use
version 14; import markers 51–60 bind source formats 5–14 to resulting schema 13.
Older marker bindings are unchanged.

Module state is outside the journal and irrelevant to complete archive inventory,
validation and preservation. Module-disabled archives still contain all eBird data.
Windows/Linux startup and activation gates remain unchanged; schema 13 is admitted
by macOS only. Journal weather explicitly validates schema 13, avoiding an implicit
future-version allowance.

## Activation boundary and next work

**Ordinary startup still targets schema 12.** The protected schema-13 initializer
is internal and is exercised under the startup lease by synthetic tests. It is not
connected to ordinary startup. Existing schema-13 journals can safely reopen, but
schema-12 targets do not gain a new format-14 Open Import combination in this step.
The read-only eBird preview still has no persistent save command or UI action.
No network/media fetching, place association UI or new module activation is added.

Before enabling import writes: review this foundation; connect the protected
schema-13 upgrade to ordinary macOS startup (including post-Recovery upgrade), then
build the confirmed preview-to-save/reimport comparison and unresolved-addition
review. Source parsing must reach the validated internal contract; current preview
name matches must never be promoted into catalog identities. Manual macOS DEV and
packaged acceptance are still required. No existing profile/export was opened or
changed for these tests, and no personal CSV is copied into fixtures.

## Validation

Synthetic production-path tests cover atomic first/repeat import, unchanged byte
content on identical input, changed/missing rows and visits, preserved additions
and multiple journal links, rollback, invalid input, module/generation guards,
manual ownership separation, archive tampering with recomputed checksums,
normalization of every modern format 5–13, complete archive activation/restart and
replaced-journal protection. The shared fixture also contains journal text/photo/
caption/weather and a manual bird list with comment/photo/caption/visit weather.
Protected migration tests inject failure at eight publication boundaries, compare
the independent schema-12 source byte-for-byte, retry, reject a changed candidate
and verify ordinary startup has not silently activated schema 13.

Local results with CI-pinned Rust **1.99.1**:

- `cargo test --locked --lib ebird`: **15 passed**, no failures (includes the
  three existing read-only preview tests and twelve foundation/upgrade tests).
- Complete macOS library suite: **1,052 passed**, **2 existing tests ignored**,
  no failures. The final manual-edit assertion added afterward is covered by a
  targeted rerun rather than repeating the entire suite.
- `cargo check --locked --all-targets`, CI-pinned formatting and
  `git diff --check`: **passed** after the final adjustment.
- Three changed-document links resolve, including the referenced heading anchor.
- No frontend contract/component was changed, so Svelte was not rerun.

These are automated synthetic tests, not manual DEV or packaged acceptance.
No CI, push, PR, manual acceptance or publication is part of this step.

## Schema-13 application-boundary correction

The pre-activation review identified three omissions above the passing internal
persistence/archive tests. Ordinary Restore selection now explicitly accepts
format 14/schema 13 and retains full backup/content verification. Bird-module
availability and ordinary create/edit use the strict schema-12-or-13 capability
helper; disabled module and platform guards remain in place. Visit-weather
preview binding and receipt revalidation use the same strict helper, preserving
context, expiry, owner and generation checks.

The adjacent shared-place update path had the same exact-schema-12 omission:
coordinate changes now invalidate preserved visit weather on schema 13 too.
Frozen schema-12/archive contracts are unchanged; arbitrary future versions and
structurally invalid databases are still rejected.

Additional CI-pinned Rust 1.99.1 evidence for this correction:

- Five new schema-13 tests passed, covering ordinary Restore preparation →
  activation → Recovery completion → reopening; invalid/future backups;
  enabled/disabled module availability and ordinary create/edit; deterministic
  preview/creation receipt application and schema rejection; shared-place
  coordinate changes preserving weather as stale.
- Relevant suites passed: `bird_app::` **10**, `bird_weather_fetch::` **7**,
  `restore_app::` **13**, `place_registry::` **15** (**45 total**, including
  the five new tests). Existing weather edit-receipt cases now run on both
  schemas 12 and 13: success, expiry, changed place, deleted list and generation.
- All-targets compilation, CI-pinned formatting and `git diff --check` passed.
  Frontend contracts did not change; no Svelte rerun was necessary.

These checks supplement the earlier complete library result; that full suite was
not rerun for this focused correction. Ordinary startup remains at schema 12,
import writing remains unregistered/unavailable and row matching is unchanged.
No manual or packaged acceptance is claimed. The next steps remain review,
protected startup integration and isolated acceptance before import activation.

## Approved bounded correction continuity

The owner approved continuity for exactly the three fields above. No schema or
archive-version change is needed: source-row UUID and its independent addition
references already support it. Tests explicitly cover exact-first matching,
one-to-one ambiguity refusal, count/comment/media corrections, identical reimport,
reordering, removed/name-changed rows, rollback after a correction and archive
preservation of corrected row IDs with their additions/links.

Photo coverage is bounded by the implemented model: eBird additions are currently
plain text anchors, with no imported-row photo attachment API/table. Tests prove
existing manual bird-photo preview/caption and original-file bytes remain unchanged,
not that imported-row photo additions already exist or have been accepted. Those
attachments remain future work; no new photo model is introduced here.

Continuity checkpoint validation (Rust 1.99.1): **18 focused eBird tests passed**,
including migration, archives, exact-first matching, ambiguous-candidate refusal,
strict duplicate rollback, count/comment/media continuity, removed/name-changed
rows, transaction rollback, unchanged original/manual-photo bytes and no-op imports.
All-targets compilation, formatting and whitespace checks passed. The earlier
**45 app-boundary tests** and **1,052-test full library result (two ignored)** are
reused for unaffected paths; they are not represented as reruns after continuity.
Ordinary startup remains schema 12. Internal schema 13/archive 14 are unchanged,
import saving is still unavailable, and no profile/personal export was accessed.
