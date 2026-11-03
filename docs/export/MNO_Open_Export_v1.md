# MNO – Öppen export, format v1

Status: exportmotor implementerad internt; ingen Tauri-command eller UI ännu.
Återimport ingår inte i steg 1. Exportformatet är fristående från backupformatet.

## Syfte och omfattning

Exportera ALLA lagrade schema-0-journalposter, även tomma eller dolda poster,
utan ändringar i källdatabasen. Exporten innehåller ingen SQLite-fil och är inte
en omdöpt säkerhetskopia. Den kan läsas med vanliga ZIP-, text- och JSON-verktyg.
Den är okrypterad; journaltext och platsreferenser kan vara privata.

Endast schema 0 och radens document_version 1 stöds. Väderlagring, bilder,
bilagor, fågelregister och andra framtida moduler ingår inte. Appinställningar,
webbvyns localStorage, cache och osparad text ingår inte. Inga refererade
sökvägar följs och externa original öppnas, ändras eller raderas aldrig.
En bildreferens eller annan okänd struktur inne i content_json bevaras som text.

## Filstruktur

En `.mno-export` är ZIP/ZIP64 med exakt fyra vanliga filer i roten:

1. `manifest.json` – UTF-8 JSON, manifestet nedan.
2. `journal.jsonl` – auktoritativa journaldata i UTF-8 JSON Lines.
3. `journal.md` – människoläsbar Markdown-vy med reservrepresentation.
4. `README.md` – engelska, fristående läsinstruktioner och integritetsinformation.

Paketen använder lagringsmetoden Stored (ingen komprimering). Det finns inga
katalognamn, absoluta sökvägar eller externa filreferenser i paketstrukturen.
Den befintliga externa backupfilen är ett annat format och ska inte accepteras
som export genom att dess filändelse byts.

## Manifest

Exempel; hashvärden och storlekar måste beräknas från filernas faktiska bytes:

```json
{
  "format": "mno-export",
  "format_version": 1,
  "database_schema_version": 0,
  "document_version": 1,
  "entry_count": 2,
  "files": {
    "README.md": { "size": 1234, "sha256": "<64 gemena hextecken>" },
    "journal.jsonl": { "size": 5678, "sha256": "<64 gemena hextecken>" },
    "journal.md": { "size": 3456, "sha256": "<64 gemena hextecken>" }
  },
  "external_originals_included": false
}
```

`format_version` identifierar endast exportprotokollet. Databasversion och
dokumentversion är separata begrepp enligt D-016. Inga P/Q-, Recovery- eller
backupversioner förändras. Manifestet har ingen självkontrollsumma; ZIP har
även CRC. SHA-256 gäller de okomprimerade filernas exakta bytes, inklusive LF.
Kontrollsummor upptäcker skador men autentiserar inte avsändaren. Framtida läsare
ska avvisa okända versioner, okända fält, extra/saknade paketfiler, felaktiga
kontrollsummor och avvikande postantal, utan delvis import eller tyst databortfall.

## Journalrader och exakt bevarande

Varje rad är ett komplett JSON-objekt följt av LF. Inga tomma rader eller BOM.
Tom journal har en journal.jsonl med noll byte och entry_count 0.
Radordningen är id COLLATE BINARY, oberoende av journalvyns datumordning.
Exakt dessa sju nycklar skrivs i följande ordning:

```json
{"id":"entry-a","entry_date":"2026-10-23","title":"Blåmes","content_json":"{ \"type\": \"doc\", \"content\": [{\"type\":\"paragraph\"}] }","document_version":1,"created_at":"2026-10-23T08:00:00.000Z","updated_at":"2026-10-23T09:00:00.000Z"}
```

De sex textfälten kräver SQL TEXT och giltig UTF-8. document_version kräver SQL
INTEGER med värdet 1. SQL NULL, BLOB, REAL eller andra typer accepteras inte som
ersättning. Ingen typkonvertering sker. ID, titel, datum och tidsstämplar bevaras
ordagrant; exporten inför inte restore-validerarens strängare ID-/datumregler.
Denna export ger ingen ny restore-auktoritet.

content_json är en JSON-STRÄNG, inte ett omskrivet inbäddat JSON-objekt. Efter
avkodning av ytterradens JSON ska dess UTF-8-bytes vara exakt de lagrade bytesen.
Escape-sekvenser, whitespace, nyckelordning, duplicerade inre nycklar och rå
talnotation i dokumentet bevaras därmed. Duplicerade nycklar renderas inte som om
en viss nyckel vann. Dokumentet måste vara ett syntaktiskt giltigt JSON-objekt,
men får vara äldre, tomt eller innehålla okända noder och bildreferenser.
Inga journalposter filtreras bort, inte heller säkert tomma poster.

