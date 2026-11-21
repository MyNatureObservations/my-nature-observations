# Egna fågelobservationer — genomförandeplan för granskning

**Aktuell status — avstämd mot mergad kod och befintlig evidens 20 november 2026.**
Granskad publik main: `0a0339e24d67659a494b9f3d1a9e41664afa4868`.
Se [produktkraven](Egna_fagelobservationer.md). Följande status ersätter äldre
formuleringar om lokalt pågående steg; den ursprungliga planen och alla daterade
utvecklings-/testprotokoll nedan bevaras. Ingen ny testning eller acceptans har utförts.

Den pausade avstämningen mot `4ad78bc` har behållits och kontrollerats mot den nya
main-committen ovan. Mellanskillnaden är endast den mergade lease-testisoleringen
och dess testprotokoll, inte fågelfunktioner, produktionslåsning, schema eller
arkivformat. Befintlig funktions-/acceptansevidens är därför fortsatt relevant;
CI-fixen räknas inte som ny manuell eller paketerad fågelacceptans.

## Aktuell leveransstatus — 20 november 2026

DEV-acceptans gäller endast de rapporterade fallen, inte varje automatiserad
felinjektion. En funktion kan vara DEV-godkänd **och samtidigt** vänta på
paketerad acceptans. Roadmapområdet ”Egna fågelobservationer” lämnas öppet.

### Implementerat med accepterad macOS DEV-evidens

