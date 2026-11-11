# Egna fågelobservationer

**Status — kravförslag för produktgranskning.** Beslut om implementation och teknisk design återstår.

## Överenskommen omfattning

- Användaren kan registrera egna fågelobservationer lokalt i MNO.
- Artval utgår från AviList-taxonomi med svenska och engelska namn. Tillgången till data och namn behöver klarläggas före implementation.
- Observationer kan kopplas till journalanteckningar när användaren har nytta av det. Kopplingen är frivillig.
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
- **Observationens minsta innehåll.** Vilka uppgifter är obligatoriska respektive frivilliga, exempelvis art, datum, plats, antal och kommentar? Hur ska osäker identifiering uttryckas?
- **Journalens länkar.** Hur ska en kopplad observation visas i journalen och hur hittar användaren tillbaka? Ska flera observationer kunna kopplas till samma anteckning, och en observation till flera anteckningar?
- **Avlänkning och radering.** Vad händer med observationen när en journalanteckning raderas eller en länk tas bort? Vad händer i journalen när en observation raderas? Förslag att granska är att avlänkning bevarar båda objekten och att permanent radering kräver ett separat tydligt beslut.
- **Bevarande.** Hur ska nya observationer och deras kopplingar omfattas av MNO:s backup och återställning? Kravet på bevarande behöver avgränsas innan leverans.

## Underlag och senare acceptans

Underlaget är roadmapkortet ”Egna fågelobservationer” och Concept v0.5. Vid genomgång av tillgängligt projektmaterial hittades inget separat fågelvisionsdokument. Ett sådant dokument var därför inte tillgängligt för jämförelse. Eventuella ytterligare krav därifrån behöver granskas separat.

Efter produktbesluten bör acceptansen omfatta lokal registrering utan externt konto, artval med beslutad namnvisning och en frivillig journalkoppling. Testerna ska också verifiera beslutade konsekvenser för avlänkning, radering och bevarande med syntetiska observationer.
