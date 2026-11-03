# Kontrollerad SQLite/VFS-stängning – 4c.2c.4a

## Omfattning

`RecoveryDirectoryRemoved::close_resources(self)` konsumerar föregående
kapabilitet och returnerar `Result<ResourcesClosed, ResourceCloseFailure>`.
Konstruktorer och ägda resurser är privata. Ingen Clone, Copy, Serde, fri
statuskonvertering, SQL-anslutning eller upplåsningsmetod exponeras.

Q lämnas kvar. Inga filer i den aktiva datakatalogen tas bort eller ändras,
ingen generation ändras och ingen driftfas övergår till Ready. Linux och
Windows saknar fortsatt stöd för recovery. Startup-kontrollen av SQLite-sidofiler
införs inte här.

## Inventering och exakt ordning

Den konsumerade kedjan äger två SQLite-anslutningar:

- `Observation.database`: den aktiva journalens read-only, descriptorbundna
  SQLite-anslutning, med kvarhållen lästransaktion och en egen registrerad VFS.
- `Observation.protection`: `PreparedRestore`, den verifierade skyddskopians
  extraherade databas med sin lästransaktion, filhandtag och privata stagingkatalog.

Verifieringsfunktionernas statements är lokala och ska vara färdiga före detta
steg. Inga BLOB-/backuphantag eller statements exponeras av den nya kapabiliteten.

1. Återverifiera den befintliga observationen under samma session: Recovery-fas,
   operation, katalog-/filidentiteter, Q och exakta pending-bytes, skyddskopia,
   SQLite/VFS-bindning, snapshot, journalhash, layout, sidofiler och avbrott.
   Ingen ny SQLite-open görs. Detta är en innehålls-/identitetskontroll, inte en
   ny beständighetskvittens för journalen.
2. Anropa `Connection::close(self)` för den aktiva journalanslutningen.
   Rusqlite 0.40.2 använder `sqlite3_close`, inte `sqlite3_close_v2`.
3. Endast efter SQLITE_OK: avregistrera dess custom VFS. Kontrollera resultatet
   och frigör callbackminne, namn och descriptorägarskap endast efter framgång.
4. Stäng `PreparedRestore`-anslutningen uttryckligen med samma close-API.
5. Skapa ResourcesClosed först när båda anslutningarna och VFS är stängda.
   Behåll filhandtag, verifieringsunderlag och privata stagingfiler, men ingen
   SQLite-snapshot, i resultatet. Staging städas vid normal Drop av framgångsresultatet.

DirectoryLease ägs fortsatt av DatabaseState. Resultaten behåller den lånade
RecoverySession och dess exklusiva reservation, samt relevant Maintenance.
Resurser förstörs före den lånade sessionen. Explicita Drop-check-implementationer
hindrar att lånade skydd förstörs före resultatet.

## Fel och panik

ResourceCloseFailure behåller antingen den ursprungliga kapabiliteten eller
samtliga återstående resurser i den påbörjade stängningen. Fel skiljer på
verifiering, aktiv SQLite-close, VFS-unregister, skyddskopians SQLite-close,
I/O vid testbara kontrollgränser och Unsupported. SQLite-fel behåller kod och text.

ResourceCloseProgress är endast diagnostik:

- Unchanged: inget lyckat aktivt close har bekräftats.
- ActiveSqliteClosed: SQLite är stängt, men VFS-avregistreringen misslyckades.
- ActiveClosed: aktiv SQLite och VFS är stängda.
- ProtectionClosed: även skyddskopians anslutning är stängd.

SQLITE_BUSY från ett kvarvarande statement behåller anslutningen och VFS;
VFS avregistreras inte. Ingen automatisk återöppning eller retry-API erbjuds.
Normal Drop får försöka släppa resurser, men utfärdar ingen framgångskapabilitet.
Vid fortsatt misslyckad close läcks anslutning och VFS konservativt tillsammans.
Vid unregister-fel behålls registrerade allokeringar; om även fallback-Drop
misslyckas läcks de hellre än att SQLite får hängande pekare.

Varje PreparedRestore har nu samma kontrollerade destruktion, även genom
Protection::Live, avvisning före stängningsstart, tidigare journal-/VFS-fel,
avbrott och panik. Två privata Option-fält äger anslutningen respektive
sammanhållna filhandtag/manifest/staging. close_inner används av både try_close
och Drop. Ingen osäker manuell destruktion av Rust-fält används.

