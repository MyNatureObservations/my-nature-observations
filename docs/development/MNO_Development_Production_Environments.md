# Development and Production environments (release-readiness A1)

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

## Start and build

From `app/`:

```sh
npm run tauri dev
npm run tauri build -- --bundles app
```

The first command automatically applies `src-tauri/tauri.dev.conf.json`. The second
builds Production using the shared base configuration. No environment variable
is needed for normal development. `tauri dev --release` and `tauri build --debug`
also receive the DEV overlay. The wrapper appends it after caller configuration
arguments and before arguments forwarded to the application.

## macOS v0.1 deployment baseline and test policy — 2026-11-02

The configured minimum requirement is **macOS 13.0 Ventura, Apple Silicon
(arm64) only**. MNO has been manually tested on **macOS 27**. The configured
minimum is not a claim of separately verified runtime compatibility on Ventura.
Earlier macOS versions will not receive separate compatibility testing before
v0.1. If users encounter compatibility problems, recommend updating macOS to a
newer release. Versions below macOS 13 are outside the configured requirement.

`src-tauri/tauri.conf.json` sets `bundle.macOS.minimumSystemVersion` to `13.0`.
Tauri 2.12.3 uses this for `LSMinimumSystemVersion` and the build's
`MACOSX_DEPLOYMENT_TARGET`, including native dependency compilation. Do not add a
competing minimum in `Info.plist` or override the deployment target in the shell.
The DEV bundle overlay inherits this packaged-build setting; identifiers/data
roots remain unchanged. Tauri's unbundled `tauri dev` ignores this bundle setting.

The previous unset value produced Tauri's default plist minimum of 10.13, while
Rust's aarch64 macOS baseline produced a Mach-O minimum of 11.0. Explicit 13.0
removes that mismatch. Build with the established command above on Apple Silicon;
no Intel or Universal artifact is introduced. The existing arm64 CI package job
now checks the generated plist and every bundled Mach-O for arm64 and the
configured minimum version. No CI execution is implied by editing the workflow.

