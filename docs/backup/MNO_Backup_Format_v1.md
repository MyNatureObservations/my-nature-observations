# MNO backupformat v1 – #9, steg 1–2

## Omfattning

En `.mno-backup` är en självständig, fullständig ögonblicksbild av den sparade SQLite-huvuddatabasen. Inga tidigare säkerhetskopior behövs. Formatet är ett vanligt ZIP/ZIP64-arkiv utan kryptering och utan komprimering (`Stored`), läsbart med vanliga ZIP-verktyg på macOS, Windows och Linux. Filändelsen är en etikett; läsaren verifierar innehållet.

Version 1 innehåller exakt två poster med fasta, relativa namn:

```text
manifest.json
journal.sqlite
```

`journal.sqlite` innehåller alla tabeller, index och sparade journaldata i `main`, inklusive ursprunglig JSON, dokumentversioner, ID:n, datum och tidsstämplar. SQLite:s interna sidlayout kan ändras när snapshoten görs självständig; journalinnehållet serialiseras inte om. Tillfälliga/anslutna databaser ingår inte; dagens MNO använder inte sådana för användardata.

Personliga inställningar, webbvyns localStorage, osparad redigerartext, cache och externa filer ingår inte. Inga sökvägar i journalinnehållet följs. Externt länkade original öppnas, kopieras, ändras eller raderas aldrig. Det finns ännu ingen bildhantering.

## Läsa en säkerhetskopia utan MNO

MNO behövs inte för att öppna formatet eller läsa den sparade journalinformationen:

1. **Skapa en kopia** av `.mno-backup` i en separat arbetsmapp med operativsystemets filhanterare. Behåll originalbackupen orörd.
2. Öppna kopian med ett ZIP-verktyg. Om verktyget inte känner igen filändelsen, byt **kopians** filändelse från `.mno-backup` till `.zip`. Innehållet behöver inte konverteras. Detta fungerar med vanliga ZIP/ZIP64-verktyg på macOS, Windows och Linux.
3. Packa upp `journal.sqlite` och `manifest.json` i arbetsmappen. Öppna inte den levande MNO-databasen av misstag.
4. Öppna den uppackade `journal.sqlite` **skrivskyddat** i ett SQLite-verktyg med uttryckligt stöd för skrivskyddad öppning. Med SQLite:s kommandoradsverktyg, från arbetsmappen:

   ```sh
   sqlite3 -readonly journal.sqlite
   ```

5. Läs innehållet med de verkliga journalfälten:

   ```sql
   PRAGMA quick_check;

   SELECT id, entry_date, title, content_json
   FROM journal_entries
   ORDER BY entry_date, id;
   ```

   `quick_check` ska ge `ok`. `content_json` innehåller själva journalen som strukturerad JSON; datum och titel är separata fält. Denna manuella läsning är ingen återställning och ändrar inga data. Avsluta kommandoradsverktyget med `.quit`.

Det grafiska SQLite-verktygets skrivskyddade läge ska aktiveras innan filen öppnas. En vanlig SQLite-kontroll ersätter inte paketets kontrollsumme- eller versionskontroll och bevisar inte att redigeraren kan tolka alla dokument.

## Säker förvaring

**Backupfilerna är inte krypterade.** Personer som får tillgång till en `.mno-backup` kan läsa journalinnehållet utan MNO och utan lösenord. Detta kan omfatta känsliga uppgifter, exempelvis privata fyndplatser.

Förvara därför kopiorna på betrodd, åtkomstskyddad lagring. Kontrollera åtkomsträttigheter och eventuell delning när filerna kopieras till extern eller molnsynkroniserad lagring. Även uppackade arbetskopior innehåller samma känsliga information och behöver skyddas. SHA-256 ger ingen sekretess och är ingen digital signatur.

## Manifest

UTF-8 JSON, högst 64 KiB. Exempel (storlek och kontrollsumma måste motsvara den verkliga databasfilen):

```json
{
  "format": "mno-backup",
  "format_version": 1,
  "backup_id": "324b2d95-3f05-41bb-9cf4-c380625d5b79",
  "created_at": "2026-10-17T14:30:00Z",
  "app_version": "0.1.0",
  "database_schema_version": 0,
  "book_id": null,
  "kind": "manual",
  "database_file": "journal.sqlite",
  "database_size": 12288,
  "database_sha256": "<64 hexadecimala tecken, SHA-256 för hela journal.sqlite>"
}
```

- `format_version` identifierar paketformatet, inte journalens dokumentformat.
- `backup_id` är en ny UUID för just denna kopia.
- `created_at` är UTC i RFC 3339, satt efter att SQLite-snapshoten har slutförts. Samtidiga ändringar kan flytta snapshotens läge under kopieringen; tidpunkten utlovar inte en exakt startrevision.
- `app_version` identifierar appversionen som skapade kopian.
- `database_schema_version` motsvarar snapshotens `PRAGMA user_version`. Dagens databas är ett ännu inte explicit versionssatt legacy-schema med värdet **0**. Det ändras inte till 1 av backupfunktionen. Journalens radvisa `document_version` är en annan sak och bevaras i databasen.
- `book_id` är **null** eftersom något stabilt naturboks-ID ännu inte finns. Ett nytt ID per backup skulle felaktigt identifiera samma bok som olika böcker. Införande av beständig bokidentitet och schemamigrering ligger utanför steg 1.
- `kind` är `manual` eller `automatic`. Typen är metadata; ingen automatisk körning eller gallring implementeras här.
- Storlek och SHA-256 gäller de exakta bytes som lagrats som `journal.sqlite`.

