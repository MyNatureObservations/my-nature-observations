# R1 – Places & Weather: aktuellt läge och kontraktsreferenser

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

R1 är implementerat på `feature/reconstruct-places-r1` till och med checkpoint
`af837e268183f148a29e879abc1287504010b8ee`. De centrala användarflödena har
verifierats manuellt i Tauri. Detta beskriver implementationen på grenen, inte
att den redan har integrerats i `main` eller publicerats.

## Genomförd omfattning

- Ett gemensamt platsregister med stabila UUID, namn, valfria koordinatpar och
  revisionsskydd. Skapande, redigering, lokal sökning och bekräftad radering finns
  i den gemensamma kartpanelen. Journalreferenser och standardplats skyddar mot
  radering; en aktiv standardplats får inte förlora sina koordinater.
- Journalens platskoppling bevarar namn, koordinater och registerrevision som
  historisk snapshot. En registerändring skriver inte om journaler. Uttryckligt
  återval av samma plats kan uppdatera den öppna journalens snapshot. Borttagna
  kopplingar behåller en revisionerad tombstone.
- En frivillig standardplats för väder i inställningarna. Den refererar till
  samma register och kan väljas, bytas och rensas. Den påverkar inte historiska
  journal- eller vädersnapshots och startar inga nätverksanrop.
- Separata manuella och hämtade väderuppgifter. Hämtning, förhandsgranskning,
  sparning och ersättning sker på uttryckligt initiativ. Dagens aktuella
  modellbild och de senaste avslutade modelldygnen kommer från ECMWF IFS;
  från sju dagars ålder används ERA5. Datumvalet följer API:ets lokala datum och
  tidszonsuppgifter. Framtida datum avvisas. Saknade värden förblir saknade.
- Väderplats kan skilja sig från observationsplats. Journalens snapshot föreslås
  när den har koordinater, annars används standardplats när journalplats saknas.
  En journalplats utan koordinater kräver ett uttryckligt annat väderplatsval.
- Databasgeneration, revisioner, sena svar och osparade utkast skyddas genom
  befintliga flöden. Hämtade snapshots innehåller plats-, käll- och tidsmetadata;
  ny hämtning eller ändrad standardplats uppdaterar aldrig tidigare väder automatiskt.
- Svenska och engelska texter via Paraglide. Open-Meteo, modell och CC BY 4.0
  anges i väderpresentationen.

## Aktuella versioner och auktoritativa kodreferenser

Tabellen är en vägvisare till befintliga kontrakt, inte en ny arkivspecifikation.

| Kontrakt | Aktuell version | Implementation och verifiering |
|---|---|---|
| SQLite-schema | **3** | [Databasstart](../../app/src-tauri/src/database.rs), [exakt schema-3-validering](../../app/src-tauri/src/database/weather_default_schema.rs), [plats-/snapshottabeller](../../app/src-tauri/src/database/place_schema.rs) |
| Innehållsbevis | **4** | [`verify_v4` och tester](../../app/src-tauri/src/storage_content.rs): journal, väder, register, journalplatser och standardplats inklusive tombstones |
| Fysisk backup | **4**, schema 3 | [Backup och arkivkontroll](../../app/src-tauri/src/backup.rs), [tester](../../app/src-tauri/src/backup/tests.rs) |
| Öppen export/import | **4**, schema 3 | [Export och manifest](../../app/src-tauri/src/open_export.rs), [format-4-kandidatbyggande i `v3.rs`](../../app/src-tauri/src/open_import/v3.rs), [rundtur och validering](../../app/src-tauri/src/open_export/tests.rs) |
| Restore-markör | **4** | [Aktivering, bindningar och Recovery](../../app/src-tauri/src/backup/restore_context/activation.rs) |
| Importens Recovery-markör | **5** | [Markörverifiering](../../app/src-tauri/src/backup/restore_context/activation.rs), [importaktivering](../../app/src-tauri/src/backup/restore_context/activation/importing.rs) |
| TipTap-dokument / väderformat | **1 / 1** | [Dokumentkontrakt](../../app/src-tauri/src/backup/restore.rs), [väderkontrakt](../../app/src-tauri/src/weather.rs) |