Endast bekräftad Connection::close får överlämna filresurser till staging cleanup.
Explicit fel återställer ägarskapet i PreparedCloseFailure. Om ett statement
har avslutats innan felresultatet släpps kan dess Drop bekräfta close och
städa normalt. Lyckad stängning medför ingen onödig permanent resursretention.

Om close fortfarande misslyckas under Drop behålls anslutning, filhandtag,
manifest och stagingägare tillsammans med säkra mem::forget. Ett unwind-skydd
behåller filresurser om close panikerar utan säker kvittens. PrivateDir får
aldrig köras i dessa osäkra fall. Minnes-/handtagsresurser frigörs vid processavslut,
men privata stagingfiler kan finnas kvar på disk. Ingen automatisk städning av
sådana rester införs.

Unwind-skydd behåller registrering respektive privata filer om close skulle
panikera. Kontrollpunkter före/efter stängningsstegen kan paniktestas; ingen
Rust-panik förs genom någon C-callback. Release har panic=abort, så catch_unwind-
tester gäller testbyggets unwind-beteende, inte en utfästelse om releaseåterhämtning.

Drop frigör endast sessionsplatsen och säkert frigörbara resurser. Recovery-
spärren, generationen, Q och processlåsets ägare ändras aldrig.

## Avgränsning mot andra ägare och nästa steg

ResourcesClosed betyder att **den konsumerade verifieringskedjans** två
anslutningar är stängda. Ett omgivande RestoreContext kan ha tre andra anslutningar:

| Anslutning | Ägare och nuvarande stängning |
| --- | --- |
| ActiveDatabase.connection | Ägs av RestoreContext sedan 4c.2c.4c.1; explicit kontrollerad close eller säker Drop. |
| RestoreContext.incoming.database | Contexts inkommande PreparedRestore; explicit try_close eller kontrollerad Drop. |
| RestoreContext.protection.candidate.database | PreparedRestore i contexts VerifiedProtection; explicit try_close eller kontrollerad Drop. |

De är skrivskyddade och använder inte klassificeringens custom VFS. De ägs inte
av RecoverySession eller ResourcesClosed och kan vara öppna efter resultatet.
Alla tre ska hanteras uttryckligen före framtida Q-borttagning eller normal
drift. Detta steg ger ingen sådan auktorisering.

Efter close hålls ingen SQLite-läsreservation för den aktiva journalen.
Processlåset och Recovery-/Maintenance-spärrarna samordnar MNO-operationer;
de är inte ett skydd mot en godtycklig extern skrivare med samma OS-rättigheter.
Den tidigare hotmodellen utvidgas inte.

4c.2c.4b och efterföljande avslutssteg behöver separata kontroller före Q-unlink:
startup-policy för sidofiler, slutlig filidentitet/innehåll, journalens
beständighet och kvarvarande ägare. En ResourcesClosed- eller diagnostisk
progress-status får inte ensam användas för Q-borttagning. Ready och
generationsbyte hör fortsatt till 4c.2c.5.

## Avbrottssemantik

Avbrott kontrolleras vid återverifieringen före stängningsstart. Ett avbrott efter
sista kontrollen kan följas av slutförd resursstängning och ResourcesClosed.
Det är avsiktligt: Q behålls och ingen fas eller generation ändras. Resultatet
betyder inte en ny innehållsverifiering eller rätt att ta bort Q.

Ingen extra avbrottskontroll införs mellan close-stegen. Den behövs inte för
säkerheten i detta steg och skulle lägga till ett avbrottsutfall med endast vissa
resurser stängda. Nästa mutationssteg måste ändå kontrollera avbrott på nytt.

## Tester

Isolerade tester täcker verklig SQLITE_BUSY med ett outstanding statement på
vardera anslutningen, injicerat annat close-fel, unregister-fel, läsbar statement
efter konservativ Drop, lyckad close och stagingstädning, avbrott, ändrat Q,
panik vid stängningsgränser, reservation/processlås, oförändrade filer, Recovery-fas
och generation. Kompilatorprober använder de faktiska kapabilitetsdefinitionerna
med stubbat resursbackend och positiva kontroller, kompletterat med riktiga
runtime-tester. De verifierar privata konstruktorer, engångsanvändning, avsaknad
av Clone/Copy/Serde/SQL-genvägar och livstidsbindning.

