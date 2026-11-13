# Egna fågelobservationer

**Status — godkända produktkrav, kompletterade den 12 november 2026.** Första macOS-flödet implementeras lokalt. Teknisk design för senare funktioner, däribland kataloguppdatering och regionval, återstår; denna uppdatering aktiverar inga nya funktioner.

## Överenskommen omfattning

- Användaren kan registrera egna fågelobservationer lokalt i MNO.
- Artval utgår från AviList-taxonomi med svenska och engelska namn. Den granskade exporten matchar AviList v2026. Projektägarens beslut om användning av exporten inklusive svenska namn anges nedan.
- Varje observationslista representerar ett separat besök och har en egen identitet. Flera listor kan ha samma plats och datum, exempelvis ett morgon- och ett kvällsbesök. Besökstider är frivilliga och används inte som unik identitet. Omsändning av samma sparförsök får inte skapa en extra lista; en uttryckligen ny lista är ett nytt besök även med samma plats och datum. Beslut från produktdiskussionen den 12 november 2026.
- Varje återanvändbar plats har ett namn och koordinater. En position gäller för hela listan. Användaren kan skapa fler platser för andra positioner.
- Besöket kan ha frivillig start- och sluttid.
- Varje fågelrad kräver art och antal. En frivillig kommentar består av vanlig text. Ett foto är frivilligt i ett senare steg; originalfotofiler bevaras.
- Vid ny registrering används en rad per taxonkoncept och besök. Ett nytt val av en redan tillagd art fokuserar dess antal utan att summera antal eller kasta kommentaren. Befintliga sparade dubblettrader får inte tyst slås ihop eller ändras.
- Artinmatning stöder svenska och engelska namn. När svenskt namn saknas visas engelskt och vetenskapligt namn med en diskret markering att svenskt namn saknas. Arten är fortsatt tillgänglig för registrering.
- Användaren kan lägga till arter en i taget eller klistra in rader som ”Talgoxe 6st”. Tolkade rader granskas och rättas före sparande. Båda inmatningssätten kan användas i samma lista.
- Från en journalanteckning kan användaren skapa en ny observationslista eller länka en befintlig lista.
- Användaren kan också skapa en fristående observationslista och vid behov länka den till journalanteckningar senare.
- Återgång från en lista som öppnats via en journalanteckning går till den ursprungliga anteckningen, oavsett andra journalkopplingar. Återgång från en fristående öppnad lista går till observationsöversikten.
- En lista som skapas från en journalanteckning länkas till den anteckningen när listan har sparats framgångsrikt. Journalanteckningens datum och plats föreslås som redigerbara standardvärden där de finns tillgängliga. Användaren kan ändra dem för listan. Listan behåller sina egna frivilliga besökstider och sitt eget frivilliga väder.
- En lista kan sparas fristående och kopplas till en journalanteckning direkt eller senare. En journalanteckning kan länka till flera listor från olika platser.
- Varje länkad lista visas under journaltexten som en kompakt klickbar förhandsvisning utan stor inramad ruta. Visa plats och eventuella besökstider, följt av högst tre arter med primärnamn och antal; markera när fler rader finns. Besöksdatum visas när det skiljer sig från journalanteckningens datum. Journalprosan är visuellt primär. Separata besök har separata förhandsvisningar. Ett klick öppnar hela listan.
- Varje lista kan ha frivilligt väder för sin egen plats och sitt besök. Vädret gäller listans fågelrader och är oberoende av journalanteckningens väder. Användaren kan ange vädret manuellt eller hämta det när lämpliga data finns tillgängliga. Vädret kan visas kompakt tillsammans med plats och tid.
- Listor kan redigeras och raderas oberoende av journalanteckningar.
- När en journalanteckning raderas bevaras dess länkade observationslistor.
- En observationslista kan länkas till flera journalanteckningar. Alla förhandsvisningar refererar till samma lista, så redigeringar återspeglas överallt där den är länkad.
- Avlänkning bevarar både listan och journalanteckningen. Om en länk tas bort bevaras också listans övriga länkar.
- Att radera en lista kräver bekräftelse. Listans förhandsvisningar tas bort från journalen, medan journaltext och originalfotofiler bevaras.
- MNO:s backup ska bevara alla observationslistor, även listor utan journalkoppling. Artidentifieringar, antal, kommentarer, platser och koordinater, besökstider, väder, lagrade fotoförhandsvisningar, referenser till originalfiler och journalkopplingar ska följa med. Originalfotofiler omfattas av MNO:s befintliga backuppolicy.
- En observationslista är eget innehåll, inte en ny journalanteckning. Att skapa eller spara en lista får inte skapa en extra anteckning eller ändra journalprosan; endast den avsedda kopplingen ska tillkomma.
- **Framtida kalenderkrav:** observationer ska ha en särskild kalendermarkering/egen färg som skiljer dem från journalanteckningar. Detaljerna hör till ett senare gränssnittssteg och kalendern ändras inte i detta steg.
- Journaltexten skrivs av användaren. Automatiska sammanfattningar och statistik skjuts upp.
- Registrering kräver inget externt konto och inget exportsteg. Tjänsteintegrationer och exportfunktioner hör till separat prioritering.

