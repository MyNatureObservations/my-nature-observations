# Skrivskyddad klassificering – steg 3A.3b.4c.1

Detta är ett internt observations-API. Ingen uppstartskontroll kopplas om, ingen
journalåtkomst låses upp och ingen återställning, rollback eller städning aktiveras.
Backupformat v1 och pending-format v1 ändras inte. Kopieringsmotorn är cfg(test).

## API och operationsbindning

`backup::restore_context::pending::classification::classify(root, cancel)` ger
ett `Assessment` med `status()` (Old, New, Uncertain) och diagnostisk `reason()`.
Alla obligatoriska kontrollfel ger Uncertain, aldrig ett godkännande.

Datakatalogen och dess befintliga .mno-data.lock öppnas utan skapande. Kontrollen
tar det befintliga OS-processlåset exklusivt och icke-blockerande. Aktiv app eller
annan inspektör med samma lås ger Uncertain. Saknad katalog/låsfil skapas inte.
Detta fristående API anropas alltså när ordinarie app inte håller låset. Framtida
uppstartsintegration måste överföra det redan hållna låsets kapabilitet till
kontrollen; den får inte försöka ta ett andra lås eller kringgå processlåset.

Resultatets privata observation behåller processlås, filhandtag, aktiv SQLite-
lästransaktion och verifierad skyddskopiesnapshot. Bindningen innehåller strikt
läst restore_id, katalog-/databasidentitet och SHA-256 för exakta markerbytes.
Status, fingeravtryck eller en boolesk flagga kan inte skapa observationen.
Inga generella SQL-anslutningar eller verifieringsbevis exponeras. Kompilatortest
med positiv kontroll verifierar resultattypens privata konstruktion.

Ett UUID i en osignerad markör kan inte ensamt autentisera en tidigare process.
Operationsbindning betyder här att observationen binds till just det lästa
operations-ID:t, filidentiteterna och markerinnehållet. Metadata ger aldrig skrivrätt.
En framtida bevisövergång måste återkontrollera samma bindning under rätt
underhållsoperation och generation. Ingen sådan övergång eller koppling till
PendingMarker::finish finns ännu. Dess tidigare dokumenterade framtida API-krav
att kräva verifierat Old/New kvarstår.

## Obligatoriska kontroller

- Befintlig strikt markörparser återanvänds: storlek, dubletter, formatversion,
  UUID, hashar och fasta relativa namn. Inlästa godtyckliga målsökvägar accepteras inte.
  Recovery måste innehålla exakt markörfilen; extra eller partiella artefakter ger Uncertain.
- Katalog-, databas-, markör-, lås- och skyddskopiehandtag behålls. Identiteter och
  länkar kontrolleras före och efter arbetet, inklusive backups/pre-restore-kedjan.
- Skyddskopiearkivets SHA-256 kontrolleras. Samma öppnade arkiv går till befintlig
  strikt backup-/databas-/dokumentverifiering genom prepare_open_file. Originalet
  återöppnas inte via namn. Extrahering sker i befintlig privat temporär lagring.
- Skyddskopians verifierade innehållsfingeravtryck måste matcha markerat Old.
- Aktiv databas öppnas descriptorbundet genom en begränsad VFS-adapter (se nedan),
  endast READ_ONLY | NOFOLLOW, utan CREATE och utan URI-parametrarna immutable/nolock. query_only slås på och en lästransaktion behålls.
  VerifiedJournal använder full integrity_check, gemensam schemaverifiering,
  dokument-/resurskontroller och det befintliga deterministiska fingeravtrycket.
- Aktiva databasbytes och skyddskopiearkiv hashas före/efter kontrollen. Ändrade
  indata ger Uncertain. Hashning använder positionella läsningar: inga extra
  databashandtag öppnas/stängs medan SQLite-låset hålls (viktigt för POSIX-lås).

Old kräver att alla kontroller lyckas och aktivt innehåll matchar Old.
New kräver samma kontroller och matchning mot New. Skyddskopian måste i båda
fallen verifiera Old. Om Old och New är identiska väljs Old deterministiskt.
Saknad fil, fel identitet, osäker SQLite-status, ogiltig backup, schema/dokumentfel,
innehållsskillnad, I/O-fel, avbrott eller resursfel ger Uncertain.

## SQLite-recovery och samtidighet

