# My Nature Observations — Concept v0.4

**Status:** Working concept, 9 October 2026\
**Tagline:** *Your nature. Your observations. Your way.*\
**Identity:** *A personal nature journal.*

> This document describes product intent, not a committed implementation or release plan. It distinguishes foundational principles, agreed directions, future possibilities, and unresolved design questions.

## 1. Purpose

My Nature Observations (MNO) is a calm, personal library for nature experiences. Its heart is the **journal**, not a taxonomic database or a reporting dashboard. A visit may involve birds, butterflies, plants, mammals, landscapes, weather, photographs, and reflections; these belong to the same experience without being forced into separate species-group applications.

MNO should help people record, connect, preserve, explore, and rediscover their nature memories. It complements specialist tools instead of trying to replace them.

**Product promise:** *Your journal. Your data. Your control.*

## 2. Foundational principles

### No lock-in. Ever.

All user-authored information, meaningful relationships, original dates and places, provenance, and media references must remain recoverable through documented, open, human-readable exports. The library must remain useful if MNO or a connected service disappears. Backup and restore are essential, not optional polish. Linked original media must never be silently deleted or moved by MNO.

### Local-first, private, and offline-capable

Core journaling, browsing, searching stored information, and editing should work without an account, subscription, cloud service, or constant connectivity. External services are optional enhancements. MNO is intended to be free and open source.

### User agency without unnecessary work

**MNO does the work. You make the choices.** Strong defaults keep the first experience simple, while useful flexibility remains discoverable. Suggestions are optional; inferred facts are not silently promoted into confirmed observations. MNO should never impose a mandatory sequence of activities or manufacture a backlog of chores.

### Historical truth and reversible changes

**The past may be corrected, but it must never be silently rewritten.** Observation date/time, recorded place, creation time, import/sync time, original source values, and subsequent edits are distinct. Preserve provenance and previous values. Detect suspicious bulk changes; support comparison and recovery of individual changes and erroneous imports. A changed taxonomy or external-service record must not overwrite what the person originally recorded.

### Quiet by design

No system notifications or reminders when MNO is not actively open. Inside the library, unobtrusive invitations and rediscovered memories may appear, with an option to turn them off. No streaks, guilt, urgency, gamification, or forced catch-up.

### Platform independent by design

Target macOS, Windows, and Linux without committing prematurely to a framework. Separate interface language from species names and taxonomic identities; design for localization.

## 3. The library metaphor

The library is a coherent place rather than a collection of disconnected modules:

- **Books / journals:** personal, flexible narratives.
- **Pages / entries:** dated or undated memories, photographs, notes, and links.
- **Bookshelves:** persistent organization and collections.
- **Coffee table:** a calm space for current books, recent activity, and an occasional rediscovered memory; automatic, manual, or hybrid selection may be supported.
- **Archive cabinet:** deliberately set-aside journals or entries, still readable, editable, restorable, and discoverable.
- **Librarian:** optional, helpful context and suggestions, never an intrusive agent.

A newcomer should be able to open a library, create a journal, and write on a blank page. Help and advanced capabilities can be available without mandatory onboarding.

## 4. Journals and entries

- Plain, comfortable writing is the default; images, media links, and more expressive layouts can be added without requiring them.
- An entry may have an exact date, approximate date, or no known historical date. Its historical date must not be confused with when it was created or imported.
- Empty calendar days do not generate compulsory blank entries.
- Reading may be continuous or page-based without changing the underlying entry.
- Journals can offer configurable opening behavior (beginning, end, today, last location, bookmark, or overview).
- Writing saves automatically. An optional **Save and…** action can end a session by going home, switching journals, or closing MNO; it does not declare an entry finished.
- An original entry may receive a **later reflection** with its own date. The new perspective is linked to the old memory without rewriting the original account.

## 5. Connected nature information

The narrative remains the person's own writing. MNO may recognize mentions of species, places, dates, or weather as a non-destructive semantic layer. Mentioning a species is not necessarily observing it: “I hope to see a woodpecker” is not a verified sighting.