## Överenskommen utgångspunkt för gränssnittet

Skissen journal → observationslista är överenskommen utgångspunkt för gränssnittet. Från journalanteckningen skapar användaren en lista eller länkar en befintlig. En ny lista får redigerbara förslag från journalens datum och plats och länkas efter lyckat sparande. Den öppnade listan har egna frivilliga besökstider och eget väder. Journalen visar listans kompakta förhandsvisning under texten. Den fristående ingången kompletterar detta flöde. Detaljerad utformning och tekniska val återstår.

## Presentation och radkommentarer — beslut 12 november 2026

- Detaljvyn följer journalens typografi, innehållsbredd och metadatastil. Varje art visas i ett kompakt kort med mjuk kontur, subtil bakgrund och försiktigt rundade hörn utan skugga. Antalet ligger nära artnamnet. Kortet växer naturligt med fler namn och kommentar.
- Primärnamnet följer appens språk. Användaren kan välja att också visa det andra språket och att visa vetenskapliga namn. Valen ska kommas ihåg och gälla alla listor, inte enskilda rader; de ändrar inte observationsdata.
- Godkänd reservvisning för saknat svenskt namn gäller oberoende av visningsval: engelskt och vetenskapligt namn samt diskret uppgift om att svenskt namn saknas. Förhandsvisningar använder endast primärnamn/antal, med reservnamnet när det behövs; extra namnval och kommentarer hör till detaljvyn.
- Under skapandet erbjuds **Lägg till kommentar** per artrad. Kommentaren är frivillig vanlig text och visas i det sparade artkortet. Den ska bevaras vid omstart, Backup/Restore och Export/Open Import.
- Denna etapp lägger inte till generell listredigering, foton, väder, kataloguppdatering, regional sökning eller regionkarta. Visuell acceptans av den omarbetade vyn kräver en ny DEV-granskning.

## Kataloglivscykel, namn och region — beslut 12 november 2026

### Referensdata och historisk läsbarhet

- Referenskataloger ska där det är praktiskt möjligt kunna uppdateras som data utan en ny appversion.
- En användarimporterad katalog ska behålla källa och versionsmetadata och valideras före aktivering. En misslyckad import eller aktivering ska lämna den nuvarande katalogen intakt.
- Redan registrerade observationer ska förbli läsbara efter katalogbyten och behålla sin registrerade taxonidentitet och katalog-/taxonomiversion. En ny katalog får inte tyst ersätta den registrerade identifieringen.
- Taxonomiska uppdelningar och sammanslagningar får inte automatiskt omklassificera äldre observationer.
- Samma bevarandeprincip gäller framtida typer av referenslistor. Detta beslut innebär inte att en generell importmotor eller ett pluginramverk ska byggas nu.
- Befintligt ägarbeslut om användning och licenstolkning för AviList-exporten ligger fast. Kataloguppdatering och regionväljare implementeras i senare steg.

### Regional sökning och svenska namn

