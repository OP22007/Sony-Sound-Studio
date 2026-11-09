# OnePlus: HeyMelody protocol evidence

Status: experimental implementation for **all 32 OnePlus product IDs / 30 model names in the supplied HeyMelody catalog**. The owner has observed a successful Windows Nord Buds 3 Pro control connection, product/capability replies and a physical NC change. Final noise-profile readback confirmation remains under investigation; other OnePlus products remain hardware-unvalidated. Static evidence, fixture checks and these preliminary user observations are separate milestones. See the [model list and compatibility rules](oneplus-models.md).

## Research input and method

Original HeyMelody Android package, version 117.6.6 / 117006006:

- APKM SHA-256: `c646b6f6bf86947a5c637af4eae52861dc324b974e55075e118bd158fb0427b1`
- Extracted base APK SHA-256: `2a592c98720444988abd9cde852dc56e4e6ed6792dd2aa3257a47731dd865f9a`

REA extracted the supplied APKM. Static analysis used the installed JADX headless engine. The package's bundled model configuration was decoded locally: the signing certificate identifies the AES-GCM key material used by its own configuration loader; the authenticated plaintext contains gzip JSON. Original classes `com.oplus.melody.common.util.U` and `whitelist.a.n` establish this path. Original packages, keys, configuration dumps and decompiled sources are excluded from Git. Implementation is independently authored from the observed schemas.

The bundled `compatWhiteList` contains 32 OnePlus entries with Classic Bluetooth support, including regional IDs sharing a model name and seven child-name aliases. For example, **OnePlus Nord Buds 3 Pro** has product ID `064414`, `supportSpp: true`, and configured service UUID `0000079A-D102-11E1-9B23-00025B00A5A5`. This UUID is not BLE-exclusive: Windows also lists it as a Classic Bluetooth service for the paired device. The desktop embeds independently assembled model facts, not the original configuration file. Static inclusion flags are not live capability proof.

## Connection and framing

| Item | Observed evidence |
| --- | --- |
| Classic Bluetooth RFCOMM service | Per-model configured UUID: `0000079A-D102-11E1-9B23-00025B00A5A5` for 18 catalog entries, `00001107-D102-11E1-9B23-00025B00A5A5` for 14 legacy entries. `A8.d` (`BaseBRDevice`) also defines the legacy default. |
| BLE service and characteristics | `y8.i` (`GattDevice`): service `0000079A…`, write `0100079A…`, notify `0200079A…`; BLE is not implemented here |
| Inner packet | `p8.a`: command LE u16, sequence u8, data length LE u16, data |
| Response correlation | `p8.b`: command OR `0x8000`, same sequence; sequence wraps after 255 |
| Outer frame | `F8.b`, `G8.b`: `AA`, unsigned 7-bit varint length, flags, reserved zero, optional fragment index, inner bytes/chunk |
| Fragment flags | 0 single, 1 first, 2 middle, 3 last; fragment index starts at 0 |

Outer length includes flags, reserved byte and optional fragment index. No checksum was observed in these wrappers. The adapter bounds packets, queues and fragmentation and starts with frames at most 20 bytes. OnePlus frames bypass the Sony decoder and Sony transport acknowledgements entirely.

### Windows code 10049 investigation

The first adapter incorrectly used the legacy `1107` UUID for every OnePlus model. A user connection attempt on Nord Buds 3 Pro returned Windows error 10049. Read-only Windows PnP inspection of that same paired device showed the configured `079A` service, audio services and other vendor services; it did not show `1107`. This identifies a routing mismatch in the implementation, without implying that a corrected connection has already been hardware-tested.

The registry now preserves each entry's configured service UUID, and both Windows and Linux use it. Regression tests cover the 18/14 endpoint split, aliases and Windows GUID conversion. Unknown OnePlus model names try the two known settings-service UUIDs; an unrecognized product ID still enables no settings. The adapter never probes unrelated firmware/OTA, audio or other vendor endpoints.

