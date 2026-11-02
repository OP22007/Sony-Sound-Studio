# Sound Connect Desktop

An independent, local-first desktop companion for Sony headphones, built with **Tauri 2, Rust, React, TypeScript and Vite**. The interface offers dark and light studio themes, device switching, an interactive EQ workspace and reduced-motion support.

This project is in active development. **It does not yet provide complete Sony Sound Connect feature parity.** The app distinguishes real Bluetooth state from an explicit, isolated studio preview. Sony and its product names belong to their respective owners; this project is not affiliated with or endorsed by Sony.

## Current behavior

- On Windows, the native Rust backend discovers paired Sony devices, opens a Sony RFCOMM control service and reads XM6 3.1.5 protocol/model/firmware/battery data. A live 500 Hz EQ step change and restoration were confirmed by device readback. The user now confirms the corrected inquiry `0x19` build tracks three tested NC/ASM modes and physical NC/AMB button changes on the WH-1000XM6.
- macOS paired-device discovery is implemented, but control connections report unsupported until RFCOMM writes have a safe, verified deadline. Linux BlueZ enumeration and RFCOMM are implemented in source and need hardware validation; the adapter requires `bluetoothctl` and `sdptool`.
- Protocol framing handles escaping, checksums, partial/coalesced reads, invalid frames and bounded buffering. A read-only protocol-info handshake is independently derived from the supplied original-app artifact.
- Windows XM6 connection, protocol-info, model, firmware, battery, one EQ band SET/readback/restore, and NC/ASM inquiry `0x19` GET/SET handling have been exercised. The user confirms the corrected rebuilt app matches all three tested NC/ASM modes and follows physical NC/AMB button changes on the WH-1000XM6 running firmware 3.1.5. This result is specific to that model, firmware and Windows; it does not establish support for every setting or model.
- Studio preview is opt-in, visibly labeled and local to this computer. Preview EQ, noise/ambient controls, model-specific state and saved presets do not send headphone commands. Reload returns to real device mode.
- Automatic listening has its own navigation tab, with opt-in Standard/Cinema rules, browser companion pairing and an optional direct VLC connection. See [setup and limitations](docs/media-following.md).
- Closing the desktop window is intended to release its webview while Rust/tray state remains active. **Quit** in the tray exits the app. Resource and lifecycle checks must be reported separately from browser tests.
- No account, server or telemetry is required for core operation. Diagnostic export is explicit and reviewable.

## Run locally

Install Node.js, Rust and the [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/). Windows development requires Microsoft C++ Build Tools with Desktop development with C++ and WebView2. macOS needs Xcode command line tools; Linux needs the distribution's WebKitGTK and other Tauri development libraries. Bluetooth adapter support is a separate requirement from building the UI.

```sh
npm ci
npm run tauri dev
```

Pair your headphones in the operating system's Bluetooth settings, power them on, then use **Find headphones** in the desktop app. A paired entry is not proof that the Sony control service is connected. Other control applications or phone connections can affect service availability.

To explore only the frontend in a browser:

```sh
npm run dev
```

Open `http://127.0.0.1:1420` and choose **Explore studio preview**. Browser mode cannot access the native Bluetooth backend.

## Build and verify

