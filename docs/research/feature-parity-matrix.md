# Feature parity baseline

Status: implementation update, 2026-11-01. **Windows XM6 identity, firmware, battery, inquiry 0x19 read/write handling, and one EQ band round-trip have been exercised. The 500 Hz EQ step changed from 5 to 6 and was restored to 5 by device readback. Raw inquiry 0x19 GET and NC/ambient notifications were captured. The user confirms the corrected rebuilt app matches all three tested NC/ASM modes and follows physical NC/AMB button changes on WH-1000XM6 firmware 3.1.5.** Complete feature parity is not achieved. “Documented” describes Sony's product, not our implementation. Availability may depend on firmware, region, services and host audio support.

## Official model baselines

Sources: [WH-1000XM6](https://helpguide.sony.net/mdr/2984/v1/en/contents/TP1001856857.html), [WH-1000XM5](https://helpguide.sony.net/mdr/wh1000xm5/v1/en/contents/TP1000534533.html), [WH-CH720N](https://helpguide.sony.net/mdr/2966/v1/en/contents/TP1000776844.html).

| Feature family | XM6 | XM5 | CH720N | Desktop acceptance evidence required |
| --- | --- | --- | --- | --- |
| Battery, identity, firmware | Desktop readbacks hardware-tested | Documented | Documented | XM6 model/firmware and valid battery/charging response observed; mobile comparison, charging transitions and disconnect updates remain |
| Noise cancellation / ambient sound | XM6 inquiry `0x19` GET/SET parser and serializer are implemented and tested. User confirms the corrected Windows build tracks all three tested modes and physical NC/AMB button changes on WH-1000XM6 firmware 3.1.5. Focus-on-voice now uses the inquiry-`0x19` VOICE mode with final readback; its separate SET test is pending. Ambient slider remains gated until its inquiry-`0x19` mode-specific range schema is confirmed. | Documented | Documented | Repeat SET/readback on each supported firmware and model; validate ambient-level capability bounds before exposing slider |
| Listening mode | Capability-driven Standard/BGM/Cinema select uses separate BGM inquiry 03 (E4-only) or 09 (EB-only) and Cinema inquiry 04 state. The BGM distance control exposes My room, Living room, and Cafe using the confirmed room-size parameter map, preserves the on/off state, and requires readback. Mode transitions preserve the selected room size, serialize changes to avoid overlapping modes, require final GET readbacks, and verify rollback after partial failure. Listening-mode selection is withheld for simultaneous E4+EB or EC series until their selector mapping is proven. The captured XM6 reply advertises EB and E5 (not E4/EC), so it should offer Standard/BGM/Cinema and BGM environments after valid reads. Hardware SET/readback remains pending. | Documented | Documented | Confirm exact feature gates and state transitions on XM5/CH720N and every supported firmware |
| Equalizer | XM6 3.1.5: one 500 Hz step SET/readback/restore confirmed | Documented | Documented, including Clear Bass | Verify every band and range, save/apply local profile; validate XM5 and CH720N separately |
| Sound enhancement | DSEE Extreme; desktop now negotiates advertised UPSCALING_AUTO_OFF and supports Off/Auto GET/SET/readback | DSEE Extreme | DSEE | Hardware-test GET/SET/readback on each model and verify interaction with audio connection mode |
| Multipoint / connected devices | Documented | Documented | Documented | List real connections, switch and recover without stale state |
| Adaptive Sound Control | Documented | Documented | Documented | Establish sensor/location dependencies and define a desktop-native equivalent where possible |
| Wear detection / automatic pause | XM6 support reply advertises playback control by wearing/removing (`0xF1`), but this is not the setting's validated GET/SET schema. A separate wear status read is not proof of the pause preference. | Documented | Not established by cited feature list | Map and hardware-test the preference read/write path; separately test actual removal/resume behavior |
| Speak-to-Chat / touch controls | Type 2 Speak-to-Chat configuration now gates on function FC, preserves the auxiliary on/off byte, and requires parameter readback; separate active-effect status is not displayed. Touch controls remain unavailable. | Documented | Not established by cited feature list | Hardware-test Type 2 GET/SET/NTFY and verify physical behavior; map touch-control parameters |
| Quick Access / linked services | Documented | Documented | Not established by cited feature list | Verify service authentication, OS support and actual action execution |
| 360 Reality Audio personalization | Documented | Documented | Documented | Establish calibration/service dependencies and supported desktop playback path |
| Voice guidance / power settings | Support reply advertises guidance variants (`0x41`, `0x42`, `0x44`). The advertised wearing-aware power-off variant (`0x25`) is read-only after valid capability/current-value replies; standard editable auto-power-off (`0x24`) is absent. No setter is inferred for `0x25`. | Documented | Documented | Map and hardware-test guidance options, persistence and downloadable language requirements; establish the `0x25` setter/readback before editing |
| Firmware update | Documented | Documented | Documented | Validate package authenticity, compatibility, interruption/recovery and post-update state |

This is a grouped baseline, not an exhaustive inventory of every screen. Additional documented flows include safe listening and listening history, Auto Play/Auto Switch where available, and model-specific spatial/audio functions. Their exact model/firmware eligibility must be recorded before implementation. Absence from a model's cited list means **unestablished**, not proof of impossibility.

## Cross-cutting parity work

| Workstream | Unresolved dependency | Acceptance condition |
| --- | --- | --- |
| Discovery and connection | Per-model transport and OS Bluetooth APIs | Connect/reconnect on Windows, macOS and Linux; distinguish paired, reachable and connected |
| Platform readiness | Windows XM6 3.1.5 read-only protocol/model/firmware/battery round trip passed; macOS paired discovery exists but control connect fails closed until RFCOMM writes can be safely bounded; Linux BlueZ transport source is untested on hardware | Build/install each target; verify connect/disconnect and lifecycle on a real adapter per OS |
| Capability discovery | Actual replies and schema versions | UI exposes negotiated functions; unknown fields preserved diagnostically; no guessed model capabilities |
| Codec / LE Audio / spatial audio | OS audio stack and hardware | Separate device preference from active codec; demonstrate actual playback behavior on each host |
| Accounts / backup / restoration | Service contracts and authentication | Opt-in authentication and successful restore, or explicit unresolved status |
| Listening history / safe listening | Measurement source and headphone support | Correct units, consent, retention and verified data provenance |
| Adaptive scenes | Phone sensors versus available desktop signals | Document equivalent behavior and gaps; desktop operation requires no phone |
| Accessibility / localization | UI implementation | Keyboard operation, screen-reader names, focus, contrast, reduced motion and text scaling |
| Background operation | Tray/window lifecycle | Connections survive window close; reopening restores verified device state; measure process-tree memory |

## Test inventory

| Device | Firmware record | Available hosts | Current result |
| --- | --- | --- | --- |
| WH-1000XM6 | Device readback confirms 3.1.5 | Windows 11 build 26300 tested; macOS/Linux available later | Protocol/model/firmware/battery readback passed; one 500 Hz EQ step was changed 5→6 and restored 6→5 by readback. Inquiry 0x19 parser, serializer and control path pass project checks and the corrected Windows app has been hardware-confirmed by the user across three tested modes, including physical NC/AMB button following. Raw GET `67 19 01 01 00 00 01 00 00`, NC Off notification `69 19 01 00 00 00 01 00 00`, and ambient notification `69 19 01 01 01 00 01 00 00` match the APK's 9-byte 0x19 layout. This validates neither all NC controls nor other models. |
| WH-1000XM5 | User reports current; exact version pending | Same | No capture or hardware test yet |
| WH-CH720N | User reports current; exact version pending | Same | No capture or hardware test yet |

Each completed test must record app build, OS/version, Bluetooth adapter, device firmware, negotiated protocol, steps, expected/actual result and capture reference. “All features supported” is reserved for completed model-specific end-to-end acceptance, not an attractive demo or a passing codec test.

The exact 0x19 byte map, enum values, APK evidence IDs and current capture decode are in [NC/ASM inquiry 0x19 schema](v2-ncasm-19-schema.md) and [machine-readable schema](v2-ncasm-19-schema.json).
