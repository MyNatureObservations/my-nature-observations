# MNO – Designbeslut och öppna designfrågor

> Historical context: earlier issue/PR numbers, commit IDs and CI links in this
> document refer to the private pre-publication archive, not this repository.
> They may be inaccessible to public readers. Technical content and recorded test
> results are retained; no matching public issues or commits are implied.

**Status:** Levande dokument
**Skapat:** 15 oktober 2026

## Syfte

Detta dokument är ett register över viktiga designbeslut,
designriktningar och öppna frågor för My Nature Observations.

Det kompletterar, men ersätter inte:

- [Design Specification v1.0](MNO_Design_Specification_v1.0.md)

Designspecifikationen beskriver helheten. Detta register
gör det lättare att hitta och följa upp enskilda beslut.

## Statusbegrepp

- **Fastställt:** Ett designbeslut som vi har enats om.
- **Designriktning:** En godkänd idé vars detaljer inte är beslutade.
- **Öppen fråga:** Något vi behöver undersöka eller besluta.
- **Reviderat:** Ett tidigare beslut som har ändrats.

Implementeringsstatus följs separat i GitHub Issues.

---

## D-001 – Japandi-inspirerad design

**Status:** Fastställt

MNO ska ha ett lugnt, luftigt och varmt gränssnitt.

Japandi är en designfilosofi, inte ett bokstavligt tema.
Vi ska inte efterlikna japanska trädgårdar, träpaneler,
rispapper eller andra dekorativa miljöer.

**Referens:** Design Specification v1.0, avsnitt 3.

## D-002 – Gränssnittet ska vara diskret men begripligt

**Status:** Fastställt

"The interface whispers. Your story speaks."

Journalens innehåll ska stå i centrum. Funktioner ska
vara lätta att hitta utan att dominera upplevelsen.

**Referens:** Design Specification v1.0, avsnitt 2–3.

## D-003 – Panelbaserad navigation

**Status:** Fastställt på principnivå

MNO ska kunna använda en vänsterpanel för huvudnavigering
och en högerpanel för kontextberoende funktioner.

Paneler ska kunna döljas och visas utan att användaren
tappar sitt sammanhang.

**Referens:** Design Specification v1.0, avsnitt 4.

## D-004 – Panelernas rörelser

**Status:** Designriktning

Vi har diskuterat paneler som öppnas från vänster,
höger och ovanifrån.

Exakta rörelser, animationer, användningsområden och
beteenden på olika skärmstorlekar behöver dokumenteras
och beslutas närmare.

**Öppen fråga:** Vilka funktioner hör hemma i respektive panel?

#### Vänsterpanelens beteende

- Vänsterpanelen är en utfällbar navigeringspanel som glider in
  horisontellt från vänster.
- Panelen visas som ett överlägg ovanpå journalens huvudyta.
  Journalens innehåll ska inte flyttas eller ändra storlek.
- Panelen använder endast den bredd som behövs och ska inte
  täcka hela gränssnittet.
- När panelen är stängd ska journalen få fullt visuellt fokus.
- Panelen stängs genom:
  - Klick utanför panelen.
  - Escape.
  - En tydlig stängningsknapp.
  - Navigering till ett datum eller en anteckning.
- Panelens öppning och stängning får inte påverka pågående
  redigering eller osparat innehåll.
- Beteendet ska vara konsekvent med andra tillfälliga
  gränssnittselement, exempelvis datumväljaren.

#### Vänsterpanelens innehåll

- Överst visas en kompakt månadskalender.
- Under kalendern visas en kronologisk journalhistorik.
- Kalendern är synlig direkt när vänsterpanelen öppnas.
- Journalhistoriken ska kunna rullas utan att kalendern
  försvinner ur sikte.
- En större årsöversikt ska kunna öppnas från kalendern.

### Vänsterpanelens innehåll

Vänsterpanelen ska vara en kompakt och gemensam ingång
till MNO:s innehåll och aktiva moduler.

Panelen innehåller:

- Global sökning överst.
- En gemensam kalender som normalt är synlig.
- Ingång till journalens översikt i huvudytan.
- Ingångar till aktiverade moduler.

Den fullständiga journalhistoriken hör hemma i journalens
arbetsyta, inte i vänsterpanelen.

Panelen ska anpassa sitt innehåll efter tillgängligt utrymme.
Målet är att undvika intern scrollning vid normala
fönsterstorlekar, utan att försämra tillgängligheten vid
små fönster eller förstorad text.

### Kompletterande detaljvy

Vänsterpanelen ska kunna öppna en kompletterande detaljvy
åt höger, ovanpå huvudytan.

Detaljvyn är en del av vänsterpanelen och räknas inte
som en separat huvudpanel.

När vänsterpanelen stängs ska även detaljvyn stängas.

