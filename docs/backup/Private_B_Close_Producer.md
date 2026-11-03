# Privata B/C-close-producenter

`PreparedWriteEpisode::close_b(self)` i `active_close.rs` är en privat,
produktionsdefinierad metod. Den har inga anrop från app, Tauri eller startup.
Den kompileras på macOS med `panic="abort"`, eller i macOS-testbyggen som
installerar den verkliga terminala panikhooken i separata barnprocesser.
Produktions-debug med unwind har ingen sådan metod.

Den befintliga ägarkedjan behålls: PreparedWriteEpisode → WriteEpochInputs →
WriteInputs → CloseOwner → ActiveCloseParts. Yttre RestoreMaintenanceScope
äger verklig Maintenance och samma TerminalPolicy. C, durable P, session,
reservation och DirectoryLease-bindning frigörs inte av B-close.

Före armering krävs policy, A=Closed, Live med B och C, rätt generation samt
befintliga session-/P-/identitetskontroller. Före-armeringsfel behåller hela
ägaren och ändrar inte aldrig-armerad Drop-semantik. Policyn armeras före
B:s direkta swap till BTransferGuard. Den konsumerande try_close-returen lagras
direkt i vaktens outcome-slot utan mellanliggande callback eller feldiagnostik.
Globala abort-hooken skyddar konsumerande anropsramar mot Rust-unwind;
vaktens egen Drop ensam skyddar inte interna rusqlite-ramar.

Verkligt Ok flyttas till SourcePhase::BClosed med samma C. Verkligt Err flyttas
till BFailed med hela PreparedCloseFailure, original-Connection och originalfel.
Flyttsekvensen kör ingen fallibel användarkod mellan uttag och fasinstallation.
Gamla Live har tomma B/C-slots när den ersätts. BFailed returneras i befintlig
ActiveCloseFailure med BSourceClose; originalfelet ägs fortfarande av fasen.

BClosed betyder endast att B:s SQLite-close lyckats explicit och att
retained-underlaget ägs. Lånad filkontroll görs därefter. Misslyckad kontroll ger
BFileCheck med BClosed och C kvar i felägaren; SQLite-close görs inte ogjord.
None är aldrig ett closebevis. B-close skapar ingen CompletionProof-,
klassificerings- eller Ready-väg. CClosed representeras nedan av Closed.

BTransferGuard har oförändrad aborterande Drop, även när den är tom. Efter
full fasinstallation kontrollerar producenten därför att båda slots är tomma
och fasen är BClosed/BFailed. Först då undertrycks just denna resurstomma
stackvakts Drop med mem::forget. Det finns då endast en lånad policyreferens
och tomma Options kvar; inga resursägare eller heapallokeringar läcks där.
Undantaget ligger inline, inte i en allmän finish/disarm-metod. Yttre policy,
CloseOwner och Maintenance-scope är fortfarande Armed och aborterar vid Drop.

BUSY tillåter lånad diagnostik och fortsatt ägande inom samma levande scope,
inte normal återgång till UI eller Maintenance-release. Ingen automatisk retry
finns. C-close får endast fortsätta från BClosed, inte BFailed. Panik stoppar processen före efterföljande Rust-unwind;
den kan inte återfå en konsumerad Connection eller hindra cleanup som redan
skett innan paniken uppstod. Processabort är inte durability-/restorebevis.

På macOS är source-hashningen positionsbaserad över befintlig File och
öppnar/duplicerar/stänger inga extra main-fd. Retained-filkontrollen efter OK
använder samma fd. Separata processprober kräver bevarat B-lås före close och
efter BUSY samt möjlighet att ta exklusivt lås efter OK. Detta bevisar inte
alla fd-alias, andra plattformar eller framtida writer-/VFS-ordning.

Tester använder verklig prepare_episode, B/C, Maintenance, P och session.
De kontrollerar phase/handle/fd-identitet, reservation/lease, file-check-fel,
panikpunkter, ofullständig handoff, utebliven avsedd panik, armerad ägar-Drop och
startup Recovery. Testprojektioner för kompilatorprober använder den faktiska
konsumerande signaturen; SQLite-/handoff-kroppen verifieras med verkliga resurser
och separat releasekompilering, inte genom stubbar.

Före användaraktivering återstår hel-episodavslut, säker felkoordinator,
writer/mål/VFS/POSIX, klassificeringsport, completion/admission och verifierad
distributionsartefakt. Denna privata producent aktiverar inget av dessa.

## C-close i samma armerade episod (5d.5m)

Privat `close_c(self)` har samma macOS/test-eller-abort-gräns som close_b och
saknar appanrop. Den kräver exakt Armed, A=Closed och komplett BClosed. Live,
BFailed, CFailed och redan Closed avvisas med Binding utan retry. BClosed bär
Some(B-underlag), Some(VerifiedProtection) och None i en ny record-slot.
Fältordningen är B, C, record. Record har inga SQLite-/fd-resurser.

