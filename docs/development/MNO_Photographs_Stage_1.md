# MNO – fotografier, etapp 1: verifierad bas och datakontrakt

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

Status: ursprungligt granskningsförslag, 2026-10-30. Efterföljande användarbeslut och den genomförda tekniska lagringsetappen dokumenteras i [MNO_Photographs_Storage.md](MNO_Photographs_Storage.md). Vid skillnader gäller den senare rapporten; förslagets 1280 px, 2 MiB, nodindex, kvoter och breda migrationsplan är inte genomförda krav.

## 1. Underlag och avgränsning

**Fastställda användarbeslut** återges i avsnitt 2. **Verifierade kodfakta** avser enbart nedanstående commit. Alla föreslagna tabeller, versionsnummer, gränser och layoutmått är **tekniska rekommendationer**, inte redan införda funktioner eller nya användarbeslut.

Repository: <https://github.com/antstan76/my-nature-observations>. `git ls-remote origin refs/heads/main` kontrollerades den 30 oktober 2026 och gav `1d2adc06c700ec84fc2937d1f4be955c7cbbccdb`, samma som överlämningens R2. En separat klon med detached HEAD på denna commit användes. Inga senare main-commits fanns vid kontrollen. Main måste kontrolleras igen före implementation.

Den ordinarie lokala kopian `~/Projects/my-nature-observations-journal` stod på `dfdd646d9a4807cbd4756a793b71d0958c296b5c`, med ospårade Recovery-prototyper, ett frontendtest och ett visionsdokument. Dessa lämnades orörda och användes inte som aktuell kod. Main innehåller därefter `0d74786` (väder 2B), `2ffd38a` (R1) och `1d2adc0` (R2). Äldre arbetskopior under projektets `work/` användes inte för slutsatser.

Projektets AGENTS.md har följts: `sources/` är skrivskyddad referens. Inga AGENTS.md hittades i den nya repositoryklonen. Aktuell [testing.md](testing.md) lästes. Inga personliga databaser öppnades. Ingen produktionskod, integration, merge, push eller PR har gjorts.

## 2. Fastställd upplevelse

JPEG och PNG stöds; HEIC väntar. Infogning sker vid markören mellan textstycken. Varje bild har vänster, höger eller egen rad samt liten eller stor storlek. Text flyter runt sidobilder när plats finns. Proportionerna bevaras utan beskärning och hörnen rundas diskret. Smala fönster ger egen rad utan ändring av sparat val. Läsning och redigering följer samma regler.

MNO sparar en mindre förhandsbild och en originalreferens. Externa original får aldrig ändras, flyttas eller raderas. Saknat original får inte ta bort journal, referens eller förhandsbild. Borttagning i journalen påverkar aldrig originalet. Förhandsbilder och referenser ska bevaras genom backup, export, import och Recovery.

Galleri, HEIC, avancerade bildtexter, användarstyrda EXIF-funktioner och synkronisering ingår inte. Återkoppling till flyttade original föreslås vänta.

## 3. Verifierade kodfynd och konsekvenser

Alla länkar är relativa till denna verifierade repositorykopia. Funktionsnamn anges där en fil har flera kontrakt.

