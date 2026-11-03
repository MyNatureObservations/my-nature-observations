# Återställningssammanhang och säkerhetskontrakt – #9, steg 3A.3b.1

## Ansvar och avgränsning

`backup::restore_context::with_restore_context(app, candidate, cancel, action)` är
enda produktionsingången. Den kör ett synkront, internt callback-flöde och måste
anropas på bakgrundstråd utan dialoger eller väntan på frontend under låset.
Ingen Tauri-command eller frontendfunktion läggs till.

Sammanhanget äger den inkommande `PreparedRestore` och det nyss skapade skyddskopiebeviset.
Det lånar samma underhållsvakt och den aktiva databasbindningen under hela callbacken.
En stackägd vakt utanför callbacken undviker självrefererande strukturer. En högre-rankad
callback hindrar att ett låsbundet sammanhang eller en auktorisering returneras och
används efter att operationens vakt stängts. Vanliga ägda resultat, exempelvis status
eller skyddskopians sökväg, kan returneras. En sökväg är aldrig en auktorisering.

Detta steg kopierar inte till aktiv databas, byter inte databasfil, ändrar inte
journaldata eller generation och skapar ingen återställningsmarkering. Skyddskopior
skapas genom den godkända motorn från 3A.3a; backupformat v1 är oförändrat.

## Flöde och databasidentitet

1. Kontrollera plattformsstöd innan appens datakatalog bestäms. Windows avvisas som
   tidigare med `durability_unsupported`; inga kataloger eller filer skapas där.
2. Hämta `DatabaseState` och databassökvägen från **samma AppHandle**. Den privata
   testvägen kan använda isolerade databaser; produktionskoden erbjuder ingen
   konstruktor som tar godtycklig sökväg och valfri State.
3. Ta befintligt underhållslås. Läs aktuell generation och kontrollera `checked_add(1)`
   innan skyddskopia skapas. En aktiv återhämtningsspärr avvisar sammanhanget.
4. Avvisa länkad databasfil/länkad datakatalog och kanonisera databassökvägen en gång.
   Behåll ett öppet filhandtag och dess identitet: Unix device/inode; Windows använder
   befintligt volym-/filindex-API. Öppet handtag motverkar återanvändning av det gamla
   inode-numret efter borttagning. Jämförelsen bygger inte bara på sökvägssträngar.
5. Läs SQLite-huvudets journalversioner innan en SQLite-anslutning öppnas. WAL avvisas
   redan här, så ett read-only WAL-anrop inte skapar WAL/SHM-filer. Öppna sedan källan
   skrivskyddat utan CREATE och kontrollera också `PRAGMA journal_mode`: endast DELETE.
6. Kontrollera att sökvägen fortfarande pekar på samma vanliga fil. Skyddskopieringen
   använder denna **redan öppnade** anslutning och den upplösta sökvägen; appens
   datakatalog slås inte upp igen. Filidentiteten kontrolleras före och efter skapandet.
7. Bind beviset och lämna sammanhanget till callbacken. Före auktorisering kontrolleras
   filidentitet, operation, generation, spärr och inkommande kandidat igen.

Filidentitet och filhandtag upptäcker vanliga filbyten/omdöpningar och bevarar den
ursprungliga filens identitet. Detta är inte en sandbox mot administratörer eller
skadlig kod med samma användaridentitet som byter filer fram och tillbaka mellan
kontroll och användning, skriver råa bytes eller ändrar databasen från en extern
SQLite-process. Underhållslåset samordnar MNO:s egna databasoperationer. Single-instance-
skyddet kvarstår. MNO_TEST_DATA_DIR/configuration förutsätts stabil under appens körning.

## Skyddskopiebevis och engångsauktorisering

Varje `Maintenance` får ett nytt processlokalt UUID vid låstagning. Bevisets privata
bindning består av operations-ID, aktuell generation, kanonisk sökväg och filidentitet.
`pre_restore::create_bound()` kräver en `ActiveDatabase` vars konstruktor är privat
i sammanhangsmodulen. Den tidigare separata app-/vakt-ingången används inte längre.
Alla verifierings- och beständighetsbarriärer måste lyckas innan bindningen utfärdas.

Ett bevis för annan databas, annan operation eller annan generation avvisas. En
identisk filväg eller ett befintligt backupfilnamn räcker inte. Ursprungliga fristående
skyddskopietesters obundna bevis accepteras inte som ett återställningskontrakt.

