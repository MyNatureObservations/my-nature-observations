# Säker uppstartsgrund – #9, steg 3A.3b.4b.1

## Omfattning

Detta steg inför en fail-closed uppstart, ett OS-hanterat processlås per datakatalog
samt en begränsad kontroll för reserverade återställningsartefakter. Ingen markörparser,
Old/New/Uncertain, markörpublicering, återställningsskrivning eller frontend införs.
SQLite-kopieringen är fortsatt helt cfg(test).

## Sökvägar och uppstartsordning

1. DatabaseState::default() är spärrat: startup-fältets OnceLock är tom.
2. database::resolve_data_directory(app) bestämmer enbart datakatalogen. I debug
   gäller fortfarande MNO_TEST_DATA_DIR om den är satt; release ignorerar den.
   Funktionen öppnar/skapar inga filer. expected_database_path(directory) är också ren.
3. DatabaseState::initialize_startup(directory) tillåter exakt ett försök per State.
4. startup::initialize() skapar vid behov datakatalogen separat från SQLite och
   tar dess processlås. Befintlig slutlig datakatalog får inte vara symlink/reparse point.
   Den valda katalogen kanoniseras; befintliga föräldraalias löses till sin faktiska plats.
5. En separat checkpoint kontrollerar recovery i den låsta katalogen, utan databasöppning.
6. Katalogens och låsfilens identitet återkontrolleras. Databasens slutliga sökväg
   får inte vara symlink/reparse point, katalog eller annan icke-reguljär fil.
7. Initialiseringsmarkeringen .mno-initialized-v1.json kontrolleras. Korrupt,
   okänd eller oläsbar markering blockerar. Saknad databas tillsammans med markering,
   tidigare låsfil eller andra katalogartefakter ger database_missing_established.
8. Saknad markering publiceras beständigt FÖRE databasskapande. Befintlig markering
   får också sina beständighetsbarriärer bekräftade, eftersom en tidigare publicering
   kan ha misslyckats efter att slutnamnet blev synligt.
9. Endast en datakatalog som skapats exklusivt av det aktuella uppstartsförsöket,
   med ny låsfil och utan andra artefakter, får skapa databasen. En redan befintlig
   tom katalog utan journal betraktas som osäker och blockeras också.
   create_new_database reserverar filnamnet med create_new och öppnar sedan SQLite
   med READ_WRITE utan CREATE. Befintlig databas öppnas alltid med READ_WRITE utan
   CREATE genom initialize_database. En befintlig databas valideras utan schemaskrivningar med samma schemakontroll
   som återställningsverifieringen: user_version=0 och exakt nuvarande tabell,
   kolumner, constraints, primärnyckelns autoindex och datumindex. Extra schemaobjekt
   eller saknade/avvikande index avvisas. Ingen automatisk reparation görs.
10. En ny databas synkroniseras tillsammans med katalogkedjan innan uppstart godkänns.
   Inga nya migrationer eller ändringar av journalformatet införs.
11. En privat konstruerad StartupReady med det behållna låset publiceras i State.
12. setup installerar DataLocation för den godkända databasplatsen och återgår.

Ordningen finns i lib.rs före dagens schemaanrop. Vid fel returnerar Tauri setup
ett fel och normal appstart avbryts; inga nya feldialoger införs. State förblir
spärrat efter misslyckad uppstart och erbjuder ingen generell upplåsningsfunktion.

Normala operationer använder den vid uppstart bundna DataLocation, inte en ny
miljövariabelläsning eller ny katalogskapning. open_database använder READ_WRITE
utan CREATE: en databas som försvinner efter uppstart ersätts inte tyst med en tom fil.
Appens data öppnas inte av utvecklingstesterna.

## Processlåset

Den reserverade låsfilen är .mno-data.lock i datakatalogen. fs2::FileExt använder
flock på macOS/Linux och LockFileEx på Windows. Ett icke-blockerande exklusivt lås
tas på ett öppet filhandtag, inte genom att tolka filens existens eller en sparad PID.

StartupReady ägs av appens DatabaseState under hela dess livstid och håller både
låsfilen och ett kataloghandtag öppna. Normala operationer och underhåll kontrollerar
att katalog och låsfil fortfarande motsvarar dessa handtag innan åtkomst beviljas.
Unix använder device/inode, Windows volym-/filindex.

Låsfilen tas aldrig bort, inte heller vid avslut eller fel. Drop låser uttryckligen
upp före stängning. Vid processkrasch släpper operativsystemet låset när handtagen
stängs; kvarvarande låsfil är normal och behöver ingen manuell städning. Unixhandtag
är close-on-exec. Den uttryckliga upplåsningen undviker också kortvarigt ärvda flock-
referenser under en parallell processstart före exec.

