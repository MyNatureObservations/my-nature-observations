# Gemensam låskapabilitet och spärrad recovery-session – 4c.2b.1

## Befintligt beteende som bevaras

Ingen klassificering, städning eller upplåsning körs automatiskt. Normal första
initialisering, initialiseringsmarkeringen, öppning utan CREATE, schemavalidering,
generationsskydd och backupfunktioner är oförändrade. Kopieringsmotorn förblir
cfg(test), och kvittopubliceraren förblir macOS-testisolerad.

## DirectoryLease

startup::DirectoryLease är den enda processlåsimplementeringen för uppstart och
klassificering. Den tidigare separata ProcessLock i klassificeraren har tagits bort.
Leasen äger låsfilshandtaget, kataloghandtaget, den kanoniska rotplatsen och fakta
om första skapandet. Identiteten bevaras genom de behållna handtagen och jämförs
mot aktuella katalog-/låsfilobjekt vid ensure_current(). Ingen Clone eller fri
unlock-metod exponeras, och databasplatsen härleds alltid som rot/mno.sqlite.

Privat acquire används av uppstarten. open_existing används av den befintliga
fristående klassificeraren och skapar varken saknad katalog, låsfil eller databas.
Båda vägarna använder samma säkerhetskontroller och fs2-låsanrop. Som tidigare
frigör Drop OS-låset utan att ta bort låsfilen; en processkrasch lämnar inget
permanent OS-lås. Detta är samordning mellan protokollföljande processer, inte
ett skydd mot godtycklig manipulation av samma användares filer eller processminne.

## Uppstartsägarskap

startup::begin() returnerar internt StartupOutcome::Ready eller ::Recovery.
Båda varianterna behåller samma DirectoryLease som togs innan kontrollpunkten.
RecoveryGate kan bara konstrueras inne i startup och innebär aldrig godkänd drift.
Kontrollpunkten letar efter både recovery och .mno-restore-completion-v1.json
med symlink_metadata. All förekomst blockerar, oavsett format, filtyp eller om
recovery-katalogen är tom. Den tolkar inte kvittot. I/O-fel räknas inte som frånvaro.
Kontrollen sker före initialiseringsmarkering, SQLite-open och schemainitialisering.

DatabaseState behåller sedan 4c.2c.1 hela StartupOutcome i en enda privat
OnceLock. Den är den gemensamma ägarplatsen för DirectoryLease oavsett
uppstartsutfall. RecoveryGate/StartupReady beskriver det oföränderliga
uppstartsbeslutet, inte aktuell driftfas. Vid recovery returnerar
initialize_startup() fortfarande fel. Normal operation och maintenance förblir spärrade. Uppstart kan
inte provas om mot en annan katalog i samma state. Ägaren lever till state släpps,
så både diagnostik och framtida recovery kan låna den utan unlock/relock-glapp.

De gamla initialize/initialize_with-hjälparna finns endast under cfg(test) som
adaptrar för tidigare tester; de kan släppa en recovery-gate när de returnerar fel.
Produktionsvägen är DatabaseState -> begin och behåller däremot gaten. Nya tester
använder verkligt DatabaseState-/begin-flöde för att verifiera denna skillnad.
Övriga fatala uppstartsfel ger fortfarande ingen StartupReady och kan frigöra
låset när uppstarten avbryts, enligt tidigare beteende.

## RecoverySession

RecoverySession<'a> ligger i database_state::recovery_session. Konstruktorerna
är privata för database_state; inga godtyckliga kombinationer av state, katalog,
databas och lås kan lämnas till dess publika interna fabriker:

- DatabaseState::recovery_session() lånar just detta DatabaseState och dess
  gemensamma låsägare efter recovery-uppstart.
- Maintenance::recovery_session() lånar just detta guards befintliga lease och
  skrivlås från dess eget DatabaseState. Den tar varken nytt processlås eller nytt
  Maintenance-lås och aktiverar den bestående processlokala recovery-spärren.
  Denna väg är ännu inte ansluten till produktionsflödet.

