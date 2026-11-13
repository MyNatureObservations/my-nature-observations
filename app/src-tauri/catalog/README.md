# MNO offline bird catalog

Source: official AviList v2026 Explorer CSV, `avilist-extended-33698-taxa.csv`.
Explorer: https://explore.avilist.org/
Release: https://www.avilist.org/checklist/v2026/
Taxonomy data: https://explore.avilist.org/data/avilist-2026.json
Version release date: 2026-10-20. CSV export date: unknown. Inspected: 2026-11-11.
Source CSV SHA-256: `be9df523541c42736f21696be902eb9adabfcc4b9f4501c2562c88eac4d81ed5`.

Attribution: AviList Core Team. Swedish alternative names are supplied through
eBird/Cornell species codes; eBird credits Erling Jirle, Örjan Sjögren and Markus
Lagerqvist for Swedish names. https://support.ebird.org/en/support/solutions/articles/48000804865
License reference: CC BY 4.0, https://creativecommons.org/licenses/by/4.0/.
The project owner's decision of 2026-11-11 interprets the published notice as
covering the entire export, including Swedish names; this is an owner decision,
not a separately verified license for that column. See the approved requirements.

Conversion: `python3 app/scripts/birds/convert.py /path/to/avilist-extended-33698-taxa.csv`.
Select only `RANK == species` from 33,698 taxa. Preserve source ordering and exact
Avibase concept IDs, scientific names, English AviList names and Swedish labels.
Retain 11,172 species; represent the 28 absent Swedish names as JSON null. No
species merging, synonym inference, code remapping or narrower identification.
Other source columns are omitted. Output is UTF-8 compact JSON with a final LF.
MNO displays English plus scientific name when Swedish is absent; these species
remain selectable. Both English and Swedish labels are searchable offline.

## Targeted crow search investigation (2026-11-12)

The supplied species export contains `avibase-E4EE9AC4`, **Corvus corone**,
English **Carrion Crow**, with no Swedish label or Cornell species code. Its
proposal 2025-1080 / decision summary explicitly includes cornix within corone.
The full CSV separately contains **Corvus corone cornix**, rank `subspecies`,
`avibase-81B32602`, also without a Swedish label. Subspecies are deliberately not
converted into this species-only catalog; there is no separate Corvus cornix
species record. The converter and the 28 missing-name entries are unchanged.

Thus `kråka` previously returned other substring matches (80 species), while
`gråkråka` returned none. This is missing Swedish name data for the parent species,
not a case-folding failure.

## Sourced Swedish name supplement and explicit referral (2026-11-13)

Following the owner's presentation decision, `name-supplements.json` supplies the
Swedish display name **Kråka** for `avibase-E4EE9AC4` / Corvus corone / AviList v2026.
It is applied consistently to search results, selection and reopened list/preview
views. English remains **Carrion Crow**. The original CSV and converted AviList
JSON stay unchanged: the source still has 28 Swedish-name gaps, one of which now
has a separately sourced display supplement. Other gaps keep the agreed fallback.

Sources retained in the supplement metadata and this record:

- [SLU Artdatabanken, Kråka eller gråkråka?](https://www.slu.se/artdatabanken/arter-och-natur/artiklar/kraka-eller-grakraka/), reviewed 2025-05-22: kråka = Corvus corone; gråkråka = C. corone cornix.
- [BirdLife Sverige NL2025](https://cdn.birdlife.se/wp-content/uploads/2025/06/NL2025.pdf), species row 7232: Corvus corone / Carrion Crow / kråka.
- [SLU, 2025-06-23 taxonomy update](https://www.slu.se/artdatabanken/rapportering-och-fynd/artportalen/nyheter/midsommaruppdatering-av-artportalens-taxonomi/) distinguishes reporting gråkråka and svartkråka after the species lump.

`gråkråka` searches offer an explicitly marked referral to the parent species,
explaining in Swedish/English that selection records the species, not the
subspecies. Referral text exists only on search results; it is not a name, stored
taxon identity, merged concept or subspecies registration. The parent is never
labelled “Gråkråka (kråka)”. Exact species names retain priority over referral ties.
Existing observations retain their Avibase IDs and recorded taxonomy versions.
This targeted supplement is not general BirdLife integration or a catalog updater.