Recognized terms in editable text must not hijack ordinary clicks or cursor placement. Deliberate actions can open related information or propose an observation. The user confirms facts before they become authoritative records.

Observations, places, species, outings/visits, journal entries, and media can be related without forcing every experience into a formal checklist or requiring a formal outing record. One journey may span multiple days, visits, and eBird checklists; the same place can be visited more than once in a day.

Species groups should be data, not hard-coded applications. Names in different languages, taxonomic identities, and historical identifications must be kept conceptually distinct.

## 6. Observations, forms, and exploration

Observations can be entered manually or imported from external sources. MNO should allow personal forms and reusable templates with meaningful custom fields, including both form and table views over the same data. It may suggest field types or shared meanings, but the person decides. Changes to standardized or community templates require clear previews and reversible migrations that retain original information.

Exploration should combine familiar views (life, year, month, place) with flexible filters and optional tables, charts, maps, and timelines. The aim is useful discovery, not an overwhelming analytical dashboard. Saved personal views are a possible direction.

## 7. Two independent workflows: observations and photography

**Independent workflows. Meaningful connections. On your terms.**

The current reference workflow is intentionally asynchronous:

1. The person records nearly all birds seen or heard in eBird on a phone while outside.
2. eBird observations are synchronized into MNO on their own schedule.
3. Camera RAW files are copied into an existing year/month/day or trip-folder hierarchy.
4. The person culls and edits photographs in Luminar Neo, often days or weeks later.
5. Selected JPEGs are exported to the corresponding folder structure. Most RAW files are eventually deleted by the person; a few may be retained.
6. When ready, the person works in MNO to enrich photographs and link them to already-recorded observations.

MNO must not make photo editing depend on eBird synchronization, or vice versa. It may prepare candidate links in the background, but must never demand that the person process unreviewed photographs.

### Media links and identity

- Respect existing folders; link to originals without mandatory copying, moving, or renaming.
- A camera filename/sequence number is a useful clue across RAW and JPEG, **not** a globally unique identity. Use EXIF, dates, paths, and other evidence carefully.
- Treat unreviewed RAW files as potentially temporary. Do not turn every scanned RAW into an unwanted permanent catalog record.
- A replacement JPEG can be proposed as the successor to a RAW; uncertain matches need confirmation.
- Missing linked files should not silently destroy confirmed observation, journal, or photo relationships.
- Image EXIF time and GPS may be absent or wrong. A date-only match is insufficient when multiple checklists or visits occur on the same day.
- Distinguish **possible association** from **user-confirmed association**.

### The MNO photo gallery

MNO can provide a nature-focused gallery where images are explored by species, place, outing, observation, and narrative. It complements rather than replaces Apple Photos, which remains the person's general photo library. Images can also be used in journals and curated albums without needless duplication.

### Starting from either direction

A person can start from an observation and add photographs, or start from a photograph and find its observation. The latter matters when a selected image inspires the thought, “This should go into eBird.” In the current workflow the observation almost always already exists, so the primary need is to find it rather than create another record.

MNO can help open the appropriate eBird checklist or observation in a browser, as deeply as the service allows. Direct uploading is **not a requirement** and must not be promised before integration capabilities are verified. Sometimes showing the right page is enough.

A future handoff from Luminar Neo or a file manager—“Open/link this folder in MNO”—is desirable if technically feasible. The standard workflow must work entirely from within MNO regardless.

## 8. External services and contextual information

MNO complements eBird, Artportalen, iNaturalist, and other specialist services; it does not attempt to replicate every reporting workflow. Preserve stable external identifiers and source links where available, and keep local data usable independently.

Optional historical weather or other externally retrieved context may be previewed on demand. Temporary information is not silently added to the permanent library or search index. If saved, source, retrieval context, uncertainty, and the person's approval matter. Manually recorded weather and sourced historical weather are different kinds of evidence.

