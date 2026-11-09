# MNO v0.1 – inventering inför skarp användning och release

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

**Ursprunglig inventering:** 2026-10-31. **Granskad main:** `8fe51b599f151c0c10476db4d78948376b810df2`, inklusive merge av PR #26. Dess spårade filträd är identiskt med den lokala granskade checkouten (`38cd8d7`). Den ursprungliga inventeringen var en kod-/dokumentgranskning, inte ett paketgodkännande.

**Omprövning 2026-10-31:** main `00fd24f14e49774f92ad787049c59f630fc02722`, inklusive PR #27 och #29. Ursprungliga avsnitt och ID A1–A4, B1–B6, C1–C4 behålls. Manuella resultat nedan är användarens uttryckliga acceptansbesked i uppdraget; denna omprövning kör inte om tester eller öppnar journaler. Äldre byggloggar som säger ”inte startad” beskriver förberedelsen före användarens senare tester.

**Ny releaseinriktning 2026-11-01:** main `6a46b3d942c4d5eacc82018b11ee91b10a55b4c6`, inklusive PR #32–#35. Enligt [D-019](../design/MNO_Design_Decisions.md#d-019--macos-först-i-v01-serien) är bara macOS obligatoriskt för första offentliga v0.1.0; Windows/Linux följer efter macOS inom serien. Äldre treplattformsbedömningar är historik, inte en kvarvarande spärr för macOS. A/B/C-ID behålls. PR #35:s CI och manuella DEV-acceptans är godkända enligt projektägaren. [Tillgänglighetsbedömningen](MNO_macOS_Accessibility_Assessment.md) är läsande; inga appfixar införs här.

## Aktuell status inför First Public Preview — 2026-11-03

Avstämning mot main `3f0764a` och projektägarens accepterade releasebeslut, utan
nya tester eller teknisk granskning. Denna status ersätter äldre framtidsformuleringar
nedan; daterade testresultat och historiska bedömningar bevaras.

- **B4/#12 klart:** PR #37 (AX-01 dialogfokus, AX-03 redigerarnamn), #38
  (AX-02 kontrast) och #39 (AX-04 zoom) är mergade och manuellt accepterade.
  Issue #12 är stängt. Se [slutlig bedömning](MNO_macOS_Accessibility_Assessment.md).
  Detta är avgränsad macOS-acceptans, inte fullständig WCAG-certifiering.
- **B6, beslut och dokumentation:** v0.1.0 blir **My Nature Observations v0.1.0 –
  First Public Preview**, markerad Pre-release på GitHub. Kostnadsfri MIT-källkod,
  lokal Tauri-DMG-byggning, ad hoc-signerad app, osignerad DMG, ingen notarisering
  och ingen automatisk updater. Teknisk baslinje macOS 13/arm64; manuellt testat
  macOS 27. macOS 13–26 har inte separat runtime-testats och krävs inte före v0.1.
  Se [miljö- och releaseguiden](MNO_Development_Production_Environments.md).
- Settings/Om MNO och #14 är avslutade; äldre formuleringar om väntande acceptans
  öppnar inte arbetet på nytt. Notisgenerering och paketerad resurskontroll är klara;
  slutlig distributionsartefakt måste fortfarande identifieras och stämmas av.
- OpenStreetMap är accepterat för v0.1: paketerad karta och attribution, även vid
  200% zoom, är manuellt verifierade. Faktiska HTTP-headers/cache har inte observerats;
  denna verifieringslucka är accepterad.
- **Open-Meteo:** Projektägaren bekräftade 2026-11-09 att frågan om
  användning av tjänsten är löst och att inget publiceringshinder kvarstår
  för v0.1. Befintlig käll- och licenshänvisning gäller fortsatt.
- **Kvar före publicering vid den ursprungliga avstämningen:** ge allmänheten åtkomst
  till repository/release (repositoryt var privat vid den kontrollen), och
  bekräfta slutlig DMG:s källrevision, signatur, notiser, SHA-256 samt nedladdnings-
  och installationsacceptans. Återanvänd dokumenterad acceptans; upprepa inte tester
  utan konkret anledning. Ingen tagg, publicering eller synlighetsändring görs här.

## 1. Sammanfattning

**Ursprungligen 4 A-fynd, 6 B-fynd och 4 C-grupper.** A1 är nu löst. B1/B2 är godkända för den testade macOS-miljön, B3 har en accepterad personlig rutin och B5 är implementerat och godkänt i paketerad acceptans. B4 är nu accepterat; kvarvarande offentlig distributionsavstämning under B6 beskrivs ovan. ID-numren räknas inte om.

**Milstolpe 1 – personlig macOS-användning:** inga nya kod-/dataskyddsblockerare är påvisade. Godkända RC-/startup-/miljötester och genomförda Production-byggförberedelser bevaras. Tidigare överlämningsnoteringar beskriver sin dåvarande fas och kräver inte att godkänt macOS-arbete görs om. Journaldata får inte flyttas eller återställas automatiskt.

