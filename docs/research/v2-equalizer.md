# MDR V2 equalizer: reading a headphone's own layout

Static, read-only analysis of Sony Sound Connect 13.3.0 (`base.apk` SHA-256
`58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`), decompiled with the harness in
`scripts/jadx-dump`. The APK was never executed. **None of this has been exercised on a headphone other
than the WH-1000XM6 3.1.5 path that was already validated.** Every write still needs a matching readback.

## Why the CH720N equalizer was unavailable

The desktop only asked for an equalizer when the headphone was exactly a WH-1000XM6 on firmware 3.1.5,
using that model's ten-band layout (`GET [56 04]`, 0-12 steps). Every other model was skipped, so the
panel could never confirm anything. The headphones do not share one layout: the Sony app reads the band
count, level steps, presets and band frequencies from each headphone and builds the screen from that.

## Commands (command group `0x50`-`0x5b`, "EQEBB")

`EqEbbInquiredType`: `PRESET_EQ = 0`, `EBB = 1`, `PRESET_EQ_NONCUSTOMIZABLE = 2`,
`PRESET_EQ_AND_ULT_MODE = 3`, `PRESET_EQ_AND_ERRORCODE = 4` (the XM6), plus sound-effect types from `0x30`.
The matching support functions are `PRESET_EQ 0x50`, `EBB 0x51`, `PRESET_EQ_NON_CUSTOMIZABLE 0x52`,
`PRESET_EQ_AND_ULT_MODE 0x53` and `PRESET_EQ_AND_ERRORCODE 0x57`.

| Step | Bytes | Source |
| --- | --- | --- |
| GET_CAPABILITY | `[50, type, language]` (English = 1) | `jn0.b` |
| RET_CAPABILITY | `[51, type, bandCount, levelSteps, presetCount, (presetId, nameLen, name)*]` | parser `jn0.z`; consumed as `EqCapability(bandCount, levelSteps, presets)` |
| GET_EXTENDED_INFO | `[5A, type]` | `jn0.c` |
| RET_EXTENDED_INFO | `[5B, type, count, (kind, hi, lo)*]`; kind `1` = Hz, `2` = kHz, `16` = specific information, value `1` = Clear Bass | `jn0.e0`, `kn0.b`, `EqBandInformationType`, `SpecificInformationType` |
| GET_PARAM | `[56, type]` | validated on the XM6 for type 4 |
| RET_PARAM | `[57, type, presetId, count, step*]` | `kn0.c` |
| SET_PARAM | `[58, type, presetId, count, step*]`; choosing a preset sends `count = 0` | `i20.h` (`kn0.c.a().c(presetId, new int[0])`) |

The type used is chosen the same way the Sony app does: `0x57` means type 4, otherwise a customizable type
0, otherwise type 2 (presets only).

## Steps, centre and labels

`i20.h` defines the step range from `levelSteps`: minimum `-(levelSteps - 1) / 2`, maximum
`(levelSteps - 1) / 2`, and the raw value is `dB + (levelSteps - 1) / 2`. The XM6's 0-12 range is
`levelSteps = 13`. Band labels come from the extended info; a `Clear Bass` entry is part of the band list
(the standard layout is Clear Bass plus 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz and 16 kHz). Preset names for known
ids come from `EqResourceMap`: Off 0, Rock 1, Pop 2, Jazz 3, Dance 4, EDM 5, R&B / Hip-Hop 6, Acoustic 7,
Bright 16, Excited 17, Mellow 18, Relaxed 19, Vocal 20, Treble 21, Bass 22, Speech 23, Gaming 32, FPS 1-3
33-35, Heavy 48, Clear 49, Hard 50, Soft 51, Custom `0xA0`, User settings `0xA1`-`0xA5`. Unknown ids use the
name the headphone sends.

## What the desktop does

- Any model other than the validated XM6 path that advertises `0x57`, `0x50` or `0x52` is asked for its
  capability, band information and current values. Replies are validated against their own declarations
  (counts, lengths, steps inside the reported range); anything that does not fit leaves the equalizer
  unavailable and the raw bytes in diagnostics.
- Band edits send the **Custom** preset (`0xA0`) with every band step; presets send only the preset id.
  Each write requires a matching readback; otherwise the previous profile is written back and verified,
  never assumed.
- Preset-only headphones (type 2) get presets and no band editing.
- The XM6 3.1.5 path is unchanged.

## Not covered

Type 3 (`PRESET_EQ_AND_ULT_MODE`), the separate Clear Bass function (`0x51`, type 1), sound-effect types,
turn-key EQ and the user-setting banks are not implemented. The meaning of `levelSteps` as a count of values
(rather than a maximum) comes from `i20.h` and is confirmed on a headphone only by the readback that every
write requires.