Att öppna detaljvyn får inte påverka pågående arbete
eller osparat innehåll i huvudytan.

## D-005 – Bentolådeprincipen

**Status:** Designriktning

MNO kan organisera information i tydliga, harmoniskt
sammanhängande ytor, inspirerat av en bentolåda.

Principen handlar om informationsarkitektur, balans
och proportioner – inte om dekorativa rutor överallt.

Idén introducerades i oktober 2026.

**Referensbild:** [Journal Concept](references/MNO_Journal_Concept.png)

## D-006 – Kalendern som orientering

**Status:** Fastställt

Kalendern ska hjälpa användaren att hitta tillbaka
till naturupplevelser, inte fungera som ett
traditionellt planeringsverktyg.

Tomma dagar är naturliga och ska aldrig signalera
att användaren har missat något.

En större kalenderöversikt ska kunna visa ett helt år.

### Tidsnavigation

Kalendern ska erbjuda tre nivåer för att navigera i tiden:

- År: Översikt över årets månader.
- Månad: Kalender med enskilda dagar.
- Dag: Innehåll och händelser kopplade till ett valt datum.

Användaren ska kunna förflytta sig direkt mellan år
och månader utan att behöva bläddra stegvis genom
alla mellanliggande perioder.

När ett datum väljs ska en kompletterande detaljvy
kunna öppnas åt höger om vänsterpanelen.

Detaljvyn ska kunna visa innehåll från flera moduler,
exempelvis journalanteckningar och observationer.

Att välja datum eller period får inte automatiskt
ersätta innehållet i huvudytan.

### Framtida tidslinjer

MNO ska på sikt kunna erbjuda flera sätt att
utforska innehåll över tid:

- En kompakt tidslinje för snabb navigering.
- En större tidslinje i huvudytan för att utforska
  och återuppleva naturupplevelser.
- Möjlighet att kombinera tidsbaserad information
  från flera moduler.

Kalender och tidslinjer ska bygga på gemensamma
principer för datum, perioder och händelser.

Journalanteckningens upplevelsedatum ska fortsatt
kunna skilja sig från dess skapandedatum.

Dessa möjligheter ska beaktas i arkitekturen,
men behöver inte färdigställas i den första
versionen av kalendern.

**Relaterat GitHub-ärende:** #4.

#### Kalenderns roll i MNO

- Kalendern är en gemensam tidsbaserad navigeringsfunktion
  för hela MNO, inte enbart för journalmodulen.
- I MVP-versionen visar kalendern journalanteckningar.
- Arkitekturen ska tillåta att framtida moduler, exempelvis
  fågelobservationer och väderobservationer, ansluts till
  samma kalender utan att kalendern behöver byggas om.
- Kalendern ska hjälpa användaren att återupptäcka
  naturupplevelser, inte uppmuntra till daglig aktivitet.
- Tomma datum är naturliga och ska inte framställas som
  något som behöver åtgärdas.

#### Navigering och datumval

- Månadskalendern visas överst i vänsterpanelen.
- En större årsöversikt ska finnas för navigering över
  längre tidsperioder.
- Datum med innehåll ska kunna väljas för att öppna
  tillhörande anteckningar.
- Om flera anteckningar finns samma datum ska användaren
  kunna välja mellan dem.
- När ett tomt datum väljs visas ett diskret alternativ
  för att skapa en ny journalanteckning.
- Den nya anteckningen får det valda datumet förvalt.
- Även datum med befintliga anteckningar ska erbjuda
  möjligheten att skapa ytterligare en anteckning.
- Möjligheten att skapa en anteckning visas först när
  användaren väljer ett datum.
- Kalendern ska inte visa permanenta plustecken eller
  andra uppmaningar på tomma datum.
- Datumet representerar när naturupplevelsen inträffade,
  inte nödvändigtvis när anteckningen skapades.

## D-007 – Visionsskissens roll

**Status:** Fastställt

Visionsskissen visar en möjlig informationsstruktur
med kalender, navigering och journalinnehåll.

Den är inte en färdig designspecifikation.

Vi ska inte efterlikna dess naturbakgrund,
pappersstruktur eller dekorativa detaljer.

**Referensbild:** [Journal Concept](references/MNO_Journal_Concept.png)

---

## Framtida beslut

Nya designbeslut får nästa lediga D-nummer.

Vid ändringar dokumenterar vi varför beslutet
reviderades, i stället för att bara skriva över
dess tidigare innebörd.

Konkreta utvecklingsuppgifter hanteras i GitHub Issues.

---

## D-008 – Kalenderns visuella markeringar och framtida moduler

**Status:** Fastställt (grundprinciper), med öppna frågor
för framtida vidareutveckling.

**Beslut:** Kalendern ska förmedla förekomsten av innehåll
utan att bli visuellt överlastad.

### Grundutseende

