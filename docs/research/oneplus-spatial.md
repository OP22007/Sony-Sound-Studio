# OnePlus spatial audio in HeyMelody

## Findings

HeyMelody exposes two headset-side spatial control paths. The newer path selects one of the model's advertised spatial types; the older path only switches spatial audio off or on. A separate Android `Spatializer` path exists for phone-rendered audio in other app packages. The HeyMelody app explicitly disables that phone path, so it is not an earbud protocol command and should not be treated as a HeyMelody capability.

The original UI labels are:

| Type | UI meaning |
|---:|---|
| 0 | Off / closed |
| 1 | Fixed when the phone route or new headset UI is selected; otherwise the UI calls it Open |
| 2 | Follow head |

The legacy headset view model only sends the feature-27 boolean, so it cannot express type 2.

## Headset commands and gates

| Path | Gate | GET | SET | Readback |
|---|---|---|---|---|
| Typed spatial mode | Live capability bit 47 advertises command 1058 (`0x0422`) and command 298 (`0x012A`). The UI selects this view model when live capabilities include 298. | GET 298 with an empty body. Response command is `0x812A`; after the status byte, one byte is the currently selected spatial type. | SET 1058 with a one-byte body: `[type]`. | GET 298 again and use its selected type byte. The source confirms the GET reply shape; it does not establish that every firmware echoes a SET or that the UI automatically issues a read after SET. |
| Legacy on/off | Static `function.spatialTypes` is present, and the typed-command route is not selected. The aggregate feature query requires command 269 (`0x010D`). SET command 1027 (`0x0403`) is guarded by capability bit 7. | GET 269 body is `[count, featureId...]`; for spatial only, `[1, 27]`. Response carries status, then count and `(featureId, value)` pairs. Feature 27 is the spatial on/off state. | SET 1027 body `[27, 0]` to disable or `[27, 1]` to enable. | GET 269 again and confirm the returned feature-27 state. |

The typed reply parser rejects a non-success status or a body without a value byte, then publishes the value as the headset's selected spatial type. Type values are unsigned in the wire format; keep them distinct from a boolean. The older feature-switch path returns a boolean and must not be used for typed selection.

## Modern GameSound profiles

The modern GameSound profile selector is separate from the legacy low-latency `gameMode` feature switch. It is enabled only when the static model entry lists GameSound types and live capability bit 49 advertises the typed GET/SET commands. The current model allowlist has types 0 and 3 for product IDs `067414`, `067814`, `066414`, `066814`, and `067014`.

| Operation | Command | Payload / response |
|---|---|---|
| GET current profile and live options | 299 (`0x012B`) | Empty request. Status-prefixed reply body is `[currentType, count, supportedType...]`. The adapter requires a bounded exact length, nonzero count, unique known types, and a current type in the intersection of model and live options. |
| SET selected profile | 1059 (`0x0423`) | `[type, 1]`. In particular, Off is type 0 and is selected with `[0, 1]`; it is not an enable=false command. |
| Confirm SET | GET 299 (`0x012B`) | Up to three fresh GETs are checked for the requested profile, with cancellation-aware short waits. A malformed or mismatched reply leaves the last confirmed UI value intact; no rollback SET is sent. |

The UI labels are `Off` for type 0 and `Sound effects for shooting games` for type 3. GameSound and `gameMode` remain independent controls: selecting Off in this typed profile path does not promote, rewrite, or disable the separately advertised feature-switch entry. These paths are covered by native fixtures, not hardware validation.

## Static OnePlus model registry

The APK's compatibility allowlist has 15 OnePlus products with `spatialTypes`. `spatialDescriptionType` is listed as registry metadata without assigning undocumented meaning to its numeric value. A dash means `spatialHiResMutex` is absent.

| Product | Product ID | `spatialTypes` | `spatialDescriptionType` | `spatialHiResMutex` |
|---|---|---|---:|---:|
| OnePlus Flow Buds | `067414` | 0, 1 | 4 | — |
| OnePlus Buds Pro 4 | `067814` | 0, 1, 2 | 1 | 1 |
| OnePlus Buds Pro 3 | `064014` | 0, 1, 2 | 1 | 1 |
| OnePlus Buds Clip | `06E410` | 0, 1 | 4 | 1 |
| OnePlus Buds 4 | `065414` | 0, 1 | 4 | 1 |
| OnePlus Buds 3 | `063C14` | 0, 1 | 4 | — |
| OnePlus Buds Ace 3 | `066414` | 0, 1 | 4 | 1 |
| OnePlus Buds Ace 2 | `064C14` | 0, 1 | 4 | — |
| OnePlus Nord Buds 4 Pro | `066814` | 0, 1 | 4 | 1 |
| OnePlus Nord Buds 4 | `067014` | 0, 1 | 4 | — |
| OnePlus Nord Buds 3 Pro | `064414` | 0, 1 | 4 | — |
| OnePlus Nord Buds 3 | `065014` | 0, 1 | 4 | — |
| OnePlus Buds 3V | `066054` | 0, 1 | 4 | — |
| OnePlus Nord Buds 3r | `065854` | 0, 1 | 4 | — |
| OnePlus Bullets Wireless Z3 | `051014` | 0, 1 | 4 | — |

These are static allowlist entries, not proof that a currently connected headset returns the corresponding live capability bits. Gate controls on both the model entry and the live command capability. `spatialTypes` advertises available UI choices; it does not itself prove which transport path is enabled.

## Phone-rendered path

The related phone path uses Android's `AudioManager` and `Spatializer` APIs rather than Bluetooth headset GET/SET commands. Its code checks API level 33 or later, spatializer availability, package identity, and whether headset spatial audio takes priority. The app-package check explicitly returns false for HeyMelody. Therefore these controls are excluded from the HeyMelody headset adapter, even though the same APK source contains their implementation for other product/app contexts.

## Related GameSound mode values

The GameSound UI uses type 0 for **Off**, type 1 for **Peacekeeper Elite League sound effects**, and type 3 for **Sound effects for shooting games**. In the type-selection radio path, the selected type is passed to the GameSound setter with `enable=true`, so selecting Off uses SET 1059 with `[0, 1]`. This differs from turning off the separate GameSound main switch, which uses the boolean feature-switch path. Treat the two controls as separate operations.

## Evidence trail

Findings were traced from the local HeyMelody APK and decoded allowlist, including `heymelody-model-config.private.json`, `hey-inspect_poll_l.json`, `hey-poll_java.json`, `hey-com_oplus_melody_btsdk_protocol_commands_l.json`, and the spatial/GameSound UI and repository class captures under `.research/evidence/`. The spatial captures identify the 269 feature-27 query builder, 1058 typed setter, 298 typed getter, parser dispatch `0x812A`, view-model route selection, and Android phone-rendered gates. GameSound captures include the type-selection path and bundled resource labels. The private configuration and raw class captures remain research evidence and are not reproduced here.

No hardware writes or physical-device validation were performed. These are static APK findings; firmware response behavior still needs a user-operated hardware check.