| Flöde | Kodfakta | Konsekvens för fotografier |
|---|---|---|
| Editor | [JournalEditor.svelte](../../app/src/lib/components/JournalEditor.svelte), `onMount`, använder endast StarterKit, `enableContentCheck`, och accepterar redigering endast vid `documentVersion === 1` och `editable`. | Ny nod, schema och explicit versionsdispatch behövs. En bild får inte tyst falla bort vid `setContent`. |
| Läsvy | [journal-reading.ts](../../app/src/lib/journal-reading.ts), `readJournalDocument`, har StarterKit-schema och nodlista. [JournalContent.svelte](../../app/src/lib/components/JournalContent.svelte) renderar rekursivt med Svelte; [JournalReading.svelte](../../app/src/lib/components/JournalReading.svelte) ordnar artiklar. | Editorändring ensam räcker inte. Läsvyn behöver samma bildkontrakt och gemensamma layoutregler. |
| Validering | [journal.rs](../../app/src-tauri/src/journal.rs), `validate_document`, `save_journal_entry_with_connection`, `map_entry`; [restore/document.rs](../../app/src-tauri/src/backup/restore/document.rs) och [document-schema-v1.json](../../app/src-tauri/src/backup/document-schema-v1.json). Backend skyddar befintlig version och originaldokument i transaktionen. JSON har djup-/värdegränser och dubbla nycklar avvisas. | Behåll v1 oförändrad; lägg till separat v2-validator inklusive resursreferenser. Bevarandekapacitet och redigerbarhet måste hållas isär. |
| Autosparande | Editor väntar 2 s, köar sparningar och använder den exakta fångade JSON-strängen. Nya ändringar under skrivning förblir osparade. Återförsök sker efter 0/2/5 s. | Bildpaket måste fångas med samma dokumentrevision och återförsök vara idempotenta. |
| Navigation/session | Editor har `beforeNavigate`, stängningsskydd, discardflagga och timerstädning vid destroy. [journal-data.ts](../../app/src/lib/journal-data.ts) skyddar sena lässvar med requestnummer och backendgeneration. [journal.rs](../../app/src-tauri/src/journal.rs), `save_in_session`, tar `state.operation(Some(generation))`; [database_state.rs](../../app/src-tauri/src/database_state.rs) samordnar generation och underhåll. | Bildavkodning och sena resursläsningar behöver både editorns livstidskontroll och databasgeneration; generation ensam skyddar inte byte av anteckning. |
| Ångra | Editorn använder StarterKits historik; det finns ingen bildresurshantering. | Infogning, layout och borttagning måste vara dokumenttransaktioner. Resurser får inte rensas medan undo/redo kan nå dem. |
| Tomhet/titel | [journal-empty.ts](../../app/src/lib/journal-empty.ts) och motsvarigheten i Rust är konservativa: okänt innehåll är inte tomt. Titel tas från första icke-tomma H2, annars första text, högst ungefär 60 tecken. | Ren bildanteckning är innehåll; bildbyte får inte skapa titel från sökväg. Behåll textregeln, visa lokaliserad reservetikett när titel saknas utan att hitta på sparad text. |
| Sökning/historik | [journal_search.rs](../../app/src-tauri/src/journal_search.rs), `document_text`, extraherar text; `image` är redan undantaget från blockavskiljare men är inte en stödd bildfunktion. [journal_history.rs](../../app/src-tauri/src/journal_history.rs) är en metadata-/datumtidslinje utan dokumentrevisioner. | Ingen OCR eller sökning i originalnamn föreslås. Bildanteckning ska ändå synas i datum- och historikflöden. Historiken är inte ett återställningslager för borttagna bilder. |
| Databas | [database.rs](../../app/src-tauri/src/database.rs), `SCHEMA_VERSION = 3`; exakta schemakontroller i [database/](../../app/src-tauri/src/database/). Schema 0–3 och separat migration/aktivering förekommer. | Additiva tabeller kräver schema 4 och nya exakta schemakontroller; att endast skapa tabellen räcker inte. |
| Innehållsbevis | [storage_content.rs](../../app/src-tauri/src/storage_content.rs) har frysta bevis v2/v3 och aktuellt `verify_v4`/`snapshot_v4` för schema 3. `field`/`field_v3` accepterar inte BLOB. [backup/journal_content.rs](../../app/src-tauri/src/backup/journal_content.rs) har separat fryst journalbevis v1. | Nytt fullständigt bevis v5 måste omfatta bildbytes, metadata och länkar. Gamla journalbeviset ensamt bevisar inte bilder. |
| Backup/Restore | [backup.rs](../../app/src-tauri/src/backup.rs), manifest/validering; [backup/restore.rs](../../app/src-tauri/src/backup/restore.rs), kandidatkontroll; [manual_backup.rs](../../app/src-tauri/src/manual_backup.rs) och [restore_app.rs](../../app/src-tauri/src/restore_app.rs). Format 1–4 motsvarar schema 0–3. | SQLite-BLOB följer fysisk kopia, men schemavalidering, bevis, resursgränser och kandidatkontroll måste utökas innan stödet fungerar. |
| Öppna arkiv | [open_export.rs](../../app/src-tauri/src/open_export.rs), [open_import.rs](../../app/src-tauri/src/open_import.rs), [open_import/v3.rs](../../app/src-tauri/src/open_import/v3.rs). Aktuell export är format 4, schema 3, manifestets dokumentversion 1. JSONL är auktoritativt; Markdown är läsversion. Versionsspecifika filinventarier och kontroller är strikta. | Bildfiler och register kräver nytt arkivformat; extra filer får inte bara läggas till ett v4-arkiv. |
| Recovery | [backup/restore_context/activation.rs](../../app/src-tauri/src/backup/restore_context/activation.rs) har aktuell Restore-markör v4 och importmarkör v5, båda med bevis v4/schema 3. [pending.rs](../../app/src-tauri/src/backup/restore_context/pending.rs) och [pending/completion.rs](../../app/src-tauri/src/backup/restore_context/pending/completion.rs) har separata äldre v1-kontrakt. [recovery.rs](../../app/src-tauri/src/recovery.rs) och [startup/](../../app/src-tauri/src/startup/) deltar i admission. | Nya kontrakt måste nå både löpande och uppstarts-Recovery, inklusive skyddskopia, avbrott och slutbevis; ett namn som `activation-v1.json` är inte kontraktsversionen. |
| Plattform | `restore_app.rs`, `import_app.rs` och aktivering har macOS-grenar och avvisning på andra plattformar. Testinstruktionen anger macOS-CI och att Windows/Linux inte är verifierade. | Första leveransens verifierade plattform är macOS. Bildstöd ger inget nytt löfte om plattformsstöd. |

