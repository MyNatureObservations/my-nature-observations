# My Nature Observations (MNO) – överlämning till nästa chatt

**Datum:** 13 oktober 2026\
**Status:** Dagens utveckling avslutad, kontrollerad, committad och pushad till GitHub. Användaren har bekräftat att `git status` visar ren arbetsyta.\
**Syfte:** Fortsätta utvecklingen i en ny chatt utan att blanda ihop MNO med det äldre projektet *Mina naturobservationer*.

## 1. Börja här i nästa chatt

Vi utvecklar **My Nature Observations (MNO)**, en **ny, separat naturjournal**. Detta är **inte** det äldre observations-/statistikprogrammet *Mina naturobservationer*, även om det senare kan bli en integrationskälla.

Senaste arbetspasset färdigställde:
- Skapande, öppning, redigering och sparande av journalanteckningar.
- Redigering av anteckningsdatum.
- Skydd mot att lämna osparade text- eller datumändringar.
- Egen bekräftelsedialog för att fortsätta redigera eller lämna utan att spara.
- Städning av felsökningsloggar, TypeScript-fel och Svelte-varningar.
- Commit och push till `origin/main`; användaren bekräftade att allt är klart och arbetsytan är ren.

**Senaste verifierade kvalitetskontroll:** `npm run check` → **0 errors, 0 warnings**.

**Rekommenderad start:** Fråga vad användaren vill prioritera härnäst, eller föreslå en kort genomgång av journalens nästa UX-funktioner. Börja inte om med osparade-ändringar-skyddet: det är färdigbyggt och testat för de scenarier som anges nedan.

## 2. Projektvision och produktprinciper

MNO ska vara en personlig, lugn, lokal naturjournal/dagbok för upplevelser och iakttagelser. Visuell riktning: **Japandi** – skandinavisk minimalism med japansk ordning och balans, **inte** bok-/rispapper-/bambu-/japansk-trädgård-metaforer.

Grundprinciper:
- **Local-first, integritet och användarkontroll.** Ingen press, inga påminnelser eller skuldbeläggande funktioner.
- **Dataportabilitet är högsta prioritet.** Export i öppna, dokumenterade format; undvik inlåsning.
- Journalinnehåll bör lagras som **versionerad strukturerad JSON**, oberoende av redigerarleverantör.
- Flera journaler ska kunna finnas och sökas övergripande på sikt.
- Flexibla anteckningar: text först, senare bilder, små kartor, teckningar/handstil, externa länkar och frivilliga strukturerade observationer.
- Anteckningens datum väljs av användaren och behöver inte vara samma som skapandedatum.
- Bilder ska kunna länkas till original eller kopieras. **MNO får aldrig radera originalfiler utanför sina egna hanterade data.**
- Känsliga lokaler, t.ex. svamp- eller fiskeplatser, ska kunna hållas privata.
- Framtida idéer: import från eBird/Artportalen, koppling till strukturerade observationer, sökning, journalöversikt som växer med innehållet, frivilligt lås med PIN/biometri, penna på iPad.
- MVP: skapa anteckning med datum, rubrik/text; därefter bilder och karta.

Dessa är **produktmål**, inte en lista över redan implementerade funktioner.

## 3. Teknik och arbetsmiljö

- Plattform: **macOS**.
- Projektrot: `~/Projects/my-nature-observations/app`.
- Appskal: **Tauri 2** med Rust-backend.
- Gränssnitt: **Svelte 5**, **SvelteKit 2** (tidigare observerat 2.70.3), TypeScript.
- Textredigerare: **Tiptap 3**.
- Lagring: lokal **SQLite** via Rust.
- Utvecklingsstart: `cd ~/Projects/my-nature-observations/app && npm run tauri dev`.
- Kontroll: `npm run check`.
- Git: branch `main`, remote `origin` på GitHub.
- Terminalredigering: användaren föredrar **nvim**.
- När terminalutdata ska klistras in i chatten: använd gärna `| pbcopy`, t.ex. `npm run check 2>&1 | pbcopy`.
- **Viktigt:** HMR/hot reload har ibland inte slagit igenom. Om en ändring inte verkar få effekt: starta om appen innan mer felsökning.

## 4. Relevanta filer

Följande nio filer ingick i den sista commiten (enligt användarens staged-status):

```text
src-tauri/src/journal.rs
src-tauri/src/lib.rs
src/app.d.ts
src/lib/components/DatePicker.svelte
src/lib/components/JournalEditor.svelte
src/routes/journal/+page.svelte
src/routes/journal/entries/+page.svelte
src/routes/journal/entries/[id]/+page.svelte
src/routes/journal/new/+page.svelte
```

