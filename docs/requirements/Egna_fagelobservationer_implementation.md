# Egna fågelobservationer — genomförandeplan för granskning

**Status — godkänt produktunderlag med lokalt pågående implementation av steg 1.** Ursprunglig plan från 11 november 2026; kompletterad med produktbesluten den 12 november 2026. Se de [överenskomna kraven](Egna_fagelobservationer.md). Kataloguppdateringar och regionväljare är planerade senare funktioner, inte implementerade i detta dokumentationssteg.

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
