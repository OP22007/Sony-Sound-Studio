# XM6 equalizer presets and local custom profiles

The desktop provides the WH-1000XM6's Off, Heavy, Clear, Hard, Soft and Game curves. The supplied original Sound Connect screenshots show the five shaped presets; Off is represented as a flat curve. The Manual entry keeps the current curve available for editing.

The ten frequencies, in order, are **31, 63, 125, 250, 500, 1000, 2000, 4000, 8000 and 16000 Hz**. The previous illustrative frequency list and Balanced/Warm/Vocal/Immersive examples are replaced in the XM6 preset selector.

| Preset | Signed gains, in frequency order |
| --- | --- |
| Off / flat | 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 |
| Heavy | 4, 4, -1, -1, 0, 0, 0, 0, 0, 0 |
| Clear | 0, 0, 0, 0, 0, 0, 0, 2, 2, 3 |
| Hard | 4, 4, -1, -1, 0, 0, 0, 2, 2, 3 |
| Soft | -3, -2, -1, 1, 1, 1, 0, 0, -2, -3 |
| Game | -6, -2, 2, 3, 1, -1, 1, 3, 0, -2 |

## Device behavior

The original 13.3.0 APK's V2 `EqPresetId` enum independently confirms OFF=0x00, GAMING_EQ=0x20, HEAVY=0x30, CLEAR=0x31, HARD=0x32, SOFT=0x33, CUSTOM=0xa0 and USER_SETTING1/2=0xa1/0xa2. Private static inspection records are excluded from Git. These identifiers do not establish device support or authorize a new factory-slot command.

The desktop therefore applies these **curves through the existing custom-EQ write path**. Signed gains are translated to the current XM6 layout's 0–12 device steps by adding 6. The backend retains its existing capability/firmware gating, serial command queue and matched band readback. The native selector follows confirmed values while a requested curve is pending or rejected. It does not claim to have selected Sony's native factory preset ID or disabled the equalizer DSP when a flat curve is selected.

No new Bluetooth command or factory/custom-bank identifier is sent. This work has browser/IPC fixture validation; complete factory-curve hardware/audio equivalence is not yet confirmed. The XM6 curves above apply to the validated XM6 firmware path only. Other Sony headphones do not use them: they describe their own equalizer (band count, step range, presets and band frequencies) and are controlled from that description, see [v2-equalizer.md](v2-equalizer.md). XM5/CH720N curves are not invented or populated from the ten-band XM6 data.

## Custom profiles

- Save an edited curve under a unique name, update an existing saved curve, rename it or delete it.
- Names are trimmed, limited to 40 characters and checked case-insensitively against factory names and existing names.
- Up to 24 preview presets and 24 device profiles can be stored. At capacity, creation is disabled instead of silently evicting older profiles.
- Preview profiles and native device profiles remain separate. Native profiles are scoped to the recorded model and firmware; they store unsigned device steps. Preview profiles retain signed values.
- Existing saved profiles are retained. Separate selector identifiers avoid collisions when an older user profile happens to share a new factory name.
- Changes to names and the local library do not send headphone commands. Storage must succeed before a save/rename/delete is reported as successful.
- Phone Manual/Custom 1/Custom 2 bank contents are not imported, guessed or overwritten. Users can create their own named local profiles.

Tests cover all six displayed curves, manual editing, save/update/rename/delete persistence, invalid names, storage failure, device-step conversion, pending/rejected confirmation and rename operations producing no device writes.
