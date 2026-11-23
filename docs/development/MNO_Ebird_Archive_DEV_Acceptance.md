# Bekräftad eBird-import — manuell DEV-arkivkontroll

**Status: användaren har godkänt macOS DEV-arkivkontrollen.**

Backup → Restore och Export → Open Import utfördes till separata mål medan
fågelmodulen var avstängd, följt av aktivering och ytterligare omstart.
Detta är användarrapporterad manuell DEV-acceptans, inte paketerad acceptans eller
ett påstående om manuell kontroll av interna rad-ID:n. Interna inaktiva rader,
identiteter och fullständiga arkivbevis täcks separat av automatiska tester.
Inga ytterligare visuella godkännanden härleds från denna kontroll.

Proceduren nedan behålls som genomförandeunderlag; profiler och arkiv ska bevaras.
[Implementationsrapport och tidigare DEV-acceptans](MNO_Ebird_Confirmed_Import.md).
Ingen ny implementation eller testkörning krävs för att förbereda denna kontroll.

## Profiler och gemensam modulinställning

Källa: `/Users/stefanantonsson/mno-birds-AFP613/journal`.
Förberedda mål/arkiv finns under `/Users/stefanantonsson/mno-ebird-archives-czbp7h1s`.
Endast föräldramapparna är skapade; båda målens `journal`-barn saknas före första
start. De ska skapas av appen. Ersätt aldrig källprofilen eller produktionsjournalen.
Behåll alla mappar och arkiv efter testet.

DEV-modulvalet ligger utanför journalen, under kontots separata DEV-inställning.
Alla dessa DEV-profiler delar samma val; produktionsinställningen är separat.
Restore och Open Import ändrar inte valet. Kör en DEV-instans i taget, avsluta
normalt och invänta terminalprompten mellan profilerna. Om bara DEV-startaren
återstår efter att appen stängts, avsluta den med Ctrl+C. Avbryt aldrig en arkivskrivning.

## 1. Källa: notera innehåll och skapa båda arkiven

Avsluta annan DEV-instans. Kör:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
npm run i18n:compile
MNO_TEST_DATA_DIR=/Users/stefanantonsson/mno-birds-AFP613/journal npm run tauri dev
```

Kontrollera källsökvägen i DEV-loggen. Aktivera fågelmodulen vid behov.
Välj några befintliga exempel, notera datum/plats och ta gärna skärmbilder:
- Importerade besök: ett X-antal, ett omappat källtaxon och mediareferenser.
  Notera källmärkning, låsta uppgifter och Öppna i eBird. Medialänkar öppnar
  originalsidor externt; arkivet ska bevara referenser, inte ladda ned media.
- Ett eBird-besök med befintlig journalkoppling samt en manuell fågelobservation.
- Journaltext, journalfoto/bildtext och journalväder, samt befintliga manuella
  observationers kommentarer, foton/bildtexter och besöksväder där sådant finns.
- Om Tillägg att granska redan visas: notera innehållet för jämförelse. Skapa inga
  tillägg via SQL. Importerade MNO-radfoton är inte en stödd funktion här.

Stäng av fågelmodulen via Inställningar → Moduler. Detta påverkar även kommande
målstarter, eftersom DEV-valet delas. Skapa sedan via vanliga Inställningar:
- Backup: `/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/archives/ebird-source.mno-backup`
- Export: `/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/archives/ebird-source.mno-export`

Använd Cmd+Shift+G i filväljaren för arkivmappen. Vänta på båda lyckade resultaten.
Avsluta källappen helt. Starta inte källprofilen för någon ersättningsoperation.
Originalbilder ska ligga kvar orörda; lagrade förhandsbilder följer befintlig arkivpolicy.

## 2. Backup → Restore, endast Restore-målet

Första start (kontrollerar även att inget dinglande symlänksbarn finns):

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
test ! -e "/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/restore/journal" &&
test ! -L "/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/restore/journal" &&
MNO_TEST_DATA_DIR="/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/restore/journal" npm run tauri dev
```

Kontrollera loggens `/restore/journal` och att modulen är avstängd. Välj
Inställningar → Restore/Återställ, välj `ebird-source.mno-backup`, granska och
bekräfta ersättning. När appen begär det, välj Avsluta MNO. Vänta på avslut.
Starta om exakt samma mål:

```sh
MNO_TEST_DATA_DIR="/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/restore/journal" npm run tauri dev
```

Välj **Verifiera och öppna journalen**, inte Behåll föregående. Kontrollera kärnans
journaltext, foto/bildtext och journalväder med fågelmodulen fortfarande avstängd.
Aktivera sedan uttryckligen modulen och gör jämförelsen i steg 4. Avsluta, kör
samma startkommando igen och upprepa jämförelsen. Modulvalet ska ha bestått.
Stäng sedan av modulen igen inför Import-målet och avsluta helt.

## 3. Export → Open Import, endast Import-målet

Första start:

```sh
cd /Users/stefanantonsson/Projects/my-nature-observations-public/app
test ! -e "/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/import/journal" &&
test ! -L "/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/import/journal" &&
MNO_TEST_DATA_DIR="/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/import/journal" npm run tauri dev
```

Kontrollera loggens `/import/journal` och avstängd modul. Lämna den nya journalen
tom. Välj Inställningar → Open Import/Importera, välj `ebird-source.mno-export`,
granska och bekräfta hela journalimporten. Detta är inte Importera från eBird.
När journalskiftet är förberett, välj Avsluta MNO och starta om samma mål:

```sh
MNO_TEST_DATA_DIR="/Users/stefanantonsson/mno-ebird-archives-czbp7h1s/import/journal" npm run tauri dev
```

Välj verifiera/öppna den importerade journalen, inte den föregående. Kontrollera
kärninnehållet med modulen avstängd; aktivera sedan och gör steg 4. Avsluta och
starta åter med exakt samma kommando. Kontrollera innehåll och bestående modulval.
Lämna DEV-valet på om du vill ha samma aktivering när källprofilen senare öppnas;
det är ett gemensamt val, inte en ändring av källjournalens innehåll.

## 4. Samma användarsynliga jämförelse i båda målen

- Importerade besök finns en gång vardera, med korrekta datum/platser, källnamn,
  antal inklusive X, omappade taxa, källkommentarer och samtliga mediareferenser.
- Från eBird och originallänken finns. Källuppgifter är låsta; ingen manuell
  källredigering erbjuds. Ingen MNO-plats har skapats genom automatisk matchning.
- Journalkopplingen kan öppnas från båda håll; manuella observationer finns kvar.
- Journaltext, foton/bildtexter, journalväder och tidigare manuellt observationsinnehåll
  motsvarar källan, även efter extra omstart. Prova den delade bildvisaren på
  journalens eller den manuella observationens befintliga bilder.
- Befintlig synlig Tillägg att granska motsvarar källan. Inaktiva rader utan
  MNO-tillägg visas inte i UI. Deras fullständiga innehåll och interna radidentiteter
  verifieras av befintliga automatiska format-14/arkivtester; ingen SQL-kontroll
  krävs av användaren och ingen ny manuell identitetsacceptans påstås.

Anteckna separat resultat för Restore och Open Import samt omstarten i respektive
mål. Om något avviker: behåll alla profiler/arkiv och rapportera vilket mål/steg.