## 4. Rekommenderat datakontrakt

### Dokument och register

Inför dokumentversion 2 med atomär blocknod, endast som direkt barn till `doc` i första etappen:

```json
{"type":"journalImage","attrs":{"nodeId":"<uuid-v4>","assetId":"<uuid-v4>","placement":"left","size":"small","alt":""}}
```

`placement` är `left|right|row`, `size` är `small|large`. Alla attribut är obligatoriska, okända attribut avvisas vid redigering. `nodeId` är unik inom anteckningen; kopiering skapar nytt nod-ID. `assetId` identifierar oföränderlig resurs; flera noder i samma anteckning får dela den. Layout tillhör noden, inte filen. `alt` är enkel alternativtext (0–500 Unicode-tecken), utan formatering eller avancerad bildtext. Ingen URL, data-URI eller originalsökväg tillåts i noden. Bildmått hämtas från det validerade registret.

Föreslagna tabeller (slutlig DDL granskas i implementationen):

- `journal_image_assets`: `asset_id` PK, `entry_id` FK till journalen, `resource_version=1`, originalreferens, visningsnamn, originalets MIME, ursprungliga mått/byteantal, förhandsbildens MIME/mått/byteantal/SHA-256 och `preview_blob` BLOB, samt skapandetid. Använd CHECK/NOT NULL för typer, enum och gränser, och FK med kontrollerad kaskad vid radering av hela anteckningen.
- `journal_image_nodes`: sammansatt nyckel `(entry_id,node_id)`, `asset_id` med ägarskapskontroll. Detta är en transaktionellt härledd inventering av dokumentets bildnoder, inte en andra layoutkälla. Dokument och inventering måste stämma exakt.

Resurser ägs av en anteckning för enkel livscykel. Kopiering mellan anteckningar skapar en ny resursidentitet; ingen global deduplicering i denna etapp. Hash är integritetskontroll, inte identitet eller autentisering.

Originalreferens v1 föreslås vara en typad JSON-struktur med `kind=local-path`, `platform` och den exakta absoluta sökvägen som UTF-8, högst 16 KiB. Icke representerbara filsökvägar avvisas tydligt vid val. Referensen är bevarad metadata, aldrig en instruktion till import/Recovery att öppna eller skriva en fil. På annan dator visas den som otillgänglig tills användaren själv agerar; ingen automatisk sökning eller återkoppling. Externa filer öppnas endast för läsning vid uttryckligt filval. Ingen originalåtkomst krävs för normal läsning, backup eller återställning. Export innehåller sökvägen och måste upplysa om detta före delning.

