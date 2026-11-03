# Verifiering och exakt journalinnehåll – #9, steg 3A.3b.2

## Interna API:er och ansvar

`backup::journal_content::VerifiedJournal::verify(&mut Connection, cancel)` verifierar
main-databasen på en redan öppnad, dedikerad SQLite-anslutning. Samma interna
`restore::validate_database` används som vid kandidatförberedelse: full
`main.integrity_check`, schema inklusive constraints/index och oväntade objekt,
`main.user_version`, radtyper/datavillkor, dokumentversion och Tiptap-JSON.
Verifieringen försvagas inte. Befintliga dokument- och SQLite-resursgränser gäller.

`VerifiedJournal::from_candidate(&PreparedRestore, cancel)` lånar i stället kandidatens
redan verifierade, behållna anslutning och snapshot via `source()`. Kandidatens
filidentitets- och hashkontroller behålls före och efter innehållsoperationer.
Ingen anslutning öppnas på nytt från en kandidatsökväg. Kandidatens authorizer,
läsreservation och resursinställningar lämnas orörda.

`compare(&VerifiedJournal, &VerifiedJournal)` returnerar `Identical` eller `Different`.
`fingerprint(&VerifiedJournal)` returnerar `{ version: 1, sha256: <64 hextecken> }`.
Båda kräver verifierade snapshots. Ogiltiga typer, NULL i obligatoriska fält,
okänd dokumentversion eller ogiltigt schema ger verifieringsfel, aldrig ett
likhetsresultat. De privata kodnings- och jämförelsefunktionerna skiljer även på
NULL och tomma värden samt samtliga SQLite-typer; detta testas separat eftersom
alla sådana värden inte är tillåtna i en giltig MNO-databas.

## Data och exakt jämförelse

Endast `main.journal_entries` ingår, i denna kolumnordning:

`id, entry_date, title, content_json, document_version, created_at, updated_at`.

Rader läses med `ORDER BY id COLLATE BINARY`. Strikt verifiering kräver unika,
icke-NULL TEXT-ID:n, vilket ger entydig ordning. TEMP-tabeller får inte skugga
main-databasen. Antal rader jämförs också. TEXT/BLOB jämförs byte för byte,
INTEGER som i64 och REAL som exakta IEEE-754-bitar. Typen måste vara densamma.
Rå JSON jämförs utan omserialisering; även blanksteg och nyckelordning räknas.
Unicode normaliseras inte. SQLite levererar TEXT i UTF-8.

Två cursors används vid jämförelse och en vid hashning. Hela naturboken läses
aldrig in i minnet. Enskilda rader/dokument måste fortfarande rymmas inom den
befintliga tekniska gränsen på 64 MiB; giltiga större dokument ger ResourceLimit.
JSON-djup/antal värden och SQLite-arbetsbudget återanvänds från 3A.2.

## Fingeravtrycksformat version 1 – exakt bytekodning

SHA-256 beräknas över följande sammanhängande byteström. Alla flertalsbytes heltal
är big-endian. `\0` betyder exakt en nollbyte. Strängliteralerna här är ASCII.

1. `MNO-journal-content\0`
2. Versionsnumret 1 som u32: `00 00 00 01`.
3. `journal_entries\0id\0entry_date\0title\0content_json\0document_version\0created_at\0updated_at\0`
4. För varje rad i ordningen ovan: byte `52` (hex, ASCII R), därefter exakt sju värden.
5. Byte `45` (hex, ASCII E), därefter totalt antal rader som u64.

Varje värde kodas med en typbyte och följande payload:

| SQLite-typ | Typbyte (hex) | Payload |
| --- | --- | --- |
| NULL | 00 | Ingen |
| INTEGER | 01 | i64, tvåkomplement, 8 byte |
| REAL | 02 | IEEE-754 binary64-bitar, 8 byte |
| TEXT | 03 | u64 antal byte, följt av UTF-8-bytes |
| BLOB | 04 | u64 antal byte, följt av råa bytes |

Resultatet är 64 gemena hextecken tillsammans med versionsfältet 1. Det fasta
antalet kolumner, typtaggar, längder och slutmarkören gör kodningen entydig.
Regressionstestet `fingerprint_v1_has_a_fixed_independently_encoded_vector_and_empty_journals_match`
innehåller en oberoende beräknad referensvektor.

