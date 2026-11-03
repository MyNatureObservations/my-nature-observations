# Beständig borttagning av P och recovery – 4c.2c.3

## Omfattning och kapabiliteter

`ReceiptDurable::remove_pending(self)` konsumerar ett beständigt, verifierat
avslutningskvitto och returnerar `Result<RecoveryDirectoryRemoved, CleanupFailure>`.
Båda typerna äger hela ReceiptDurable och därmed samma session, exklusiva
reservation, DirectoryLease, relevanta Maintenance, operation-/sessions-ID,
SQLite-snapshot, VFS och filhandtag. Inga Clone/Copy/Serde-konstruktorer,
statuskonverteringar, fria sökvägar eller upplåsningsmetoder exponeras.

Den äldre PendingMarker::finish/finish_with är nu endast cfg(test). Ingen
alternativ produktionsväg får radera P utan den nya kapabiliteten. Ingen
frontend, Tauri-återställning eller automatisk uppstartsstädning införs.

## Ordning

1. Återverifiera hela observationen: Recovery-fas och session, DirectoryLease,
   restore-ID, katalog-/databasidentitet, VFS och kvarhållen lästransaktion,
   oförändrat verifierat Old/New-innehåll, skyddskopia och arkivhash, P/Q-bytes,
   layout, sidofiler och avbrottsflagga. Ingen ny journalanslutning öppnas.
2. Bekräfta kvarhållet Q med F_FULLFSYNC(Q), fsync(datakatalog), F_FULLFSYNC(Q).
   Återverifiera igen före den första borttagningen.
3. Om P finns: återverifiera, jämför filtyp, en länk och fysisk identitet mot
   behållet P-handtag med fstatat(AT_SYMLINK_NOFOLLOW), och använd unlinkat på det
   fasta namnet pending-restore.json relativt behållet recovery-handtag.
   Ingen sökvägsbaserad remove_file används i denna produktionsväg.
4. Efter lyckad borttagning kräver det privata verifieringsunderlaget **frånvaro**
   av P. Det ursprungliga handtaget och de exakta pending-bytes behålls. En ny P
   på samma namn avvisas, även med giltigt eller identiskt JSON-innehåll.
5. Verifiera den nya layouten. fsync(recovery) och därefter F_FULLFSYNC(Q) måste
   lyckas, följt av återverifiering, innan P betraktas som beständigt borta.
6. Verifiera igen före rmdir: hela beviskedjan, tom katalog och samma fysiska
   recovery-objekt. unlinkat(root_fd, "recovery", AT_REMOVEDIR) avvisar atomiskt
   icke-tom katalog. Inga barn raderas rekursivt och inga okända filer städas.
7. Efter lyckad rmdir kräver underlaget frånvaro även av recovery. Behåll dess
   handtag. fsync(datakatalog) och F_FULLFSYNC(Q), följt av full slutkontroll,
   krävs före RecoveryDirectoryRemoved. Q:s bytes och namn lämnas oförändrade.

Identitetskontrollen upprepas direkt vid namnoperationerna. Kontrollerna är
inte en atomisk compare-inode-and-unlink-primitiv. Processlåset samordnar MNO-
processer; det är ingen allmän spärr mot godtycklig manipulation av samma
användares filer. Namnoperationerna begränsas till fasta MNO-namn under behållna
kataloghandtag, utan följning av symlänkar eller rekursiv radering. Upptäckta
fil-/katalogbyten stoppar operationen. Detta ger inte en garanti mot en fientlig
process som ändrar namn exakt efter sista kontrollen; starkare skydd mot sådan
manipulation kräver en annan hotmodell/OS-isolering.

## Förväntad layout och återupptagning

Endast den konsumerande städningen ändrar sina privata layoutförväntningar efter
lyckat unlinkat. Generell P/Q-klassificering har inte liberaliserats: den laddar
faktiskt tillstånd på nytt och kräver fortsatt strikt verifiering. Förväntningarna
är inte fria verifieringsbevis.

| Verifierat underlag | Åtgärd |
| --- | --- |
| P + Q | Verifiera och synka Q; ta bort P och sedan tom recovery med barriärer. |
| Q + tom recovery | Ingen P skapas. Bekräfta dess frånvaro med recovery-fsync och Q-barriär, sedan rmdir. |
| Endast Q | Verifiera och synka Q/root, bekräfta frånvaro och slutkontroll. Ingen katalog skapas. |
| Konflikt, okända filer, osäkra identiteter eller SQLite-sidofiler | Avvisa och behåll spärren. |

