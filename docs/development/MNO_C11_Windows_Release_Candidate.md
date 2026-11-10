# C11a Windows release candidate

Use source commit `7ae3313677e6c4b45b9742365bcf7567e5ceac67` on
`codex/cross-platform-ci`. The C11a documentation commit adds
instructions only. This review prepared the build procedure. No candidate
binary was built or inspected.

| Identity | Value |
| --- | --- |
| Candidate label | Windows RC1 |
| Recommended filename | `My-Nature-Observations-0.1.0-windows-x64-rc1-7ae3313-setup.exe` |
| Embedded identity | My Nature Observations, version `0.1.0`, ID `org.mynatureobservations.app` |
| Packaging | Unsigned current-user NSIS, `x86_64-pc-windows-msvc`, default features |

Keep the version and identifier in the package unchanged. The candidate
suffix belongs to the filename and build record. Windows still treats
this build as version `0.1.0` for installer maintenance. An older
Windows test installer with that same version does not establish a
distinct-version upgrade. The existing macOS artifact and release are
outside this candidate.

## Local build on Windows 11 x64

Use a short local checkout path and the reviewed x64 MSVC v143
`14.43.34808` environment with a Windows SDK. In a non-elevated
Command Prompt, initialize it using the installed Build Tools path:

```bat
call "<Build Tools installation path>\VC\Auxiliary\Build\vcvarsall.bat" x64 -vcvars_ver=14.43
echo %VCToolsVersion%
echo %WindowsSDKVersion%
where cl
where link
where rc
```

Verify Node.js `26.12.1` x64 with its npm, Rust `1.99.1` for the
MSVC x64 host and commit `58cf8f9c35b355199727627d07ae34a4e2846d67`,
and `cargo-about 0.9.3`. Run each command only after the preceding one
succeeds. Stop on a dirty checkout or toolchain mismatch.

```bat
cd /d C:\src\mno
git status --short
git fetch origin codex/cross-platform-ci
git switch --detach 7ae3313677e6c4b45b9742365bcf7567e5ceac67
git rev-parse HEAD
git status --short
set "RUSTUP_TOOLCHAIN=1.99.1-x86_64-pc-windows-msvc"
rustc -vV
node --version
node -p process.arch
npm.cmd --version
cargo about --version
```

If needed, install the pinned notice generator with
`cargo install cargo-about --version 0.9.3 --locked --features cli`.
From the checkout, build locally:

```bat
cd /d C:\src\mno\app
npm.cmd ci
npm.cmd run i18n:compile
npm.cmd run tauri build -- --ci --no-sign --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.windows-test.conf.json -- --locked
```

The pre-build hook builds the frontend and generates the Windows license
and notice resources. The npm lockfile pins Tauri CLI 2.12.2. The output
is under `app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`.
Identify the newly generated setup executable and copy it to a local
candidate folder under the recommended name. Use the existing workflow
as the verification reference. Its upload step creates a downloadable
artifact on a public repository, so keep this preparation local.
[Windows configuration](../../app/src-tauri/tauri.windows-test.conf.json)
· [Packaging workflow](../../.github/workflows/ci.yml)

## Verify the exact candidate

1. Record the source commit, embedded version, bundle ID, target and
   actual toolchain versions. Check the generated
   `release/nsis/x64/installer.nsi` for the current-user install mode
   and bundle ID. Verify PE machine `0x8664` and the product name and
   version of the extracted `app.exe`, following the packaging workflow.
2. Extract the renamed candidate into an empty temporary directory
   with 7-Zip. From `app/`, run
   `node scripts/notices/validate.mjs --resources "<extracted notices directory>"`
   and
   `node scripts/verify-nsis-executable.mjs "<release app.exe>" "<extracted app.exe>"`.
   Inspect the generated NSIS license reference and
   `src-tauri/notices/WINDOWS-INSTALLER-LICENSE.txt` for the MIT text
   and reviewed runtime terms. During desktop acceptance, verify the
   installer license page and About MNO's offline notices.
