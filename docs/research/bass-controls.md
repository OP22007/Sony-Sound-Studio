# HeyMelody Bass Engine / BassWave evidence

Status: static research of HeyMelody 117.6.6 only. No headphones were queried or written. The app's API uses the name “Bass Engine”; “BassWave” is the desktop product label.

## Model allowlist

The bundled `compatWhiteList.function.bassEngineSupport` flag is `1` for these product IDs. The original poll builder adds feature-switch ID `29` only when this flag is enabled.

| Product ID | Model |
| --- | --- |
| `067414` | OnePlus Flow Buds |
| `067814` | OnePlus Buds Pro 4 |
| `064014` | OnePlus Buds Pro 3 |
| `062014` | OnePlus Buds Pro 2 |
| `063414` | OnePlus Buds Pro 2R |
| `063014` | OnePlus Buds Pro 2 (light edition) |
| `065414` | OnePlus Buds 4 |
| `063C14` | OnePlus Buds 3 |
| `066414` | OnePlus Buds Ace 3 |
| `064C14` | OnePlus Buds Ace 2 |
| `062814` | OnePlus Buds Ace |
| `066814` | OnePlus Nord Buds 4 Pro |
| `067014` | OnePlus Nord Buds 4 |
| `064414` | OnePlus Nord Buds 3 Pro |
| `065014` | OnePlus Nord Buds 3 |
| `062414` | OnePlus Nord Buds 2 |
| `051014` | OnePlus Bullets Wireless Z3 |
| `050C14` | OnePlus BulletsWireless Z2 ANC |

Absent or zero `bassEngineSupport` entries are not allowlisted. The static model flag is separate from live command capability.

## Numeric level

Original `HeadsetCoreService.F0` sends command **1051 (`0x041B`)**, checking command support first. Its body is exactly three bytes in this order: `[minimum, maximum, current]`. The values are cast to Java bytes. `BassEngineInfo` exposes each as an `int`; parser `commands.g.c(0, data)` reads the three bytes as signed Java bytes at offsets 0, 1 and 2.

Thus the wire range is signed 8-bit (`-128..127`), but the allowed UI range and step size are device supplied. The APK evidence does not establish a universal range, step or direction/polarity rule. Do not derive “boost” direction or clamp to `[-6,+6]` from custom EQ behavior.

The command capability table `b8.a.f14745a` groups command 1051 with command 292 at bit index **38**. `HeadsetCoreService.D` issues empty GET command **292 (`0x0124`)** after checking command 292 support. In the `commands.l.M` response dispatcher, response `0x8124` maps to the BassEngine GET handler: it checks status byte 0 and then calls `g.c(1, data)`. The GET response is therefore `[status, signed min, signed max, signed current]`. A separate inbound dispatcher case `1288` (`0x0508`) calls `g.c(0, data)` and delivers the statusless three-byte value as an update/event; this is distinct from GET `0x8124`.

The original UI accepts SET status zero as success; the captured SET path does not issue an immediate follow-up GET. The protocol has a verified GET route for reading the current range and value, so a desktop can confirm post-SET application by polling GET `0x0124` and comparing the current value, while treating the SET reply alone as acceptance.

## Enable state

Enable is a separate feature-switch state from the numeric level. For allowlisted models, `commands.l.m` includes feature ID **29** in the GET feature list. GET command **269 (`0x010D`)** carries a count followed by feature IDs; its result is a count followed by `(feature ID, value)` records. The general feature-switch interface SET is command **1027 (`0x0403`)** with `[feature ID, 0-or-1]`; for Bass Engine the feature ID is 29. The original service follows a SET with GET 269 when requesting current state. SET capability is bit index **7**. The status value represents enabled/disabled; this does not specify any numeric Bass Engine level.

## Product context

Sony's WH-CH720N help guide documents the Sony Headphones Connect app as supporting an “Equalizer/CLEAR BASS” setting and custom equalizer settings ([Sony WH-CH720N guide](https://helpguide.sony.net/mdr/2966/v1/en/contents/TP1000776844.html)). That is an official example of Clear Bass being presented with the EQ controls. HeyMelody's Bass Engine is represented in the inspected API as a separate enable feature and a numeric range/current value. The sources do not establish that Sony Clear Bass and OnePlus Bass Engine use equivalent processing or control semantics.

## Evidence used

- `hey-b8_a_summary.json`: command capability index-to-command grouping.
- `hey-poll_java.json`: feature ID 29 added when `bassEngineSupport` is true; GET 269 encoding.
- `eq-headsetservice-full.txt` and `hey-poll_dispatch_full.json`: `F0` SET 1051 body; empty GET 292 in `D`; response `0x8124` status check and parser offset 1; separate `0x0508` update parsed at offset 0.
- `hey-com_oplus_melody_btsdk_protocol_commands_g.json`: signed-byte min/max/current parser.
- `hey-inspect_BassEngineInfo.json`: the API DTO fields.
- `heymelody-model-config.private.json`: model allowlist; private dump is excluded from Git.

These are decompiler observations, not firmware behavior. Device-specific range behavior and actual post-SET device readback still need hardware confirmation.

## Sony Clear Bass research (Sony Sound Connect 13.3.0 APK)

The Sony Headphones Connect/ Sound Connect app protocol has an EQEBB command family. Its inquiry type enum separates `EBB` (`0x01`) from `CUSTOM_EQ` (`0x31`) and `PRESET_EQ` (`0x00`). The capability GET parser accepts the EBB inquiry type; this establishes a protocol category, not that a specific headphone supports it. The custom EQ notification handler parses the custom EQ payload as a counted array of unsigned band steps. The EBB notification parser accepts a three-byte payload `[command, inquiry-type, level]`, with the third byte represented as a signed Java byte and constrained to `>= -127` by the parser. This shows standalone EBB has its own scalar notification path; it does not show that Clear Bass is a field in the custom EQ payload.

The static APK inspection did not identify a supported-model mapping or a CH720N/XM5 device reply proving that inquiry type `EBB` is available on those models. The ordinary EQ support documented by Sony for WH-CH720N does not prove EBB's separate protocol inquiry is supported. The inspected classes also do not establish the EBB parameter GET/SET payload schema, device-supplied range/step, or preservation semantics between EBB writes and the five-band EQ. No live device capability/status/parameter captures are available. Therefore the native implementation must not expose or write Clear Bass for a model based only on this evidence; the missing evidence is model-scoped capability and parameter response data, plus a verified SET/readback path that preserves the current EQ bands.

Evidence: Sony APK SHA-256 `58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`; JADX classes `EqEbbInquiredType`, `jn0.a`/`jn0.b` (capability inquiry), `jn0.f` (parameter notifications), `jn0.l` (EBB notification), `jn0.g` (custom EQ notification), and `kn0.a` (counted EQ bands). This is static decompiler evidence only, not runtime or hardware verification.
