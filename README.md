# Sony Sound Studio

A local desktop companion for Sony headphones, with experimental OnePlus support. Shape your sound, switch listening modes and manage your headphones from a dedicated desktop workspace.


## Features

### Sound Studio

- **Noise control:** switch between noise cancelling, Ambient Sound and Off, with device confirmation and physical-button state updates on the tested WH-1000XM6.
- **Dedicated Equalizer tab:** choose XM6 Off, Heavy, Clear, Hard, Soft or Game curves; create, update, rename and delete local custom presets, and adjust supported DSEE modes. Sound Studio keeps listening modes and noise controls together. The visualization responds to bass, mids and treble. [Preset behavior and evidence](docs/research/eq-presets.md).
- **Listening modes:** Standard, Background Music and Cinema appear when supported by the connected device. Background Music includes model-supported room options such as My room, Living room and Cafe.
- **Animated headphone scenes:** official model artwork, distinct listening-mode environments and transitions that respond to sound settings.

### Experimental OnePlus support

- **All 32 OnePlus product IDs in the supplied HeyMelody catalog:** 30 model names across Buds, Buds Pro, Buds Z, Buds Ace, Nord Buds, Flow Buds, Buds Clip and Bullets neckbands, plus seven bundled aliases. [Full model list](docs/research/oneplus-models.md).
- Product-ID verification chooses each model's own noise-control indices and EQ preset labels. Firmware/battery reads, ANC/Transparency profiles, EQ presets, wear detection and game mode appear where the model mapping and live replies support them. Non-ANC models do not receive ANC controls.
- Wear detection and game mode appear only after live capability and current-value replies. Changes require device readback; failed confirmation does not send a restoration command.
- **Model-specific equalizers:** built-in sound profiles use each model’s own protocol indices. Custom curves use the earbuds’ reported frequencies and dB range; save edits, rename and apply profiles with matching device readback. Supported models can create a new profile from their original-app tuning layout.
- **Spatial listening:** model-listed Off/fixed modes and head tracking appear only after compatible live capability and current-mode replies. Older firmware uses its spatial on/off path. [Protocol evidence and model coverage](docs/research/oneplus-spatial.md).
- **BassWave:** a dedicated slider in the Equalizer tab uses the connected model’s reported range. Supported models also expose a separate enable switch. Both level changes and enablement require device readback.
- All OnePlus entries are experimental and awaiting hardware testing; these controls are included in the 0.1.4 preview. Models outside the catalog can be discovered by their OnePlus headphone name, but unknown product IDs cannot enable settings. Custom profile deletion and multipoint management are unavailable. macOS control connections remain disabled.

See the [HeyMelody evidence and testing guide](docs/research/oneplus-heymelody.md) for command layouts, original-app references and validation limits.

### Automatic listening · Windows

- A dedicated tab for opt-in media-based switching between Standard and Cinema.
- Movies and shows on supported streaming sites request Cinema through the paired browser companion. Other browser media, including ordinary YouTube videos, requests Standard.
- An optional local VLC connection distinguishes video from audio-only playback when Windows does not expose VLC as a media session.
- Manual mode choices pause automatic switching until you resume it.

See [automatic listening setup](docs/media-following.md) for supported rules, browser pairing and VLC configuration. Automatic Dolby Atmos detection is not implemented.

### Device and desktop settings

- Discover paired Sony and OnePlus headphones, and read available model, firmware, battery and control capabilities.
- Device controls appear when their capabilities and current values have been established; changes stay pending until confirmation.
- Dark, light and system appearance, reduced motion, launch at login, start minimized, close to tray and automatic reconnect preferences.
- Closing to tray releases the studio webview while the native companion remains active. Quit from the tray to disconnect and exit.
- **Signed in-app updates:** check release notes, download updates and install/restart from App settings on Windows, macOS and Linux AppImage. [Update behavior and maintainer setup](docs/app-updates.md).
- An explicit **Studio preview** lets you explore XM6, XM5 and CH720N layouts without sending headphone commands.
- Core operation needs no account, cloud server or telemetry. Preferences and saved presets stay on your computer.

## Download