Ändringarnas omfattning vid staging: **9 filer, 1367 additions, 45 deletions**. Commit-meddelande som föreslogs och sedan användes i det bekräftade avslutet:

```text
Add journal entry management and unsaved changes protection
```

Exakt commit-hash är **inte dokumenterad** här. Hämta den vid behov med `git log -1 --oneline`.

## 5. Implementerad journalfunktionalitet

### Anteckningsflöden

- Journalöversikt och anteckningslista.
- Ny anteckning via `src/routes/journal/new/+page.svelte`.
- Befintlig anteckning via `src/routes/journal/entries/[id]/+page.svelte`.
- Tiptap-redigeraren i `src/lib/components/JournalEditor.svelte`.
- Datumväljaren i `src/lib/components/DatePicker.svelte`.
- Rust-kommandon för att hämta och spara journalanteckningar i `src-tauri/src/journal.rs`, registrerade i `src-tauri/src/lib.rs`.

### JournalEditor – viktiga tekniska detaljer

Komponentens tidigare kända props:

```ts
entryDate: string;
isNew?: boolean;
journalEntryId?: string | null;
onEditingChange?: (editing: boolean) => void;
```

Tiptap använder `Editor` från `@tiptap/core` med `StarterKit`.

Kända kommandon/anrop från den senaste utvecklingen:

```ts
invoke('get_journal_entry', { id: journalEntryId })
invoke('load_journal_entry', { entryDate })
invoke('save_journal_entry', {
  id: entryId,
  entryDate,
  title: generateTitle(),
  contentJson: JSON.stringify(editor.getJSON())
})
```

Efter att en ny anteckning sparats navigeras till dess permanenta URL med `goto('/journal/entries/${id}', { replaceState: true, state: { justSaved: true } })`. Spara-bekräftelse visas även efter denna navigering. Den egna SvelteKit-sidtypen finns i `src/app.d.ts`:

```ts
declare global {
  namespace App {
    interface PageState {
      justSaved?: boolean;
    }
  }
}
export {};
```

**Varning:** I detaljsidan behövs importen av `page` från `$app/state` för att läsa `page.params.id`. Den togs bort vid ett tidigare tillfälle och då slutade anteckningsnavigeringen fungera.

### Osparade ändringar – färdig lösning

Tidigare sattes `hasUnsavedChanges = true` direkt i Tiptaps `onUpdate`. Det gav **falskt positivt** redan när man klickade på **Redigera**. Det löstes genom att jämföra aktuell editor-JSON mot en sparad snapshot:

```ts
let hasUnsavedChanges = $state(false);
let savedContentJson = '';
let savedEntryDate = $state(''); // kontrollera exakt deklaration i aktuell fil

let hasPendingChanges = $derived(
  hasUnsavedChanges ||
  (savedEntryDate !== '' && entryDate !== savedEntryDate)
);

// I Tiptap-konfigurationen:
onUpdate: ({ editor }) => {
  if (isEditing) {
    hasUnsavedChanges =
      JSON.stringify(editor.getJSON()) !== savedContentJson;
  }
}
```

`savedContentJson` uppdateras efter att anteckningen laddats och efter lyckad sparning. `savedEntryDate` uppdateras likaså. Detta gör att även **⌘Z/ångra** kan återställa `hasUnsavedChanges` till `false` när innehållet åter matchar det sparade.

För navigering används SvelteKits `beforeNavigate`. `window.confirm()` **fungerade inte** i den aktuella Tauri/WebKit-miljön: ett manuellt test i Web Inspector returnerade `false` utan att visa dialog. Därför finns en **egen dialogruta** i komponenten.

Kända delar av den slutliga lösningen:

```ts
let showLeaveConfirmation = $state(false);
let pendingNavigationUrl = $state<string | null>(null);
let allowDiscardNavigation = false;

beforeNavigate((navigation) => {
  if (!hasPendingChanges || allowDiscardNavigation) return;

  navigation.cancel();
  pendingNavigationUrl = navigation.to?.url.href ?? null;
  showLeaveConfirmation = true;
});

async function leaveWithoutSaving() {
  if (!pendingNavigationUrl) return;

  const destination = pendingNavigationUrl;
  showLeaveConfirmation = false;
  pendingNavigationUrl = null;
  allowDiscardNavigation = true;

  try {
    await goto(destination);
  } finally {
    allowDiscardNavigation = false;
  }
}
```

Dialogens rubrik: **Osparade ändringar**. Knappar: **Fortsätt redigera** och **Lämna utan att spara**. Den har mörk backdrop (`rgba(0, 0, 0, 0.35)`) som användaren tycker är något kraftig men **vill behålla tills vidare**. Utseendet bedömdes i övrigt som bra. Dialogen är för närvarande avsiktligt enkel; framtida tillgänglighetsförbättringar kan innefatta fokushantering och Escape.

