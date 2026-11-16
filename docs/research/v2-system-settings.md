# MDR V2 system settings: wear detection, general settings, wearing-aware power off

Static, read-only analysis of Sony Sound Connect 13.3.0 (`base.apk` SHA-256
`58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`). The APK was never executed. The
decompile was produced with REA's JADX 0.7.1 library (`scripts/jadx-dump`). Nothing here has been
exercised against a headphone yet; each control still waits for a matching device readback before
the UI shows it as changed.

## Why these controls were listed as "not validated"

The desktop only had function IDs and a few unrelated command layouts. The Device settings page
listed a fixed set of rows regardless of what the headphone supports, so rows such as touch panel
and multipoint said "not validated" even when no command schema was ever searched for. This pass
decodes the schemas from the APK and makes the rows depend on what the headphone advertises.

## Wear detection ("Pause when headphones are removed")

Function `NO_1/0xF1` (`PLAYBACK_CONTROL_BY_WEARING_REMOVING_HEADPHONE_ON_OFF`) is handled by
`V2ControlByWearingRepository`. Earlier notes said the `0xF6/0x01` and `0xF2/0x01` paths were not
shown to be this setting; the repository proves they are.

| Step | Bytes | Source |
| --- | --- | --- |
| GET_STATUS | `[F2, 01]` -> `[F3, 01, 0=ENABLE / 1=DISABLE]` | `nj0.n0.b`, parser `nj0.j3.a` |
| GET_PARAM | `[F6, 01]` -> `[F7, 01, 0=ON / 1=OFF]` | `nj0.b0.b`, parser `nj0.x2.a` |
| SET_PARAM | `[F8, 01, 0=ON / 1=OFF]` | `nj0.b4.b` (opcode is `Imgcodecs.IMWRITE_PNG_ALL_FILTERS` = 248) |
| NTFY_PARAM | `[F9, 01, value]` | same command group as Speak-to-Chat `F6/F7/F8/F9` |

The status byte says whether the setting can currently be changed; the app only offers the toggle
when it is `ENABLE`.

## General settings 1-4 (touch sensor control panel, multipoint, voice guidance, ...)

Functions `NO_1/0xD1..0xD4` (`GENERAL_SETTING_1..4`) use inquiries `0xD1..0xD4`. The headphone
describes each setting itself, so the same four slots carry whichever settings a model has.
`GsTitleTitle` lists the titles the app knows: `TOUCH_PANEL_SETTING` (Touch sensor control panel),
`MULTIPOINT_SETTING`, `VOICE_GUIDANCE_SETTING`, `FACETAP_SETTING`, `SIDETONE_SETTING`,
`TAP_SENSITIVITY_SETTING`, `TWS_ONE_SIDE_USE_NCASM_SETTING` and the `ASSIGNABLE_KEY_SETTING*` family.
Titles and options are sent either as raw text or as Sony enum names (for example
`VOICE_GUIDANCE_ELEM_ENGLISH`).

| Step | Bytes | Source |
| --- | --- | --- |
| GET_CAPABILITY | `[D0, inq, language]` (English = 1) | layout from parser `hj0.b`; no serializer exists in the app's KMP code |
| RET_CAPABILITY, boolean | `[D1, inq, 0, format, titleLen, title, summaryLen, summary]` | `hj0.n` |
| RET_CAPABILITY, list | the same with type `1`, then `count (1..64)` and `count` elements `[format, subjectLen, subject, summaryLen, summary]` | `hj0.p`, element parser `hj0.b3` |
| GET_STATUS | `[D2, inq]` -> `[D3, inq, 0=ENABLE / 1=DISABLE]` | `hj0.f`, `hj0.v` |
| GET_PARAM | `[D6, inq]` -> boolean `[D7, inq, 0, 0=ON / 1=OFF]` or list `[D7, inq, 1, index < 64]` | `hj0.d`, `hj0.r`, `hj0.t` |
| SET_PARAM | boolean `[D8, inq, 0, value]`, list `[D8, inq, 1, index]` | parsers `hj0.x`, `hj0.z`; opcode `D8` follows the group sequence |
| NTFY_PARAM | `[D9, inq, ...]` same layout as RET_PARAM | `hj0.g`, `hj0.i` |

`format` is `0` raw text or `1` enum name. The slot subclasses are `kh0.a`, `lh0.a`, `mh0.a`,
`nh0.a` (constructor argument `209..212`).

The desktop keeps two limits visible: the capability request bytes and the SET opcode are inferred
from the matching parsers and the command-group pattern, because the shipped app builds those
frames elsewhere. They are confirmed on a device only by the matching GET readback that every write
requires.

## Wearing-aware automatic power off

`AUTO_POWER_OFF_WITH_WEARING_DETECTION` is function `0x25`, inquiry `0x05`. The setter builder is
`yn0.w0`: `[28, 05, activeElement, selectedTime]`, the same layout the editable `0x24` / inquiry
`0x04` path already used. Element codes: `0` 5 min, `1` 30 min, `2` 60 min, `3` 180 min, `4` 15 min,
`16` when removed from ears, `17` disabled. The control that was read-only is now editable, with
readback and restoration as for `0x04`.

## Not implemented

* **Multipoint device management.** Sony's multipoint screen is a paired-device list with connect
  and disconnect actions (table-2 `PAIRING_DEVICE_MANAGEMENT` functions `0x30`, `0x32`, `0x33`,
  `MultipointDeviceSettingsFragment`). Only a headphone that exposes a plain `MULTIPOINT_SETTING`
  general setting gets a toggle.
* **Table-2 voice guidance (`0x41`, `0x42`, `0x44`) and notification-tone (`0x48`) functions.**
  They are advertised by the XM6 but use a different command group; the page now says so.
* **Which functions the XM6 actually advertises.** The raw support reply is not stored in the
  repository. The page lists general settings only when the headphone advertises `0xD1..0xD4`.

## Reproduce

Dump the relevant classes with `scripts/jadx-dump` (patterns `^hj0\.`, `^nj0\.`, `^kh0\.a`, `^yn0\.`),
then compare against the unit tests in `src-tauri/src/system_settings.rs` and the session tests in
`src-tauri/src/discovery.rs`.