Get the [latest preview release](https://github.com/OP22007/Sony-Sound-Studio/releases). The 0.1.4 preview is a manual Windows x64 download; GitHub Actions is currently blocked by an account billing issue, so its macOS/Linux packages and stable updater publication are pending. Existing 0.1.3 packages remain available for those platforms. [0.1.4 changes and limitations](docs/releases/v0.1.4.md).

Package formats and platform support:

| Platform | Packages | Current support |
| --- | --- | --- |
| Windows x64 | Setup EXE | Native headphone controls; XM6 firmware 3.1.5 is the hardware-tested baseline |
| macOS Apple Silicon / Intel | DMG | UI and paired-device discovery; headphone control connections are disabled |
| Linux x64 | DEB, RPM, AppImage | BlueZ discovery and RFCOMM implemented; hardware validation remains outstanding |

These are preview builds with signed updater packages; macOS builds are not notarized and the Windows installer has no Authenticode publisher signature. Read the [installation and testing notes](docs/releases/v0.1.3.md) before installing. Automatic listening currently requires Windows. On Linux, install the packages providing `bluetoothctl` and `sdptool` and pair through your operating system first.

**This project is in development and does not yet offer full Sony Sound Connect feature parity.** Hardware confirmation covers XM6 noise-mode tracking and a tested EQ-band change/readback on Windows; it does not establish every control, model or firmware. XM5 and CH720N still need hardware validation. Firmware updates and factory reset are unavailable.

## Feedback and contributions

**Open source contributions are welcome.** Help with Linux/macOS Bluetooth support, headphone compatibility testing, accessibility, interface improvements, documentation or bug fixes.

- [Share testing feedback or report a bug](https://github.com/OP22007/Sony-Sound-Studio/issues/new?template=tester-feedback.yml). Include app version, OS/CPU, headphone model/firmware and steps to reproduce.
- Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, validation, protocol evidence requirements and the pull request workflow.
- Open an issue before starting a substantial feature so we can agree on its scope. Small fixes and documentation improvements can go straight to a pull request.

Review screenshots and diagnostics before posting; remove pairing keys, passwords, device addresses and personal media information.

## Run from source

### Product website

The standalone animated product site lives in [`website/`](website/README.md). Run `npm ci` and `npm run dev` from that directory to preview it on port 4321. It builds to static files for GitHub Pages or Vercel; the desktop app keeps its own dependencies and build.

### Desktop app

Built with Tauri 2, Rust, React, TypeScript, Vite and Anime.js. Install Node.js 22+, Rust 1.90+ and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
git clone https://github.com/OP22007/Sony-Sound-Studio.git
cd Sony-Sound-Studio
npm ci
npm run tauri dev
```

For the browser-only interface, use `npm run dev`, open `http://127.0.0.1:1420` and choose **Explore studio preview**. Native Bluetooth needs the desktop app. If port 1420 is in use, stop the existing development server before starting another.

Pair and power on your headphones, then choose **Find headphones**. A paired entry does not guarantee an available control connection; another control app or phone connection can affect it.

## Development checks

```sh
npm run build
node --test extensions/media-mode/test/extension.test.cjs
npx playwright install chromium
npm run test:e2e
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
npm run tauri build
```

Branch and pull request CI checks the frontend, browser companion, browser integration tests and native Rust tests/builds on Windows, macOS and Linux. Version tags use the separate release workflow to package installers. Passing CI establishes build/test results; Bluetooth behavior still needs hardware testing.

## Evidence and project status

The app separates preview state, advertised capabilities and confirmed device values. A transport acknowledgement alone does not confirm that a setting changed.

- [Feature coverage and validation gaps](docs/research/feature-parity-matrix.md)
- [Device control status](docs/research/device-controls-status.md)
- [Original-app evidence report](docs/research/sony-sound-connect-evidence.md)
- [Hardware-confirmed XM6 noise-control schema](docs/research/v2-ncasm-19-schema.md)
- [Desktop preference behavior](docs/app-settings.md)
- [Resource measurements and their limits](docs/research/resource-measurements.md)

The [sony-device-center](https://github.com/marconvcm/sony-device-center) project was inspected as a secondary reference; its code was not copied. APK/APKM inputs, decompiled content and private raw research captures are excluded from this repository.

## License and attribution

Independently authored project code is [MIT licensed](LICENSE), copyright © 2026 Om Patil. Sony product artwork, trademarks, original app packages and decompiled material are not covered by that license. See the [artwork attribution manifest](public/headphones/SOURCES.md).

This is an independent project, unaffiliated with and not endorsed by Sony.