- En liten, diskret punkt under datumet visar att innehåll
  finns registrerat den dagen.
- Det valda datumet markeras med en svagt tonad bakgrund.
- Punkten visar att det finns innehåll, medan bakgrunden
  visar vilket datum användaren har valt.
- Tomma datum visas neutralt, utan särskilda markeringar.
- Kalendern ska följa MNO:s Japandi-inspirerade designsystem
  och principen "Quiet, not hidden".
- Markeringarna ska fungera i både ljust och mörkt tema.
- Samma grundprincip ska användas i månadskalendern och
  den större årsöversikten.

### Information på begäran

- Kalenderns grundvy ska inte visa separata symboler för
  varje innehållstyp.
- När användaren väljer ett datum kan kalendern visa vilka
  typer av innehåll som finns registrerade.
- Informationen kan exempelvis omfatta journalanteckningar,
  fågelobservationer och väderobservationer.
- Detaljer ska presenteras först när användaren efterfrågar
  dem, så att kalenderns grundutseende förblir lugnt.

### Framtida utbyggnad

- Kalendern ska kunna representera innehåll från flera
  olika MNO-moduler.
- Möjlighet att filtrera efter innehållstyp ska finnas
  med som framtida designriktning.
- Eventuella färgvariationer eller ytterligare markeringar
  för olika moduler är ännu inte beslutade.
- Kalenderns utformning ska inte förutsätta att endast
  journalanteckningar existerar.

### Öppna frågor

- Hur ska information om flera innehållstyper presenteras
  när användaren väljer ett datum?
- Hur ska framtida modulfilter utformas?
- Behöver årsöversikten en annan detaljnivå än
  månadskalendern?
- Hur säkerställer vi att markeringarna är tillgängliga
  även utan färgseende?

### Koppling till utvecklingen

- GitHub #4 – Kalenderöversikt
- GitHub #14 – Designsystem och övergripande användarupplevelse

**Motivering:** Kalendern ska kunna växa från en enkel
journalöversikt till en gemensam ingång till MNO:s samlade
naturupplevelser och observationer, utan att förlora sitt
lugna och återhållsamma uttryck.

---

## D-009 – Gemensam navigering och tre paneler

**Status:** Fastställt

MNO ska ha en gemensam navigeringsstruktur som är tillgänglig
från hela applikationen.

### Panelernas roller

MNO:s navigering bygger på fyra tydliga ansvarsområden:

- **Vänster – Hitta och öppna:** Global sökning, kalender,
  journalhistorik och navigering till aktiva moduler.
- **Mitten – Uppleva och arbeta:** Journal, observationer,
  kartor, analyser och modulernas arbetsytor.
- **Höger – Anpassa och hantera:** Globala inställningar,
  modulinställningar, datakällor, integritet, verktyg
  och hantering av valfria moduler.
- **Överst – Se sammanhanget:** Kartor, större översikter
  och information som ger ett bredare perspektiv.

Vänsterpanelen är den huvudsakliga ingången till MNO:s
innehåll och aktiva moduler.

Högerpanelen används för att anpassa hur MNO och dess
moduler fungerar, inte för att navigera mellan dem.

Den övre panelens beteende är inspirerat av klassrummens
neddragbara rullgardinskartor. Liknelsen beskriver funktionen,
inte panelens visuella utseende.

### Globala och modulspecifika inställningar

Högerpanelen ska ha en konsekvent grundstruktur.

- Globala inställningar ska alltid vara tillgängliga.
- Modulspecifika inställningar ska visas när de är
  relevanta för den aktiva modulen.
- Användaren ska kunna aktivera, inaktivera och konfigurera
  valfria moduler via högerpanelen.
- Aktiverade moduler ska bli tillgängliga via vänsterpanelen.
- Inställningar ska inte blandas ihop med funktioner
  som hör hemma i modulernas arbetsytor.

MNO ska kunna användas som en enkel naturjournal eller
byggas ut med ytterligare funktionalitet efter användarens
behov.

### Gemensamma beteenden

- Endast en panel får vara öppen åt gången.
- När en annan panel öppnas stängs den föregående.
- Ingen panel får täcka hela applikationen.
- Panelerna lägger sig ovanpå arbetsytan utan att flytta
  eller ändra storlek på den.
- Panelerna ska kunna stängas med Escape, klick utanför
  eller en tydlig stängningsknapp.
- Rörelserna ska vara mjuka, diskreta och konsekventa.
- Panelerna ska anpassas till fönsterstorleken.
- Att öppna eller stänga en panel får inte påverka
  pågående redigering eller osparat innehåll.

### Arbetsytans roll

Arbetsytan är MNO:s centrum.

Panelerna används för att hitta, välja och öppna funktioner.
Själva arbetet sker i huvudytan.

