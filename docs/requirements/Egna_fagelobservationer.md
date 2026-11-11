# Egna fågelobservationer

**Status — kravförslag för produktgranskning.** Beslut om implementation och teknisk design återstår. Val av backupformat och migrering lämnas till den tekniska designen.

## Överenskommen omfattning

- Användaren kan registrera egna fågelobservationer lokalt i MNO.
- Artval utgår från AviList-taxonomi med svenska och engelska namn. Den granskade exporten matchar AviList v2026. Projektägarens beslut om användning av exporten inklusive svenska namn anges nedan.
- Det finns en observationslista per plats och datum.
- Varje återanvändbar plats har ett namn och koordinater. En position gäller för hela listan. Användaren kan skapa fler platser för andra positioner.
- Besöket kan ha frivillig start- och sluttid.
- Varje fågelrad kräver art och antal. En kommentar och ett foto är frivilliga. Originalfotofiler bevaras.
- Artinmatning stöder svenska och engelska namn.
- Användaren kan lägga till arter en i taget eller klistra in rader som ”Talgoxe 6st”. Tolkade rader granskas och rättas före sparande. Båda inmatningssätten kan användas i samma lista.
- Från en journalanteckning kan användaren skapa en ny observationslista eller länka en befintlig lista.
- Användaren kan också skapa en fristående observationslista och vid behov länka den till journalanteckningar senare.
- En lista som skapas från en journalanteckning länkas till den anteckningen när listan har sparats framgångsrikt. Journalanteckningens datum och plats föreslås som redigerbara standardvärden där de finns tillgängliga. Användaren kan ändra dem för listan. Listan behåller sina egna frivilliga besökstider och sitt eget frivilliga väder.
- En lista kan sparas fristående och kopplas till en journalanteckning direkt eller senare. En journalanteckning kan länka till flera listor från olika platser.
- Varje länkad lista visas under journaltexten som en kompakt förhandsvisning med plats, datum, eventuella besökstider och de tre första arterna med antal. Om listan har fler rader framgår det. Ett klick på förhandsvisningen öppnar hela listan. En förhandsvisning visas per lista.
- Varje lista kan ha frivilligt väder för sin egen plats och sitt besök. Vädret gäller listans fågelrader och är oberoende av journalanteckningens väder. Användaren kan ange vädret manuellt eller hämta det när lämpliga data finns tillgängliga. Vädret kan visas kompakt tillsammans med plats och tid.
- Listor kan redigeras och raderas oberoende av journalanteckningar.
- När en journalanteckning raderas bevaras dess länkade observationslistor.
- En observationslista kan länkas till flera journalanteckningar. Alla förhandsvisningar refererar till samma lista, så redigeringar återspeglas överallt där den är länkad.
- Avlänkning bevarar både listan och journalanteckningen. Om en länk tas bort bevaras också listans övriga länkar.
- Att radera en lista kräver bekräftelse. Listans förhandsvisningar tas bort från journalen, medan journaltext och originalfotofiler bevaras.
- MNO:s backup ska bevara alla observationslistor, även listor utan journalkoppling. Artidentifieringar, antal, kommentarer, platser och koordinater, besökstider, väder, lagrade fotoförhandsvisningar, referenser till originalfiler och journalkopplingar ska följa med. Originalfotofiler omfattas av MNO:s befintliga backuppolicy.
- Journaltexten skrivs av användaren. Automatiska sammanfattningar och statistik skjuts upp.
- Registrering kräver inget externt konto och inget exportsteg. Tjänsteintegrationer och exportfunktioner hör till separat prioritering.

## Överenskommen utgångspunkt för gränssnittet

Skissen journal → observationslista är överenskommen utgångspunkt för gränssnittet. Från journalanteckningen skapar användaren en lista eller länkar en befintlig. En ny lista får redigerbara förslag från journalens datum och plats och länkas efter lyckat sparande. Den öppnade listan har egna frivilliga besökstider och eget väder. Journalen visar listans kompakta förhandsvisning under texten. Den fristående ingången kompletterar detta flöde. Detaljerad utformning och tekniska val återstår.

## Relevanta principer från konceptet

[Concept v0.5](../vision/MNO_CONCEPT_v0.5.md) ger följande vägledning för denna avgränsning.

- **Avsnitt 1–3.** Journalen står i centrum. Lokalt bruk, användarens kontroll och frivillig struktur ska göra det enkelt att anteckna utan obligatorisk administration.
- **Avsnitt 5.** Observationer får stå för sig själva. Osäkerhet ska bevaras och saknade uppgifter ska inte ersättas med påhittad precision. Taxonomiförändringar ska inte tyst ändra en tidigare identifiering.
- **Avsnitt 10 och 13.** Användaren behöver förstå skillnaden mellan att ta bort en koppling och att radera innehåll. Konsekvenserna ska vara tydliga innan en permanent radering.
- **Avsnitt 14.** Återgång mellan observation och journal ska bevara sammanhanget. Grundläggande tangentbordsåtkomst och tydliga etiketter ingår i användbarheten.

