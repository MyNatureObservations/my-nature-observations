# My Nature Observations

**Your nature. Your observations. Your way.**

My Nature Observations (MNO) is a free, open-source, local-first nature journal.
Keep your stories, observations and photographs on your own computer, with offline access.

## Vision

A calm, personal space to document nature experiences, revisit memories and explore observations over time.

**The interface whispers. Your story speaks.**

## Project status

The first public release is being prepared as **v0.1.0 – First Public Preview**,
marked as a GitHub Pre-release for macOS. Windows and Linux are planned for later releases after platform-specific validation; they are not available in this macOS preview.

See the [release decision](docs/design/MNO_Design_Decisions.md#d-019--macos-först-i-v01-serien) and [readiness inventory](docs/development/MNO_v0.1_Release_Readiness_Inventory.md) for verified scope and remaining release checks.

## Included in v0.1

- Rich-text journal entries with photographs and captions
- Search and browsing by date, month and year
- Saved places and interactive maps
- Manual weather notes and optional weather fetching
- Journal backup/restore and open export/import
- Swedish and English interfaces, keyboard navigation and 100–200% interface zoom

Journal writing and stored content work offline. Opening the map loads tiles from
**OpenStreetMap** for the displayed area. Choosing to fetch weather contacts
**Open-Meteo**, sending the selected coordinates and date/time, not journal text.
These services are used for their respective map and weather functions; ordinary
journal writing does not require them. Online maps and weather fetching require
an internet connection and depend on service availability.

## macOS download and installation

The first public release is being prepared. Once available, download the Apple
Silicon DMG from [GitHub Releases](https://github.com/MyNatureObservations/my-nature-observations/releases).

- **Requirements:** Apple Silicon (arm64) and macOS 13 Ventura or later. Intel Macs are not supported in v0.1.
- **Compatibility:** manually tested on macOS 27. macOS 13–26 have not been separately runtime-tested. If you encounter compatibility problems, we recommend updating macOS.
- **Installation:** open the DMG, drag **My Nature Observations** into **Applications**, then eject the disk image and open the app from Applications.

The initial public version will **not be notarized by Apple**. It uses ad hoc
signing rather than an Apple Developer ID certificate, so macOS may block the
first launch because it cannot verify the developer or check the app for malware.
If you trust the download from this project's Releases page, first try opening
MNO, then go to **System Settings → Privacy & Security → Open Anyway** for MNO
and confirm the prompt. This approves this specific app; keep your other security
settings unchanged. See [Apple's instructions](https://support.apple.com/en-gb/102445).
If macOS reports malware or damage, stop and report the warning rather than
forcing the app to open.

### Updating

Create a journal backup using MNO's backup feature before updating, then quit MNO.
Download the new DMG and replace **My Nature Observations.app** in Applications.
Your journal is stored separately from the app in
`~/Library/Application Support/org.mynatureobservations.app`; **do not delete this
data directory**. Replacing the app bundle preserves it.

## Backup and restore

In **Settings → Data & security → Backup**, choose **Create backup…** and save
the `.mno-backup` file somewhere safe, preferably on a separate drive or in a
backed-up location. Wait for MNO to confirm that the backup was created.

To restore, choose **Restore…** in the same section, select an MNO
backup and follow the validation and confirmation steps. **Restoring replaces
your current journal; it does not merge entries.** Create a fresh backup first.
MNO retains a safety backup of the previous journal. Follow the app's restoration,
Quit/reopen and verification instructions until the journal is available again.
Do not manually remove recovery files or the journal data directory.

## Feedback & Support

MNO is preparing its first public preview, and constructive feedback is welcome.
Use [GitHub Issues](https://github.com/MyNatureObservations/my-nature-observations/issues)
for bug reports and concrete improvement proposals that need tracking. Use
[GitHub Discussions](https://github.com/MyNatureObservations/my-nature-observations/discussions)
for questions, ideas and general conversation.

## Principles

- Local-first and offline-capable
- User ownership of data
- Open and documented export formats
- Privacy by default
- Accessible, thoughtful design
- Simple, maintainable architecture

## Development documentation

This repository starts from a reviewed source snapshot with a new Git history.
Earlier development records are retained for technical context; their issue/PR
numbers and commit IDs refer to a private historical archive, not this repository.
See [documentation provenance](docs/README.md).

## Technology

Built with Tauri 2, SvelteKit, TypeScript, TipTap, Rust and SQLite.

## License

My Nature Observations is licensed under the [MIT License](LICENSE).
Copyright (c) 2026 Stefan Antonsson.

Third-party dependencies and assets retain their respective licenses; the project
license does not relicense them. Bundled third-party license texts, required notices
and other applicable compliance obligations must be addressed before public v0.1.