En modul som öppnas via vänsterpanelen ska använda
huvudytan som arbetsområde, inte begränsas till panelen.

### Koppling till utvecklingen

- GitHub #14 – Designsystem och övergripande användarupplevelse

**Motivering:** MNO ska kunna växa med fler funktioner utan
att arbetsytan blir rörig eller att användaren tappar
sammanhanget i sitt pågående arbete.

---

## D-010 – Startsidan som beständig utgångspunkt

**Status:** Fastställt

MNO ska öppnas med en välkomnande startsida som har en
konsekvent och igenkännbar grundstruktur.

### Två tydliga vägar

Startsidan ska alltid erbjuda:

- **Ny journalanteckning:** Skapa en ny sida i journalen.
- **Fortsätt:** Återvänd till en befintlig journalanteckning.

Båda alternativen ska vara tydliga och lättillgängliga.

### Startsidan över tid

När användaren ännu inte har några anteckningar ska
startsidan erbjuda en naturlig introduktion till att börja skriva.

När innehåll tillkommer kan startsidan kompletteras med
diskreta personliga tillbakablickar och översikter.

Startsidan ska behålla sin grundläggande struktur även
när MNO får fler moduler och funktioner.

### Ett lugnt första intryck

Startsidan ska inte utvecklas till en överlastad instrumentpanel.

Den ska inte innehålla uppmaningar som skapar prestationskrav
eller dåligt samvete över utebliven dokumentation.

Den gemensamma navigeringen ska göra övriga funktioner
tillgängliga utan att startsidan behöver visa allt.

### Koppling till utvecklingen

- GitHub #14 – Designsystem och övergripande användarupplevelse

**Motivering:** En beständig startsida ger igenkänning
och gör MNO lätt att förstå, samtidigt som användaren
får frihet att börja något nytt eller återvända till
tidigare naturupplevelser.

---

## D-011 – Ikoner och utfällbara funktioner

**Status:** Fastställt

MNO ska använda ett konsekvent och återhållsamt visuellt
språk för ikoner, inställningar och kompletterande funktioner.

### Ikonernas utformning

- Ikoner ska vara monokroma och ha ett enhetligt linjeuttryck.
- Ikoner ska harmoniera med MNO:s färgpalett, i första hand
  den dämpade gröna accentfärgen.
- Symboler ska användas för igenkänning, inte som dekoration.
- Symboler ska normalt kombineras med korta, tydliga texter.
- Etablerade symboler kan användas utan synlig text när
  betydelsen är tillräckligt tydlig.

### Utfällbara funktioner

Inställningar och alternativ som inte behöver vara synliga
hela tiden ska kunna placeras i utfällbara sektioner.

- Sektionerna ska ha en tydlig symbol och en kort etikett.
- En diskret pil visar om sektionen kan öppnas eller stängas.
- Alternativen visas först när användaren öppnar sektionen.
- Det aktiva alternativet ska markeras tydligt men återhållsamt.
- Starka systemfärger ska undvikas när de bryter mot
  applikationens visuella uttryck.
- Avstånd och proportioner ska ge ett lugnt och sammanhållet
  intryck utan att försämra användbarheten.

### Tillgänglighet

- Funktioner ska kunna användas med tangentbord.
- Interaktiva element ska ha synliga fokusmarkeringar.
- Ikonknappar ska ha begripliga namn för skärmläsare.
- Valda alternativ och utfällda sektioner ska kunna
  identifieras även utan visuell information.
- Klickytor ska vara tillräckligt stora för enkel användning.
- Animationer ska vara diskreta och respektera användarens
  inställning för minskad rörelse.

### Första tillämpning

Språkvalet i MNO:s högra navigeringspanel är den första
implementeringen av principen.

En monokrom jordglob identifierar funktionen. Språkvalen
visas i en utfällbar sektion där det aktiva språket markeras
med en diskret grön färg och en bock.

Samma princip ska vägleda framtida funktioner i panelerna.

### Koppling till utvecklingen

- GitHub #14 – Designsystem och övergripande användarupplevelse

**Motivering:** Ett konsekvent ikon- och interaktionsspråk
gör MNO lättare att förstå och använda. Genom att visa
alternativ först när de behövs kan gränssnittet förbli
stillsamt även när applikationen får fler funktioner.

---

## D-012 – Teman och personlig färgsättning

**Status:** Fastställt

MNO ska ha en sammanhållen visuell identitet samtidigt
som användaren ska kunna anpassa applikationens utseende
efter sina egna önskemål.

Personlig anpassning ska först och främst handla om färger.
På längre sikt ska även andra visuella egenskaper kunna
anpassas.

### Grundläggande principer

- MNO Original ska vara standardtemat och alltid finnas
  tillgängligt.
- Användaren ska kunna återställa det ursprungliga utseendet.
- Teman ska påverka applikationens gränssnitt, inte
  användarens journalanteckningar eller övriga innehåll.