- [x] Skapa från journal eller fristående; spara, återöppna, redigera/Avbryt och
  radera besök. Datum, koordinatstödd namngiven plats, frivilliga tider, flera
  självständiga besök på samma plats/datum, arter/antal, kommentarer och ihågkomna
  namnvisningsval. Offline AviList v2026, svensk/engelsk/vetenskaplig sökning,
  namnluckors reservvisning och det riktade kråkkomplementet finns i kod.
  [Första flödets rapporterade acceptans](../development/MNO_Bird_First_Mac_Flow.md#final-acceptance-and-complete-branch-review-2026-11-13).
- [x] Datumgrupperad översikt, befintlig lista från journalen, länkning från detalj,
  flera journallänkar, riktad avlänkning och återgång/omstart.
  [Fristående flödets DEV-resultat](../development/MNO_Bird_First_Mac_Flow.md#standalone-flow-checkpoint-review--2026-11-16).
  Journalradering bevarar listägaren; transaktioner, sena skrivningar och beständiga
  begärankvitton har automatiserad säkerhetstäckning, inte generell manuell felacceptans.
- [x] Inklistring med uttrycklig granskning/rättning, val vid dubbletter och slutlig
  valideringscopy; Avbryt bevarar formuläret och Apply sparar inte databasen.
  [Inklistringens DEV-acceptans](../development/MNO_Bird_First_Mac_Flow.md#paste-review-checkpoint-review--2026-11-17).
- [x] Ett foto och separat bildtext per artrad, tillägg/byte/borttagning/Avbryt,
  gemensam bildvisare och bevarade original. Backup → Restore och Export → Open
  Import till separata mål med ytterligare omstarter, inklusive länkade/olänkade
  listor, bilder, bildtexter, kommentarer och länkar.
  [Foto-/arkivacceptans](../development/MNO_Bird_First_Mac_Flow.md#bird-photos-and-shared-viewer--final-local-review-and-accepted-macos-dev-checks).
- [x] Eget manuellt och hämtat dygnsväder, dagens prognosmärkning, explicit manuell
  återanvändning, inaktuellt väder och omstart. Separata arkivmål bevarade värden,
  käll-/datum-/prognosmetadata och ursprungligt inaktuellt sammanhang, tillsammans
  med journalväder, foton och länkar.
  [Vädrets DEV-/arkivacceptans](../development/MNO_Bird_Visit_Weather_Persistence.md#final-owner-reported-macos-dev-acceptance--19-november-2026).
- [x] Navigationens och kalenderns **visuella** DEV-acceptans: två huvudkort och
  mindre registreringsfack, diskret dold sidopanel, etiketter och gröna/blå
  typmarkörer. Sista justeringen behåller enbart semantisk aktuell destination,
  hover och tangentbordsfokus.
  [Visuell acceptans](../development/MNO_Observation_Navigation_Calendar.md#owner-visual-acceptance-and-final-quiet-link-adjustment--20-november-2026).
  Gemensam dagboksordning, observationsdagar, unikhet, uppdatering och lämnaskydd
  har [automatiserad slutverifiering](../development/MNO_Observation_Navigation_Calendar.md#final-combined-review-and-local-checkpoint--20-november-2026);
  ingen separat full manuell funktionsmatris för den senaste kombinationen påstås.

Kodspårning: [listkommandon](../../app/src-tauri/src/bird_app.rs),
[listägarskap och skrivskydd](../../app/src-tauri/src/bird_observations.rs),
[formulär](../../app/src/lib/components/BirdListForm.svelte),
[artregister](../../app/src-tauri/src/species_registry.rs),
[katalog/proveniens](../../app/src-tauri/catalog/README.md),
[listväder](../../app/src-tauri/src/bird_visit_weather.rs) och
[gemensam läsvy](../../app/src/lib/components/JournalReading.svelte).
Mergad lagringsnivå är **schema 12 / arkivformat 13** med skyddade uppgraderingar,
äldre stödda läsare och ersatt journal i skydd. Dessa bevarandeskydd har framför
allt automatiserad evidens. Den tidiga manuella uppgraderingen gällde
[schema 4 → 5](../development/testing.md#bird-observations-macos-schema-upgrade-dev-acceptance),
inte ett paketerat direktsteg från publicerad app till dagens schema 12.

### Implementerat, men paketerad acceptans återstår

- [ ] Välj nästa macOS-kandidatversion/källcommit separat och bygg/inspektera en
  faktisk produktionskandidat: arm64, appidentitet, licens/notiser inklusive
  katalogattribution, signatur och SHA-256. Ingen sådan ny fågelkandidat godkänns här.
- [ ] Öppna en syntetisk journal från den publicerade macOS-appen i kandidaten;
  verifiera uppgradering till schema 12, skyddad källkopia, text, bilder, platser,
  journalväder och ännu en omstart. DEV-uppgraderingar eller CI är inte detta test.
- [ ] Kort samlat pakettest av listlivscykel/inklistrning, gemensam dagbok med en
  observationsdag utan anteckning, länkar/återgång, foton/bildvisare och eget väder.
  Kontrollera Backup/Restore och Export/Open Import i separata syntetiska mål.
- [ ] Observera den paketerade filväljarens språk. Appens egna texter är
  lokaliserade; systemknappar styrs inte separat av det befintliga dialog-API:t.

### Överenskommet men inte implementerat

Ingen ytterligare saknad **kodfunktion i den ursprungliga macOS-listomfattningen**
identifierades i denna avstämning. Detta innebär inte att hela roadmapkortet är klart.
Följande är senare produktbeslut och återstår uttryckligen:

- [ ] Kataloguppdatering som data/användarimport med validering före aktivering,
  versions-/källmetadata och bibehållen aktiv katalog vid fel. Historiska namn måste
  kunna slås upp efter byte; nu bevaras taxon-ID/version men registret är fast
  inbäddad AviList v2026. Ingen tyst omklassificering vid splits/merges får införas.
- [ ] Allmänt stöd för BirdLife Sveriges svenska namn. Det källbelagda **Kråka**-
  komplementet och gråkråke-hänvisningen är inte full BirdLife-integration.
- [ ] Regional prioritering av likvärdiga sökträffar, ihågkommen region och bevarat
  försteg för exakta namn utan att låsa arter utanför regionen. Regional status får
  inte beskrivas som lokal abundans.
- [ ] Offline regionkarta och kompakt regionval med korrekt källgeografi,
  beskrivning/källa/version. VP12:s identifierarmappning och dokumenterade
  gränsoklarhet behöver klarläggas före den integrationen; nu finns ingen regionväljare.

### Kända icke blockerande begränsningar och uppskjutet designarbete

- UX-005: blank högeryta i artträff kan kräva två klick i macOS Tauri DEV;
  artnamnet väljer direkt. [Publik UX-backlog #4](https://github.com/MyNatureObservations/my-nature-observations/issues/4).
  Detta är inte fixat eller visuellt accepterat som korrekt interaktion.
- [Systemägda filväljartexter](../development/MNO_Bird_First_Mac_Flow.md#native-picker-investigation)
  kan inte språkstyras separat genom det pinnade dialog-API:t; appstyrd text följer MNO.
- Källexportens 28 svenska namnluckor och taxonomiska kombinationsnamn kräver fortsatt
  korrekt reservvisning/koncepttolkning. Ett separat kråkkomplement finns; original-
  exporten ändras inte. Full namnlikvärdighet över taxonomier påstås inte verifierad.
- Automatisk journalprosa/sammanfattningar, statistik och bredare analys är
  ursprungligen uppskjutna, inte ofärdiga delar av den första leveransen.
- Windows/Linux-fågelaktivering, importleverantörer och nya observationstyper ingår
  inte i denna macOS-leverans. Befintliga plattformsgates ska behållas.

**Minsta nästa steg:** en avgränsad macOS RC-/paketacceptansarbetsorder med vald
ny versionsidentitet, därefter ett syntetiskt uppgraderings- och kärnflödestest av
exakt samma artefakt. Använd den kombinerade dagbokens nya flöde och separata mål
för arkivkontrollerna. Inga katalog-/regionfunktioner behöver läggas till för detta
steg. Nuvarande evidens visar inget ytterligare kodblocker; paket-/uppgraderings-
acceptansen är den konkreta återstående verifieringsgränsen före releasebeslut.

---

## Ursprunglig plan och daterade genomförandenoteringar

Texten nedan bevaras som historik. Exempelvis ”lokalt”, äldre schema-/arkivnummer,
väntande foto-/väder-UI eller ännu öppen huvudnavigation anger dåvarande etapp,
inte dagens leveransstatus. Använd checklistan ovan för aktuellt läge.

## Små modulgränser i befintlig modulär monolit

- **Artregistret** äger det versionsangivna, skrivskyddade AviList-underlaget, Avibase-identitet, namnuppslag och sökning på svenska och engelska. Det äger även källmetadata och attribution, historiskt versionsmedvetet namnuppslag samt framtida validering/aktivering av referensdata. Regional tillhörighet och BirdLife-namn är versionsangivna tillägg till artregistret, inte ersättning för hela AviList-katalogen. Observationsrader bevarar taxonidentitet och använd artregisterversion så att senare registerbyten inte tyst ändrar identifieringar. Den granskade exporten och ägarens licenstolkning styr urvalet. De 28 svenska namnluckorna, godkänd reservvisning och taxonomiska snedstreckskombinationer behöver egna kontroller.
- **Fågelobservationslistor** äger listans datum, platsreferens, besökstider, rader, kommentarer, egna bilder och väder samt kopplingarna till journalen. Det erbjuder avgränsade operationer för spara, läsa, redigera, länka, avlänka och radera. Listan finns oberoende av journalanteckningens livslängd.
- **Journalen** äger fortsatt journaltext och visar listförhandsvisningar genom listmodulens läsgräns. Den lagrar inte kopior av listinnehållet. Samma lista kan förekomma i flera anteckningar.
- **Befintliga gemensamma funktioner** äger platser, vädervärden och hämtning, bildbearbetning, databasåtkomst och backup. Listmodulen använder dem genom små uttryckliga anrop. Inga separata databaser eller tjänster behövs för denna avgränsning.

Kostnaden är tydliga list- och artregistergränser, några nya kommandon och frontendfunktioner samt avgränsad återanvändning av journalbundna hjälpfunktioner. Undvik att flytta hela befintliga moduler. Framtida import kan använda listmodulens validerade spargräns med separat beslut om proveniens och källauktoritet. Andra observationstyper kan senare återanvända beprövade plats- och bildfunktioner. Någon generell observationstyp, pluginmodell eller importmotor behövs inte nu.

## Återanvändning och konkreta anpassningar

| Befintligt område | Återanvändning och gräns |
| --- | --- |
| [journal.rs](../../app/src-tauri/src/journal.rs), [database_state.rs](../../app/src-tauri/src/database_state.rs), [journal_search.rs](../../app/src-tauri/src/journal_search.rs) | Använd befintligt databasläge, generationskontroll och samordning av skrivningar och underhåll. Kopiera inte journalens nya-post- eller autosavebeteende till listan. Sparande och första journallänk ska vara en transaktion. Sena sparförsök får inte återskapa raderade listor. |
| [place_registry.rs](../../app/src-tauri/src/place_registry.rs) och frontendens platsväljare/karta | Återanvänd namngivna platser, koordinatvalidering och val av position. Lägg till listors användning i platsens användningskontroll innan listor får referera till platser. Behåll versionsmedveten schemaspärr och platsvalidering när nya fält införs. |
| [weather.rs](../../app/src-tauri/src/weather.rs), [weather/historical.rs](../../app/src-tauri/src/weather/historical.rs), WeatherForm och WeatherAttribution | Återanvänd värdevalidering, manuell inmatning, lämplig historisk hämtning och attribution. Lagring och sparflöde är journalbundna och behöver en separat liten listkoppling. Hämtning utgår från listans plats/datum/besök. Saknade lämpliga data lämnar manuellt väder möjligt. |
| [photographs.rs](../../app/src-tauri/src/photographs.rs) och dess undermoduler | Återanvänd validering, avkodning, förhandsvisningar och originalreferenser. Nuvarande tillgångar och sparvägar är journalbundna. Listbilder behöver eget ägarskap per fågelrad, högst ett foto, och ska överleva journalradering. |
| [backup.rs](../../app/src-tauri/src/backup.rs), [backup/restore.rs](../../app/src-tauri/src/backup/restore.rs), [backup/journal_content.rs](../../app/src-tauri/src/backup/journal_content.rs), [storage_content.rs](../../app/src-tauri/src/storage_content.rs) | Återanvänd SQLite-snapshot, arkivkontroller, kandidatbyggare, innehållsjämförelse och verifierad återställning. Utöka schema- och innehållskontroller till listor, även olänkade, och alla nya ägda data. |
| JournalReading, sparad anteckningsvy, befintliga menyer och dialoger | Återanvänd navigations- och lämnaskydd, lokalisering, Cancel-fokus och journaländringssignaler. Lägg listförhandsvisningar under texten med plats, datum, tider och de tre första arterna med antal. Markera fler rader och öppna hela listan. |

## Fyra självständigt verifierbara steg

### 1. Första hela flödet från journalen med säkert bevarande

Skapa, spara och återöppna en lista från en sparad journalanteckning. Använd enskilt artval från det granskade AviList-underlaget, svenska/engelska namn och obligatoriskt antal. Välj återanvändbar plats med namn och koordinater samt datum. Föreslå journalens datum och plats där de finns och låt användaren ändra dem utan att ändra journalen. Frivilliga besökstider kan ingå direkt. Skapa journallänken först vid lyckat sparande. Avbrutet skapande lämnar ingen lista eller länk. Visa en kompakt klickbar förhandsvisning och återöppna efter omstart. Återgång från listan går till den journalanteckning som öppnade den. Saknat svenskt namn visas med engelskt och vetenskapligt namn samt en diskret markering; arten kan registreras.

Detta steg omfattar den första nödvändiga schemaändringen för listor, fågelrader, artidentifiering och journallänkar, med självständig besöksidentitet; flera listor får ha samma plats och datum och besökstider är inte unik identitet. Inför inga sparbara tomma ersättningsobjekt vid fel. Databas-, plats- och innehållsvaliderare behöver uppdateras samordnat. Journalradering ska redan nu bevara listan. Nya sparade data måste kunna hittas igen även efter att en länk försvunnit, exempelvis genom en enkel listöversikt.

**Bevarande före skrivbar funktion.** Först verifieras nytt schema, säker uppgradering från aktuellt schema 4, backup och återställning av de data som detta steg tillåter. Backupens format-/schemakombination, kandidatbyggare och innehållsdigest måste förstå listor och länkar. Bevara tidigare formatläsare och frysta protokolldigester. Äldre appar ska avvisa ett nytt inkompatibelt schema/format, aldrig skapa en backup som påstås komplett men utelämnar observationer. Om en säker migrering inte kan isoleras, avgränsa den som separat förarbete och behåll listskrivning otillgänglig.

**Tester och klartkriterium.** Ny och uppgraderad syntetisk databas, avbruten migrering med bevarat original, transaktionsfel, ogiltig art/antal, avbrutet skapande, flera besök på samma plats/datum, omsändning av samma sparförsök efter förlorat svar, avvisad återanvändning av sparidentitet med annat innehåll, utgången generation och omstart. Backup → verifierad kandidat → återställning ska bevara både länkade och olänkade sparade listor med artidentitet och plats. Relevant befintlig startup, backup, Restore och journalradering ska passera. Först därefter får det första flödet accepteras.

### 2. Fristående användning, delade länkar och hela livscykeln

Lägg till fristående skapande, länkning av befintlig lista från journalen och senare länkning från listan. Samma lista kan länkas till flera journalanteckningar och en anteckning kan länka till flera listor. Återgång går till den ursprungliga journalanteckningen när listan öppnades därifrån, oavsett övriga länkar. Fristående öppning ger återgång till observationsöversikten. Redigering uppdaterar alla förhandsvisningar. Avlänkning bevarar båda objekten och andra länkar. Radering av lista kräver bekräftelse och tar bort dess förhandsvisningar utan att röra journaltext eller originalbilder.

**Tester och klartkriterium.** Återöppna olänkad lista, verifiera återgång till rätt ursprungsanteckning från båda länkarna till samma lista och till observationsöversikten efter fristående öppning, avbryt från båda ingångarna, inga dubbla länkar, delad redigering, avlänkning, journalradering, avbruten och bekräftad listradering, rollback och sena sparanrop efter radering. Återställ en lista med flera journallänkar och kontrollera samma listidentitet. Behåll befintligt journalbeteende och tangentbordsåtkomst.

### 3. Effektiv registrering och ett foto per rad

Lägg till inklistring som ”Talgoxe 6st” med tydlig gransknings- och rättningsfas före sparande. Enskild och inklistrad registrering kan blandas i samma lista. Oklara namn och antal ska kräva rättning, utan gissad artidentitet. Radkommentar ingår nu i pågående steg 1 enligt förtydligandet ovan. Lägg här till ett frivilligt foto per fågelrad.

**Tester och klartkriterium.** Båda språkens artnamn, okända/tvetydiga namn, felaktigt antal, blandad inmatning och avbruten granskning. Bildval och avbrutet bildval, sparade förhandsvisningar, originalreferenser och oförändrade originalfiler. Uppdatera schema och backupverifiering för kommentarer och bilder före första skrivbara sådan funktion. Återställ även olänkade listors bilder och kommentarer. Raderings- och sena-sparningar-tester måste omfatta bilder.

### 4. Listans eget frivilliga väder och samlad acceptans

Använd manuellt väder och historisk hämtning när lämpliga data finns för listans plats och besök. Vädret delas av listans fågelrader, är oberoende av journalvädret och kan visas kompakt vid plats och tid. Bevara attribution. Avgränsa tidsmässig representation mot befintligt väderunderlag vid detaljgranskningen, utan att presentera en dagsuppgift som verifierat väder för en exakt tidpunkt.

**Tester och klartkriterium.** Saknat, manuellt och hämtat väder, nätverksfel, korrigerad plats/tid och sena svar. Journalvädret får inte ändras. Utöka schema, validering, innehållsdigest och backup före vädersparande. Gör slutlig återställningskontroll med alla kravfält, länkade och olänkade listor, foton och flera länkar. Automatiska sammanfattningar och statistik kvarstår som uppskjutna.

**Lagringsgrund, beslut 19 november 2026.** Separata manuella/hämtade väderdelar ägs av besökslistan, med ursprungligt plats-/datum-/tidssammanhang och bestående inaktuell status. Hämtning avser initialt dygnsdata; dagens data märks prognos. Manuellt väder kan beskriva besöket. Inaktuellt väder utesluts från aktuell sammanfattning och manuell återanvändning kräver uttrycklig bekräftelse. Schema 12/arkivformat 13 följer skyddad kandidatmigration och äldre innehållsbevis. Inga besöksväderkommandon, nätverksanrop eller gränssnitt aktiveras i detta steg. Se [lagringsmodell och verifiering](../development/MNO_Bird_Visit_Weather_Persistence.md).

## Förtydligande för pågående steg 1: presentation och kommentarer

[De nya presentationsbesluten](Egna_fagelobservationer.md#presentation-och-radkommentarer--beslut-12-november-2026) tillämpas i skapande/återöppning: kompakt datumvillkorad journalförhandsvisning, mjuka artkort och gemensamt ihågkomna namnvisningsval. Kommentar flyttas fram från steg 3 till denna etapp, men generell redigering och foton är fortsatt senare arbete. Vid nytt artval fokuseras en redan tillagd arts antal utan förändring av antal/kommentar. Befintliga dubblettrader migreras och visas oförändrade.

Den lokala implementationen använder schema 8 (nullable `bird_rows.comment`), innehållsbevis 9 och arkivformat 9. Uppgradering från grenens schema 7 sker i befintlig verifierad kandidatväg med bevarad källkopia; inga profiler behöver tömmas. Äldre läsare/kontrakt behålls. Kommentarsdata omfattas av atomiskt sparande och omsändningsjämförelse samt migrering, backup och export/import innan UI får skriva dem. Nya listkommandon avvisar upprepat taxonkoncept; ingen unikhetsändring som skulle förstöra äldre dubbletter läggs till.

Namnvisningsval är lokala visningsinställningar, separata från journalinnehållet. De återläses i varje lista och påverkar även artnamn i skapandet; förhandsvisningar visar inte extra namn. Kommentarer är vanlig text, visas utan HTML-tolkning och har en uttrycklig teknisk gräns på 8192 UTF-8-byte utan nolltecken, med felhjälp vid kontrollen. Tom kommentar sparas som null av formuläret. Originalbilder påverkas inte.

**Verifiering:** namnval över listor/omstart, båda språken och namnluckor, omval av art med bevarad kommentar/antal, separata besöksförhandsvisningar med datumvillkor och ursprungsnavigation, äldre dubblettrader, kommentarsmigrering med avbrott/retry, omstart, arkivåterläsning och skydd av ersatt journal. Godkänd funktion ska inte förväxlas med accepterad visuell utformning; den nya presentationen väntar på manuell DEV-granskning.

## Senare avgränsat katalog- och regionsteg

Utgå från [beslut om livscykel, sökning och region](Egna_fagelobservationer.md#kataloglivscykel-namn-och-region--beslut-12-november-2026) och [VP12-referenserna](Egna_fagelobservationer.md#underlag-för-regionväljaren). Behåll modulär monolit och artregistrets ägarskap; ingen generell importmotor.

1. **Före första katalogbytet:** säkra uppslag utifrån observationens lagrade version och Avibase-ID, samt tillgång till historiskt namnunderlag. Ett byte av en enda aktiv `VERSION`-konstant får inte ersätta tidigare namnunderlag. Identifiera datautgåva och källmetadata uttryckligt, inklusive namnunderlagets revision/digest när den är känd. Välj minsta bevarandealternativ vid separat teknisk design: behåll oföränderliga äldre katalogutgåvor eller bevara tillräckliga namnsnapshots. Kravet är läsbarhet och identitet, inte en bestämd lagringsmodell. Samordna backup/Restore/Export/Open Import innan nytt katalogberoende innehåll aktiveras.
2. **Datauppdatering:** bygg en liten artregisterägd gräns för användarimport av katalogdata med metadata, strikt validering och säker aktivering. Fel ska lämna aktiv och historiskt nödvändig data intakt. Använd befintliga beprövade bevarande-/publikationsmekanismer där de passar, utan att nu utforma ett nytt generellt ramverk. Split/merge blir ett uttryckligt framtida identifieringsbeslut, aldrig en automatisk ändring.
3. **Regionala namn/sökning:** verifiera VP12:s versionsidentitet, valda geografiska definition och taxonmappning innan BirdLife-namn och regionala flaggor används. Behåll alla AviList-arter och exakta namnträffars företräde. Lägg regional prioritet endast på annars likvärdiga träffar, utan sällsynthetsupplåsning. Separera regional status från lokal abundans.
4. **Regionval:** spara vald region som användarinställning. Visa offlinekarta med dämpad grön markering, källriktig geografi, kort beskrivning och källa/version i väljaren. Registreringsvyn har bara kompakt region-/ändrakontroll. Detaljerad UX och geografiskt dataformat granskas senare.

**Verifiering:** katalogbyte och felaktig import utan förlust av aktiv katalog, historiska observationer och arkivåterläsning över versionsbyte; split/merge utan automatisk omklassificering; båda namnkällorna, exakt-matchning före regional prioritet, val av utomregional art; korrekt offlinegeografi och ihågkommet regionval. Denna arbetsordning öppnar inte det redan fattade AviList-licensbeslutet.

## Kataloggranskning inför merge av första flödet

Granskad lokal kod på `codex/mac-bird-list-flow`, med bas `748ea0198447844026888d4d6e36142f6958fa7f` och ocommittade ändringar. Detta är källgranskning, inte ett nytt apptest.

| Konkret i koden | Konsekvens och minsta nödvändiga uppföljning |
| --- | --- |
| [species_registry.rs](../../app/src-tauri/src/species_registry.rs) använder `include_str!`, `LazyLock` och en fast v2026-konstant. [Konverteraren](../../app/scripts/birds/convert.py) accepterar endast den godkända CSV-filens digest; [källmetadata](../../app/src-tauri/catalog/README.md) är dokumenterade. | Nuvarande lokala flöde har ingen katalogbytesfunktion och ska inte beskrivas som datauppdaterbart ännu. Detta är inget belagt mergehinder för den uttryckligen fasta v2026-etappen. Inför uppdatering behövs en separat dataladdnings-/aktiveringsgräns och maskinellt validerad versions-/källmetadata. Att bara byta konstant eller JSON-fil är inte en säker uppdateringslösning. |
| [bird_observations.rs](../../app/src-tauri/src/bird_observations.rs) och [storage_content.rs](../../app/src-tauri/src/storage_content.rs) bevarar Avibase-ID och registrerad version i rader, innehållsbevis och arkiv. [bird_app.rs](../../app/src-tauri/src/bird_app.rs), `read_view`, slår däremot upp namn endast när lagrad version är den aktiva; annars visas ID och tomt vetenskapligt namn. | Identitet/version bevaras nu, men framtida versionsbyte skulle förlora begriplig namnvisning om dagens uppslag kopieras oförändrat. Versionsmedvetet historiskt namnuppslag och dess arkivbevarande är ett konkret krav **före första katalogbytet**. Inget nytt databasschema krävs enbart för att dokumentera beslutet. Full läsbarhet för flera katalogversioner får inte påstås verifierad nu. |
| Sökningen använder hela katalogen, Unicode-gemener och exakt/prefix/delsträngsprioritet före 30-träffsgränsen. Inga regionala flaggor, BirdLife-alias eller regionsinställningar finns. | Exakt-matchningsordningen kan återanvändas. Regional flagga får bli en sekundär prioritet, inte ett filter. Dessa funktioner och deras VP12-mappning kan vänta utan att blockera det fasta globala första flödet. |

**Mergeavgränsning:** inga nya konkreta kodblockerare har belagts enbart genom dessa framtida katalogbeslut, så länge merge gäller första flödet med oförändrad, medföljande v2026-katalog. Katalogbyte, BirdLife-namn och regionval ska förbli otillgängliga tills respektive bevarandekrav är verifierat. Historiskt namnuppslag är den första grundändringen i det senare katalogarbetet; ingen sådan ändring implementeras i denna granskning. Detta är inte en generell slutgranskning av hela den ocommittade implementationen.

## Manuell acceptans och kvarvarande UX-granskning

Projektägaren har bekräftat korrigerad artsökning samt flera separata besök på samma plats/datum, med och utan tider, inklusive sparande och återöppning efter omstart i macOS DEV. Det är funktionsacceptans för dessa kontroller, inte paketerad releaseacceptans. Översiktens täthet och detaljsidans visuella design är **kvar för UX-granskning** och ska inte beskrivas som accepterad design. Se [lokal flödes-/testredovisning](../development/MNO_Bird_First_Mac_Flow.md).

## Schema, backup och plattformar

Den ursprungliga planens bas använde schema 4. Den lokala implementationens senare schema-/arkivsteg och verifierade kandidatuppgradering beskrivs i [flödesredovisningen](../development/MNO_Bird_First_Mac_Flow.md). [database.rs](../../app/src-tauri/src/database.rs) och startup behåller plattformsvillkor och strikt innehållsverifiering. Vid ytterligare beständiga katalog-/namnuppgifter ska samma etablerings-, identitets-, lås- och persistensskydd bevaras.

Hel SQLite-kopia gör inte nya tabeller automatiskt verifierade. [Restore](../../app/src-tauri/src/backup/restore.rs) kontrollerar uttryckliga format/schema-par. Kandidatbygge, innehållsjämförelse och öppning måste behandla nya data konsekvent. Varje nytt beständigt fält behöver skyddas innan det exponeras. Välj exakt schema- och formatversionsnummer vid implementation efter kontroll av dåvarande main. Undvik framtida tomma tabeller bara för att minska antalet versioner.

Steg 1 börjar med macOS-acceptans. Kontrollera samtidigt Windows/Linux-kompilering och befintlig startup/backup i CI. Ingen plattform får tappa nya data vid öppning eller backup. Håll ny skrivfunktion otillgänglig på plattformar vars migrering och återställningsväg ännu inte är verifierade. Linux Restore/Recovery-begränsningar måste respekteras och behandlas uttryckligen innan fågelfunktionen levereras där.

## Minsta första arbetsorder och macOS DEV-acceptans

Godkänn steg 1 med dess schema- och bevarandekontroller som en vertikal leverans. Börja med en syntetisk databastest som sparar och återställer listan innan UI kopplas på. Om migreringsarbetet visar sig större, gör endast det förarbetet först.

1. Starta macOS DEV med en isolerad syntetisk journal vars sista katalog ännu inte finns. Använd befintlig DEV-konfiguration och `MNO_TEST_DATA_DIR`. Skapa en journalanteckning med datum och plats.
2. Skapa en lista därifrån. Kontrollera föreslagna värden, ändra listans datum eller plats och lägg till arter och antal med svenska och engelska namn. Spara och kontrollera att journalen behåller sina värden.
3. Öppna hela listan från förhandsvisningen och återgå till den ursprungliga journalanteckningen. Kontrollera tre första arter, antal och markering för fler rader. Starta om och kontrollera innehållet.
4. Avbryt ett nytt skapande och kontrollera att inget nytt objekt eller länk finns kvar.
5. Skapa extern backup, återställ endast till en separat syntetisk målprofil och kontrollera listor och länkar. Kör samma automatiserade bevarandekontroll för olänkad lista och verifiera att journalradering inte tar bort listan.

Navigationsacceptansen ska verifiera den godkända återgången utifrån var listan öppnades. Steg 1 ska också verifiera registrering med engelskt och vetenskapligt namn och diskret markering när svenskt namn saknas. Ägarens dokumenterade licenstolkning, okänt exportdatum och taxonomiska namnkontroller följer med artregistrets underlag.


## Lokal uppföljning av steg 2 — fristående listor och journalkopplingar

På grenen `codex/mac-bird-observations-overview`, från publik main
`788a740734ca871f7590641a9a563df7fff11e24`, återanvänds det mergade första
flödet för observationsöversikt, fristående skapande/återöppning/redigering/radering
samt val av befintlig lista och separat avlänkning från en sparad journalanteckning.
Schema 9 och arkivformat 10 återanvänds utan ändring. Översikten läser listägare och
visar varje identitet en gång; återgång styrs av öppningssammanhanget, inte listans
övriga länkar. Listor överlever journalradering och avlänkning. Samma lista kan
länkas till flera anteckningar utan dubbla kopplingar.

Detta är lokal implementation med automatiska kontroller. Projektägaren har den
16 november 2026 bekräftat macOS DEV-kontroller av datumgruppering/besöksordning,
segmenterad tidsinmatning, länkning/riktad avlänkning från detaljen samt återgång
och bevarande efter omstart. Detta är inte paketerad releaseacceptans. Se [resultat och samma-profils-checklista](../development/MNO_Bird_First_Mac_Flow.md#standalone-lists-overview-and-journal-links--local-next-stage).
UX-005 är oförändrad och icke blockerande. Senare artinklistrning, foton, listväder,
katalog-/regionarbete och kalendermarkeringar ingår inte här.

Slutgranskningen mot basen ovan fann inga konkreta korrekthets- eller
bevarandeblockerare. UX-005 och den uppskjutna utformningen av huvudnavigationen
kvarstår som icke blockerande uppföljningar. Se
[granskning och lokal checkpoint](../development/MNO_Bird_First_Mac_Flow.md#standalone-flow-checkpoint-review--2026-11-16).

## Lokalt fotosteg efter granskad inklistring — 17 november 2026

Ett frivilligt foto per fågelrad implementeras på `codex/mac-bird-row-photo` utan
att ändra produktkraven. Bildval och förhandsvisningsbearbetning återanvänds, men
ägarskapet ligger hos fågelraden så att journalradering inte tar bort bilden.
Skyddad kandidatuppgradering till schema 10 och arkiv-/innehållsformat 11 omfattar
förhandsvisningar och originalreferenser innan formuläret tillåts spara dem.
Originalfiler ingår fortsatt inte i arkiven och ändras aldrig. Sparande är fortsatt
atomiskt via listans befintliga generations-/skrivskydd; avbruten redigering sparar
inte bildändringar. Se [implementation, verifiering och DEV-checklista](../development/MNO_Bird_First_Mac_Flow.md#optional-bird-row-photo--local-implementation-pending-macos-dev-acceptance).
Manuell macOS DEV-acceptans av fotosteget återstår. Väder, regionala funktioner,
kataloguppdatering och UX-005 ändras inte.


### Frivillig bildtext i det lokala fotosteget

Bildtext lagras separat från radkommentar och sparas bara med formulärets vanliga
Spara. Bildbyte behåller texten för granskning; bildborttagning tar även bort bildtexten
vid sparande. Avbryt lämnar lagrat innehåll oförändrat. Den gemensamma bildvisaren visar
bildtext och art/plats/datum separat, även från journalens länkade miniatyrer. Schema 11
/ arkivformat 12 följer befintlig skyddad uppgradering och bevarar äldre läsare.
Se [lokal verifiering och återstående manuell acceptans](../development/MNO_Bird_First_Mac_Flow.md#optional-bird-photo-captions--local-follow-up-17-november-2026).


### Fotostegets DEV-acceptans och lokal granskning

Projektägaren har godkänt tillägg/byte/borttagning/Avbryt, bildtexter, kontrollernas
placering och den gemensamma bildvisaren. Separata målprofiler har godkänts för både
Backup → Restore och Export → Open Import, inklusive ännu en omstart och bevarade
journalbilder, länkade/olänkade observationsbilder, bildtexter, radkommentarer och
journalkopplingar. Detta är manuell macOS DEV-acceptans, inte paketerad releaseacceptans.
Se [slutlig lokal granskning och testbevisning](../development/MNO_Bird_First_Mac_Flow.md#bird-photos-and-shared-viewer--final-local-review-and-accepted-macos-dev-checks).
Denna acceptans ersätter tidigare väntande fotostatus ovan. Windows/Linux-grindar,
UX-005 och övrig uppskjuten produktomfattning är oförändrade.


## Gemensam navigation och kalender — 19 november 2026

Implementera [navigationsbeslutet](Egna_fagelobservationer.md#navigation-och-typbaserad-kalender--beslut-19-november-2026)
genom befintliga journal-/observationsrutter och dagpanelen. Fågelregistrering är
den enda aktiva observationstypen; ingen typväljare eller importkälla behövs.
Startsidan förblir journalcentrerad. Sparad journal använder en sammanhållen
Lägg till observation-meny med Skapa ny/Koppla befintlig.

Återanvänd `read_bird_lists` via generationskontrollerad läsare och tillgänglighetsgate.
Markörerna räknar listidentiteter/besöksdatum, aldrig antal journallänkar eller
importkälla. Journalmarkörer använder oförändrat datumkommando. Dagpanelen visar
båda typerna i separata avsnitt; observationsöppning ger översikten som återgång.
Befintlig ändringssignal uppdaterar båda; redo-vyn monteras om efter Recovery och
journalbyte. Sena svar ignoreras vid datumbyte, ny läsning och avmontering.

Inga schema-/arkivändringar eller backendkommandon behövs. Framtida andra typer
kan få egen markeringsfärg utan ett pluginramverk. Testa datum-/typkombinationer,
fristående och flerlänkade besök, ändring/radering, kontext, tangentbord och gates.
[Lokalt resultat och manuell DEV-checklista](../development/MNO_Observation_Navigation_Calendar.md).


### Godkänd Home/sidopanelriktning och gemensam läsvy — 20 november 2026

Fyrkortslayouten är avvisad, liksom det tidigare textlänksförslaget. Återanvänd de
två etablerade huvudkorten och lägg ett mindre registreringsfack under dem.
Sidopanelen är fortsatt dold tills menyknappen används och ger tillgång till
Naturdagboken och Observationer. Behåll den befintliga dagpanelen.

Utöka `JournalReading` med besök från befintligt generationskontrollerat
`read_bird_lists`; inga nya backendkommandon behövs. Samla på anteckningarnas
respektive besökens egna datum. Bevara journalens ordning och renderare, visa varje
besöksidentitet en gång och använd kompakt länk i journaltextens observationsdel.
Fortsätt och explicit datum ska även hitta observationsdagar. Läs båda delarna
innan initial navigation; fel eller olika generationer får inte tyst gömma besök.
Ändringssignalen laddar om i befintlig vy utan att återställa läsposition.
Fristående besöksöppning använder befintlig observationsrutt/återgång; referens
från journalen behåller journalanteckningens kontext.

Se [aktuellt genomförande och DEV-checklista](../development/MNO_Observation_Navigation_Calendar.md#approved-home-and-sidebar-direction--20-november-2026).

## Valbar macOS-aktivering — 20 november 2026

Det [uttryckliga modulbeslutet](Egna_fagelobservationer.md#valbar-fågelmodul--uttryckligt-produktbeslut-20-november-2026)
inför en liten separat kontoinställning, inte en ny modulplattform.
Rusts `BirdModule` läser en versionsmärkt boolesk inställning i Tauri-appens
kontokonfigurationskatalog i en separat syskonnamnrymd utanför journalroten. Programidentifieraren separerar DEV från produktion.
Saknad/ogiltig inställning är avstängd; skrivfel rapporteras utan att ändra valet.

Alla fågelkommandon antas under ett lästillstånd som hålls genom kommandot.
Ändring av inställningen tar exklusivt tillstånd och väntar på antagna operationer;
väderpreviews ogiltigförklaras före slutförandet. Frontend förfrågar formulärets
befintliga lämna-dialog före avstängning och avstår om skrivningar/bildarbete eller
osäkra resultat ännu behöver klarläggas. Fågelvyn lämnas före inställningsbytet;
vyernas livscykelskydd avvisar sena svar. Ändringshändelser uppdaterar fågelläsning, navigation/kalender och bildsamlingar
utan att montera om journalredigeraren eller kasta kärnans osparade innehåll.

Persistens-, migrations- och arkivlagren frågar inte efter modultillgänglighet.
Schema 12/arkivformat 13 och Windows/Linux-gates behålls. Journalens bilder,
bildvisare och väder förblir kärna. Inställningen ingår inte i journalarkivet.

Se [lokal rapport och separat DEV-checklista](../development/MNO_Optional_Bird_Module.md).
Den accepterade 0.2.0-kandidaten från `1685de6` ersätts inför kommande byggnad;
ingen ny paketacceptans eller publicering har genomförts för moduländringen.