Unix öppnar låsfilen med O_NOFOLLOW och O_NONBLOCK och kräver reguljär fil,
aktuell ägare och endast en hårdlänk. Nya kataloger och låsfiler skapas med högst
0700 respektive 0600. Windows öppnar reparse point själv för kontroll och avvisar
sådana objekt; kataloghandtaget använder BACKUP_SEMANTICS. Windowshandtagen delar
läsning/skrivning men inte delete, så det öppna objektet inte normalt kan ersättas.
Fel vid öppning, identitetskontroll eller låsning innebär nekad uppstart.

Referenser: fs2 0.4.3:s bundlade källkod unix.rs/windows.rs och
[Microsoft LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex).

## Uppstartsspärr och övergångskontroll

DatabaseState::operation() och maintenance() kräver StartupReady. Därmed nekas
journalens läsningar, sessionsskapande, sparningar och manuell backup innan normal
uppstart godkänts. Återhämtningsflaggan och generationskontrollen finns kvar.
Ingen generell clear/unlock-metod exponeras. Ett felat startupförsök kan inte
återanvändas för att godkänna en annan katalog i samma process.

Checkpointens avsiktligt konservativa policy:

- Endast NotFound för den reserverade platsen recovery, med kontrollerad befintlig
  låst föräldrakatalog, godtas som frånvaro.
- ALLT befintligt där blockerar: vanlig fil, tom katalog, temporär fil under en
  recovery-katalog, okänt innehåll, symlink eller dangling symlink.
- PermissionDenied, I/O-fel och andra kontrollfel är fel, aldrig frånvaro.
- Inga markörartefakter läses, tolkas eller raderas i detta steg.

Detta gäller även på Windows. En framtida parser måste ersätta checkpointen under
samma uppstartsspärr; det räcker inte att införa en parser vid en senare tidpunkt.

## Tester och kontroller

Isolerade tester täcker ren sökvägsberäkning, ny och befintlig databas, bevarad
journalfil, markeringsartefakter, nekad metadataåtkomst, uteblivet initialiseringsanrop,
misslyckad initialisering, engångsgodkännande, symlänkar och utbytt låsfil.
Behörighetsfelet injiceras som ett riktigt io::ErrorKind::PermissionDenied vid
checkpointens metadataanrop; därmed är testet oberoende av testanvändarens privilegier.

En child-process kvitterar låstagning via stdout innan föräldern provar en andra
uppstart. Handshake, inte en tidsbaserad gissning, bekräftar att låset är taget.
Föräldern testar både vanlig frigöring och kill/wait följt av ny uppstart.
Journal-/backupregressioner verifierar att databasöppnaren inte ens anropas vid spärr.

Befintliga koordineringstester använder en uttrycklig ready_for_test()-fixture.
Den och StartupReady::for_test() finns endast i cfg(test). Produktions-Default
förblir alltid spärrat.

Kvalitetskontroller: Rust-tester, frontendtester, npm check/build, release-check,
Clippy och formatering av berörda filer samt git diff --check. Befintliga Clippy-
undantag är collapsible_if samt dead_code och drop_non_drop för testkod.

## Begränsningar och nästa steg

macOS har körtestats. Linux använder motsvarande granskade flock-kod; Windows
LockFileEx/reparse/filidentitetskod kräver egna körtester. Lås-/åtkomstfel nekas,
men ingen garanti ges för alla nätverksfilsystems låssemantik eller en trasig VFS.
Låset är ett samordningsprotokoll för MNO-versioner som respekterar det. Äldre
versioner och externa SQLite-program är inte förhindrade från att skriva databasen.

Detta är inte en sandbox mot skadlig kod med samma OS-rättigheter som flyttar
föräldrakataloger, byter filer mellan kontroller eller manipulerar råa databasbytes.
Slutliga symlänkar avvisas och bindningar återkontrolleras; inga externa originalfiler
ändras eller tas bort av uppstartsstödet.

Nästa steg får införa format/parser, beständig markörpublicering och senare
uppstartsklassificering. Databasåterställning får fortfarande inte kopplas in utan
dessa delar, skyddskopieauktorisering och hela det godkända säkerhetskontraktet.

## Initialiseringsmarkering – korrigering efter säkerhetsgranskning

