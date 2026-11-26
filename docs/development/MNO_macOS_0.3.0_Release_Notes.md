# My Nature Observations 0.3.0 for macOS

## Svenska

- Importera din personliga CSV-export från eBird. Aktivera **Fågelobservationer** och **Import från eBird** under **Inställningar → Moduler**. Importanslutningen är avstängd tills du aktiverar den.
- Granska nya, ändrade och oförändrade besök före import. Samma export kan importeras igen utan dubbletter. Besök som saknas i en senare export bevaras. Källrättelser uppdaterar besöket och behåller dess journalkopplingar.
- eBirds källfält är skrivskyddade i MNO. Du kan läsa tidigare importerade besök och koppla dem till journalen när fågelmodulen är på, även med eBird-importen avstängd. Medielänkar öppnas externt och media laddas inte ned.
- Om en källrad försvinner eller inte kan matchas entydigt bevaras tidigare uppgifter. Egna tillägg på inaktiva rader kan visas för granskning. Omfördelning av tillägg, egna foton på importerade artrader och koppling till sparade MNO-platser ingår inte. Efter omstart kräver en oavslutad import ny granskning och bekräftelse.
- För Apple Silicon med macOS 13 eller senare. Ingen motsvarande eBird-import för Windows eller Linux. Uppgraderingen från v0.2.0 migrerar journalen till databasschema 13. Spara en säkerhetskopia från v0.2.0 före byte. För återgång till v0.2.0, använd den tidigare säkerhetskopian.

RC1-appen är ad hoc-signerad och inte notariserad. DMG:n är osignerad. Gatekeeper avvisade appen vid kontroll.

## English

- Import your personal eBird CSV export. Enable **Bird observations** and **Import from eBird** under **Settings → Modules**. The import connection is off until you enable it.
- Review new, changed and unchanged visits before importing. Reimporting the same export creates no duplicates. Visits absent from a later export are retained. Source corrections update the visit while preserving its journal links.
- eBird source fields are read-only in MNO. With Bird observations enabled, previously imported visits remain readable and linkable while eBird import is off. Media links open externally and media is not downloaded.
- When a source row disappears or cannot be matched unambiguously, previous data is retained. Local additions on inactive rows can appear for review. Reassigning additions, attaching local photos to imported species rows and associating source locations with saved MNO places are outside this release. After restarting, an unfinished import needs a fresh review and confirmation.
- For Apple Silicon Macs running macOS 13 or later. eBird import is unavailable on Windows and Linux. Upgrading from v0.2.0 migrates the journal to database schema 13. Make a v0.2.0 backup before replacing the app. To return to v0.2.0, use that earlier backup.

The RC1 app is ad hoc signed and is not notarized. The DMG is unsigned. Gatekeeper rejected the app during assessment.

[Packaged RC1 acceptance](MNO_macOS_0.3.0_RC1_Acceptance.md).