### SQLite prövas som huvudval

Lagra begränsade förhandsbilder i SQLite. Det möjliggör en enda commit för text och resurser, fysisk backup utan sidokatalog och Recovery utan separat filpublicering. Kostnaderna är större databas/WAL, BLOB-avkodning och att befintliga bevis saknar BLOB-stöd. Behåll resursradens storlek långt under nuvarande SQLite-radbudget; hämta inte alla bilder med journaltexten. Ladda förhandsbilder vid behov med begränsad cache och frigör objekt-URL:er vid byte/destroy.

Före beslut ska syntetiskt prov med gränsfallen mäta minne, avkodning, autosparande, backup och Restore på macOS. Använd berörda rad-/SQLite-gränser explicit. Om detta misslyckas ska lagringsvalet tas upp igen; ett filbaserat alternativ får inte införas utan ett eget atomaritets- och Recovery-kontrakt.

### Sparande, borttagning och sena operationer

1. Fånga editorinstans, anteckningsidentitet (även lokal draft-ID), databasgeneration och ett monotont operationstoken vid filval. Läs, identifiera och avkoda begränsat i backend utanför skrivtransaktionen. JPEG-orientering normaliseras i förhandsbilden och metadata tas bort; originalet lämnas orört. PNG-alfa bevaras.
2. Returnera ett tillfälligt, generationsbundet resurstoken. Kontrollera editorns identitet och token igen efter varje väntan. Infoga nod och resurs i ett sammanhängande undo-steg först när förhandsbilden är färdig. Under väntan används en separat status, ingen halvfärdig sparbar nod. Navigation får avbryta väntan.
3. Spar-API tar dokumentversion, exakt dokumentrevision och de nya resurstoken som denna revision använder. Under befintlig normal-operation/generationsreservation: kontrollera nuvarande dokument, alla referenser, ägarskap och gränser; skriv nya BLOB-rader, härledd nodinventering och journal i samma SQLite-transaktion. Commit lyckas för allt eller inget. Återförsök med samma asset-ID och identiskt innehåll godtas; samma ID med andra bytes avvisas.
4. Bekräfta bara den sparade revisionen. Fortsatt redigering, ångra och nya bilder får inte fångas upp som redan sparade. Bildbusy/dirty måste ingå i editor-, navigations- och fönsterstängningsskyddet.
5. Vid borttagning försvinner noden och dess aktiva länk; resursen behålls tills undo/redo-sessionen avslutats. Autosparande får inte förstöra redo. Efter sessionen får en separat transaktion rensa endast resurser utan aktiv nod och utan levande sessionspinne. Avbruten process får lämna orphans, aldrig dangling aktiva länkar. Rensning ska kunna upprepas och kräver samma databasgeneration. Första versionen kan skjuta upp rensning helt hellre än att riskera dataförlust.
6. Även kvarhållna orphans ingår i bevis, backup och export för fullständig lagringsbevaring. Exportregistret markerar aktiva respektive kvarhållna resurser. Kvoter räknar båda. Opublicerade temporära resurser får rensas när token avbrutits/åldrats; de är inga sparade journaldata.
7. Vid navigation/destroy/discard ogiltigförklaras token och pending jobb avbryts. Varje callback kontrollerar livstid och generation före editorändring. Backend avvisar gamla resurstoken även om frontend gör fel. Vid databasbyte får inga jobb, cacheposter eller förhandsbildssvar användas i den nya sessionen. Bind cache till generation och asset-ID.

Saknat original är ett tillgänglighetstillstånd vid uttrycklig kontroll, inte en raderingsorsak. Visa förhandsbilden och meddelandet ”Originalet är inte tillgängligt / Original unavailable”. Ändra inte sparad referens när en kontroll misslyckas. Saknad eller skadad förhandsbild är däremot integritetsfel; bevara rådata och stoppa aktivering av en ogiltig importkandidat.

### Öppet arkiv och kontroll före aktivering

