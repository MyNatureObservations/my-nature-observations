# My Nature Observations — Concept v0.5

**Status:** Working concept, 10 October 2026\
**Stage:** Product discovery and experience design — not an implementation specification\
**Tagline:** *A personal nature journal.*\
**Promise:** *Your nature. Your observations. Your way.*

> **Document note:** This edition consolidates decisions from our concept discussions, including the developments after v0.4. It is a reconstructed consolidation, not a line-by-line revision of an available v0.4 source file. Technical choices and unresolved questions are explicitly left open.

## 1. Purpose and identity

My Nature Observations (MNO) is a personal, local-first nature journal and connected library of memories, observations, media, places and knowledge. It is **not primarily a species database**, a scientific research platform, a social network or a cloud storage provider.

The journal is the emotional center. The library connects the information behind it. The same underlying information can be experienced as journal pages, books, albums, maps, searches, dashboards, timelines and other presentations without creating needless duplicates.

MNO is a **new project**, distinct from the earlier *Mina naturobservationer* application. The earlier app remains a useful working reference, not the architecture or codebase that the new product must inherit.

### Guiding principles

- **No lock-in. Ever.** Your journal. Your data. Your control.
- **The interface whispers. Your story speaks.**
- **MNO does the work. You make the choices.**
- **Power without pressure.** Strong capabilities without mandatory administration.
- **Structure what exists. Never demand what is missing.**
- **Your data is contributed, never harvested.**
- **Automate what is known. Suggest what is inferred.**
- **One library. Many lenses. Infinite stories.**
- **Explore freely. Never lose your place.**

## 2. Privacy, ownership and independence

MNO works offline without a required account, subscription, proprietary cloud or continuous network connection. It is intended as free/open-source software designed for macOS, Windows and Linux, with tablet-ready interaction and possible mobile support later.

The reader controls where libraries and files live, how they are backed up and whether they are synchronized or shared. Portable exports should use documented, readable formats and retain enough content and context to remain useful even if MNO disappears.

**Contribution requires intention.** Writing a note, choosing an image, importing a file or confirming a relationship are deliberate acts. Permission to browse a folder is **not** permission to ingest or index everything inside it. MNO may index and analyze content that has been contributed, but must not silently harvest external folders or services. Outbound sharing requires explicit choice.

## 3. The journal: write first

On first launch, the reader encounters an open, editable journal page rather than a required setup wizard. An optional prompt can invite them to write what they want to remember. Date and title are editable; no species, location or event is required.

Journal pages support free prose, photos, other media, spatial composition and optional templates. Visual annotations and overlays do not alter original media. Historical event dates remain distinct from creation and edit timestamps. Ordinary writing supports undo and crash recovery, but does not require an infinite revision history of every keystroke. Deliberate snapshots remain possible.

A simple note about rain is as legitimate as a detailed species record. Memories, feelings and impressions retain value without being forced into measurable data.

Reading is a separate, distraction-free experience rather than a complicated additional authoring mode. Text can reflow for accessible reading while deliberately composed layouts are respected where possible.

## 4. The library, desk and archive drawer

The **desk** is where the reader works. The **library** is where they retrieve and rediscover. The library can be pulled down across the workspace like a classroom map, or opened as a contextual side drawer like a filing cabinet. Both lead to the same information.

Books and journals have identities independent of their placement. A book can appear on multiple shelves without being copied. Removing it from a shelf does not delete it. Default organization is simple (such as alphabetical title order); optional manual organization must never become a prerequisite for using MNO.

A contextual drawer may suggest material related to the current page while always permitting unrestricted exploration of the available library. Content can be inserted by drag-and-drop or accessible alternatives.

Journals may continue indefinitely or be deliberately closed into volumes. Automatically prepared yearbooks are **living presentations**, containing only actual material and natural chronological boundaries; empty calendar years do not create empty automatic volumes. Readers can create thematic books, series and photo books, or preview the librarian's proposals without saving them.

Reusable templates may be complete books, sections or individual layouts, including optionally shared community blueprints. Templates are editable; private content is not included in a shared template without explicit choice.

## 5. A connected information foundation

Observations, media, species, places, dates, notes, events, sources and collections share a common relationship model. They are not trapped in separate bird, butterfly or plant modules.

- **Events are optional.** An observation may stand alone or belong to an outing; a project or collection may span many outings.
- **Nature is broader than species.** Frost, lake ice, weather, budding trees, landscapes, tracks and unidentified organisms are valid subjects.
- **Uncertainty is meaningful.** Approximate dates, places, quantities and identifications retain their stated precision; MNO must not invent exact values.
- **Places are reader-defined.** Named areas, polygons, subareas and imported localities can be related without falsely treating nearby sites as identical. Membership in a clearly defined area can update automatically; uncertain boundary cases must remain uncertain.
- **Taxonomy may evolve.** Current taxonomy can inform present-day views while original identifications and source provenance remain preserved. MNO must not silently guess the result of a species split.
- **Species registers are optional.** Missing taxonomic coverage may prompt an offer to import or connect a suitable source, never a silent download.
- **Personal prose stays personal.** The librarian may suggest that text describes an observation, but the reader chooses whether to structure it.

