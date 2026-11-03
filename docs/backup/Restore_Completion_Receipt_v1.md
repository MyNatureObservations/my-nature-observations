# Avslutningskvitto v1 – steg 3A.3b.4c.2a

## Omfattning och behörighet

Kvittot är metadata för framtida återupptagning av avslutningsstädning. Det är
aldrig ett operationsbevis, en stabil SQLite-snapshot eller rätt till upplåsning.
Sedan 4c.2c.2 finns ett internt beviskrävande macOS-API, men inget
Tauri-kommando eller frontendanrop. Fria byte-/utfallsbaserade publiceringsanrop
kompileras fortfarande endast i tester. Produktionsvägen kräver ett konsumerat
CompletionProof; kvittometadata räcker aldrig som behörighet.

`pending::completion::read(root)` ger `Option<LoadedReceipt>` eller ett typat fel.
Typens fält är privata och ingen omvandling till Assessment, AuthorizedRestore,
StartupReady eller Maintenance finns. Läsningen öppnar aldrig SQLite, skapar
aldrig en saknad databas och gör inga filändringar. Den kontrollerar metadata-
bindningen, inte journalens innehåll eller skyddskopians giltighet.

## Format

Plats: `<datakatalog>/.mno-restore-completion-v1.json`.

Exakt fyra obligatoriska fält:

| Fält | Innehåll |
| --- | --- |
| format | Strängen `mno-restore-completion` |
| version | Heltalet `1` |
| pending_json | UTF-8-sträng med den ursprungliga pending-markörens exakta JSON-text |
| verified_outcome | Strängen `old` eller `new` |

JSON-strängkodningen av pending_json avkodas tillbaka till originalbytes, inklusive
whitespace. Ingen normalisering eller omskrivning av den inbäddade markören sker.
Befintlig strikt pending-parser återanvänds för schema, version, identifierare,
relativa sökvägar och fingeravtryck. Okända, saknade och dubbla fält avvisas även
i inbäddade objekt. verified_outcome läses uttryckligen som en JSON-sträng:
objektrepresentationerna {"old":null}/{"new":null} och alla andra JSON-typer
avvisas. Kvittots påstådda utfall är inte betrott utan ny verifiering.

Metadata begränsas till `6 * 16384 + 1024 = 99328` bytes före parsning. Sex är
värsta JSON-escape-expansionen per markerbyte och 1024 rymmer det fasta kuvertet.
Själva markören behåller gränsen 16 KiB. Inläsning mäter även faktisk längd och
läser högst gränsen plus en byte; gränsen gäller inte naturbokens storlek.
Positionsbaserad läsning ändrar inte delade filpositioner och tillåter upprepade
kontroller mot samma öppnade objekt.

## Identitet och läsning

Behållna katalog- och kvittohandtag jämförs mot aktuella sökvägsidentiteter.
Symlänkar, specialfiler och flerhårdlänkade kvitto-/markörfiler avvisas genom
befintlig open_plain och kontroller av filidentitet. Datakatalogens identitet och
databasfilens metadataidentitet måste motsvara inbäddad pending-markör.
Databasen öppnas inte ens med ett extra vanligt filhandtag: det undviker att
stängning av en sådan descriptor frigör SQLite:s POSIX-lås.

Finns pending-markören måste dess exakta bytes överensstämma. Läsaren tillåter
som metadata att pending eller recovery redan saknas, för ett framtida
städningssteg. Det är inte ett godkännande av tom recovery-katalog och påverkar
inte nuvarande uppstartsspärr. Okänd recovery-katalogtyp eller I/O-fel avvisas.

## Isolerad publiceringsmotor

Testerna äger isolerade datakataloger och det befintliga processlåset. Motorn
kräver en befintlig, identitetsbunden pending-markör och bevarar den. Den använder:

1. Behållet kataloghandtag och exklusivt `openat` för privat temporärfil (0600).
2. Fullständig skrivning, flush och verifiering av exakt innehåll.
3. Befintlig obligatorisk filbarriär via `durability::sync_opened` (F_FULLFSYNC på macOS).
4. Ny kontroll av bindning och markerbytes.
5. `renameatx_np(RENAME_EXCL)` relativt samma kataloghandtag. Befintligt namn
   ersätts aldrig. Filsystem som inte stöder operationen ger fel, ingen fallback.
6. Identitetskontroll mot det behållna filhandtaget, fsync på datakatalogens
   handtag och slutlig F_FULLFSYNC på det publicerade filhandtaget.
