# Settings polish and About MNO — issue #14

Implemented on main's PR #34 notice infrastructure, 2026-11-01. No journal formats, database operations, backup/Restore/Recovery flows, dependencies or installed applications are changed.

## Implementation

- `AboutMno.svelte`: compact native disclosure in Settings, approved icon, running application version from Tauri `getVersion()`, MIT identification and expandable **original repository LICENSE text**, GitHub link through the existing opener, and an offline third-party notice action. All interface strings use Paraglide in Swedish and English. Legal source texts remain verbatim. No hard-coded version or application installation path.
- `about.rs`: the parameterless `open_third_party_notices` command resolves the fixed `notices/THIRD-PARTY-NOTICES.html` through Tauri's `BaseDirectory::Resource` API, checks a nonempty regular file can actually be read, then uses the existing opener plugin. macOS's detached opener alone may not report a missing file. Resolution, read and launch failures return one safe code; the UI displays a localized message without a path or stack trace. No broader filesystem/open-path permissions or new dependencies are needed.
- `AppNavigation.svelte`: Data and Security now uses the same 18px decorative icon, 13px summary typography, padding, focus indication and disclosure chevron treatment as Language and Localization. Its existing h3 subsections and all four data actions remain in the original disclosure. About uses the same native disclosure pattern. No change to navigation guards or data-operation status handling.
- `PlaceMapPanel.svelte`: the shared map-based saved-place picker used by Settings and journal flows now gives all text/coordinate fields explicit appearance, border-box sizing, rounded borders, inherited font and contained focus. Labels, decimal input modes, values, validation, storage and event handlers are unchanged. The separate legacy EntryPlace fallback is not redesigned.

## Saved-place investigation

The shared picker had `appearance: auto`, square field corners, and the application's global 2px focus outline offset by 3px. Fields fill a 260px-wide scrolling aside: that outline projects 5px outside the field and can be clipped at the scrolling boundary. This explains the stray/partial focus lines without changing place data or validation.

An isolated native WKWebView probe with the actual before/after component styles and three synthetic labelled fields measured:

| Field state | Before | After |
| --- | --- | --- |
| Field / label width | 260 / 260 px | 260 / 260 px |
| WebKit appearance | auto | none |
| Border radius | 0 px | 6 px |
| Keyboard-visible outline | 2 px, offset +3 px | 2 px, offset −2 px |
| Outline extends outside field | yes | no |

The new focus treatment also applies to pointer focus. The probe used a temporary window and nonpersistent WebKit store, not a journal or MNO installation. This is focused native CSS/geometry evidence, not a claim that every application interaction or VoiceOver path has been manually accepted.

## macOS native dialog language

Confirmed code path: Tauri dialog → rfd → AppKit `NSOpenPanel`/`NSSavePanel`. MNO already supplies translated titles (e.g. `manual_backup.rs`), but native Save/Cancel/sidebar text belongs to AppKit. Paraglide `changeLanguage()` uses `setLocale(..., { reload: false })`; it does not change macOS preferences or native bundle language.

The previous bundle declared the default English development region but no `CFBundleLocalizations`. The minimal fix is `src-tauri/Info.plist`, automatically merged by Tauri, declaring **en and sv** as manually supplied localizations. It does not replace the development-region fallback, app identifier, version or environment configuration. No macOS preferences, private localization APIs or custom replacement dialogs are introduced.

Foundation language-selection checks using the **built DEV bundle's metadata** returned Swedish for `[sv-SE, en]`, English for `[en-US, sv]`, and English for unsupported French. The packaged bundle contains both declarations. This confirms supported-language selection, not visual dialog acceptance.

Supported contract: MNO-provided text follows Paraglide; system controls follow the macOS preferred/per-app language and fallback, potentially requiring a complete quit/reopen. **Live synchronization of AppKit controls with MNO's internal selector is deferred.** A packaged native-dialog matrix remains manual acceptance, not an automatic public-v0.1 blocker solely because a mismatched OS/app choice produces mixed languages.

