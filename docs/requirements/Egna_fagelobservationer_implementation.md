# Egna fågelobservationer — genomförandeplan för granskning

**Status — föreslagen arbetsordning, 11 november 2026.** Underlaget är de [överenskomna kraven](Egna_fagelobservationer.md) på commit `98b44c1`. Planen behöver godkännas före implementation.

## Små modulgränser i befintlig modulär monolit

- **Artregistret** äger det versionsangivna, skrivskyddade AviList-underlaget, Avibase-identitet, namnuppslag och sökning på svenska och engelska. Det äger även källmetadata och attribution. Observationsrader bevarar taxonidentitet och använd artregisterversion så att senare registerbyten inte tyst ändrar identifieringar. Den granskade exporten och ägarens licenstolkning styr urvalet. De 28 svenska namnluckorna, föreslagen reservvisning och taxonomiska snedstreckskombinationer behöver egna kontroller.
- **Fågelobservationslistor** äger listans datum, platsreferens, besökstider, rader, kommentarer, egna bilder och väder samt kopplingarna till journalen. Det erbjuder avgränsade operationer för spara, läsa, redigera, länka, avlänka och radera. Listan finns oberoende av journalanteckningens livslängd.
- **Journalen** äger fortsatt journaltext och visar listförhandsvisningar genom listmodulens läsgräns. Den lagrar inte kopior av listinnehållet. Samma lista kan förekomma i flera anteckningar.
- **Befintliga gemensamma funktioner** äger platser, vädervärden och hämtning, bildbearbetning, databasåtkomst och backup. Listmodulen använder dem genom små uttryckliga anrop. Inga separata databaser eller tjänster behövs för denna avgränsning.

Kostnaden är tydliga list- och artregistergränser, några nya kommandon och frontendfunktioner samt avgränsad återanvändning av journalbundna hjälpfunktioner. Undvik att flytta hela befintliga moduler. Framtida import kan använda listmodulens validerade spargräns med separat beslut om proveniens och källauktoritet. Andra observationstyper kan senare återanvända beprövade plats- och bildfunktioner. Någon generell observationstyp, pluginmodell eller importmotor behövs inte nu.

## Återanvändning och konkreta anpassningar

| Befintligt område | Återanvändning och gräns |
| --- | --- |
| [journal.rs](../../app/src-tauri/src/journal.rs), [database_state.rs](../../app/src-tauri/src/database_state.rs), [journal_search.rs](../../app/src-tauri/src/journal_search.rs) | Använd befintligt databasläge, generationskontroll och samordning av skrivningar och underhåll. Kopiera inte journalens nya-post- eller autosavebeteende till listan. Sparande och första journallänk ska vara en transaktion. Sena sparförsök får inte återskapa raderade listor. |
| [place_registry.rs](../../app/src-tauri/src/place_registry.rs) och frontendens platsväljare/karta | Återanvänd namngivna platser, koordinatvalidering och val av position. Lägg till listors användning i platsens användningskontroll innan listor får referera till platser. Nuvarande schemaspärr accepterar endast schema 3 och 4 och måste förstå det nya schemat. |
| [weather.rs](../../app/src-tauri/src/weather.rs), [weather/historical.rs](../../app/src-tauri/src/weather/historical.rs), WeatherForm och WeatherAttribution | Återanvänd värdevalidering, manuell inmatning, lämplig historisk hämtning och attribution. Lagring och sparflöde är journalbundna och behöver en separat liten listkoppling. Hämtning utgår från listans plats/datum/besök. Saknade lämpliga data lämnar manuellt väder möjligt. |
| [photographs.rs](../../app/src-tauri/src/photographs.rs) och dess undermoduler | Återanvänd validering, avkodning, förhandsvisningar och originalreferenser. Nuvarande tillgångar och sparvägar är journalbundna. Listbilder behöver eget ägarskap per fågelrad, högst ett foto, och ska överleva journalradering. |
| [backup.rs](../../app/src-tauri/src/backup.rs), [backup/restore.rs](../../app/src-tauri/src/backup/restore.rs), [backup/journal_content.rs](../../app/src-tauri/src/backup/journal_content.rs), [storage_content.rs](../../app/src-tauri/src/storage_content.rs) | Återanvänd SQLite-snapshot, arkivkontroller, kandidatbyggare, innehållsjämförelse och verifierad återställning. Utöka schema- och innehållskontroller till listor, även olänkade, och alla nya ägda data. |
| JournalReading, sparad anteckningsvy, befintliga menyer och dialoger | Återanvänd navigations- och lämnaskydd, lokalisering, Cancel-fokus och journaländringssignaler. Lägg listförhandsvisningar under texten med plats, datum, tider och de tre första arterna med antal. Markera fler rader och öppna hela listan. |

