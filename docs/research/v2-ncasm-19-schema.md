# NC/ASM inquiry 0x19 schema

This page documents the Sony Sound Connect Android APK's static protocol schema and separate live WH-1000XM6 observations. The APK was not executed. These findings establish how the app serializes and parses inquiry 0x19; they do not by themselves establish model support or successful setting application. The full machine-readable layout and evidence IDs are in [v2-ncasm-19-schema.json](v2-ncasm-19-schema.json). REA evidence records remain private under `.research/evidence/`.

## Wire layout

The full GET response / notification is 9 bytes:

| Offset | Field | Wire value / type |
| ---: | --- | --- |
| 0 | Command | `0x67` GET response or `0x69` notification |
| 1 | Inquiry | `0x19` (`MODE_NC_ASM_DUAL_NC_MODE_SWITCH_AND_ASM_SEAMLESS_NA`) |
| 2 | Change status | `ValueChangeStatus`: `0=UNDER_CHANGING`, `1=CHANGED` |
| 3 | Inherited on/off | `NcAsmOnOffValue`: `0=OFF`, `1=ON` |
| 4 | NC/ASM mode | `NcAsmMode`: `0=NC`, `1=ASM` |
| 5 | Ambient mode | `AmbientSoundMode`: `0=NORMAL`, `1=VOICE` |
| 6 | Ambient level | Unsigned byte |
| 7 | Mode-specific on/off | `NcAsmOnOffValue`: `0=OFF`, `1=ON` |
| 8 | Noise-adaptive sensitivity | `0=STANDARD`, `1=HIGH`, `2=LOW` |

The typed parser is `sn0.m0`. Its inherited status/on-off accessors read offsets 2 and 3. Its specific accessors decode offsets 4–8. The `0x19` validator in `un0.g.f` requires a 9-byte buffer and byte 1 equal to `0x19`, then validates enum values at offsets 4, 5, 7 and 8.

The SET builder `sn0.d1.b.h` writes the inquiry and inherited fields first, then appends `[NcAsmMode, AmbientSoundMode, level-low-byte, mode-specific NcAsmOnOffValue, NoiseAdaptiveSensitivity]`. The level is written as the low byte of the provided integer. This describes the APK serializer and our implementation schema; it does not prove device acceptance.

### Validator/accessor mode discrepancy

The response validator checks offset 4 with `NcNcssAsmMode`, whose values are NC=0, ASM=1 and NcSS=2. The typed parser accessor and SET builder use `NcAsmMode`, which only defines NC=0 and ASM=1. Therefore offset value 2 passes the validator's first enum check but produces `OUT_OF_RANGE` through the typed mode accessor. Preserve/report that discrepancy; do not silently map NcSS to NC or ASM.

## Notification dispatch

The table-1 command factory maps `NCASM_NTFY_PARAM` (`0x69`) to switch case 72 and returns an `sn0.d.b` parser instance. Inquiry 0x19 is then validated and decoded using the 9-byte layout above. This dispatch is statically observed in the APK. It does not by itself prove that every notification reaches the UI or that a later state read is final.

## WH-1000XM6 live captures

The following raw diagnostic observations were supplied during tests on the user's WH-1000XM6 running firmware 3.1.5. They demonstrate that the captured frame fits the schema. Their semantic label is a decode using the APK enum values; a user's audible observation remains a separate observation.

| Raw bytes | Decode |
| --- | --- |
| `67 19 01 01 00 00 01 00 00` | GET: CHANGED, inherited on/off ON, NC, NORMAL, level 1, mode-specific on/off OFF, STANDARD |
| `69 19 01 00 00 00 01 00 00` | Notification: CHANGED, inherited on/off OFF, NC, NORMAL, level 1, mode-specific on/off OFF, STANDARD |
| `69 19 01 01 01 00 01 00 00` | Notification: CHANGED, inherited on/off ON, ASM, NORMAL, level 1, mode-specific on/off OFF, STANDARD; user reports ambient sound is audible |

The 0x19 read/write path is implemented and covered by project checks. The user has now confirmed that the corrected rebuilt Windows app matches all three tested NC/ASM mode selections, confirms the requested state, and follows physical NC/AMB button changes on the WH-1000XM6 running firmware 3.1.5. This hardware result is model-, firmware- and host-specific; it does not establish full NC/ASM parity on other models or operating systems.

## Evidence

The machine-readable JSON lists the evidence ID for each claim. The principal static records show: command factory dispatch (`ev_70c0…`), the 9-byte validator (`ev_2cf5…`), accessors (`ev_a3c0…`, `ev_b878…`, `ev_4d07…`, `ev_3f75…`, `ev_6cfb…`, `ev_1b5c…`, `ev_42a9…`), setter builder/writer (`ev_b6f6…`, `ev_5a0f…`), and enum constants (`ev_d7d0…`, `ev_07aa…`, `ev_e218…`, `ev_25fa…`). Evidence is derived from the shipped APK SHA-256 `58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`; see the exact full IDs in the JSON.
