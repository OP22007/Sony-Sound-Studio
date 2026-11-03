# Signed app updates

Open **App settings → App updates → Check for updates**. A newer release shows its notes and an **Install & restart** button. Installation is always your choice. Downloads continue if you close the window to the tray; reopen it to see progress.

Windows setup installations, macOS app bundles and Linux AppImage support in-app installation. DEB/RPM users install a new package from [Releases](https://github.com/OP22007/Sony-Sound-Studio/releases). Development builds cannot install updates over their executable.

Versions before 0.1.2 need one manual upgrade to an updater-enabled build. The update channel includes this project's preview releases. App settings shows a failed check as an error, never as “up to date”.

The native companion fetches a fixed HTTPS feed and verifies each downloaded package against its compiled signing public key before installation. A failed download or signature leaves headphone sessions running. The Windows installer shuts down sessions and cleans up Tauri before exiting; other platforms restart after installation. No GitHub access token is shipped in the app. Release notes are displayed as plain text.

Packages must also carry a signed version matching the feed. This prevents a modified feed from presenting an older signed installer as a newer release. Tauri's release builder signs this version automatically.

The updater temporarily buffers the installer in memory during download and verification; this memory is released after installation or failure. It does not add a background polling service. The app contacts GitHub only when you request an update check or installation.

Updater signatures do not replace Windows code signing or Apple signing/notarization. macOS previews still have the unsigned-app warning.

## Maintainer release setup

The release workflow uses `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` GitHub Actions secrets. It creates signed artifacts for all four architectures, publishes the versioned release, then publishes `latest.json` on the fixed `updater-preview` release. Artifact URLs point at the versioned release. Publication is serialized and refuses to replace the feed with an older version.

Version tags are restricted to repository administrators. Before signing, the workflow verifies that the tag uses `vMAJOR.MINOR.PATCH`, points at a commit already on protected `main`, and matches the application version and saved release notes. Treat release operators and all code merged into main as trusted: native build scripts execute with the signing environment during packaging.

The initial signing key is stored outside the checkout at `%USERPROFILE%\.tauri\sound-studio\updater.key`, with its password protected locally in `password.dpapi` using Windows DPAPI. Back up the private key and password securely: DPAPI is tied to the Windows account and machine. Never commit either file or print them in logs. Losing the signing key requires a manually installed key-transition release for existing users.

For your own fork, generate a separate key using Tauri's signer, configure your repository secrets, and replace the public key and feed URL in `src-tauri/tauri.conf.json`. Local release packaging with updater artifacts requires the same signing environment variables. Ordinary source builds and development mode do not require the private key.
