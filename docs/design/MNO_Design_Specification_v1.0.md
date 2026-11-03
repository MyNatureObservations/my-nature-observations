# My Nature Observations (MNO)
## Design Specification v1.0

**Status:** Sammanställd produkt- och UX-specifikation\
**Datum:** 12 oktober 2026\
**Produktvision:** *Your nature. Your observations. Your way.*\
**Designprincip:** *The interface whispers. Your story speaks.*\

> **Versionsförtydligande:** Detta är specifikationens version 1.0, inte appens versionsnummer. Första planerade appversionen är **MVP v0.1**.

> **Senare scopebeslut 2026-10-31:** enligt [D-017](MNO_Design_Decisions.md#d-017--temasystem-efter-v01) accepteras nuvarande ljusa utseende för v0.1. Dokumentets ursprungliga formuleringar om ljust/mörkt systemtema bevaras som historik men är inte längre v0.1-krav. Temasystem med systemval, manuellt val och möjliga naturteman skjuts till efter v0.1; namn och UX är preliminära. Tillgänglighet och kontrast i levererat ljust utseende gäller fortfarande.

## 1. Syfte och avgränsning

MNO är en personlig naturdagbok som ger användaren ett lugnt och långsiktigt hem för berättelser, naturupplevelser och, i senare faser, observationer, arter, platser och fotografier. Upplevelsen och användarens egna ord står i centrum. MNO ska vara enkel att börja använda men kunna växa med materialet över många år.

Specifikationen sammanfattar fastställda principer och funktionella beslut från designsamtalen. Den är **inte** en färdig teknisk arkitektur eller en utfästelse om att samtliga funktioner ingår i v0.1. Där ett detaljerat beteende ännu inte beslutats markeras det som öppet.

## 2. Icke förhandlingsbara principer

1. **Local-first och offline:** kärnfunktionerna fungerar utan nätverk. Ingen obligatorisk inloggning, molntjänst, prenumeration eller telemetri.
2. **Användaren äger sitt material:** data ska vara portabla, kunna säkerhetskopieras och exporteras.
3. **Ingen obehörig filhantering:** MNO får aldrig radera, skriva över eller ändra originalfiler utanför appens egna hanterade lagringsutrymme. Länkade original är läsbara endast med användarens medgivande. Externa säkerhetskopior och exporter lämnas orörda.
4. **Ingen oombedd inventering:** inga automatiska sökningar på diskar, inga bakgrundsskanningar av fotoarkiv och inga oombedda platsuppslag.
5. **Inga påhittade fakta:** MNO får inte gissa arter, platser, datum eller metadata och presentera dem som säkra.
6. **Quiet, not hidden:** gränssnittet är diskret, men centrala handlingar ska vara lätta att upptäcka.
7. **KISS:** börja med en riktigt bra dagbok; inför specialfunktioner när de skapar tydligt värde.
8. **En gemensam verklighet:** journal, observationer, arter, platser och bilder ska länka till samma underliggande uppgifter, inte skapa parallella kopior.
9. **Plattformsoberoende:** målet är en lokal, fri och öppen applikation för flera operativsystem.

## 3. Visuell identitet och interaktion

- Känsla: luftig, stillsam, varm, naturbok snarare än administrationssystem; Japandi-inspirerad utan dekorativ överlast.
- Föreslagen palett: varmvit `#F8F7F3`, text `#343530`, dämpad grön `#788E7B`, linjer `#D9D8D1`, redigeringsyta `#F0F0EA`.
- Serif för längre journaltext, sans-serif för navigering och kontroller. Inga konstgjorda pappersskuggor.
- Läsbredd för text cirka 570 px; större bilder upp till cirka 900 px, responsivt och utan tvångsbeskärning.
- Ljust/mörkt tema följer operativsystemet. Språk: svenska och engelska.
- Textstorlek i tre användarval: **Mindre / Normal / Större**. Ingen fri typsnittsväljare i första skedet.
- Navigeringen ska komma ihåg kontext och exakt läsposition när användaren går tillbaka.

## 4. Informationsarkitektur

**Huvudområden:** Journalen, Platser, Observationer och Fotografier. Inom Observationer finns bland annat Artlista och Observationstillfällen. Dessa områden hör till den långsiktiga helheten och är inte alla MVP-krav.

- Vänster huvudnavigering kan vara dold som standard.
- Höger kontextpanel för verktyg och sammanfattningar kan öppnas/stängas; valet bevaras mellan sidor och appstarter. På smala skärmar används överlägg.
- Fullständiga analyser och översikter visas i huvudinnehållet.
- En konsekvent, kontextkänslig **+ Nytt** ger rätt startpunkt beroende på var användaren befinner sig.
- Global sökning ska hjälpa användaren att hitta innehåll utan att tvinga fram separata registerkopior.

## 5. Journalen – produktens kärna

### 5.1 Läsning och kronologi

- En sammanhängande kronologisk journal som visar datum med faktiskt innehåll; inga tomma dagar behöver visas.
- Stor dagssiffra, mindre veckodag/månad/år, diskreta avdelare och större mellanrum mellan månader och år.
- En subtil botanisk årstidsvinjett vid den första innehållsbärande månaden i en ny årstid, anpassad efter användarens hemregion/halvklot när detta är känt.
- Klistrad/kvarliggande månad-år-indikator kan öppna tidslinje eller kalender.
- Appen öppnar vid den **senaste befintliga berättelsen**, inte vid en tom sida för dagens datum.
- Berättelser visas i sin helhet; obligatoriska titlar och automatiska utdragskort undviks.

### 5.2 Berättelser och redigering

- Flera självständiga berättelser kan finnas på samma datum. De visas tillsammans och kan flyttas upp/ned manuellt. Standard är skapelseordning, inte klockslag.
- En ny berättelse öppnas direkt i redigeringsläge med markören i texten.
- Standarddatum är idag, eller det datum användaren utgick från; datum kan ändras senare.
- Datumet beskriver **upplevelsen**, inte när texten senast redigerades. Ändringstid kan sparas internt men visas inte i läsläget.
- Rubrik är frivillig och läggs till via **+ Lägg till rubrik**. Utan rubrik används textutdrag vid sökning/val.
- Enkel rik text: fet, kursiv, länk och en nivå underrubrik.
- Autosparning. **✓ Klar** avslutar redigering, men är inte en sparknapp. Klick utanför ska inte ofrivilligt avsluta redigering. Navigering får inte kasta bort ändringar.
- Ingen separat utkastkategori: när text finns är den en vanlig berättelse; helt tomma sidor sparas inte.
- Samma berättelsetyp kan rymma allt från kort fältnotering till längre artikel.

### 5.3 Innehållskopplingar

- Observationstillfällen kan frivilligt kopplas till en viss berättelse på samma datum och visas i dess sammanhang.
- Observationer utan berättelse är fortfarande synliga för dagen.
- **Hör till berättelse / Ingen berättelse** kan ändras utan att observationer dupliceras.
- Observation skapad inifrån berättelse kopplas automatiskt dit.
- Ett observationstillfälle med många arter är fortfarande **ett** observationstillfälle, inte en journalpost per art.

## 6. Observationer – senare fas

- Ett observationstillfälle är en sammanhållen sida med datum, eventuell tid, plats, källa, kommentarer, artposter och karta när koordinater finns.
- Naturboksliknande presentation. Artposter sorteras språkkorrekt, inklusive Å/Ä/Ö i svenska.
- Arter med enbart antal visas kompakt; arter med anteckningar eller bilder kan visas mer utförligt.
- Klick på ett observationskort öppnar hela observationen direkt; tillbaka återställer läspositionen.
- Manuell observation ska kunna skrivas på en levande sida utan obligatorisk steg-för-steg-guide; idag förifylls.
- Flera arter kan **läggas till** genom sökning eller inklistring med förhandsgranskning; detta är inte ett beslut om massredigering av befintliga artposter.
- Import, exempelvis eBird, ska bevara relevanta källidentifierare och proveniens. Källuppgifter får inte tyst slås ihop eller förvanskas.

## 7. Arter – senare fas

- En artsida är ett personligt kapitel, inte ett uppslagsverk med generiska stockbilder.
- Kan visa användarens bilder, första/senaste observation, besökta platser och observationer genom åren – även sådana utan foto.
- **Mina tankar** är en separat personlig textyta, skild från kommentarer om enskilda möten.
- Användaren kan välja ett beständigt omslagsfoto. Utan eget val får MNO välja ett tillfälligt omslag från användarens bilder; finns ingen bild används inte ett påhittat foto.

## 8. Platser – senare fas

- Platssidan prioriterar historiken: daterade besök och berättelser/observationer kopplade till platsen.
- En liten karta kan expanderas när koordinater finns; en plats kan även finnas med enbart namn.
- Landskapsbilder kan kopplas till plats utan att knytas till en art.
- Användaren kan välja omslagsbild; annars fungerar ren typografi.
- Ett gemensamt platsregister används. Stabilt importerade plats-ID:n (t.ex. från eBird) får inte automatiskt slås ihop enbart på namnlikhet.
- Koordinater för ett enskilt observationstillfälle kan skilja sig från platsens standardkoordinater.
- Ett fotos GPS-position avser kamerans position och är inte automatiskt observationens plats.

## 9. Fotografier – senare fas

### 9.1 Grundmodell

- Ett självständigt, gemensamt fotobibliotek; bilder kan finnas där utan att användas någon annanstans.
- Ett original registreras en gång och återanvänds i flera berättelser, observationer, arter och platser.
- Dubbletter identifieras pålitligt, inte bara med filnamn. Ingen sammanslagning av olika bilder på lösa antaganden.
- Originalets tekniska filnamn är inte automatiskt bildens titel. Ett valfritt bildnamn hör till fotot; **bildtext** hör till varje enskild användning.
- Biblioteket har filter **Inte använda**, men ingen särskild inkorg som antyder att bilder måste placeras.
- Standardordning efter faktiskt fotograferingsdatum (EXIF när tillgängligt), nyast först. Andra sorteringar kan väljas. Saknas datum markeras det som okänt.
- Datum kan korrigeras i MNO utan att ursprunglig EXIF eller originalfil ändras; både ursprungligt och korrigerat värde bevaras.
- **Massförskjutning av fotograferingstid** för flera markerade bilder är uttryckligen **mycket lågt prioriterad**, inte del av MVP.

### 9.2 Bilder i berättelser

- Fotografier placeras mellan textstycken via ett synligt infogningsval.
- Enskilda bilder och manuellt skapade grupper behåller naturliga proportioner; stora bilder beskärs inte automatiskt.
- Möjlighet att ändra ordning med upp/ned-kontroller; drag-och-släpp krävs inte initialt.
- Frivillig bildtext per användning; tom bildtext lämnar inget synligt tomrum.

### 9.3 Bildvisare

- Klick öppnar en mörk, minimalistisk ljuslåda och bevarar exakt läsposition vid stängning.
- Föregående/nästa gäller bilderna i aktuellt sammanhang; stoppar vid sista bilden utan att gå runt.
- Diskreta synliga pilar, tangentbord vänster/höger och Escape, svep på pekskärm.
- Klick för zoom runt klickpunkt, klick igen för att passa in; synliga +/−, panorering och stöd för vanliga gester. Zoom återställs vid bildbyte.
- Bildtext visas diskret när den finns och kan döljas.
- En separat, valbar informationspanel visar först viktig EXIF: datum, kamera, objektiv, brännvidd, slutartid, bländare, ISO, GPS om tillgängligt. **Visa alla metadata** expanderar.
- Metadata visas skrivskyddat tills användaren uttryckligen väljer **Redigera bildinformation**; **✓ Klar** avslutar redigering och ändringarna sparas i MNO.

### 9.4 Länkade original och lokal cache

- MNO länkar till original på intern eller extern disk och undviker onödig duplicering av originalfiler.
- Vid tillägg skapas lokala miniatyrer och högkvalitativa förhandsbilder för offlinevisning; tillgänglig EXIF läses. Redan bearbetade bilder bearbetas inte om i onödan.
- **Visa originalfil** öppnar dess plats i systemets filhanterare om den finns.
- När originalet saknas visas fortfarande cachad bild och en diskret statusindikator. Skillnad görs mellan frånkopplad volym och flyttad/saknad fil när det kan avgöras.
- **Hitta originalfil** låter användaren själv välja ny sökväg. MNO kan därefter föreslå relänkning av närliggande filer utifrån relativa sökvägar, men först med förhandsgranskning, identitetskontroll och godkännande.
- Status för otillgängliga original sammanfattas diskret i Fotografier, med hänsyn till när kontroll senast gjordes. Ingen falsk realtidsstatus.
- Länkar kontrolleras vid behov; en full kontroll görs endast genom uttryckligt **Kontrollera originalfiler**. Ingen skanning av alla diskar vid appstart.

### 9.5 Bearbetning och fel

- Större tillägg bearbetas i bakgrunden med diskret framsteg, utan att låsa journalen.
- Kön är säker att avbryta vid appstängning och återupptas vid nästa start. Färdiga bilder görs inte om; partiella resultat behandlas inte som färdiga.
- En trasig bild stoppar inte övriga: t.ex. **97 fotografier klara · 3 behöver åtgärdas**. Fel kan granskas och försökas igen individuellt eller samlat.
- Återupptagning av redan beställd bearbetning är tillåten; oombedd inventering av disk är det inte.

### 9.6 Borttagning

- **Ta bort från berättelse/sida** tar endast bort den aktuella kopplingen.
- **Ta bort ur biblioteket** visar vilka sammanhang som påverkas och kräver bekräftelse. Kopplingar och MNO:s egna data kan tas bort, men aldrig det externa originalet.
- En diskret **Ångra** erbjuds i ungefär tio sekunder och återställer bilden och dess kopplingar. Ingen separat intern papperskorg i första skedet.
- Bilder som inte längre används ligger kvar i biblioteket tills användaren själv tar bort dem. Ingen automatisk städning av oanvända foton.

## 10. MVP v0.1 – den första användbara versionen

**Målet är en utmärkt, textbaserad naturdagbok, inte ett komplett naturdatabassystem.**

**Ingår som mål:**
- Lokal, offline journal med datumstyrda berättelser och kronologisk läsning.
- Skapa, redigera, autospara och återöppna berättelser; frivillig rubrik och enkel rik text.
- Flera berättelser samma dag, justerbara datum och begriplig navigering.
- Tidslinje/kalender och grundläggande sökning.
- Manuellt väder samt uttryckligt valfri väderhämtning, utan automatiska platsuppslag.
- Enkla historiska grafer/översikter när data finns.
- Ljust/mörkt tema enligt systemet, svenska/engelska och tillgängliga textstorlekar.
- Säkerhetskopiering och export i portabla format.

**Inte MVP-krav:**
- Fullt fotobibliotek, ljuslåda, bildimport och EXIF-arbetsflöden.
- Kompletta art-, plats- och observationsregister eller eBird-import.
- Avancerad analys, plugin-ekosystem, massredigering av bilddatum.
- Molnsynk, användarkonton eller AI-baserad artidentifiering.

**Viktig avgränsning:** De detaljerade besluten om fotografier och strukturerade observationer beskriver framtida faser. De får inte smyga in som blockerande krav för v0.1.

## 11. Föreslagen etappindelning (preliminär)

| Etapp | Inriktning | Status |
|---|---|---|
| **v0.1** | Den användbara, textbaserade naturdagboken | Prioriterat mål |
| **Senare etapp** | Strukturerade observationstillfällen, arter och platser | Designad riktning; ordning ej fastställd |
| **Senare etapp** | Gemensamt fotobibliotek och fotografier i journalen | Utförligt UX-designat; ordning ej fastställd |
| **Längre fram** | Fördjupad analys, import/integrationer, avancerad bildhantering | Låg eller ännu ej beslutad prioritet |

Den exakta ordningen mellan framtida etapper bestäms först efter teknisk genomgång och MVP-planering.

## 12. Öppna frågor inför teknisk arkitektur (steg B)

1. Vilken cross-platform-teknik ger snabb, stabil, offline desktop-upplevelse och tillgänglig textredigering?
2. Hur ska lokalt dataformat/databas, migrationer, backup och export utformas för lång livslängd och återställbarhet?
3. Hur modelleras berättelser, datum, senare observationer och kopplingar utan dubbletter?
4. Vilket format lagrar rik text utan att låsa användaren till en viss editor?
5. Hur separeras appens egna data från externa, läsbara originalfiler på OS-nivå?
6. Hur fungerar språk, datumformat, tidszoner och mörkt läge konsekvent mellan plattformar?
7. Hur kan framtida foto-cache, robust jobbkö och återlänkning läggas till utan att belasta v0.1?
8. Vilka automatiska tester krävs för autosparning, datamigrationer, backup/restore och principen att externa filer aldrig skrivs över eller raderas?
9. Vilken fri licens, paketering och distributionsmodell passar projektets mål?

## 13. Arbetsregel framåt

Vid nya förslag ska vi alltid fråga: **Gör detta naturdagboken bättre för användaren nu, eller hör det hemma i en senare fas?**

Beslut som inte uttryckligen är fastställda ska inte framställas som avgjorda. Specifikationen kan revideras med tydlig versionshistorik när tekniska val eller verklig användning ger nya insikter.

---

*Underlag: gemensamma produkt- och designbeslut i MNO-samtalen till och med designbeslut 84. Dokumentet är en strukturerad konsolidering av dessa beslut, inte en ordagrann återgivning av varje enskild fråga.*