- Alla teman ska bevara MNO:s lugna och återhållsamma
  grundläggande formspråk.
- Personlig anpassning ska vara frivillig och inte
  krävas för att använda applikationen.

### Steg A – Färger

Den första versionen av temafunktionen ska erbjuda:

- MNO Original med den befintliga färgpaletten.
- Ett urval färdiga och harmoniska färgteman.
- Möjlighet att skapa och spara egna färgpaletter.

Användaren ska kunna välja en accentfärg och få hjälp
att skapa en sammanhängande färgpalett.

Typografi, former och layout ska förbli oförändrade
mellan färgteman.

### Steg B – Utökad personlig anpassning

På längre sikt ska MNO kunna erbjuda ytterligare
möjligheter att anpassa gränssnittet.

Det kan exempelvis omfatta:

- Val mellan olika typsnitt.
- Anpassning av vissa former och visuella detaljer.
- Andra begränsade utseendeinställningar.

Den exakta omfattningen beslutas senare.

Arkitekturen ska inte förhindra sådan vidareutveckling.

### Designsystem och teknisk förberedelse

MNO ska gradvis införa semantiska designvariabler
för färger och andra gemensamma visuella egenskaper.

Färger ska i möjligaste mån definieras utifrån
sin funktion i gränssnittet, exempelvis:

- Bakgrund och ytor.
- Primär och sekundär text.
- Accentfärg och kontrasterande text.
- Ramar och avgränsningar.
- Fokusmarkeringar och aktiva tillstånd.

Nya komponenter bör använda gemensamma designvariabler
när det är praktiskt.

Befintliga komponenter kan anpassas successivt.

Samma grundläggande arkitektur ska kunna stödja
framtida mörkt läge.

### Tillgänglighet

- Text och interaktiva element ska ha tillräcklig kontrast.
- Fokusmarkeringar ska förbli tydliga i alla teman.
- Viktig information får inte förmedlas enbart genom färg.
- Egna färgval ska kontrolleras så att läsbarhet och
  användbarhet inte försämras.

### Lokal lagring och dataportabilitet

- Temaval och personliga färgpaletter ska lagras lokalt.
- Egna teman ska kunna exporteras och importeras i ett
  öppet och dokumenterat format.
- Temainställningar ska hållas åtskilda från användarens
  journalanteckningar och övriga naturdata.
- Användaren ska inte vara beroende av MNO för att
  kunna återanvända sina egna färgpaletter.

### Avgränsning

Temafunktionen ska inte implementeras i detta skede.

Beslutet ska i stället vägleda den fortsatta utvecklingen
av MNO:s designsystem och komponenter.

### Koppling till utvecklingen

- GitHub #14 – Designsystem och övergripande användarupplevelse

**Motivering:** MNO är en personlig plats för att dokumentera
och återuppleva naturupplevelser. Möjligheten att anpassa
färgerna stärker känslan av ett eget utrymme, samtidigt
som ett gemensamt designsystem säkerställer att MNO
behåller sin visuella identitet och användbarhet.

## D-013 – Väderinformation i journalen

**Status:** Fastställt och genomfört inom R1 – Places & Weather. Ursprunglig inriktning och tidigare öppna frågor bevaras nedan som historik.

MNO ska kunna komplettera journalanteckningar med väderinformation. Funktionen är frivillig och ska ge sammanhang åt naturupplevelsen utan att ta fokus från själva berättelsen.

### Ursprungliga fastställda principer (historik)

- Väderinformation är valfri för varje journalanteckning.
- Användaren ska kunna registrera väder manuellt och hämta väderinformation från externa källor. Dessa uppgifter ska hållas åtskilda och båda ska kunna sparas.
- En daglig vädersammanfattning är utgångspunkten. Om användaren anger en specifik tidpunkt ska även timvisa väderuppgifter kunna visas.
- Open-Meteo är den första externa väderleverantören i version 0.1. Andra leverantörer, exempelvis SMHI, kan tillkomma senare.
- Platsangivelse är frivillig. Karta och tidigare sparade lokaler är möjliga lösningar för platsvalet.
- Personliga väderstationer är en möjlig framtida integration och ingår inte i version 0.1.
- Väderfunktionerna ska vara diskreta och kunna fällas ut vid behov. Journaltexten ska förbli det centrala innehållet.

### Tidigare öppna frågor (historik)

Följande frågor var öppna före R1. Platsval och väderpanel är nu genomförda enligt besluten nedan; val av historiskt klockslag har skjutits fram.

- Hur väderkontrollerna ska placeras och fungera i journalens gränssnitt.
- Hur dagliga sammanfattningar och timvisa uppgifter ska presenteras.
- Hur användaren ska välja eller ange en plats.

