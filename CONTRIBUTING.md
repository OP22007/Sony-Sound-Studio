# Contributing

Open source contributions are welcome: bug fixes, interface and accessibility improvements, device compatibility testing, platform support and documentation all help.

For a substantial feature, open an issue to discuss the scope first. For a focused fix, fork the repository, create a branch and submit a pull request against `main`. Use a descriptive branch name, for example `fix/linux-discovery`. Maintainer development branches may use `codex/`.

## Setup

Install Node.js 22+, Rust 1.90+ and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Linux development needs WebKitGTK 4.1, GTK/AppIndicator and the other Tauri system libraries; actual Bluetooth testing also needs BlueZ tools. Windows needs the C++ desktop build tools and WebView2. macOS needs Xcode command line tools.

```sh
npm ci
npm run tauri dev
```

For UI work without headphones, run `npm run dev` and enter Studio preview. Preview state must remain isolated from native device commands.

## Verify your change

```sh
npm run build
node --test extensions/media-mode/test/extension.test.cjs
npx playwright install chromium
npm run test:e2e
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
```

Linux CI uses `npx playwright install --with-deps chromium`. To use installed Edge locally on Windows, set `$env:PLAYWRIGHT_CHANNEL = 'msedge'` in PowerShell before running the browser tests. The test server needs port 1420.

Build the desktop executable with `npm run tauri build -- --no-bundle` or build installers with `npm run tauri build`. Building on one OS does not establish compatibility on another.

## Pull requests and CI

- Describe the problem, resulting behavior and relevant verification. Attach screenshots for visible changes and explain any hardware limitations.
- Branch pushes and pull requests run frontend/browser checks and native Rust tests/builds on Windows, macOS and Linux. The final `CI passed` check succeeds only when all jobs pass.
- Keep PRs up to date with `main` and resolve review conversations before merging. Submit changes through PRs; the repository protection policy is defined in [the branch protection configuration](.github/branch-protection.json).
- CI has read-only repository access and does not publish branch builds as releases. Fork PRs do not receive repository secrets. GitHub may require a maintainer to approve a first-time contributor's workflow run.
- Hardware smoke tests are explicitly ignored by default. Only run one after reading its requirements and understanding the commands it sends. Report hardware results separately from mocked/browser checks.

## Device protocol changes

Include independently summarized evidence for capability discovery, value decoding, supported ranges and read/write confirmation. State the exact headphone model, firmware and operating system tested. Do not treat a transport ACK as a successful setting change, invent capabilities from model names, or enable controls whose schema is unvalidated.

Preserve serialized device commands, bounded timeouts, cancellation, device readback and explicit pending/error state. Never introduce speculative firmware, reset or unsupported setting commands. See [device control status](docs/research/device-controls-status.md) and the [evidence report](docs/research/sony-sound-connect-evidence.md).

## Privacy and assets

Do not commit pairing keys, passwords, device addresses, personal media metadata, environment secrets, APK/APKM files, decompiled Sony code or private captures. Review diagnostic attachments before sharing. Link to independently summarized findings instead of uploading original app material.

Project code uses the [MIT license](LICENSE). Sony artwork and trademarks retain their owners' rights; follow the [asset attribution manifest](public/headphones/SOURCES.md). Contributions must be your own work or carry a compatible license and attribution.
