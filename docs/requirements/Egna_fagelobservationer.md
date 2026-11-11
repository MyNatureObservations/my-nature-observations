# Egna fågelobservationer

**Status — kravförslag för produktgranskning.** Beslut om implementation och teknisk design återstår. Val av backupformat och migrering lämnas till den tekniska designen.

## Överenskommen omfattning

- Användaren kan registrera egna fågelobservationer lokalt i MNO.
- Artval utgår från AviList-taxonomi med svenska och engelska namn. Tillgången till data och namn behöver klarläggas före implementation.
- Det finns en observationslista per plats och datum.
- Varje återanvändbar plats har ett namn och koordinater. En position gäller för hela listan. Användaren kan skapa fler platser för andra positioner.
- Besöket kan ha frivillig start- och sluttid.
- Varje fågelrad kräver art och antal. En kommentar och ett foto är frivilliga. Originalfotofiler bevaras.
- Artinmatning stöder svenska och engelska namn.
- Användaren kan lägga till arter en i taget eller klistra in rader som ”Talgoxe 6st”. Tolkade rader granskas och rättas före sparande. Båda inmatningssätten kan användas i samma lista.
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

## Relevanta principer från konceptet

[Concept v0.5](../vision/MNO_CONCEPT_v0.5.md) ger följande vägledning för denna avgränsning.

- **Avsnitt 1–3.** Journalen står i centrum. Lokalt bruk, användarens kontroll och frivillig struktur ska göra det enkelt att anteckna utan obligatorisk administration.
- **Avsnitt 5.** Observationer får stå för sig själva. Osäkerhet ska bevaras och saknade uppgifter ska inte ersättas med påhittad precision. Taxonomiförändringar ska inte tyst ändra en tidigare identifiering.
- **Avsnitt 10 och 13.** Användaren behöver förstå skillnaden mellan att ta bort en koppling och att radera innehåll. Konsekvenserna ska vara tydliga innan en permanent radering.
- **Avsnitt 14.** Återgång mellan observation och journal ska bevara sammanhanget. Grundläggande tangentbordsåtkomst och tydliga etiketter ingår i användbarheten.

Konceptets fristående bibliotek, relationsgraf, analysverkstad, presentationslager, AI-hjälp, synk och tjänsteintegrationer ingår inte i detta kravförslag.

## Öppna frågor för beslut

- **AviList och namn.** Vilken utgåva och åtkomstväg ska användas? Vilka villkor gäller för lokal distribution och uppdatering? Varifrån kommer svenska och engelska namn, hur kopplas de till AviList och vad visas när ett namn saknas? Detta är ännu inte verifierat.
- **Journalens länkar.** Hur hittar användaren tillbaka från den fullständiga listan till journalanteckningen?

## Underlag och senare acceptans

Underlaget är roadmapkortet ”Egna fågelobservationer”, Concept v0.5 och den överenskomna produktdiskussion som återges i arbetsordern för denna uppdatering. Produktdiskussionen fastställer listans innehåll, inmatningssätt, frivilliga journalkopplingar, kompakta förhandsvisningar, listans eget väder samt regler för redigering, flera journalkopplingar, avlänkning, radering samt backupomfattning. Vid genomgång av tillgängligt projektmaterial hittades inget separat fågelvisionsdokument. Ett sådant dokument var därför inte tillgängligt för jämförelse. Eventuella ytterligare krav därifrån behöver granskas separat.

Acceptansen ska använda syntetiska observationer och omfatta följande.

- Skapa och återöppna en fristående lista för en plats och ett datum. Återanvänd en namngiven plats med koordinater och använd en separat plats för en annan position. Kontrollera att positionen gäller hela listan.
- Spara besök både med och utan start- och sluttid.
- Kräv art och antal på varje fågelrad. Spara rader både med och utan kommentar och foto. Kontrollera att originalfotofilen bevaras.
- Mata in arter med svenska och engelska namn. Blanda enskild registrering och inklistrade rader i samma lista. Granska och rätta tolkningen av exempelvis ”Talgoxe 6st” före sparande.
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
- Komplettera acceptansen efter beslut om AviList-underlag och återgång till journalen.