## Fyra självständigt verifierbara steg

### 1. Första hela flödet från journalen med säkert bevarande

Skapa, spara och återöppna en lista från en sparad journalanteckning. Använd enskilt artval från det granskade AviList-underlaget, svenska/engelska namn och obligatoriskt antal. Välj återanvändbar plats med namn och koordinater samt datum. Föreslå journalens datum och plats där de finns och låt användaren ändra dem utan att ändra journalen. Frivilliga besökstider kan ingå direkt. Skapa journallänken först vid lyckat sparande. Avbrutet skapande lämnar ingen lista eller länk. Visa en kompakt klickbar förhandsvisning och återöppna efter omstart.

Detta steg omfattar den första nödvändiga schemaändringen för listor, fågelrader, artidentifiering och journallänkar, med en lista per plats/datum och självständig listidentitet. Inför inga sparbara tomma ersättningsobjekt vid fel. Databas-, plats- och innehållsvaliderare behöver uppdateras samordnat. Journalradering ska redan nu bevara listan. Nya sparade data måste kunna hittas igen även efter att en länk försvunnit, exempelvis genom en enkel listöversikt.

**Bevarande före skrivbar funktion.** Först verifieras nytt schema, säker uppgradering från aktuellt schema 4, backup och återställning av de data som detta steg tillåter. Backupens format-/schemakombination, kandidatbyggare och innehållsdigest måste förstå listor och länkar. Bevara tidigare formatläsare och frysta protokolldigester. Äldre appar ska avvisa ett nytt inkompatibelt schema/format, aldrig skapa en backup som påstås komplett men utelämnar observationer. Om en säker migrering inte kan isoleras, avgränsa den som separat förarbete och behåll listskrivning otillgänglig.

**Tester och klartkriterium.** Ny och uppgraderad syntetisk databas, avbruten migrering med bevarat original, transaktionsfel, ogiltig art/antal, avbrutet skapande, plats/datum-konflikt, utgången generation och omstart. Backup → verifierad kandidat → återställning ska bevara både länkade och olänkade sparade listor med artidentitet och plats. Relevant befintlig startup, backup, Restore och journalradering ska passera. Först därefter får det första flödet accepteras.

### 2. Fristående användning, delade länkar och hela livscykeln

Lägg till fristående skapande, länkning av befintlig lista från journalen och senare länkning från listan. Samma lista kan länkas till flera journalanteckningar och en anteckning kan länka till flera listor. Redigering uppdaterar alla förhandsvisningar. Avlänkning bevarar båda objekten och andra länkar. Radering av lista kräver bekräftelse och tar bort dess förhandsvisningar utan att röra journaltext eller originalbilder.

**Tester och klartkriterium.** Återöppna olänkad lista, avbryt från båda ingångarna, inga dubbla länkar, delad redigering, avlänkning, journalradering, avbruten och bekräftad listradering, rollback och sena sparanrop efter radering. Återställ en lista med flera journallänkar och kontrollera samma listidentitet. Behåll befintligt journalbeteende och tangentbordsåtkomst.

### 3. Effektiv registrering, kommentarer och ett foto per rad

Lägg till inklistring som ”Talgoxe 6st” med tydlig gransknings- och rättningsfas före sparande. Enskild och inklistrad registrering kan blandas i samma lista. Oklara namn och antal ska kräva rättning, utan gissad artidentitet. Lägg till frivillig kommentar och ett foto per fågelrad.