### Genomförda beslut i R1

- Ett gemensamt, beständigt platsregister används för journalens observationsplats och för val av väderplats. Namn är obligatoriskt, koordinater frivilliga. Platser skapas, redigeras och raderas i den gemensamma Leaflet-baserade skolkartan med lokal sökning. Refererade registerplatser skyddas mot radering.
- Journalens platskoppling bevarar en historisk snapshot. Registerändringar ändrar inte tidigare journaluppgifter. Ett uttryckligt ”Använd plats” kan uppdatera den öppna anteckningens snapshot till aktuell registerversion, även för samma plats-ID.
- Standardplats för väder väljs under Inställningar → Språk och lokalisering. Den styr kartans utgångspunkt och är ett väderförval; den skapar ingen observationsplats och startar ingen hämtning.
- Väderplats och observationsplats är separata. Journalens plats föreslås när den har koordinater; saknas journalplats föreslås standardplatsen. Om journalplats finns utan koordinater krävs ett uttryckligt annat väderplatsval.
- Den kompakta vädervisningen öppnar en panel för egna observationer och hämtat väder. Manuella uppgifter är frivilliga, kan ändras och rensas separat och har företräde i sammanfattningen. Väder lagras utanför TipTap-dokumentet.
- Open-Meteo anropas endast på användarens begäran. Dagens väder är en aktuell modellbild från ECMWF IFS, de senaste avslutade dygnen använder modellens dagsvärden och äldre dygn använder ERA5. Framtida datum avvisas. Dagens modellbild ska inte tolkas som ett färdigt dygn eller en faktisk mätning.
- Förhandsgranskning skriver inte till databasen. Sparning och ersättning bekräftas uttryckligen och kontrollerar revision, databasgeneration och relevant journal-/platsunderlag. Manuellt väder bevaras. Källa, modell, licens och tidsuppgifter följer den sparade väderbilden.
- API:ets tidszon och dygnsindelning används enligt rimlighetsprincipen. Ingen egen rekonstruktion av historiska tidszoner eller timbaserad dygnsmotor ingår.

Aktuella kontraktsreferenser och förbättringar utanför R1 finns i [R1 – Places & Weather](../development/R1_Places_Weather.md). Historiskt klockslagsval är en senare möjlighet, inte ett kvarvarande acceptanskrav för R1. Andra väderleverantörer och personliga väderstationer är fortsatt framtida funktioner.

### Motivering

Vädret kan vara en viktig del av minnet av en naturupplevelse. Möjligheten att både beskriva det själv och komplettera med historiska väderuppgifter ger journalen ett rikare sammanhang.

Funktionen ska vara ett stöd för berättelsen, inte ett krav för att skriva en anteckning.

---

## D-014 – Sammanhängande journal

**Status:** Fastställt

MNO ska främst erbjuda ett sammanhängande och scrollbart läsläge för journalen.

- Journalposter visas med nyaste upplevelsedatum överst och äldre datum längre ned.
- Anteckningar från samma upplevelsedatum grupperas med stabil inbördes ordning.
- Knappen "Fortsätt" på startsidan öppnar början av den senaste synliga datumgruppen.
- Kalendern kan öppna specifika datum och enskilda anteckningar i journalflödet.
- Läsningen sker genom sammanhängande vertikal scrollning. Föregående/nästa-knappar ingår inte i den nuvarande etappen.
- Varje journalpost redigeras separat, med återgång till samma anteckning i journalflödet.
- En bokliknande sidvisning är en framtida möjlighet och ska inte inkluderas i den nuvarande etappen.
- Den nuvarande startsidesdesignen ska bibehållas.

---

## D-015 – Upptäck på startsidan

**Status:** Designriktning

En diskret yta på startsidan kan visa intressant innehåll från journalen och framtida moduler.

- Den första implementationen kan utgå från journalminnen. På sikt ska även andra moduler kunna bidra med innehåll.
- Endast ett innehåll visas åt gången, utan automatisk växling medan användaren tittar på ytan.
- Funktionen ska inte skapa någon form av prestationspress, aktivitetsmål eller uppmaningar att skriva oftare.
- Användaren ska kunna välja att stänga av funktionen.
- Privat och känsligt innehåll samt användarens synlighetsinställningar ska respekteras.
- "Upptäck" är ett arbetsnamn och behöver inte visas i det slutgiltiga gränssnittet.
- Den exakta urvalsalgoritmen och den tekniska implementationen beslutas senare.


---

## D-016 – Versioned database schema and module evolution

**Status:** Fastställt

MNO ska kunna utöka databasen med nya moduler utan att
äldre data, backuper eller återställningsmarkörer förlorar
sin betydelse eller befintliga säkerhetsspärrar försvagas.

### Självständiga versionsansvar

