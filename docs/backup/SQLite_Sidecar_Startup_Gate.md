# Konservativ sidofilsspärr vid startup – 4c.2c.4b

## Policy

Normal uppstart blockeras om något av följande namn finns vid MNO:s aktiva
`mno.sqlite`, även om objektet är tomt:

- mno.sqlite-journal
- mno.sqlite-wal
- mno.sqlite-shm

Namnen härleds från database::expected_database_path, inte från ett alternativt
journalfilnamn. Kontrollen använder symlink_metadata och följer inte den sista
sökvägskomponentens symlänkar. Alla befintliga objekttyper blockerar: filer,
kataloger, symlänkar (även brutna), reparse points och andra typer. Metadatafel
är aldrig bevis för frånvaro; endast NotFound godtas som frånvaro, under
DirectoryLease-kontrollerna.

MNO försöker inte avgöra om en rollback-journal är hot genom att öppna SQLite.
Ingen automatisk SQLite-recovery, rollback, schemareparation eller filradering
utförs. En legitim krasch eller ett strömavbrott kan därför lämna sidofiler som
hindrar nästa uppstart. Det betyder **inte** att journaldata är förlorade eller
korrupta. En separat kontrollerad recoveryfas behövs i framtiden. Detta steg
inför inte den fasen och rekommenderar inte att användaren själv raderar filerna.

## Exakt kontrollpunkt och öppningsinventering

Startup::begin_with håller befintlig DirectoryLease genom hela förloppet:

1. Ta processlåset och verifiera katalog-/låsidentitet.
2. Behåll tidigare recovery/P/Q-kontroll. Om recovery-katalog eller Q finns,
   returnera befintlig Recovery-spärr före någon SQLite-open.
3. Kontrollera sidofiler innan etableringsmarkeringen läses/publiceras.
4. Kör tidigare kontroller av etableringsmarkering och databasens existens.
5. Bekräfta/publicera etableringsmarkeringen enligt befintligt protokoll.
6. Verifiera DirectoryLease, upprepa sidofilskontrollen direkt före initialiseraren
   och verifiera DirectoryLease igen.
7. Först därefter tillåts database::initialize_database (befintlig journal,
   READ_WRITE utan CREATE) eller database::create_new_database (endast bevisad
   nyinstallation, exklusiv reservation följd av READ_WRITE utan CREATE).

DirectoryLease och etableringsfunktionerna använder filsystemsanrop, inte SQLite.
Ingen förberedande journalanslutning öppnas före kontrollerna. Referensschemats
in-memory-databas skapas först inne i schemaverifieringen efter tillåten öppning.

Journalens vanliga skrivbara anslutningar går genom database::open_database,
efter DatabaseState::operation och godkänd startup. Manuell backup kräver också
ett operationstillstånd och öppnar skrivskyddat. RestoreContext kräver Maintenance;
klassificeringen är separat descriptorbunden och avvisar redan sidofiler.
Kopieringsmotorn är fortsatt cfg(test). Ingen produktionsrestore aktiveras här.

Detta är en **startup-policy**. Kontrollen läggs inte framför varje SQL-operation
under normal drift: en legitim pågående MNO-transaktion kan då ha en journalfil.
Vanlig SQLite-låsning och befintlig samordning behålls under normal drift.

## Fel och ägarskap

- Upptäckt objekt: database_sqlite_sidecar_present.
- Metadata-/I/O-fel: database_sqlite_sidecar_check_failed med felorsak.
- Recovery/P/Q har fortsatt prioritet med database_recovery_artifact_present.

Sidofilsblockering lagras i befintlig RecoveryGate tillsammans med DirectoryLease.
DatabaseState behåller samma låsägare och Recovery-fas, och returnerar den specifika
felorsaken. Normala journaloperationer, backup och Maintenance är spärrade.
Generation ändras inte. Att släppa en eventuell lånad recovery-session ger inte
Ready. Ingen automatisk klassificering eller städning körs. Processlåset frigörs
först när dess ägande DatabaseState förstörs, exempelvis när appen avslutas.
Övriga startupfel behåller tidigare felkontrakt.

Vid sidofiler skapas ingen ersättningsdatabas, även om etablerad journal saknas.
Tidigt upptäckta sidofiler medför ingen publicering av etableringsmarkering.
Om en extern sidofil tillkommer först mellan första och andra kontrollen kan
markeringen redan ha publicerats, men SQLite-open/ny DB förhindras. En senare
start hanteras då konservativt av det befintliga etableringsprotokollet.

## TOCTOU och plattformar

Kontrollen och SQLite-open är inte en atomisk filsystemsoperation. Den andra
kontrollen minskar tidsluckan efter metadata-/synkroniseringsarbete och tester
visar att synliga sena sidofiler och I/O-fel blockerar. DirectoryLease samordnar
MNO-processer; identitetskontroller avvisar upptäckta katalog-/låsbyten.

Ingen garanti läggs till mot en godtycklig extern process med samma OS-rättigheter
som byter filer eller skapar en sidofil efter sista kontrollen. Att eliminera
sådan manipulation kräver en annan hotmodell/OS-isolering. Ingen generell
filsystemssandbox eller ny VFS införs här.

Kontrollen är plattformsoberoende och recovery-artefakter ignoreras inte på
Linux/Windows. Verifierad recovery förblir Unsupported där. macOS körtestas;
Linux/Windows är inte körtestade i detta steg. Tidigare Windows-begränsning för
beständig första initialisering kvarstår.

## Tester

Isolerade tester täcker första och etablerad startup, samtliga tre sidofilsnamn,
tomma filer, symlänkar, kataloger, injicerade metadatafel i båda kontrollerna,
kombinationer med Q/recovery, saknad etablerad DB, oväntade sidofiler vid
nyinstallation och ett riktigt separat MNO-processförsök mot behållet lås.
Initialiserarens testgräns visar att ingen startup-SQLite-open körs vid blockering.

En subprocess avbryts efter kontrollerad cache-spill i en SQLite-transaktion.
En faktisk hot rollback-journal verifieras via sina bytes; startup lämnar både
journal och databas byteidentiska och öppnar aldrig SQLite för recovery.
DatabaseState-test kontrollerar Recovery-fas, oförändrad generation och spärrade
operationer/Maintenance. Fysisk strömavbrottstålighet bevisas inte av testerna.