7. Slutkontroll av filidentitet, kvittoinnehåll och ursprunglig markörbindning.

Temporärfilens RAII-städning gäller endast dess ursprungliga, privata namn med
matchande identitet. Vid lyckad namnpublicering avaktiveras den städningen före
nästa fallibla steg. Ett färdigt kvitto raderas ALDRIG av Drop eller felhantering.
Sena fel kan därför lämna ett komplett kvitto men ger aldrig ett framgångsresultat.
Misslyckad temporärstädning kan lämna en artefakt som framtida kod måste hantera
konservativt; ingen automatisk städning efter processkrasch införs nu.

macOS /dev/fd/N används INTE som katalogsökväg; kataloghandtagsrelativa systemanrop
används i stället. Det påverkar inte VFS-adapterns tidigare bindning av SQLite-
huvudfilen via /dev/fd/N.

## Tester och begränsningar

Isolerade tester täcker strikt format och storlek, exakta markerbytes, båda giltiga
utfallssträngarna, konflikt trots semantiskt likadan JSON, fel identitet, symlänk,
katalog, FIFO, hårdlänk, rättigheter, oförändrad journal och skyddskopia, bibehållen
generation/spärr, delvis skrivning och fel vid samtliga publiceringsgränser.
Två trådar möts vid en barriär före namnpublicering; exakt en lyckas och den
förlorande lämnar vinnarens läsbara kvitto orört. Katalogbyte kan inte dirigera
filskapandet till den nya katalogen.

Utöver felinjektion före barriärer kan synkoperationens eget returvärde ersättas
i den macOS-testisolerade publiceringsmotorn. Tester injicerar EIO från temporär-
filens synkning, datakatalogens synkning och den slutliga filbarriären och
kontrollerar att samma fel når anroparen, att senare steg uteblir och att en redan
publicerad fil bevaras. Övriga synkoperationer använder den riktiga funktionen.
Ingen sådan injektionsmekanism införs i produktionskod. Tester av felreturer
bevisar inte fysiskt strömavbrottsbeteende eller att lagringen respekterar flush. Faktisk
processkrasch med temporära kvarlevor återstår för senare steg. Parsern är inte
fuzztestad. Läsning och publicering stöds endast på macOS; Linux/Windows avvisas
med Unsupported utan filskapande. Ingen osäker fallback finns.

## Krav inför 4c.2b

- Koppla publicering till ett privat, konsumerande, operationsbundet bevis från
  aktuell Assessment under samma processlås och rätt uppstarts-/underhållsspärr.
- Lägg till startup-detektering av kvittot före databasöppning. NUVARANDE startup
  känner inte igen kvittot, vilket är säkert i detta steg eftersom ingen
  produktionskod kan skapa det; isolerade testkvitton får inte föras till appdata.
- Verifiera journal, VFS-bindning och skyddskopia på nytt från kvittots underlag.
- Hantera P/Q-konflikter och återupptagning utan att återta ett redan ägt processlås.
- Implementera separat beviskrävande markör-/katalogstädning och beständiga
  avslutningsbarriärer. Ingen sådan funktion finns i 4c.2a.

Backupformat v1, pending-format v1, skyddskopia och journaldata ändras inte.
Inget kvitto, ingen pending-markör eller recovery-katalog tas bort av detta steg.

## Beviskrävande publicering – 4c.2c.2

`CompletionProof::publish_receipt(self) -> Result<ReceiptDurable<'a>, ReceiptFailure<'a>>`
är första mutationssteget, inte en avsluts-/upplåsningsoperation. Både framgång
och fel äger hela det ursprungliga beviset: sessionens exklusiva reservation,
DatabaseState/låsägare, Maintenance när relevant, SQLite-lästransaktion,
VFS och behållna filhandtag. Typerna har privata fält och saknar Clone, Copy,
Serialize/Deserialize, fri konstruktion och metoder för driftövergång.
Ingen generation ändras. Släpp av resultat ger aldrig Ready.

En privat, lånad Authorization kan endast skapas inne i bevisets implementation.
Den återanvända publiceringsmotorn kan inte producera ett kvitto från fria bytes
i produktionsbygget. Testernas gamla publiceringshjälpare är fortsatt cfg(test).
Kvittots version och JSON-format ovan är oförändrade.

### Ordning för nytt Q

1. Återverifiera den kvarhållna observationen, operationsbindningen, P/Q,
   filidentiteter, VFS, lästransaktion, oförändrade journalbytes, skyddskopia och
   frånvaro av SQLite-sidofiler. Avbrott avvisas. Härled exakt pending_json och
   Old/New från beviset.