Återupptagning efter krasch kräver en **ny** RecoverySession → Assessment →
CompletionProof → ReceiptDurable. Ingen diagnostisk status eller CleanupProgress
kan ge rätt att fortsätta. Detta är ett internt API; uppstarten kör inte flödet
automatiskt. Q finns kvar och spärrar fortsatt normal uppstart/drift.

## Fel, framsteg och krascher

CleanupFailure innehåller typat verifierings-, I/O- eller Unsupported-fel
samt ett diagnostiskt CleanupProgress. Stegen är Unchanged, PendingUnlinkAttempted,
PendingUnlinked, PendingDurable, RecoveryUnlinkAttempted, RecoveryUnlinked och
RecoveryDurable. Attempted sätts före namnoperationen och lovar inte att ett
misslyckat systemanrop lämnade filsystemet oförändrat. Durable-steg betecknar
senast bekräftade barriärer, inte en fristående eller tidlös filidentitetsgaranti.

Alla fel stoppar vidare städning. Ingen P återskapas, Q tas aldrig bort och ingen
blind rollback görs. Felresultatet behåller resurserna; Drop frigör bara dessa
normalt och sessionsplatsen, aldrig Recovery-spärren eller generationen. Detsamma
gäller panik. Synkfel kan inträffa efter en redan genomförd namnoperation; sådan
borttagning försöker MNO inte ångra.

| Kraschpunkt | Möjlig observerad layout efter processkrasch | Nästa uppstart |
| --- | --- | --- |
| Före P-unlink | P + Q | Spärr; ny full verifiering före fortsatt städning. |
| Efter P-unlink | Q + tom recovery | Spärr; verifiera från Q och bekräfta barriärer igen. |
| Före recovery-fsync | Q + tom recovery | Samma; ingen beständighetskvittens finns ännu för P-borttagningen. |
| Efter recovery-fsync, före Q-slutbarriär | Q + tom recovery | Samma; kvittera inte enbart utifrån katalogsynkningen. |
| Före recovery-rmdir | Q + tom recovery | P-borttagningen har bekräftats; verifiera allt på nytt. |
| Efter recovery-rmdir | Endast Q | Spärr; verifiera Q/journal/skyddskopia och bekräfta root-barriärer. |
| Före root-fsync | Endast Q | Samma; katalogborttagningen är ännu inte kvitterad. |
| Efter root-fsync, före Q-slutbarriär | Endast Q | Samma; avslutningskapabilitet har ännu inte utfärdats. |

Tabellen beskriver processkrasch. Vid strömavbrott kan ännu inte bekräftade
namnoperationer återkomma eller saknas. De tre giltiga layouterna ovan verifieras
på nytt; övriga tillstånd blockerar. Inga tester bevisar fysiskt strömavbrotts-
beteende eller att hårdvaran respekterar F_FULLFSYNC/fsync. Inga best-effort-
fallbacks finns. macOS körtestas. Linux/Windows förblir Unsupported för recovery.

## Tester och återstående arbete

Isolerade tester täcker alla tre layouter med Old/New, full barriärordning,
varje synkfel, injicerade unlink-fel, verkligt ENOTEMPTY, filbyten/symlänkar,
återkommande P/recovery, okända filer, avbrott före/efter namnoperationer,
panik, bevarade snapshots/reservationer och oförändrad generation under
Maintenance vid både framgång och sent fel. Subprocesser dödas efter handskakning
vid samtliga åtta kraschpunkter; efterföljande processlåsning och ny verifiering
prövar återupptagning. Kompilatortester använder faktiska kapabilitetsdefinitioner
med teststubbar för I/O och verifiering, kompletterat av de verkliga runtime-testerna.

4c.2c.4 behöver separat granskade nästa avslutssteg för kvarvarande Q,
resursstängning och säker driftövergång. Inget av dessa steg är aktiverat här.
RecoveryDirectoryRemoved är inte StartupReady. Ingen produktionsrestore,
automatisk SQLite-recovery, generationsändring eller ändring av backupformat v1
har införts. Tester använder endast tillfälliga, isolerade datakataloger.
