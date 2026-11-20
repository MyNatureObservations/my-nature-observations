# My Nature Observations 0.2.0 for macOS — draft

> Publication paused. These notes describe the forthcoming replacement candidate.
> The accepted `1685de6` DMG does not contain optional-module activation and remains
> unchanged; its existing release draft stays unpublished. Replacement package
> acceptance is pending.

## English

### What’s new

- Bird observations are optional on macOS. Enable them in Settings → Modules →
  Bird observations. New and upgraded installations start with the module off.
  Turning it off hides bird features without deleting their content; turning it
  back on restores access. Journal photos, the shared viewer and journal weather
  remain core. Journal replacement does not change this account-level setting.

- Register bird observations offline with AviList v2026 species search in Swedish,
  English and scientific names. Each visit has its own place, date, optional times,
  species counts and comments. Create, reopen, edit or delete visits independently.
- Keep observations standalone or link them to several journal entries. Paste a
  species list for explicit review before saving. Name-display preferences are remembered.
- Add one optional photo and caption per species row. A shared viewer presents
  journal and observation photos while keeping your original files untouched.
- Add independent manual or fetched daily visit weather. Today’s daily data is a
  forecast; changed visits clearly flag previous weather for review.
- Continue opens the combined nature diary. Calendar markers distinguish journal
  entries and bird visits, including standalone visits. Home and the sidebar
  provide quieter, clearer entry points.
- Backup/Restore and Export/Open Import preserve observation content, links,
  stored photo previews/captions and visit weather, including unlinked lists.

### Installation and data compatibility

For Apple Silicon Macs running macOS 13 or later. Quit MNO before replacing the
app. **Create an external backup before upgrading and retain it.**

This version upgrades the journal database to schema 12. Older app versions cannot
be assumed to reopen the upgraded journal. Replacing the app with an older copy
is not a database downgrade. Keep the pre-upgrade backup and protected source copy;
do not attempt to open the upgraded journal in an older app. Original photo files
follow the existing backup policy and are not copied into backups/exports.

This candidate uses ad hoc signing and is not Apple-notarized. macOS may display
security warnings; verify the official source and the accompanying SHA-256.
Do not disable macOS security protections globally.

### Scope and known limitations

This is a macOS candidate, not a Windows/Linux bird-feature release. Existing
platform gates remain. AviList is bundled as a fixed v2026 catalog; catalog updates,
general BirdLife names, regional search/maps and statistics are not included.
Missing Swedish names use the recorded fallback. In macOS DEV, clicking blank
space to the right of a species result may require two clicks (UX-005); clicking
the species name works directly. Native file-picker buttons may use the system
language independently of MNO. Historical packaged macOS acceptance passed for the earlier `1685de6` candidate,
which did not include optional-module activation. Replacement acceptance is pending.
In that earlier test: upgrade from public
v0.1.1 preserved text, photo and caption, and a new edit persisted after restart.
Bird observations and separate-account Backup/Restore and Export/Open Import
also passed, including linked and unlinked observations. See the
[acceptance record](MNO_macOS_0.2.0_RC_Acceptance.md) for artifact identity and scope.

## Svenska

### Nytt i versionen

- Fågelobservationer är valbara på macOS. Aktivera dem under Inställningar →
  Moduler → Fågelobservationer. Modulen är avstängd från början även efter
  uppgradering. Avstängning döljer funktionerna men behåller innehållet;
  återaktivering visar det igen. Journalbilder, gemensam bildvisare och
  journalväder är kärnfunktioner. Journalbyte ändrar inte kontots inställning.

- Registrera fågelobservationer offline med AviList v2026 och sökning på svenska,
  engelska och vetenskapliga namn. Varje besök har egen plats, datum, frivilliga
  tider, arter, antal och kommentarer. Skapa, öppna, redigera eller radera besök.
- Spara observationer fristående eller koppla dem till flera journalanteckningar.
  Klistra in artlistor och granska dem före sparande. Namnvisningsval koms ihåg.
- Lägg till ett frivilligt foto med bildtext per artrad. Den gemensamma bildvisaren
  visar journal- och observationsbilder och lämnar originalfilerna orörda.
- Ange eget manuellt väder eller hämta dygnsväder för besöket. Dagens uppgifter är
  prognoser; när besöket ändras markeras tidigare väder för granskning.
- Fortsätt öppnar den gemensamma naturdagboken. Kalendern skiljer anteckningar från
  fågelbesök och visar även fristående observationer. Startsidan och sidopanelen
  har tydligare, diskreta ingångar.
- Backup/Restore och Export/Open Import bevarar listor, länkar, lagrade
  bildförhandsvisningar/bildtexter och besöksväder, även för olänkade listor.

### Installation och datakompatibilitet

För Mac med Apple-chip och macOS 13 eller senare. Avsluta MNO innan appen ersätts.
**Skapa en extern backup före uppgraderingen och behåll den.**

Versionen uppgraderar journalens databas till schema 12. Äldre appversioner kan
inte antas kunna öppna den uppgraderade journalen. Att lägga tillbaka en äldre
app nedgraderar inte databasen. Behåll backupen från före uppgraderingen och den
skyddade källkopian; försök inte öppna den uppgraderade journalen med en äldre app.
Originalbilder omfattas av befintlig backuppolicy och kopieras inte in i arkiven.

Kandidaten är ad hoc-signerad och saknar Apple-notarisering. macOS kan visa
säkerhetsvarningar; kontrollera den officiella källan och medföljande SHA-256.
Stäng inte av macOS säkerhetsskydd globalt.

### Omfattning och kända begränsningar

Detta är en macOS-kandidat, ingen fågelleverans för Windows/Linux. Befintliga
plattformsgates behålls. AviList är en fast medföljande v2026-katalog; uppdateringar,
allmänt BirdLife-namnstöd, regional sökning/karta och statistik ingår inte. Saknade
svenska namn använder beslutad reservvisning. I macOS DEV kan blankytan till höger
om en artträff kräva två klick (UX-005); klick på artnamnet fungerar direkt.
Filväljarens systemknappar kan följa ett annat språk än MNO. Paketerad macOS-
acceptans är historiskt godkänd för den tidigare kandidaten `1685de6`, utan valbar
modulaktivering. Ersättningskandidatens acceptans återstår. I det tidigare testet: uppgradering från publicerad
v0.1.1 bevarade text, foto och bildtext, och en ny redigering fanns kvar efter
omstart. Fågelobservationer samt Backup/Restore och Export/Open Import i separata
konton godkändes också, inklusive länkade och olänkade observationer. Se
[acceptansprotokollet](MNO_macOS_0.2.0_RC_Acceptance.md) för kandidatens identitet
och testets omfattning.