Gemensam kontroll av A/P/session/bindning har brutits ut ur check_after;
ursprungliga anrop kräver fortfarande Live före I/O. C-close kontrollerar även
B:s retained-fil och C:s source genom befintliga handtag före konsumtion.
VerifiedProtection::into_parts överför C till en redan etablerad tom
BTransferGuard och ProtectionRecord till BClosed.record. Därefter lagras
try_close-resultatet direkt i vaktens outcome. Ingen ny vakt eller policy behövs.

Isärtagning och fasinstallation har kortlivade tuple-/matchtemporärer. De
skyddas av pre-unwind-policyn, inte av påståendet att de alltid ligger i ett
vaktfält. Ingen callback, I/O eller vanlig felretur finns i dessa slutna
flyttsekvenser. Tester injicerar panik före konsumtion och efter skyddad
resultat-/fasplacering, inte mitt i en temporär payload.

C-Ok producerar Closed med verkliga B/C-close-underlag och samma record.
C-Err producerar CFailed med B-underlag, original-Connection/originalfel i
PreparedCloseFailure och samma record. Gamla BClosed är resurstom innan den
ersätts. Bara efter full fasinstallation och verifierat tomma guardslots glöms
lokalvakten; yttre CloseOwner och Maintenance-scope förblir Armed.

En privat lånad check_abc_connections_closed kräver A=Closed, SourcePhase::Closed
samt samma giltiga P/session/bindning. Ingen fristående kapabilitet exporteras.
Closed betyder uttryckligen bara episodens verifierade SQLite-handle-close.
Retained-filerna är fortfarande öppna. Efterföljande filkontroll kan misslyckas;
hela Closed-ägaren returneras då i felägaren och förblir terminalt skyddad.

BUSY returnerar CSourceClose utan automatisk retry eller normal Maintenance-
release. P, session, reservation och DirectoryLease-bindningen behålls. Även
full Closed förbjuder normal Drop av den armerade episoden. Ingen writer,
CompletionProof, AllConnectionsClosed, klassificering, Finished eller Ready
produceras. Befintliga fristående databasöppningar gör ett globalt påstående om
alla appens anslutningar otillåtet.

C-proven använder verkliga resurser och tretton subprocesscenarier. De jämför
C:s SQLite-handle, retained-fd/filidentitet och record-bindning där close
återvänder, kontrollerar samma yttre ägare och P-bytes/startup Recovery efter
barnets slut. Separata processprober kontrollerar B-lås släppt/C-lås kvar före
C-close, C-lås kvar efter BUSY och släppt efter Ok. Negativa kontroller kräver
status 77 före senare Drop vid utebliven avsedd panik samt abort före verifierad
tom vakt vid utebliven handoff. Exakt markörordning krävs utöver SIGABRT.

Detta bevisar inte alla inode-alias, nativefel, hookersättning, strömavbrott,
distributionsartefakt eller framtida writer/VFS-finalisering. Samlad completion,
klassificering och normal Maintenance-release kräver separata kontrakt.

## P2-rättning: namnvalidering utan extra main-fd

Gemensam P-validering använder nu lstat-baserad symlink_metadata och metadata
från det behållna P-handtaget: vanlig fil, ingen symlänk, samma dev/inode och
ett länkat namn. P-bytes läses fortfarande positionsbaserat från originalfd.
DirectoryLease::ensure_current återöppnar inte lockfilens namn på Unix; den
jämför namnets metadata med originalhandtaget och behåller befintlig ägar-,
länkantal-, filtyp- och katalogidentitetskontroll. Katalogöppningar behåller
O_DIRECTORY och kan inte öppna en vanlig SQLite-main-fil. Övriga plattformars
lockvalidering är oförändrad.

Detta fryser inte namn mot samtidiga ändringar. Kontrollerna är observationer;
efterföljande ägande och läsning använder behållna handtag. Även en ersättning
mellan metadatakontroller kan därför inte orsaka open/close av C:s inode genom
dessa två namnvalideringar. En stabil felaktig alias avvisas före C-close.
Ingen garanti om atomär beständig namngivning eller framtida mutation införs.

Sex tillkommande subprocesscenarier (totalt nitton) prövar P-/lockfilshårdlänk,
samtidig namnväxling och negativa kontrollfall. De positiva fallen kräver
avvisning med komplett BClosed/C kvar samt ett externt exklusivt SQLite-lås
fortfarande blockerat före terminal ägar-Drop. Racefallen växlar namn medan
lånad validering och 32 externa låsprober körs per namn. De negativa fallen
stänger avsiktligt ett extra alias-fd endast i barnets testkod, verifierar att
låset faktiskt tappas och avslutar med status 78 före någon yttre Drop.
Originalnamnen återställs för P-byte- och startup-Recovery-kontroller; inga
originaldata eller produktionskataloger används.
