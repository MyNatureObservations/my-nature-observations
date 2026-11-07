# Install My Nature Observations on Windows

The Windows preview is intended for **Windows 11 x64**. These
instructions apply once a Windows installer is available.

1. Download the Windows x64 `-setup.exe` from the official
   [GitHub Releases page](https://github.com/MyNatureObservations/my-nature-observations/releases).
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
3. An unsigned installer can trigger an unknown-publisher or SmartScreen
   warning. Confirm the download source and checksum. If Windows or
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

**Open Import** of a portable export is unavailable on Windows. Restore
from an MNO backup is available in Settings.

For reproducible issues, use [GitHub Issues](https://github.com/MyNatureObservations/my-nature-observations/issues).
For questions, use [GitHub Discussions](https://github.com/MyNatureObservations/my-nature-observations/discussions).
Include the Windows version, installer filename, and exact error text.
Keep personal journal files and backup contents private.
