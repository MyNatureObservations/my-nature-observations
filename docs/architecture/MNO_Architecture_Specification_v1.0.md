# My Nature Observations — Architecture Specification v1.0

**Status:** Beslutad arkitekturinriktning, före implementation\
**Datum:** 12 oktober 2026\
**Omfattning:** Tekniska beslut T1–T43\
**Projekt:** My Nature Observations (MNO) — nytt projekt, skilt från den äldre appen *Mina naturobservationer*

> **Your nature. Your observations. Your way.**\
> **The interface whispers. Your story speaks.**

> **Senare scopebeslut 2026-10-31:** enligt [D-017](../design/MNO_Design_Decisions.md#d-017--temasystem-efter-v01) accepteras nuvarande ljusa utseende för v0.1. Dokumentets ursprungliga formuleringar om ljust/mörkt systemtema bevaras som historik men är inte längre v0.1-krav. Temasystem med systemval, manuellt val och möjliga naturteman skjuts till efter v0.1; namn och UX är preliminära. Tillgänglighet och kontrast i levererat ljust utseende gäller fortfarande.

> **Senare plattformsbeslut 2026-11-01:** [D-019](../design/MNO_Design_Decisions.md#d-019--macos-först-i-v01-serien) ersätter samtidig treplattformsleverans: första offentliga v0.1.0 kräver bara macOS. Windows/Linux följer efter macOS-publiceringen inom v0.1-serien och verifieras separat i #11, inklusive native-licenser, ikoner och paketering. Samma versionsnummer får bara återanvändas för samma oförändrade release-källversion; kodändringar kräver lämplig patchrelease. Ursprungliga formuleringar nedan behålls som historik. Tillgänglighet #12 gäller fortsatt före macOS-publiceringen.

## 1. Syfte och dokumentets status

Detta dokument samlar arkitekturinriktningen för MNO inför utvecklingen av MVP v0.1. Det är ett **beslutsunderlag och en teknisk kompass**, inte ett påstående om att funktionerna redan är implementerade. Ingen kod har ännu skrivits för det nya MNO-projektet.

Arkitekturen är **levande**. Grundprinciperna är långsiktiga, men bibliotek, ramverk och implementationer får omprövas när prototyper, tester eller teknikutveckling ger goda skäl. Större förändringar dokumenteras i nya arkitekturbeslut (ADR).

### 1.1 Produktens identitet

MNO är en personlig, stillsam naturdagbok för macOS, Windows och Linux. Användaren ska äga sina uppgifter, kunna arbeta utan internet och aldrig behöva ett konto eller en molntjänst. Upplevelsen ska vara textnära, varm och diskret: ett digitalt naturhäfte, inte ett tungt administrationssystem.

Designriktning: Japandi-inspirerad, luftiga ytor, naturliga färger, varsam typografi och tydlig användbarhet. Föreslagen baspalett: bakgrund `#F8F7F3`, text `#343530`, dämpad grön `#788E7B`, avdelare `#D9D8D1`. Fotografier ska kunna visas i sina naturliga proportioner.

### 1.2 Grundprinciper

1. **Lokal först:** grundfunktionerna fungerar offline.
2. **Användaren äger sin information:** öppna, dokumenterade exporter och ingen inlåsning.
3. **Integritet som standard:** ingen telemetri, inget obligatoriskt konto, inga oönskade nätverksanrop eller disksökningar.
4. **Enkelhet med framförhållning:** förebygg kända arkitekturproblem men undvik spekulativ överkonstruktion.
5. **MNO ska kännas hemma på varje plattform:** etablerade interaktionsmönster och tangentbordsgenvägar.
6. **Tekniken ska stödja berättelsen:** journalen är produktens centrum.
7. **Bevara informationens betydelse, inte gamla implementationer för alltid.**

## 2. Omfattning och prioriteringar

### 2.1 MVP v0.1 — första användbara naturdagboken

- Textbaserad journal med inline-redigering och grundläggande formatering.
- Autosparning, normal ångra-/gör om-historik och tydligt avslut av redigering.
- Redigerbart upplevelsedatum; separat teknisk information om skapande och ändring.
- Kronologisk tidslinje och kalender-/månad-/årsnavigering.
- Flera journalanteckningar per dag och frivilliga rubriker.
- Enkel sökning och grundläggande historiska sammanställningar där relevant data finns.
- Manuellt väder samt frivillig väderhämtning när en konkret källa och användarflöde valts.
- Ljust/mörkt tema med hänsyn till operativsystemet.
- Svenska och engelska.
- Lokal säkerhetskopiering, återställning och öppna exporter.
- Grundläggande tillgänglighet, felhantering och automatiserade tester.

**Viktig gräns:** Strukturerade artobservationer, fullständiga art-/platsmoduler och fotobibliotek ingår **inte** i första MVP:n, även om flera framtida krav redan har diskuterats. Vi skapar inte tomma moduler bara för att de är planerade.

#### Plattformskrav för MVP v0.1

MNO v0.1 ska vara en fungerande och installerbar
skrivbordsapplikation för:

- macOS
- Windows
- Linux

Samtliga tre operativsystem är obligatoriska för MVP v0.1.

Det finns ingen fastställd tidsgräns för när v0.1 ska
vara färdig. Funktionalitet, installation och grundläggande
datalagring ska verifieras på alla tre plattformarna.

Windows och Linux kan testas i virtuella miljöer,
bland annat via Proxmox.

Exakt paketeringsstrategi, kodsignering och
distributionskanaler beslutas separat.

### 2.2 Senare utveckling

- Observationstillfällen och artobservationer, bland annat import från eBird.
- Arter, platser, personliga artkapitel och statistik.
- Fotobibliotek med länkade original, miniatyrer, förhandsvisningar och återkoppling av saknade filer.
- Fotografier inbäddade mellan journalstycken och kopplade till observationer.
- Eventuellt flera naturböcker, en aktiv åt gången.
- Fler språk, avancerade sökfunktioner och vissa anpassningsmöjligheter.

## 3. Övergripande systemarkitektur

```text
┌─────────────────────────────────────────────────────┐
│                    MNO Desktop                      │
│  TypeScript UI (ramverk avgörs efter prototyper)    │
│  Journalredigerare, navigering, teman, tillgänglighet│
└───────────────────────┬─────────────────────────────┘
                        │ Tauri-kommandon / händelser
┌───────────────────────▼─────────────────────────────┐
│                  Rust-applikationskärna              │
│  Validering · journal · sökning · import/export      │
│  backup · filåtkomst · framtida domänmoduler         │
└───────────────┬───────────────────┬─────────────────┘
                │                   │
       ┌────────▼────────┐   ┌──────▼─────────────────┐
       │ SQLite / FTS5   │   │ Bokmappens mediafiler  │
       │ Primärdata      │   │ Framtida förhandsbilder│
       │ Relationer      │   │ och miniatyrer         │
       └─────────────────┘   └────────────────────────┘
```

- **Tauri 2** används för skrivbordsappen.
- **TypeScript** ansvarar för direkt användarinteraktion och redigeringssessionens tillstånd.
- **Rust** äger domänlogik, validering, lagring, sökning, import/export och säker filåtkomst.
- **SQLite** är gemensam lokal databas för en naturbok; **FTS5** används för fulltextsökning.
- Arkitekturen är en **modulär monolit**, inte ett distribuerat system eller ett pluginramverk.
- Inget separat lokalt webbserverlager behövs för den nya appen.

### 3.1 Modulgränser

**Nu:** journal, lagring, sökning, export/backup, appinställningar och plattformsintegration.\
**Senare:** observationer, arter/platser, fotografier och externa datakällor.

Varje modul har tydligt ansvar, men delar kärna och databas. Förväntade fel hanteras lokalt; problem som hotar dataintegriteten får stoppa berörda skrivoperationer.

### 3.2 Textredigerare — öppet teknikval

Vi ska prototypa och jämföra:

- **Svelte 5 + Tiptap**
- **React + Lexical**

Prototyperna ska innehålla samma realistiska journalinnehåll och bedömas utifrån redigeringskänsla, stabilitet, formatering, autosparning, framtida text-/bildflöde, tillgänglighet och visuellt uttryck. **Inget av alternativen är valt ännu.**

Journalinnehåll lagras i en **versionshanterad, redigeraroberoende strukturerad JSON-modell** i SQLite. Redigerarens interna format får inte bli ett permanent lås.

## 4. Data, lagring och livscykel

### 4.1 En naturbok som flyttbar katalog

Standardplaceringen föreslås vara en mapp `My Nature Observations` under operativsystemets dokumentkatalog; användaren kan välja annan plats. Konceptuell struktur:

```text
My Nature Observations/
  manifest.json
  nature.sqlite
  media/                 # skapas när behov uppstår
    previews/
    thumbnails/
```

Sökvägar inne i naturboken ska så långt möjligt vara relativa. Maskinspecifika inställningar, loggar och andra appfiler lagras separat. Vi ska inte förutsätta att användaren för alltid har bara en naturbok, men v0.1 arbetar med en bok.

### 4.2 Primärdata och härledd data

**Primärdata:** journalinnehåll, användarens egna val och korrigeringar, upplevelsedatum, relationer och framtida observationer.\
**Härledd data:** sökindex, statistik, årslistor, cache och andra beräknade sammanställningar.

Härledd data ska kunna byggas om från primärdata. Mellanlagring för prestanda är tillåten men får inte vara enda källan till en uppgift.

### 4.3 Datum och tidszoner

Håll isär upplevelsedatum, frivillig upplevelsetid, känd tidszon samt tekniska tidsstämplar för skapande och ändring. Ett datum utan tid är giltigt; MNO får inte hitta på ett klockslag eller flytta en upplevelse till annan kalenderdag när användaren reser.

### 4.4 Databasmigreringar

- Versionshanterat schema från första versionen.
- Transaktionssäkra migreringar där det är möjligt.
- Kontroll av schema-/appkompatibilitet; en äldre app får inte skriva i en inkompatibel nyare databas.
- Verifierad återställningskopia före destruktiva eller svåråterkalleliga migreringar.
- Tre nivåer: automatisk, användarassisterad, obligatorisk användarassisterad migrering.
- Användaren kan behöva tolka tvetydig äldre information innan redigering får fortsätta.
- Inga automatiska gissningar som ändrar informationens betydelse.
- Gamla tekniska fält behöver inte bevaras för alltid när innehållet migrerats säkert.

### 4.5 SQLite och oväntade avslut

Använd SQLite:s transaktioner och lämplig WAL-konfiguration. Normal återhämtning sker genom SQLite; ytterligare integritetskontroll körs vid misstänkta problem och kan även startas manuellt. Ingen riskfylld automatisk reparation. En naturbok har **en aktiv skrivande appinstans**; försök att öppna samma bok igen ska i första hand fokusera befintlig instans.

### 4.6 Autosparning och ångra

Spara pragmatiskt efter ungefär 1–2 sekunders paus samt vid relevanta navigerings-/avslutshändelser. Textredigering ska inte behöva en Rust-rundresa för varje tangenttryckning. Autosparning får **inte** tömma normal, sessionsbaserad ångra-/gör om-historik. Permanent revisionshistorik är inte ett MVP-krav. Målet är rimligt dataskydd, inte ett tungt händelseloggssystem.

## 5. Backup, export och dataportabilitet

### 5.1 Säkerhetskopior

- Manuell backup finns.
- Automatisk lokal backup är **avstängd som standard** och kräver opt-in, vald plats och intervall.
- SQLite-backup görs med säker mekanism för öppen databas, inte en okontrollerad filkopiering.
- Externt länkade originalfiler kopieras inte automatiskt.
- MNO raderar inte användarens säkerhetskopior utanför apphanterat utrymme.
- Återställning ska vara säker och testad.

### 5.2 Öppna exporter

En fullständig export är **inte samma sak** som en backup. Exporten ska vara dokumenterad, versionshanterad och innehålla tillräckligt med struktur för att bevara information, metadata, ursprung och relationer. Markdown ger en människoläsbar representation; en fullständig maskinläsbar representation möjliggör återimport och vidare användning utanför MNO.

Exporten ska ange vilka eventuella externa originalfiler som inte ingår. Äldre format hanteras genom dokumenterade och testade konverteringssteg.

### 5.3 Verifiering

Automatiska rundturstester: skapa bok → exportera → importera till tom bok → jämför semantiskt innehåll och relationer. Testa också äldre format, Unicode, saknade frivilliga fält och felaktiga filer. Backup/återställning testas separat. Testerna växer när nya datatyper faktiskt införs.

## 6. Integritet, externa integrationer och media

### 6.1 Nätverksåtkomst

MNO fungerar offline. Inga automatiska nätverksanrop, uppdateringskontroller eller telemetri utan uttryckligt val. Manuell uppdateringskontroll är möjlig; automatisk kontroll är avstängd från början och installation kräver alltid ett aktivt beslut.

Framtida importer och tjänster använder tydliga integrationsgränser. Externa format konverteras till MNO:s interna modell; proveniens och externa identifierare bevaras. Import förhandsgranskas, valideras och tillämpas atomärt när operationen kräver det.

### 6.2 Fotografier — framtida funktion

- Länkade original ska kunna användas utan dubbel lagring.
- MNO får aldrig ändra eller radera original utanför MNO-hanterad lagring.
- Förhandsvisningar och miniatyrer skapas när bilder läggs till, i bakgrunden där lämpligt.
- En saknad filreferens bevaras; eventuell förhandsvisning kan fortfarande visas.
- Manuell återkoppling först; återkoppling av hela flyttade mappar senare.
- Ingen automatisk genomsökning av diskar.
- Avbruten förhandsbildsbearbetning kan behålla färdiga resultat och fortsätta senare.

Ytterligare framtida UX-beslut om bildvisning, metadata och galleri dokumenteras i den separata designspecifikationen och implementeras först när fotomodulen prioriteras.

## 7. Användarupplevelse, språk och tillgänglighet

- MNO ska följa plattformarnas vanliga beteenden och tangentbordsgenvägar.
- Tillgänglighet byggs in från början, med WCAG 2.2 AA som relevant riktmärke: tangentbord, fokus, kontrast, skärmläsare, skalbar text och reducerad rörelse.
- Färdiga, välunderhållna tekniska komponenter används för svåra problem; det visuella uttrycket byggs med egna designkomponenter och CSS.
- Svenska och engelska är likvärdiga språk från MVP. Gränssnittstexter hanteras genom i18n-system; språk och region skiljs åt. Journaltexter översätts aldrig automatiskt.
- Felmeddelanden ska vara begripliga; tekniska detaljer finns i lokal felsökning.

## 8. Fel, loggar och bakgrundsarbete

### 8.1 Felhantering och loggning

Lokal strukturerad loggning utan telemetri. Undvik journaltexter, privata detaljer och fullständiga filsökvägar i vanliga loggar. Rotera och begränsa loggar inom apphanterad katalog. Användaren ska kunna öppna logg eller granska en frivillig felsökningsrapport innan delning. Utvecklingsversionen kan logga mer tekniskt, med samma respekt för personliga uppgifter.

Fel isoleras där möjligt: ett misslyckat bildjobb ska inte stoppa journalen. Databasproblem som kan hota integriteten får däremot stoppa skrivning.

### 8.2 Längre operationer och samtidighet

Tyngre operationer körs utan att blockera gränssnittet. Visa diskret men tydlig status. Tillåt bara samtidiga åtgärder som är säkra; förklara varför en funktion tillfälligt är låst. En åtgärd är inte klar förrän den faktiskt har slutförts.

Avbrott är operationsspecifika: importer/återställningar kan behöva allt-eller-inget; oberoende förhandsbildsjobb kan behålla färdiga resultat. Bygg inte ett avancerat generellt jobbsystem i förtid.

## 9. Utveckling, testning och publicering

### 9.1 Projektstruktur

Ett Git-repository, exempelvis:

```text
my-nature-observations/
  src/              # TypeScript-gränssnitt
  src-tauri/        # Rust/Tauri
  tests/            # gemensamma integrationstester där lämpligt
  docs/             # specifikationer, arkitekturöversikt och ADR
  scripts/          # utvecklingshjälpmedel vid behov
  README.md
```

Strukturen är **illustrativ**, inte en slutlig kataloglista. Tester får ligga nära berörd kod. Skapa inte framtida moduler som tomma skal.

### 9.2 Git och leverans

- `main` är normalt fungerande integrationsgren.
- Kortlivade `feature/...` och `fix/...`-grenar vid behov.
- Versionskandidater och stabila versioner märks med Git-taggar.
- Releasegren skapas bara om parallellt underhåll faktiskt behövs.
- Utveckling → Release Candidate → Stable; RC testas i verklig användning och får inga nya funktioner.

### 9.3 Utvecklingsmiljö och riktiga data

Development och daglig användning är separata installationer med skilda app-ID:n, inställningar, loggar och standardböcker. DEV markeras tydligt och får inte oavsiktligt skriva till verkliga data. Användaren testar själv appen i vardagen före publik release; ingen fast publiceringstid är beslutad.

### 9.4 CI och testböcker

GitHub Actions införs tidigt och stegvis: grundläggande Rust-/TypeScript-kontroller → integrationstester och plattformskontroller → RC-byggen för macOS, Windows och Linux → reproducerbara stabila paket.

Reproducerbara **syntetiska testböcker** omfattar tom, liten, mellanstor, stor, specialfall och äldre databasversion. De byggs ut med faktisk funktionalitet. Privata verkliga data ingår aldrig i publikt repository.

### 9.5 Beroenden

Välj få, etablerade, välunderhållna bibliotek med lämpliga licenser. Återanvänd svåra tekniska funktioner (redigerare, tillgängliga dialoger, SQLite) men undvik stora visuella ramverk som styr MNO:s uttryck. Inga abstraktioner utan konkret behov.

## 10. Inställningar

Appspecifika val (tema, fönster, backup, uppdateringar) lagras i lokal versionshanterad JSON/TOML-konfiguration; bokrelaterade val och innehåll lagras i SQLite. Migrera inställningar lättviktigt. Felaktiga inställningar får inte skada boken. Nya integritetskänsliga funktioner aktiveras inte automatiskt vid uppdatering.

## 11. Beslutsregister T1–T43

| ID | Beslut | Kärna |
| --- | --- | --- |
| T1 | Plattform | Tauri 2, TypeScript, Rust, SQLite |
| T2 | UI-ramverk | Jämför Svelte 5/Tiptap och React/Lexical före val |
| T3 | Ansvarsfördelning | Rust äger data och domänlogik; UI äger direkt redigeringstillstånd |
| T4 | Journaldokument | Versionshanterad redigeraroberoende JSON och full dataportabilitet |
| T5 | Databas | En SQLite-databas per naturbok; mediafiler separat |
| T6 | Backup | Manuell; automatisk endast opt-in; externa original ingår inte automatiskt |
| T7 | Uppdateringar | Manuell kontroll; automatiska kontroller opt-in; installation explicit |
| T8 | Konfiguration | Lokala JSON/TOML-inställningar skilda från bokdata |
| T9 | Bokmapp | Flyttbar katalog i Dokument som standard; användarval möjlig |
| T10 | Flera böcker | Förbered arkitekturen; en bok i MVP |
| T11 | Databasmigrering | Versionshantering, säkerhetskopior och kompatibilitetskontroll |
| T12 | Sökning | SQLite FTS5, lokalt och återskapbart |
| T13 | Autosparning | Pragmatisk debounce och säker sparning utan överkonstruktion |
| T14 | Kvalitet | Automatiska datatester och verklig manuell användning |
| T15 | Miljöer | Separat DEV och daglig installation/data |
| T16 | Releaseflöde | Development → RC → Stable |
| T17 | Arkitekturstil | Modulär monolit; inget pluginramverk nu |
| T18 | Beroenden | Få, väl valda; eget uttryck; återanvänd svåra standardlösningar |
| T19 | Loggning | Lokal, begränsad, integritetsmedveten; ingen automatisk rapportering |
| T20 | Felisolering | Lokala fel lokalt; skydda dataintegritet |
| T21 | Repository | Ett Git-repository med tydlig intern struktur |
| T22 | Git-grenar | `main`, kortlivade arbetsgrenar, taggar och releasegrenar vid behov |
| T23 | CI | GitHub Actions från början, stegvis utbyggnad |
| T24 | Språk | Svenska/engelska från start; i18n och region separat |
| T25 | Datum | Upplevelsedatum/-tid/-tidszon skilt från tekniska tidsstämplar |
| T26 | Återhämtning | SQLite WAL, säker återstart, kontroller vid behov |
| T27 | Saknade filer | Behåll referenser, återkoppla manuellt, ingen diskskanning |
| T28 | Instanser | En aktiv skrivande instans per naturbok |
| T29 | Skalbarhet | Planera för växande data, mät före avancerad optimering |
| T30 | Integrationer | Tydliga gränser, proveniens, validering och frivillig nätverksåtkomst |
| T31 | Datakällor | Primärdata är sanningen; härledd data kan återskapas |
| T32 | Modellförändring | Bevara betydelse; användarassisterad/obligatorisk migrering vid behov |
| T33 | Exportformat | Öppet, dokumenterat, versionshanterat |
| T34 | Exporttester | Automatisk export–återimport och separat backup-testning |
| T35 | ADR | Numrerade beslut och levande arkitekturöversikt |
| T36 | Testdata | Reproducerbara syntetiska testböcker i olika storlekar |
| T37 | Inställningsmigrering | Versionshantering och säkra standardvärden |
| T38 | Tillgänglighet | Bygg in från start; WCAG 2.2 AA som riktmärke |
| T39 | Genvägar | Plattformstypiska standardgenvägar; egna sparsamt |
| T40 | Ångrahistorik | Normal sessionshistorik, oberoende av autosparning |
| T41 | Bakgrundsarbete | Följsamt, synlig status, blockera oförenliga åtgärder |
| T42 | Avbrott | Allt-eller-inget eller behåll färdigt arbete efter operationstyp |
| T43 | Arkitekturstyrning | Levande arkitektur; principer stabila, teknik omprövbar |

## 12. Öppna frågor och uttryckliga icke-beslut

Följande är **inte** beslutade och ska inte behandlas som fastslagna:

1. Val mellan **Svelte 5 + Tiptap** och **React + Lexical** (kräver prototypjämförelse).
2. Exakt schema, tabellnamn och JSON-format för journalens innehåll.
3. Exakt format och manifeststruktur för fullständiga exporter.
4. Konkret väderkälla, tillståndsflöde och omfattning i v0.1.
5. Exakt installations-/paketeringsstrategi, kodsignering och distributionskanaler för respektive OS.
6. Slutlig katalogstruktur, namnkonventioner och val av små hjälpbibliotek.
7. Detaljerad gränssnittsdesign och interaktionsprototyper utöver redan dokumenterade designprinciper.
8. Datum och innehåll för en publik första release.

## 13. Nästa steg

1. **Granska och godkänn denna arkitekturspecifikation** som utgångspunkt; korrigera eventuella feltolkningar.
2. Lägg den i `docs/` när det nya MNO-repositoryt skapas.
3. Bryt vid behov ut beslut T1–T43 till korta ADR-dokument. Register och översikt ska fortsatt vara lättnavigerade.
4. Skapa en **konkret, stegvis utvecklingsplan för MVP v0.1** med små verifierbara leveranser.
5. Genomför en jämförbar redigerarprototyp innan frontendramverket fastställs.
6. Börja först därefter implementera MNO i den nya projektstrukturen.

---

**Sammanfattande ledstjärna:** *Bygg det som gör MNO unikt. Återanvänd det som andra redan löst väl. Bevara användarens naturhistoria, inte varje gammalt teknikval.*