## Markdown

Datum, titel och post-ID visas för varje rad i samma ordning som JSONL.
Kända strukturer återges som rubriker, stycken, fet/kursiv/genomstruken text,
listor, citat, kod och länkar. Markdown är ingen fullständig representation av
alla editorattribut; exempelvis understrykning och vissa list-/länkattribut
förenklas. JSONL är alltid auktoritativ.

Okända strukturer/attribut, duplicerade JSON-nycklar, dokument över 1 MiB och
kodmarkerad text med LF eller CR visas i sin helhet som märkt
Original-JSON/reservrepresentation. Kodstaketet
väljs längre än dokumentets längsta följd av backticks så att originaltext inte
kan avsluta staketet. Renderaren följer inga länkar och bäddar inte in bilder.
HTML i vanlig text escapes. Okänd struktur medför aldrig en partiell rendering
som låtsas omfatta hela dokumentet.

## Snapshot, resurser och fel

Exportmotorn kräver en separat, redan öppnad read-only SQLite-anslutning utan
pågående transaktion. En framtida appkoppling måste hålla normalt operationstillstånd
under läsningen och respektera Maintenance. Motorn öppnar eller duplicerar inte
SQLite-main-filen. Anroparen äger och stänger sin dedikerade anslutning även vid fel.
En misslyckad rollback får inte tolkas som att anslutningen säkert kan återanvändas.

Schema, integrity_check och alla rader läses inom samma kvarhållna transaktion.
JSONL och Markdown produceras från samma rad. Schema 1 och extra schemaobjekt
avvisas. Kopiering och hashning av temporära exportfiler är strömmande.

Gränser per post:
- content_json: högst 64 MiB UTF-8 (67 108 864 byte).
- Vart och ett av de fem övriga textfälten: högst 1 MiB UTF-8.
- Högst 1 000 000 JSON-värden och JSON-djup högst 64 (rotens djup är 0).
- Markdown-träd byggs endast för dokument upp till 1 MiB; större objekt
  syntaxkontrolleras strömmande och skrivs som original-JSON.
- SQLite LIMIT_LENGTH begränsar materialisering till 64 MiB + 5 MiB + 1 KiB
  på den dedikerade anslutningen. Fältgränserna kontrolleras också efter läsning.
  Detta är en ändlig materialiseringsgräns, inte en garanti om exakt RSS.
- JSONL/Markdown har separata expansionsgränser per rad. Hela journalen
  samlas aldrig i minnet och det finns ingen fast gräns för antal poster.

Korrupt JSON, fel lagringstyp, resursfel, SQLite-/I/O-fel och avbrott stoppar hela
exporten. Inga poster hoppas över. Denna validering är medvetet skild från den
striktare restore-valideringen. Avbrott kontrolleras mellan rader och filblock;
en enskild SQLite-kontroll eller dokumentparsning avbryts inte omedelbart.

Paketet byggs i temporära, ägda filhandtag i målkatalogen. ZIP avslutas och dess
fil synkas innan persist_noclobber ger den slutliga `.mno-export`-sökvägen.
En befintlig fil ersätts aldrig, även om den dyker upp precis före publicering.
Vanliga fel rensar temporärer. Processkrasch kan lämna temporära rester men ingen
ofullständig fil med det begärda slutnamnet. Inga extra SQLite-main-descriptors
öppnas eller stängs av exportmotorn.

Detta är inte ett nytt beständighetsprotokoll: katalogsynk/F_FULLFSYNC och fysisk
strömförlust garanteras inte i steg 1. Efter publiceringsögonblicket kan en komplett
export finnas även om processen därefter dör. Vid strömförlust kan en nyligen
publicerad fil försvinna; inget restore-/Ready-bevis utfärdas av export.
Aktiv same-user pathname-/ABA-manipulation ingår inte i detta stegs hotmodell.

## Obligatoriskt acceptanskrav för återimport, steg 2

Förlustfri återimport av ALLA exporterbara dokument är ett acceptanskrav, även
äldre JSON-strukturer, okända noder, bildreferenser, duplicerade inre nycklar och
dokument som dagens restore-validerare avvisar. De sju fälten ska bevaras exakt.
Varken Markdown-rendering, vanlig autosparning eller strikt restore-validering
får användas för att tappa, normalisera eller välja bort sådant innehåll.

Steg 2 kräver ett separat beslut om kompatibilitet före implementation. En liten
exportspecifik importvalidering och ett explicit tillåtet aktiveringsflöde för
bevarade äldre dokument är ett alternativ. Befintlig restore-validering och
aktivering ändras INTE i steg 1. Ingen färdig återimport utlovas av exportmotorn.
