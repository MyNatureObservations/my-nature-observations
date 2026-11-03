# Produktionsintern skrivförberedelse (5c)

Detta steg förbereder ägarskap. Ingen skrivmotor, skrivbar journalöppning,
journalmutation, Tauri-exponering, Ready-övergång eller automatisk recovery ingår.

## Två konsumtionssteg

Befintlig `RestoreContext::authorize()` och P-publicering kräver att Maintenance
fortfarande är användbar. `Maintenance::recovery_session()` spärrar normal drift.
En session kan därför inte skapas före P med bibehållna auktoriseringskontrakt.

Det privata backend-API:t använder därför:

```text
RestoreContext::publish_write_preparation(self)
    -> Result<DurableWritePreparation<'m, 's>, Box<WritePreparationFailure<'m, 's>>>

DurableWritePreparation::prepare_write(self, RecoverySession<'m, 's>)
    -> Result<PendingWrite<'m, 's>, Box<WritePreparationFailure<'m, 's>>>
```

Samordnaren äger den ursprungliga Maintenance-vakten utanför contextet. Den
publicerar först P genom den verkliga VerifiedProtection/AuthorizedRestore-kedjan.
`publish_with()` måste lyckas inklusive filsync, no-clobber-publicering,
katalogsynkning och sista filbarriär innan mellanägaren kan returneras.

Därefter lämnar samordnaren över en befintlig verklig Running-session. Den enda
reservationen etableras via den befintliga Maintenance-fabriken; förberedelsekoden
anropar ingen sessionsfabrik och tar inget OS-lås. Om samordnaren behöver skapa
sessionen sker det efter första steget, före konsumtionen i det andra. Den
obundna mellanägaren ger ingen skrivrätt och behåller Recovery-spärren även om
samordnaren avbryter här. 5c kopplar inte in någon produktionssamordnare.

Bindningen kontrollerar verklig aktiv session/reservation, Running-ursprung,
identisk levande Maintenance-instans och DatabaseState:s reservationsplats via
befintlig `bind_context()`, samt sessions-ID, operation, generation,
databassökväg och fysisk katalogidentitet. P och context återverifieras innan
`PendingWrite` skapas. Startup- och främmande Maintenance-sessioner avvisas.

En senare session kan bindas först när mellanägaren ännu är obunden. Det är då
en ny initial bindning, aldrig ommärkning av ett existerande sessionsbundet
PendingWrite. Ingen API-väg kan byta session i ett färdigt PendingWrite.

## Resurser och livstider

Originalets rot-, recovery- och P-handtag, exakta P-bytes, operation och generation
flyttas direkt ur den verkliga PendingMarker. Auktoriseringslånen avslutas innan
contextet flyttas. Samma context äger ActiveDatabase, incoming PreparedRestore
och VerifiedProtection med dess verifierade skyddskopieresurser. Både
Maintenance- och DatabaseState-livstiden behålls.

Den gemensamma ägaren håller följande fält i destruktionsordning:

1. Context: ActiveDatabase och PreparedRestore använder befintlig kontrollerad
   SQLite-close före beroende filresursers respektive stagingens cleanup.
2. P:s behållna fil- och kataloghandtag samt exakta bytes/identiteter.
3. Den verifierade ContextSessionBinding som lånar samma Maintenance.
4. Den konsumerade RecoverySession med samma exklusiva Reservation.

DirectoryLease ligger kvar i DatabaseState hela tiden. Inga självreferenser,
unsafe-operationer, kloner av kapabiliteter eller livstidsförlängningar införs.
Typerna har privata fält och ingen Clone, Copy, Serde eller fri konstruktor.
PendingWrite exponerar ingen SQL-åtkomst eller metod för journalbyte/admission.

## Fel och panik

Felägaren behåller det uppnådda ägarsteget. Före P finns bara contextet; efter
publicering kan P redan finnas även om publiceringen rapporterar ett sent fel.
Efter lyckad publicering behålls även P:s originalresurser. När sessionen lämnas
till andra steget placeras den under samma unwind-skydd före alla kontroller och
testcallbacks. Även en avvisad session ägs av felresultatet tills detta släpps.
`error()` är en diagnostisk observation och ger ingen fortsättningsauktoritet.

Ingen felväg tar bort P, återskapar artefakter, höjer generation eller häver
Recovery. Vanlig Drop använder befintliga säkra SQLite-destructorvägar.
SQLITE_BUSY behåller anslutning och staging tillsammans enligt PreparedRestore:s
befintliga kontrakt. En normalt släppt ägare frigör till sist sessionsplatsen;
Recovery-spärr och processlås i DatabaseState består även om en osäkert stängd
read-only-anslutning med beroenden konservativt har behållits.

Under unwind behålls hela aggregatet konservativt med `mem::forget`, inklusive
en redan konsumerad session/reservation. Det är avsiktlig retention tills
processen avslutas. De kvarhållna lånade posterna kan inte återfinnas eller
användas efter att deras lånade ägare försvunnit. Inga godtyckliga callbacks eller
panikpayloads ägs av produktionsägaren. Testcallbacks och kontrollpunkter finns
endast bakom `cfg(test)`.

## Tester och begränsningar