Den aktuella verifieraren accepterar bara denna formatversion, schema 0 och `book_id: null`. Okända fält, extra poster och okända versioner avvisas uttryckligen. Framtida paketversioner kan lägga till MNO-förvaltade filer eller frivilliga personliga inställningar genom utökad metadata och fler poster. Äldre läsare ska då avvisa paketet, inte tyst ignorera innehåll. Bilder och inställningsformat definieras inte nu.

## Interna Rust-funktioner

`backup::create_backup(&Connection, &Path, BackupKind)` skapar en kopia och returnerar verifierat manifest. Målkatalogen ska finnas och målsökvägen ska sluta med `.mno-backup`. Ett befintligt mål ersätts aldrig, även om det skapas av en annan operation efter den första existenskontrollen.

`backup::verify_backup(&Path)` kontrollerar ett paket och returnerar manifestet, utan åtkomst till den levande journalen.

Grundfunktionerna är synkrona och interna. Steg 2 anropar skapandet via Tauri-kommandot `create_manual_backup`, som visar en native Spara-dialog och kör SQLite-backup och verifiering på en bakgrundstråd. Funktionen finns under Säkerhetskopiering i panelen Verktyg och inställningar. Ingen automatisk körning vid start eller journalsparning, återställning, historik eller gallring har införts.

Ett namn i lokal tid föreslås: `MNO-backup-ÅÅÅÅ-MM-DD_HHMM.mno-backup`. Användaren väljer plats och namn. Avbruten dialog öppnar inte databasen och skapar ingen backup. Ett befintligt mål ersätts aldrig, även om operativsystemets Spara-dialog frågar om ersättning; MNO visar i stället ett meddelande om att välja ett annat namn eller en annan plats. Saknad filändelse kompletteras med `.mno-backup`; annan uttrycklig filändelse avvisas. Dubbla starter förhindras både i gränssnittet och med ett processgemensamt Rust-skydd, som omfattar dialog och hela arbetet. Den sparade databasen öppnas skrivskyddat via befintlig datakatalog, inklusive `MNO_TEST_DATA_DIR` i utvecklingsläge. Osparad redigerartext sparas inte som en bieffekt av backupen.

## Skapande och publicering

1. Avvisa en källanslutning med aktiv transaktion och ett befintligt mål.
2. Skapa ett privat temporärt arbetsutrymme på målets filsystem.
3. Kopiera `main` via SQLite Online Backup API i steg om 128 sidor. Endast bekräftade ändringar inkluderas. Källanslutningen och källschemat ändras inte. Andra anslutningar kan använda databasen under arbetet.
4. Hantera tillfälliga `Busy`/`Locked` med kort väntan och avbryt efter cirka 30 sekunder. Gränsen kontrolleras mellan stegen; ett enskilt steg kan dessutom vänta enligt källanslutningens busy-handler. Ingen obegränsad `run_to_completion` används.
5. Gör mål-snapshoten självständig med DELETE-journal om WAL följt med. Stäng anslutningen; inga `-wal`/`-shm`-filer behövs i paketet.
6. Kör `quick_check` och schema-kontroller, skapa manifest och skriv ZIP till ett temporärt filnamn utan `.mno-backup`.
7. Synkronisera arkivfilens innehåll till lagringen, öppna arkivet igen och verifiera hela paketet inklusive en ny extraherad databas.
8. Publicera med `persist_noclobber` i samma katalog/filsystem. Målet skapas endast av en redan verifierad fil och ett befintligt mål skrivs aldrig över. För vanlig manuell backup sker därefter inga operationer vars fel skulle rapportera en publicerad kopia som misslyckad. Obligatoriska skyddskopior har ytterligare beständighetskrav enligt avsnittet nedan.

Vanliga fel städar temporära filer automatiskt. Ett processavbrott kan lämna temporära rester, men de har inget färdigt backupnamn. Tidigare kopior berörs inte. Filsynkronisering är ingen garanti för molnsynkronisering eller mot felande hårdvara. Katalogpostens beständighet vid strömavbrott garanteras inte av denna grundimplementation; en ny kopia kan då saknas, men publicering sker aldrig genom att strömma halvfärdigt innehåll till slutnamnet. Filsystem som inte stöder säker publicering utan överskrivning ger fel.

## Verifiering