## 6. Information frames and collections

Each contributed media object can carry an MNO **information frame**: notes, species identification, date, place, provenance and confirmed relationships. These belong to the object and follow it wherever it is referenced. A caption written only for one book page belongs to that presentation instead.

Collections can group photographs or mixed media without stripping individuals of their identities. A photo may belong to several collections. Shared collection metadata does not automatically assert that every member has the same properties.

Collections are **living by default**: new members can appear wherever the collection is referenced, while frozen selections preserve a chosen moment. Updates must not silently rearrange carefully composed journal pages.

## 7. OH-film: presentation without duplication

The **OH-film** is MNO's presentation layer: a transparent perspective over selected information from one or more libraries. It can arrange observations, photos, prose, maps and analyses into pages, stories, books or other views without merging or duplicating the underlying objects.

- **Live OH-films** reflect changes to their referenced source information.
- **Frozen OH-films or snapshots** preserve a deliberate edition or result.
- A presentation can reference multiple independent libraries.
- Readers may share an element, page, selection or complete work — **not necessarily the whole book**.
- A shared standalone edition contains only explicitly selected material; it does not grant access to source libraries.

**Share the view, not the library.**

## 8. Multiple independent libraries

MNO supports multiple self-contained libraries, such as a personal nature journal, bird observations, fungi, a club project or research material. They can have independent content, storage, backup and synchronization choices. Their technical storage layout is **not yet decided**.

Readers may search, explore, analyze and create OH-films across selected available libraries as one experience. Cross-library content may be **linked** (live reference), **copied** (independent object) or **moved** (explicit transfer). What crosses a library boundary must be visible and reader-controlled; sensitive fields need not accompany a shared view.

A library being unavailable on one device should not destroy the meaning of an existing reference. MNO should retain whatever lawful, locally available reference context exists and clearly explain what cannot currently be opened.

## 9. Search, analysis and the librarian

**One search**, scoped to selected available libraries, can find prose, observations, places, photos and related objects. It supports an exploratory grouped view and a text-search view with snippets, filters and direct navigation to originals.

The **analysis workshop** offers understandable pivot-table-inspired exploration: grouping, filtering, tables, maps, timelines and charts. Suggested views are starting points, not mandatory dashboards. A saved **snapshot** preserves a result and its counting rules; a saved **dashboard** preserves a live perspective. Neither needs to be saved for a quick question.

The **librarian** is a quiet helper, not a scientific authority or an obligatory AI chat. She may identify promising connections, memories, inconsistencies and patterns **within contributed material**. Readers can browse a list of discoveries in her workroom, inspect evidence and choose what to pursue. Not every pattern warrants a notification.

The discreet **idea lamp** lights only for genuinely noteworthy new findings. Suggestions can be confirmed, rejected, ignored in context or left alone; silence is not consent. Rejected proposals are not repeatedly resurfaced without materially new evidence. The librarian does not automatically turn speculative relationships into facts.

MNO helps people rediscover *their own* nature history. Specialized scientific inference and research workflows belong to dedicated tools.

## 10. Relationship graph and consequences

An optional **interactive relationship graph**, inspired by CMDB relationship/dependency views, reveals how observations, images, places, journal pages, OH-films and libraries connect. Readers can navigate progressively rather than viewing an overwhelming whole-library web.

The graph is also editable: readers can create, modify and remove relationships directly, with keyboard and touch-accessible alternatives. Confirmed links and librarian-proposed links are visually distinguishable (for example solid versus dashed lines). Proposals include their supporting evidence and can be accepted or rejected from the graph.

MNO should show **where an object is used** and, before consequential changes, distinguish:

- live references that will update;
- analyses or counts that may change;
- frozen editions that will remain unchanged;
- uncertainty about any predicted effect.

Warnings must be proportionate: not for every minor edit. Ordinary changes support undo. The graph is a temporary exploration and editing workspace; **saved graph documents are not a core requirement**.

**Understand the consequences. Keep control of the change.**

## 11. Media, linked originals and previews

Readers contribute media in a **meaningful context**, such as an observation or journal page, not into an obligatory loose-file inbox. In a photography workflow, finished, deliberately chosen photographs may be added days or weeks after the journal entry; MNO should not monitor memory cards or unprocessed photo folders.