Föreslaget format 5 behåller journalens exakta `content_json` och radens `document_version` i `journal.jsonl`, tidigare väder-/platsfiler och läsversioner. Lägg till `images.jsonl` med registermetadata/originalreferenser och `image_nodes.jsonl` med nodinventering, samt `images/<assetId>.jpg|png`. Inga original kopieras eller följs. Markdown länkar till den relativa förhandsbilden och anger bildens ordning; dess layout är en approximation, JSON är auktoritativt.

Manifest v5 anger schema 4, högsta stödda dokumentkontrakt 2, resurskontrakt 1, exakta filnamn, byteantal, hash och radantal. Blanda v1- och v2-anteckningar via radens version; ändra inte äldre JSON-strängar. Förhandsbildsbytes exporteras utan omkodning och importeras byteidentiskt. För bevarande av skyddade okända dokument gäller tidigare rådatapolicy: import är inte tillstånd att normalisera eller göra dokument redigerbara. En resursreferens som inte kan verifieras får inte döljas med en tom ersättningsbild.

Före import/Restore: verifiera känt format/schema, exakt inventarium, unika namn och ID, inga traversal-/absoluta arkivsökvägar, symlänkar eller dubbla ZIP-poster; bibehåll nuvarande arkivsäkerhet och Stored-policy. Kontrollera faktiska lästa bytes mot deklarationer, hash, radantal, kvoter och avbrytning under strömläsning. Dekoder identifierar faktisk MIME, mått och pixelbudget, inte bara filändelse. Originalreferenser avkodas som inert metadata. Validera FK, dokumentversioner och exakt likhet mellan bildnoder och aktiva länkar; alla referenser måste ha rätt resursägare. Kandidaten måste klara SQLite-integritet, schemakontroll och nytt fullständigt innehållsbevis innan skyddskopia och aktivering. Efter publicering och vid Recovery verifieras samma bevis igen före Ready. Fel lämnar aktiv databas orörd och får inte utlösa partiell import.

## 5. Versions- och kompatibilitetsmatris

Nummer i förslagskolumnen är reserveringsförslag mot den granskade main; kontrollera kollisioner före implementation.

| Kontrakt | Verifierat nu | Rekommendation |
|---|---|---|
| Journalens dokument | v1, strikt schema utan bilder | Ny v2 för bildnod. V1-validator/fixture förblir fryst. Behåll gamla rader som v1; uppgradera en rad först vid första bildinfogning i samma commit. Återgå inte automatiskt till v1 när sista bilden tas bort. |
| SQLite `user_version` | 3; äldre 0–2 hanteras separat | 4, med kontrollerad migration och nya exakta schema-/innehållskontroller. Ingen tabelländring på originaldatabas innan migrationsskydd är klart. |
| Fullständigt bevis | `mno-preserved-content` v4; tidigare v2/v3 frysta | Ny v5 med egen domän, entydigt typ-/längdkodat BLOB-stöd och deterministisk ordning för hela bildinventariet. Behåll äldre verifierare byteidentiska. |
| Backup | Format 1–4 för schema 0–3 | Nytt format 5/schema 4. V1–4 läses med gamla regler och migreras till ny kandidat; inga bildtabeller uppfinns i gamla bevis. |
| Öppen export/import | Format 1–4, manifestets dokumentversion 1 | Format 5/schema 4/dokumentkapacitet 2; v1-rader kvar. Äldre stödda arkiv ska fortfarande accepteras via versionsspecifika läsare. Ingen nedexport som tappar bilder. |
| Bildresurs/originalreferens | Saknas | Separata kontrakt v1, för att kunna utvecklas utan att omtolka dokumentets layoutfält. |
| Databasaktivering | Restore-markör v4; importmarkör v5 i `activation.rs` | Nya Restore v5 och import v6, var och en i sin formatdomän, med schema 4/bevis 5 och explicit källformat. Tillåt äldre källscheman via separat verifierad migration, inte genom att vidga gammal parser. |
| Äldre journalbevis/pending/completion | Journalbevis v1, pending v1, completion v1 är separata protokoll | Oförändrade för sina gamla flöden. Led aldrig bilddatabaser genom en v1-gren som endast bevisar journaltext. Ny aktiveringskedja ska bevisa bilddata även vid slutkontroll. |
| Generation, väder-/platskontrakt | Egna befintliga kontrakt | Ingen automatisk höjning. Lägg generationsbundna bildtoken ovanpå befintlig admission; ändra bara version om det faktiska kontraktet ändras. |