- Databasschema, backupformat, återställningsmarkörer P och Q,
  innehållsbevis och modulkontrakt har självständiga
  versionsbegrepp.
- Varje känt databasschema ska valideras strikt enligt
  versionsspecifika struktur- och innehållsregler.
- Att en version är känd, att dess innehåll är giltigt och
  att en viss operation är tillåten är skilda frågor.
- Tillåtna versionskombinationer bestäms per operation,
  inte genom en global lista över stödda versioner.
- Äldre databas- och backupformat får identifieras utan
  att detta ger tillstånd att migrera eller återställa dem.

### Migrering och innehållsbevis

- Migreringar ska vara explicita, linjära och transaktionella.
  De aktiveras separat efter säkerhetsgranskning.
- Nya beständiga användardata ska omfattas av motsvarande
  innehållsvalidering och versionsspecifika innehållsbevis.
- Äldre P/Q-format och innehållsfingeravtryck ska bevara
  sin ursprungliga betydelse och sitt byteformat.
- Okända eller avvikande versioner får inte leda till
  automatisk reparation, databortfall eller kringgående
  av Recovery-spärrar.
- Lösningen ska vara liten och statisk. Ett generellt
  migreringsramverk ska inte införas innan ett konkret
  behov uppstår.

### Implementerat i steg 1B

- Versionsansvaren har separerats med oförändrat accepterat
  produktionsbeteende.
- Aktuell databasschemaversion är fortfarande 0.
- Befintlig schema-0-validering är oförändrad.
- Backupens, P:s och Q:s versionsregler är självständigt
  definierade.
- Innehållsfingeravtryck version 1 är oförändrat.
- Inga nya migrerings-, restore- eller Recovery-vägar har
  införts.

**Referenscommit:** `dad474c78e0392e27156ab8b17fb21b495e1cd60`.

### Ännu inte implementerat

- Databasschema 1 och vädertabellen.
- Versionsspecifik schema-1- och väderinnehållsvalidering.
- Nytt innehållsbevis som omfattar väderdata.
- Databasmigrering 0 → 1.
- Aktivering av migrering vid normal uppstart.
- Nya tillåtna restoreövergångar.

Dessa funktioner kräver separata säkerhets- och
kompatibilitetsgranskningar innan de implementeras
eller aktiveras. Beslutet ger inte i sig tillstånd
att genomföra dessa förändringar.

### Säkerhetsgränser

D-016 ändrar inte befintliga regler för Recovery,
CompletionProof, Ready, descriptorägarskap eller POSIX-lås.

Det återstående säkerhetsarbetet kring arkivanskaffning
är inte löst genom steg 1B. Versionsseparationen innebär
inte att produktionsrestore är godkänd eller aktiverad.

### Koppling till utvecklingen

- GitHub #9 – Backup/Restore
- Fortsatt databasutbyggnad för väder och framtida moduler.

**Motivering:** Självständiga versionsansvar gör det möjligt
att utöka MNO stegvis utan att tyst ändra äldre formats
betydelse. Strikt validering och separat tillåtelse för
varje operation skyddar användarens data samtidigt som
lösningen förblir liten och överblickbar.

## D-017 – Temasystem efter v0.1

**Datum:** 2026-10-31
**Status:** Fastställt för releasescope; designriktning för framtida teman

Nuvarande ljusa utseende accepteras för personlig användning och offentlig v0.1.
A3 behåller sitt historiska inventerings-ID men är inte längre en releaseblockerare.
Detta ersätter det tidigare v0.1-kravet på systemstyrt ljust/mörkt tema i
arkitektur-/designspecifikationerna och MVP-checklistan. Det innebär inte att
mörkt läge har implementerats eller att tillgänglighetskraven har slopats.

