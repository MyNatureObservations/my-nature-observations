# My Nature Observations 0.1.0 — Ubuntu Preview

My Nature Observations is a local-first nature journal. This preview is an
amd64 Debian package built from source commit
`06a8b2058a86ad84192e70f53c2327b5b561f06d`. It was tested on Ubuntu 26.04.1
x86_64 with Wayland. The installed app passed a clean first launch, saved and
reopened a journal entry after restart, and completed Backup and Export.

## Install on Ubuntu

1. Download the `.deb` and its matching `.deb.sha256` manifest from the official
   GitHub pre-release. Keep the files together and open a terminal in that folder.
2. Replace the example filename below with the actual name of the downloaded
   package. Continue only if the checksum check reports `OK`.

   ```sh
   deb='name-of-downloaded-package.deb'
   sha256sum --check -- "$deb.sha256"
   ```

3. Install with `sudo apt install "./$deb"` and open **My Nature Observations**
   from the applications menu. APT may need internet access to fetch dependencies.

## Preview limitations

**Restore** and **Open Import** are unavailable on Linux. Keep backups and exports
outside the application data directory. This preview was tested on the Ubuntu
configuration above. Other Linux distributions, architectures and desktop
sessions remain outside the tested scope.

Use GitHub Issues for reproducible problems or GitHub Discussions for questions.
Keep private journal files and backups out of reports.
[Issues](https://github.com/MyNatureObservations/my-nature-observations/issues) ·
[Discussions](https://github.com/MyNatureObservations/my-nature-observations/discussions)
