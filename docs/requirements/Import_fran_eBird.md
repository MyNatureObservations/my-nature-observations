# Import från eBird — beslut och första skrivskyddade förhandsgranskning

Status: godkända produktbeslut; första implementationen är en macOS DEV-preview.
Bygger på publik main `2db4d2667808f632f71197994e45d227695d91c9` och ägarens
produktbeslut efter undersökningen 21 november 2026. Gränssnittet sparar ännu ingen
import. En separat intern lagringsgrund utvecklas enligt statuslänken nedan.

## Ägarskap och återimport

- Användaren importerar hela sin personliga export upprepade gånger. eBird är
  master och äger/låser källfälten. Besök identifieras av **eBird + Submission ID**,
  inte plats/datum, namn eller radnummer i filen.
- MNO:s journaltext, journalkopplingar och egna tillägg är separata och bevaras.
  Besök som saknas i en senare export raderas inte automatiskt.
- Ändrade eller försvunna källrader får inte kasta lokala tillägg eller tyst koppla
  dem till en annan taxonidentitet. Tilläggen bevaras för granskning.
- Utgångsläget är en separat källägd eBird-plats. Association till en befintlig
  MNO-plats är uttrycklig; källans plats-ID/namn/koordinatsnapshots skriver aldrig
  över den. Ingen namn-baserad automatisk sammanslagning.
- **X betyder ospecificerat antal**, inte noll eller ett. Taxa utan säker mappning
  behåller sina källnamn utan gissad katalogidentitet. Vetenskaplig namnlikhet är
  inte i sig en versionssäker taxonomisk mappning.
- Alla Macaulay-ID:n bevaras som mediareferenser, även flera per rad. Anta inte att
  varje referens är en bild. Första steget öppnar objektsidan externt; ingen
  automatisk hämtning, cache eller CDN-integration införs.
- Importerade fåglar tillhör samma [valbara fågelmodul](Egna_fagelobservationer.md#valbar-fågelmodul--uttryckligt-produktbeslut-20-november-2026)
  och samma observationstyp/kalenderfärg oberoende av källa. Källmärkning och
  originallänk är separata från modul-/typidentitet.

## Första DEV-steget: endast filgranskning

Via aktiverade Fågelobservationer → Observationer → Granska eBird-export (DEV).
Direktrutten och filkommandot kräver macOS DEV och aktiverad modul; ingen
produktions-/Windows-/Linux-aktivering. Funktionen är tydligt skild från
**Open Import**, som ersätter hela journalen från ett MNO-exportarkiv.

Förhandsgranskningen väljer och läser en UTF-8 CSV genom befintlig native-dialog.
Den parsar BOM, citerade kommatecken/citat/radbrytningar och utelämnade valfria
slutfält, grupperar på Submission ID, validerar obligatoriska uppgifter och
konsekvent besöksmetadata samt visar rad-/fältfel utan att tyst kasta eller ändra
rader. X, okända taxa och alla mediareferenser visas uttryckligt. Datum/tid är
källans lokala värden; saknad tid eller tidszon fylls inte i.

Översikten visar besök, källplatser, rader, ospecificerade antal, taxon utan exakt
vetenskaplig namnträff och mediareferenser. Ett besök kan öppnas med låsta
källfält. Källplatsen visas skild från en eventuell framtida MNO-association;
ingen sådan association föreslås eller sparas nu. Rapporten för en enskild
checklista avvisas med vägledning till fullständig MyEBirdData.csv.

**Ingenting sparas:** ingen databasåtkomst i filkommandot, ingen platsändring,
ID-reservation, schemauppgradering eller arkivändring. Avbryt/lämna innebär endast
att förhandsgranskningen stängs. Kontrollen gäller den valda filen; jämförelse mot
tidigare importer är ännu inte implementerad. Filer begränsas till 16 MB och
100 000 datarader; alla problem behålls, medan UI visar de första 100 felen.

## Acceptans

- Aktiverad macOS DEV-modul ger ingången; avstängd modul, andra plattformar och
  produktionsbyggen kan inte läsa en fil genom detta kommando.
- Syntetiska CSV-fall täcker quoting, BOM, radbrytningar, valfria slutfält,
  checklistgruppering, platsseparation och motstridig metadata.
- X bevaras; felaktiga antal/taxa ändras inte. Flera media-ID:n bevaras och
  länkar öppnas bara efter ett uttryckligt klick.
- Okända taxa visas med källnamn. Katalogträffar redovisas som namnträffar, utan
  att tillskriva källan AviList-version eller bekräftad konceptidentitet.
- Läsfel, valideringsfel och enchecklistrapport får naturliga sv/en-meddelanden.
  Avbruten dialog behåller eventuell befintlig preview; lämna och sena svar
  förändrar inte journalen eller modulvalet.
- Användarens personliga exporter kopieras inte till repot eller testfixturer.

## Senare, separat lagringsgrund

Inför beständiga källidentiteter/snapshots, importhistorik, platsbindningar och
separerade tillägg med skyddad migration och full arkivverifiering innan import
kan sparas. Definiera radidentitet och granskning över taxonomibyten; CSV:n ger
inte ett separat stabilt observations-ID. Bevara befintliga manuella fågellistor.
Återimportens diff/uppdatering, associationsskrivningar och eventuell mediahämtning
är senare steg. Ingen pluginplattform eller generell importmotor behövs.

## Intern lagringsgrund — ännu inte aktiverad

Se [skyddad eBird-lagring](../development/MNO_Ebird_Persistence_Foundation.md)
för schema-/arkivgränser, konservativa radkopplingsregler och verifiering.
CSV:n ger inga permanenta rad-ID:n eller en explicit taxonomiversion.
Oförändrade källrader matchas först. Därefter får en entydig en-till-en-koppling
inom samma checklista behålla radidentitet och MNO-tillägg när endast **Count**,
**Observation Details** eller **ML Catalog Numbers** ändras. Alla andra källfält
måste vara oförändrade. Regeln avser kontinuitet för en källrad, inte permanent
taxonidentitet. Varken vetenskapliga namn eller taxonomisk ordning är permanenta
ID:n, och taxonomiversion gissas inte.

Namnändringar, tvetydiga fall och andra ändringar hanteras konservativt:
originalrader och MNO-tillägg bevaras för granskning utan tyst omkoppling.
Borttagna rader behålls; saknade besök raderas inte. Befintlig strikt validering
för dubbletter och ogiltig indata kvarstår.
Importens sparande är fortfarande otillgängligt i UI och inga personliga exporter
ingår i testfixturerna. Nästa aktiveringssteg kräver separat granskning och acceptans.

Den vanliga macOS-starten använder nu den skyddade schema-13-uppgraderingen även
när fågelmodulen är avstängd. Pågående Recovery blockerar fortfarande uppgradering;
ett äldre återställt resultat uppgraderas vid nästa vanliga start efter verifierad
slutföring. Detta aktiverar inte eBird-importens sparande.