Konceptets fristående bibliotek, relationsgraf, analysverkstad, presentationslager, AI-hjälp, synk och tjänsteintegrationer ingår inte i detta kravförslag.

## Granskat dataunderlag

CSV-exporten `avilist-extended-33698-taxa.csv` granskades den 11 november 2026. Den innehåller 33 698 taxonrader totalt. Av dessa har 11 172 rangen `species`. Samtliga 11 172 arter har engelskt AviList-namn och ett ifyllt, unikt Avibase-ID. Svenska namn finns för 11 144 arter och saknas för 28. Namnen och identifierarna finns på samma rader i denna export.

Föreslagen reservvisning för de 28 arterna utan svenskt namn är det engelska AviList-namnet tillsammans med det vetenskapliga namnet. Förslaget anger ingen ny svensk översättning.

### Verifierad version och namnproveniens

Den 11 november 2026 jämfördes samtliga 33 698 rader och 26 taxonomikolumner med Explorers officiella v2026-data enligt dess exportkonvertering. Alla 876 148 celler matchade. Två rich-text-värden i beslutssammanfattningarna exporteras som `[object Object]`; detta är en begränsning i exportens innehåll. CSV-filens taxonomidel är därmed verifierad mot v2026. Versionsmetadata anger publicering den 20 oktober 2026. Rubrikernas kvarvarande versionsplatshållare fastställer inte utgåvan. [Explorers metadata](https://explore.avilist.org/versions.json) · [v2026-data](https://explore.avilist.org/data/avilist-2026.json)

Explorers publicerade klient hämtar alternativa namn från eBirds taxonomi-API med språkparametern `sv`. Den svenska exportkolumnen fylls på artnivå genom uppslag på Cornell-artkod. AviLists officiella beskrivning bekräftar eBird som källa för alternativa namn. eBird anger Erling Jirle, Örjan Sjögren och Markus Lagerqvist som leverantörer och förvaltare av de svenska namnen. Detta verifierar källkedjan, men fastställer inte exakt datum eller revision för namnsvaret när den granskade CSV-filen skapades. [Explorer](https://explore.avilist.org/) · [AviLists beskrivning](https://www.avilist.org/avilist-updates-june-2026/) · [eBirds namndokumentation](https://support.ebird.org/en/support/solutions/articles/48000804865)

### Projektägarens beslut om användning och licenstolkning

Projektägaren har den 11 november 2026 beslutat att MNO ska använda den officiella AviList v2026 Explorer-exporten, inklusive dess svenska namn, för offlineanvändning och som offentliga appdata. Ägaren tolkar den publicerade CC BY 4.0-märkningen som omfattande exporten i dess helhet. Detta dokumenterar ägarens beslut och tolkning. Någon separat verifiering av licensen för den svenska namnkolumnen har inte erhållits. Beslutet ersätter den tidigare öppna tillståndsförfrågan till Cornell/eBird. [AviList v2026](https://www.avilist.org/checklist/v2026/) · [Explorer](https://explore.avilist.org/) · [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)

MNO ska bevara och visa relevant attribution, inklusive AviList Core Team och den dokumenterade eBird-källan för svenska namn, samt ange AviList-version, käll-URL och licenshänvisning. Exportdatum ska registreras där det är känt; annars anges att datumet är okänt. Granskningsdatumet ovan ska inte användas som ett antaget exportdatum. MNO:s filtrering, exempelvis urval av artnivån, och eventuell formatkonvertering ska dokumenteras så att bearbetningen går att följa.

De tidigare granskade eBird-villkoren och avsaknaden av ett separat bekräftat tillstånd för namnkolumnen kvarstår som underlag till ägarens bedömning. [API-villkor](https://www.birds.cornell.edu/home/ebird-api-terms-of-use/) · [Dataåtkomstvillkor](https://www.birds.cornell.edu/home/ebird-data-access-terms-of-use/)

eBird dokumenterar att svenska namn kan omfatta snedstreckskombinationer där taxonomiernas artgränser skiljer sig. Namntäckningen i exporten bevisar därför inte att varje svenskt namn entydigt avser exakt samma taxonkoncept. Sådana etiketter behöver bevaras och granskas utan att MNO gissar en snävare identifiering. [eBirds namndokumentation](https://support.ebird.org/en/support/solutions/articles/48000804865)

Granskad fils SHA-256 är `be9df523541c42736f21696be902eb9adabfcc4b9f4501c2562c88eac4d81ed5`.

## Öppna frågor för beslut

- **Journalens länkar.** Hur hittar användaren tillbaka från den fullständiga listan till journalanteckningen?

## Underlag och senare acceptans

Underlaget är roadmapkortet ”Egna fågelobservationer”, Concept v0.5 och den överenskomna produktdiskussion som återges i arbetsordern för denna uppdatering. Produktdiskussionen fastställer listans innehåll, inmatningssätt, frivilliga journalkopplingar, kompakta förhandsvisningar, listans eget väder samt regler för redigering, flera journalkopplingar, avlänkning, radering samt backupomfattning. Vid genomgång av tillgängligt projektmaterial hittades inget separat fågelvisionsdokument. Ett sådant dokument var därför inte tillgängligt för jämförelse. Eventuella ytterligare krav därifrån behöver granskas separat.

Acceptansen ska använda syntetiska observationer och omfatta följande.

- Skapa och återöppna en fristående lista för en plats och ett datum. Återanvänd en namngiven plats med koordinater och använd en separat plats för en annan position. Kontrollera att positionen gäller hela listan.
- Spara besök både med och utan start- och sluttid.
- Kräv art och antal på varje fågelrad. Spara rader både med och utan kommentar och foto. Kontrollera att originalfotofilen bevaras.
- Mata in arter med svenska och engelska namn. Blanda enskild registrering och inklistrade rader i samma lista. Granska och rätta tolkningen av exempelvis ”Talgoxe 6st” före sparande.
- Skapa en ny lista från en journalanteckning. Kontrollera att datum och plats föreslås från anteckningen där de finns tillgängliga och kan ändras före sparande. Kontrollera att de ändrade värdena sparas på listan utan att journalanteckningens datum eller plats ändras, och att listan länkas till anteckningen först efter framgångsrikt sparande.
- Länka en befintlig lista från en journalanteckning och kontrollera att länken öppnar samma lista med dess befintliga datum, plats och frivilliga väder.
- Skapa och spara en fristående lista utan journalkoppling. Länka den senare till journalanteckningar och kontrollera att listans egna frivilliga besökstider och väder bevaras.
- Avbryt skapandet från både en journalanteckning och den fristående ingången. Kontrollera att ingen oavsiktlig lista eller journalkoppling lämnas kvar.
- Koppla en lista till journalen direkt och i efterhand. Länka flera listor från olika platser till samma journalanteckning. Kontrollera att journaltexten förblir användarskriven.
- Kontrollera en förhandsvisning per länkad lista under journaltexten med plats, datum och eventuella besökstider. Prova listor med färre än tre, exakt tre och fler än tre fågelrader. Visa högst de tre första arterna med antal och markera när fler rader finns. Öppna hela listan från förhandsvisningen.
- Spara en lista utan väder, med manuellt väder och med hämtat väder när lämpliga data finns. Kontrollera att vädret avser listans plats och besök, delas av dess fågelrader och kan skilja sig från journalanteckningens väder utan att det senare ändras.
- Redigera och radera fristående listor. Redigera även en länkad lista oberoende av journalanteckningen.
- Radera en journalanteckning med länkade listor och kontrollera att listorna och deras observationer finns kvar.
- Länka samma lista till flera journalanteckningar. Redigera listan och kontrollera att samtliga förhandsvisningar visar ändringarna. Ta bort en av kopplingarna och kontrollera att listan, journalanteckningen och listans övriga kopplingar finns kvar.
- Avbryt bekräftelsen för radering av en lista och kontrollera att listan och dess förhandsvisningar finns kvar. Bekräfta sedan raderingen och kontrollera att listan och dess journalförhandsvisningar försvinner, medan journaltext och originalfotofiler bevaras.
- Säkerhetskopiera och återställ både länkade och fristående listor. Kontrollera att samtliga listor, artidentifieringar, antal, kommentarer, platser och koordinater, besökstider och väder finns kvar.
- Kontrollera efter återställning att lagrade fotoförhandsvisningar och referenser till originalfiler bevaras och att originalfotofiler hanteras enligt MNO:s befintliga backuppolicy.
- Återställ en lista som är länkad till flera journalanteckningar. Kontrollera att alla kopplingar finns kvar och refererar till samma återställda lista samt att dess förhandsvisningar visar rätt innehåll.
- Kontrollera det granskade underlagets artantal, engelska namn och unika Avibase-ID:n samt svensk namntäckning. Om reservvisningen antas, kontrollera engelskt och vetenskapligt namn för arter utan svenskt namn.
- Kontrollera attribution, AviList-version, käll-URL, licenshänvisning och exportdatum där det är känt samt dokumentation av MNO:s filtrering och formatkonvertering. Kontrollera att ägarens licenstolkning är spårbar.
- Granska svenska etiketter med taxonomiska snedstreckskombinationer och andra avvikande artkopplingar utan att automatiskt anta en snävare identifiering.
- Komplettera acceptansen efter beslut om återgång till journalen.
