# R1E2: kartans världsvy – historik och uppföljning i R2E3

## Ursprunglig begränsning i R1E2

När en plats skapades utan koordinater visades en komplett Web Mercator-värld
utan upprepade karttiles, men startvyn var inte optimalt anpassad till den breda
kartdialogens proportioner. Tile-fel visades med ett informationsmeddelande;
sparade platser och manuella koordinater fungerade även utan kartbakgrund.

## Löst i R2E3

- Världsvyn anpassas till kartytans faktiska mått med contain-beteende. Hela
  Web Mercator-världen visas med neutralt utrymme där proportionerna kräver det,
  utan beskärning eller upprepade världskartor.
- Storleksändring anpassar en orörd världsvy, men bevarar användarens panorering,
  zoomning och uttryckliga platsval. Kamerapositionen skapar inga koordinater.
- Tile-fel visas diskret med **Försök igen**. Samma tile-lager tas bort och läggs
  till igen genom Leaflets publika API; kameran, markörer, platsval, koordinater
  och OSM-attribution bevaras. Detta undviker det identifierade felet med
  `redraw()` vid fraktionell zoom i Leaflet 1.9.4.
- En tom ersättningsbatch räknas inte som lyckad återhämtning. Misslyckade
  ersättningstiles behåller relevant varning och möjlighet till ett nytt försök.

Världsvy, storleksändring och tile-återhämtning är manuellt verifierade i Tauri.
Se [R2:s slutförandenot](R1_Places_Weather.md#r2--journal--places-ux-slutförd-implementation)
för gemensam omfattning och verifiering.

## Kvarstående operativ osäkerhet

Kartbakgrunden kräver nätåtkomst och OpenStreetMaps tillgänglighet. Sparade
platser och manuella koordinater är fortsatt användbara utan tiles.

De faktiska identifierings-/request-headeruppgifterna och HTTP-cachebeteendet i
paketerad Tauri/WebKit har ännu inte verifierats mot
[OSM:s tile-policy](https://operations.osmfoundation.org/policies/tiles/).
Detta är en känd operativ osäkerhet, inte ett bekräftat policy- eller appfel.
Ingen ny leverantör, offlinecache eller automatisk återhämtningsmekanism har
införts för att kringgå den osäkerheten.