SQLite kan normalt behöva skriva tillbaka sidor vid läsning av en databas med
hot rollbackjournal. En skrivskyddad anslutning kan då ge SQLITE_READONLY_ROLLBACK.
Se [SQLite locking](https://www.sqlite.org/lockingv3.html) och
[Atomic commit](https://www.sqlite.org/atomiccommit.html).

Kontrollen avvisar konservativt ALLA befintliga mno.sqlite-journal, -wal och -shm,
inklusive kalla/tomma filer och länkar. DELETE-header krävs före SQLite-öppning.
Sidofiler återkontrolleras senare. Före första SQLite-läsningen sätts anslutningens
locking_mode=EXCLUSIVE och värdet läses tillbaka. Det är en anslutningsinställning,
inte en ändring av journal_mode eller databasfilen. Detta undviker skapande av en
shared-memory wal-index vid en samtidig WAL-lägesväxling, se
[SQLite WAL utan shared memory](https://www.sqlite.org/wal.html#use_of_wal_without_shared_memory).
Ett särskilt regressionstest växlar till WAL mellan headerkontroll och SQLite-
öppning och verifierar Uncertain utan nya sidofiler. Lås-/recoveryfel i en samtidig SQLite-race ger
Uncertain; kontrollen växlar aldrig till skrivläge. Ingen sidofil raderas.
En faktisk hot journal kan därför inte klassificeras Old/New i detta steg.
Det skulle kräva en separat uttryckligen godkänd SQLite-recoveryfas.

Kontrollen skriver inte journalen, markören eller skyddskopian. Den verifierade
backupen extraheras däremot till privat temporär lagring, som städas vid drop/fel.
Skrivskyddad betyder alltså oförändrade källfiler, inte att inga temporära filer
skapas. Filsystemets normala läsåtkomsttider kan ändras.

En kvarvarande godkänd observation håller en SQLite-läsreservation som hindrar
en annan SQLite-anslutning från att committa i DELETE-läge. En extern skrivare
kan fortfarande påbörja en transaktion; statusen avser den behållna snapshoten,
inte ett generellt löfte om framtida filstatus. Framtida konsumtion kräver ny
bindningskontroll. Efter drop släpps reservation och processlås utan upplåsning av
DatabaseState. SQLite stängs före andra handtag till samma inode. Observationen
ska inte behållas medan framtida frontend väntar på ett användarbeslut.

## Tester och begränsningar

Isolerade tester täcker Old/New, felaktigt innehåll, saknade filer, identiteter,
SHA-256, fingeravtryck, schema, korrupt databas/arkiv, ogiltig markör och symlänkar.
De jämför källfilernas bytes före/efter, testar extern skrivare och synkroniserade
ändringar under kontrollen samt en verklig hot journal efter dödad barnprocess.
Det senare verifierar också SQLite READONLY_ROLLBACK utan att utföra recovery.

macOS är körtestat. Linux och Windows klassificering ger nu Uncertain/Unsupported
innan filer öppnas: motsvarande fysiska bindning är ännu inte godkänd eller körtestad.
Detta är en avsiktlig skärpning från tidigare, otillräcklig sökvägsbindning på Linux. Befintlig Windows-blockering för obligatorisk skyddskopiering kvarstår.
Klassificeringen gör flera strömmande fulla verifierings-/hashpass och kan ta tid
för stora naturböcker; inga nya godtyckliga bokstorleksgränser införs. Befintliga
tekniska verifieringsgränser (bland annat 64 MiB per dokument) gäller fortsatt.
Processlåset samordnar MNO-versioner som följer protokollet. Detta är inte en
sandbox mot skadlig kod med samma OS-rättigheter som manipulerar råa databasbytes.
Faktiskt strömavbrott och alla filsystems låssemantik bevisas inte av tester.


## Korrigering: bindning till SQLite:s faktiska huvudfil

Grundfelet var att ett behållet OS-handtag verifierades medan SQLite separat
öppnade mno.sqlite via namn. Ett tillfälligt byte till en Old/New-fil, följt av
återställning av namnet, kunde få slutkontrollerna att godkänna fel snapshot.

På macOS finns nu BoundSqlite, en privat ägare med Connection följd av en
registreringsvakt. En unik, icke-default VFS registreras endast för denna
anslutning; processens standard-VFS ändras inte. Adaptern konstruerar SQLite:s
publika sqlite3_vfs fältvis utan att läsa native.pNext och delegerar fil-I/O/låsning till bundled SQLite:s
native unix-VFS. Ingen privat unixFile-layout eller osäker fd-offset används.

Vid xOpen för huvudfilen ersätts själva OS-öppningen med /dev/fd/N, där N ägs av
exakt det behållna READ_ONLY-handtaget. På macOS beskriver fd(4) denna öppning som
en descriptor-duplicering av samma öppna filobjekt, inte ny namnupplösning av
mno.sqlite. Handtaget ägs via Arc<File>, utan extra öppning/stängning av aktiva
inode-handtag under verifieringen. SQLite:s pager behåller journalens riktiga
namn för rollbackkontroller. VFS-adaptern avvisar WAL-öppning, skrivande öppning
av beständiga journaler och radering av huvudfilen eller dess sidofilnamn.
Privata SQLite-temporärfiler tillåts fortfarande enligt befintlig verifiering.

SQLITE_FCNTL_HAS_MOVED används direkt på den öppnade native-filen före xOpen
kvitteras. Här jämför VFS sin faktiska fil med descriptoraliaset, som hålls vid
liv av det ursprungliga OS-handtaget. Initial bindning följer alltså av descriptor-
öppningen; HAS_MOVED används INTE som enda bevis för att rätt namn öppnades.
SQLITE_FCNTL_FILE_POINTER måste därefter visa att main-anslutningen använder just
filobjektet från denna xOpen, och HAS_MOVED måste fortsatt lyckas utan filbyte.
Kontrollerna körs före innehållsläsning, efter verifiering och vid slutkontroller.
Saknat stöd, avvikande returvärde eller kontrollfel ger Uncertain utan fallback.
Vanliga namn-/katalogidentiteter, hashar och sidofiler kontrolleras fortfarande.

Assessment äger nu också VFS-vakten och dess ursprungliga handtag. SQLite stängs
före avregistrering och frigörande av filnamn/handtag, även på felvägar. Inga fria
statusvärden, SQL-anslutningar eller genvägar till PendingMarker::finish exponeras.
Detta bevisar filobjektet för den behållna snapshoten; det autentiserar fortfarande
inte osignerad metadata eller skyddar mot skadlig kod som manipulerar processens
minne/descriptorer. Normal osäkerhet och oupptäckbart plattformsstöd godkänns aldrig.

Regressionstester täcker den reproducerade Old- och New-sekvensen, ett separat
bevis att native SQLite läser det behållna originalet trots ersatt sökväg,
byten/saknad fil/symlänkar vid varje kontrollgräns samt fel eller saknat stöd vid
samtliga native-identitetskontroller. Befintliga tester för samtidiga skrivare,
read-only/hot journal, WAL-race och resultattypens kompilatorskydd kvarstår.

Referenser: macOS SDK:s fd(4), SQLite:s publika
[VFS-kontrakt](https://www.sqlite.org/c3ref/vfs.html),
[filkontroller](https://www.sqlite.org/c3ref/c_fcntl_begin_atomic_write.html) och
[sqlite3_create_filename](https://www.sqlite.org/c3ref/create_filename.html).


## VFS-registrets samtidighets- och ägarskapskontrakt

SQLite ändrar endast pNext i en registrerad VFS och skyddar registret med sin
interna statiska mutex. Adaptern läser därför aldrig native.pNext eller kopierar
hela native-strukturen. Övriga publika fält läses individuellt; native unix-VFS
har statisk livstid i bundled SQLite och dess övriga fält förblir oförändrade.

Varje registrering äger en separat Box<RegisteredVfs>, vars första fält är
UnsafeCell<sqlite3_vfs>. SQLite får en råpekare härledd med ptr::from_ref från
hela RegisteredVfs-allokeringen, därefter castad till dess sqlite3_vfs-prefix.
Ingen fältbegränsad pekare från UnsafeCell::get utvidgas. repr(C), UnsafeCells
layoutgaranti och en kompileringstidskontroll säkerställer prefixets offset noll.
Callbackarnas återkonvertering behåller därmed proveniens även för context-fältet.
Ingen Rust-referens till den registrerade sqlite3_vfs-strukturen skapas. Callbackarna
läser enbart den oföränderliga Context-pekaren utanför cellen och skapar sedan
referenser till en separat Box<Context>. Dessa referenser omfattar aldrig
SQLite-muterat minne. Native pAppData bevaras för de delegerade callbackarna.

Registreringarna har unika namn och är aldrig standard-VFS. SQLite:s befintliga
registermutex räcker; inget eget globalt lås eller unsafe Send/Sync införs.
Connection stängs före avregistrering. Context, filnamn och descriptorer frigörs
först efter lyckad avregistrering. Ett oväntat avregistreringsfel behåller samtliga
refererade allokeringar och descriptorer för processens återstående livstid i
stället för att riskera dangling pointers. Det är en konservativ resursläcka,
inte en ny auktorisering eller skrivväg. Registreringsfel ger inget godkännande.

Tester injicerar FILE_POINTER-felkod, saknat stöd, nullpekare och främmande
filpekare; samtliga ger Uncertain. HAS_MOVED-testerna kvarstår. Åtta trådar
registrerar egna adaptrar med barriärstyrda överlappande livstider och kontrollerar
läsning, identitet, unika namn och avregistrering. Separata feltester kontrollerar
städning efter registreringsfel och att callbacktillståndet bevaras om
avregistrering misslyckas. Dessa tester ersätter inte aliasing-/livstidsgranskning
eller bevisar frånvaro av datarace; faktisk ThreadSanitizer-körning har inte gjorts.
