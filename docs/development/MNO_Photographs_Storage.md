# Photographs: storage foundation

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

This report records Stage 1. The subsequent editor/reader integration is documented in [MNO_Photographs_Stage_2A.md](MNO_Photographs_Stage_2A.md).

This implements the technical storage stage requested after the Stage 1 proposal. No image insertion controls, editor integration, rendering, CSS, gallery or cleanup are enabled. The existing text editor continues to protect document v2 from editing with its v1 schema.

## Contract and architectural choices

- Document v1 is unchanged. Document v2 adds a top-level atomic `journalImage` block with canonical UUID-v4 `nodeId` and `assetId`, required `placement` (`left|right|row`) and `size` (`small|large`), and optional independent `caption` and `alt` strings. Captions belong to nodes; different nodes can share one asset without sharing captions. The backend validates identifiers, structure, duplicate node IDs and ownership. The schema-only TipTap extension lives in `journal-image.ts` and is deliberately not registered in the editor yet.
- One `journal_image_assets` table holds an immutable asset ID, owning entry ID, complete original path, preview MIME, preview dimensions and preview BLOB. There is no derived node index, content-addressed identity, orphan collector or external preview directory. Nodes refer directly to assets; the backend checks ownership without loading BLOBs for each reference.
- `save_journal_with_photographs` saves document v2 and new resources in one SQLite transaction. It validates the existing document before replacement, inserts assets, then checks all references before commit. Failure rolls back everything. Identical assets can be resubmitted; reusing an ID with different values fails. Assets remain after node removal and can be reused by subsequent saves. Original paths and image bytes are excluded from the TipTap document.
- `prepare_journal_photograph`, `save_journal_with_photographs` and `get_journal_photograph` use the existing database operation/generation guard. These are backend building blocks, not a new UI flow. Future UI callers must retain the generation and discard late preparation results after changing editor/entry; no UI job lifecycle machinery is introduced here.
- Preparation opens only a read-only original file handle, detects JPEG/PNG from bytes, applies orientation to decoded pixels and re-encodes one preview without copying EXIF metadata. It never modifies, moves or removes originals. Preview validation, reading, backup, import and Recovery never open the stored original path. Paths from another platform remain inert metadata; no relinking is attempted.
- Preparation keeps aspect ratio, never upscales or crops, and starts at a maximum 2500-pixel longest side. JPEG uses quality 85, PNG preserves alpha. If encoded output exceeds 4 MiB, dimensions are reduced by 25% and encoding retried until it fits. This is a simple size safeguard, not a quota system.
- Defensive input limits are 64 MiB source bytes, 60 million decoded pixels, 32768 pixels per source side, and the decoder's 512 MiB allocation limit. Preview dimensions are at most 2500 per side and stored bytes at most 4 MiB. Metadata bounds are 32 KiB original path, 16 KiB each caption/alt, and existing journal bounds. The decoder allocation setting is not a promise of a process-wide peak-memory cap. No total book quota or cleanup is added.

Example document v2:

```json
{"type":"doc","content":[{"type":"journalImage","attrs":{"nodeId":"f07c5922-8ebd-4d83-b869-c2e30e6b5e52","assetId":"1ae7ba86-808a-4899-8e8c-3020483d1012","placement":"left","size":"small","caption":"A robin by the path","alt":"Robin on a branch"}}]}
```

## Persistence, proofs and portability

| Contract | Current value | Reason |
|---|---|---|
| Journal document | 1 unchanged; new 2 | New atomic image node and node metadata |
| SQLite schema | 4 (was 3) | New per-entry asset table |
| Full preservation proof | 5 (was 4) | Commits all prior journal/weather/place/default values plus typed, length-prefixed preview BLOBs and image metadata |
| Physical backup | 5 (was 4) | Exact schema-4 snapshot with image data |
| Open export/import | 5 (was 4) | Adds `images.jsonl`; manifest declares document capacity 2 |
| Restore activation marker | 5 (was 4) | Binds schema 4, backup 5 and old/new content proof 5 |
| Import activation marker | 6 (was 5) | Binds schema 4, export 5 and old/new content proof 5 |
| Legacy journal proof/pending/completion; weather/place data formats | Unchanged | Separate existing contracts; not used as proof of photograph preservation |

Open export uses a single additional, ordered `images.jsonl` payload containing asset metadata and base64-encoded preview bytes. This intentionally replaces the proposal's per-image ZIP entries and derived node file: a fixed eight-entry archive fits the existing importer, keeps the manifest small and avoids another index. It is an open, documented encoding; base64 costs approximately one third more than binary storage. Every asset is exported, including unused assets. Import verifies archive inventory, payload checksum/size/count, unique ordered asset IDs, image decoding/metadata and document references before activation. Physical backups carry the SQLite BLOBs directly. Both paths preserve previews byte-for-byte.

The Markdown view remains a text reading/fallback representation; image display is outside this stage. JSONL is authoritative. Original files are never embedded or followed. The open archive README explicitly states that complete original paths are included and that the archive is unencrypted.

The existing activation, protection-backup, running verification and startup Recovery chain is reused. The new proof is checked during preparation, after copying and before Recovery admission. Missing originals do not affect any proof. Existing weather, place and weather-default semantic validation is shared with the previous proof; its frozen public v3/v4 functions keep their old behavior.

