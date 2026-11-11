# Egna fågelobservationer

**Status — kravförslag för produktgranskning.** Beslut om implementation och teknisk design återstår.

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
- **Journalens länkar.** Hur ska en kopplad lista visas i journalen och hur hittar användaren tillbaka? Ska en lista kunna länkas till flera journalanteckningar?
- **Avlänkning och radering.** Vad händer med listan och dess observationer när en journalanteckning raderas eller en länk tas bort? Vad händer med journalens länkar när en lista eller en observation raderas? Förslag att granska är att avlänkning bevarar båda objekten och att permanent radering kräver ett separat tydligt beslut.
- **Bevarande.** Hur ska nya observationer och deras kopplingar omfattas av MNO:s backup och återställning? Kravet på bevarande behöver avgränsas innan leverans.

## Underlag och senare acceptans

Underlaget är roadmapkortet ”Egna fågelobservationer”, Concept v0.5 och den överenskomna produktdiskussion som återges i arbetsordern för denna uppdatering. Produktdiskussionen fastställer listans innehåll, inmatningssätt och frivilliga journalkopplingar. Vid genomgång av tillgängligt projektmaterial hittades inget separat fågelvisionsdokument. Ett sådant dokument var därför inte tillgängligt för jämförelse. Eventuella ytterligare krav därifrån behöver granskas separat.

Acceptansen ska använda syntetiska observationer och omfatta följande.

- Skapa och återöppna en fristående lista för en plats och ett datum. Återanvänd en namngiven plats med koordinater och använd en separat plats för en annan position. Kontrollera att positionen gäller hela listan.
- Spara besök både med och utan start- och sluttid.
- Kräv art och antal på varje fågelrad. Spara rader både med och utan kommentar och foto. Kontrollera att originalfotofilen bevaras.
- Mata in arter med svenska och engelska namn. Blanda enskild registrering och inklistrade rader i samma lista. Granska och rätta tolkningen av exempelvis ”Talgoxe 6st” före sparande.
- Koppla en lista till journalen direkt och i efterhand. Länka flera listor från olika platser till samma journalanteckning. Kontrollera att journaltexten förblir användarskriven.
- Komplettera acceptansen efter beslut om AviList-underlag, länkvisning, en lista i flera anteckningar, avlänkning, radering samt backup och återställning.
