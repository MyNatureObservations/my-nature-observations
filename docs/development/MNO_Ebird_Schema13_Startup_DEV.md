# Protected schema-13 ordinary macOS startup — DEV acceptance

Starting checkpoint: `fb4a57a786220b95bb199e69adb2967dd8564245`.
Branch: `codex/ebird-persistence-foundation`. Automated integration is recorded in
[the persistence report](MNO_Ebird_Persistence_Foundation.md). The project owner accepted the DEV upgrade checklist below at the tested source
checkpoint recorded at the end. Automated checks used disposable synthetic data;
this document does not claim packaged or new manual archive acceptance.

Ordinary macOS startup now calls the protected schema-13 initializer only after
the directory lease and existing Recovery/artifact checkpoints. New journals use
the supported schema-4 bootstrap followed by the same protected upgrade. Established
schema-12 journals retain an independent `schema12.sqlite` under
`backups/bird-schema-upgrade/<operation>/`; old content proofs are compared before
publication, then full schema-13 content is verified. Reopening schema 13 does not
repeat migration. Neither module activation nor preview visibility controls this.
Windows/Linux retain their current startup paths. No eBird import writer is enabled.

## Recovery boundary

Unfinished Restore, Import or Recovery blocks before any schema migration. Existing
completion verifies the selected journal under its original proof and retires its
Recovery gate; that process can hold an older schema temporarily. **Quit and relaunch
normally after completion** to upgrade an older restored journal. The new migration
is not inserted between Recovery verification and its admission commit. An older
Open Import into schema 13 already normalizes its private candidate to schema 13.
All snapshots and replaced-journal protection remain independent of module state.

## Prepare a genuine schema-12 profile

Use the same Terminal throughout, or retain the printed root path to reuse it.
Never point these commands at a production or existing test journal. Quit MNO DEV
before switching source versions. The pinned checkpoint predates ordinary schema-13
activation and really creates schema 12; do not simulate this by editing user_version.

```sh
MNO_SCHEMA13_ROOT=$(mktemp -d "$HOME/mno-schema13-dev-XXXXXX")
export MNO_SCHEMA13_ROOT
mkdir "$MNO_SCHEMA13_ROOT/schema12-source"
git -C /Users/stefanantonsson/Projects/my-nature-observations-public archive \
  fb4a57a786220b95bb199e69adb2967dd8564245 \
  | tar -x -C "$MNO_SCHEMA13_ROOT/schema12-source"
printf 'Retain this root for every restart: %s\n' "$MNO_SCHEMA13_ROOT"
test ! -e "$MNO_SCHEMA13_ROOT/journal"
cd "$MNO_SCHEMA13_ROOT/schema12-source/app"
npm ci
npm run i18n:compile
MNO_TEST_DATA_DIR="$MNO_SCHEMA13_ROOT/journal" npm run tauri dev
```

In the old DEV app, create synthetic journal text, attach a disposable photo with
a caption and set journal weather. Enable Birds if necessary and create a linked
manual bird visit with count/comment, photo/caption and visit weather. Optionally
add an unlinked visit. Save, quit normally and verify the real version read-only:

```sh
sqlite3 -readonly "$MNO_SCHEMA13_ROOT/journal/mno.sqlite" 'PRAGMA user_version;'
shasum -a 256 "$MNO_SCHEMA13_ROOT/journal/mno.sqlite"
```

Expect **12**. Retain the SHA-256. DEV module choice is shared across DEV profiles,
not stored per journal; production settings are separate. Record your existing DEV
choice before changing it. For a core-only upgrade test explicitly turn Birds off
in Settings before quitting; do not delete or reset any preference file.

## Upgrade and restart the exact same profile

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
MNO_TEST_DATA_DIR="$MNO_SCHEMA13_ROOT/journal" npm run tauri dev
```

1. Confirm core journal text, photo/caption and journal weather. If Birds was off,
   its UI stays hidden and the chosen preference is unchanged.
2. Explicitly enable Birds and confirm the existing visits, comments, photos,
   captions, links and visit weather. No eBird saving action should appear.
3. Quit normally. Check version **13** and compare the retained schema-12 copy's
   checksum with the original checksum. The original journal path is unchanged.

```sh
sqlite3 -readonly "$MNO_SCHEMA13_ROOT/journal/mno.sqlite" 'PRAGMA user_version;'
find "$MNO_SCHEMA13_ROOT/journal/backups/bird-schema-upgrade" \
  -name schema12.sqlite -exec shasum -a 256 {} \;
```

4. Restart with the **identical command and profile**, then quit and check again:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
MNO_TEST_DATA_DIR="$MNO_SCHEMA13_ROOT/journal" npm run tauri dev
```

Content and preference should persist. There should still be exactly one
`schema12.sqlite` migration copy for this upgrade. Keep all profile/archive/copy
files; do not clean or reset the test root. Restore the prior DEV module choice
if the test changed it. Do not reopen the upgraded active journal with the old app.

Automated tests, rather than manual file manipulation, cover injected migration
interruptions and pending Recovery. Older-archive replacement tests verify that
Recovery leaves the installed bytes unchanged until completion, that the replaced
eBird journal stays protected, and that a subsequent ordinary startup reaches 13.

## Accepted macOS DEV upgrade — owner-reported result

Tested source checkpoint: `bacbaa4c0347f7552fdc6cd2ee2f83153b44e8d9`.
This acceptance record is added afterward and does not change the tested build.

The owner confirmed:

- A genuine schema-12 fixture upgraded through ordinary macOS DEV startup.
- Journal and bird content, photos/captions, weather and links were preserved.
- The disabled module preference was preserved; explicit reactivation restored
  access to the existing bird content.
- The active database was confirmed as schema 13.
- The independent schema-12 copy matched the original checksum.
- Another restart preserved content/preference and left exactly one schema-12 copy.

No checksum value was supplied for this acceptance, so none is recorded. The
checksum match above is the owner's reported check, not a newly recalculated hash.
This accepts the DEV upgrade/restart behavior only. It does not accept user-facing
eBird saving, imported-row photo attachments, an unresolved-row review interface,
packaged behavior or new manual Restore/Open Import tests.