Efter v0.1 omfattar [GitHub #8](https://github.com/antstan76/my-nature-observations/issues/8):

- Automatiskt ljus/mörkt utseende efter operativsystemet.
- Manuellt utseendeval.
- Naturinspirerade teman, möjligen Forest, Coast, Mountain och Night.
- Gemensamma semantiska designtokens som grund för konsekvent utseende.
- Konsekvent tillgänglighet, kontrast, läsbarhet och fokusmarkeringar.

Temanamnen är arbetsförslag. Paletter, antal teman, väljarnas placering och samspelet
mellan systemval, manuellt val och naturteman är preliminära UX-frågor, inte nya
fastställda implementationkrav. Lös dem proportionerligt när arbetet prioriteras.
B4:s granskning av det levererade ljusa utseendet kvarstår inför offentlig release.
Ingen kod eller nytt temaramverk införs genom detta beslut.

## D-018 – MIT som projektlicens

**Datum:** 2026-10-31
**Status:** Fastställt av projektägaren

My Nature Observations antar MIT som projektlicens inför offentlig v0.1, efter
beroendelicensgranskningen. Valet prioriterar kompatibilitet, enkel förvaltning och
framtida modulär utveckling. Det ersätter det tidigare öppna licensvalet; historiska
uppgifter om att licensen inte var vald beskriver läget före detta beslut.

- Rotens `LICENSE` innehåller standardtexten för MIT med
  `Copyright (c) 2026 Stefan Antonsson`. Namnet stöds av repositoryts ursprungliga
  författarhistorik; projektets första commits är från 2026.
- README, npm-metadata och Rust-paketets licensdeklaration är konsekventa med MIT.
- A4 markeras som slutfört för projektlicensen, utan omnumrering av inventeringen.
- Tredjepartsberoenden och tillgångar behåller sina egna licenser. Beslutet ger inte
  rätt att omlicensiera dem eller anta att alla tillgångars ursprung är verifierat.
- Paketerade tredjepartslicenser, copyright/NOTICE, eventuell källkodstillgång och
  övrig licensefterlevnad kvarstår före offentlig v0.1 under B6. Valbara licenser
  ska skiljas från samtidiga krav; en MIT-projektlicens ersätter inte dessa villkor.
- En framtida tredjepartslicensvy i Inställningar → Om MNO implementeras inte här.
  Den kan använda automatiskt genererat underlag från låsta beroenden, med en liten
  granskad lista över undantag, i stället för hundratals manuella poster.

Uppföljning sker i befintliga MVP-ärendet #2. Inget nytt licensärende eller någon
ändring av UX-trackern #14 behövs. Appbeteende, beroendeversioner och dataformat
ändras inte genom detta beslut.

## D-019 – macOS först i v0.1-serien

**Datum:** 2026-11-01
**Status:** Fastställt av projektägaren

Första offentliga **MNO v0.1.0 publiceras för macOS**. Windows och Linux följer
senare inom v0.1-serien, efter plattformsspecifik implementation och verifiering.
Detta ersätter kravet att alla tre plattformar måste vara klara före första
publicering. Det ursprungliga treplattformsmålet behålls som historik och som
mål för serien; det är inte ett samtidigt leveranskrav.

- A2 behåller sitt inventerings-ID men blockerar inte första macOS-publiceringen.
  [#11](https://github.com/antstan76/my-nature-observations/issues/11) hålls öppet för
  senare plattformsarbete, inklusive befintliga lås-/durability-/Recovery-spärrar.
  Fail-closed-skydd ska inte försvagas för att producera fler paket.
- Före varje ytterligare plattform publiceras verifieras dess native-beroenden,
  licenser/notiser och eventuell källkodstillgång, ikoner, paket, installation,
  lagring och centrala journal-/portabilitetsflöden. macOS-resultat överförs inte
  automatiskt till Windows/Linux. VM-testning är tillåten där den är representativ.
- B4/[#12](https://github.com/antstan76/my-nature-observations/issues/12) gäller före
  macOS-publiceringen. D-017 gäller fortsatt: ljust utseende accepteras, men dess
  tillgänglighet och kontrast ska verifieras. Inget nytt temasystem krävs.
- Godkända macOS-resultat från RC B1/B2, A1, B5 och PR #32–#35 bevaras.
  PR #35:s CI och manuella DEV-acceptans är godkända enligt projektägaren.
  Ingen omtestning eller ny funktion krävs utan konkret ny avvikelse, ändrad kod
  eller kvarvarande uttryckligt releasekrav. En avgränsad tillgänglighetsbrist är
  skäl för just den rättningen, inte för att börja om hela macOS-arbetet.
- Första macOS-artefakten anger den faktiskt verifierade OS-/CPU-baslinjen och
  vald distributionsväg under B6. Beslutet kräver inte automatiskt Intel/Universal,
  ny installerare, automatisk updater eller nya distributionsfunktioner.

### Källversion och versionsnummer

Senare plattformsartefakter får dela versionsnumret **0.1.0 endast om de byggs
från samma oförändrade release-källversion**, identifierad med tagg/commit och
låsta beroenden. Plattformarnas binärer/signaturer/checksummor blir naturligt olika;
varje artefakt ska kunna spåras till sin källversion och bygginstruktion.

Om kod, beroenden eller leveransrelevant plattformskonfiguration behöver ändras
används en lämplig ny patchversion, exempelvis **0.1.1**, med egen källreferens och
releasenotering. Återanvänd inte 0.1.0 för annan kod och flytta inte en publicerad
tagg. Plattformarna behöver inte publiceras samtidigt; detta beslut ändrar inga
versionsnummer, databaser, dokumentformat eller arkiv nu.

**Motivering:** publicera den verifierade macOS-produkten först och behåll tydliga
publiceringsgrindar för varje senare plattform, utan onödig utvidgning av första
releasen. Implementering/verifiering följs separat i #11, #12 och B6.
