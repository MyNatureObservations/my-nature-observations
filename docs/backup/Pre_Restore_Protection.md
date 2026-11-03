# Obligatorisk skyddskopia före återställning – #9, steg 3A.3a

## Syfte och avgränsning

Innan en framtida återställning ersätter naturboken måste den aktuella **sparade**
journalen ha en verifierad och återställningsbar skyddskopia. Om skapande eller
verifiering misslyckas ska återställningen avbrytas utan ändring av den aktiva databasen.
Osparad redigering ingår inte: den samordnas uttryckligen av frontend i ett senare steg.

Detta steg inför endast ett internt synkront Rust-API. Ingen återställningsmotor,
frontend, dialog, automatisk backup eller gallring införs. Kör API:t på bakgrundstråd.

## Placering, rättigheter och format

Fast plats: `<app-data>/backups/pre-restore/`. App-data härleds från samma
`database::database_path()` som den aktiva journalen. `MNO_TEST_DATA_DIR` gäller därmed
även här i utvecklingsläge. Ingen användarvald backupmapp används och ingen fallback
sker om den fasta platsen är otillgänglig.

Underkatalogerna skapas med privata rättigheter. På Unix är de 0700 och backupfilerna
0600. Befintliga vanliga underkataloger begränsas till 0700 på Unix; Windows kräver
verifierad skyddad ACL för användaren och SYSTEM. Kataloglänkar/junctions avvisas för
att inte följa en omdirigering till externa filer. App-datarotens rättigheter ändras
inte. Detta skyddar inte mot en administratör eller skadlig kod med samma identitet
som byter ut kataloger medan operationen körs.

Den befintliga backupmotorn återanvänds helt. Dess SQLite-snapshot får nu också en
privat stagingkatalog och en förskapad 0600-fil på Unix; förbättringen gäller även
manuell backup. Den färdiga backupfilen skapas av samma NamedTempFile/publiceringsväg
som tidigare, med privat åtkomst i den skyddade målkatalogen. Windows använder
ärvda kontrollerade ACL:er. Format v1, ZIP-poster, manifest och checksumma är oförändrade.

Filnamn: `pre-restore-YYYYMMDD-HHMMSS-<UUID>.mno-backup` med UTC-tid. UUID undviker
kollisioner även inom samma sekund. `kind` är `manual` i det befintliga manifestet.
En fil som ändå upptar namnet skrivs aldrig över, inte heller om den dyker upp
precis före publicering. Det finns ingen automatisk omplacering eller namnretry.

## Skapande och verifiering

1. Anroparen innehar exklusivt `Maintenance` från den aktiva appens `DatabaseState`.
2. Källan öppnas skrivskyddat utan CREATE eller schemainitialisering.
3. SQLite Online Backup API skapar en konsekvent, fristående snapshot av sparade data.
4. Backupmotorn kontrollerar SQLite med quick_check och verifierar det verkliga arkivet,
   manifest, mängd data och SHA-256 som tidigare.
5. Vid motorns befintliga kontrollpunkt **före publicering** körs hela den strikta
   verifieraren från 3A.2: integrity_check, schema, dokument, resursskydd och hash.
6. Efter den strikta verifieringen synkas arkivfilen igen. Synkningsfel avbryter före
   publicering. Samma kompletta arkiv publiceras sedan utan överskrivning.
7. På macOS/Linux synkas katalogerna i ordningen `pre-restore/`, `backups/`, app-dataroten
   och dess föräldrakatalog. Det täcker den slutliga filposten och de nyskapade
   katalogernas poster. Samma kedja synkas även om katalogerna redan fanns, så en
   tidigare misslyckad synkning inte kan kringgås vid nästa försök.
8. En sista synkning av den publicerade filen utförs. Först när samtliga barriärer
   lyckats returneras `VerifiedProtection`, som behåller sökväg, kandidat och lån av
   underhållslåset. Ingen skyddskopia raderas om ett sent synkningsfel uppstår.

Förberedelsens databas använder de privata rättigheter och den kvarhållna lästransaktion
som beskrivs i [Restore_Candidate_Verification.md](Restore_Candidate_Verification.md).
Originaljournalen, dess generation och externa länkade filer ändras inte.

64 MiB per dokument är fortsatt en tillfällig teknisk säkerhetsgräns för återställning
v1, inte ett boktak eller en permanent formatbegränsning. SQLite-raden har också ett
64 MiB-tak. Giltig data som når resursgränsen ger `verification/resource_limit` och
**ingen framgångsrik skyddskopia**. Originaljournalen lämnas kvar.