Korrigeringens regressioner håller verkliga statements öppna i skyddskopian
under ett tidigare journal-close-fel, unregister-fel, tidigt avbrott och panik
före egen try_close. En trådlokal cfg(test)-logg bekräftar faktisk SQLITE_BUSY
från close under Drop. Statement kan fortfarande läsas efter Drop och staging
finns kvar. Motfall verifierar SQLITE_OK och normal cleanup efter lyckad close,
även efter tidigare fel. Sent avbrott testas separat utan upplåsning.


## Contextägarskap och stängning – 4c.2c.4c.1

RestoreContext äger nu ActiveDatabase och de två PreparedRestore-kandidaterna.
Maintenance ägs fortfarande utanför context. Callbacken får ett ägt context;
alla AuthorizedRestore-/PendingMarker-lån måste upphöra innan det konsumeras.
Den högre rangens callbacklivstid hindrar context och stängningsresultat från
att lämna den lokala Maintenance-operationen.

`RestoreContext::close_owned_connections(self, &RecoverySession)` returnerar
(sessionsargumentet tillkom i 4c.2c.4c.2a)
`Result<ContextConnectionsClosed, ContextCloseFailure>` och stänger exakt:

1. A: den aktiva journalens contextanslutning.
2. B: den inkommande kandidatens anslutning.
3. C: contextets verifierade skyddskopiekandidat.

Alla tre måste ge bekräftad Connection::close (sqlite3_close) före framgång.
Ingen close_v2 används som bevis. Vid bindingfel behålls hela context. Vid ett
stängningsfel behålls redan stängda filresurser och återstående levande ägare.
SQLITE_BUSY och dess SQLite-felkod rapporteras; inget stängningsbevis utfärdas.
PreparedCloseFailure behåller den ursprungliga felinformationen och stagingägaren.

ActiveDatabase använder ett eget close_inner för explicit stängning och Drop.
Dess anslutning och samlade filhandtag/identitet/sökväg överförs med Option::take.
Ett unwind-skydd etableras före close. Vid fortsatt BUSY under Drop behålls
anslutning och filresurser konservativt. Om close unwinder behålls filresurserna;
en panik är aldrig en stängningskvittens. PreparedRestore återanvänder oförändrat
sitt befintliga motsvarande skydd. Ingen manuell destruktion eller transmute används.

Det gäller också vid panik efter ägaröverföring, efter A och efter A+B:
återstående PreparedRestore-ägare får inte städa staging utan bekräftad close.
Om ett outstanding statement har avslutats före Drop kan en ny close lyckas och
normal cleanup ske. Annars kan privata stagingfiler och anslutningsresurser
konservativt bli kvar till processavslut; stagingfiler kan bli kvar även därefter.
Ingen automatisk städning av sådana rester införs.

Framgångsresultatet behåller de tre anslutningarnas filresurser, skyddskopians
bindningsmetadata och lånet till Maintenance. Det exponerar inte SQL eller
konstruktorer och saknar Clone/Copy/Serde. Normal Drop städar stängd staging.
Det ändrar aldrig generation eller driftfas och rör inte P, Q eller recovery-katalogen.

Detta är INTE det gemensamma avslutsbeviset: ContextConnectionsClosed äger inte
klassificeringens RecoverySession/reservation eller ResourcesClosed och ger ingen
rätt att ta bort Q. DirectoryLease ägs oförändrat av DatabaseState. Maintenance-
lånet hålls i både framgångs- och felresultat. Att släppa dem återställer inte
Ready; en redan satt Recovery-spärr består. Vanliga befintliga contextanrop som
inte har gått in i Recovery behåller sin tidigare Maintenance-semantik.

4c.2c.4c.2 måste förena contextstängningen med ResourcesClosed (eller styrkt
startupursprung utan context), kontrollera samma operation/session/databas och
generation samt genomföra slutlig fil-/innehålls-/artefakt- och beständighetskontroll.
Inget av detta, Q-borttagning eller driftövergång aktiveras här. Ingen ny
SQLite-open eller automatisk recovery införs av stängningsoperationen. Linux och
Windows recovery är fortsatt Unsupported.