Tester använder verklig publicering och verkliga sessioner. De kontrollerar
originalhandtag före/efter sessionsbindning medan handtagen fortfarande ägs,
exakta bytes, identiteter, processlås, Busy vid kritiska punkter, fel, ändrat P,
Startup/Running-separation, panik, normal Drop och faktisk SQLITE_BUSY.
En befintlig testobservation vid Reservation::drop verifierar att normal
stagingcleanup skett före sessionsplatsens frigörande.

Kompilatorprober kompilerar produktionsägare och konsumtionsmetoder samt verkliga
context- och callbackdeklarationer. I/O och angränsande backends ersätts i proberna,
medan runtime-tester använder verkliga implementationer. En positiv kontroll
krävs före varje grupp av negativa diagnostikkontroller.

macOS är enda målplattform för denna kapabilitet. Linux och Windows avvisas före
P-publicering; ingen fallback har lagts till. Cross-compilering verifieras inte
av macOS-testkörningen. Befintlig accepterad TOCTOU-begränsning mot en godtycklig
extern aktör kvarstår. Lyckade systemanrop bevisar inte verkligt
strömavbrottsbeteende. En framtida skrivmotor måste göra färska kontroller innan
mutation; detta steg ger inte i sig någon journalbytesmetod.

## Låssäker kontroll av behållet P (5d.6)

`OwnedPending::retained_name_matches(&self)` används av både tidig `verify`
och den senare kontrollen efter A-close. Namnets metadata jämförs med det
redan ägda P-handtaget: vanlig fil, inte symlänk, ett länknamn och samma
enhet/inod. P-namnet öppnas eller dupliceras aldrig för denna kontroll.
Längd, innehåll och tidigare identitets-/sessionskontroller är kvar; läsning
använder det behållna handtaget och ändrar inte den delade filpositionen.

Detta förhindrar att just dessa kontroller stänger en extra descriptor till
B/C-main vid hårdlänksutbyte av P. Metadata fryser inte namnrymden: samtidiga
byten och ABA kan passera vid ett giltigt observationstillfälle. Ingen ny
skriv-, close-, klassificerings- eller Ready-auktoritet skapas. Annan
path-baserad resursanskaffning, inklusive arkivöppning i preflight, ingår inte
i denna begränsade rättning och kräver fortsatt separat säkerhetsarbete.

## Lånad PendingMarker-kontroll (5d.7)

Även den lånade `PendingMarker::check` före överföring till `OwnedPending`
använder nu den gemensamma privata `retained_pending_name_matches`-funktionen.
Båda återöppningarna av P-namnet är borttagna ur denna kontroll. Hela det
behållna P-innehållet läses från offset noll med positionsbaserade läsningar,
högst LIMIT+1 bytes; strikt parsning sker före jämförelse med förväntade bytes.
Korta läsningar fortsätter till EOF eller gränsen och den delade filpositionen
ändras inte. Felkategorier och befintlig resurs-/Drop-policy bevaras.

Ett macOS-testcheckpoint ligger efter återvändande FinalSync och omedelbart
före den verkliga lånade kontrollen. Det ändrar inte barriärernas krokar eller
ordning. Separata testcheckpoint under läsningen prövar tillväxt och trunkering
utan nya filhandtag. Samtliga checkpoint saknas i produktionsbyggen.

Metadata är fortfarande en observation, inte en fryst namnrymd. Tidigare
öppningar av temporärt P, path-baserade synkbarriärer och skyddsarkiv ingår
inte i rättningen. Hela publiceringskedjan eller framtida restore är därför
inte bevisad låssäker av detta steg. Ingen ny armering eller restore-auktoritet
införs.


## P-publicering genom behållna filhandtag

Temporärt P verifieras nu med samma begränsade positionsbaserade läsare som
lånat P, genom det ursprungliga skapade filhandtaget. Namnkontrollen för
såväl temporärt som publicerat P delar samma privata metadataimplementation.
Ingen vanlig P-fil öppnas eller dupliceras för dessa kontroller.

FileSync och FinalSync behåller sin krokordning och använder sync_opened på
just detta filhandtag, före respektive efter persist_noclobber. Publiceringens
katalogbarriärer är oförändrade. Metadata och exakta P-bytes kontrolleras före
och efter filbarriären; krok- och I/O-fel förblir vanliga Result-fel. Samma
Maintenance-/Recovery-låsning, ägarkedja och Drop-/terminalpolicy behålls.

Ett namnbyte efter förkontrollen kan inte rikta om filens synkning till en
SQLite-main-inod. Kontrollerna fryser däremot inte namn eller innehåll: ABA kan
passera när ursprungsnamnet återställts före nästa observation. Ett lyckat
resultat är inte ett skydd mot senare unlink, innehållsändring eller strömavbrott.
Testerna använder endast isolerade barnprocesser, verkliga B/C-resurser och
låsförlustkontroller; inga testcheckpoint finns i produktionsbyggen.

Arkivanskaffning, kandidatens SQLite-inodbindning, äldre separat läsning och
cleanup/finish ingår inte. Ingen writer, CompletionProof, Ready eller
produktionsrestore aktiveras av denna begränsade publiceringsrättning.