## Fel, avbrott och bevarande

`ProtectionError` har strukturerade koder för `durability_unsupported`,
`durability_failed` och bland annat otillräckligt utrymme,
rättighetsfel, otillgänglig källa/katalog, namnkollision, backupfel och verifieringsfel.
`Verification(PrepareError)` bevarar den strikta verifierarens särskiljbara felkoder.
Avbrott före start ger `cancelled`; avbrott under verifieringen ger
`verification/cancelled`. Den befintliga snapshotmotorn har 30 sekunders timeout och
är inte omedelbart avbrytbar under kopieringen; flaggan läses av verifieringen före
publicering. En misslyckad publicering städar även en redan förberedd kandidat.

Temporära filer städas via RAII vid vanliga fel och avbrott. Städning är best effort
vid filsystemfel; processkrasch kan lämna privata temporära filer. En ofullständig
backup publiceras aldrig med slutligt namn. Synkningsfel ger `durability_failed` och
aldrig ett skyddskopiebevis. En komplett kopia som redan publicerats lämnas kvar,
även när operationen rapporterar fel. Tidigare kopior påverkas inte.

Namnkonflikter är typade `io::ErrorKind::AlreadyExists` även i backupmotorns tidiga
kontroll och översätts till `already_exists` utan texttolkning. Såväl den yttre
kontrollen som motorns kontroll och den sena no-clobber-publiceringen omfattas.

Färdiga skyddskopior raderas inte av `VerifiedProtection::drop` eller vid senare fel.
De ska alltid bevaras efter både lyckad och misslyckad återställning och **aldrig**
ingå i automatisk gallring. Någon gallringsfunktion finns inte i detta steg. Framtida
gallring måste uttryckligen utesluta `backups/pre-restore/`, även om manifestets kind
är manual. Skyddskopior kan förberedas med samma kandidat-API som andra v1-backuper.
De är okrypterade och innehåller privata journaldata.

## Kontrakt för 3A.3b – inget oskyddat mellanrum

All användarinteraktion och hantering av osparad text ska ske före fabriken
`restore_context::with_restore_context(&app, candidate, cancel, action)`.
Den inkommande kandidaten ska redan vara förberedd. Fabriken införs i 3A.3b.1 och:

- hämtar rätt databas och `DatabaseState` från samma app,
- tar underhållslåset och väntar in pågående operationer,
- kontrollerar generationsutrymme och skapar skyddskopian under samma lås,
- binder bevis till operation, generation, kanonisk väg och filidentitet,
- lämnar ett låsbundet sammanhang till callbacken först efter lyckad verifiering och
  beständig publicering.

Ta inte ett underhållslås före anropet till fabriken och ta inte ett normalt
operationslås inuti callbacken; sådant återinträde skulle kunna låsa operationen.
Vid fel i detta delsteg släpps vakten utan ändring av aktiv data eller generation.
En existerande backupfil kan aldrig ersätta ett lyckat bundet bevis.

Den framtida kopieringsfunktionen ska kräva och konsumera sammanhangets engångs-
auktorisering. Behåll samma underhållsoperation genom ersättning, resultatkontroll,
felhantering och generationsbyte. Sammanhang och bevis måste släppas innan vakten kan
muteras för generationsbyte, men vakten måste fortsätta vara tagen till dess normal
drift kan tillåtas. Den ägande fabrikens förlopp ska utökas för detta i senare delsteg.

Vid osäkert resultat ska kommande kod sätta den separata åtkomstspärren innan vakten
släpps. En beständig markering och kontroll vid uppstart måste också finnas innan
någon aktiv kopiering införs. Vid verifierad framgång ska generationen ändras innan
normal drift, så gamla sparningar avvisas efter att de fått operationslåset.

Använd alltid inkommande kandidatens behållna verifierade anslutning, aldrig en
återöppnad sökväg. Verifiera kopieringsresultatet före godkännande enligt 3A.2:s kontrakt.
Filsystemskontrollerna skyddar inte mot en komprometterad process med samma identitet.
Ingen kopiering till aktiv databas finns ännu. Se [Restore_Context.md](Restore_Context.md).

## Testning och plattformar

