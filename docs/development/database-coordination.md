# Samordning av databasoperationer – #9, steg 3A.1

Detta är en intern grund. Ingen återställning eller datamigrering är implementerad.

## Rust

`DatabaseState` hanteras av Tauri och gäller den enda aktiva naturboken i processen.
Normala operationer tar ett delat tillståndslås innan de öppnar databasen och behåller
permitten tills anslutningen har stängts. SQLite hanterar den vanliga samtidigheten.
Journalens läsningar, sparningar och manuella backup omfattas. Databasinitiering sker
vid uppstart innan appen tas i bruk.

`maintenance()` reserverar först underhållsläge och väntar därefter på exklusiv åtkomst.
Operationer som hinner ta ett delat lås under reservationen avvisas med
`database_maintenance`. Anrop som väntar på exklusivt underhåll kan fortsätta efteråt.
En sparnings generation kontrolleras **efter** låstagningen och före databasöppning.
En gammal generation avvisas med `journal_generation_conflict`.

Underhåll är endast ett internt Rust-API. En framtida återställning måste:

1. Slutföra användarinteraktion och förberedelser före exklusiv låstagning.
2. Låta tidigare databasoperationer avslutas.
3. Använda egna databasanslutningar under underhållspermitten; inte återinträda via
   normala kommandon eller försöka ta ett nytt operationslås.
4. Höja generationen med `advance_generation()` innan spärren släpps när data bytts.
5. Vid fel avgöra om data har ändrats och hålla gränssnittet säkert tills läget är känt.
6. Kasta gammalt UI-tillstånd och läsa om naturboken.

RAII släpper permitterna. Ett förgiftat tillståndslås avvisar operationer.
Generationer finns bara i minnet och börjar om när processen startas; gamla IPC-anrop
överlever inte en processomstart. Ingen generation skrivs till användardatabasen.

Lås hålls inte över Spara-dialogen, frontendbekräftelser eller autosparningens väntetider.
Backup behåller en delad permit under sitt databasarbete och verifiering; andra normala
operationer kan fortsätta. Underhåll väntar tills backupoperationen har avslutats.

## Frontend

Alla journal-lässvar har formen `{ generation, data }`, även ett tomt resultat.
`new_journal_session` ger en generation för nya anteckningar utan databasändring.
`createJournalReader()` kontrollerar sena svar mot aktuell Rust-generation och avvisar
svar från en tidigare begäran hos samma konsument. Ingen automatisk omläsning flyttar
befintlig redigering till en ny generation.

Generationskontrollen av lässvar är en kontroll vid mottagning, inte ett löfte om att
framtida ändringar aldrig kan ske efter kontrollen. En framtida återställning behöver
fortfarande samordna omladdning av UI:t. Rusts kontroll vid varje sparning är det
slutliga skyddet mot gamla skrivningar.

Editorn behåller sin generationsidentitet under hela sessionen och fångar den i
sparningens snapshot. Generationskonflikt stoppar köade sparningar, automatiska
återförsök och manuell sparning i denna session. Text och skydd mot att lämna osparade
ändringar bevaras. Användaren får ett översatt meddelande och måste ta vara på sin text
och öppna anteckningen på nytt; frontend får aldrig märka om gammalt innehåll med en
ny generation.

Testerna använder temporära databaser, kanaler och kontrollerade låstillstånd.
Ingen produktionsväg kan starta underhåll eller höja generationen i detta delsteg.