Nya isolerade tester använder riktiga outstanding statements för BUSY i A/B/C,
verifierar staging efter Drop och fortsatt läsbart statement, lyckad retry-Drop,
exakt close-ordning och ingen upprepad close vid framgångs-Drop. Panik injiceras
endast under cfg(test), efter etablerat skydd och vid ägar-/steggränser. Q,
skyddskopia, journal, generation, Recovery-spärr och processlås kontrolleras.
Kompilatorprober kompilerar den faktiska stängningsmodulen och contextdeklarationerna
med stubbat skapande/verifieringsbackend och positiv kontroll. De kontrollerar
privata resultat, konsumtion, aktiva auktoriseringslån och lånade livstider.
De ersätter inte runtime-tester eller granskning av panik-/FFI-säkerheten.

Slutkompletteringen injicerar SQLITE_IOERR i A:s close-resultat enbart under
cfg(test) och verifierar levande anslutning, behållet filhandtag och staging utan
stängningsbevis. Ett separat paniktest håller riktiga statements öppna i A och C
vid A:s etablerade unwind-skydd. Efter unwinding är statementen fortsatt giltiga,
A:s filhandtag och C:s staging finns kvar, medan säkert stängd B-staging städas.
Den explicita A-stängningen återinträds inte. Scenarierna verifierar även
oförändrat Q, skyddskopia, journal, generation och fortsatt Recovery-spärr.


## Sessionsbindning före contextstängning – 4c.2c.4c.2a

Maintenance räcker inte som sessionsidentitet: samma guard kan skapa successiva
RecoverySession-instanser när föregående reservation har släppts. Därför kräver
contextstängningen nu en referens till en verklig RecoverySession innan någon
anslutning lämnar sin ägare eller något close-anrop görs.

RecoverySession::bind_context(maintenance) skapar en privat ContextSessionBinding.
Den verifierar Recovery-fas, aktiv reservation i rätt DatabaseState, Running-
ursprung och identisk Maintenance-instans med pekarlikhet. Startup-sessioner
avvisas även om deras övriga metadata skulle stämma. DirectoryLease måste
fortfarande verifiera sin behållna katalog och låsfil. Katalogidentiteten tas
från dess befintliga kataloghandtag via den gemensamma identitetsfunktionen.
Inget nytt processlås, Maintenance-lås eller SQLite-open införs.

Stängningsfabriken kräver dessutom att:

- contextets generation stämmer med den oförändrade Maintenance-generationen,
- skyddskopians befintliga ProtectionBinding matchar operation, generation,
  databassökväg och fysisk filidentitet,
- sessionens tilldelade databassökväg är contextets kanoniska databassökväg,
- contextets behållna databasfil fortfarande motsvarar sökvägen och dess
  sparade fysiska identitet fortfarande matchar det behållna filhandtaget.

ContextConnectionsClosed behåller en privat ContextBinding: sessionens redan
befintliga UUID, Maintenance-operation (också restore-ID enligt befintligt
pending-protokoll), generation, katalogidentitet och tilldelad databassökväg,
samt contextets verifierade databasidentitet. Inga nya eller anropsstyrda ID:n
införs. Stängningsordningen och bekräftad sqlite3_close är oförändrade: A → B → C.

Bindningen lånar Maintenance men inte själva RecoverySession-objektet. Den
kan därför inte användas för att flytta ut contextbeviset ur sin lånade
Maintenance-livstid, men hindrar inte att samma session flyttas till Assessment,
CompletionProof och ResourcesClosed. Sessionen äger fortsatt reservationen;
DatabaseState äger fortsatt DirectoryLease. Funktionen varken konsumerar eller
släpper sessionen. Ett referenslån håller sessionen levande under själva
bindnings- och stängningsanropet, även vid fel eller panik.

Contextbeviset kan därefter finnas kvar efter session-drop, men är då endast
ett inert historiskt stängningsbevis. Det ger ingen auktoritet till avslut,
upplåsning eller mutation. ContextSessionBinding::check_session kräver en
verklig aktuell Running-session och avvisar ett nytt sessions-ID, även under
samma Maintenance. Den returnerar inget nytt kapabilitetsbevis. Bindningens
fält är privata och typen saknar Clone/Copy/Serde.

Vid sessions- eller identitetsfel returneras ContextCloseFailure med hela
oförändrade context, före första close. Vid ett senare close-fel bevaras
ContextBinding i den befintliga Closing-ägaren tillsammans med stängda och
återstående levande resurser. Panik och Drop återanvänder oförändrad kontrollerad
resursstängning. Anroparens session och reservation berörs inte; Drop aktiverar
inte Ready och ändrar inte generation. Inget nytt retry- eller auktoriserings-API
införs. Q förblir orört.