Primary references: [Tauri Info.plist merging](https://v2.tauri.app/reference/config/#macconfig), [Apple CFBundleLocalizations](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundlelocalizations), [Apple preferred localizations](https://developer.apple.com/documentation/foundation/bundle/preferredlocalizations).

## Automated and local verification

- Paraglide compilation; Svelte/TypeScript: 0 errors, 0 warnings.
- **426 frontend tests passed**, including nine focused About/Settings tests. Actual rendered Swedish/English markup, injected runtime versions, resource errors, repeated clicks, disposal, disclosure semantics and field rules are covered. The existing navigation SSR fixture now stubs the new child; About itself has separate real-markup tests.
- **915 Rust library tests passed, 2 existing ignored**, including three new temporary-file tests: readable resources/open failures, missing/empty/directory resources, and genuinely denied read access. No user journal is used.
- Rust formatting and whitespace checks passed; optimized macOS DEV build succeeded.
- DEV bundle: `My Nature Observations DEV`, `org.mynatureobservations.app.dev`, version `0.1.0`, arm64. Packaged notices match all nine generated resource files byte-for-byte (345 component records, five MPL source archives). DEV frontend also serves the original MIT import successfully.
- No MNO application was installed or launched. Native WebKit and Foundation probes are synthetic. Packaged About clicks, VoiceOver and real native dialog text remain manual checks. Windows/Linux packaging and notice inventories remain unvalidated as documented in the notice guide.

## Manual acceptance (Swedish)

Bygg om vid behov från `app/`:

```sh
npm run tauri build -- --config src-tauri/tauri.dev.conf.json --bundles app
npm run notices:validate -- 'src-tauri/target/release/bundle/macos/My Nature Observations DEV.app'
```

1. Avsluta eventuell äldre DEV-instans. Öppna det byggda **My Nature Observations DEV.app** från `app/src-tauri/target/release/bundle/macos/` i Finder; installera eller ersätt inte Production. DEV använder sin befintliga separata katalog `~/Library/Application Support/org.mynatureobservations.app.dev` och visar DEV-markeringen.
2. Öppna Settings. Prova Enter/Space och Tab för Språk och lokalisering, Data och säkerhet samt Om MNO. Kontrollera ikoner, avstånd, fokus, öppna/stäng och befintliga dataåtgärders synlighet. Kör inte Restore/import för detta test.
3. Kontrollera Om MNO i svenska och engelska: ikon, namn, version mot paketets `CFBundleShortVersionString`, originalets MIT-text och GitHub-länken. Koppla från nätverket och öppna Tredjepartslicenser; följ en lokal MPL-källkodslänk. Nätverk krävs för GitHub, inte för licensnotiserna. Saknad/oläsbar resurs verifieras redan automatiskt med temporära filer; ändra inte en installerad apps resurser.
4. Öppna platsväljaren via standardplatsen för väder. Prova namn/latitud/longitud i ny/redigera-läget vid normal och smal fönsterbredd. Tabba mellan fälten, kontrollera en sammanhängande fokusram och avbryt utan att spara. Bekräfta befintligt val och fältvärden.
5. För en separat dialogkontroll: välj macOS programspråk svenska respektive engelska för DEV i Systeminställningar, avsluta helt och öppna igen. Öppna en befintlig native filväljare/spara-dialog och avbryt. Prova intern MNO-lokal sv/en i båda OS-fallen: appens titel ska följa MNO, systemkontroller ska följa macOS programspråk. Återställ testets språkval efteråt. Lova inte omedelbart språkbyte för systemkontroller.

In ordinary unbundled DEV, notices may not yet have been generated/copied; the localized resource error is intentional. Use the packaged DEV build for offline acceptance. #14 stays open for review and manual acceptance; no update checker or post-v0.1 feature is included.

## Native macOS menu localization — focused follow-up, 2026-11-02

Application-owned menu headings and controllable command labels follow MNO's
selected sv/en locale through Paraglide, including live changes. File/Edit/View/
Window/Help and About, Services, Hide/Hide Others, guarded Quit, Close Window,
Undo/Redo/Cut/Copy/Paste/Select All, Full Screen, Minimize/Zoom and the three
application-zoom commands are covered. The application-name heading remains a
proper name. Before frontend locale initialization, the native defaults remain
usable; zoom items are enabled only once their listener and labels are ready.

`macos_menu.rs` retains native item handles and stable submenu IDs, including
Tauri's Window/Help identifiers. Label updates never search for `"View"` or rebuild
menus/handlers. Menu structure, separators, predefined native selectors and
shortcuts remain intact; Quit still routes through the existing close guard.
`ApplicationZoom.svelte` reuses its serialized locale-update queue, independent
of zoom state/persistence. Menu-update errors have a separate localized status.

Platform boundary: labels inside OS-populated Services/Window content, native
About-panel content, and file-dialog/system controls are not rewritten. Their
language remains controlled by macOS or the supplying application. No OS language
or per-app language preference is changed. Native menu localization is macOS-only.

Local checks: 31 focused zoom/menu, editor-accessibility and environment tests;
Paraglide compilation; Svelte/TypeScript (0 diagnostics); frontend build;
`cargo check --lib --locked --offline`; Rust format and whitespace checks.
No packaged DEV application was built or launched. Manual macOS DEV acceptance
is pending: test initial saved locale and repeated sv/en switching; all headings
and commands; editing shortcuts; Full Screen/Minimize/Close; zoom actions and
persistence; and Cmd+Q/unsaved-dialog cancellation/discard. Confirm unchanged
file-dialog behavior. This follow-up does not reopen #14 or #12.