Det fanns tidigare ingen tillförlitlig, beständig etableringsindikator. Låsfilen
är ett låsobjekt, inte ett bevis på lyckad initialisering. Dess tidigare existens
används dock konservativt för att INTE tolka en avbruten start som nyinstallation.
Även backups och andra befintliga katalogartefakter blockerar nyskapande vid saknad
journal. En äldre installation vars databas finns kvar migreras till protokollet;
originaldatabasen öppnas utan CREATE och befintliga data bevaras.

Den separata .mno-initialized-v1.json är oföränderlig och innehåller exakt UTF-8:

```json
{"format":"mno-initialized","version":1}
```

Den kanoniska filen avslutas med en LF-byte. Läsningen är längdbegränsad och kräver
exakt dessa bytes. Den innehåller ingen journaldata, inget skyddskopiebevis och
ingen återställningsauktorisering. Den betyder att katalogen tagits i anspråk av
initialiseringsprotokollet, inte att varje initialiseringssteg säkert slutförts.
Den är inte pending-restore.json, raderas aldrig automatiskt och ersätts inte.

Publiceringen använder privat NamedTempFile i samma katalog, flush, filsynkning,
persist_noclobber, fsync av den kanoniska katalogkedjan och en sista filbarriär.
macOS kräver F_FULLFSYNC på filen; Linux använder sync_all/fsync. Ett fel före
publicering städar endast den egna temporärfilen. Ett sent fel bevarar slutfilen.
Nästa försök bekräftar synkbarriärerna igen; en synlig markering antas inte automatiskt
vara ett kvitto på tidigare lyckad beständig publicering.

Kraschfönster:

| Avbrott | Nästa uppstart |
| --- | --- |
| Före publicering, kvarvarande lås-/temporärartefakt, ingen DB | Blockeras konservativt; ingen ny tom journal |
| Efter markering men före DB-skapande | Saknad etablerad journal; blockeras |
| Efter filreservation men före fungerande schema | Ofullständig databas; blockeras |
| Befintlig fungerande journal utan markering | Beständig markering, sedan öppning utan CREATE |
| Efter framgång, journal borttagen/flyttad | Blockeras; ingen ersättningsfil |
| Tidigare markörpublicering gav sent synkfel | Markeringen bevaras; nya barriärer krävs före godkännande |

Regressionssviten täcker dessa fönster med kontrollerade injicerade fel, korrupt/
okänd/oläsbar markering, namnkonflikter, DB som försvinner före öppning samt recovery
samtidigt med saknad journal. Ett sammanhängande test utför riktig startup, den
ordinarie generationskontrollerade sparvägen och manuell backup, verifierar innehållet
i backupen och startar sedan om mot samma datakatalog.

### Beständighets- och plattformsgränser

macOS är körtestat. Linux kräver egna körtester. Windows och andra plattformar
utan godkänd katalogbarriär nekas nu normal uppstart med
**database_initialization_durability_unsupported**, även om en giltig markering
redan finns. Det är en uttrycklig fail-closed begränsning i detta delsteg; processlåset
ersätter inte beständig publicering och någon osäker Windows-fallback införs inte.
Recovery-artefakter kontrolleras fortfarande först även där. Windows behöver en
separat godkänd beständighetslösning innan normal uppstart kan aktiveras igen.

Faktiskt strömavbrott har inte testats och lagringsmaskinvarans beteende kan inte
bevisas av dessa tester. En avbruten första installation kan förbli spärrad även
om ingen användardata någonsin sparats; ingen automatisk reset införs i detta steg.
För en redan existerande tom katalog saknas bevis för nyinstallation, så den
blockeras. Om däremot HELA datakatalogen med alla metadata har raderats kan platsen
inte skiljas från en aldrig använd plats. Protokollet ger inte skydd mot sådan radering,
extern manipulation med samma OS-rättigheter eller förlust av hela datakatalogen.

### Schemavalidering vid befintlig databas

`database::validate_schema` delas av uppstart och strikt återställningsverifiering.
Den jämför normaliserade schemaobjekt med en separat referensdatabas i minnet,
byggd med MNO:s verkliga schemadefinition. Endast blanksteg normaliseras; detta är
samma konservativa kompatibilitetsregel som tidigare använts vid återställning.
Befintlig databas kör aldrig CREATE TABLE/INDEX vid uppstart. Endast nya databaser
använder initialize_schema. Avvikelse ger database_schema_incompatible; SQL-/I/O-fel
ger database_schema_validation_failed. Uppstartsspärren behålls i båda fallen.
Ingen journal- eller dokumentvalidering tillkommer. En etableringsmarkering kan
publiceras innan schemafelet upptäcks, men själva databasfilen repareras eller
ändras inte. Saknat datumindex efter avbruten initialisering blockerar nu också.