- Hela AviList-katalogen ska finnas tillgänglig. Arter utanför vald region ska gå att välja direkt utan upplåsning eller särskilt sällsynthetsläge.
- Exakta namnträffar har företräde. Bland i övrigt likvärdiga namnträffar prioriteras arter som ingår i vald region; regional prioritet får inte tränga undan en bättre namnträff.
- BirdLife Sveriges svenska namn ska stödjas vid sidan av befintliga söknamn. Namnens källa och koppling till taxonkoncept ska vara spårbara; ett nytt namn får inte i sig byta en observations taxonidentitet.
- Sökningen ska fortsatt vara skiftlägesokänslig för svenska, engelska och vetenskapliga namn, inklusive Å/Ä/Ö. Originalnamnen bevaras för visning.
- MNO ska komma ihåg vald sökregion. Regional tillhörighet betyder att arten ingår i källans regionlista, inte att den är vanlig på användarens plats. Förekomststatus och lokal abundans får inte blandas ihop.

### Regionväljare

- När region väljs eller ändras visas en offlinekarta i MNO:s visuella stil med dämpad grön markering.
- Kartan ska följa den valda källans verkliga geografiska definition. Ett generellt landurval eller en ungefärlig ruta får inte presenteras som källans exakta region.
- Visa kort beskrivning, källa och version för regionen. Registreringsvyn ska endast ha en kompakt region-/ändrakontroll; kartan hör till själva valet.
- Använd de tillhandahållna VP12-underlagen nedan som referenser. Regionväljaren och kataloguppdateringar ingår inte i den nuvarande implementationen.

### Underlag för regionväljaren

Lokalt tillhandahållna och granskade den 12 november 2026; filerna har inte lagts in i källkodsförrådet:

| Underlag | Kontrollerat innehåll och avgränsning |
| --- | --- |
| `VP12.pdf` — *Checklist of the Birds of the Western Palearctic*, Erling Jirle, Gustav Asplund, Markus Lagerqvist och Martin Stervander | Inledningen anger 2026 års AviList-uppdatering. Sidan 2, **Regional Boundaries**, skiljer Greater Western Palearctic från Lesser/Classic WP och visar utvidgningen med en separat gräns. Sidans beskrivning, öar och undantag ska läsas tillsammans; Greater WP får inte ersättas med klassiskt WP. Förekomstkolumnernas B/b/M/I/V är regional status, inte lokal abundans. |
| `VP12.xlsx`, bladet `VP 11` | Rubriker J4–L4 skiljer **Greater WP**, **Cramp (Classic WP)** och **Europe**. M3–P3 anger vetenskapligt namn, engelskt namn, svenskt namn och förekomstinformation. Exempel: M804–O804 är *Pica pica*, Eurasian Magpie, skata. Filnamnet och bladnamnet fastställer inte ensamma en officiell versionsidentitet. |

Spårning av exakt referens: PDF SHA-256 `cac2dd6c81a2e31f5375f2bb254d47f455962bbb43a907be19ff1316bb4ad35f`; XLSX SHA-256 `8e27299c999c90713b85a227b4d0eab1f086e2147a4c3a7ed21fac2adac60307`.

Före regional dataintegration behöver kopplingen till Avibase-koncept verifieras; de granskade namnkolumnerna är inte en sådan identifierarmappning. Arbetsboken innehåller även felceller i arbetskolumner, exempelvis E804 `#REF!`; de får inte tyst användas som godkänt namnunderlag. PDF-sidan 2 säger ”east of 30°E” samtidigt som den inkluderar flera namngivna Atlantöar. Den uppgiften behöver klarläggas mot källan innan en exakt geografisk gräns implementeras; MNO ska inte själv gissa en korrigering. Detta hindrar inte nuvarande globala katalogsökning.

## Relevanta principer från konceptet

[Concept v0.5](../vision/MNO_CONCEPT_v0.5.md) ger följande vägledning för denna avgränsning.