Sidstorlek, rowid, fysisk radordning, freelist, filhash, SQLite-header och manifest
ingår inte. Schema och schemaversion kontrolleras separat, inte i fingeravtrycket.
En ändrad definition kräver ny fingeravtrycksversion; backupformat v1 ändras inte.
Den exakta jämförelsen använder aldrig hash som ersättning för värdejämförelse.
SHA-256 har en teoretisk kollisionsrisk och är inte ett äkthetsbevis.

## Snapshots, transaktioner och lås

För vanlig verifiering lånas anslutningen exklusivt i Rust under tokenens livstid.
Finns ingen transaktion startas `BEGIN DEFERRED`; integritetskontrollen etablerar
main-databasens läsbild före schema- och innehållskontroller. En befintlig
lästransaktion används och lämnas öppen. En befintlig skrivtransaktion avvisas
som `WriteTransaction` innan inställningar ändras. Verifieraren avslutar aldrig
anroparens transaktion. `finish()` avslutar endast den egna lästransaktionen och
rapporterar städfel; Drop gör samma städning som reservväg, utan felrapportering.
Kandidatens transaktion ägs och skyddas fortsatt av PreparedRestore.

I rollbackjournal-läge kan läsreservationen hindra en annan skrivare från att
committa tills snapshoten släpps. WAL tillåter nya commits men läsaren fortsätter
se sin gamla stabila snapshot. Funktionen väntar inte på frontend och tar inga
samordningslås. Anroparen får inte vänta på en skrivare vars commit kräver att
läsreservationen släpps. Varje databas får sin egen stabila läsbild; detta är inte
en gemensam atomisk tidpunkt för två oberoende databaser. Framtida användning med
aktiv databas ska ske under det befintliga underhållslåset och säkerhetskontraktet.

Vanliga verifieringsanslutningar ska vara dedikerade och sakna progress-handler
som behöver bevaras. Verifieraren installerar och sedan tar bort sin egen handler,
och lämnar skärpta limits/defensive/trusted_schema/cache/mmap-inställningar kvar.
Den ändrar aldrig authorizer. TEMP-lagring måste vara FILE eller den verifierade
bundlade FILE-standarden. MEMORY/okänd standard avvisas med ResourceLimit i stället
för att ändra temp_store, vilket annars kan radera anroparens TEMP-objekt.
Kandidatvägen ändrar inte dessa inställningar alls.

## Fel, begränsningar och framtida användning

ContentError skiljer Database(PrepareError), WriteTransaction och SnapshotLost.
PrepareError behåller skillnaden mellan korruption, inkompatibelt schema/dokument,
I/O, utrymmesbrist, resursbegränsning och avbrott. SQLite busy/locked är I/O-fel här,
aldrig ett likhetsresultat. Avbrottsflaggan kontrolleras under radströmningen;
vanliga verifieringsanslutningar använder också SQLite:s progress-handler.
Kandidaten behåller sin befintliga säkerhetskonfiguration. Hashning av en enskild
begränsad rad och kandidatens filhashkontroll kan inte avbrytas mitt i arbetet.

En framtida återställningsmotor ska verifiera resultatanslutningen, jämföra den
med den behållna kandidaten och avsluta egna läsreservationer innan nästa skrivsteg.
En beständig markering måste lagra fingeravtrycksversionen tillsammans med hashen;
uppstartskontroll måste först verifiera kompatibilitet och sedan beräkna samma
version. Fel får aldrig översättas till Identical eller normal drift. Varken
återställningsmotor, markering, uppstartsåterhämtning eller generationsbyte finns
i detta steg. Auktorisering och obligatorisk skyddskopia får inte kringgås.

API:erna ändrar inte journalrader eller databasformat. Vanlig SQLite-åtkomst kan
hantera journal-/WAL-sidofiler och lås enligt SQLite:s regler. Ingen produktionsväg
anropar den nya verifieraren ännu. Tester använder endast tillfälliga databaser;
ingen verklig journal eller extern originalfil berörs. Verifieringen har körtestats
på macOS. Linux/Windows kräver egna körtester; Windows-begränsningen för obligatorisk
skyddskopia kvarstår. Verifieraren är inte ett skydd mot en angripare som har samma
OS-rättigheter och kan skriva råa databasbytes utanför SQLite:s låsmodell.

Steg 3A.3b.3 rättar avbrottsklassificering även när ett SQLite-anrop misslyckas.
Den helt testisolerade kopieringsbyggstenen beskrivs i
[Isolated_SQLite_Copy.md](Isolated_SQLite_Copy.md).