Äldre appversioner ska tydligt avvisa nytt schema/arkiv utan skrivning. Ny app måste bevara äldre skyddade journaler och stödja gamla arkiv enligt tidigare kontrakt. Kompatibilitet betyder inte att alla råa dokument blir redigerbara. Vid migration till schema 4 ska även nuvarande schema 1–3 ha en verifierad väg; äldre `migration.rs` för schema 0 är inte i sig bevis för detta.

## 6. Föreslagna resursgränser

Gränserna är nya produktgränser för bildfunktionen, inte retroaktiva begränsningar för gamla textarkiv. MiB/GiB avser binära enheter.

| Resurs | Rekommenderad hård gräns och beteende |
|---|---|
| Indata | 32 MiB per fil; JPEG/PNG identifieras från bytes. Högst 40 miljoner avkodade pixlar och 32 768 px per sida, kontrollerat före full avkodning. Avvisa animerad PNG i första etappen. |
| Avkodning | En bild åt gången; högst 256 MiB arbetsbudget per jobb inklusive buffertar. Avbryt utan dokumentändring om budget inte kan hållas. |
| Förhandsbild | Längsta sida högst 1 280 px, ingen uppskalning, högst 2 MiB lagrat. JPEG för JPEG, PNG med alfa för PNG. Anpassa kvalitet/mått nedåt för att möta bytegränsen; om det inte går, avvisa med begripligt fel. |
| Infogningsoperation | Högst 10 bilder, 64 MiB sammanlagd indata och 20 MiB färdiga förhandsbilder. Hela valda batchen förbereds innan ett undo-steg; misslyckad bild avbryter batchen. |
| Anteckning | Högst 100 bildnoder och 100 resurser inklusive kvarhållna orphans, högst 200 MiB lagrade förhandsbilder. Befintliga dokumentgränser gäller separat. |
| Arkiv/import/Restore | Högst 10 000 bildresurser, 100 000 aktiva bildnoder och 2 GiB faktiska förhandsbildsbytes. Högst 4 GiB hela bildformatarkivet och dess sammanlagda okomprimerade innehåll. Strömmande summering, överflödessäker aritmetik och samma budget före och under extraktion. |
| Register | Högst 32 KiB metadata per resurs; originalreferens högst 16 KiB, visningsnamn högst 1 KiB. Manifest v5 högst 4 MiB, inte tidigare läsares 16 KiB. |

Arkivgränser gäller också producenten: varna och stoppa ny bildinfogning innan lagrad bildmängd överskrider vad fullständig backup/export kan bevara. Ingen automatisk radering eller ofullständig export vid gräns. Total arkivstorlek omfattar även text/plats/väder: en redan stor äldre bok måste fortfarande kunna bevaras med sitt äldre kontrakt; inför inte bilder förrän en fullständig v5-backup ryms. Gränsbeslut behöver syntetiska mätningar; 40 MP tillåter inte obegränsade dekoderallokeringar.

## 7. Layout och tillgänglighet – rekommendation

Infoga vid markören genom att dela aktuellt textstycke. Om markören ligger i rubrik, lista, citat eller kodblock placeras bilden efter det omslutande toppnivåblocket, med en kort status som förklarar placeringen. Första etappen tillåter inte bilder inuti dessa strukturer. Bevara markering via editorbookmark under förberedelsen; om användaren fortsätter skriva mappas den genom transaktionerna. Om markeringen inte längre är giltig avbryts infogningen utan att gissa plats.

Liten har maximal bredd 240 CSS-px; stor 480 CSS-px. Båda har `max-width:100%`, automatisk höjd, 6 px hörnradie och 16 px mellanrum till text. Storlek ändrar presentation, inte lagrade bytes. Text får flyta om minst 280 px textbredd återstår inklusive mellanrummet. Mät journalens innehållsyta, inte hela fönstret; under 640 px innehållsbredd visas alla bilder på egen rad. Ingen transaktion eller attrs-ändring görs vid responsivt byte.

