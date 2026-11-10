# My Nature Observations 0.1.0 — Fedora Preview

My Nature Observations is a local-first nature journal. This preview is a
`0.1.0-1` x86_64 RPM for Fedora KDE 44. Installation and first launch passed
on a separate Fedora KDE 44 VM without the MNO build dependencies installed.
Journal persistence after restart, Backup and Export passed on the Fedora KDE 44
build machine.

## Install on Fedora KDE 44

1. Download the RPM and its matching `.rpm.sha256` manifest from the official
   GitHub pre-release. Keep them together and open a terminal in that folder.
2. Substitute the actual downloaded filename below. Install only if the
   checksum check reports `OK`.

   ```sh
   rpm_file='name-of-downloaded-package.rpm'
   sha256sum --check -- "$rpm_file.sha256" && sudo dnf install "./$rpm_file"
   ```

3. Open **My Nature Observations** from the applications menu. DNF may need
   internet access to fetch runtime dependencies.

## Preview limitations

**Restore** and **Open Import** are unavailable on Linux. Keep backups and
exports outside the application data directory. The test evidence covers
Fedora KDE 44 x86_64.

Report reproducible problems through GitHub Issues. Keep private journal
files and backups out of reports.
[GitHub Issues](https://github.com/MyNatureObservations/my-nature-observations/issues)
