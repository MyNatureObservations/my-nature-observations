# macOS 0.2.0 RC1 — build and packaged acceptance

Base: public main `0a0339e24d67659a494b9f3d1a9e41664afa4868`.
Branch: `codex/macos-v0.2.0-rc1`. The exact clean source checkpoint, finished DMG
path and checksum are reported after the build; this document does not claim a
candidate has passed manual acceptance. [Draft bilingual release notes](MNO_macOS_0.2.0_Release_Notes.md).

## Build identity

- App version 0.2.0, Apple Silicon `aarch64-apple-darwin`, macOS 13.0 minimum.
- Production identifier remains `org.mynatureobservations.app`; default features.
- CI tools: Node 26.12.1, Rust 1.99.1 and cargo-about 0.9.3.
- Production command from `app/`: `npm run tauri build -- --target aarch64-apple-darwin --bundles dmg --ci`.
  Set `MACOSX_DEPLOYMENT_TARGET=13.0`; use ad hoc signing and no notarization credentials.
- Standard output: `app/src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/My Nature Observations_0.2.0_aarch64.dmg`.
- Generate notices through the existing before-build hook. Inspect the finished
  mounted app, not only the staging app: Info.plist, all Mach-O files, signature,
  offline MIT content and generated/package notices comparison. Verify the DMG
  with `hdiutil verify` and record SHA-256. A notarization lookup is separate from
  ad hoc signature integrity; neither an ad hoc signature nor a build proves notarization.

No Windows/Linux package is delivered and their bird-feature gates are unchanged.
Do not rebuild this artifact after acceptance without treating it as a new candidate.

## Isolated production-app manual procedure — pending

Use three **separate disposable standard macOS accounts**, for example
`MNO-020-Source`, `MNO-020-Restore` and `MNO-020-Import`. Do not use the owner's
normal account or production journal. Do not run acceptance through `tauri dev`.

Each production app uses that account's own
`~/Library/Application Support/org.mynatureobservations.app/mno.sqlite`.
`MNO_TEST_DATA_DIR` is ignored for the production identifier and is not an
isolation mechanism. Separate accounts provide the separation here. For a genuinely
clean target, use a new account; do not delete an existing journal to reset it.

### 1. Published v0.1.1 source fixture

1. In `MNO-020-Source`, download the published
   [v0.1.1 DMG and its manifest](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.1).
   Verify its SHA-256. The accepted v0.1.1 DMG digest is
   `5f7c3de52f4b3cdafc87060853c12f3c56c3bfc85ecd4bdd3b8591a438979529`.
2. Create `~/Applications` if needed and copy the app **there**, rather than
   replacing an app in shared `/Applications`. Launch this exact copy using Finder
   from `~/Applications`; quit any other MNO instance first.
3. Create a synthetic journal entry with recognizable text and a disposable photo
   with caption. Add a synthetic place/journal weather if convenient. Save, quit,
   reopen the same app and verify the entry/photo.
4. Through Settings, create a backup outside app data, e.g. under
   `~/Documents/MNO-020-Acceptance`. Retain it unchanged. Quit MNO. This is the
   pre-upgrade backup; it must remain available independently of the upgraded app.

### 2. Upgrade with the exact candidate

1. Verify the RC1 DMG against the candidate SHA-256 reported with this build.
   Mount it and replace only `MNO-020-Source`'s `~/Applications` copy. Do not rebuild.
2. Launch that copy and confirm About reports **0.2.0**. Check the earlier text,
   photo/caption, place/weather if used, and another edit saved across quit/reopen.
3. Verify the independent original database exists under the profile's
   `backups/bird-schema-upgrade/<operation-id>/schema4.sqlite` and remains after
   another restart. Read-only SQLite inspection, only while MNO is quit, can confirm
   the active file's `PRAGMA user_version` is 12. Never edit these databases.
4. Do not launch v0.1.1 against this upgraded profile. Restoring an older app bundle
   does not downgrade schema 12. Keep the pre-upgrade backup and retained source.

### 3. Bird-feature smoke test in the upgraded source account

1. Create a linked visit using species/counts (including paste review), a comment,
   times, one photo/caption and manual weather. Save, reopen and edit it.
2. Create another standalone visit on a different date, optionally with fetched
   daily weather. Check source/date and today's forecast label when applicable.
   Fetching needs internet; a failed fetch is not evidence of archive data loss.
3. Link one visit to a second journal entry. Verify only one chronological visit
   item, its independent date, compact journal references and correct return.
4. Check Home/Continue, hidden-by-default sidebar, mixed/observation-only calendar
   days, inline journal photos and the shared viewer. Review both languages briefly.
   Exercise unlink and delete only on additional disposable visits, preserving the
   linked and unlinked archive fixtures.
5. Quit/reopen the same installed copy and confirm all fixture content remains.
   In About, open the MIT license and third-party notices offline.
6. Create a new **0.2.0 backup and open export** through Settings, outside app data.
   Keep the original v0.1.1 backup separate. Copy the new archives and exact RC1 DMG
   to a shared readable transfer location, leaving the source files intact.

### 4. Backup → Restore in `MNO-020-Restore`

1. Install the same verified RC1 into this account's `~/Applications`. Launch it
   normally to establish its own fresh production journal; confirm About 0.2.0.
2. Use Settings → Restore to choose the **0.2.0 backup**, prepare it and follow the
   instruction to quit/reopen the same app/account. Complete **Verify and open
   journal** when offered; do not bypass the production Recovery screen.
3. Check linked and unlinked visits, counts/comments, captions/previews, weather
   source/date/forecast state, journal text/photos and both links. Originals from
   the source account may be inaccessible; stored previews should remain viewable
   without being presented as originals. Verify the shared viewer from the journal.
4. Quit/reopen again in this same account and repeat the short content check.
   Do not Restore over `MNO-020-Source`.

### 5. Export → Open Import in `MNO-020-Import`

1. Install and establish a fresh production profile using the same RC1 in this
   third account. In Settings → Open Import, choose the **0.2.0 open export**.
2. Preview/confirm through the existing UI; quit/reopen when instructed. Complete
   verification/opening through the normal production screen. This is whole-journal
   replacement, not adding observations to the existing journal.
3. Perform the same content, links and shared-viewer checks as for Restore, then
   quit/reopen again and verify persistence. Leave source and Restore target intact.

Record the exact source commit/DMG hash, macOS version, account isolation, outcomes
and any security/picker warnings. These instructions are not an acceptance result.
UX-005 and system-owned native-dialog language limitations remain documented;
catalog/region work is not part of this candidate.