Nya databaser skapas direkt med schema 3. Äldre utvecklingsdatabaser migreras
inte automatiskt utan avvisas utan att skrivas om eller ersättas. Före v0.1 krävs
inte ytterligare stöd för äldre utvecklingsformat eller nedgradering. Kvarvarande
historisk kod innebär inte att dess format är tillgängliga i aktuella appflöden.

Backup använder en fysisk SQLite-kopia. Öppen export bevarar även
`places.jsonl`, `journal_places.jsonl` och `weather_default_place.jsonl` tillsammans
med journal och väder. Befintliga manifest-, kontrollsumme-, innehållsbevis- och
privata kandidatkontroller gäller före aktivering; Restore/import återanvänder
skyddsbackup, säkert filbyte och Recovery. Externa filer följs inte.

Äldre dokument behåller sin historiska betydelse:
[backupformat 1](../backup/MNO_Backup_Format_v1.md),
[öppen export 1](../export/MNO_Open_Export_v1.md) och
[väderlagring etapp 1](../weather/MNO_Weather_Storage_Stage_1.md).
De ska inte läsas som fullständiga specifikationer för dagens format 4.

Fler funktionsreferenser: [platsoperationer](../../app/src-tauri/src/place_registry.rs),
[journalens platsval](../../app/src/lib/components/EntryPlace.svelte),
[kartpanelen](../../app/src/lib/components/PlaceMapPanel.svelte),
[väderplatsförval](../../app/src/lib/weather-history.ts),
[väderförhandsgranskning och skyddad sparning](../../app/src-tauri/src/weather/historical/saving.rs)
och [val av aktuell/recent/historisk källa](../../app/src-tauri/src/weather/historical/recent.rs).
Se [testinstruktionen](testing.md) för aktuella kontroller och avgränsningar.

## Uppföljning av förbättringar utanför R1

Följande förbättringar återstod när R1 beskrevs ovan. De tre första är nu
levererade i R2; R1:s historiska omfattning och kontrakt är oförändrade.

- **Platsnamn i journalöversiktens metadatarad — levererat i R2E1:**
  [JournalReading](../../app/src/lib/components/JournalReading.svelte) visar
  anteckningens sparade platssnapshot, inte registrets aktuella namn.
- **Visa vilka journaler som använder en registerplats — levererat i R2E2:**
  dialogen vid blockerad radering visar refererande anteckningar och användning
  som standardplats för väder. Anteckningar kan öppnas via dialogen;
  backendens referensskydd och skydd för osparade journaländringar behålls.
- **Kartans världsvy och tile-fel — levererat i R2E3:** världsvyn anpassas till
  kartytan och tile-fel kan återhämtas genom ett uttryckligt nytt försök.
  Se [kartnotens historik och kvarstående osäkerhet](R1E2_Place_Map_Limitation.md).
- **Exakt klockslagsval — uppskjutet på obestämd tid:** val av exakta klockslag
  för journalanteckningar och historiskt väder har övervägts, men ingår inte i
  aktuell färdplan och medför inget implementationsåtagande. Den ursprungliga
  väderinriktningen i [D-013](../design/MNO_Design_Decisions.md#d-013--väderinformation-i-journalen)
  bevaras som historik. Dagens aktuella modellbild med giltighetstid är inte
  samma funktion.

## R2 – Journal & Places UX: slutförd implementation

R2E1–R2E3 ovan är implementerade på `feature/r2-journal-places-ux` genom
commits `a81ef94`, `675f578` och `8912037`. Användaren har slutfört manuell
Tauri-verifiering av de centrala flödena, inklusive världsvy, storleksändring
och tile-återhämtning efter rättningen för fraktionell zoom.

Slutkontrollen omfattade 355 frontendtester, i18n-kompilering, Svelte/TypeScript,
frontendbygge, Rust-bibliotekssviten och de fyra CI-valda integrationstargetarna
samt format- och diffkontroller. Två bibliotekstester krävde en separat omkörning
med tillåtelse att öppna lokala HTTP-testlyssnare; båda passerade. Befintliga
undantag enligt [testinstruktionen](testing.md) behölls.

R2 ändrar inte databasschema, innehållsbevis eller arkiv-/Recovery-kontrakt.
Paketerad WebKit/OSM-policyverifiering återstår enligt kartnoten; detta är en
operativ osäkerhet, inte ett konstaterat fel. Denna not beskriver färdig
implementation och verifiering, inte att R2 redan är integrerat i `main`.
