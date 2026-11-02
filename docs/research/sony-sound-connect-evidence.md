# Sony Sound Connect: evidence report

Research date: 2026-10-31. Target artifact: Android Sound Connect 13.3.0. This report describes independently observed behavior; it is not a claim of desktop feature parity.

## What the app does

Sound Connect discovers compatible headphones, obtains their supported functions, reads current settings, sends control requests and receives state changes. Some functions run on the headphones; others involve phone sensors, operating-system audio facilities, Sony services or third-party services. Reproducing the visible controls alone would not reproduce those dependencies.

Sony documents the mobile app and says there is no PC version; settings configured on mobile can carry over when headphones are used with a computer. See [Sony's app support page](https://www.sony.com/electronics/support/software/00269130). The model-specific baseline is in the [feature matrix](feature-parity-matrix.md).

## Evidence levels

| Label | Meaning | Current coverage |
| --- | --- | --- |
| Documented | Official Sony documentation describes a feature | Model feature baselines |
| Static | Observed in the supplied APK using REA/JADX | Transport, framing, identifiers, request handling |
| Captured | Observed in a timestamped runtime trace | None yet |
| Hardware-tested | Independently implemented and verified on a named model/firmware/OS | None yet |
| Unresolved | Evidence is insufficient | Full payloads, model negotiation, platform parity |

Static evidence establishes what code contains, not which path a particular headphone executes. No headphone writes, resets or firmware updates were performed in this research.

## Artifact identity and method

- Supplied bundle: `com.sony.songpal.mdr_13.3.0-130305109_2arch_7dpi_24lang_a52dbe8e5d5c9ac22edf03014be365e7_apkmirror.com.apkm`
- Bundle SHA-256: `7aa59ffa1037e126791290f67d3b22309b39f7233e6851f714de825ca4dbd7a7`
- Extracted base APK SHA-256: `58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`
- Metadata: package `com.sony.songpal.mdr`, version 13.3.0, versionCode 130305109; Android 11 minimum; two ARM ABIs; 24 languages.
- Declared v2/v3 signer certificate SHA-256: `4276919680f4aba6d41924031c20ac2624feea5e8ec68a36529b2acb664d911e`. This is an inspection result, **not cryptographic verification of package authenticity**.
- Analysis used REA 6.8.0, Android bridge 7, JADX headless 0.7.1 and Java 21.0.13. The APK was inspected statically, not executed.

Private raw evidence exports are under `.research/evidence/`, excluded from version control together with APK/APKM inputs. They contain decompiled material and are local audit inputs, not product source. Evidence IDs below identify records inside those exports. Preserve the exports locally to reproduce this audit.

## Transport and request lifecycle

`ep0.c.a` obtains a remote Bluetooth device, cancels discovery, creates an RFCOMM socket with a supplied UUID, connects and starts its connection handler. This proves a classic Bluetooth control path exists. It does not establish which path each target model chooses.

Two service UUID providers return:

- `96CC203E-5068-46AD-B32D-E316F5E069BA`
- `956C7B26-D49A-4BA8-B03F-B17D393CB6E2`

`lm0.c.n` serializes sends, uses a sequence byte, waits for an ACK where required, retries with the same sequence and closes the connection when retries are exhausted. Timeout and retry values come from callers; their defaults and sequence progression remain unverified.

`uv.c.a` and `wv.c.a` register a response listener, send a request, wait up to 30 seconds and remove the listener in a finally path. A transport ACK therefore must not be treated as proof that a setting change succeeded. Desktop operations need matched responses/readback and asynchronous cancellation.

Other Airoha SPP paths and BR/EDR versus LE transport enum values exist. Their presence does not assign a chipset, Bluetooth mode or service to XM6, XM5 or CH720N.

## Wire format

The encoder `qm0.b.e` constructs this unescaped body:

```text
type: u8 | sequence: u8 | payload_length: u32 big-endian | payload | checksum: u8
```

Checksum is the sum of the type, sequence, four length bytes and payload bytes modulo 256. It excludes delimiters and is computed before escaping. The escaped body is surrounded by start `0x3e` and end `0x3c`.

| Body byte | Wire encoding |
| --- | --- |
| `0x3c` | `0x3d 0x2c` |
| `0x3d` | `0x3d 0x2d` |
| `0x3e` | `0x3d 0x2e` |

The inspected unescape helper rejects a dangling escape. The encoder splits framed data into multiple transport writes when it exceeds the transport write size. Such write splitting is distinct from the protocol's large-data message type. A desktop decoder must tolerate arbitrary read boundaries; one socket read cannot be assumed to contain one frame.

Observed data types include ACK `0x01`, MDR table 1 `0x0c`, MDR table 2 `0x0e`, no-ACK shot variants `0x1c`/`0x1e`, and large MDR data `0x2c`. Normal table routes and the table-1 restriction on the inspected large-data path were separately observed.

## Command identifiers

The V2 table-1 enum contains the following identifiers. These are **not complete sendable payloads**, and V2 must not be assumed for every device.

| Family | Observed identifiers, hexadecimal |
| --- | --- |
| Protocol, capability, device info, supported functions | GET/RET pairs `00/01`, `02/03`, `04/05`, `06/07` |
| EQ | Capability `50/51`; status `52/53`, notification `55`; parameter GET/RET/SET/NOTIFY `56/57/58/59`; extended GET/RET `5a/5b` |
| Noise cancellation / ambient | Capability `60/61`; status GET/RET/SET/NOTIFY `62/63/64/65`; parameters `66/67/68/69` |
| Power | Capability GET `20`; status GET `22`; parameter GET `26` |
| Update | Capability `30/31`; status `32..35`; parameters `36..39`; transfer `3e`; message notification `3f` |

Manifest declarations include Bluetooth, Internet, camera, location and background location, foreground services and notifications. Update services, voice-guidance/language activities, safe-listening and backup flows are present. Declarations do not prove that every permission is used for every model or that a desktop replacement needs them.

## Audit index

Paths below are relative to the repository root. Method names are artifact-specific and may be obfuscated.

| Finding | Private export | Method / evidence ID |
| --- | --- | --- |
| RFCOMM connection | `.research/evidence/rfcomm-callers.json` | `ep0.c.a`; `ev_17334590cee7efade52e38b428629b7d23605f74cc3a1123db65ab06e1bd9bd7` |
| UUID provider A | `.research/evidence/send-path-and-package.json` | `lm0.i$a.a`; `ev_90a3a3627af7c375ef9979f5c4bfba467e446881110e114236203fb3d36d112e` |
| UUID provider B | same | `lm0.i$b.a`; `ev_f9851123be526b36d42e8b2d6bf67a1066902e783cd31d0c932eb34f2692c90d` |
| ACK and retry | same | `lm0.c.n`; `ev_e62cecf830da6cb4d1b475b10a8890518aa5ae2cf0f399c5748596733d228381` |
| Matched response wait | `.research/evidence/frame-encoding.json` | `uv.c.a`; `ev_d7b6108902b948b48ebee95410c5c68fc177b8e06e33ea16c3f6503def33559b` |
| Encoder and splitting | `.research/evidence/wire-framing.json` | `qm0.b.e`; `ev_152997e79175afb797cf0cda23cacac3afd6caf034cd3850d776a6b1b8ad2b23` |
| Length endian | `.research/evidence/wire-details.json` | `util.e.i`; `ev_8357f8fd2fed324e4899b46ff1f802e025b9ad0afc414d9a56daa18f2782c15b` |
| Checksum addition | `.research/evidence/checksum-and-escaping.json` | `cp0.f.update`; `ev_9c5daf6fce5a8fa4f1dd8bafa9259bc414119ea7d527713dfebd9cf87d9805c0` |
| Escape mapping | same | `qm0.e` initializer; `ev_3e7ff648fba37211c73edce4b5589e2669f229dba6d89ef317e019ee0a838be2` |
| Data types | `.research/evidence/protocol-enums.json` | `DataType` initializer; `ev_8bff419b49555e0f61e954dfabcd8294de764392db16f52f99493765b239a4fb` |
| V2 commands | same | `Command` initializer; `ev_d9cccf8ed979ec857809751ef6115ffd04891654386e70e3bfd9b123bdb4f518` |

## Reference project assessment

The [reference repository](https://github.com/marconvcm/sony-device-center) was reviewed at commit `0bf98223c1ba402dd6c5136c21e6ab35482e4390` as a secondary source, not copied or treated as authoritative.

Its README reports partial XM6 support and unresolved noise-mode readback. Its [capability discovery](https://github.com/marconvcm/sony-device-center/blob/0bf98223c1ba402dd6c5136c21e6ab35482e4390/libs/sony-protocol/src/CapabilityDiscovery.cpp) uses predefined capabilities for known models. Its [frame codec](https://github.com/marconvcm/sony-device-center/blob/0bf98223c1ba402dd6c5136c21e6ab35482e4390/libs/sony-protocol/src/FrameCodec.cpp) rejects oversized frames, whereas the inspected Sony encoder splits transport writes. These differences justify independent capability negotiation and protocol tests.

## What remains to prove

Payload schemas, model-specific capability replies, UUID selection, sequence transitions, response matching, BLE characteristics and handshakes remain unresolved. Firmware signing, recovery and chipset-specific update behavior need separate research. Phone-sensor automation, accounts/backup, service integrations, spatial playback and head tracking require independent desktop feasibility checks.

Next evidence must come from timestamped captures on the actual hardware. See the [validation plan](desktop-design-and-validation.md). No complete desktop app has been implemented yet.
