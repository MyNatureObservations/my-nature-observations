# Förberedd återställningskandidat – #9, steg 3A.2

## Avgränsning och API

`backup::restore::prepare_candidate(path, Arc<AtomicBool>)` är ett synkront, internt
Rust-API. Framtida anrop måste köras på bakgrundstråd. Det tar inte emot någon aktiv
journalsökväg, öppnar inte den aktiva databasen och ändrar inte datagenerationen.
Ingen återställningsmotor, filväljare eller frontendfunktion införs.

Resultatet `PreparedRestore` äger en skrivskyddad SQLite-anslutning med en kvarhållen
lästransaktion, ett öppet filhandtag, verifierade metadata och en privat katalog.
`source()` kontrollerar att transaktionen finns kvar, att filidentiteten är oförändrad
och att SHA-256 över det behållna filhandtaget fortfarande motsvarar den verifierade
nyttolasten. Den lämnar sedan den **befintliga anslutningen** till framtida kopiering.
Originalbackupen behöver inte öppnas igen. `manifest()` lämnar verifierade metadata.
Kontrollsumman läses strömmande men kräver en extra genomläsning vid varje `source()`.

På Unix skapas katalogen med 0700 och databasfilen med 0600 innan journaldata skrivs,
oberoende av umask. På Windows skapas katalogen atomärt med skyddad DACL för aktuell
användare och SYSTEM. Filen ärver endast dessa rättigheter och ACL kontrolleras innan
data skrivs. Filsystem som inte kan ge detta skydd avvisas. Ett behållet Windows-
filhandtag tillåter endast delad läsning, vilket även blockerar nya skriv-/raderingshandtag.

SQLite-transaktionen etableras av verifieringens läsningar och hålls öppen under
kandidatens hela livstid. I kandidatens fristående rollbackläge kan en annan normal
SQLite-anslutning inte committa ändringar eller byta journalmode. Källdatabasen kan
använda WAL: backupmotorn skapar ändå en fristående rollbackkopia. En WAL-fil accepteras
inte som kandidat. I WAL skulle en lästransaktion behålla en äldre snapshot, men den
modellen används inte för kandidaten. SQL som försöker avsluta transaktionen, skapa
savepoints eller ändra PRAGMA avvisas via anslutningens authorizer.

**Krav på framtida återställningsmotor:** behåll kandidaten och använd anslutningen från
`source()` under hela SQLite Online Backup-kopieringen; öppna aldrig kandidatens sökväg
på nytt och släpp inte transaktionen. Kontrollera den färdigkopierade databasen innan
någon aktiv databas ersätts. Ett `candidate_changed` ska avbryta operationen. API:t
exponerar ingen produktionssökväg till kandidaten. Betrodd Rust-kod kan avsiktligt
ersätta anslutningens authorizer och måste följa detta kontrakt.

Privata rättigheter skyddar mot andra vanliga lokala användare, inte administratörer
eller skadlig kod med samma användaridentitet. På Unix kan sådan kod kringgå SQLite-
lås med råa filskrivningar. Identitets-/hashkontrollen upptäcker byte eller ändring före
`source()`, men är ingen garanti mot rå manipulation **under** en senare kopiering.
Återställningsmotorn måste därför verifiera sitt kopieringsresultat före aktivering;
hård isolering mot en komprometterad användarprocess ligger utanför detta steg.

När kandidaten släpps stängs anslutningen och filhandtaget före katalogstädningen,
även på Windows. Fel och avbruten förberedelse använder samma RAII-städning.
Städning är best effort vid filsystemfel; en processkrasch kan lämna en privat
katalog. Ingen sådan katalog är en publicerad backup eller aktiverad naturbok.
Originalbackupen öppnas endast skrivskyddat; aktiva journaldata och externa filer berörs inte.

## Format och SQLite

Backupformat v1 är oförändrat. Den vanliga backupverifieringen behåller sin tidigare
avgränsning. Kandidatförberedelsen återanvänder den strömmande centralkatalogkontrollen
som upptäcker verkliga ZIP-dubbletter innan bibliotekets namnindex byggs.

Kontroller inför kandidatgodkännande:

- Exakt `manifest.json` och `journal.sqlite`, inga dubbletter eller andra sökvägar.
- Endast okomprimerade, okrypterade filposter, inga kataloger eller symlänkar.
- Deklarerade storlekar jämförs mot verklig arkivlängd och faktisk kopierad mängd.
  Manifest, ZIP-CRC och SHA-256 kontrolleras; en kontrollsumma är ingen signatur.
- Formatversion 1, schemaversion 0, `book_id: null`, UUID, UTC-tid och giltiga metadata.
- SQLite-huvud och filstorlek måste beskriva en komplett, fristående rollbackjournal-
  snapshot. En WAL-fragmentfil accepteras inte.