Testerna omfattar fel före första close, felaktigt startupursprung, fel
Maintenance-instans även vid lika UUID/generation, fel operation/generation,
fil- och katalogbyten, inaktiv reservation och successiva sessioner. Verkliga
BUSY- och paniktester kör nu med giltig sessionsbindning. Kompilatorprober
kontrollerar obligatoriskt sessionsargument, användning efter session-drop,
privata bindningsfält, avsaknad av Clone och lånade Maintenance-livstider.
Integrationstestet flyttar samma session genom den verkliga verifieringskedjan
till ResourcesClosed medan contextstängningsbeviset behålls, och avvisar sedan
en efterföljande session från samma guard. Processlås, reservation, Recovery-spärr,
journal, Q, skyddskopia och generation verifieras fortsatt i isolerade miljöer.

4c.2c.4c.2b ska konsumera och förena de två beviskedjorna. Det måste jämföra
contextbindningen med den session och den OperationBinding som faktiskt hålls
av ResourcesClosed, inklusive katalog-/databasidentiteter, restore-ID och
generation. En ny session får aldrig märka om ett gammalt contextbevis. En
separat startupväg måste kräva privat Startup-ursprung och får inte låta
Running-sessioner välja bort contextbeviset med None/bool. Inget gemensamt
stängningsbevis, slutverifiering, Q-borttagning, generationsbyte eller
Ready-övergång implementeras här. Linux/Windows recovery förblir Unsupported.


## Gemensamt stängningsbevis – 4c.2c.4c.2b

Två separata, interna och konsumerande vägar skapar AllConnectionsClosed:

- ResourcesClosed::combine_context(ContextConnectionsClosed) för Running.
- ResourcesClosed::confirm_startup() för recovery efter omstart.

Inga anropsstyrda ID:n, None, bool eller fria ursprungsmarkörer tas emot.
Konstruktorer och fält är privata. AllConnectionsClosed och AllCloseFailure
saknar Clone/Copy/Serde och SQL-accessorer. De bevarar hela ResourcesClosed
med dess verkliga RecoverySession, exklusiva reservation, verifieringsunderlag,
Q-/journal-/skyddskopiehandtag och kvarvarande stängda stagingägare. DirectoryLease
ägs som tidigare av DatabaseState, vars livstid skyddas av sessionens lån.
Running-resultatet behåller dessutom hela ContextConnectionsClosed.

### Bindningar vid Running

Kombinationen kräver Recovery-fas och aktiv session. Den jämför:

1. Contextets sparade sessions-ID med den faktiskt kvarhållna sessionens ID.
2. Den identiska Maintenance-instansen genom ContextSessionBinding::check_session;
   samma UUID räcker inte. Funktionen återanvänder den tidigare bindningskontrollen.
3. Sessionens Maintenance-operation med ResourcesClosed.bindings restore-ID och
   contextets sparade restore-ID. Befintligt protokoll använder samma operations-ID.
4. Contextets sparade generation med den fortfarande lånade Maintenance-generationen.
5. Datakatalogens sparade fysiska identitet med DirectoryLease och det behållna
   root-handtaget i ResourcesClosed. Contextets sessionsbindning kontrollerar
   samma katalogidentitet och tilldelade databassökväg.
6. Databassökvägen i båda kedjorna med samma tilldelade databasplats.
7. Contextets sparade databasidentiteter med ResourcesClosed.bindings identitet
   och dess behållna journalfilhandtag.

ResourcesClosed lämnar endast en delad, skrivskyddad projektion till
kombinationssteget; ingen produktionsaccessor kan ändra de verifierade
bindningarna eller byta deras resursägare.

Det är jämförelser mellan tidigare verifierade, kvarhållna objekt och deras
bindningar. Ingen ny SQLite-open, innehållshashning, integritetskontroll eller
slutlig återverifiering av journalen införs. Identitetsuppgifterna läses från
befintliga handtag och DirectoryLease; markerformat och backupformat ändras inte.
Det nya beviset lovar inte att journalsökvägen eller innehållet förblir oförändrat
vid senare extern manipulation. Den kontrollen återstår till slutverifieringen.

