# ADR-001: Frontend Framework and Rich Text Editor

- **Status:** Accepted
- **Date:** 2026-10-12
- **Project:** My Nature Observations (MNO)

## Context

MNO is an open-source, local-first nature journal for macOS, Windows and Linux, built with Tauri 2, Rust and SQLite.

The application needs a calm, book-like writing experience and a maintainable frontend that can later support photographs, species observations, locations, maps, search and statistics.

Two editor prototypes were implemented and evaluated:

1. Svelte 5 with Tiptap 3
2. React with Lexical

Both were tested with the same visual design and core formatting features, including headings, bold, italic, bullet lists, undo and redo. The user reported no noticeable difference in writing experience.

The Svelte/Tiptap prototype was additionally tested with JSON persistence in browser localStorage. Text and formatting were successfully restored after reloading the page.

## Decision

MNO will use:

- **Desktop shell:** Tauri 2
- **Frontend:** Svelte 5 with TypeScript
- **Rich text editor:** Tiptap 3
- **Build tool:** Vite
- **Application backend:** Rust
- **Database:** SQLite

## Rationale

- Svelte supports a focused, component-based interface with relatively little boilerplate.
- Tiptap provides the required editing features with a straightforward extension model.
- The prototype demonstrated the desired writing experience and successful JSON round-trip persistence.
- Neither Svelte nor Tiptap prevents future galleries, species records, maps or statistics.
- The chosen combination fits the project's preference for simplicity and long-term maintainability.

## Alternatives considered

### React + Lexical

The prototype met the tested functional and visual requirements. React has a large ecosystem, and Lexical is a capable editor. However, implementing the tested formatting and toolbar behavior required more integration code in our prototype. No user-experience advantage was observed.

### Other combinations

Tiptap can also be integrated with React. Framework and editor are conceptually separate choices; the two prototypes do not establish that all other combinations are inferior.

## Data architecture implications

- Rich text will be stored as structured, versioned document data.
- Images and original files will be managed separately and referenced from journal entries.
- Species observations, locations and other domain records will be stored as structured SQLite data.
- Journal entries may link to these records without embedding the complete domain model in editor JSON.
- Future document-format changes must support migration and preservation of user data.

## Validation and outstanding work

Validated:

- Editing and formatting in both prototypes
- Comparable writing experience
- JSON save and reload in the Svelte/Tiptap prototype

Not yet validated:

- SQLite persistence through Tauri/Rust
- Crash-safe autosave and recovery
- Document schema versioning and migrations
- Image linking, previews and embedded media
- Cross-platform packaging and filesystem permissions
- Export/import fidelity and accessibility testing

## Consequences

The Svelte/Tiptap prototype becomes the reference for the initial journal editor. The React/Lexical prototype remains in the repository as an evaluated alternative.

Future development will proceed with Tauri 2, Svelte 5, Tiptap 3, Rust and SQLite. This decision can be revisited if new technical evidence warrants a change.