3. Run `Get-FileHash -Algorithm SHA256 -LiteralPath "<candidate path>"`
   and `Get-AuthenticodeSignature -FilePath "<candidate path>"`.
   Record the hash alongside the exact filename and confirm
   `NotSigned` for this candidate. Use the same bytes for the next check.
4. Download the exact file through a browser from an approved test
   location. Verify the downloaded SHA-256. In an isolated standard-user
   Windows 11 x64 profile, capture any warning or policy block and the
   WebView2 state. Install for the current user and check startup, a
   synthetic journal save/reopen, and the presence of Backup and Restore.
   Reuse [C9h](MNO_C9_Windows_Quality_Readiness_Review.md#c9h-default-windows-restore)
   and [C10b](MNO_C10_Windows_Installation_Data_Preservation_Review.md#c10b-desktop-acceptance)
   evidence for the longer Restore and maintenance flows.
5. Track a genuine increasing-version upgrade separately once two
   approved versions exist. Require that test before claiming upgrade
   support. See the [C10 preservation plan](MNO_C10_Windows_Installation_Data_Preservation_Review.md#minimal-windows-desktop-acceptance-plan).

## Release decision

The reviewed package configuration, Windows notices pipeline and prior
desktop results support an unsigned preview for Windows 11 x64 if the
exact candidate passes package and browser-download acceptance on the
intended Windows security policy. No new packaging defect was found
in this review. A failed notices or identity check, unexpected
journal behavior, or a policy block on the intended installation path
must be resolved before distribution for that audience. A successful
local launch alone does not establish download or security-policy
behavior.

Use the [Windows installation guide](../MNO_Windows_Installation.md)
as the user-facing companion. No release, tag or installer is created
by this preparation.

## C11c browser-download acceptance

The local RC1 passed package verification and standard-user installation
in `MNO-Test`, preserved the synthetic journal, and passed save/reopen and
Backup, Export and Restore visibility checks. Its SHA-256 is
`4142C6A3C66DDE1E7E17A65BCAF5B0FDB34DF9A65015A57E2477F7232616A693`.
That hash identifies the local installer only.

The manual Windows RC1 job in the registered [CI workflow](../../.github/workflows/ci.yml)
always checks out `7ae3313677e6c4b45b9742365bcf7567e5ceac67`.
It builds the unsigned default-feature NSIS package, verifies its
identity, packaged executable, installer license and notices, and logs
the hash of the installer it built. It uploads
`mno-windows-x64-rc1-7ae3313` for three days with the installer,
`SHA256SUMS.txt` and `BUILD-INFO.json`. The repository is public and
repository readers can download the artifact.

Select **CI** in Actions, choose `codex/cross-platform-ci`, and run
with scope `windows-rc1-browser`. The workflow is already registered
on the default branch. The RC1 job is defined on the development
branch and runs only for this manual scope.

```sh
gh workflow run ci.yml --ref codex/cross-platform-ci -f scope=windows-rc1-browser
```

1. From the run's **Artifacts** section, download the ZIP through a normal
   browser and extract it. Record any browser download warning.
2. In PowerShell in the extracted directory, verify the workflow-built
   bytes and check the embedded metadata:

   ```powershell
   $installer = '.\My-Nature-Observations-0.1.0-windows-x64-rc1-7ae3313-setup.exe'
   $expected = ((Get-Content .\SHA256SUMS.txt -Raw).Trim() -split '\s+')[0]
   if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $expected) {
       throw 'Installer checksum mismatch'
   }
   Get-Content .\BUILD-INFO.json
   (Get-Item -LiteralPath $installer).VersionInfo | Select-Object ProductName, ProductVersion
   Get-AuthenticodeSignature -FilePath $installer | Select-Object Status
   ```

   Expect My Nature Observations, version 0.1.0 and `NotSigned`.
   Check the manifest's pinned commit, x64 target and bundle identifier.
   Compare against the workflow manifest, even if the locally built RC1
   has a different hash. Byte-identical reproducibility is unverified.
3. Install as the standard user `MNO-Test` with only synthetic data.
   Record SmartScreen or policy warnings. If security policy blocks
   installation, stop and record it with protections left enabled.
4. Open the journal and verify Backup, Export and Restore are present.
   If needed, save and reopen one synthetic entry. Record the run,
   installer hash, Windows version and outcome.

A browser download of an Actions ZIP and subsequent extraction can
produce different security behavior from a standalone EXE downloaded
from GitHub Releases. Verify the final public download path separately
before treating its security behavior as established.

## C11d Windows 0.1.0 Preview release review

### C11c desktop acceptance — PASS

[Actions run 42515457927](https://github.com/MyNatureObservations/my-nature-observations/actions/runs/42515457927)
completed successfully. The tester downloaded the Actions ZIP through a
browser without browser warnings, extracted it, and verified the installer
against the workflow checksum. Windows SmartScreen displayed its blue
protection warning. The tester explicitly chose **Run anyway**. Installation
under the disposable standard-user `MNO-Test` account succeeded, MNO
launched normally, the existing synthetic journal remained available,
and Settings offered Backup, Export and Restore.

| Candidate identity | Verified value |
| --- | --- |
| File | `My-Nature-Observations-0.1.0-windows-x64-rc1-7ae3313-setup.exe` |
| Source | `7ae3313677e6c4b45b9742365bcf7567e5ceac67` |
| Package | My Nature Observations `0.1.0`, ID `org.mynatureobservations.app`, unsigned Windows x64 current-user NSIS |
| Workflow-built and browser-tested SHA-256 | `FFA81CB8AF9A83E66DA5924A65BC739987AD33033885444FB94B4952A2CD6856` |

This is the Actions-built installer. The earlier local RC1 has a
different checksum and remains separate evidence. The workflow verified
the packaged executable, staged and packaged installer license, and
extracted third-party notices. The browser test covered the Actions ZIP.
The standalone EXE download from GitHub Releases remains untested.

### Readiness decision

- **Verified.** [C9](MNO_C9_Windows_Quality_Readiness_Review.md) records
  default Windows Restore, restart admission, persistent edits and
  rejection of a malformed backup without losing access to the journal.
  [C10b](MNO_C10_Windows_Installation_Data_Preservation_Review.md#c10b-desktop-acceptance)
  records byte preservation across same-version reinstall and
  uninstall/reinstall with data deletion unchecked. Blocked-startup
  Recovery has separate scoped desktop evidence in the
  [C7 review](MNO_C7_Generation_Recovery_Review.md).
- **Acceptable preview limitations.** Scope the preview to Windows 11 x64.
  Open Import is unavailable. Setup may need internet access to obtain
  WebView2 when missing. The unsigned installer can trigger SmartScreen
  or be blocked by local policy. Disclose the observed warning and leave
  Windows protections enabled. The desktop tests did not cover data
  deletion, a distinct-version upgrade or crash and power-loss durability.
  Withhold upgrade support claims until two approved versions are tested.
- **Publication prerequisites.** Select and retain the exact accepted
  executable and manifest before the Actions artifact expires. Confirm
  the hash of the file selected for upload and obtain explicit approval.
  No implementation blocker is identified for a scoped unsigned preview.
  Verify the eventual standalone download separately. A policy block on
  that path for the intended audience requires a distribution decision.

### Publication checklist

- [x] Retain the browser-tested Actions executable and checksum manifest
  locally. Match the SHA-256 above before selecting it for publication.
  If the exact bytes are unavailable, treat any replacement as a new
  candidate and verify it.
- [x] Prepare Windows preview release notes covering Windows 11 x64,
  unsigned SmartScreen behavior, WebView2, Open Import unavailability
  and the unverified distinct-version upgrade path. Include the exact
  filename and full SHA-256, and link the
  [Windows installation guide](../MNO_Windows_Installation.md).
- [x] Verify that the Windows publication plan leaves the existing macOS
  v0.1.0 release, tag and assets untouched.
- [x] Obtain explicit user approval before any publication work.
- [x] At the approved publication step, verify the standalone browser
  download hash and record its security behavior on Windows 11. Reuse
  the existing functionality and preservation evidence.

The publication and standalone download results are recorded in C11h below.

## C11e publication preparation

At C11e preparation, GitHub listed one published pre-release, the macOS
`v0.1.0` at `1bf9bd150d3a187bd5002805b684d2ec989f96af`, with
`My.Nature.Observations_0.1.0_aarch64.dmg`. Use a separate Windows
pre-release titled **My Nature Observations 0.1.0 — Windows Preview**,
with a proposed tag `v0.1.0-windows-preview` at the accepted source
`7ae3313677e6c4b45b9742365bcf7567e5ceac67`. Separate tags keep GitHub's
source archives aligned with each platform's binary and preserve the
existing macOS release. The separate tag was later created in C11f.

The accepted executable, `SHA256SUMS.txt` and `BUILD-INFO.json` were
retained locally from Actions run `42515457927`, artifact
`mno-windows-x64-rc1-7ae3313`. The executable hash matched the checksum
manifest, build metadata and the C11d acceptance record. Keep those
exact bytes. No rebuild is needed.

- [x] Obtain explicit approval for publication and the separate Windows
  tag and pre-release.
- [x] Recheck the retained
  `My-Nature-Observations-0.1.0-windows-x64-rc1-7ae3313-setup.exe`
  against `FFA81CB8AF9A83E66DA5924A65BC739987AD33033885444FB94B4952A2CD6856`.
- [x] Use the existing workflow evidence for bundled installer license
  and offline third-party notices. Upload the exact installer and
  checksum manifest only to the approved Windows pre-release.
- [x] Use the [draft release notes](MNO_Windows_0.1.0_Preview_Release_Notes.md)
  and link the [Windows installation instructions](../MNO_Windows_Installation.md).
- [x] Confirm the existing macOS `v0.1.0` release, tag and DMG remain intact.
- [x] After approved publication, download the standalone EXE through
  a browser, match the accepted SHA-256 and record Windows security
  behavior. The Actions ZIP acceptance does not cover this download path.

## C11h publication closeout

**COMPLETE — C11g passed on the final public distribution path.**
[Windows 0.1.0 Preview](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.0-windows-preview)
is a separate public GitHub pre-release. Tag `v0.1.0-windows-preview`
points to `7ae3313677e6c4b45b9742365bcf7567e5ceac67`. The published
macOS `v0.1.0` release remains unchanged.

On Windows 11 the tester downloaded the standalone installer directly
from the release using Microsoft Edge. Edge displayed a SmartScreen
reputation warning for the uncommon, unsigned executable. The tester
chose **Keep anyway**, and the downloaded
`My-Nature-Observations-0.1.0-windows-x64-rc1-7ae3313-setup.exe` matched
the accepted SHA-256:

`FFA81CB8AF9A83E66DA5924A65BC739987AD33033885444FB94B4952A2CD6856`

Setup launched without additional warnings and installed successfully
under the disposable standard-user `MNO-Test` account. MNO launched
normally, the existing synthetic journal remained available, and
Settings offered Backup, Export and Restore.

The earlier Actions ZIP test showed no browser warning, but Windows
SmartScreen warned at launch and the tester chose **Run anyway**.
The standalone release test warned in Edge at download and showed no
additional warning at launch. These observations apply to the tested
environment. Reputation and local policy can produce different results.

The preview remains scoped to Windows 11 x64. The installer is unsigned
and local policy may block it. Open Import remains unavailable. Setup
may need internet access if WebView2 is missing. Distinct-version
upgrades, uninstall with app-data deletion selected, and exhaustive
crash and physical power-loss behavior remain outside acceptance.
See the [current Windows installation guide](../MNO_Windows_Installation.md).

## Linux handoff

Next phase: **desktop feasibility and packaging assessment**. Start
by mapping the production platform gates and the Linux application-data
path, then compare that inventory with Ubuntu CI coverage. Identify
which actions are usable and which are restricted across Backup,
Export, Restore and Recovery, including path handling and isolation
of synthetic test journals.

Review WebKitGTK and other Tauri system dependencies, compare AppImage
and Debian packaging, and identify the minimum native desktop and CI
acceptance checks. Review target-specific license and third-party
notice packaging, distribution limits and user-facing instructions.
Keep the distribution and package format open until the assessment
is complete.