```sh
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Build native packages on each target OS. A Windows build does not validate macOS or Linux support. Installers are produced under `src-tauri/target/release/bundle/` when packaging succeeds.

Browser integration tests use [Playwright](https://playwright.dev/docs/intro). Install its Chromium browser once, then run:

```sh
npx playwright install chromium
npm run test:e2e
```

Alternatively, use an installed browser channel; PowerShell example:

```powershell
$env:PLAYWRIGHT_CHANNEL = 'msedge'
npm run test:e2e
```

The test suite starts/reuses Vite at port 1420. Screenshots, traces and JSON results stay in ignored `tests/artifacts/`. Tests use reduced motion for deterministic interaction checks; this does not certify animation frame rates.

## Validation scope

| Area | Evidence as of 2026-11-02 |
| --- | --- |
| Browser UI | 28 Playwright integration tests passed using Edge Chromium, including navigation, automatic listening, VLC setup and preview isolation |
| Backend and companion | 59 Rust unit tests and 8 browser-companion tests passed; five explicit hardware/environment smoke tests are ignored by default |
| Preview isolation | First launch contains no sample connection; explicit preview enter/exit and reload behavior tested |
| EQ interactions | Keyboard edit, per-model state, reset, saved preset persistence/deletion tested in preview |
| Appearance/accessibility | Theme persistence, navigation, reduced-motion setting, narrow-view page overflow and separated mobile hero layout tested; full screen-reader/contrast audit remains open |
| Native IPC behavior | Three browser tests use explicit mock native fixtures to verify capability gating, pending/rejected controls retaining confirmed state, and newer disconnect events winning over delayed connection responses; these are not hardware tests |
| Artwork | Distinct official product renders load with actual image dimensions for XM6, XM5 and CH720N |
| Windows XM6 | V2 protocol-info, model WH-1000XM6, firmware 3.1.5, battery, NC/ASM inquiries, and a 500 Hz EQ SET were read from the device. EQ changed from 5 to 6 and restored to 5 by readback. Inquiry `0x19` read/write handling passes project checks and is hardware-confirmed on the corrected build: the user confirms all three tested NC/ASM modes and physical NC/AMB button changes are reflected correctly. |
| XM5 / CH720N hardware | Not yet tested; exact firmware versions still need recording |
| macOS / Linux hardware | Not yet tested |
| Complete mobile-app parity | Not achieved; see feature matrix |

A one-session Windows process-tree sample is recorded in [resource measurements](docs/research/resource-measurements.md). It measured the active WebView2 tree separately from the lightweight Rust tray process; active-window memory still needs broader profiling.

Use `scripts/measure-native.ps1 -RootProcessId <id> -Phase window-open` on Windows for a bounded whole-process-tree resource sample. Repeat with the same root process after closing to tray and reopening. It reports private bytes, working set and CPU change for stable processes; shared working-set pages and changing subprocess lifetimes limit interpretation. No minimum-memory claim has been made.

## Research and implementation boundaries

- [Follow media setup and validation](docs/media-following.md): automatic Standard/Cinema rules, browser companion pairing, and the optional local VLC connection.

- [Original-app evidence report](docs/research/sony-sound-connect-evidence.md): artifact identity, evidence levels and observed framing.
- [Feature parity matrix](docs/research/feature-parity-matrix.md): official model baselines and required end-to-end checks.
- [V2 readout specification](docs/research/v2-readout-spec.json): independently summarized payload layouts and private original-artifact evidence references. See the inquiry `0x19` schema for the corrected XM6 noise-control confirmation; EQ hardware confirmation currently covers one band on one XM6.
- [NC/ASM inquiry 0x19 schema](docs/research/v2-ncasm-19-schema.md): byte offsets, enum values, notification dispatch, SET serialization and XM6 hardware-confirmed behavior.
- [Design and validation plan](docs/research/desktop-design-and-validation.md): architecture and hardware validation sequence.
- [Implementation registry](docs/implementation-registry.md): ownership, accepted review conditions and IPC contract.

Requests are not considered successful merely because a transport ACK arrives. A real setting needs a matched response or readback. Unknown setting schemas, firmware updates and factory reset are not sent speculatively. Phone-sensor automation, linked services, cloud backup, spatial playback and OS codec support have unresolved dependencies.

The reference [sony-device-center](https://github.com/marconvcm/sony-device-center) was inspected as a secondary implementation reference and cross-checked against static original-artifact evidence. Its source was not copied. Android APK/APKM inputs, decompiled content and raw research captures are private and excluded from version control. Optional scripts under `scripts/` support read-only local REA inspection and candidate-reference discovery; candidate results require independent validation and are not protocol proof by themselves.

## License and product artwork

Independently authored project code is distributed under the [MIT license](LICENSE), copyright © 2026 Om Patil. That code license does **not** grant rights to Sony product photography, renders, logos, trademarks, original application packages or decompiled material.

The UI's headphone renders originate from Sony's product materials. See the [asset attribution manifest](public/headphones/SOURCES.md) for the verified product pages, download URLs and hashes. Those assets remain subject to their owners' rights and are not represented as MIT-licensed.