- **Avsnitt 1–3.** Journalen står i centrum. Lokalt bruk, användarens kontroll och frivillig struktur ska göra det enkelt att anteckna utan obligatorisk administration.
- **Avsnitt 5.** Observationer får stå för sig själva. Osäkerhet ska bevaras och saknade uppgifter ska inte ersättas med påhittad precision. Taxonomiförändringar ska inte tyst ändra en tidigare identifiering.
- **Avsnitt 10 och 13.** Användaren behöver förstå skillnaden mellan att ta bort en koppling och att radera innehåll. Konsekvenserna ska vara tydliga innan en permanent radering.
- **Avsnitt 14.** Återgång mellan observation och journal ska bevara sammanhanget. Grundläggande tangentbordsåtkomst och tydliga etiketter ingår i användbarheten.

Konceptets fristående bibliotek, relationsgraf, analysverkstad, presentationslager, AI-hjälp, synk och tjänsteintegrationer ingår inte i detta kravförslag.

## Granskat dataunderlag

CSV-exporten `avilist-extended-33698-taxa.csv` granskades den 11 november 2026. Den innehåller 33 698 taxonrader totalt. Av dessa har 11 172 rangen `species`. Samtliga 11 172 arter har engelskt AviList-namn och ett ifyllt, unikt Avibase-ID. Svenska namn finns för 11 144 arter och saknas för 28. Namnen och identifierarna finns på samma rader i denna export.

Godkänd reservvisning för de 28 arterna utan svenskt namn är det engelska AviList-namnet tillsammans med det vetenskapliga namnet och en diskret markering att svenskt namn saknas. Arterna är fortsatt tillgängliga för registrering.

### Verifierad version och namnproveniens

