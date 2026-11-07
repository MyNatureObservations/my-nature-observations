# Offline third-party notices

Initial implementation scope: macOS arm64 release packaging, issue #14 prerequisite for About MNO. MNO remains MIT-licensed. The later [Settings/About implementation](MNO_Settings_About_MNO.md) adds access to this resource without journal or installed-application changes.

## Reproduce

From `app/`, using Node/npm from CI and **Rust 1.99.1**:

```sh
npm ci
npm run i18n:compile
cargo install cargo-about --version 0.9.3 --locked --features cli
npm run build
npm run notices:generate
npm run notices:validate
npm run tauri build -- --bundles app
npm run notices:validate -- 'src-tauri/target/release/bundle/macos/My Nature Observations.app'
```

The Tauri `beforeBuildCommand` builds the client and generates/validates notices automatically. Generation requires build-time network access when locked Cargo sources or metadata dependencies are not cached. `cargo about generate` deliberately does not use `--offline`: the preceding target-specific fetch does not necessarily populate every package needed while Cargo resolves metadata (including other-platform packages). `--locked` remains mandatory, so downloads cannot change dependency versions. Offline operation applies to viewing the packaged notices and local source archives, not to generating them. It does not install a runtime service. `cargo-about` **0.9.3** is checked exactly; Vite and other npm build tools come from `package-lock.json`. Cargo resolution uses `--locked`, the explicit `aarch64-apple-darwin` target and the release `tauri/custom-protocol` feature. Build dependencies are conservatively included; development/test dependencies are excluded. This deliberately includes some components whose code does not survive final linking.

`client-inventory.mjs` records modules with emitted client code and imported CSS during the actual Vite build, rather than using npm's production/development classification. Generated Paraglide runtime is explicitly attributed to its locked compiler package. This includes Svelte/SvelteKit runtime and excludes unrelated npm test/build tooling. A missing lock entry or version mismatch stops generation.

Generated files (ignored in Git):

```text
app/src-tauri/notices/
  THIRD-PARTY-NOTICES.html
  inventory.json
  source-checksums.json
  sources/*.crate
```

Tauri maps this directory to `notices/` in its resources. On macOS it becomes `Contents/Resources/notices/`. A future About UI should use Tauri's resource API (`resolveResource('notices/THIRD-PARTY-NOTICES.html')`), not a fixed filesystem path. The tracked `.gitkeep` permits development startup before generation; distributable Tauri builds run generation and fail if notices cannot be produced. Calling Cargo directly is not the supported packaging procedure.

HTML has inline styling, escaped verbatim texts, no scripts, external styles or fetched resources. Identical text blocks are shared by hash while distinct copyright notices remain separate. `inventory.json` retains complete component/text associations for auditing. Source URLs in upstream text are informational; local MPL source links work offline. `.crate` files are ordinary tar/gzip source archives.

## Reviewed evidence and policy

`app/licensing/about.toml` defines accepted SPDX identifiers; MIT is preferred where upstream offers a choice. The generator evaluates AND, OR, parentheses and WITH exceptions. It preserves the declared expression separately from included terms: e.g. ring **Apache-2.0 AND ISC**, brotli **BSD-3-Clause AND MIT**, dpi **Apache-2.0 AND MIT**, and compiler-builtins **MIT AND Apache-2.0 WITH LLVM-exception**. Choosing one OR alternative does not drop an AND obligation. Actual top-level LICENSE/COPYING/COPYRIGHT/NOTICE files supplement cargo-about's extracted text, including ring's BoringSSL/other-bits notices, BSD attribution and CDLA-Permissive-2.0 certificate-data terms.

`exceptions.json` contains 19 narrowly scoped replacements for synthesized notices, not blanket license exemptions. Each is bound to package name/version, published crate checksum and `.cargo_vcs_info.json` source commit. `evidence/` stores verbatim, SHA-256-checked upstream material with its origin URL in the registry. Versions, checksums or commits changing require renewed review. `.gitattributes` preserves upstream evidence byte-for-byte, including its original whitespace; SHA-256 checks protect it rather than reformatting legal material.

In particular, objc2 0.6.5 and block2 0.6.2 provide an upstream `LICENSE.md` licensing declaration but no separate copyright notice or complete MIT permission text in the published crate. Their exact upstream declaration is retained together with standard MIT permission/warranty terms **without an invented copyright line**. Related objc2 framework bindings use the same reviewed policy. Their upstream Apple-SDK licensing qualification is preserved, not silently removed or represented as a new legal determination. Tauri MIT notices, alloc-stdlib's BSD notice, cargo_toml's actual Apache license and dpi's libm MIT material replace generic generated placeholders.