## 9. Rediscovering memories

The library is not only for capturing information; it is for enjoying it again.

An occasional, private **Remember this?** item can appear on the coffee table while MNO is open. It might recall the same calendar day in another year, a surprising photograph from another season, an older trip, or visits to the same place across years. It need not change every launch or become a feed.

No outside-app notifications, no engagement pressure, no obligation to respond. A rediscovered memory may inspire a new, separately dated reflection linked to the original record.

**Memories can grow without rewriting their history.**

## 10. Archiving, indexing, and performance

Journals and individual entries can be archived deliberately, restored, and edited. Archiving may reduce background work on heavy narrative/media content, but structured nature facts—species, observations, dates, times, locations, and relationships—must remain available for ordinary searching and exploration. Search can link back to archived source narratives. Performance behavior should be based on actual needs, not artificial restrictions.

## 11. Albums, sharing, and optional intelligence

Albums are curated stories combining photographs, observations, journal excerpts, maps, and other context. Potential modes include live-linked and frozen snapshots, printable PDFs, and self-contained copies for sharing without granting ongoing access to the private library. Rights and provenance matter.

An optional AI librarian may assist with finding connections, drafting, or interpreting personal data. Local/offline options are preferred where feasible. External AI services are guests requiring specific permission and transparent data access; their suggestions must remain reviewable and reversible. AI is not required for the core library.

Optional shared templates or extensions may be offered through a free community “gift shop,” without a social feed or monetization being central to MNO.

## 12. Flexible capture and future audio

**Capture should be flexible and independent of format.** A memory may begin as text, a photograph, an observation, or eventually an audio note; no single device or sequence should be compulsory.

**Future possibility, not first-version scope:** record a quick voice note (for example, weather at a location), retain the original audio, optionally transcribe it, and propose structured facts for approval. Transcription errors must not replace the recording as historical evidence. Mobile capture may be especially useful, but a mobile application has not been committed to.

## 13. What belongs in the first release?

**Not yet decided.** The next design phase is to identify a small, coherent, genuinely useful first library. This document does not imply that every described feature belongs in v0.1 or v1.0.

Candidate foundations to evaluate: create/open a local library; a comfortable journal and entries; dependable local persistence and export; a minimal observation model; basic linking between journal, observations, and media; safe backups and provenance. The scope, order, and technical feasibility remain open.

## 14. Open design and research questions

- First-version user journey and smallest complete feature set.
- Data model for journal entries, visits, checklists, observations, media identities, provenance, and version history.
- Practical mechanisms for migration rollback, backup verification, and lossless open export.
- Linking/scanning large external photo collections, missing files, and RAW-to-JPEG succession.
- Available eBird deep links, APIs, and permission constraints; possible Luminar/file-manager handoff.
- Taxonomic data sources, local caching, multilingual names, and historical taxonomy.
- Scope and UI for configurable forms, searching, and saved exploration views.
- Cross-platform technology and release tooling; no framework is selected yet.
- Optional AI and audio privacy, permissions, and local-processing feasibility.

## 15. Project and working approach

MNO is a **new project**, not a continuation or refactor of the older *Mina naturobservationer* application. The older application is a working reference and source of lessons; code reuse may be selective, never assumed. Public, open-source development is intended from the beginning, with no deadline.

Work in small, testable steps: discuss and design, implement one contained change, test, and continue. Favor the actual user's lived workflow over speculative feature lists, while keeping the foundation extensible for others. Research existing standards and tools before building custom replacements.

---

### Concept statements worth keeping

> Your nature. Your observations. Your way.

> No lock-in. Ever.

> MNO does the work. You make the choices.

> Understand the journal without rewriting it.

> Independent workflows. Meaningful connections. On your terms.

> A good librarian sometimes only needs to show you the right page.

> The past may be corrected, but it must never be silently rewritten.

> Memories can grow without rewriting their history.

> A nature library isn't only about preserving memories. It's about rediscovering them.