Den 11 november 2026 jämfördes samtliga 33 698 rader och 26 taxonomikolumner med Explorers officiella v2026-data enligt dess exportkonvertering. Alla 876 148 celler matchade. Två rich-text-värden i beslutssammanfattningarna exporteras som `[object Object]`; detta är en begränsning i exportens innehåll. CSV-filens taxonomidel är därmed verifierad mot v2026. Versionsmetadata anger publicering den 20 oktober 2026. Rubrikernas kvarvarande versionsplatshållare fastställer inte utgåvan. [Explorers metadata](https://explore.avilist.org/versions.json) · [v2026-data](https://explore.avilist.org/data/avilist-2026.json)

Explorers publicerade klient hämtar alternativa namn från eBirds taxonomi-API med språkparametern `sv`. Den svenska exportkolumnen fylls på artnivå genom uppslag på Cornell-artkod. AviLists officiella beskrivning bekräftar eBird som källa för alternativa namn. eBird anger Erling Jirle, Örjan Sjögren och Markus Lagerqvist som leverantörer och förvaltare av de svenska namnen. Detta verifierar källkedjan, men fastställer inte exakt datum eller revision för namnsvaret när den granskade CSV-filen skapades. [Explorer](https://explore.avilist.org/) · [AviLists beskrivning](https://www.avilist.org/avilist-updates-june-2026/) · [eBirds namndokumentation](https://support.ebird.org/en/support/solutions/articles/48000804865)

### Projektägarens beslut om användning och licenstolkning

Projektägaren har den 11 november 2026 beslutat att MNO ska använda den officiella AviList v2026 Explorer-exporten, inklusive dess svenska namn, för offlineanvändning och som offentliga appdata. Ägaren tolkar den publicerade CC BY 4.0-märkningen som omfattande exporten i dess helhet. Detta dokumenterar ägarens beslut och tolkning. Någon separat verifiering av licensen för den svenska namnkolumnen har inte erhållits. Beslutet ersätter den tidigare öppna tillståndsförfrågan till Cornell/eBird. [AviList v2026](https://www.avilist.org/checklist/v2026/) · [Explorer](https://explore.avilist.org/) · [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)

MNO ska bevara och visa relevant attribution, inklusive AviList Core Team och den dokumenterade eBird-källan för svenska namn, samt ange AviList-version, käll-URL och licenshänvisning. Exportdatum ska registreras där det är känt; annars anges att datumet är okänt. Granskningsdatumet ovan ska inte användas som ett antaget exportdatum. MNO:s filtrering, exempelvis urval av artnivån, och eventuell formatkonvertering ska dokumenteras så att bearbetningen går att följa.

De tidigare granskade eBird-villkoren och avsaknaden av ett separat bekräftat tillstånd för namnkolumnen kvarstår som underlag till ägarens bedömning. [API-villkor](https://www.birds.cornell.edu/home/ebird-api-terms-of-use/) · [Dataåtkomstvillkor](https://www.birds.cornell.edu/home/ebird-data-access-terms-of-use/)

eBird dokumenterar att svenska namn kan omfatta snedstreckskombinationer där taxonomiernas artgränser skiljer sig. Namntäckningen i exporten bevisar därför inte att varje svenskt namn entydigt avser exakt samma taxonkoncept. Sådana etiketter behöver bevaras och granskas utan att MNO gissar en snävare identifiering. [eBirds namndokumentation](https://support.ebird.org/en/support/solutions/articles/48000804865)

Granskad fils SHA-256 är `be9df523541c42736f21696be902eb9adabfcc4b9f4501c2562c88eac4d81ed5`.

## Underlag och senare acceptans

Underlaget är roadmapkortet ”Egna fågelobservationer”, Concept v0.5 och den överenskomna produktdiskussion som återges i arbetsordern för denna uppdatering. Produktdiskussionen fastställer listans innehåll, inmatningssätt, frivilliga journalkopplingar, kompakta förhandsvisningar, listans eget väder samt regler för redigering, flera journalkopplingar, avlänkning, radering samt backupomfattning. Vid genomgång av tillgängligt projektmaterial hittades inget separat fågelvisionsdokument. Ett sådant dokument var därför inte tillgängligt för jämförelse. Eventuella ytterligare krav därifrån behöver granskas separat.

Acceptansen ska använda syntetiska observationer och omfatta följande.

Kommande katalog-/regionsteg ska dessutom verifiera:

- Uppdatera referensdata utan apputgåva där det stöds. Behåll källa/version, förhandsvalidera och avvisa felaktiga data utan att ändra aktiv katalog.
- Öppna observationer från tidigare katalogversion efter uppdatering, även när taxon saknas eller har delats/slagits ihop. Identitet/version och begriplig historisk namnvisning ska bevaras utan tyst omklassificering. Kontrollera även efter backup/återställning.
- Behåll exakta namnträffars företräde; prioritera regionala arter endast bland annars likvärdiga träffar. Välj även en art utanför regionen direkt. Sök både befintliga namn och BirdLife-namn utan att byta taxonkoncept.
- Byt region med offlinekartan, kontrollera dämpad grön markering mot rätt källavgränsning och visa beskrivning/källa/version. Kontrollera ihågkommet val efter omstart och att registreringen endast visar den kompakta regionkontrollen. Regional förekomst får inte beskrivas som lokal abundans.

För själva observationsflödet gäller fortsatt:


- Skapa och återöppna en fristående lista för en plats och ett datum. Återanvänd en namngiven plats med koordinater och använd en separat plats för en annan position. Kontrollera att positionen gäller hela listan.
- Kontrollera att listskapande och omsändning bevarar journalens anteckningsantal, identiteter och prosa och endast lägger till listdata samt avsedd länk. I ett senare kalendersteg: kontrollera att observationer har en särskild markering/färg.
- Spara flera separata besök på samma plats och datum, både med och utan start- och sluttid. Kontrollera att varje besök återöppnas separat.
- Skicka om samma sparförsök efter ett förlorat svar: exakt samma lista och länk ska återges. Återanvänd sparförsökets identitet med annat innehåll: avvisa utan att ändra den sparade listan. Bevara inmatningen när ett svar är osäkert.
- Välj samma taxonkoncept igen under skapande: fokusera befintligt antal utan ny rad, summering eller förlorad kommentar. Återöppna äldre sparade dubblettrader och kontrollera att de inte slås ihop.
- Växla primärnamn med appens språk och prova båda ihågkomna namnvalen över flera listor och omstart. Kontrollera saknat svenskt namn även med vetenskapliga namn avstängda.
- Lägg till, töm och spara en kommentar i skapandet. Kontrollera radbrytningar, att eventuell HTML-liknande text visas som text och att ingen kommentar visas i journalförhandsvisningen. Kontrollera kommentar efter omstart och arkivåterläsning.
- Kräv art och antal på varje fågelrad. Spara rader både med och utan kommentar och foto. Kontrollera att originalfotofilen bevaras.
- Mata in arter med svenska och engelska namn. Blanda enskild registrering och inklistrade rader i samma lista. Granska och rätta tolkningen av exempelvis ”Talgoxe 6st” före sparande.
- Skapa en ny lista från en journalanteckning. Kontrollera att datum och plats föreslås från anteckningen där de finns tillgängliga och kan ändras före sparande. Kontrollera att de ändrade värdena sparas på listan utan att journalanteckningens datum eller plats ändras, och att listan länkas till anteckningen först efter framgångsrikt sparande.
- Länka en befintlig lista från en journalanteckning och kontrollera att länken öppnar samma lista med dess befintliga datum, plats och frivilliga väder.
- Skapa och spara en fristående lista utan journalkoppling. Länka den senare till journalanteckningar och kontrollera att listans egna frivilliga besökstider och väder bevaras.
- Avbryt skapandet från både en journalanteckning och den fristående ingången. Kontrollera att ingen oavsiktlig lista eller journalkoppling lämnas kvar.
- Koppla en lista till journalen direkt och i efterhand. Länka flera listor från olika platser till samma journalanteckning. Kontrollera att journaltexten förblir användarskriven.
- Kontrollera en kompakt förhandsvisning per besök under journaltexten med plats och eventuella tider. Visa datum bara när det skiljer sig från anteckningens datum; behåll journalprosan visuellt primär. Prova listor med färre än tre, exakt tre och fler än tre fågelrader. Visa högst de tre första arterna med antal och markera när fler rader finns. Öppna hela listan från förhandsvisningen.
- Spara en lista utan väder, med manuellt väder och med hämtat väder när lämpliga data finns. Kontrollera att vädret avser listans plats och besök, delas av dess fågelrader och kan skilja sig från journalanteckningens väder utan att det senare ändras.
- Redigera och radera fristående listor. Redigera även en länkad lista oberoende av journalanteckningen.
- Radera en journalanteckning med länkade listor och kontrollera att listorna och deras observationer finns kvar.
- Länka samma lista till flera journalanteckningar. Redigera listan och kontrollera att samtliga förhandsvisningar visar ändringarna. Ta bort en av kopplingarna och kontrollera att listan, journalanteckningen och listans övriga kopplingar finns kvar.
- Avbryt bekräftelsen för radering av en lista och kontrollera att listan och dess förhandsvisningar finns kvar. Bekräfta sedan raderingen och kontrollera att listan och dess journalförhandsvisningar försvinner, medan journaltext och originalfotofiler bevaras.
- Säkerhetskopiera och återställ både länkade och fristående listor. Kontrollera att samtliga listor, artidentifieringar, antal, kommentarer, platser och koordinater, besökstider och väder finns kvar.
- Kontrollera efter återställning att lagrade fotoförhandsvisningar och referenser till originalfiler bevaras och att originalfotofiler hanteras enligt MNO:s befintliga backuppolicy.
- Återställ en lista som är länkad till flera journalanteckningar. Kontrollera att alla kopplingar finns kvar och refererar till samma återställda lista samt att dess förhandsvisningar visar rätt innehåll.
- Kontrollera det granskade underlagets artantal, engelska namn och unika Avibase-ID:n samt svensk namntäckning. Kontrollera engelskt och vetenskapligt namn, diskret markering för saknat svenskt namn och lyckad registrering av sådana arter.
- Kontrollera attribution, AviList-version, käll-URL, licenshänvisning och exportdatum där det är känt samt dokumentation av MNO:s filtrering och formatkonvertering. Kontrollera att ägarens licenstolkning är spårbar.
- Granska svenska etiketter med taxonomiska snedstreckskombinationer och andra avvikande artkopplingar utan att automatiskt anta en snävare identifiering.
- Öppna samma lista från två olika journalanteckningar och kontrollera återgång till den ursprungliga anteckningen i respektive fall. Öppna listan fristående och kontrollera återgång till observationsöversikten även när listan har journalkopplingar.