- Skrivskyddad SQLite-öppning, `trusted_schema=OFF`, defensivt läge, ingen mmap.
- Full `PRAGMA integrity_check`: exakt ett resultat, `ok`, krävs.
- Verklig `user_version` måste vara 0.
- Alla objekt i `sqlite_schema` jämförs med en isolerad databas skapad av MNO:s
  verkliga `initialize_schema()`. Typ, namn, tabelltillhörighet och SQL-definitioner
  jämförs, med normaliserat mellanrum. Detta omfattar kolumner, defaults, constraints,
  datumindexet och primärnyckelindexet. Extra tabeller, index, vyer, triggers,
  genererade kolumner och ändrade constraints avvisas.
- Journalfält som ska vara text måste verkligen vara text, inte NULL eller BLOB.
  ID måste vara ett icke-tomt säkert URL-segment (ASCII-bokstäver, siffror, `_`, `-`).
  Datum måste vara ett giltigt `YYYY-MM-DD`.

Schemakontrollen är avsiktligt konservativ: även en semantiskt likvärdig men annorlunda
SQL-definition kan avvisas. Ingen automatisk konvertering görs. En nyare appversion
accepteras bara om format, schema och alla dokument fortfarande klarar samma kontroller;
appversionen är inte i sig ett kompatibilitetsbevis.

## Dokumentformat och #15

Nuvarande journal sparar rå Tiptap/ProseMirror-JSON i `content_json`, med separat
`document_version`, vars enda stödda värde är 1. Ingen separat bokidentitet eller
redigeraroberoende dokumentmodell har införts.

`document-schema-v1.json` beskriver schemat från den faktiskt installerade, oförändrade
StarterKit-konfigurationen i `JournalEditor.svelte`. Ett frontendtest jämför schemat
mot Tiptap och kontrollerar den omfattande giltiga Rust-fixturen med ProseMirror.
Biblioteks-/editorkonfiguration som ändrar schemat måste därför granskas uttryckligen.

Rust kontrollerar hela dokumentträdet: kända nodtyper, tillåtna barnrelationer,
attributnamn och värdetyper, icke-tomma textnoder, formateringsmarkeringar, deras ordning
och ömsesidiga uteslutning. Okända fält, JSON-dubblettnycklar och innehåll som annars
kan tappas bort avvisas. Rubriknivåer är 1–6; numeriska liststarter måste vara ändliga heltalsvärden som JavaScript Number kan
representera. Även 9007199254740992 och exponentnotation accepteras. Heltalstokens
inom Rusts heltalsintervall måste motsvara den kortaste decimalrepresentation som
Number-värdet kan rundtrippas med. Detta inkluderar 1000000000000000100, trots att
dess exakta binära heltalsvärde skiljer sig från decimaltexten. Godtycklig heltals-
precision garanteras inte; samma IEEE-754-modell som i redigeraren används. Dokumentet skrivs aldrig om eller normaliseras av verifieringen.

Detta löser kompatibilitetsspärren **för återställningskandidater**. #15 är inte färdigt:
vanlig läsning och redigering av redan lagrade dokument har inte ändrats i detta steg.

## Resursskydd

Det finns inget fast tak för naturbokens totala storlek eller antal anteckningar.
Databasen extraheras strömmande och anteckningar kontrolleras radvis.

Tekniska gränser:

| Resurs | Gräns / princip |
|---|---|
| Manifest | 64 KiB, även läsningen begränsas |
| Samlad ZIP-overhead | 1 MiB utöver de två lagrade nyttolasterna; täcker standardens begränsade namn, extrafält, kommentarer och ZIP64-huvuden |
| Databasnyttolast | Måste rymmas i den verkliga arkivfilen; Stored ger ingen uppackningsexpansion |
| Ledigt utrymme | Databasens storlek plus 16 MiB marginal före extrahering; marginalen kontrolleras igen under kopieringen |
| Kopieringsbuffert | 64 KiB |
| SQLite-rad / enskilt dokument | 64 MiB; minnes-/parsergräns, inte ett totalt boktak |
| JSON | Högst 64 nivåer och 1 000 000 värden, kontrollerat under parsning före fortsatt allokering |
| SQLite SQL / kolumner / uttryck | 64 KiB / 32 / 32; inga anslutna databaser |
| SQLite-cache | 8 MiB mål; detta är inte en hård total minneskvot |
| SQLite-arbete | Högst `256 × databasbytes + 10 000 000` VM-instruktioner, kontrollerat i steg om 10 000 |

För stora enskilda dokument eller onormala strukturer avvisas med resursfel utan
konvertering. Det påverkar inte originalet. Ingen väggklocktimeout sätter ett indirekt
tak för långsam hårdvara eller stora böcker. Avbrottsflaggan kontrolleras under
kopiering, mellan dokument och i SQLite:s progress-handler.

