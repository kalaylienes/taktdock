# Testing the updater end to end

The updater is the one part of the app that cannot be fixed after the fact: a
release that installed copies cannot update to strands every one of them. This
is how it was tested before the first release, and how to test it again after
anything touches `update.rs`, the key, or the bundle configuration.

The idea is two builds of a separate product, an old one and a new one, both
signed with the real key, and a local server playing the part of GitHub.

1. Two configuration overlays, identical except for the version:

   ```json
   {
     "productName": "TaktDockUpdateTest",
     "version": "1.0.0",
     "identifier": "com.kalaylienes.taktdock.updatetest",
     "plugins": {
       "updater": {
         "endpoints": ["http://127.0.0.1:8765/latest.json"],
         "dangerousInsecureTransportProtocol": true,
         "windows": { "installMode": "quiet" }
       }
     }
   }
   ```

   A different product name and identifier keep the test copy out of the real
   installation and out of its single instance lock. The insecure transport
   flag exists only in these overlays and must never reach a real build.

2. Build both with the signing key in the environment
   (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). The
   overlays need `"bundle": { "createUpdaterArtifacts": true }` as well, since
   the manifest leaves update packages off for ordinary builds:

   ```powershell
   npx tauri build --config old.json
   npx tauri build --config new.json
   ```

3. Put the new setup executable and a `latest.json` naming it, with the
   contents of its `.sig` file as the signature, in a folder, and serve it:

   ```powershell
   python -m http.server 8765 --bind 127.0.0.1
   ```

4. Install the old one with `/S`, and start it with its own data directory so
   it does not read the real settings (a `settings.json` there with
   `"widget": {"visible": false}` keeps it off screen):

   ```powershell
   $env:TAKTDOCK_DATA_DIR = "$PWD\data"
   & "$env:LOCALAPPDATA\TaktDockUpdateTest\taktdock.exe" --update
   ```

5. The log in that data directory should show, within a few seconds:

   ```
   bundle TaktDockUpdateTest 1.0.0
   update available: 1.0.1
   installing 1.0.1 on request
   bundle TaktDockUpdateTest 1.0.1
   ```

   The last line is the new copy, started by the installer after it replaced
   the old one. It only gets there if the signature matched the public key
   compiled into the old copy.

6. Uninstall with `%LOCALAPPDATA%\TaktDockUpdateTest\uninstall.exe /S`.

Last run: 10 September 2026, passed.