For the isolated candidate, verify `Info.plist`, `lipo -archs` and `otool -l` before
installation. Verify bundled notices using `npm run notices:validate -- /path/to/App.app`.
For a local app-only candidate with only a linker ad hoc signature, the established
repair below applies to the isolated bundle only. For distribution, use the
[manual release procedure](#manual-macos-v01-release-procedure) instead: signing
must happen before DMG packaging, not as a repair after packaging.

```sh
codesign --force --sign - --identifier org.mynatureobservations.app --timestamp=none '/path/to/My Nature Observations.app'
codesign --verify --deep --strict --verbose=2 '/path/to/My Nature Observations.app'
```

This does not provide Developer ID signing, notarization or public Gatekeeper
acceptance. No application needs to be launched to inspect these build properties.
Local candidate verification on 2026-11-02 passed: `LSMinimumSystemVersion=13.0`,
one bundled Mach-O executable with `minos 13.0` and arm64 architecture, unchanged
Production identifier/version, icon/localizations and byte-matching notice
resources. The new CI verification code also passed locally against this artifact;
no GitHub CI was triggered. The isolated candidate received an ad hoc bundle
signature and passed `codesign --verify --deep --strict --verbose=2`. It was not
installed or launched. This build used main `3e5c9ae` plus the local deployment
configuration change, not a new committed source revision.

Runtime acceptance on an actual macOS 13 environment remains **not verified**;
no separate Ventura test is required before v0.1 under the approved policy.
Newer-host compilation and CI on macOS 15 do not establish Ventura runtime
compatibility. In inspected Wry
0.57.0 paths, newer 13.3/14 APIs have selector/version checks; page zoom requires
11.0. This is a targeted dependency review, not an exhaustive availability audit.

| Environment | Application identifier | Product name | macOS persistent root |
| --- | --- | --- | --- |
| Production | `org.mynatureobservations.app` | My Nature Observations | `~/Library/Application Support/org.mynatureobservations.app` |
| DEV | `org.mynatureobservations.app.dev` | My Nature Observations DEV | `~/Library/Application Support/org.mynatureobservations.app.dev` |

The root is the current user's Tauri `data_dir()` plus the configured application
identifier. SQLite (`mno.sqlite`), photographs, Recovery state and other managed
files remain under the selected environment's root. Existing backup, import,
export and Restore implementations receive that root through the existing
startup state; their formats and behavior are unchanged. Externally selected
backup/export files remain at the user's chosen destination.

Production keeps its existing location and files. Nothing is migrated, copied,
imported or removed. Document v2, SQLite schema 4 and archive/proof versions stay
unchanged. DEV shows a small noninteractive `DEV` badge, including in Recovery,
and the native title `My Nature Observations — DEV`.

## Manual macOS v0.1 release procedure

Public distribution will use GitHub Releases, without a paid Apple Developer
Program membership. Build locally using Tauri's built-in DMG bundler. v0.1 has
no automatic updater or GitHub Actions release workflow; the existing CI checks
are not a publication pipeline.

### Build from a reviewed revision

Start in the local repository outside iCloud Drive. Record `git rev-parse HEAD`
and inspect `git status --short` and `git diff HEAD`. Use a known, reviewed commit
with no tracked changes. Preserve existing unrelated untracked files; do not
include them in the release or delete them to obtain a clean status. Use the
project's Node/Rust toolchain and installed Apple command-line tools, with the
`aarch64-apple-darwin` Rust target available.

From the repository root:

```sh
cd app
npm ci
npm run i18n:compile

CARGO_TARGET_DIR="$PWD/src-tauri/target" \
APPLE_SIGNING_IDENTITY="-" \
npm run tauri build -- --target aarch64-apple-darwin --bundles app,dmg
```

The npm wrapper leaves this release build on Production configuration: no
`--debug`, DEV overlay or custom configuration. The Production/DEV identifiers
and data roots are listed above. Building does not launch the app or open either
data directory. Do not set notarization credentials for this non-notarized build.
`beforeBuildCommand` builds the frontend and generates third-party notices.

The installed Tauri CLI 2.12.2 supports the explicit target and comma-separated
bundle list above. `APPLE_SIGNING_IDENTITY="-"` selects ad hoc signing for this
invocation; leave `tauri.conf.json` unchanged. Tauri signs the app before creating
the DMG and deliberately skips DMG signing for the ad hoc identity (see the
[CLI 2.12.2 bundler source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/macos/dmg/mod.rs)).
The resulting DMG is unsigned and neither artifact is notarized. Ad hoc signing
checks integrity; it does not establish an Apple-verified developer identity or
remove Gatekeeper warnings. See [Tauri's signing guidance](https://v2.tauri.app/distribute/sign/macos/).
Do not alter or re-sign the app after packaging: rebuild the DMG if its contents
need to change.

### Verify the final artifacts

Continue from `app/`. For v0.1.0, expected paths are:

```sh
mno_bundle_root="$PWD/src-tauri/target/aarch64-apple-darwin/release/bundle"
mno_app_bundle="$mno_bundle_root/macos/My Nature Observations.app"
mno_dmg_file="$mno_bundle_root/dmg/My Nature Observations_0.1.0_aarch64.dmg"

plutil -p "$mno_app_bundle/Contents/Info.plist"
lipo -archs "$mno_app_bundle/Contents/MacOS/app"
otool -l "$mno_app_bundle/Contents/MacOS/app"
codesign -dv --verbose=2 "$mno_app_bundle"
codesign --verify --deep --strict --verbose=2 "$mno_app_bundle"
npm run notices:validate -- "$mno_app_bundle"
hdiutil verify "$mno_dmg_file"
```

Require product name `My Nature Observations`, identifier
`org.mynatureobservations.app`, version `0.1.0`, `LSMinimumSystemVersion=13.0`,
arm64 only and Mach-O `LC_BUILD_VERSION` / `minos 13.0`. Inspect every bundled
Mach-O component if additional executables, frameworks or libraries appear; the
current app has one executable. Require an ad hoc bundle signature and successful
strict verification, not merely a linker-generated executable signature. The
notices validator compares packaged resources with generated notices. These
checks do not establish runtime compatibility on macOS 13–26.

Verify the copy actually distributed by mounting the DMG read-only:

```sh
mno_mount_dir="$(mktemp -d /private/tmp/mno-dmg-check.XXXXXX)"
hdiutil attach -readonly -nobrowse -mountpoint "$mno_mount_dir" "$mno_dmg_file"
mno_mounted_app="$mno_mount_dir/My Nature Observations.app"
plutil -p "$mno_mounted_app/Contents/Info.plist"
lipo -archs "$mno_mounted_app/Contents/MacOS/app"
otool -l "$mno_mounted_app/Contents/MacOS/app"
codesign --verify --deep --strict --verbose=2 "$mno_mounted_app"
npm run notices:validate -- "$mno_mounted_app"
ls -l "$mno_mount_dir"
hdiutil detach "$mno_mount_dir"
rmdir "$mno_mount_dir"

shasum -a 256 "$mno_dmg_file" > "$mno_dmg_file.sha256"
```

Require the same metadata/deployment target, valid signature and notices in the
mounted app, plus the Applications shortcut. Stop on any failed check; detach
before investigating. Keep the verified DMG unchanged after generating its hash.

### Acceptance and manual publication

Use a separate macOS user account and synthetic journal data for release runtime
acceptance. Release builds ignore `MNO_TEST_DATA_DIR`, so it is not a substitute
for this account boundary. Do not use personal Production/DEV data or backups.
Check installation from the DMG, startup, synthetic entries and photographs,
settings persistence, backup/restore and an update over an earlier synthetic
installation. Check Swedish/English menus, zoom and guarded Quit. Keep the
existing regular app installation untouched during candidate testing.

After acceptance, manually prepare a GitHub Release tied to the recorded source
commit and approved version tag; attach the verified DMG and SHA-256 file. Before
making it public, download the DMG through the browser from the draft release
and test it in the separate account. Compare its SHA-256 and verify the actual
first-launch Gatekeeper flow using the README's app-specific Privacy & Security
instructions. A local build alone does not test download quarantine. Do not
remove quarantine attributes or disable Gatekeeper to make acceptance pass.

Release notes must state Apple Silicon, macOS 13 minimum, testing on macOS 27
only, the recommendation to update macOS if compatibility problems occur, and
the unsigned DMG/ad hoc-signed, non-notarized app. Publish only after downloaded
artifact acceptance; no automatic updater or additional release automation is
required for v0.1.

## Safety and isolated profiles

Before installing plugins (including single-instance handling), development builds
reject the Production identifier. This includes debug builds and Tauri development
mode with optimizations. Bypassing the npm wrapper with the base configuration
therefore fails closed rather than opening Production. Use the supported command.
Different identifiers also separate native single-instance handling.

Debug builds retain the isolated-profile override:

```sh
MNO_TEST_DATA_DIR=/absolute/path/to/new-synthetic-profile npm run tauri dev
```

The override must be absolute, contain no `..`, and neither equal, contain nor be
inside the reserved Production root. Existing ancestors are canonicalized so
symlink aliases are checked too; reserved paths are compared without case on
macOS. Invalid paths fail before database initialization. Release builds ignore
the override, as before. An older diagnostic invocation using the Production
identifier plus a test override must now use DEV or a distinct test identifier.

Profiles with the same application identifier still share single-instance
handling: quit the ordinary DEV instance before launching an isolated profile,
or use an explicit distinct nonproduction identifier for a concurrent native
fixture. Existing directory leases, SQLite locks and fail-closed startup checks
remain in force. This is an environment boundary, not a mechanism for deliberately
copying personal data into test profiles.

## Verification performed

- Frontend: 411 tests passed; Svelte/TypeScript check: zero errors or warnings.
- Rust library: 905 passed, 2 ignored; selected database crash/safety,
  descriptor-owner and Recovery-handoff integration targets: 18 passed.
- Rust formatting and whitespace checks passed.
- Ordinary Production app bundle built successfully; its Info.plist contains
  `org.mynatureobservations.app` and `My Nature Observations`. It was not launched.
- Default native Tauri DEV startup, with `MNO_TEST_DATA_DIR` removed, reported
  `~/Library/Application Support/org.mynatureobservations.app.dev`.
  This was a new DEV root. No Production database was opened for verification.
- Focused tests cover command selection, distinct identities, unchanged Production
  resolution, isolated profiles, overlap/case/symlink rejection and simultaneous
  synthetic DEV/Production persistence with independent locks and DEV reopen.
- Native visual/caret verification was unavailable because the Mac was locked.
  Native startup is verified; visible badge and interactive save/reopen remain
  manual acceptance steps below.

## Manual acceptance

1. From `app/`, run `npm run tauri dev` without a test override. Confirm the DEV
   title and badge. The terminal prints the DEV root above.
2. Create a clearly synthetic entry, save, quit and repeat the same command.
   Confirm the entry reopens from DEV. Test a photograph and backup/Restore using
   synthetic data and an explicitly chosen temporary archive destination.
3. Quit DEV. Launch with a new absolute `MNO_TEST_DATA_DIR`. Confirm an independent
   empty journal, then quit and return to ordinary DEV; its earlier entry remains.
4. Use automated overlap tests rather than pointing an interactive app at personal
   Production data. The synthetic concurrency test opens both roots together and
   confirms DEV writes do not reach the synthetic Production database.
5. Build Production with the command above and inspect its bundle identifier/name.
   On an approved personal-use launch, Production should retain its existing
   journal and have no DEV indication. This last launch was intentionally not
   performed during implementation.

## OpenStreetMap identification and v0.1 acceptance

The shared window `userAgent` is
`MyNatureObservations/0.1.1 (+https://github.com/MyNatureObservations/my-nature-observations)`.
Tauri 2.12.3 passes this to Wry 0.57.0's WKWebView `customUserAgent`, including
packaged builds. DEV inherits it. It applies to webview HTTP traffic, including
Leaflet images, not Rust's weather client. Keep the version aligned on releases.

Configuration and build checks do not prove actual headers or caching. On
2026-11-03, the remaining network verification gap was accepted for v0.1. The
packaged app worked in a separate macOS test account, and attribution was visually
confirmed, including at 200% zoom. Actual HTTP headers and cache behavior were
not observed; Safari exposed no inspectable release webview. This is not a claim
of directly observed network compliance.

If network verification is needed later, observe a packaged app in a separate
macOS user account with synthetic data. Release builds ignore `MNO_TEST_DATA_DIR`; never
launch the Production candidate in the personal journal account.

Using an available WebKit network inspector, with cache enabled and no forced reload:

1. Open a small viewport at synthetic coordinates (e.g. latitude 0, longitude 0).
   Confirm tile requests carry the MNO User-Agent and no unnecessary request
   `Cache-Control: no-cache`, `no-store` or `Pragma: no-cache` headers.
2. Record response cache headers; revisit the same viewport/zoom and reopen the
   app. Fresh cached tiles should not download again. Check conditional requests
   for expired tiles where supported. Distinguish memory/disk cache hits and
   304 responses from full response-body downloads.
3. Check attribution and its copyright link at minimum window size, 100%/200%
   app zoom, and with the tile-error warning visible. Do not repeatedly request
   tiles to manufacture an outage.

If later investigation requires it, an approved external inspection method or a
separately prepared inspectable packaged DEV candidate can be considered. Do not disable
TLS verification or macOS security protections. No automated OSM load test is needed.
The detailed network checks and additional visual scenarios above have not all
been performed and do not block the agreed v0.1 acceptance. Leaflet's in-memory
tiles are not proof of HTTP caching. See the [OSM tile policy](https://operations.osmfoundation.org/policies/tiles/).