Isolerade tester täcker lyckad skyddskopia, plats, privata rättigheter, unika namn,
återställningsbar journal, oförändrad källa, befintlig fil och sen publiceringskollision,
felaktigt schema/dokument, resursgräns, simulerad full disk vid sent fel, korrupt paket,
avbrott, otillgänglig katalog, externa länkar och bevarande av tidigare kopior.
Ett deterministiskt kötest väntar på en faktiskt blockerad operation och visar att
samma underhållslås kan omfatta skyddskopiering och ett senare generationsbyte.

Utökade tester kör verkliga synkningsanrop och kontrollerar arkivets återställningsbara
innehåll före publicering, dess existens vid katalogsynkning och ordningen för hela
katalogkedjan. Fel injiceras vid var och en av de sex obligatoriska barriärerna. Testerna
verifierar felresultat, frånvaro av slutlig fil vid tidigt fel, bevarande av kompletta
publicerade kopior vid sent fel samt oförändrad journal och tidigare kopior. Motorns
tidiga typade namnkonflikt testas separat från den yttre kontrollen och sen publicering.

## Plattformsgarantier och begränsningar

- **macOS:** kataloger öppnas med `O_DIRECTORY | O_NOFOLLOW` och synkas med `fsync`.
  Filbarriärerna begär uttryckligen `F_FULLFSYNC`, oberoende av Rust-versionens
  implementation av `sync_all()`.
  Den sista filbarriären efter katalogsynkning begär även tömning av lagringsenhetens
  cache efter metadataskrivningarna. `F_FULLFSYNC` begärs inte direkt på kataloger,
  eftersom det inte stöds för alla kataloger/filsystem. Nödvändiga fel ignoreras inte.
  Verkliga anrop har körtestats på utvecklingsmaskinens macOS-filsystem.
- **Linux:** samma ordning; fil- och katalogsynkning använder `fsync`. Linux är inte
  körtestat här. Ett filsystem som avvisar katalogsynkning leder till fel, inte fallback.
- **Windows:** obligatorisk skyddskopiering är tills vidare **avstängd** och returnerar
  `durability_unsupported` innan källan öppnas eller backupkataloger skapas. Vanlig
  manuell backup och format v1 är oförändrade. Befintligt `tempfile` använder
  `MoveFileExW` utan `MOVEFILE_WRITE_THROUGH`. Den flaggan ensam ger inte tillräckligt
  underlag för att garantera beständighet för hela kedjan av nyskapade kataloger.
  `FlushFileBuffers` kräver rätt handtag/skrivrättigheter; volymsynkning kräver normalt
  administrativ åtkomst och införs inte. Inga obestyrkta katalogflush-anrop eller nya
  Windows-API:er används för att rapportera framgång. Återaktivering kräver en separat
  underbyggd implementation och Windows-körtester. Tidigare tester av Windows-ACL:er
  för kandidatförberedelse kvarstår; framgångstester för skyddskopia körs bara på de
  plattformar som denna funktion stöder.
- Övriga plattformar avvisas också med `durability_unsupported`.

Garantin gäller att alla föreskrivna OS-barriärer lyckats innan beviset returneras,
under förutsättning att filsystem och lagringsenhet respekterar dem. Den är inte en
absolut garanti mot trasig hårdvara, felaktig firmware, nätverkslagring som ignorerar
flush eller manipulation av en komprometterad process med samma identitet.
Katalogkontrollerna är inte fullständig sandboxning mot samtidiga katalogbyten.

Automatiska tester bevisar inte beteende vid fysiskt strömavbrott. Ett processavslut
är inte samma sak som att förlora OS-/diskcache. Ingen verklig volym fylls för att
testa full disk. Återställningsmotorn och dess avbrottssäkra aktivering hör till 3A.3b.

Källor: [fsync och katalogposter](https://www.man7.org/linux/man-pages/man2/fsync.2.html),
[Apple fsync/F_FULLFSYNC](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html),
[MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw),
[FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers).

## Sammanhang från steg 3A.3b.1

Den tidigare separata app-/underhållsvakt-ingången har ersatts av en kontrollerad
sammanhangsfabrik. Den tar själv låset och binder skyddskopiebevis till operation,
generation, kanonisk databasväg och filidentitet. Ta därför inte ett underhållslås
utanför fabriken och anropa fabriken under detta lås. Se [Restore_Context.md](Restore_Context.md)
för aktuell anropsordning och det processlokala spärrgränssnittet. Ingen återställning
eller beständig återställningsmarkering har införts i detta delsteg.
