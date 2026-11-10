# Install My Nature Observations on Windows

The Windows preview supports **Windows 11 x64**.

1. Download the Windows x64 `-setup.exe` from the official
   [Windows Preview release](https://github.com/MyNatureObservations/my-nature-observations/releases/tag/v0.1.0-windows-preview).
   Use the SHA-256 published for that exact filename. In PowerShell,
   run the following with the path to your downloaded file:

   ```powershell
   Get-FileHash -Algorithm SHA256 -LiteralPath 'C:\path\to\your-installer.exe'
   ```

   Compare the full hash before opening the installer. Stop if it differs.
2. Run Setup from the Windows account that will use MNO. It installs
   for the current user. MNO uses Microsoft Edge WebView2. If the
   Evergreen runtime is missing, Setup needs internet access to
   download and install it.
3. The installer is unsigned. Microsoft Edge may warn during download,
   and Windows SmartScreen may warn when you open Setup. Confirm the
   official release URL and verify the checksum before running Setup. If Windows or
   your organization's policy blocks installation, stop and report
   the message. Keep your Windows security settings in place.
4. Open **My Nature Observations** from the Start menu.

## Keep your journal and backups

Before reinstalling or removing MNO, use **Settings → Backup** and save
the backup outside the app installation and app-data folders, for example
in a dedicated Documents folder. The journal is stored under
`%APPDATA%\org.mynatureobservations.app`.

During uninstall, **leave the optional app-data deletion box unchecked**
to keep your journal and retained generations. Selecting it can delete
the journal and any backups stored in the app-data folder.

**Backup**, **Export** and **Restore** are available in Settings.
**Open Import** of a portable export is unavailable on Windows.
Upgrades between different Windows versions of MNO have yet to be verified.

For reproducible issues, use [GitHub Issues](https://github.com/MyNatureObservations/my-nature-observations/issues).
For questions, use [GitHub Discussions](https://github.com/MyNatureObservations/my-nature-observations/discussions).
Include the Windows version, installer filename, and exact error text.
Keep personal journal files and backup contents private.