Sessionen är inte Clone och har ingen Deserialize, fri konstruktor eller
upplåsningsmetod. Den innehåller lånade kapabiliteter, ett processlokalt
sessions-ID och sedan 4c.2c.1 en exklusiv sessionsreservation. database_path/ensure_current återkontrollerar bindningen; den
körande varianten kan diagnostiskt visa befintligt underhålls-ID och generation.
Sessions-ID är inte en pending-markörs restore_id och är inte ett verifieringsbevis.

Drop avslutar lånen och frigör endast sessionsplatsen. DatabaseState behåller
processlåset i sin enda ägarplats.
I runtime-fallet förblir recovery_required satt även efter att session och sedan
Maintenance har släppts. Ingen generation ändras. Ägaren eller Maintenance kan
inte släppas medan sessionen fortfarande används, vilket kompilatortester bekräftar.

Låsordningen förblir processlås -> uppstarts-/underhållsspärr -> filhandtag ->
SQLite -> verifiering. Sessionen själv öppnar inte SQLite eller journalfilen. Den binder till katalogen
och den därifrån härledda databasplatsen, inte till verifierat journalinnehåll
eller en kvarhållen fysisk SQLite-filidentitet; detta tillkommer i klassificeringen.
Alla getters är diagnostik, inte auktorisering.

## Tester och plattformar

Isolerade tester täcker artefaktkombinationer, trasigt/endast kvitto, osäkra
kvittofiltyper, I/O-fel, ingen databasinitialisering, bevarad befintlig journal,
engångsuppstart, katalog-/låsfilbyten och rena open_existing-fel utan filskapande.
Runtime-testet lånar det redan hållna underhållslåset och kontrollerar bibehållen
generation/spärr. Separata processer verifierar att låset förblir taget efter
session-drop och att krasch frigör det. Handshake ersätter tidsbaserade gissningar.
Kompilatortester inkluderar den faktiska sessionstypen, med små stubbar för ägare
/UUID, samt en positiv kontroll och negativa fall för konstruktion, Clone,
ägarsläpp och överlevnad utanför lånets livstid. De ersätter inte runtime-testerna
för att fabrikerna väljer rätt verkliga lease/state.

macOS har körtestats. Sessionstillverkning och klassificering avvisas på Linux
/Windows med Unsupported; kvitto-/recovery-förekomst blockerar ändå uppstart på
alla plattformar. Ordinarie plattformsspecifika låskontroller har inte försvagats.
Windows/Linux kräver egna körtester. Inga garantier tillkommer för trasiga eller
icke-stödda filsystems lås- och beständighetssemantik.

## Återstår för 4c.2b.2 och senare

4c.2b.2 kopplar klassificeringen till sessionen enligt avsnittet nedan.
4c.2b.3 inför CompletionProof enligt nedan. Ingen produktionspublicering,
städning, upplåsning eller generationsändring har införts.

Framtida övergång till normal drift måste behålla leasen i samma OnceLock.
Ingen övergång från recovery till normal drift finns ännu; uppstartsbeslutet
får inte användas som ett nytt godkännande av recovery.


## Sessionsbunden P/Q-klassificering – 4c.2b.2

`classification::classify_session(session, cancel) -> Assessment<'a>` konsumerar
RecoverySession. Assessment behåller sessionen under hela sin livstid, även vid
Uncertain och NotNeeded. Det tar inget nytt process- eller underhållslås. En
in-process-session måste dessutom avse pending-underlagets restore_id genom sitt
befintliga underhålls-ID. Vid uppstart är den tidigare operationens identitet
hämtad ur strikt validerat P/Q-underlag, inte ur den nya processens sessions-ID.
Generation och underhållsspärr behålls genom sessionens lån och ändras inte.