**Milstolpe 2 – första offentliga v0.1.0 för macOS:** B4/#12 är avslutat med AX-01–AX-04 implementerade och manuellt accepterade; se aktuell status ovan. B6:s befintliga macOS-krav på ärlig OS-/CPU-baslinje, releaseguide, identifierad artefakt/notiser och vald distributionsväg kvarstår. A1, A4 och Settings-/notisleveransen är genomförda; ljust utseende är accepterat enligt D-017. Ingen Windows-/Linux-implementation krävs före denna publicering.

**Efter macOS-publiceringen:** A2 och övriga plattformars B1/B2/B6 verifieras i #11 före deras respektive publicering inom v0.1-serien. Ny plattform med samma källversion kan dela versionsnummer; ändrad kod/beroenden/leveranskonfiguration kräver lämplig patchrelease enligt D-019. Treplattformsmålet är inte borttaget, utan sekvenserat.

### Återstående prioritering per milstolpe

| Post | Personlig macOS-användning | Första offentliga macOS v0.1.0 | Nästa minsta åtgärd |
| --- | --- | --- | --- |
| A2 | Nej. | Nej enligt D-019; spärrarna gäller senare Windows/Linux. | Efter macOS: säkra plattformsekvivalenter och paket-/licens-/ikonacceptans i #11. |
| A3 | Nej. | Nej; nuvarande ljusa utseende accepterat för v0.1. | Post-v0.1 temaförbättring i #8; inget temaarbete krävs före release. |
| A4 | Klart. | Projektlicensen klar; tredjepartsleveransen kvarstår separat under B6. | MIT antaget, LICENSE/README/Rust/npm konsekventa; se D-018. Ingen tredjepart omlicensieras. |
| B1/B2, offentlig del | Godkända macOS-flöden står fast. | Kvarvarande macOS-distributionskontroll under B6; upprepa inte genomförd acceptans utan konkret anledning. | Windows/Linux verifieras senare per OS i #11. |
| B3 | Beslutad rutin; praktisk dokumentation/uppladdningskontroll återstår. | Dokumentation krävs, ingen automationsfunktion. | Skriv ned rutin och separat originalskydd i B6. |
| B4 | Avgränsad macOS-acceptans klar; ingen full WCAG-certifiering. | Klart, #12 stängt efter AX-01–AX-04. | Bevara godkända resultat; inget nytt tillgänglighetsarbete krävs inför v0.1. |
| B6 | Tidigare Production-/driftförberedelser bevaras; inga nya personliga krav införs. | Aktuell macOS-guide, OS/CPU-baslinje, artefakt och distributionsbeslut återstår att stämma av. | Återanvänd verifierade byggen/notiser och dokumentera vald macOS-leverans. |
| C1–C4, inklusive #28 | Nej. | Nej enligt nu beslutad avgränsning. | Låt ligga post-v0.1; ingen implementation nu. |

### Verifierade framsteg vid omprövningen

| ID / område | Status och evidens | Typ av besked |
| --- | --- | --- |
| A1 | PR #27 mergad: automatiskt DEV-ID/data, stabil Production-identitet, överlappsskydd och tester. Se [miljödokumentationen](MNO_Development_Production_Environments.md). | Implementerat och testat. |
| B1 | Isolerat optimerat macOS-paket installerat och startat från Finder. Skapa/redigera/spara/omstart/offline samt JPEG/PNG, bildtexter och editorial layout godkända. | Användarens genomförda manuella acceptans. |
| B2 | Backup/Restore och öppet Export/Import bevarar journalinnehållet i paketet. Manuellt appbyte bevarade gamla och nya anteckningar över omstart. | Användarens genomförda manuella acceptans. Appbytet var samma källversion (`7605dba`), inte en formatmigrering. |
| B3 | Manuell MNO-backup till användarvald extern molnlagring accepterad som personlig rutin. Automatisk backup krävs inte för v0.1. | Accepterat driftbeslut, inte ny implementation eller verifiering av molnleverantören. |
| B5 | PR #29 mergad. Ogiltig SQLite och verkligt otillgänglig datakatalog godkända i isolerat paketerat macOS-test. Se [B5-underlaget](MNO_B5_Startup_Error_Handling.md). | Implementation/tester samt användarens senare manuella dialog-/avslutsacceptans. |
| C2 / #28 | Restore/Import ska senare återgå till hem efter verifiering. | Uttryckligen post-v0.1 UX, inget känt dataskyddsfel. |

### Underlag och begränsningar