Varje ny bild rensar föregående float; flera bilder i följd staplas i dokumentordning, ingen implicit mosaik. Egen rad rensar båda sidor. Nästa rubrik, lista, citat, kodblock och horisontella linje rensar float och börjar under bilden. Vanliga textstycken får flyta; om texten är kort fortsätter nästa vanliga stycke bredvid tills bilden tar slut. Artikelbehållaren etablerar eget flöde så bilden inte hamnar i nästa anteckning. Läsare och editor delar samma CSS-regler; NodeView-kontroller får inte påverka bildens mått.

| Svenska kontrollnamn | English |
|---|---|
| Infoga fotografi | Insert photograph |
| Bildplacering; Vänster; Höger; Egen rad | Image placement; Left; Right; Own row |
| Bildstorlek; Liten; Stor | Image size; Small; Large |
| Alternativtext | Alternative text |
| Ta bort ur journalen | Remove from journal |
| Förbereder fotografi; Avbryt | Preparing photograph; Cancel |

Använd namngivna knappar/radiogrupper med synligt valt läge. Tab når infogningsknapp och bildkontroller; Enter/Space aktiverar; Escape stänger kontroller och återför fokus. Bilden går att nodmarkera med piltangenter; Delete/Backspace tar endast bort markerad nod i ett undo-steg. Undo/redo använder plattformens vanliga genvägar. Efter borttagning återgår fokus till logisk textposition. Status för förberedelse/fel läses via live-region. Tom alternativtext ger ett neutralt lokaliserat tillgängligt namn ”Fotografi / Photograph”; inget filnamn eller sökväg läses upp automatiskt.

## 8. Kvarstående användarbeslut

Följande kräver ställningstagande före produktimplementation, men hindrar inte granskning av detta förslag:

1. Godkänna SQLite-BLOB som huvudval efter mätprovet, samt föreslagna storleks-/kvotgränser och reducering av förhandsbild för att klara 2 MiB.
2. Godkänna att originalreferensens fulla sökväg bevaras även i öppet exportarkiv med tydlig information vid delning; portabiliteten blir begränsad till förhandsbilden på andra datorer.
3. Godkänna enkel alternativtext, bildplacering efter omslutande block i listor/rubriker och att följdbilder staplas.
4. Godkänna senarelagd återkoppling till flyttade original och uppskjuten säker resursrensning i första implementationen. Det senare kan ge växande databas efter många bildbyten, inom kvoterna.

Ingen av punkterna ändrar de fastställda kraven om originalskydd, responsiv layout eller fullständig databevaring.

## 9. Nästa sammanhängande implementationetapp

Föreslå en etapp med intern ordning: **datakontrakt och bevarande först, UI-aktivering sist**. Ingen synlig infogningsfunktion slås på innan hela kedjan verifierats.

Omfattning: macOS, JPEG/PNG, filval, begränsad backendavkodning, SQLite-resurser, dokument v2, sparande och undo/redo, gemensam editor-/läslayout, enkel alternativtext, versionerad migration, backup/export/import/Restore/Recovery med fullständigt bevis. Ingen bildredigering, drag-and-drop, clipboardimport, galleri, HEIC, OCR, avancerad bildtext, originalflytt, automatiskt relänkande eller synkronisering. Säker rensning kan vänta; ingen osäker automatisk radering införs.

Berörda områden är editor/läsare/tomhet och journaltyper; `journal.rs`, databas/schema/migration, nytt bildmodul/API och Tauri-registrering i `lib.rs`; `storage_content.rs`; backup, open_export/open_import inklusive versionerade läsare; skyddskopia, aktivering, löpande och uppstarts-Recovery; språkfiler, gemensam layout, tester och utvecklingsdokumentation. Sökning och historik ska få nödvändiga kompatibilitetskontroller men ingen ny bildsökfunktion.

### Nödvändiga tester