Fail on unresolved expressions, missing mandatory texts/notices, lost text, generated attribution placeholders, stale reviewed exceptions/evidence, missing MPL sources, checksum mismatch or external/missing HTML resource references. Canonical Apache appendix instructions are license text, not a fabricated attribution. A missing package copyright metadata field alone is not a failure: available actual notices must be retained; verified upstream omissions are documented and never filled with guessed authors/years.

## Rust runtime and native components

`native.json` pins Rust **1.99.1**, source commit **58cf8f9c35b355199727627d07ae34a4e2846d67**, and the matching `library/Cargo.lock`. Evidence includes the installed MIT/Apache/COPYRIGHT files, `COPYRIGHT-library.html`, Unicode license, matching compiler-builtins/libm terms (including LLVM exception), and license/NOTICE files from checksum-verified registry sources corresponding to installed std rlibs. The compiler version and commit are checked at generation. The installed copyright inventory's limited external-component section is supplemented by this source evidence; an unavailable generated report is not required.

The runtime evidence is conservative, not a claim that every source object is linked. When changing Rust, review the new source lock, linked runtime and license files together; do not just update the version string. For new evidence: record the exact source URL/commit or installed file, store the unchanged bytes by SHA-256, and record the package archive checksum where applicable. Never edit upstream text to remove inconvenient terms.

SQLite's embedded copyright disclaimer/blessing is preserved alongside libsqlite3-sys's MIT binding license. macOS frameworks (including AppKit/WebKit) are OS-provided dynamic dependencies, not bundled copies. The old, unused Svelte/Tauri/Vite template SVGs have been removed from public assets; the favicon now comes from the already approved MNO icon. Leaflet's bundled images/CSS are covered with its BSD-2-Clause material. MNO's owner-provided icon remains project artwork, not relabeled third-party art.

## MPL sources

Five locked crates selected as MPL-2.0 are supplied unmodified: cssparser, cssparser-macros, dtoa-short, option-ext and selectors. Their cached published `.crate` bytes must match **Cargo.lock** checksums before copying. HTML explains source availability and links to each local archive; `source-checksums.json` records versions, hashes and origins. Validation requires an archive for each selected MPL component and checks its bytes. Consumers can obtain corresponding source without relying on a live upstream service. Some CSS crates are build-time dependencies, included conservatively.

## Verification and remaining work

Local verification: 417 frontend tests (including five notice-policy regression tests), Paraglide compilation, Svelte/TypeScript checks, frontend build and optimized macOS `.app` build. The packaged resource validator compares every generated notice/source file byte-for-byte with the app resources. Two generations with identical inputs must produce identical hashes. No application is installed or launched and no journal is opened.

CI keeps the existing quality checks and adds a macOS notice/package job with the pinned generator. `macos-15` is the current arm64 hosted label; the job checks `uname -m` explicitly ([GitHub runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)). No check status is implied until GitHub has executed it.

Remaining before public release:

- About MNO now provides offline notice access and the project MIT text; see the [Settings review and packaged manual acceptance](MNO_Settings_About_MNO.md). #14 stays open for review/acceptance.
- Windows x64 has a separately reviewed native inventory and packaged-notice validation for internal NSIS builds. Verify the exact public candidate as described in the [C10 distribution review](MNO_C10_Windows_Installation_Data_Preservation_Review.md#c10c-distribution-readiness). Linux packaging still needs a reviewed target inventory before the generator can support it.
- Future dependency/toolchain updates require regenerating and reviewing changes, including new NOTICE files, assets, source obligations and version-specific exceptions. This process is not a general legal certification or signing/notarization approval.
- Manual acceptance: copy the packaged `notices/` folder to an offline location, open HTML, inspect representative MIT/Apache, Leaflet BSD, ring, CDLA and compiler-builtins entries, and expand an MPL source archive. Verify future About navigation separately.

## Open-Meteo weather data attribution

Open-Meteo is an external data service, not bundled API server code. Its weather
API data are attributed separately from MNO's MIT code and bundled component
notices: [Open-Meteo](https://open-meteo.com/) and
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
Saved weather summaries in journal editing/reading and the entry list show both
links next to fetched data. Weather details, fetch previews and replacement
comparisons retain their source/model and license links. Manual observations
are not attributed to Open-Meteo. Labels reuse the existing Swedish/English
weather source translations.

Data attribution does not settle API service-use limits or distribution terms;
keep the outstanding provider clarification separate. See the
[provider's license guidance](https://open-meteo.com/en/licence).