- ZIP:s faktiska centralförteckning läses post för post med `rawzip`, utan deduplicering. Endast de två exakta namnen tillåts, varje namn exakt en gång. Det faktiskt lästa antalet kontrolleras även mot arkivets angivna antal; fel vid läsningen avvisas. Detta görs innan `zip::ZipArchive` skapar sitt namnindex.
- Därefter används `zip` för innehållsläsning, kontroll av `Stored`-metoden och CRC. Ingen egen ZIP-parser eller godtycklig sökvägsextrahering används.
- Manifestet kan läsas och har känd version, databasfil, typ, UUID, UTC-tid och metadata.
- Databasens längd och SHA-256 stämmer; ZIP-poster läses till slut, vilket också kontrollerar deras CRC.
- Databasen extraheras endast till en slumpmässig temporär fil, aldrig via arkivets sökvägar.
- Den öppnas skrivskyddat. `PRAGMA quick_check` måste ge `ok`.
- `user_version` måste vara 0. `journal_entries` måste vara en tabell och alla nuvarande journalfält måste kunna läsas.

Detta kontrollerar lagring och grundläggande schema, inte Tiptap-kompatibilitet, sanningshalten i journalinnehållet eller full migreringskompatibilitet. JSON bevaras rått även om en framtida redigerare inte kan tolka det. Djupare dokumentkompatibilitet hanteras i #15. `quick_check` ersätter inte `integrity_check` vid framtida återställningskontroll. Kontrollsumman upptäcker förändringar men är ingen digital signatur; paketet är inte krypterat. Godtyckliga externa paket behöver dessutom en genomtänkt storleks-/lagringspolicy innan en användarsynlig import- eller återställningsfunktion införs.

## Tester och kvarstående arbete

Rust-tester använder endast tillfälliga kataloger och appens verkliga journalschema. De täcker hela journalrader, oförändrad originalfil, oberoende snapshot, fel/timeout, skydd av befintligt mål, aktiv skrivtransaktion på annan anslutning, samtidiga WAL-commits och avvisning av skadat paket/okänd version/felaktig SQLite.

Dubblettester bygger riktiga ZIP-arkiv med tre poster men två unika namn: dubbla `manifest.json` respektive dubbla `journal.sqlite`. De visar uttryckligen att det gamla ZIP-namnindexet ser två poster, medan verifieringen nu avvisar arkivet. Extra poster och oväntade sökvägar avvisas också.

Två samtidiga skapanden synkroniseras efter komplett paketverifiering men före publicering. Båda har då passerat den första existenskontrollen. Testet kräver exakt en vinnare och en förlorare och kontrollerar både det publicerade manifestet och vinnande journalinnehåll efter att båda operationerna avslutats. Ingen timing används för att åstadkomma konkurrensen; tidsgränser är endast skydd mot ett fastnat test.

Ett kontrollerat fel efter full verifiering men före publicering lämnar inget slutligt backupnamn eller temporära rester. Ett separat test skapar en giltig konkurrerande backup precis vid samma gräns och låter den verkliga `persist_noclobber`-operationen misslyckas; den redan publicerade filen bevaras byte för byte och förlorarens temporära fil städas även om anroparen behåller felobjektet. Produktionsanrop använder en tom intern callback vid testpunkten; ingen användarsynlig felinjektion har införts.

Steg 2 har kopplat in manuell bakgrundskörning och val av målfil. Senare steg behöver eventuell beständig bokidentitet och schemamigrering samt separat beslut om återställningsflödet. Automatisk backup, historik, gallring, automatisk backupmapp och introduktion införs inte här. Backupformatet är heller inte det framtida redigeraroberoende exportformatet.

Referenser:
- [SQLite Online Backup API](https://www.sqlite.org/backup.html)
- [SQLite backup-transaktioner och samtidighet](https://www.sqlite.org/c3ref/backup_finish.html)
- [SQLite quick_check](https://www.sqlite.org/pragma.html#pragma_quick_check)

- [rawzip: ZIP-läsning utan deduplicerad namnlista](https://docs.rs/rawzip/0.5.2/rawzip/)

## Strikt förberedelse inför återställning (#9, steg 3A.2)

En separat intern kandidatverifierare behåller en privat, fullt kontrollerad SQLite-kopia
utan att ändra format v1 eller den vanliga backupverifieringen. Den kontrollerar även
integritet, exakt stödd schemastruktur, dokumentversioner och aktuellt editorformat.
Resursgränser och begränsningar dokumenteras i [Restore_Candidate_Verification.md](Restore_Candidate_Verification.md).
Ingen återställningsmotor eller användarsynlig återställningsfunktion ingår ännu.

## Skyddskopia före återställning (#9, steg 3A.3a)

Skyddskopior använder samma format v1 och `kind: manual`, men lagras permanent i
MNO:s privata `<app-data>/backups/pre-restore/`. De verifieras strikt före publicering
och får aldrig gallras automatiskt. På macOS/Linux krävs dessutom lyckad fil- och
katalogsynkning efter publicering innan ett skyddskopiebevis returneras. Vid sent
synkningsfel bevaras den publicerade filen men operationen rapporterar fel.
På Windows avvisas obligatorisk skyddskopiering tills en tillräckligt underbyggd
beständighetslösning finns; vanlig manuell backup påverkas inte.
Se [Pre_Restore_Protection.md](Pre_Restore_Protection.md).
Ingen återställningsmotor ingår ännu.
