# Isolerad SQLite-kopiering – #9, steg 3A.3b.3

## Avgränsning och intern test-API

Hela `backup::isolated_copy` är kompilerad endast med `#[cfg(test)]`.
Den saknas i både utvecklingsappen och produktionsappen, har inget Tauri-kommando
eller frontendanrop och är inte kopplad till `with_restore_context()`.

`copy(&VerifiedJournal, &mut TestDestination, cancel, hook) -> Outcome` arbetar
synkront i steg om en sida. TestDestination har privata fält och dess enda
konstruktor skapar en egen TempDir och en ny testdatabas. Den accepterar ingen
målsökväg. Efter skapandet öppnas målet med READ_WRITE | PRIVATE_CACHE, utan CREATE.
Endast uttryckligen skapade, temporära testdatabaser används i detta steg.
TestDestination deklarerar Connection före TempDir, så SQLite stängs innan
katalogstädningen börjar. Samma fältordning gäller vid fel och panic-unwind.

Källan lånas från en VerifiedJournal-token. Dess behållna anslutning och verifierade
lästransaktion används; ingen källsökväg öppnas. För PreparedRestore bevaras dess
read-only-anslutning, authorizer, filkontroller och ursprungliga snapshot.

Komponenten erbjuder inget produktionskontrakt och inget skyddskopiebevis.
Detta är en isolerad testbyggsten, inte en möjlig ingång för återställning av appdata.

## SQLite-API och livstid

rusqlite 0.40.2 med bundled/backup/hooks/limits återanvänds. Dess Backup-wrapper
ignorerar resultatet från sqlite3_backup_finish i Drop. Därför används en liten
RAII-wrapper över samma rusqlite::ffi: init, step(1), remaining och finish.
Inga nya beroenden införs. Den explicita finish returnerar SQLite-resultatet;
Drop är en reservväg som stänger handtaget vid panic/unwind.

Det opaka CopyHandle ligger i en privat undermodul till journal_content.
VerifiedJournal::start_copy() lämnar endast detta handtag med step, remaining och
finish; varken SQL-anslutning eller rå SQLite-pekare exponeras. Handtaget lånar
VerifiedJournal och låser därmed dess livstid genom hela kopieringen.
Källans ursprungliga exklusiva Rust-lån gäller fortsatt under efterkontrollen.
Den tidigare copy_source()-åtkomsten är borttagen.

En CopyDestination-kapabilitet krävs för målet. Den kan bara konstrueras inuti
isolated_copy från den egna testdatabasens anslutning. start_copy accepterar alltså
inte en godtycklig målanslutning från andra interna moduler.

Wrappern lånar källsnapshoten och målets anslutning exklusivt. Ingen API-funktion
anropas på målanslutningen mellan init och finish. Krokarna får bara steginformation,
inte anslutning eller backuphandtag. Handtaget avslutas exakt en gång, även vid fel.

Källans lästransaktion håller samma snapshot genom alla steg. Målet får inte ha
någon aktiv transaktion. SQLite äger destinationens skrivtransaktion för kopieringen.
Efter avslutning verifieras målet med samma strikta verifierare som i 3A.2/3b.2 och
jämförs exakt med källan. Avbrott efter commit hoppar inte över denna kontroll.