The default is to **link to an original file**, with an optional managed copy. MNO stores a lightweight thumbnail ('stamp') and, when useful, a larger reading-quality preview for contributed images. Original files remain unmodified and in reader-selected storage.

A per-device **preview storage budget** sets the limit; MNO manages disposable previews inside it, prioritizing useful content and honoring explicit offline preservation choices. Permanent knowledge, annotations and references are **not** disposable cache. Missing/offline originals do not erase their information frames or memories; relinking should be possible.

Other contributed media (audio, video, documents, GPX and structured data) follow the same consent principle with appropriate viewers and metadata.

## 12. Import, synchronization and reporting

Import, sync and outbound reporting are distinct operations, presented as a coherent reader-facing workflow appropriate to each service's actual capabilities. Possible integrations include eBird, Artportalen and iNaturalist; none is required to use MNO.

**Authority follows provenance.** Source-managed eBird records remain sourced to eBird for relevant synchronization, while manually created MNO records remain reader-managed. Sync previews distinguish additions, changes, removals and conflicts. Removing an observation from its source can remove it from active statistics while preserving appropriate traceability and protecting separate reader-authored content. Matching source and manual records must not cause double-counting of one real observation.

If MNO knows that two sources disagree, it should discreetly tell the reader, show evidence and freshness, and leave external corrections under reader control.

MNO will **not** offer proprietary cloud storage. Optional synchronization may use reader-chosen providers (such as Dropbox, WebDAV or self-hosted storage), with local usable libraries on each device, offline operation and careful conflict handling. A live database file should not simply be assumed safe to share through a generic file-sync folder. Encryption of synchronized data is a design consideration to resolve during architecture work.

**Synchronization is selective per library.** For example, birds may sync to Mac and tablet while fungi remain only on the Mac. Media previews and originals have separate transfer and storage policies.

## 13. Backup, portability and deletion

A **library-only backup** preserves actual stored observations, journal content, metadata and relationships, not merely pointers to imported source services. An optional **inclusive portable backup** can also collect linked original files explicitly selected for inclusion. Backup receipts must transparently state included, excluded, inaccessible and verified content. Migration may help reconnect moved media with reader-approved matching.

Removing an object from a page or shelf removes **that reference**, not the underlying object. Deleting a library object is a separate act. MNO supports either a recoverable trash workflow or direct deletion, according to reader preference; permanent deletion requires clear, proportionate confirmation. Deleting a linked item from MNO does not delete its external original file.

**Deletion is a choice, not a surprise.**

## 14. Workspace, navigation and accessibility

MNO should feel like one coherent environment rather than separate programs for journal, search, graph and analysis. Temporary tools can open beside or over current work and close without losing the original context. Navigation supports back, forward and return to starting point.

On desktop, tabs, split views and detachable independent windows may support multiple monitors. Do not impose a system of named workspace presets. Startup offers a simple choice: **restore the previous session** or **open the chosen home** (journal, library or dashboard). A journal can choose its last page or a fresh page when opened as home.

An optional guided introduction coexists with subtle contextual help. Accessibility is foundational: scalable text, contrast, screen-reader semantics, keyboard operation, understandable reading order, and alternatives to hover, right-click and drag-only actions. Tablet input is considered from the outset; a future mobile field-capture experience may be designed later.

**Explore freely. Never lose your place.**

## 15. Three experience checks

### A. After a birdwatching trip

The reader writes freely in a journal and chooses when to sync eBird data. Photographs are added **only when deliberately selected**, often after external editing. Adding a photo to a journal automatically establishes its use on that page; MNO does not infer that it depicts a particular imported observation without confirmation.

### B. An unidentified mushroom in the field

A possible later mobile workflow captures a quick observation without requiring species identification, exact place or complete metadata. The reader enriches it later. This is a future capability, not a requirement for the first desktop release.

### C. Ten years of rosefinch observations

A cross-library search leads to an exploratory analysis, linked journal memories and photographs. The reader can explore without saving, or preserve a live perspective or frozen result. The librarian may offer meaningful discoveries but must not turn personal observation counts into unsupported scientific claims.

## 16. Deliberately open decisions

These are **not yet implementation commitments**:

- Database and storage architecture for independent libraries and cross-library references.
- Sync protocol, conflict resolution, encryption, provider support and offline media availability.
- Detailed OH-film representation, live/frozen semantics and portable export packaging.
- Desktop/tablet framework and possible future mobile app scope.
- Precise taxonomy sources, licensing and update mechanism.
- Accessibility acceptance criteria, media preview policies and practical performance limits.
- First-release feature boundaries and incremental delivery plan.

The next phase should continue validating the experience and its simplicity before selecting technology. **Not every possible capability needs to become a feature.**

---

*My Nature Observations — Your nature. Your observations. Your way.*
