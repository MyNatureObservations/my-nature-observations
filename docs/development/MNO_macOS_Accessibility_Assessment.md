# macOS-tillgänglighet inför v0.1.0 — #12

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

**Datum:** 2026-11-01. **Källbas:** main `6a46b3d942c4d5eacc82018b11ee91b10a55b4c6` (PR #35 mergad).
**Scope:** första offentliga macOS-versionen enligt D-019, befintligt ljust utseende enligt D-017.

**Ursprungsbedömningens metod (historik):** läsande kod-/DOM-/CSS-granskning,
inte implementation eller WCAG-certifiering.
Projektägarens godkända PR #35-CI och manuella DEV-acceptans beaktas; Settings ska inte
byggas om eller testas om generellt utan konkret skäl. Ingen MNO-installation eller
journal har öppnats. Inga app-/Rust-testsuiter eller CI-jobb kördes för bedömningen.
**VoiceOver kördes inte i ursprungsbedömningen.** Senare användarverifiering och
slutacceptans redovisas nedan; inledningens metoduppgifter är historik.

## Slutacceptans 2026-11-02 — macOS v0.1

**Aktuell bedömning:** main `75c8a57a7bd43530f8cdd3159f3ea5b28823d152`.
AX-01–AX-04 är implementerade, mergade och manuellt accepterade av projektägaren
i macOS DEV. Samtliga tre PR:er har enligt projektägaren godkänd GitHub CI.
Inget bekräftat releaseblockerande tillgänglighetsfel kvarstår inom överenskommen
macOS v0.1-scope. #12 avslutas med nedanstående uttryckliga begränsningar och
separata UX-uppföljningar. Detta är avgränsad acceptans, inte WCAG-certifiering.

| ID | Åtgärd | Leverans och slutstatus |
| --- | --- | --- |
| AX-01 | Fokus i dialogen för osparade ändringar | [PR #37](https://github.com/antstan76/my-nature-observations/pull/37), implementerad och manuellt accepterad |
| AX-02 | Textkontrast i ljust utseende | [PR #38](https://github.com/antstan76/my-nature-observations/pull/38), implementerad och manuellt accepterad |
| AX-03 | Redigerarens lokaliserade tillgängliga namn | PR #37, implementerad och manuellt accepterad; identifieraren återanvänds inte för zoom |
| AX-04 | Native applikationszoom 100–200%, kommandon och persistens | [PR #39](https://github.com/antstan76/my-nature-observations/pull/39), implementerad och manuellt accepterad |

### Kompletterande manuell acceptans — projektägarens bekräftelse

Följande utfördes av användaren i macOS DEV; det är inte agentens egen appkörning:

1. Tangentbord: navigering från hemskärmen in i en journalanteckning och tillbaka fungerade.
2. VoiceOver: navigering genom journaltext med VoiceOver-kommandon fungerade.
3. Bild: VoiceOver identifierade en inlinebild och läste dess synliga bildtext efter att en bildtext lagts till.
4. Datum: VoiceOver annonserade ett fel för ogiltigt datum.
5. Dialog för osparade ändringar: VoiceOver läste knapparnas etiketter så att alternativen kunde identifieras.

Tidigare manuell acceptans av AX-01–AX-04 gäller fortsatt. Exakt macOS-version,
inställningar och källcommit för varje manuell session har inte lämnats i denna
slutbekräftelse och fylls inte i genom antaganden. Bedömningen av aktuell kod avser
main-commit ovan; den är inte ett påstående att alla sessioner använde den committen.

**Verifieringsgränser:** synlig bildtext är verifierad, men separat alternativtexts
kvalitet har inte oberoende verifierats. Inte heller hävdas att samtliga
applikationsvägar, alla foto-/kart-/väder-/fel-/startupscenarier eller varje
WCAG 2.2 AA-kriterium har testats uttömmande. Tidigare käll-/mocktester styrker
implementationen men ersätter inte faktisk VoiceOver-interaktion. Återstående
oprövade scenarier är dokumenterade begränsningar, inte konstaterade fel eller
nya releaseblockerare utan konkret hinder. Windows/Linux och mörkt tema förblir
separat arbete enligt D-019/D-017. Ingen ny testkörning, byggning eller CI gjordes
för denna dokumentationsavslutning.

### Separata, uppskjutna UX-uppföljningar

Registrerade i befintlig UX-tracker [#14, uppföljningskommentar](https://github.com/antstan76/my-nature-observations/issues/14#issuecomment-6576999459),
som ett uppföljningsunderlag utan att återöppna den accepterade leveransen:

- **Öppna journalanteckning via titel:** användaren föredrar att klick/tangentbords-
  aktivering av titeln öppnar läsläge, med separat Redigera-val. Källkontroll på
  aktuell main: `routes/journal/entries/+page.svelte` och sökresultat länkar till
  postens sida utan `edit=true`; sidan skickar `startInEditMode=false` och
  `JournalEditor.svelte` börjar skrivskyddat med separat Redigera-knapp.
  `CalendarDayPanel.svelte` länkar till läsjournalen. I `JournalReading.svelte` /
  `JournalContent.svelte` renderas journalens rubriker som vanligt innehåll, inte
  som postöppnande länkar; separat Redigera-länk finns. Uppföljningen ska först
  precisera vilken titel/ingång användaren avser och önskad konsekvens mellan
  ingångarna. Ingen lösning eller ändrad navigationsmodell är beslutad.
- **macOS menyöversättningar:** vissa native menyrubriker är engelska samtidigt
  som menyval är svenska. Uppskjuten lokaliserings-/UX-uppföljning, skild från
  dokumentets språk och de accepterade zoomkommandona; inget bekräftat
  tillgänglighetshinder har rapporterats.

En ytterligare journal-UX-observation har nämnts utan specificerat innehåll.
Inget spekulativt ärende eller påhittad beskrivning skapas för den.

## Uppföljning 2026-11-02 — AX-01 och AX-03

AX-01 och AX-03 är implementerade i den fokuserade editorrättningen. Den ursprungliga
bedömningen nedan bevaras som historisk källbas; dess kodrader avser föregående version.
AX-02 ingick inte i denna rättning men är nu levererad genom PR #38.
AX-01/03 är manuellt accepterade enligt slutacceptansen ovan.

- **AX-01:** den befintliga bekräftelsen använder native `dialog.showModal()` med
  bibehållet namn/beskrivning och `alertdialog`-semantik. Bakgrunden blir inaktiv.
  Fokus går till Fortsätt redigera; Tab/Shift+Tab cirkulerar mellan aktiva knappar.
  Escape/native cancel följer samma säkra avbrytväg och återställer tidigare fokus
  om kontrollen finns kvar. Escape upphäver inte ett redan bekräftat kassera-beslut
  som väntar på pågående skrivning. Befintliga spar-/navigations-/stängningsskydd
  och kassera-hanteraren är oförändrade. WeatherTile:s befintliga fokusmarkör bevaras.
- **AX-03:** TipTaps faktiska redigerbara textbox får `aria-label` via Paraglide:
  **Anteckningens text / Journal entry text**, samt `aria-multiline=true`.
  Befintlig språkbytestransaktion uppdaterar attributen utan dokument-/undoändring;
  ingen extra etikett läggs på omslutande element.
- **Verifierat:** 433 frontendtester passerar, inklusive 7 nya regressionstester;
  Paraglide-kompilering och Svelte/TypeScript ger 0 fel/varningar. Fem befintliga
  Rust-miljötester och Rust-formatkontroll passerar. Optimerat macOS DEV-paket
  byggt med separat DEV-identitet; paketerade licensresurser validerade.
- **Native WebKit:** en isolerad WKWebView-prob använder funktionerna extraherade
  från komponentens källkod, verklig TipTap och genererade språkmeddelanden.
  23 kontroller passerar: native modal/inaktiv bakgrund, fokuscykel/återgång,
  Escape/native cancel, borttagen tidigare kontroll, sv/en-attribut och oförändrat
  dokument/undo. Detta är inte ett fullständigt Svelte-/Tauri-end-to-end-test.
  Proben använder bara syntetiskt innehåll och ingen journalbackend.
- **Manuell slutstatus:** AX-01 och AX-03 är accepterade av projektägaren i macOS
  DEV, inklusive tidigare sv/en-VoiceOveracceptans. Den kompletterande kontrollen
  ovan bekräftar dialogknapparnas etiketter och VoiceOver-navigering i journaltext.
  Detta innebär inte att varje editor-/dialogscenario har testats uttömmande.

Inga databas-, schema-, backup-/Recovery-, färg- eller miljökontrakt ändras.
Ingen installerad MNO-app eller befintlig journal har öppnats under verifieringen.

## Uppföljning 2026-11-02 — AX-02 textkontrast

Implementerad på källbas main `5ae32114ca4f842da4272235c8df57d70779f83c`
(PR #37). **AX-02 är nu manuellt accepterad** enligt slutacceptansen ovan. De tidigare
AX-02-fynden nedan bevaras som historik. AX-01/03:s manuella VoiceOver-acceptans
i sv/en är enligt projektägaren klar efter PR #37; denna ändring gäller bara AX-02.

### Berörd text och färgval

Befintlig CSS har lokala ljusa ytor och semantiska `var(--accent, …)` /
`var(--text-muted, …)`-fallbacks, men inga gemensamma tokendefinitioner eller
aktiva temaöverstyrningar. Rättningen använder samma mönster: befintlig mörk
accent `#435b47` för aktiv grön text och `#686d63` som fallback för sekundär text.
Ingen temamotor, layout-, språk-, data- eller Rust-ändring görs.

- `AppNavigation`: Settings-grupprubriker och sektionsrubriker, backup-/export-/
  import-/restoreknappar, språkval (valt/övriga), kalenderns tillbaka-knapp och
  hemlänkens hover. Sökrutan får mörkare placeholder och dess omslutande
  `opacity: 0.7` tas bort så även inmatad text får full kontrast.
- Hem-/journalsidor samt läs-/ny-/postlist-/sök-/historiksidor och import-/
  restore-sidornas tillbaka-länkar: sektions-/kortetiketter, kortbeskrivning,
  datum, sökfråga/resultatdatum, informations-/tomstatus och sidfot.
- `CalendarDayPanel`, `CalendarGrid`, `DatePicker`: rubrik, länkar, information,
  datum och veckodagar samt månadstitelns hover och Idag-knapp. Vald dag och
  vald månad får mörk accentbakgrund bakom befintliga vita textetiketter.
- `JournalEditor`/`JournalReading`: laddstatus, datum och informationsmeddelanden.
- `EntryPlace`, `PlaceMap`, `PlaceMapPanel`, `AboutMno`: platsknapp/-metadata,
  sökresultat-/tom-/formulärhjälp, kartinstruktioner, användningslistans datum,
  versionsnummer och hjälptext. Väderformulärets etiketter/summary och
  väderhistorikens/-detaljernas hjälpinformation, typ, attribution, metadataetiketter och översiktens tillägg/märkning ingår.

Ikonknappar, dekorativa pilar/chevrons/prickar, fokuslinjer och appens wordmark
behåller sina färger. Inaktiva kontrollers befintliga opacitet behålls. Exempelvis
platsväljarens vita primärknapp på `#607763` är redan 4,865:1 och ändras inte;
väderknapparnas befintliga mörka accent behålls också. Dämpad text har alltså
bedömts efter funktion och faktisk bakgrund, inte ersatts med en enda färg.

### Reproducerade kontraster

Opaka deklarerade sRGB-färger, samma luminansformel som i ursprungsbedömningen.
Kvoter avrundas endast för presentation; tester jämför oavrundat med 4,5.

| Text / bakgrund | Före | Efter |
| --- | --- | --- |
| Gröna kontrolltexter `#788e7b` → `#435b47` / `#f8f7f3` | 3,290:1 | 6,932:1 |
| Samma text på knapp-hover `#eeeee8` | 3,028:1 | 6,380:1 |
| Språkval `#888b82` → `#686d63` / `#f8f7f3` | 3,231:1 | 4,952:1 |
| Samma text på knapp-hover `#eeeee8` | 2,974:1 | 4,557:1 |
| Kart-/About-hjälp `#777b73` → `#686d63` / `#f8f7f3` | 4,027:1 | 4,952:1 |
| Datum/veckodagar/status `#888a80` → `#686d63` / `#f8f7f3` | 3,268:1 | 4,952:1 |
| Sidfot `#999a91` → `#686d63` / `#f8f7f3` | 2,652:1 | 4,952:1 |
| Kortbeskrivning `#777970` → `#686d63` / `#fbfaf7` | 4,233:1 | 5,085:1 |
| Platsmetadata `#777d72` → `#686d63` / `#f8f7f3` | 3,949:1 | 4,952:1 |
| Platshjälp `#7a7d75` → `#686d63` / `#f8f7f3` | 3,904:1 | 4,952:1 |
| Platsknapp `#687b6b` → `#435b47` / `#f8f7f3` | 4,227:1 | 6,932:1 |
| Platsknapp på hover/fokus `#eef1eb` | 3,974:1 | 6,517:1 |
| Väderhjälp `#707568` → `#686d63` / `#faf9f5` | 4,496:1 | 5,039:1 |
| Vald dag/månad: vitt / `#788e7b` → `#435b47` | 3,527:1 | 7,431:1 |
| Sök-placeholder på `#f8f7f3`, inklusive tidigare 0,7 wrapper-opacitet | 2,155:1 | 4,952:1 |

### Historisk testverifiering och aktuell slutstatus

- 73 fokuserade kontrasttester kontrollerar faktiska CSS-regler/fallbacks för
  texten på relevanta normal-/hover-/fokusytor, vita kalenderetiketter och
  sökrutans opacitet samt reproducerar ursprungliga mätningar. De ingår i `npm test`.
- 506 frontendtester passerar. `npm run i18n:compile`, `npm run check`
  (0 fel/varningar) och `npm run build` passerar. Befintliga låsta beroenden
  återställdes med `npm ci --offline`; inga låsfilsändringar. Inga Rustbyggen.
- Detta är käll-/CSS-baserad verifiering, inte pixelmätning i levererad WKWebView,
  automatisk helappscertifiering eller en ny VoiceOver-körning. Ingen app eller
  personlig journal har öppnats. Framtida temaöverstyrningar ingår inte.
- **Historiskt manuellt granskningsunderlag (AX-02 nu accepterad):** granska hem/kort/sidfot, Settings
  (språk, backup och About), sök-placeholder/inmatning/resultat, kalender och
  datumväljare (vald dag/månad, hover och tangentbordsfokus), läs-/ny-/redigerings-
  status samt plats-/kart-/väderinstruktioner. Kontrollera normal-, hover- och
  fokuslägen i aktuell macOS DEV-WKWebView; mät renderade text-/bakgrundsfärger
  vid avvikelse. Kontrollera att kontrast, lugn visuell hierarki och läsbarhet vid
  200% förstoring består och att val/fel går att förstå utan enbart färg.
  Anteckna commit, DEV-version och macOS. Ingen Production-journal behövs.

## Uppföljning 2026-11-02 — AX-04 applikationszoom

**AX-04** gäller applikationszoom; AX-03 behåller betydelsen redigerarens
tillgängliga namn. Källbas: main `7acdef2` efter PR #38. Implementationen är klar
genom PR #39 och **manuellt accepterad** enligt slutacceptansen ovan.

### Implementation

- En controller (`application-zoom.ts`) äger nivåerna **100, 125, 150, 175 och
  200%** och anropar befintliga Tauri 2.12.3 `getCurrentWebview().setZoom()`.
  Begäranden köas, inklusive initial återställning. Nästa nivå beräknas efter
  föregående avslutade API-anrop; upprepade keydown-events ger inga extra steg.
  Gränserna kläms och återställning sätter 100%. Misslyckade API-anrop ändrar inte
  sparad nivå och köas inte om automatiskt; nästa kommando kan försöka igen.
- `ApplicationZoom.svelte` monteras en gång utanför rotlayoutens ready-/Recovery-
  villkor, återställer före efterföljande zoomkommandon och behåller samma
  controller över vanlig SvelteKit-navigering. Fel visas som lokaliserad status
  utan fokusflytt. Ingen zoomindikator eller ny Settings-sektion läggs till.
- **macOS:** Cmd+Plus/Equals, Cmd+Minus och Cmd+0. På övriga desktopplattformar
  används Ctrl. Endast dessa kombinationer fångas i capture-fasen; Alt-, IME-
  och andra redigeringskommandon lämnas ifred. Native View-menyn får Zoom In /
  Zoom Out / Actual Size, med svenska Paraglide-etiketter vid sv och uppdatering
  efter språkbyte. Befintliga menyposter och guarded Quit bevaras.
- Native menyval skickar en avgränsad händelse till huvudfönstrets controller.
  **Menyvalen har inga egna acceleratorer**; tangentbordshanteraren äger
  kombinationerna så ett tangenttryck inte kan ge både native och DOM-zoom.
  Kortkommandosymboler visas därför inte bredvid de nya menyvalen. Menyvalen
  aktiveras först när frontendens händelselyssnare och lokaliserade etiketter finns.
  Tauris fristående `zoomHotkeysEnabled` är uttryckligen false.
- `mno.application-zoom.v1` i localStorage återanvänder språkpreferensens
  lagringsmekanism, men inte dess nyckel. Endast exakta stödda värden accepteras.
  Saknat/ogiltigt värde blir 100%; lyckad applicering sparar nivån. Vid blockerad
  localStorage fungerar sessionszoom utan sparning. Ingen SQLite-migrering,
  journalinställning, settings-plugin eller nytt beroende införs.
- Endast `core:webview:allow-set-webview-zoom` läggs till i befintlig capability
  för fönstret `main`. Native zoom skalar hela webview-innehållet, inklusive
  editor, HTML-dialoger, paneler, formulär och kartor; OS-menyer, titelrad och
  native filväljare påverkas inte. Ingen CSS-skalning/layoutomdesign införs.

### Verifiering och begränsningar

- **16 fokuserade zoomtester** passerar: nivåer/gränser/reset, återställning,
  ogiltiga värden, saknad lagring, API-fel och återhämtning, serialisering,
  macOS-/Ctrl-kommandon, upprepning/dubbelhantering, faktisk komponentscript-
  integration med mockade Tauri-anrop, språkbyte och avregistrering vid unmount.
  Därutöver passerar 7 befintliga editor-accessibilitytester och 3 miljötester.
- Paraglide-kompilering och Svelte/TypeScript passerar (0 fel/varningar).
  Frontend-produktionsbygge passerar. `cargo check --lib --locked --offline`
  verifierar de ändrade macOS-menyanropen mot låsta beroenden, med befintlig
  target-cache utanför iCloud. Rust-format och whitespace kontrollerade.
  Ingen full testsuite, paketerad DEV-app eller appstart har körts.
- **Manuell macOS DEV-slutstatus:** projektägaren har accepterat PR #39.
  Implementationens käll-/mocktester är separat evidens och utgör inte en
  generell WCAG-certifiering.
- Windows/Linux återanvänder controller och Ctrl-kombinationer, men native
  zoommenyer byggs bara på macOS i denna leverans. Verklig WebView2/WebKitGTK-,
  OS-skalnings- och tangentbordslayoutacceptans återstår inom plattformsarbetet.
- Återställningen sker vid frontendmount; ett kort initialt 100%-läge kan synas.
  localStorage är en lokal app-/webviewpreferens, inte del av journalens backup
  eller import. Native menyöppning kan fånga tangenttryck medan menyn är öppen;
  de nya kommandona kan då väljas direkt i menyn.

### Historiskt regressionsunderlag för AX-04 (inte en kvarvarande releasegrind)

AX-04 är manuellt accepterad. Listan bevarar föreslagna regressioner; varje enskilt
scenario görs inte därmed till ett oberoende verifierat resultat.

- Anteckna commit, appversion, macOS och tangentbordslayout. Kontrollera
  Plus respektive Equals på svensk och engelsk layout, Minus och 0.
- Stega 100→125→150→175→200 och tillbaka, även via blandade meny-/tangentval.
  Ett tryck ger ett steg; upprepning och gränser ger inga dubbla/extra steg.
- Stäng/starta appen på 175/200%; nivån återkommer. Navigera mellan sidor.
  Kontrollera även ogiltigt/saknat lagrat värde och otillgänglig lagring i isolerad
  testmiljö; 100% respektive sessionszoom ska vara säkra reservbeteenden.
- Zooma medan journalfält, toolbar, formulär och lämna-dialog har fokus.
  Caret, urval, undo, autosave, Tab/Shift+Tab, Escape, fokusåtergång och guarded
  Quit ska behålla funktion. Byt språk och kontrollera de tre native menytexterna.
- Vid 200%, både normalfönster och minsta stödda 900×650: nå all väsentlig
  text och alla knappar via rullning/tangentbord. Kontrollera navigation/Settings,
  dialogernas nederkant, väderoverlay och långa svenska texter, läs-/redigeringsläge,
  synliga fokusmarkörer och att innehåll inte klipps bort.
- Platsväljare/kartpanel: kontrollera kartstorlek, tiles, markörträffar,
  kontextmenyposition, list-/koordinatalternativ och väderoverlayposition efter zoom.
  Applikationszoom ska inte ändra kartans geografiska zoomnivå.
- Dokumentera reproducerade hinder separat: förlorad åtkomst till väsentliga
  kontroller blockerar acceptans; kosmetiska förbättringar är uppföljning. Ingen
  bred layoutomdesign ingår utan visat behov. #12 avslutas enligt den avgränsade slutacceptansen ovan.

## Sammanfattning och releasegrind (ursprunglig bedömning)

Tre konkreta brister bör rättas före offentlig macOS v0.1.0: **AX-01 modalfokus,
AX-02 textkontrast och AX-03 redigerarens tillgängliga namn**. AX-01/03 kan vara en
liten redigerar-PR och AX-02 en separat avgränsad färgrättning. Därefter verifieras
kärnflöden med tangentbord och VoiceOver; övriga risker blir blockerare först om ett
verkligt hinder bekräftas. B6:s redan befintliga release-/distributionskrav kvarstår.
Windows/Linux är senare #11-arbete och blockerar inte första macOS-publiceringen.

## 1. Ursprungligen bekräftade brister (nu åtgärdade)

### AX-01 — högst prioritet: fokus i dialogen för osparade ändringar

**Evidens:** `JournalEditor.svelte:139–146, 839–881` sätter `showLeaveConfirmation`
och renderar en `div role="alertdialog" aria-modal="true"`. Dialogen får inget
initialfokus, bakgrunden görs inte inert, Tab begränsas inte och avbryt återställer
inte uttryckligen fokus. Namn/beskrivning finns redan och ska bevaras.

En isolerad native WKWebView-reproduktion av samma DOM-mekanik (syntetiskt textfält,
infogad aria-modal-div; ingen journalbackend) gav:

```json
{"focusOutsideAfterInsertion":true,"backgroundStillFocusable":true,"dialogAriaModal":"true"}
```

Detta verifierar att ARIA-attributet inte implementerar modalbeteendet. Det är inte
en fullständig app-/VoiceOver-reproduktion. Kodens avsaknad av fokusåtgärder och
webbläsarens beteende visar ändå ett konkret hinder i den säkerhetskritiska
lämna-/stäng-bekräftelsen: fokus kan ligga bakom dialogen medan den uppges vara modal.

**Minsta åtgärd:** välj ett befintligt enkelt mönster, exempelvis native `dialog`
som fotoredigeringen eller lokal initialfokus/avgränsning/inert-bakgrund. Fokusera
säkert val (fortsätt redigera), låt Tab/Shift+Tab stanna i modal dialog, låt Escape
avbryta och återställ relevant fokus. Behåll save-/leave-/close-guardarnas beslut,
spärrar, revisioner och data oförändrade. Bygg inget generellt dialogramverk.

**Relevans:** W3C modal dialog pattern, WCAG 2.4.3/2.4.11 och 4.1.2.

### AX-02 — bekräftad för låg kontrast för liten, aktiv text

**Evidens:** `AppNavigation.svelte:734–785` använder 13px `#788e7b` för aktiva
sektionsrubriker och `#888b82` för språkknappar, på panelens `#f8f7f3`.
Backup-knappar använder samma dämpade gröna text. sRGB-relativ luminans ger:

| Förgrund / bakgrund | Kontrast | Bedömning för normal text |
| --- | --- | --- |
| `#788e7b` / `#f8f7f3` | 3,290:1 | Under 4,5:1 |
| `#888b82` / `#f8f7f3` | 3,231:1 | Under 4,5:1 |
| `#777b73` / `#f8f7f3` | 4,027:1 | Även viss 12px hjälptext ligger under 4,5:1 |
| `#343530` / `#f8f7f3` | 11,537:1 | Befintlig brödtext klarar tröskeln |
| `#435b47` / `#f8f7f3` | 6,932:1 | Befintlig mörk accent är ett möjligt textval |

Mätningen använder deklarerade, opaka färger i normalstatus; den är inte en
heltäckande pixel-/hover-/opacitetsgranskning. Kontrollera övrig relevant småtext
(datum, status, instruktioner) i samma avgränsade åtgärd. 13px semibold är inte stor
text enligt kontrastkriteriet. Detta är en läsbarhetsbrist, inte kosmetisk polish.

**Minsta åtgärd:** mörkare befintlig färg på berörd text, sedan kontrollerade
normal-/hover-/fokuslägen. Ändra inte alla dekorativa ikoner eller godkänd logotyp
blint: text och icke-text har olika krav, och inaktiva kontroller har undantag.
Ingen temamotor eller generell tokenmigrering behövs. Detta nya mätunderlag är ett
konkret skäl för riktad rättning, inte för att återöppna hela #14/PR #35.

**Relevans:** WCAG 1.4.3 (normal text 4,5:1; stor text 3:1), 1.4.11 där tillämpligt.

### AX-03 — journalens textfält saknar författarangivet tillgängligt namn

**Evidens:** `JournalEditor.svelte:244–260, 833–835` skapar TipTap utan namngivande
`editorProps.attributes`; den tomma monterings-diven är inte en label. Den låsta
TipTap-versionens `getEditorViewAttributes()` kontrollerades direkt och gav
`{ role: 'textbox' }`, utan aria-label/aria-labelledby. Rollen finns alltså redan;
rapporten föreslår inte att bygga ett nytt redigerar- eller dokumentkontrakt.

**Minsta åtgärd:** lokalisera och koppla ett namn till själva redigerbara roten,
bevara textbox-rollen och uttryck fler-radig redigering korrekt där det behövs.
Uppdatera namnet vid sv/en-byte. Kontrollera sedan VoiceOver i tom och fylld text,
listor, rubriker och efter byte mellan läs-/redigeringsläge. Ingen innehållsmigrering.
Det är en bekräftad namngivningsbrist, **inte** belägg för att VoiceOver redan testats
eller att det är omöjligt att skriva med VoiceOver.

**Relevans:** WAI-ARIA textbox kräver ett tillgängligt namn; WCAG 4.1.2/3.3.2.

## 2. Ursprungliga risker och regressionsscenarier

Tabellen bevarar ursprungsbedömningens scenarier, inte kvarvarande konstaterade
fel. Slutacceptansen ovan anger vad som faktiskt har verifierats. Oprövade
scenarier ska inte läsas som vare sig godkända tester eller nya releaseblockerare.

| Område | Befintligt stöd / evidens | Ursprungligen föreslagen kontroll; eskalera bara vid faktiskt hinder |
| --- | --- | --- |
| Rich text | Knappar har namn/aria-pressed; ProseMirror hanterar text/undo; autosave/fel har status/alert. `.tiptap` har `outline:none` vid rad 909 men textcaret kan vara ett giltigt fokusbevis. | VoiceOver: hitta/öppna fält, skriva/rätta/läsa, lista/rubrik, undo/redo, Klar och återöppning. Verifiera synlig caret/fokus; förklara inte outline:none ensamt som bevis för WCAG-fel. |
| Navigationspaneler | Namngivna nav/asides, native länkar/knappar, Escape finns i AppNavigation. | Kontrollera fokus efter öppna/stäng/Escape och vid borttaget fokuserat element. Gör inte dockade/icke-modala asides till modaler generellt. |
| Karta/platsväljare | PlaceMapPanel får fokus vid öppning; lista, namngivna fält och manuella koordinater finns. Blockerad radering har egen fokusavgränsning och inert innehåll. Markörer har tangentbordsstöd och lista är ett alternativ till kartan. | Tabba igenom den fullhöga kartpanelen: hamnar fokus dolt i underliggande journal? Kontrollera återgång vid stängning och radera-/avbryt-bekräftelser. Välj/skapa plats utan dragning eller precisionspekning. Ingen ny karta eller automatisk geografisk beskrivning krävs. |
| Formulärfel | Datum aria-invalid + alert; väderfält har label, aria-invalid och alert; platsfel har gemensam alert. | VoiceOver ska förstå vilket fält som är fel och behålla inmatning. Kontrollera vid behov fältkoppling/aria-describedby och fokus till fel; frånvaro av varje sådant attribut är inte ensamt bevis för blockerad användning. |
| Väder | WeatherTile är uttryckligen icke-modal (`aria-modal=false`), har panel-/triggerfokus, Escape och lokala bekräftelser. | Stängning, växling av steg, jämförelse och fel ska inte tappa fokus eller dölja relevant status. Inför inte en generell fokusfälla i dessa icke-modala paneler. |
| Kalender/sökning/semantik | Dagknappar har datumetiketter; sökning har namn och faktiska anteckningslänkar; SvelteKit ger ramverksstöd för routning. | Hitta datum/anteckning med tangentbord och VoiceOver; kontrollera rubriker, ordning, sidbyte och återgång. Allmänna rubrikförbättringar är inte automatiskt blockerare. |
| Foton | Bildknapp/Shift+F10, menyval med checked-state och native redigeringsdialog finns; alt/bildtext är separata, bildtext semantisk. | Alt-text, bildtext, meny, avbryt och fokusåtergång med VoiceOver; ingen permanent tangentbordsfälla. Återöppna inte Stage 2B-layout eller inför obligatoriskt alt-/schemaarbete utan nytt belägg. |
| Fel/portabilitet/About | Restore/import har status/alert; Recovery är huvudinnehåll med namngivna val; B5:s tvåspråkiga native fel och PR #35 är accepterade. | Lyssna att kritiska fel/val förstås och att osparade data inte förloras. Begränsad tillgänglighetskontroll, inte ny funktionell Restore-/Recovery-utredning. |
| Förstoring och fokus som täcks | Responsiv CSS finns; kartor är tvådimensionellt innehåll, andra formulär behöver läsbarhet. | Kontrollera textförstoring/zoom till 200%, tillgängliga knappar och rullning i paketet. Zoomstöd är inte nyverifierat här. Notera macOS tangentbordsinställning och VoiceOver-läge så OS-standard inte feltolkas som appfel. |

Inga påståenden om faktisk VoiceOver-uppläsning härleds från ARIA- eller DOM-kontroller;
användarens bekräftade uppläsning redovisas separat i slutacceptansen.
Egna symboler har i stor utsträckning text/namn eller är dekorativa; namn på knappar,
valt format, sparstatus och fel uttrycks inte enbart med färg. Hela appen har inte
inventerats för varje ikon-/nyans-/rörelsefall.

## 3. Icke-blockerande / utanför första releasen

- Ren justering av rubrikrytm, avstånd, dekoration och identiska fokusstilar när
  befintligt fokus är tydligt: kosmetik, inte skäl för omdesign.
- Temasystem/mörkt läge (#8), Restore/Import-hemnavigation (#28), nya kartfunktioner,
  gallerier och ny generell editor-/dialogarkitektur: utanför första macOS-scope.
- Windows/Linux och deras AT-/native-/ikon-/licens-/paketkontroller: senare i #11
  enligt D-019, men obligatoriska före respektive plattform publiceras.

## 4. Avslutad, avgränsad macOS v0.1-acceptans

AX-01, AX-02, AX-03 och AX-04 är implementerade och manuellt accepterade.
Projektägarens kompletterande kontroller och uttryckliga begränsningar står i
slutacceptansen ovan. Det finns inget bekräftat kvarvarande releaseblockerande
tillgänglighetsfel inom överenskommen scope; #12 avslutas. Det är inte ett
påstående att alla möjliga flöden eller WCAG-kriterier är uttömmande verifierade.

De separata UX-uppföljningarna ovan blockerar inte denna acceptans. Framtida
konkreta hinder bedöms efter sin faktiska påverkan. Ursprungliga rekommendationer
om implementation och väntande acceptans har ersatts av denna slutstatus;
historiska mätningar och testresultat behålls för spårbarhet.

## Normativt underlag och reproduktion

[W3C modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/),
[WCAG 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum),
[WAI-ARIA textbox](https://www.w3.org/TR/wai-aria-1.2/#textbox),
[WCAG fokus](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible).

Kontrast: för varje sRGB-kanal c=byte/255, linjärisera c/12,92 om c≤0,04045, annars
((c+0,055)/1,055)^2,4. L=0,2126R+0,7152G+0,0722B. Kvot=(Lljus+0,05)/(Lmörk+0,05).
Native-proben infogade en vanlig aria-modal-div med samma fokusmekanik efter ett
fokuserat syntetiskt textfält; den testade inte Svelte-komponenten eller dess
backend end-to-end. TipTap-attributen kontrollerades i den installerade låsta
biblioteksversionen. Ursprungsbedömningens övriga slutsatser är kodinspektion och
då dokumenterade resultat. Senare användarverifiering redovisas i slutacceptansen;
ingen ny full test-/VoiceOver-körning gjordes vid dokumentationsavslutningen.