Referens: [SQLite Online Backup API](https://www.sqlite.org/c3ref/backup_finish.html).

## Mode och beständighet

Både källa och mål måste använda journal_mode=DELETE. WAL och andra lägen avvisas;
inget journal_mode ändras automatiskt. För en PreparedRestore är DELETE-formatet
redan verifierat innan öppning och dess authorizer förbjuder PRAGMA; det kontraktet
återanvänds i stället för att kringgå authorizern.

Målet får synchronous=EXTRA (3). På macOS sätts fullfsync=ON. Båda läses tillbaka
och måste ha förväntat värde innan kopiering. busy_timeout sätts till noll: BUSY
eller LOCKED ger ett resultat direkt, utan obegränsade återförsök eller vänteloop.

EXTRA kompletterar DELETE-lägets synkning med katalogsynkning efter borttagning av
rollbackjournalen. fullfsync begär macOS starkare synkbarriär via SQLite. Att en
PRAGMA accepterats är inte bevis för att maskinvara eller filsystem överlever varje
strömavbrott. Faktisk strömavbrottsbeständighet är inte testad. SQLite-journalfiler
raderas aldrig manuellt. Inget manuellt filbyte eller automatisk återställning görs.

Referens: [SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous)
och [fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync).

## Commitgräns och utfall

Commitgränsen är när sqlite3_backup_step returnerar SQLITE_DONE: destinationens
skrivtransaktion har då slutförts. Det är inte finish-anropet eller progressens
remaining-värde som definierar commit.

- `Unchanged(reason)`: operationen avvisades innan kopiering, eller ett avbrott/
  låsfel före DONE följdes av lyckad finish, som rullade tillbaka transaktionen.
- `Completed { cancel_requested }`: DONE, lyckad finish och strikt verifiering med
  exakt innehållslikhet. Avbrott efter DONE visas genom cancel_requested=true;
  det redovisas aldrig som om destinationen vore oförändrad.
- `Uncertain(reason)`: nödvändig avslutning eller efterkontroll misslyckades, eller
  ett allvarligt SQLite-fel inträffade. Ingen garanti om oförändrad destination ges.

Den bundlade SQLite-versionen kan returnera BUSY även från finish efter ett
BUSY-steg. Ett sådant resultat klassas konservativt som Uncertain(Busy), även om
regressionstestet kontrollerar att gamla målrader bevarats. Övriga avslutningsfel
bevarar tidigare felorsak om sådan finns, annars Finish(returncode).

Felorsaker omfattar avbrott, Busy, Locked, I/O, Full, saknat mål, fel mode,
aktiv måltransaktion, inställningsfel, ogiltig/förlorad källa och verifieringsfel.
Okända SQLite-koder bevaras. WAL-källa avvisas som Source, WAL-mål som Mode.
Ett avbrott som SQLite:s progress-handler orsakar under compare/fingerprint
klassificeras nu som Cancelled när flaggan är satt, även om SQL-anropet misslyckas.
Uttömd VM-budget utan avbrottsflagga förblir ResourceLimit.

## Tester och begränsningar

Isolerade tester täcker exakt kopiering, gamla målrader, oförändrad källa,
stängning/återöppning, kandidat utan originalarkiv, avbrott före/mitt i/nära sista
sidan och efter commit, rollbackjournalens källås, faktiskt BUSY, saknat mål,
WAL, aktiv transaktion, nekade PRAGMA-inställningar och ogiltigt dokument.

SQLITE_FULL framkallas med SQLite:s riktiga max_page_count, inte genom att fylla
värddatorns disk. Avslutningsfel injiceras efter att det riktiga handtaget avslutats;
testet visar att även komplett måldata då ger Uncertain. Ett separat test framkallar
fel i efterkontrollen efter DONE. Panic under partiell kopiering provar RAII-rollback.
Avbrott mitt under hashning och jämförelse framkallas med SQLite:s progress-handler,
med deterministiskt antal VM-anrop utan tidsbaserade gissningar.

Ett kompilatorbaserat kontraktstest kompilerar de faktiska journal_content- och
copy_handle-filerna i en temporär testharness, med en positiv kontroll först.
Orelaterad arkivförberedelse och målkapabilitetens implementation ersätts av
typstubbar; de verifierade API:erna kopieras från den aktuella källkoden.
Testet kräver sedan specifika kompilatorfel för den borttagna copy_source-metoden,
privat anslutningsfält, SQL-anrop på CopyHandle och återanvändning av den ursprungliga
anslutningen medan snapshoten lever. Det behöver rustc och byggda rlib-beroenden.
Ett runtime-test försöker committa en extern ändring under snapshotens livstid och
visar att både kopia och jämförelse använder ursprungligt innehåll. Städning efter
lyckat och avbrutet flöde kontrolleras genom att testkatalogen försvinner.

LOCKED och utökade IOERR-koder testas i klassificeringen. En riktig LOCKED-källkonflikt
är inte konstruerad: verifierad källa har redan lästransaktion, saknar skrivväg och
målet öppnas med PRIVATE_CACHE. Verkliga VFS-I/O-fel, trasig synkning, OS-diskfull och
strömavbrott simuleras inte av dessa tester. Felinjektion bevisar klassificering,
inte hur ett verkligt trasigt filsystem beter sig. Kod har körtestats på macOS;
Linux och Windows behöver egna körtester. Windows spärr för obligatorisk skyddskopia
kringgås inte av denna testmodul.

Teststödet antar ägda, isolerade testkataloger. Det är inte skyddat mot externa
filbyten med samma OS-rättigheter och ska aldrig lyftas in i produktion genom
att bara ta bort cfg(test).

## Krav inför produktionsintegration

Innan aktiv data får ändras krävs beständig återställningsmarkering och uppstartsspärr,
skyddskopieauktorisering bunden till rätt databas/underhållsoperation/generation,
internt härledd och identitetskontrollerad målanslutning, samma underhållslås genom
hela förloppet samt spärr av journalåtkomst vid Uncertain. Ett befintligt backupfilnamn
räcker aldrig som auktorisering. Efterkontroll, generationsbyte och frontendens
hantering av gammal/osparad text måste samordnas. Ingen blind rollback från en
skyddskopia får ske vid osäkert utfall. Dessa delar är inte implementerade här.