**Tester och klartkriterium.** Båda språkens artnamn, okända/tvetydiga namn, felaktigt antal, blandad inmatning och avbruten granskning. Bildval och avbrutet bildval, sparade förhandsvisningar, originalreferenser och oförändrade originalfiler. Uppdatera schema och backupverifiering för kommentarer och bilder före första skrivbara sådan funktion. Återställ även olänkade listors bilder och kommentarer. Raderings- och sena-sparningar-tester måste omfatta bilder.

### 4. Listans eget frivilliga väder och samlad acceptans

Använd manuellt väder och historisk hämtning när lämpliga data finns för listans plats och besök. Vädret delas av listans fågelrader, är oberoende av journalvädret och kan visas kompakt vid plats och tid. Bevara attribution. Avgränsa tidsmässig representation mot befintligt väderunderlag vid detaljgranskningen, utan att presentera en dagsuppgift som verifierat väder för en exakt tidpunkt.

**Tester och klartkriterium.** Saknat, manuellt och hämtat väder, nätverksfel, korrigerad plats/tid och sena svar. Journalvädret får inte ändras. Utöka schema, validering, innehållsdigest och backup före vädersparande. Gör slutlig återställningskontroll med alla kravfält, länkade och olänkade listor, foton och flera länkar. Automatiska sammanfattningar och statistik kvarstår som uppskjutna.

## Schema, backup och plattformar

Nuvarande app använder schema 4 och strikt innehållsverifiering. [database.rs](../../app/src-tauri/src/database.rs) returnerar `database_schema_upgrade_required` för äldre scheman. Befintlig migration och dess plattformsvillkor utgör ingen färdig uppgradering till fågelschemat. Utred den exakta säkra uppgraderingsvägen som första del av steg 1 och bevara etablerings-, identitets-, lås- och persistensskydd.

Hel SQLite-kopia gör inte nya tabeller automatiskt verifierade. [Restore](../../app/src-tauri/src/backup/restore.rs) kontrollerar uttryckliga format/schema-par. Kandidatbygge, innehållsjämförelse och öppning måste behandla nya data konsekvent. Varje nytt beständigt fält behöver skyddas innan det exponeras. Välj exakt schema- och formatversionsnummer vid implementation efter kontroll av dåvarande main. Undvik framtida tomma tabeller bara för att minska antalet versioner.

Steg 1 börjar med macOS-acceptans. Kontrollera samtidigt Windows/Linux-kompilering och befintlig startup/backup i CI. Ingen plattform får tappa nya data vid öppning eller backup. Håll ny skrivfunktion otillgänglig på plattformar vars migrering och återställningsväg ännu inte är verifierade. Linux Restore/Recovery-begränsningar måste respekteras och behandlas uttryckligen innan fågelfunktionen levereras där.

## Minsta första arbetsorder och macOS DEV-acceptans

Godkänn steg 1 med dess schema- och bevarandekontroller som en vertikal leverans. Börja med en syntetisk databastest som sparar och återställer listan innan UI kopplas på. Om migreringsarbetet visar sig större, gör endast det förarbetet först.

1. Starta macOS DEV med en isolerad syntetisk journal vars sista katalog ännu inte finns. Använd befintlig DEV-konfiguration och `MNO_TEST_DATA_DIR`. Skapa en journalanteckning med datum och plats.
2. Skapa en lista därifrån. Kontrollera föreslagna värden, ändra listans datum eller plats och lägg till arter och antal med svenska och engelska namn. Spara och kontrollera att journalen behåller sina värden.
3. Öppna hela listan från förhandsvisningen. Kontrollera tre första arter, antal och markering för fler rader. Starta om och kontrollera innehållet.
4. Avbryt ett nytt skapande och kontrollera att inget nytt objekt eller länk finns kvar.
5. Skapa extern backup, återställ endast till en separat syntetisk målprofil och kontrollera listor och länkar. Kör samma automatiserade bevarandekontroll för olänkad lista och verifiera att journalradering inte tar bort listan.

Återgång från listan till rätt journalanteckning är fortfarande en öppen produktfråga, särskilt vid flera länkar. Den behöver beslutas inför steg 1:s navigationsacceptans. Reservvisningen för saknade svenska namn behöver godkännas. Ägarens dokumenterade licenstolkning, okänt exportdatum och taxonomiska namnkontroller följer med artregistrets underlag.