Direct original-app confirmation: `z8.b.<init>(Context, DeviceInfo)` resolves `SupportDeviceConfig` using product ID and device name, reads `getUuid()` and passes that configured UUID to `o(UUID)`. It uses `A8.d.f312A` (`1107`) only when the configuration UUID is empty. `z8.b.o(UUID)` stores the provided value in the BR client's `z8.a.f286d` before starting its handler. This establishes that `079A` is an intentionally configured BR/RFCOMM service, not merely a BLE service inferred from its spelling.

Microsoft documents error 10049 as an invalid Bluetooth channel or device address; when port is zero, Winsock resolves the channel using `serviceClassId`. The app now reports a settings-service resolution failure and attempted UUIDs instead of assuming the headphones need re-pairing. [Microsoft Bluetooth connect documentation](https://learn.microsoft.com/en-us/windows/win32/bluetooth/bluetooth-and-connect).

## Identity, capabilities and readouts

Commands below are hexadecimal. Every normal response begins with status `00` for success.

| Command | Request / response body after status | Source |
| --- | --- | --- |
| `0103` product identity | Empty / three little-endian product-ID bytes; e.g. `14 44 06` identifies `064414` | Command dispatcher and model configuration |
| `0100` capabilities | Empty / bitmap, least significant bit first in each byte | `b8.a`, `commands.l.G` |
| `0105` firmware | Empty / count, UTF-8 comma-separated device-type/version-type/version triples | `commands.g.m`, `VersionInfo` |
| `0106` battery | Empty / count, repeated device-type and packed level/charging byte | `commands.g.d`, `BatteryInfo` |

Firmware and battery reads require capability bits 0 and 1 respectively. Battery level uses the low seven bits; charging uses bit 7. The desktop retains raw percent and derives its summary from the minimum reported earbud level; the phone can round levels using a configured radix. Per-component records are retained as metadata. A model name alone never enables a setting.

## Noise control

Sources: `CurrentNoiseModeInfo`, `NoiseReductionInfo`, `HeadsetCoreService.I0`, `commands.l.h/v/M`, `commands.j.b/c` and the target model's `noiseReductionMode` map.

- Capability bit 8 advertises this command family.
- GET `010C [02 01]`: supported profiles; response body `[02 01 mask…]`.
- GET `010C [01 01]`: current profile; response body `[01 type bitmap…]`, with current-mode bitmap types 1 and 5 established by `CurrentNoiseModeInfo`.
- SET `0404 [01 01 one-hot-mask…]`: request a profile.

| Protocol bit index | Target configuration meaning |
| --- | --- |
| 0 / 3 | Off / alternate Off |
| 1 | Noise cancelling |
| 4 | Max / deep noise cancelling |
| 5 | Moderate noise cancelling |
| 6 | Mild noise cancelling |
| 7 | Smart / adaptive noise cancelling |
| 2 / 8 | Transparency / alternate Transparency |

This table is a **Nord Buds 3 Pro example**, not a universal index map. Nord Buds 2 assigns index 0 to noise cancelling and 1 to Off; the original Buds Pro assigns index 1 to Transparency. The registry stores separate mappings for each product. The UI includes only configured profiles returned in the live supported mask and requires an unambiguous current profile. SET success is acceptance only: a subsequent current-profile GET must match. Mismatch never triggers an automatic restoration command.

### Current-mode bitmap correction

A user reported that Transparency changed physically to noise cancelling while the first adapter returned an ambiguous/unsupported-profile error. Their logged valid passive body `01 01 04` identifies Transparency for this product; the rejected GET body was not retained by the first implementation, so the exact failing bytes are not yet known.

The original `CurrentNoiseModeInfo` reads all bitmap bits, supports type 1 **and** type 5, and its `getCurrentNoiseReductionModeIndex()` returns the first set bit. It does not enforce a one-hot mask. The desktop's earlier one-hot/type-1-only assumption was therefore too restrictive. The corrected desktop parser uses the bundled parent/child profile tree to resolve a single category/detail chain. It rejects unrelated categories, sibling strengths and unknown protocol indices. A known detail can identify an advertised ancestor without enabling writes to the unadvertised detail. When a category request retains an advertised child strength, confirmation reports the actual returned profile.

An empty or conflicting transient readback now remains pending for the bounded confirmation window. Rejected readbacks are retained in bounded diagnostics, and failed confirmation errors include the raw body and advertised indices. Fixtures cover these cases; this is not a claim that the user's previously unrecorded body has already been reproduced.

`NotificationCommandManager.j.c/b` parses subscribed battery event 1 and current-noise event 3/action 1 without invoking its explicit reply helper `j.d`. No speculative `8204` acknowledgement is added for these events. The separate support DTO recognizes types 1/3/4; poll method `l.v` chooses `[2,3]`/`[2,4]` for long-press configurations and `[2,1]` otherwise. Those support formats are not conflated with current-mode bitmap type 5.

GET `0200` returns supported notification IDs. SET/register `0201 [event]` subscribes only to advertised battery event 1 and noise event 3. `8201` can contain an initial event snapshot after status. Notification `0204` has `[event, event data…]`, with no leading result byte. Noise notifications contain the same current-profile body. Matched GET replies and notifications are processed in wire order. No polling or synchronization of other physical controls is claimed.

## Equalizer

Sources: `HeadsetCoreService.M0/N0`, `commands.g.b`, `commands.l.d/g/M`, `eb.w.d`, bundled `equalizerMode`, `W9.d` and `eb.q`.

- Bit 10: GET `010F`, body `[mode]`; SET `0406 [mode]`.
- Nord Buds 3 Pro preset example: **3 Balanced, 4 Pure Voice, 5 Bass Plus**. Resource names `melody_ui_equalizer_balanced`, `melody_ui_equalizer_pure_voice` and `melody_ui_equalizer_bass_plus` establish these labels. Other products use their own registry indices and labels.
- Bit 34: GET `0122`, body `[count, profiles…]`.
- Each profile has selected u8, minimum i8, maximum i8, ID u8, name byte-length u8, UTF-8 name, band count u8, then repeated **frequency LE u16 followed immediately by signed dB i8**.

The implementation exposes built-in presets with exact readback and model-specific custom curves. A valid but unlisted current preset remains visible as a disabled observation; it no longer hides the model’s established factory choices. Bounded raw preset diagnostics distinguish missing capabilities, rejected replies and unknown current indices. It does not substitute Sony's ten-band EQ.

Additional labels were verified in `eb.w.d` against the APK's English resources: semantic type 13 is Bold, 15 Gentle, 17 Balanced, 19 Hans Zimmer Soundscape Tuning, 30 Dynaudio featured, 34 Vibrant and 41 Clear & crisp. These semantic types are translated to each product's own protocol indices. Types 19 and 41 are firmware/app-version-dependent configuration entries; the adapter exposes them only when currently reported by the device, rather than inventing a firmware ordering rule. Other conditional entries follow the same rule.

For newer noise profiles, `NoiseReductionItem.updateActionView` associates semantic type 10 with Adaptive. Types 11–13 appear as child profiles in the Buds Pro 4 configuration, but their individual display labels were not established; they retain explicit numeric profile labels. Their ordering alone does not prove a High/Moderate/Low mapping.

Custom SET `0418` uses the same profile layout with an action byte replacing the selected flag. Original UI call sites establish action 1 create/import (`eb.q.i.a`, `Ea.D.onClick`), 2 select/save edits/rename (`eb.q.t`, `eb.q.l.run`, `Hc.s.onClick`), 3 delete (`Ec.m.onClick`). `BluetoothService.a.handleMessage` passes this action unchanged into `HeadsetCoreService.M0`. The UI's create path (`eb.q.i.a`) constructs a fresh `W9.b`, sets name, range `[-6,+6]`, frequencies and dB values, then sends action 1 without setting an ID; `W9.b` inherits the `EqInfo` primitive integer ID field, whose Java default is 0. Thus the observed UI sends ID 0 in that request; no evidence establishes firmware-side ID allocation. The default frequency template in `eb.q.f26918D0` is `[62, 250, 1000, 4000, 8000, 16000]` Hz, used unless model configuration replaces it. Nord Buds 3 Pro has no `customEqFrequency` entry, so this six-band template is the observed UI default for that model. The fragment sets `f26954j0 = 3` and compares list size with `f26954j0 + 1` before/after create, but the list's exact composition is not established; this is not enough evidence for a device profile limit.

The desktop exposes action 2 on an existing, GET-confirmed profile for selection, band edits and renaming. It checks capability bit 34, refreshes the live list before writing, preserves the reported frequency layout and range, sends `0418`, then GETs `0122` and requires exact profile readback. SET acceptance alone does not confirm a save.

Creation uses action 1 with ID 0 and the original app’s model-specific frequency template and range. The device chooses the saved ID; desktop confirmation requires exactly one newly returned profile with the requested name, selection, frequencies, range and values. The capability and a successfully parsed custom-profile reply are required even when the list is empty. The app does not assume a firmware allocation algorithm or device profile limit. Deletion (action 3) is not exposed. All custom writes still require hardware testing. Custom profile selected flags and built-in preset mode are separate state; profile IDs must not be sent as preset modes.

## Wear detection and game mode

Sources: `BluetoothService.a.handleMessage` case 1012, `HeadsetCoreService.j1/S`, `l.m`, `commands.l.M` (`810D`), `commands.g.i`, `FeatureSwitchInfo`, `b8.a`.

- Capability bit 7 advertises SET `0403` (decimal 1027).
- GET `010D` (decimal 269) is in the original base-supported command set. Request: count followed by requested feature IDs.
- Response body: count followed by repeated `[feature ID, status]` pairs.
- SET `0403 [feature ID, 0-or-1]`, followed by GET `010D` for confirmation.
- Feature 4 is wear detection / automatic pause; feature 6 is legacy game mode. Both are included in this model's static configuration. A toggle still requires its live current entry and SET capability.

The model registry chooses game feature 40 for entries with the newer GameSound list; other configured game modes use feature 6. A live capability bit 41 (commands 1059/299 in `b8.a`) also selects the newer feature-40 path for a configured game-mode model, matching the original aggregate GET list builder. The corresponding live entry and SET capability must still be returned. Models without wear/game configuration do not receive these queries. Multipoint is not exposed as a simple boolean. Passive feature-switch notifications have not been established.

## Validation and next hardware check

Fixture tests cover fragmented streams, malformed packets, bounded recovery, product mismatch, capability gating, command/sequence correlation, rejected commands, cancellation, delayed NC application, wire-order notifications, no automatic rollback, battery/firmware validation and EQ profile byte layout. Browser tests cover model-specific controls and IPC routing. These checks cannot establish the physical transport or firmware behavior.

User-operated Windows test:

1. Connect Nord Buds 3 Pro from **Find headphones**; check model, firmware and battery.
2. Change Off, Transparency and advertised NC profiles one at a time; compare the audible result and confirmed selection.
3. Change NC using the earbud gesture; verify the UI follows.
4. In Equalizer, try Balanced, Pure Voice and Bass Plus separately. Wait for confirmation each time and compare the audible result.
5. Use New profile to create a named curve. Change a band, save it, rename it and apply another saved profile. Reconnect and verify the names, frequencies, values and selected flag persist. Existing phone-created profiles should retain their returned band layout.
6. Try any live wear/game toggles. If a step fails, report the app error and transport diagnostics, removing addresses or other personal information. EQ diagnostics retain bounded `010F`/`0122` bytes for diagnosing unexpected firmware layouts.

macOS control connections remain disabled. Linux RFCOMM routing is implemented but hardware-unvalidated. Firmware transfer, reset, multipoint management, gestures, spatial features and custom EQ deletion remain outside this experimental adapter. [BassWave protocol and model support](bass-controls.md) are documented separately.