2. Begär journalens F_FULLFSYNC genom **befintligt skrivskyddat filhandtag**,
   fsync på behållen datakatalog och därefter journalens F_FULLFSYNC igen.
   Inget nytt filhandtag till journalen öppnas. Inga journalbytes skrivs.
3. Kontrollera Recovery-fas och aktiv sessionsreservation; återverifiera.
4. Skapa privat 0600-temporärfil med openat/O_EXCL/O_NOFOLLOW i datakatalogen.
   Skriv och verifiera kvittot, sedan obligatorisk F_FULLFSYNC.
5. Återverifiera omedelbart före namnpublicering. Endast motorns egen exakta
   temporärpost får undantas från kontrollen av okända temporärfiler; dess
   fysiska identitet kontrolleras av publiceraren. Övriga artefakter avvisas.
6. Publicera med renameatx_np(RENAME_EXCL), relativt behållet kataloghandtag.
   Befintlig målfil skrivs aldrig över. Ingen fallback finns.
7. Behåll den publicerade Q-filen i bevisets underlag, kontrollera bytes,
   identiteter och hela verifieringskedjan igen. Synka datakatalogen och utför
   slutlig F_FULLFSYNC på det behållna Q-handtaget. Slutkontroll före framgång.

### Befintligt Q

Återanvänd endast det Q-objekt som redan ingick i Assessment: samma filidentitet,
strikt format, exakta ursprungliga Q-bytes, restore-ID, exakta pending-bytes och
samma verifierade Old/New. Nytt innehåll eller ersatt objekt avvisas. Bekräfta
F_FULLFSYNC(Q), fsync(datakatalog), F_FULLFSYNC(Q) och hela underlaget på nytt.
Ett Q som dyker upp efter klassificeringen avvisas konservativt, även om dess
metadata skulle matcha; gör då en ny klassificering. Ingen överskrivning sker.

### Fel och krascher

ReceiptFailure visar typat verifierings-/kvitto-/beständighetsfel och
`may_have_receipt()`: true om beviset redan höll Q eller den egna publiceringen
kan ha nått slutnamnet. False garanterar **inte** att ingen annan aktör skapat Q.
Värdet är diagnostik, aldrig auktorisering. Alla normala fel returnerar bevisets
skydd. En komplett publicerad Q-fil raderas aldrig vid sena fel. Före publicering
städas bara motorns egen temporärpost när dess identitet fortfarande stämmer;
okända eller utbytta poster lämnas orörda.

Vid processkrasch kan en temporärfil eller ett komplett men ännu inte
beständighetsbekräftat Q finnas kvar. P och skyddskopian behålls alltid. Uppstart
förblir spärrad vid recovery-artefakter; inget autoavslut eller blind rollback
införs. Kraschtester dödar separata processer vid handskakade kontrollpunkter,
men bevisar inte strömavbrottsegenskaper hos lagringshårdvara. macOS måste kunna
utföra alla obligatoriska barriärer; fel ger ingen ReceiptDurable, ingen
best-effort-fallback. Linux och Windows avvisar fortsatt recovery och har inte
körtestats för detta steg.

### Verifiering och nästa steg

Isolerade tester omfattar Old/New, P+Q och Q utan P, bevarat innehåll/snapshot,
reservation, underhållslås och generation, varje synkfel, existerande/ändrat Q,
filbyten, avbrott, sena fel och subprocesskrascher. Kompilatortester kontrollerar
kapabiliteternas faktiska typdefinitioner med stubbar för I/O/verifiering;
runtime-testerna använder riktiga SQLite- och filsystemoperationer.

4c.2c.3 måste konsumera ReceiptDurable, upprepa slutkontroller före varje mutation
och implementera separat granskad, beständig städning. P, recovery-katalog och Q
tas inte bort i detta steg. Ingen produktionsrestore, SQLite-recovery,
generationsändring eller övergång till normal drift är aktiverad.

## Fortsättning i 4c.2c.3

ReceiptDurable kan nu konsumeras av remove_pending(). P och tom recovery kan
raderas beständigt medan Q och journalspärren behålls. Tidigare avsnitts uppgift
om att ingen städning finns avser 4c.2c.2. Se
[Recovery_Pending_Cleanup.md](Recovery_Pending_Cleanup.md) för det aktuella
kontraktet, felsteg och kraschmatris. Ingen Q-borttagning eller Ready-övergång
har införts.