Den fristående `classify(root, cancel)` finns kvar för diagnostik och tidigare
tester. Den tar en DirectoryLease en gång och använder samma privata verifierare.
Den ska inte anropas när en session redan håller katalogens lås.

Ett privat Evidence behåller faktiskt öppnade recovery-/pending-/kvittoobjekt,
ursprungliga pending-bytes och relevanta identiteter. Kontroller återupprepas vid
SQLite- och verifieringsgränser. Pending läses positionsbaserat med befintlig
storleksgräns; upprepade läsningar är inte beroende av filhandtagets offset.
LoadedReceipt lämnar endast valideringsunderlag till klassificeraren, aldrig
auktorisering. Klassificeringen skriver inte till någon av källfilerna.

| Underlag | Krav och resultat |
| --- | --- |
| Endast P | Full verifiering ger Old eller New; om båda matchar väljs Old. |
| P och Q | Exakta inbäddade pending-bytes måste matcha P; aktivt innehåll måste matcha Q:s angivna utfall. |
| Endast Q, med eller utan tom recovery-katalog | Samma fullständiga verifiering från Q:s pending-underlag. Tom katalog är inget avslutningsbevis. |
| Tom recovery-katalog utan P/Q | Uncertain. |
| Konflikt, skadade eller osäkra artefakter | Uncertain. |
| Inga recovery-artefakter | NotNeeded, endast diagnostik; ingen verifierad Observation och ingen rätt till normal drift. |

Q verifieras aldrig enbart som kvittometadata. Journalens fysiska filbindning,
schema, integritet, dokument och innehåll verifieras tillsammans med skyddskopians
arkivhash, backupformat och Old-innehåll. Identiska Old/New-fingeravtryck tillåter
Q:s angivna Old respektive New när innehållet matchar; det bevisar inte kopieringshistorik.

Okända poster i recovery-katalogen och övergivna `.mno-completion-*` i datakatalogen
avvisas. De raderas inte. Andra vanliga appfiler räknas inte som recovery-metadata.

Assessment har privata fält, saknar Clone och kan inte skapas från fri status,
bool eller fingerprint. Old/New behåller en Observation med den verifierade
SQLite-lästransaktionen. Fältordningen stänger BoundSqlite (anslutning före VFS)
före övriga journalhandtag och släpper sessionens lån sist. En uttrycklig Drop
aktiverar Rusts drop-check så att ägaren måste leva till faktisk destruktion,
även om senaste statusavläsningen redan passerat. Uncertain behåller inget
verifierat innehåll. Ingen variant kan auktorisera finish eller upplåsning i detta steg.

Processlåset skyddar mot andra protokollföljande MNO-processer. Filidentiteter och
innehåll kontrolleras mot externa ändringar vid verifieringsgränser; detta är inte
ett generellt skydd mot godtycklig manipulation av samma användares processminne
eller filer. CompletionProof behåller skydden; framtida avslut måste dessutom göra slutliga
kontroller innan beviset används för filändringar. Ingen automatisk SQLite-recovery har införts;
rollback/WAL/SHM ger fortfarande Uncertain före SQLite-läsning.

macOS är fortsatt enda aktiverade klassificeringsplattform. Linux och Windows
avvisas utan fallback. Uppstartens befintliga spärrar är oförändrade.

De nya isolerade testerna täcker matrisen, byteavvikelser, lika fingeravtryck,
kvittoutfall, filbyten, symlänkar, temporärfiler, avbrott, oförändrade källor,
underhålls-/operationsbindning samt kvarhållna processlås och SQLite-snapshots.
Kompilatortestet inkluderar den faktiska Assessment-definitionen med små ägarstubbar
för att kontrollera privata konstruktorer, icke-klonbarhet och lånade livstider.
Det ersätter inte runtime-testerna av verklig session och DatabaseState.


## Operationsbundet CompletionProof – 4c.2b.3

Det interna API:t är:

```rust
Assessment::into_completion_proof(self)
    -> Result<CompletionProof<'a>, RejectedAssessment<'a>>
RejectedAssessment::reason(&self) -> ProofRejection
RejectedAssessment::into_assessment(self) -> Assessment<'a>
```

CompletionProof har privat konstruktion och äger hela det ursprungliga Assessment.
Det saknar Clone, Copy, Serialize, Deserialize och generell konstruktion från status,
bool, fingeravtryck eller deserialiserad metadata. Det exponerar inga anslutningar,
skrivbara filhandtag, metoder för publicering eller upplåsning. En framtida
avslutsfunktion måste ta beviset **med värde**, exempelvis `finish(proof:
CompletionProof<'_>)`; någon sådan produktionsfunktion införs inte nu.

Konvertering kräver både verifierad Old/New-observation och en faktisk
RecoverySession. En fristående diagnostisk klassificering kan inte ge bevis även
om dess status är Old/New. Uncertain och NotNeeded ger särskilda avvisningsorsaker.

Före konvertering återkontrolleras den behållna VFS-bindningen, aktiv lästransaktion,
DirectoryLease, exakta P/Q-bytes, restore-ID, katalog-/databasidentiteter och
skyddskopians verifierade kandidat. Ursprunglig SHA-256 för aktiv databas behålls
från klassificeringen och jämförs igen, liksom skyddskopians arkivhash. Inga nya
handtag till aktiv databas öppnas. Befintlig avbrottsflagga respekteras.
Hashkontrollerna strömmar med begränsat arbetsminne men har linjär kostnad i
filstorleken. Detta är återanvända identitets-/verifieringsmekanismer, ingen ny
parallell definition av schema, dokumentformat eller innehållsfingeravtryck.

Beviset behåller den aktuella sessionens processlokala ID och det beständiga
restore-ID:t som två olika bindningar. Vid uppstart efter krasch hålls
RecoveryGate genom sessionens lån; det finns då inget gammalt Maintenance-guard
eller en beständig generation att återskapa. Under samma process behålls däremot
samma Maintenance, dess operations-ID, DatabaseState och generation genom lånet.
Operations-ID:t måste matcha pending-underlaget. Ingen generation ändras.

Avvisning returnerar ett RejectedAssessment som äger det ursprungliga Assessment,
inklusive kvarvarande skydd och eventuell verifierad snapshot. Orsakerna är
Uncertain, NotNeeded, SessionRequired eller Invalidated(Reason). Det går att
återta Assessment med en konsumerande metod; dess diagnostiska status är inte en
auktorisering och varje nytt konverteringsförsök gör kontrollerna igen. Att släppa
felet eller beviset släpper aldrig RecoveryGate eller återställer recovery-latchen.

Bevisets ägda Assessment har fortsatt uttrycklig Drop för drop-check: även vid
implicit destruktion måste de lånade ägarna leva tills snapshoten förstörts.
SQLite-anslutningen stängs före VFS, journalhandtag och sessionslån.

Detta är ett processlokalt bevis, inte en beständig garanti om framtida filnamn
eller filer. Vid framtida avslut måste skydden hållas kvar och aktuella identiteter,
artefakter och nödvändiga beständighetsvillkor kontrolleras igen före mutation.
Godtyckliga skrivningar av samma användare som kringgår SQLite/OS-protokollen kan
inte förhindras av denna typ. Ändrade objekt/bytes som upptäcks av konverteringen
avvisas konservativt. Ingen blind rollback eller SQLite-recovery har införts.

Isolerade runtime-tester kontrollerar P/Q-matrisen, lika fingeravtryck, ID-bindningar,
förändringar mellan Assessment och konvertering, avvisning, Drop, generation,
oförändrade källor samt SQLite-reservation och processlås. Kompilatortester använder
de faktiska Assessment-/CompletionProof-definitionerna, små ägarstubbar och verkliga
Serde-traits med positiva kontroller. De testar privat konstruktion, konsumtion,
livstider och avsaknad av kloning, kopiering och serialisering/deserialisering.