**Viktigt:** Kodutdragen här är orienterande från arbetsdialogen, inte en garanterat fullständig kopia av slutfilen. Läs faktisk kod innan ändringar.

## 6. Tester som användaren uttryckligen bekräftat

1. Att öppna en befintlig anteckning utan ändringar och navigera tillbaka fungerar utan varning.
2. Att klicka **Redigera** utan att skriva sätter inte längre smutsflaggan; att skriva gör det; att ångra hela ändringen med ⌘Z återställer flaggan.
3. När en befintlig anteckning ändrats och användaren försöker navigera bort visas den egna dialogen.
4. **Fortsätt redigera** stänger dialogen och bevarar osparad text i editorn.
5. **Lämna utan att spara** navigerar tillbaka; vid återöppning är osparade textändringar inte kvar.
6. Om ändringarna sparas går det att navigera normalt igen.
7. Om **endast datumet** ändras visas dialogen. Fortsätt redigera behåller det valda datumet; lämna utan att spara behåller originaldatumet i databasen.
8. Om en **ny, ännu osparad anteckning** innehåller text visas dialogen vid navigering. Fortsätt redigera behåller texten; lämna utan att spara skapar ingen anteckning.
9. Efter CSS- och typstädning: `npm run check` → **0 errors, 0 warnings**.
10. Användaren bekräftade slutligen att commit och push genomfördes och att Git-arbetsytan är ren.

## 7. Dagens städning

- Tog bort tillfälliga `console.log` och felsöknings-`$effect` från JournalEditor.
- Lade till `src/app.d.ts` med `App.PageState.justSaved?: boolean` för att lösa TypeScript-felet.
- Gjorde `pendingNavigationUrl` reaktiv med `$state<string | null>(null)`.
- Gjorde `dateInput` i DatePicker reaktiv med `$state<HTMLInputElement | undefined>(undefined)`.
- Tog bort oanvänd CSS-regel `.date-edit button` i DatePicker.
- Tog bort oanvända CSS-regler `h1`, `.entry-body`, `.entry-body p` samt mobil-`h1` i journalöversikten.
- Slutkontroll utan fel eller varningar.

## 8. Öppna frågor och lämpliga nästa steg

**Ingen akut blockerare är känd.** Bestäm prioritet tillsammans med användaren vid nästa pass.

Möjliga nästa uppgifter (inte överenskomna som en fast ordning):
- Nästa naturliga steg i journalens användarupplevelse: förfina flödet för nya och befintliga anteckningar.
- Kontrollera om navigeringsskyddet även ska täcka **stängning av Tauri-fönstret/appen**, extern navigering och andra navigeringssätt; de är **inte verifierade** av dagens tester.
- Tillgänglighet i den egna dialogen: initialt fokus, fokusfälla, Escape, återställning av fokus.
- Bedömning av tomma anteckningar och om sparning utan innehåll ska tillåtas.
- Framtida stöd för bilder och små kartor i journalen.
- Fortsatt designarbete inom Japandi-riktningen, men behåll nuvarande dialogutseende tills användaren vill ändra det.
- Övriga större produktmål (portabilitet, flera journaler, sökning, importer) är framtida arbete.

**Viktigt:** Ändra inte eller återuppfinn inte fungerande lösningar i onödan. Börja med att läsa de berörda filerna och föreslå en enda konkret förbättring.

## 9. Arbetsstil och samarbetsregler

Användaren föredrar:
- **Ett litet steg i taget**: ge en konkret ändring/ett test, invänta återkoppling, fortsätt sedan.
- Tydliga filvägar och exakt vad som ska ersättas, läggas till eller tas bort.
- Kommandon anpassade till macOS och `nvim`; använd `| pbcopy` där det är praktiskt.
- Kontrollera efter varje steg, särskilt efter ändringar i navigation, reaktivitet eller Rust.
- Ingen stor mängd kod eller många samtidiga ändringar om det kan undvikas.
- Svenska i samtalet; vänlig, saklig och gärna lite 🌿😊.
- Skilj alltid **MNO** från det gamla projektet **Mina naturobservationer**.

## 10. Bra första kontrollkommandon nästa gång

```bash
cd ~/Projects/my-nature-observations/app
git status
git log -1 --oneline
npm run check
```

Starta utvecklingsappen vid behov:

```bash
npm run tauri dev
```

**Slutstatus 2026-10-13:** Allt dagens arbete sparat på GitHub, inga kända Svelte/TypeScript-fel eller varningar, navigeringsskyddet fungerar i testade scenarier. Användaren planerar att återuppta arbetet nästa dag, men kan återkomma redan under kvällen.
