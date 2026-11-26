# macOS v0.3.0 RC1 packaged acceptance

The owner reported the staged macOS acceptance checks passed on 2026-11-26.

- Tested build commit: `6de2c589e4d2735ba2668e32a0715604b764f43e`, based on `8a53c564884b73fabd9787f27169e422450b242a`. Later commits record acceptance and release wording. The DMG was built from the tested build commit.
- Artifact: `My-Nature-Observations-0.3.0-macos-arm64-rc1-6de2c58.dmg`.
- SHA-256: `248d25b3a4579677f57dabae11c011f9ce98cbaeb38e329f992c8f4e8086f682`. The accepted file was rehashed and matched before submission for review.
- Package inspection verified version 0.3.0, Apple Silicon arm64, macOS 13.0 minimum, production identifier `org.mynatureobservations.app`, the embedded MIT license, the arm64 notice bundle and DMG integrity. Strict signature verification passed for the ad hoc signed app. The DMG is unsigned, the app is not notarized, and Gatekeeper rejected it during assessment.

## Accepted manual checks

| Stage | Result |
| --- | --- |
| Upgrade from public v0.2.0 | The test journal was backed up before upgrading. Journal and bird content, journal links and the saved bird-module choice survived the upgrade. eBird import started disabled. |
| eBird import and restart | Enabling the connection allowed the test CSV to be imported. Identical reimport after restart produced no duplicates. A controlled source correction updated the visit and preserved its journal link. |
| Connection disabled | Previously imported visits remained accessible with eBird import off and the bird module enabled. The v0.3.0 Backup and Export were created in this state. |
| Backup → Restore | In a separate Restore account, the backup was restored and verified after restarting the candidate. Journal content, manual and imported observations, the correction and journal links survived another restart. The target's eBird setting stayed off. |
| Export → Open Import | In a separate Import account, the export was imported and verified after restarting the candidate. Journal content, manual and imported observations, the correction and journal links survived another restart. The target's enabled eBird setting survived replacement and restart. |

These results are packaged RC1 acceptance using disposable test data and separate accounts. Earlier DEV and automated import evidence is recorded separately. [Confirmed import evidence](MNO_Ebird_Confirmed_Import.md).

[Bilingual release notes](MNO_macOS_0.3.0_Release_Notes.md) · [Public v0.2.0 upgrade source](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.2.0)