`fs2::available_space` använder plattformens uppgift om tillgänglig lagring på macOS,
Windows och Linux. Den är rådgivande: andra processer, kvoter och filsystem kan ändra
utrymmet efter kontrollen. Faktiska disk-/kvotfel hanteras också. En verklig full disk
simuleras genom kontrollerade testpunkter, inte genom att fylla användarens volym.

Detta är inte processisolering eller en absolut CPU-/minnesgaranti. SQLite:s interna
schemaladdning omfattas inte fullt ut av VM-progress-handlern, och cachegränsen är
ingen hård heapgräns. En global SQLite-heapgräns används inte eftersom den även skulle
påverka den aktiva journalens anslutningar. Hård isolering av fientliga databaser är
ett separat framtida beslut om ett sådant skydd krävs. Ingen produktionsväg använder
ännu kandidat-API:t. Windows/Linux har inte körtestats i detta macOS-steg.

## Identifierbara fel

`PrepareError` serialiseras som stabila snake_case-koder för senare översättning:
`damaged_archive`, `unsupported_format`, `invalid_database`, `incompatible_schema`,
`incompatible_document`, `insufficient_space`, `resource_limit`, `cancelled`, `io`, `candidate_changed`.
Fel innehåller ingen journaltext eller automatisk ändring av originaldata.

## Tester

Isolerade tester täcker äkta backupmotorutdata, fulla journalrader, oförändrad
originalbackup och källdatabas, kvarhållen kandidat efter borttagen originalfil,
skrivad åtkomst som avvisas, skadat ZIP/SQLite, hashfel, okända versioner, schemaändringar,
triggers/vyer/index, JSON/nodstruktur, ZIP-dubbletter och storleksmanipulation,
parsergränser, simulerad utrymmesbrist samt städning efter fel, avbrott och normal drop.

De två tidigare kötesterna har en testkompilerad kontrollpunkt som signalerar först
när själva operationsvägen gjort ett misslyckat försök att ta låset. Generationsbytet
sker därefter. Produktionsvägen använder fortfarande samma blockerande låstagning och
samma generationskontroll. Ingen timinggissning används för att skapa konkurrensen.

Referenser: [SQLite security](https://www.sqlite.org/security.html),
[SQLite integrity_check](https://www.sqlite.org/pragma.html#pragma_integrity_check),
[fs2 available_space](https://docs.rs/fs2/0.4.3/fs2/fn.available_space.html).


## Bildanalys och beslutad resursgräns för återställning v1

Dagens editor använder enbart StarterKit, utan bildnod, filhanterare eller egen
paste/drop-kod som läser bildfiler. HTML-bilder kan inte representeras i detta schema;
bildfiler från urklipp eller dragning bäddas inte in av MNO. Användaren kan däremot
klistra in Base64 som vanlig text eller mycket stora textmängder.

Vanlig sparning har inte kandidatverifieringens 64 MiB-gräns. Ett strukturellt giltigt
dokument skapat med dagens editor kan därför i princip överskrida denna gräns, liksom
64 JSON-nivåer vid mycket djupa nästlade listor eller en miljon JSON-värden vid extremt
många små formaterade textdelar. Sådana dokument avvisas som resursfel, inte som
formatfel. Gränserna ändras inte i denna åtgärd.

Beslut: gränsen 64 MiB per journaldokument behålls tills vidare som en teknisk
säkerhetsgräns för återställning v1. Den är inte en permanent begränsning av
journalformatet eller naturbokens totala storlek. Giltiga dokument som överskrider
gränsen klassificeras som `resource_limit`, inte som skadade eller inkompatibla.
SQLite har dessutom samma gräns för en hel rad, inklusive övriga journalfält och
radens overhead; en rad kan därför nå resursgränsen även om dess JSON är något mindre.
Ett godkännande av steget innebär inte att varje tänkbar stor eller djupt nästlad
anteckning kan förberedas för återställning. Framtida justeringar av gränserna kräver
separat bedömning av resursförbrukningen.

Framtida fotografier ska lagras separat och refereras från JSON, som MNO-hanterade
kopior eller länkar till externa original. Bildbytes ska inte räknas in i JSON-gränsen.
Många referenser kan fortfarande öka dokumentstorleken. Nya bildnoder behöver ett
uttryckligt kompatibilitetsbeslut enligt #15. Externa original får aldrig ändras eller
raderas av MNO. Ingen bildhantering införs här.

Nya regressionstester täcker umask 000/022 i separata processer, privata Unix-lägen,
SQL-skrivning/trigger/journalmode mot kvarhållen snapshot, förbjudet transaktionsslut,
kopiering från den verifierade anslutningen, rå ändring och filbyte på Unix,
WAL-källa samt stora liststarter. Windows har separat ACL-/delnings-/städningstest,
men det måste köras på Windows; macOS-körningen verifierar inte Windows-API:erna.