ContextSessionBinding lånar fortfarande Maintenance och inte själva sessionen.
Det låter samma session flyttas till ResourcesClosed före kombinationen.
Successiva sessioner A och B under samma guard skiljs genom sina verkliga UUID:n;
ett gammalt contextbevis från A avvisas med ResourcesClosed från B.

### Startup-recovery

Startup-vägen kräver att den faktiska sessionens privata Access-variant är Startup.
Running-sessioner avvisas. DatabaseStates oföränderliga startupägarskap skiljer
RecoveryGate från normal uppstart med senare Maintenance-recovery. En andra
DatabaseState kan inte ta samma DirectoryLease medan den första ägaren lever.

Startup saknar ett gammalt RestoreContext. ResourcesClosed bevisar redan att
startupklassificeringens båda anslutningar och VFS har stängts. Den vägen verifierar
session, reservation, tilldelad sökväg och de behållna fil-/katalogidentiteterna och
behåller samma verifierade P/Q-underlag och restore-ID. Den observerar den aktuella
processens generation utan att ändra den. Pendingformatet innehåller ingen
beständig generation från föregående process; ingen sådan uppgift uppfinns här.

Startup-ursprunget kontrolleras före generationens läslås. Running håller redan
generationens skrivlås genom Maintenance och får inte försöka ta ett nytt läslås.
Ingen ny Maintenance- eller processlåsning införs. Normal åtkomst är fortsatt
spärrad under startup-recovery.

### Fel, avbrott, Drop och panik

Båda vägarna konsumerar sina bevis och bevarar dem i ett privat ägarobjekt innan
kontrollerna körs. Ett fel returnerar AllCloseFailure med detta ägarskap intakt.
Det finns ingen konvertering från felresultat till framgång. Diagnostiska fel
skiljer Unsupported, bindningsfel, sessionsfel, avbrott och I/O vid testpunkter.
Avbrott kontrolleras både före bindningsjämförelsen och vid slutgränsen.

AllConnectionsClosed behåller sessionens reservation under sin livstid. Ett andra
sessions-/avslutsförsök kan inte skapa en ny verifieringskedja för samma state.
Rusts konsumtion förhindrar återanvändning av de enskilda bevisen; privata
runtimekontroller avvisar olika sessions- och operationsidentiteter. Vanliga
livstidsparametrar kan inte ensamma skilja dynamiska sessions-ID:n åt.

Drop av framgång eller fel förstör contextets stängda resurser före ResourcesClosed
och dess session. Alla SQLite-anslutningar i dessa ägare är redan bekräftat stängda.
Känd privat staging kan därför städas normalt; ingen färdig skyddskopia eller
recovery-artefakt städas. Session-slot frigörs vid slutlig Drop men DirectoryLease
ligger kvar i DatabaseState. Recovery-fasen och generationen ändras aldrig.
Samma ordning gäller vid unwinding: ingen levande SQLite-resurs behöver frigöras
av detta nya steg. Release panic=abort avslutar processen och är ingen
framgångskvittens. Q och övriga beständiga underlag lämnas orörda.

### Tester och återstående gräns

Isolerade tester använder den verkliga verifierings-, publicerings-, cleanup-
och close-kedjan för att skapa bevisen. De täcker giltig Running/Startup,
successiva sessioner, olika kontexter, fel restore-ID/generation/session/katalog/
databas, Running som försöker välja startup, fel/avbrott vid båda kontrollgränserna,
panik, processlås, reservation, generation och Recovery-spärr. Persistent journal,
Q och skyddskopior jämförs före/efter. Testkrokarna är cfg(test).
Kompilatorprober behåller de faktiska kapabilitetstyperna och konsumerande
entrymetoderna med stubbat jämförelsebackend; positiva prober måste kompilera före
negativa tester av konsumtion, privat konstruktion, lånade livstider, Serde och
avsaknad av SQL-/upplåsningsvägar. Runtime-testerna verifierar verkliga bindningar.

Slutverifieringen måste därefter återkontrollera aktuella journal-/Q-/skyddskopie-
identiteter och innehåll, etableringsmarkering, sidofiler, artefaktlayout och
beständighet under samma kvarhållna skydd. AllConnectionsClosed ger ingen rätt
att ta bort Q, ändra generation eller aktivera Ready och aktiverar ingen restore.
Inga SQLite-recoveryåtgärder eller filmutationer utförs av kombinationen.
Linux/Windows recovery förblir Unsupported; ingen fallback införs.
