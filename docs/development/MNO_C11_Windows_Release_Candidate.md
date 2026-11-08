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

The manual [Windows RC1 browser acceptance workflow](../../.github/workflows/windows-rc1.yml)
always checks out `7ae3313677e6c4b45b9742365bcf7567e5ceac67`.
It builds the unsigned default-feature NSIS package, verifies its
identity, packaged executable, installer license and notices, and logs
the hash of the installer it built. It uploads
`mno-windows-x64-rc1-7ae3313` for three days with the installer,
`SHA256SUMS.txt` and `BUILD-INFO.json`. The repository is public and
repository readers can download the artifact.

GitHub requires the workflow file on the default branch before it can
receive a manual dispatch. Once registered, select **Windows RC1 browser
acceptance** in Actions and run it on `codex/cross-platform-ci`, or use:

```sh
gh workflow run windows-rc1.yml --ref codex/cross-platform-ci
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