Kvar efter 4c.2b är att ansluta en separat granskad, beviskrävande avslutsoperation
för beständig kvittopublicering och kontrollerad städning, därefter säker uppstarts-
och driftövergång. PendingMarker::finish är inte kopplad till CompletionProof i
detta steg. Ingen sådan koppling får kringgå den nya beviskedjan. Produktions-
återställning och automatisk recovery är fortsatt avstängda. macOS är enda
aktiverade klassificeringsplattform; Linux och Windows avvisas utan fallback.

## Gemensam ägare och exklusiv reservation – 4c.2c.1

DatabaseState har en stabil, privat `directory: OnceLock<StartupOutcome>` och
behåller samma DirectoryLease under normal drift och spärrad recovery. Ingen
ny processlåsning eller överlämning genom unlock/relock införs. Sessionerna lånar
DatabaseState direkt; in-process-sessioner lånar dessutom redan taget Maintenance.
StartupOutcome är ett transporterat uppstartsbeslut med privata låsägare, inte
ett fritt kommando för fasövergång. Den framtida Ready-övergången får inte byta
eller ta ut DirectoryLease ur dess ägare.

En privat `Reservation<'a>` reserverar DatabaseStates atomiska sessionsplats med
compare_exchange. Endast en samtidig session lyckas; övriga ger det särskiljbara
felet `database_recovery_busy`. Fabrikerna använder samma state för leasen och
reservationen, så olika kataloger/lås kan inte kombineras. Reservationen saknar
Clone/Copy och fria konstruktorer. Den flyttas med sessionen genom Assessment,
CompletionProof och RejectedAssessment. En avvisning håller därför platsen tills
resultatet faktiskt släpps. Drop frigör endast sessionsplatsen, aldrig OS-lås,
underhållslås, recovery-spärr, driftfas eller generation. En läckt reservation
kan blockera fler sessioner men kan inte godkänna normal drift.

Privata faser är Starting, Recovery, Ready och Blocked. Endast framgångsrik normal
uppstart sätter Ready (undantaget strikt testisolerad ready_for_test). Fel under
uppstart sätter Blocked. Recovery-uppstart och in-process-recovery sätter Recovery;
den befintliga recovery-latchen behålls för felkontraktet. Completing införs först
när ett verkligt avslutsprotokoll finns. Inga fas-setters exponeras. Avslutat på
disk och Ready är fortfarande separata begrepp: inget avslut på disk eller ny
Ready-övergång implementeras här.

Tester med barriärer håller vinnande reservation tills alla åtta samtidiga
försök är avslutade: exakt en lyckas och sju ger Busy, utan tidsantaganden.
Övriga tester kontrollerar reservation genom Assessment, proof och avvisning,
normal/felaktig uppstart, in-process-lån, oförändrad generation, bevarat OS-lås
och fortsatt spärr efter Drop. Kompilatortester kontrollerar även reservationens
privata konstruktion samt verkliga sessionstypens ägarskap och livstider.
Tidigare separata process-/kraschtester bevaras.

Återstår för 4c.2c.2: separat granskad konsumtion av CompletionProof,
slutkontroller, beständig kvittopublicering och kontrollerad städning. Därefter
behövs en beviskrävande driftövergång utan att leasen flyttas eller släpps.
Ingen av dessa operationer aktiveras här. Plattformarnas säkerhetsbegränsningar
är oförändrade; macOS körtestas, Linux/Windows avvisar recovery.

## Uppdatering i 4c.2c.2

CompletionProof har nu den beviskrävande metoden publish_receipt(self), beskriven
i Restore_Completion_Receipt_v1.md. Den behåller session och reservation i både
ReceiptDurable och ReceiptFailure. Tidigare avsnitts beskrivning av endast
testisolerad publicering avser tidigare delsteg; fri bytepublicering är fortsatt
testisolerad. Ingen städning, Ready-övergång eller generationsändring har införts.