- **Ursprungligt underlag:** läste main, [MVP #2](https://github.com/antstan76/my-nature-observations/issues/2), samtliga fem öppna issues (#2, #8, #11, #12, #14) samt relevanta uppföljningskommentarer. Äldre planer har jämförts med kod, inte behandlats som nulägesbeskrivningar.
- **Projektbrädan kunde inte inventeras:** `gh project list` nekades eftersom anslutningen saknar `read:project`. Ingen behörighet ändrades. Publik webbläsning gav inget åtkomligt underlag; repositoryt är privat. Eventuella enbart brädlagda uppgifter återstår att stämma av.
- **Historisk CI vid ursprungsinventeringen:** main-[CI](https://github.com/antstan76/my-nature-observations/actions/runs/41217439588), för ovanstående SHA, var **completed/success**. Samtliga ordinarie kvalitetssteg var gröna. Ingen CI startades och inga tester eller appstarter kördes för denna inventering.
- **Nuvarande kontroll:** main-CI för `00fd24f` är [success](https://github.com/antstan76/my-nature-observations/actions/runs/41230838168). Öppna ärenden #2, #8, #11, #12, #14 och #28 granskades. Inga issues/checklistor ändrades; projektbrädans tidigare behörighetsbegränsning kvarstår som underlagslucka och provades inte om.
- Tidigare lokala foto-/Desktop-utredningar användes som kompletterande historik. De två fristående utredningsrapporterna är lokala ospårade filer, inte main-dokumentation. Ingen personlig datakatalog eller signeringsnyckel inspekterades.

## 2. Historisk avstämning mot MVP #2

| Krav | Faktiskt nuläge | Slutsats |
| --- | --- | --- |
| Inline-journal och formatering | TipTap/StarterKit i [JournalEditor](../../app/src/lib/components/JournalEditor.svelte), Rust-lagring i [journal.rs](../../app/src-tauri/src/journal.rs). | Implementerat; grundflödet manuellt godkänt i macOS-paket. |
| Autosparning, undo/redo, avslut | Serialiserad sparning, debounce, generationskontroll, navigerings-/avslutsskydd och fotodrafts. | Implementerat och testtäckt. |
| Upplevelsedatum och tekniska tidsstämplar | Separata databasfält och datumredigering. | Implementerat. |
| Tidslinje och kalender/månad/år | Journalvyer, datumväljare och historik med år/månader. | Implementerat. |
| Flera anteckningar/dag och frivillig rubrik | ID-baserade anteckningar och rubrikhärledning; datum är inte en unik anteckningsnyckel. | Implementerat. |
| Enkel sökning och historiska sammanställningar | [search](../../app/src/routes/search/+page.svelte), [history](../../app/src/routes/history/+page.svelte), [journal_search.rs](../../app/src-tauri/src/journal_search.rs), [journal_history.rs](../../app/src-tauri/src/journal_history.rs): textträffar, års-/månadsantal och navigering. | Omarkerad ruta är inaktuell för den grundläggande funktionen. Ingen avancerad artstatistik krävs. |
| Manuellt/frivilligt hämtat väder | Manuella värden, Open-Meteo-hämtning och sparade snapshots. | Implementerat; #2:s R1-kommentar bekräftar detta. |
| Utseende och framtida teman | Ljust gränssnitt finns och accepteras för v0.1. Systemstyrt ljus/mörkt utseende och temaval saknas ännu. | Tidigare MVP-krav omprövat 2026-10-31: A3/#8 är post-v0.1, inte en genomförd temaimplementation. |
| Svenska/engelska | Paraglide; [language.svelte.ts](../../app/src/lib/language.svelte.ts) uppdaterar även dokumentets `lang`. | Implementerat. Den specifika språk-attributpunkten i #12 är redan löst; helhetsgranskning återstår. |
| Backup, Restore, öppna exporter | Format 5 inklusive bilder, import och Recovery finns. | Implementerat och manuellt godkänt i macOS-paket, B2; andra OS återstår. |
| Tillgänglighet, felhantering, tester | Omfattande tester och många lokala fel/statusflöden finns. Övergripande tillgänglighetsgranskning återstår; B5:s två paketerade felscenarier är godkända. | Delvis klart; inte skäl att bygga en ny test-/felarkitektur. B4–B5, #12. |
| Installerbar macOS/Windows/Linux | Tauri-bundlekonfiguration finns; PR #34:s CI bygger och kontrollerar ett macOS-appaket. Windows/Linux har fortsatt plattformsbegränsningar. | macOS-RC/DEV godkända; första offentliggörandet gäller macOS. Windows/Linux följer i A2/#11 enligt D-019. |

#2:s avgränsning säger fortfarande att inbäddade fotografier ligger utanför MVP. Stage 2B finns nu i main och bör behållas och stabiliseras; detta innebär inte att fotobibliotek, gallerier eller mer bildfunktionalitet behöver byggas. Begränsad platsfunktion finns också redan, utan krav på en fullständig plats-/artmodul.

## 3. Dataskydd och vardagsdrift – befintlig grund

- **SQLite och sparning:** transaktioner, kataloglås/filidentitet, read-only verifiering av etablerad databas och gemensamma generations-/underhållsspärrar finns i [database.rs](../../app/src-tauri/src/database.rs), [startup](../../app/src-tauri/src/startup.rs), [database_state.rs](../../app/src-tauri/src/database_state.rs) och [samordningsdokumentationen](database-coordination.md). Det finns inte belägg här för ett nytt generellt lagringsproblem.
- **Dokument och foton:** dokument 1/2 stöds; schema 4 och proof/arkiv 5 bär journal, väder, platser och bildresurser. [photographs.rs](../../app/src-tauri/src/photographs.rs) sparar dokument/resurser atomärt och validerar referenser och preview. Original ändras aldrig och saknade original hindrar inte preview. JPEG/PNG-preview är högst 2500px och 4 MiB. Oanvända resurser behålls avsiktligt.
- **Kompatibilitet:** nya bygget läser äldre Small/Large och legacy Large på sidan utan lastningsskrivning. Äldre binärer kan däremot avvisa Medium. Startup accepterar aktuell databasschemaversion och kräver uppgradering för äldre syntetiska scheman. Detta är inte anledning att bygga utvecklingsdatabasmigreringar; frys och dokumentera den första skarpa baslinjen.
- **Portabilitet:** SQLite-backup, verifierad publicering utan överskrivning, skyddskopia före ersättning, export/import och startup-Recovery har omfattande teststöd; se [fotolagring](MNO_Photographs_Storage.md), [fototester](../../app/src-tauri/src/photographs/tests.rs), [Restore-kontext](../backup/Restore_Context.md). JSONL är auktoritativt; Markdown är en läsvy/fallback och ersätter inte maskinexporten.
- **Recovery:** tidigare respektive inkommande journal hanteras inom en verifierad ersättningsoperation, inte som en generell bokhanterare. Vanlig Restore och Import använder delvis olika bekräftelse-/navigeringsflöden. Det är inte i sig en dataregression; B2:s paketerade dataflöden är nu manuellt godkända. Hemnavigeringen efter Restore/Import är uppskjuten i [#28](https://github.com/antstan76/my-nature-observations/issues/28), inte ett återstående B2-blockerande fel. Manuell avsluta/öppna är en fungerande avsedd gräns tills ett säkert automatiskt restartflöde eventuellt införs.
- **Sekretess och storlek:** databas/arkiv är inte krypterade av MNO; export inkluderar originalens fullständiga sökvägar men inte originalfilerna. Preview är inte en fullupplöst originalbackup. Full verifiering i [storage_content.rs](../../app/src-tauri/src/storage_content.rs), `verified_snapshot_v5`, validerar/avkodar alla previews, även oanvända. Det motiverar mätning med representativa data, inte en obevisad optimering eller automatisk gallring.

## 4. A – måste åtgärdas före angiven milstolpe

Status efter temabeslutet 2026-10-31: A1 klart; A4 nu slutfört genom MIT-beslutet och projektlicensen. A2 återstår för senare Windows/Linux-publicering enligt D-019; tredjepartsnotiser och licensefterlevnad kvarstår separat under B6. A3 behåller sitt historiska ID men är nu uppskjuten förbättring, inte blockerare för någon milstolpe. Ursprungligen krävdes mörkt/systemstyrt tema inför offentlig v0.1; det kravet har uttryckligen omprövats. Se [D-017](../design/MNO_Design_Decisions.md#d-017--temasystem-efter-v01).

| ID / milstolpe | Fynd och evidens | Praktisk risk | Minsta åtgärd | Befintligt issue |
| --- | --- | --- | --- | --- |
| **A1 – klart för personlig macOS-användning** | Ursprungligen delad DEV/Production-identitet. Löst i [PR #27](https://github.com/antstan76/my-nature-observations/pull/27); separata ID/rötter och fail-closed-skydd i [environment.rs](../../app/src-tauri/src/environment.rs). | Den ursprungliga risken är åtgärdad och testad. | Ingen ny implementation. Behåll Production `org.mynatureobservations.app`, DEV `.dev` och nuvarande datarötter. Använd ett identifierat stabilt Production-paket för riktiga data. | PR #27; #11 närliggande. |
| **A2 – före senare Windows/Linux, inte första macOS v0.1.0** | Faktiska funktionsspärrar, inte bara saknade paket: [startup/established.rs](../../app/src-tauri/src/startup/established.rs), `require_supported`, avvisar Windows med `database_initialization_durability_unsupported`. [database_state.rs](../../app/src-tauri/src/database_state.rs), `recovery_session`, och startup-Recovery avvisar icke-macOS med `database_recovery_unsupported`. | Windows första journal kan inte etableras genom ordinarie väg; Linux/Windows kan inte lovas samma säkra ersättnings-/Recovery-funktion. Tauri som ramverk gör inte automatiskt appen plattformsfärdig. | Efter macOS-publiceringen: implementera/verifiera plattformsekvivalenter för lås, publicering och durability; behåll fail-closed-garantier. Testa sedan respektive nytt OS, native-licenser, ikoner och paket. Linux normaldrift kan fungera, men är inte därmed verifierad. | **#11**, sekvensering enligt D-019; #2:s tidigare samtidiga treplattformskrav är historik. |
| **A3 – post-v0.1, inte releaseblockerare** | **Beslut 2026-10-31:** befintligt ljust utseende accepteras för offentlig v0.1. Det tidigare kravet på mörkt/systemstyrt tema kvarstår endast som historik. | Ingen kvarvarande releasekonsekvens av att mörkt läge saknas. Tillgänglighet/kontrast i det levererade ljusa utseendet ska fortfarande verifieras under B4. | Efter v0.1: automatiskt systemutseende, manuellt utseendeval, naturinspirerade teman, gemensamma semantiska designtokens och konsekvent tillgänglighet/kontrast. Forest, Coast, Mountain och Night är möjliga arbetsnamn; namn, paletter och UX är preliminära. Ingen implementation nu. | **#8**, [D-017](../design/MNO_Design_Decisions.md#d-017--temasystem-efter-v01). |
| **A4 – slutfört: MIT-projektlicens** | Ursprungligen saknades projekt-LICENSE och licensbeskedet var inkonsekvent. **Beslut 2026-10-31:** standard-MIT i [LICENSE](../../LICENSE), copyright 2026 Stefan Antonsson, tydlig README och MIT i Rust/npm. Se [D-018](../design/MNO_Design_Decisions.md#d-018--mit-som-projektlicens). | Den ursprungliga oklarheten om projektlicensen är löst. Detta bekräftar inte färdig licensefterlevnad för varje distribuerad tredjepartskomponent. | Ingen ytterligare projektlicensimplementation krävs. Bevara tredjepartslicenser; paketerade licenstexter/notiser, eventuell MPL-källkodstillgång, tillgångars ursprung och distributionskrav ska hanteras under B6 före offentlig release. Ingen licensvy införs här. | **#2**, D-018; ingen dubblett behövs. |

## 5. B – historiska fynd och evidens (aktuell status ovan)

Godkännanden gäller testad macOS-miljö och angivna scenarier, inte alla OS eller framtida format. B4 gäller före macOS-publicering; B6 avser kvarvarande macOS-releaseförberedelser. Windows/Linux följer senare enligt D-019. Godkända personliga/macOS-flöden öppnas inte på nytt utan konkret avvikelse.

| ID / milstolpe | Fynd och evidens | Risk / minsta nästa åtgärd | Befintligt issue |
| --- | --- | --- | --- |
| **B1 – macOS-pilot verifierad; offentlig leverans återstår** | Finder-installation/start, offline, journal/foton och manuellt paketbyte är godkända av användaren i RC B1/B2. Byggunderlag: `7605dba`, arm64, version 0.1.0. | **Personligt:** ingen ny funktionsblockerare; förbered ett Production-paket från main med B5 och kontrollera identitet/version vid överlämning. **Offentligt:** välj OS-/CPU-baslinje och verifiera paket/installation samt Gatekeeper/distributionsväg; Windows/Linux efter A2. RC-paketens plist anger 10.13 men arm64-binären kräver 11.0; lova inte 10.13. | **#11**. |
| **B2 – macOS-flöden godkända; andra plattformar återstår** | Användaren har verifierat Backup/Restore, öppet Export/Import och bevarat journalinnehåll i paketerad macOS-app. Manuellt appbyte bevarade befintliga och nya poster efter omstart. | **Personligt:** ingen kvarvarande påvisad blockerare; behåll känd återläsningsbar backup. **Offentligt:** kör motsvarande plattformsspecifika dataflöden efter A2. Appbytet var en likvärdig version, inte bevis för framtida formatmigreringar. Påstå inte att varje avbrottsvariant manuellt provats; befintliga säkerhetstester kvarstår. #28 är uppskjuten UX. | **#11/#2**, UX **#28**. |
| **B3 – personlig rutin beslutad; kort dokumentation återstår** | Manuell MNO-backup till extern molnlagring är användarens accepterade rutin. Restore är godkänd under B2. | **Personligt:** ingen automatisk backup krävs. Dokumentera när kopian tas och bekräfta att den färdiga arkivfilen verkligen laddats upp; originalbilder kräver separat skydd om fullupplösta original ska bevaras. **Offentligt:** beskriv rutin, okrypterade arkiv och originalundantag i B6. Större datamängder/prestanda är uppföljning vid behov, ingen bevisad blockerare. | #2; inget särskilt rutinärende. |
| **B4 – före första offentliga macOS-release** | [#12](https://github.com/antstan76/my-nature-observations/issues/12): [bedömning 2026-11-01](MNO_macOS_Accessibility_Assessment.md) visar modalfokusbrist AX-01, textkontrast 3,23–3,29:1 för 13px kontrolltext AX-02 och saknat namn på journalens textbox AX-03. Befintliga labels, status/alert, native fotodialoger och dokumentets språkbyte bevaras. | Rätta de tre avgränsade bristerna. Verifiera sedan core-flöden med tangentbord/VoiceOver, formulärfel, panel-/kartfokus och förstoring. Potentiella risker är inte bevisade hinder; ingen full WCAG-efterlevnad påstås. | **#12**; ingen ny theme-/dialogarkitektur. |
| **B5 – klart i testad macOS-miljö** | [PR #29](https://github.com/antstan76/my-nature-observations/pull/29) visar tvåspråkig native-dialog utan råa sökvägar vid förväntat startfel. Journalen medges inte; stängning begär exit. Användaren har godkänt paketerade prov för ogiltig SQLite och nekad katalogåtkomst. | Den tidigare personliga releaseblockeraren är löst. Oförändrade validerings-, lås-, Recovery- och panic-skydd. Ingen ny Recovery/automatisk reparation behövs. Andra OS verifieras under A2/B1, inte genom att öppna denna macOS-acceptans igen. | PR #29; #12 närliggande. |
| **B6 – macOS-releaseguide och distribution** | Miljö-/B5-/byggunderlag och godkända RC-flöden finns. PR #32–#35 har levererat ikon, Settings/Om MNO och offline-notiser; PR #35 har godkänd CI och manuell DEV-acceptans enligt projektägaren. | **Personligt:** återöppna inte genomförda bygg-/driftförberedelser utan konkret anledning. **Första offentliggörandet:** aktuell macOS-guide, ärlig OS-/CPU-baslinje, vald distributions-/signeringsmodell, identifierad källversion, artefakt och checksummor/notiser. Windows/Linux/licenser/native-beroenden/ikoner verifieras senare separat. OSM/cache och capabilities/CSP är avgränsade kvarvarande granskningspunkter, inte bekräftade sårbarheter. | **#11**, D-019; #12 före macOS, #14 avslutat. |

### Produktion, dataplats och distribution

Förväntad macOS-plats från koden är `~/Library/Application Support/org.mynatureobservations.app/mno.sqlite`, med tillhörande appägda skydds-/Recovery-filer i samma datarot. Det är **inte** repo- eller appbundlekatalogen. Härledningen följer [Tauris appDataDir/dataDir](https://v2.tauri.app/reference/javascript/api/namespacepath/#appdatadir); RC-identiteten har testats separat; den slutliga personliga Production-installationens identitet/dataplats ska bekräftas vid driftöverlämningen utan att flytta befintliga filer. Byt inte identitet efter skarpa data utan en uttrycklig bevarandeplan.

Tauri kan paketera den statiska frontenden och Rust-kärnan i en vanlig [macOS-appbundle](https://v2.tauri.app/distribute/macos-application-bundle/); användaren behöver ingen utvecklingsserver. Detta har nu visats med RC-paket på användarens Mac; motsvarande löfte gäller ännu inte Windows/Linux.

**Egen lokalt byggd app och offentlig nedladdning är olika milstolpar.** Offentlig distribution behöver en beslutad och testad Gatekeeper-/signeringsväg. Developer ID och notarization är den normala vägen för verifierad extern macOS-distribution; ad-hoc är inte likvärdigt och kan kräva användarens manuella godkännande. Se [Tauris signeringsdokumentation](https://v2.tauri.app/distribute/sign/macos/). Ett betalt notariseringsflöde är inte i sig en förutsättning för den egna lokala piloten. Ingen signeringsidentitet/notariseringspipeline syns i repo; eventuella externa nycklar/miljöinställningar inventerades inte. App Store eller automatisk updater krävs inte för v0.1.

## 6. Kvalitetsläge och osäkerheter

- Main-CI för `00fd24f` är grön (länk i underlaget), inklusive översättningar, typkontroll, frontendtester, frontendbuild, Rust `--lib`, fyra namngivna integrationstargets och formattering. [testing.md](testing.md) beskriver exakt urval. Den bygger inte signerade releasepaket och testar inte Windows/Linux eller grafiska helhetsflöden.
- **Historik:** Stage 2B-[rapporten](MNO_Photographs_Stage_2B_Implementation_Report.md) anger 408 frontendtester och native WebKit-verifiering; tidigare fulla lokala Rustresultat är 900 godkända/2 ignorerade och 18 valda integrationstester. Dessa är **tidigare resultat**, inte nykörningar i inventeringen. CI-stegens status lästes, inte varje loggräknare på nytt.
- `managed_target_probe::reserved_lock_must_survive_closing_another_descriptor_bound_target` är ett dokumenterat felande historiskt modelltest som inte körs i CI. Det är inte nuvarande produktionsimplementation. Påstå inte att obegränsad `cargo test` är grön, men gör inte en isolerad modell till releaseblockerare utan produktionskoppling.
- Desktop-backupincidenten är inte ett fastställt publiceringsfel. Den tidigare isolerade native-matrisen gav fyra bestående, giltiga `.mno-backup`-filer. Om det återkommer: fånga faktisk picker-/destinationssökväg och omedelbar filexistens. Spekulativa fallbackmappar/namnändringar behövs inte.
- **Nyare tidigare testunderlag:** B5-implementationen körde 912 Rust-tester godkänt/2 ignorerade, 411 frontendtester och gröna typ-/format-/releasebyggkontroller. Det senaste isolerade acceptansbygget körde sju fokuserade B5-tester. Inga tester kördes om i denna statusuppgift.
- **Manuell acceptans uppdaterad:** användaren bekräftar nu även dialog och säkert avslut för båda B5-scenarierna. Äldre dokumenterade verktygsbegränsningar och ”manuellt kvar” i byggförberedelsen är historik, inte ett fortsatt B5-underkännande.
- Kvarvarande fokuserade frågor är offentlig OS-/CPU-baslinje, tillgänglighet och distributionsväg, faktiskt uppladdad molnbackup samt separat originalskydd. OSM/cache och projektbrädans eventuella extra uppgifter är inte fullständigt avstämda. Större fotojournalers prestanda följs vid observerat problem; inget sådant blockerande fel är rapporterat.
- Inget nytt olöst macOS-start-/dataskyddsfel är påvisat. Lyckat testbyte mellan likvärdiga byggen lovar inte framtida schema-/formatmigreringar. Dessa ska planeras först när ett verkligt formatbyte behövs, med personliga data som bevarandevärda.

## 7. C – uppskjutet; bygg inte detta före v0.1

1. **C1 – mer fotografifunktion:** drag-to-resize, gallerier/collage, HEIC/EXIF, avancerad layout, relänkning och automatisk orphan-gallring. Nuvarande previews och layout ska stabiliseras, inte byggas ut.
2. **C2 – extra översikts-/Recovery-UX:** thumbnails/utdrag, mer panelpolish, automatisk restartknapp och generell bokväxling. R2:s platsvisning och referensnavigation är redan levererade enligt #14; implementera dem inte igen. Korrekt verifiering och val vid ersättning hör däremot till B2 och bevaras. [#28](https://github.com/antstan76/my-nature-observations/issues/28), återgång till hem efter Restore/Import, är uttryckligen uppskjuten post-v0.1.
3. **C3 – nya domän-/automationsfunktioner:** artmodul/eBird, fullständiga platsmoduler, avancerad statistik/sökning, exakt väderklockslag (R2E4 uttryckligen uppskjutet), molnsynk, automatisk updater och schemalagd backupmotor. Manuell uppdatering/backup med dokumenterad rutin räcker; fungerande enkel sökning/historik ska inte utvidgas för att stänga #2.
4. **C4 – ny infrastruktur utan visat behov:** avancerade migreringar av syntetiska pre-v0.1-arkiv, nya generella jobb-/Recovery-ramverk, permanent revisionsmotor och full GUI-testmatris vid varje commit. En enkel reproducerbar RC-process räcker. Historiska prototypers städning kan hanteras separat, med fortsatt ärlig testredovisning.

**A3 utan omnumrering:** temasystemet ligger nu också utanför releasekritisk omfattning. Systemets ljus/mörkt, manuellt val, möjliga naturteman och semantiska tokens hanteras efter v0.1 i #8. Ingen ny C-numrering införs. Temanamn och UX är preliminära; B4:s tillgänglighetskrav på befintligt ljust utseende kvarstår.

## 8. Historisk releaseplan — ersatt av aktuell status 2026-11-03

1. **Behåll klart arbete:** A1, macOS B1/B2/B5, beslutad personlig backuprutin och PR #32–#35. #14 är avslutat; skapa inte nya Settings-funktioner eller generella ramverk för att återskapa godkännandet.
2. **Nästa minsta implementation – B4/AX-01:** rätta fokus i dialogen för osparade ändringar, utan att ändra save-/leave-/close-guardarnas beslut. AX-03:s lokaliserade namn för journaltextfältet kan ingå i samma lilla redigeraruppgift.
3. **B4/AX-02:** rätta mätt textkontrast i aktiva kontroller och relevant småtext i befintligt ljust utseende. Inget nytt tema eller tokenramverk krävs.
4. **Avgränsad macOS-acceptans i #12:** tangentbord, VoiceOver, formulärfel, redigerare/foton, platslistans koordinatalternativ och fokus vid panelstängning. Följ bedömningens checklista; gör bara ytterligare ändringar när ett hinder har konkret stöd.
5. **B6, macOS-publicering:** ange faktiskt verifierad OS-/CPU-baslinje och vald leverans-/signeringsmodell, färdigställ aktuell guide och identifiera artefakt, källversion, notiser och checksummor. Detta inför inte automatiskt krav på Intel/Universal, updater eller ny installationsarkitektur.
6. **Publicera macOS v0.1.0** när relevanta B4/B6-grindar är klara. Bevara oföränderliga källreferenser; ingen merge/publicering görs genom denna plan.
7. **Därefter Windows/Linux i #11:** lös A2 med befintliga säkerhetsgarantier och verifiera varje plattforms native-beroenden/licenser/notiser, ikoner, paket, installation och dataflöden. Samma källversion kan dela version; annan kod kräver lämplig patchversion. De blockerar inte steg 6.

## 9. Produktbeslut som återstår

- **Redan beslutat:** Production `org.mynatureobservations.app`, normalt appnamn My Nature Observations och befintlig Application Support-plats. DEV använder `.dev`. Ingen flyttbar bokhanterare eller omlokalisering behövs.
- **Redan beslutat:** manuell backup till extern molnlagring; ingen automatisk backup krävs för v0.1. Vid personlig driftöverlämning behöver bara praktiska detaljer skrivas ned: frekvens, vald destination, bekräftad uppladdning och separat skydd för originalbilder.
- **Kvar för offentlig release:** minsta OS-versioner och CPU-arkitekturer, distributionskanal/signering samt paketerade tredjepartsnotiser och licensefterlevnad. **Projektlicensen är beslutad: MIT**, dokumenterat i LICENSE och D-018; det tidigare öppna valet är avslutat. macOS gäller först enligt D-019; Windows/Linux ingår därefter i v0.1-serien och får inte blockera första macOS-publiceringen.
- **Kvar som avgränsad personlig driftdetalj:** vilken identifierad Production-build från main med B5 blir första skarpa baslinje? Befintliga RC-böcker är testdata och får inte automatiskt importeras. Från första riktiga anteckningen ska databevarande styra alla senare uppdateringar.

**Historisk slutstatus för föregående omprövning:** endast denna befintliga lokala inventering uppdaterades; struktur, identifierare och ursprunglig granskningsbas behölls. Ingen appkod/konfiguration ändrades, inga appstarter/tester kördes och inga commits, issues, PR:ar eller CI-körningar skapades. Befintliga ospårade utredningar bevarades.

**Senare A3-planeringsändring 2026-10-31:** det nya omfattningsbeslutet dokumenteras i denna inventering, D-017 samt relevanta specifikationsnoteringar. GitHub #8 omformuleras som post-v0.1; #2:s temapunkt anpassas utan att markera ett oimplementerat temasystem som färdigt. Ändringen lämnas som dokumentations-PR. Ingen appkod eller temafunktion ändras.

**Senare A4-planeringsändring 2026-10-31:** projektägaren har valt MIT. LICENSE, README och paketmetadata har stämts av; D-018 registrerar beslutet. A4 är slutfört med bibehållet ID. Tredjepartslicenser och tillgångar omlicensieras inte; deras paketerade notiser och övriga efterlevnad är fortsatt ett offentligt v0.1-krav under B6 och MVP #2. Ingen tredjepartslicensvy implementeras.

**Tredjepartsnotiser – macOS-implementation 2026-10-31 (B6, #14):** reproducerbar generering från låsta Cargo/npm-beroenden, faktisk klientinventering, versionsbundna granskade undantag samt verifierade lokala MPL-källarkiv har införts. Tauri paketerar offline-HTML och källarkiv; lokalt macOS arm64-paket kontrolleras mot genererat innehåll. Se [bygg- och granskningsguide](MNO_Third_Party_Notices.md). Projektlicensen A4 förblir MIT. Detta slutför macOS-notisartefaktens infrastruktur, inte About-vyn eller offentlig plattformsacceptans: Windows/Linux kräver egna beroende-/paketkontroller och About MNO i #14 återstår. Ursprungliga B6-krav och historik ovan behålls.

**Settings/Om MNO – 2026-11-01 (B6, #14):** Om MNO har implementerats med godkänd ikon, körande Tauri-version, oförändrad MIT-licenstext, GitHub-länk och läsbart kontrollerad offline-notisresurs via Tauris resurs-API och befintlig öppnare. Data/säkerhetsdisclosure har harmoniserats med språksektionen; platsväljarens fokusram har korrigerats med isolerad WebKit-verifiering. macOS-paketet deklarerar en/sv; systemdialogernas kontroller följer macOS programspråk, inte ett omedelbart Paraglide-byte. 426 frontendtester och 915 Rusttester passerade (2 befintliga ignorerade); arm64 DEV-paket och dess notisresurser verifierade utan installation/start av MNO. Granskning och paketerad manuell acceptans i båda språk återstår; Windows/Linux och övriga B6-krav förblir separata. Se [implementation och teststeg](MNO_Settings_About_MNO.md). Historiska A/B/C-ID och tidigare resultat behålls.

**Omprövning 2026-11-01:** D-019 ersätter samtidig treplattformsleverans. B4/#12 har nu en källbaserad [macOS-bedömning](MNO_macOS_Accessibility_Assessment.md) med prioriterade små åtgärder och manuell acceptans, utan påstående om WCAG-certifiering. Äldre ”manuell acceptans kvar” för PR #35 föregår projektägarens senare godkännande; de öppnar inte #14 på nytt. Ingen appkod, miljöidentitet, journal eller licenspolicy ändras i denna planeringsuppgift.
