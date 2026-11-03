# Pågående återställning – markörformat v1

Steg 3A.3b.4b.2 inför endast intern metadata och beständig lagring. Ingen
produktionskopiering, återhämtning, klassificering, generationsändring eller
frontend aktiveras. Backupformat v1 ändras inte.

## Fast plats och öppet JSON-format

Endast `recovery/pending-restore.json` under den bundna, kanoniska datakatalogen.
Exempel (hasharna är illustrativa):

```json
{
  "format": "mno-pending-restore",
  "version": 1,
  "restore_id": "a29aa4d2-7298-4a75-a7e3-6c824e64fbe8",
  "state": "pending",
  "database_path": "mno.sqlite",
  "database_identity": {"platform":"unix","volume":"0000000001000010","file":"0000000000012345"},
  "directory_identity": {"platform":"unix","volume":"0000000001000010","file":"0000000000012340"},
  "protection_path": "backups/pre-restore/pre-restore-20261018-143000-a29aa4d2-7298-4a75-a7e3-6c824e64fbe8.mno-backup",
  "protection_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "old_content": {"version":1,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
  "new_content": {"version":1,"sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},
  "schema_version": 0,
  "created_at": "2026-10-18T14:30:00Z"
}
```

restore_id är underhållsoperationens UUID v4. Filidentiteter återanvänder interna
volym-/filnummer (Unix device/inode, Windows volym-/filindex), kodade som 16 gemena
hextecken per u64 för att undvika precisionstapp i JSON-verktyg. Plattformarna måste
överensstämma. Flytt till annat filsystem kan ändra identitet; framtida återhämtning
får inte ignorera detta. Tidpunkten är diagnostisk RFC 3339, aldrig auktorisering.
Fingeravtrycken är version 1 från journal_content::fingerprint på de behållna
verifierade snapshotsen för skyddskopia och kandidat. Se
[Journal_Content_Verification.md](Journal_Content_Verification.md).
protection_sha256 avser hela skyddskopiearkivet. Schemat är SCHEMA_VERSION=0.

## Strikt läsning

Alla fält krävs. Saknade, extra och dubbla nycklar avvisas även i nästlade objekt.
Okända format/versioner/state, felaktiga UUID, hashar, identiteter och tider
avvisas. SHA-256 kräver 64 gemena hextecken. Läsgränsen är **16 KiB** plus en
extra detektionsbyte; detta begränsar endast metadata, aldrig journalen.
Målsökvägen måste vara exakt mno.sqlite. Skyddskopiesökvägen måste vara exakt
backups/pre-restore/pre-restore-YYYYMMDD-HHMMSS-<uuid-v4>.mno-backup med giltig tid.
Absoluta sökvägar, .., andra namn och alternativa separatorer avvisas.
Läsaren öppnar endast fasta namn, avvisar länkar och oväntade filtyper och använder
O_NOFOLLOW/O_NONBLOCK och identitetskontroll på Unix. Inlästa sökvägar används aldrig
för skrivning/radering. Syntaktisk giltighet bevisar inte att identiteter eller
skyddskopia motsvarar aktuell datakatalog; sådan kontroll tillhör nästa steg.

## API och säkerhetskontrakt

backup::restore_context::pending::read(root) läser enbart metadata.
AuthorizedRestore::publish_pending() kräver befintlig engångsauktorisering,
operationsbundet skyddskopiebevis, aktuell generation och behållet underhållslås.
Inga godtyckliga metadata eller målsökvägar accepteras. Metadata ger aldrig skrivrätt.
PendingMarker är inte klonbar, lånar underhållslåset och aktiv databas och håller
fil-/katalogidentiteter samt exakt publicerade bytes. finish(self) konsumerar
handtaget och kontrollerar operation, generation, identiteter och innehåll.
Drop raderar aldrig något. Ett inläst metadataobjekt kan inte skapa ett handtag.

## Beständighet och fel

macOS/Linux: recovery skapas exklusivt med högst 0700, temporärfil med 0600.
Innehållet flushas, läses tillbaka och synkroniseras före persist_noclobber.
Slutnamnet skrivs aldrig över. Recovery och hela föräldrakedjan fsyncas före sista
filbarriären. Först då kvitteras publiceringen. Barriärkoden delas med skyddskopian.
macOS använder F_FULLFSYNC för filer och fsync för kataloger; Linux sync_all/fsync.
Alla obligatoriska fel returneras. Sena fel bevarar redan publicerad markör;
endast den egna temporärfilen städas före publicering.

finish tar endast bort det fasta markörnamnet, synkroniserar recovery och
föräldrakedjan och gör sista filbarriären på MNO:s låsfil. Journal och skyddskopior
ändras inte. Vid sent fel kan markören redan vara borttagen; det räknas inte som
framgång. DatabaseState spärras före beständig mutation och låses inte upp här.
**Den tomma recovery-katalogen behålls även efter lyckad finish.** Därmed blockerar
nuvarande uppstart fortfarande efter framgång, fel och omstart. Ett senare steg
måste införa verifierad klassificering och beständig övergång till normal drift;
att bara radera katalogen eller nollställa flaggan är ingen godkänd genväg.

## Tester och begränsningar

Isolerade tester täcker formatfel, storlek, dubbla nycklar, sökvägar, rättigheter,
samtidig publicering, alla katalogbarriärer, sena fel, fel operation, ändrad/bytt
markör, avbrott, avslutningsfel och bevarad journal/skyddskopia/uppstartsspärr.
Fel injiceras deterministiskt utan tidsbaserade antaganden.
macOS är körtestplattform. Linux kräver egna körtester. Windows publicering och
avslutning avvisas före fil-/katalogskapande tills godkänt beständighetskontrakt finns.
Faktiskt strömavbrott och lagringsmaskinvarans barriärer bevisas inte av tester.
Protokollet samordnar MNO och är inte en sandbox mot skadlig kod med samma
OS-rättigheter som byter filer mellan kontroller. Externa originalfiler berörs inte.