- Gemensamma syntetiska fixtures för v1/v2, bild-only, felaktiga enum/ID/ägare, dubbletter och okända dokument. V1 ska varken omtolkas eller normaliseras.
- SQLite-test med fel före/under commit: text+resurs+länk är allt eller inget. Idempotenta återförsök, undo/redo över autosave, borttagning och krasch före senare rensning. Originalfiler kontrolleras byteidentiska och kvar på samma plats.
- Fördröjda avkodnings-/läs-/sparsvar efter anteckningsbyte, discard, destroy, Restore och databasbyte. Gamla generationer och token får aldrig ändra ny session.
- Gränstester vid och över varje kvot, trasig JPEG/PNG, falsk MIME, överstor pixelheader, alfa och orientering; mätningar för minnesbudget och maxarkiv utan att allt materialiseras samtidigt.
- Byteidentisk tur och retur av förhandsbilder, originalreferenser, v1/v2-JSON, väder och platser genom backup/Restore och export/import. Saknat original ska inte påverka resultatet. Äldre stödda format 1–4 testas separat.
- Skadade bildbytes ska ändra beviset och stoppa aktivering; testa BLOB-typ, inventarium, FK, hash, traversal, dubbel ZIP-post och deklarerade kontra faktiska storlekar.
- Isolerade subprocess-/tempdatabastester för avbrott före/efter kandidatpublicering och vid Recovery: både löpande och uppstart, gammal/ny databas samt skyddskopia. Ready kräver bevis som omfattar bilder.

Följ aktuell `testing.md`: frontend `npm ci`, `npm run i18n:compile`, `npm run check`, `npm test`, `npm run build`; Rust `cargo test --locked --lib`, namngivna integrationstester och `cargo fmt --all -- --check`. Nya frontendtestfiler läggs explicit i testskriptet. Den historiska managed-target-proben är uttryckligen exkluderad enligt testinstruktionen; påstå inte att obegränsad `cargo test` passerar.

### Manuella acceptansscenarier och färdigkriterier

1. Syntetisk ny anteckning: infoga bild mitt i stycke, skriv vidare, ändra layout, spara, stäng och öppna. Bild-only ska visas i journal, datum och historik utan att bli tom.
2. Alla sex layoutkombinationer i editor/läsare, kort/lång text, följdbilder, listor och rubriker. Växla bred/smal innehållsyta och tillbaka; sparade attrs ska vara oförändrade. Proportioner och alfa bevaras.
3. Genomför hela flödet med tangentbord och skärmläsare, på svenska och engelska. Kontrollera fokus, nodborttagning, status och undo/redo efter autosave.
4. Ta bort/flytta ett syntetiskt original utanför MNO. Förhandsbild och referens finns kvar. Ta sedan bort noden och ångra/gör om; originalets bytes/plats ändras aldrig av MNO.
5. Exportera och återställ i separat syntetisk profil utan originalfiler. Gör samma sak med backup och med avbruten aktivering följd av Recovery. Alla förhandsbilder och referenser jämförs maskinellt före/efter.
6. Avbryt förberedelse, navigera och byt databas medan operationer är fördröjda; ingen bild hamnar i fel anteckning. Simulera skrivfel och skadat arkiv utan påverkan på aktiv databas.

Etappen är färdig när ovanstående scenarier och relevanta automatiska tester är genomförda med redovisade resultat, migrations-/versionsmatrisen har motsvarande fixtures, mätprovet stöder gränserna och originalskyddet är verifierat. Bild-UI får aktiveras först när hela bevarandekedjan passerar. Kvarstående plattformsbegränsningar ska redovisas.

## 10. Verifiering utförd i denna dokumentationsetapp

Utfört: läsning av aktuell GitHub-main, separat checkout och commitkontroll, jämförelse mot äldre lokala main utan att ändra den, instruktioner/testdokumentation samt riktad statisk kodläsning av filerna ovan. Dokumentets interna fillänkar och whitespace kontrollerades. Detta är ingen körd demonstration av ett bildkontrakt; lagrings- och gränsförslagen återstår att prova.

Inga apptester har körts eftersom endast ett granskningsdokument tillkommit och aktuella instruktioner inte kräver full appsvit för detta. Inga gamla testresultat åberopas som nya. Ingen CI-körning krävs eller startades för denna lokala dokumentleverans; ingen PR skapades.