`RestoreContext::authorize()` lämnar en icke-klonbar `AuthorizedRestore` som lånar
sammanhanget. Auktorisering kan hämtas en gång; ett nytt försök ger `ProofAlreadyUsed`,
även efter att den första auktoriseringen släppts. Beviset stannar i sammanhanget och
kan inte flyttas ut eller ersättas av anropande kod. Privata fält förhindrar godtycklig
konstruktion. Interna testpunkter för synkning är inte en publik produktionsingång.

Auktoriseringen exponerar den inkommande kandidatens verifierade read-only-anslutning,
generationsplanen och skyddskopians sökväg. **Ingen skrivbar anslutning exponeras.**
Kommande kopieringsfunktion ska konsumera auktoriseringen inom callbacken, kontrollera
aktiv filidentitet igen och får inte öppna en godtycklig målfil från frontend.
Att samma auktorisering inte används två gånger i en framtida kopieringsfunktion
måste också upprätthållas genom dess ägarskapssignatur; sådan funktion finns inte här.

När callbacken avslutas stängs kandidat/proof innan de stackägda anslutningarna och
underhållsvakten släpps. Publicerade skyddskopior bevaras. Vanliga fel återför kontrollen
utan att ändra aktiv data eller generation. Vid panic fungerar Rusts RAII-unwind;
RwLock kan bli poisoned och då nekas nya operationer, inte tyst normal drift.
En byggkonfiguration med panic=abort innebär processavslut, ingen RAII-städgaranti.

## Generation och framtida osäkert tillstånd

`Maintenance::generation()` läser värdet under det exklusiva låset.
`checked_next_generation()` ger nästa värde eller generationsutmattning. Inget värde
skrivs här. Gamla sparsessioner får inte automatiskt få denna nya generation.
Befintliga sparanrop kontrollerar fortfarande generation först efter operationslåset.

Ett separat processlokalt `recovery_required`-tillstånd har lagts till i DatabaseState.
`Maintenance::require_recovery()` kan sätta det under exklusiv åtkomst. Spärren överlever
att underhållsvakten släpps och avvisar normala läsningar/sparningar/backuper via
befintliga operationspermitter. En gammal generation kan fortfarande avvisas tidigare
som generationskonflikt; åtkomst beviljas aldrig. Underhåll kan fortfarande tas för
framtida verifierad återhämtning. Inget återställnings- eller reset-API införs ännu.

Ingen produktionsväg i detta steg sätter spärren. Den är **inte beständig** och ger
inte skydd efter omstart. Den framtida markeringen och uppstartsspärren måste införas
innan aktiv databas får ändras. Kopieringsfel med osäkert resultat ska då sätta spärren
före låssläpp; framgång ska byta generation före normal drift. Nästa generation är
kontrollerad i förväg, men själva generationsbytet hör till senare delsteg.

## Tester och återstående arbete

Isolerade tester täcker rätt bindning, fel fil/operation/State, engångsauktorisering,
att befintlig backup inte ersätter bevis, generations-overflow före skyddskopia,
låsets livstid med en faktiskt blockerad operation, filbyte trots identiska bytes,
kanonisk aliasväg, föråldrad spargeneration, read-only-anslutningar, WAL utan sidofiler,
felstädning och spärr efter låssläpp. Tester av tidigare journal- och backupflöden kvarstår.

macOS testas här. Linux-koden har samma identitets- och låsmodell men är inte körtestad.
Windows är fortsatt avstängt för obligatorisk skyddskopiering; dess filidentitetskod
återanvänder befintlig implementation men behöver körtest när Windows återaktiveras.

Kommande delsteg behöver implementera resultatjämförelse, SQLite-kopiering med säkra
anslutningsinställningar, avbrott/commit-hantering, beständig markering, uppstartskontroll,
generationsbyte och slutligen frontendvalen. Detta sammanhang är inte tillräckligt
för att börja skriva aktiv journal innan dessa återstående kontrakt är uppfyllda.

## Kommande resultatverifiering

Steg 3A.3b.2 tillhandahåller verifierade lässnapshots, exakt strömmande
innehållsjämförelse och ett versionerat logiskt SHA-256-fingeravtryck. Se
[Journal_Content_Verification.md](Journal_Content_Verification.md) för API-kontrakt,
transaktionshantering och exakt bytekodning. Ingen kopiering, beständig markering
eller generationsändring införs av detta delsteg.