No established development database is upgraded in place. Startup requires schema 4 and leaves older databases untouched with `database_schema_upgrade_required`. Current production import/Restore accepts format 5. Existing lower-level historical readers and test protocols remain where already present, but no migration from historical archives into the current schema is added. Synthetic development profiles must be recreated; their old archives are not a migration path for this stage. This follows the requested pre-v0.1 simplification rather than the proposal's extensive compatibility matrix.

## Changed areas

| Area | Files |
|---|---|
| Photograph implementation and tests | `app/src-tauri/src/photographs.rs`, `photographs/tests.rs` |
| Node schema and frontend test registration | `app/src/lib/journal-image.ts`, `app/tests/journal-image.test.mjs`, `app/package.json` |
| Dependencies and command registration | `app/src-tauri/Cargo.toml`, `Cargo.lock`, `src/lib.rs` |
| Schema and existing features | `src/database.rs`, `src/place_registry.rs`, `src/weather/storage.rs` |
| Preservation/portability | `src/storage_content.rs`, `src/backup.rs`, `src/backup/restore.rs`, `src/open_export.rs`, `src/open_import.rs`, `src/open_import/v3.rs` |
| Activation and app entry points | `src/backup/restore_context/activation.rs`, `activation/importing.rs`, `activation/startup.rs`, `activation/marker_contract/preparation.rs`, `src/import_app.rs`, `src/restore_app.rs` |
| Existing tests updated for the current contract | Tests in backup, open_export, export_app, import_app, restore_app, startup, place_registry, database/schema_v1, weather/historical/saving; current-startup test in `manual_backup.rs` |
| Documentation | This report and the prior Stage 1 proposal, marked as superseded where decisions differ |

Paths beginning `src/` in the table are relative to `app/src-tauri/`.

## Synthetic measurements

Measured locally in the Rust debug/test build, macOS, Rust 1.99.1. Source fixtures are generated procedurally, not personal photographs. Timings use `Instant` around preparation and the SQLite save including validation; source-fixture generation time is excluded. The database stores exactly the reported preview BLOB bytes, without base64 overhead. These are observations from one run, not performance guarantees.

| Synthetic source | Source bytes | Preview | Stored preview bytes | Preparation | Save |
|---|---:|---|---:|---:|---:|
| Smooth PNG, 640×480 | 346,156 | 640×480 | 346,156 | 133 ms | 22 ms |
| Landscape JPEG, 4000×3000 | 1,749,935 | 2500×1875 | 821,969 | 4,442 ms | 341 ms |
| Noisy large JPEG, 6000×4000 | 44,800,862 | 2500×1667 | 3,171,665 | 10,742 ms | 681 ms |
| Noisy transparent PNG, 3200×2400 | 30,724,808 | 1054×790 | 2,267,782 | 8,982 ms | 77 ms |

The large source fits the 64 MiB safeguard. High-entropy PNG with transparency needs more reduction than ordinary photographs to meet 4 MiB. The coarse reduction step leaves headroom below the limit, deliberately avoiding an optimization/search subsystem. All cases remained within aspect-ratio rounding tolerance, did not upscale, and left original bytes unchanged. SQLite persistence and archive tests compare bytes exactly. Future UI preparation should run away from the UI thread; release-build latency has not been measured.

The measurement test is explicitly marked manual (`cargo test --locked --lib photographs::tests::preparation_measurements -- --ignored --nocapture`) to keep repeated CI runs from regenerating large synthetic source files. It was actually executed, not inferred from another run.

## Verification

Local verification completed: Tests use temporary databases, isolated synthetic profiles and subprocesses; no personal databases or synced `sources/` files are touched. The historical managed-target probe remains excluded exactly as specified in `testing.md`; unrestricted `cargo test` is not claimed.


| Check | Actual result |
|---|---|
| `npm ci`, `npm run i18n:compile` | Completed; compilation used the configured online plugin |
| `npm run check` | 0 errors, 0 warnings |
| `npm test` | 356 passed, 0 failed |
| `npm run build` | Passed |
| `cargo test --locked --lib` | 898 passed, 0 failed, 2 explicitly ignored (manual measurements and the pre-existing live HTTPS forecast probe) |
| `cargo test --locked --lib photographs` after the final test refinements/addition | 9 passed, 0 failed, 1 manual measurement test ignored; includes the additional automatic PNG resize/4-MiB case |
| Manual synthetic image measurement test | Actually executed: all four cases passed; dimensions, sizes and times recorded above |
| Required integration targets | `database_crash`: 3 passed; `database_safety`: 1 passed; `descriptor_owner`: 10 passed; `recovery_handoff_prototype`: 4 passed |
| `cargo fmt --all -- --check`, `git diff --check` | Passed |

Tools: Node 26.13.0 (CI pins 26.12.1), npm 11.20.0, Rust 1.99.1, macOS arm64. Initial local runs exposed version-expectation failures and a missing new-proof dispatch; these were corrected and rerun. Network-restricted execution initially prevented the existing local HTTP fixtures; the successful full library run allowed those isolated fixtures. No old run is presented as a new result.

The additional final PNG test and strengthened metadata-hash assertion were run in the focused suite after the full suite; no production code changed after the successful full suite. The historical standalone managed-target probe is not among the selected integration targets and is not claimed to pass.

Branch: `feature/photograph-storage-stage1`, based on main `1d2adc06c700ec84fc2937d1f4be955c7cbbccdb`. The PR triggers the existing macOS CI workflow. Its result is deliberately left for user monitoring; no CI success is claimed here.
