# Cinema mode: processing location and portability

Investigated 2026-11-05. Scope: Sony WH-1000XM6 360 Reality Audio Upmix for Cinema; Sony Sound Connect 13.3.0 static APK evidence and the desktop's current control implementation. No headphone commands, firmware extraction, app launching or audio capture were performed for this investigation.

## Finding

Cinema is a DSP algorithm running inside the supported headphone. The phone app configures it. In the hardware/software distinction, it is software/firmware on the headphone's processing hardware, rather than an effect executed by Sound Connect on the phone.

Sony's own developer interview explicitly locates the real-time stereo analysis inside the headphones. It describes estimating component positions, upmixing to a three-dimensional signal including elevation, HRTF processing, and a proprietary cinema-space acoustic simulation. This is stronger evidence of the processing location than the presence of a Bluetooth setting alone.

- [Sony WH-1000XM6 developer interview](https://www.sony.co.in/headphones/products/wh-1000xm6/developer-interview), section on Cinema and BGM, especially the paragraphs beginning with the developer's Cinema introduction and the following two paragraphs.
- [Sony Cinema help guide](https://helpguide.sony.net/mdr/2984/v1/en/contents/TP1001924935.html): applies to stereo sources, is switched through Sound Connect, retains its setting across headphone power cycles, and is disabled for calls and headphone-cable playback.

These sources do not identify the exact processor block executing every stage, provide firmware code, or prove the capabilities of other headphone models.

## Original APK evidence

Inspected saved REA/JADX observations from the original app, APK SHA-256 `58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`:

- `device-settings-cinema-parser-details.json`: `fn0.m0.b` binds the parameter response validator to `AudioInquiredType.UPMIX_CINEMA`, requires exactly three payload bytes, and validates an `OnOffSettingValue`. `fn0.m0.i(byte[])` decodes byte 2 as that setting.
- `device-settings-cinema-notify-command.json`: `fn0.g.b` similarly validates the three-byte Cinema notification. Its parent `fn0.s.b` uses `AUDIO_NTFY_PARAM`.
- `device-settings-listening-builders.json`: `fn0.z0.b` serializes `AUDIO_SET_PARAM` followed by the inquiry type. `fn0.g0.b` validates `AUDIO_RET_PARAM`.
- `listening-mode-ui.json`: `qx.t.q()` derives the listening-mode selection from repository state, including the Cinema state. These UI methods manage state and feature exclusions; they do not establish a host audio-processing path.

The evidence files live under the ignored `.research/evidence/` directory; original decompiled sources are not redistributed with the project. This investigation re-examined the retained static evidence. It did not dynamically trace the APK or recover headphone firmware.

## Current desktop path

The existing implementation in `src-tauri/src/discovery.rs` agrees with that control model:

| Operation | Sony MDR payload, before outer transport framing |
| --- | --- |
| Read Cinema state | `E6 04` |
| Read response | `E7 04 state` |
| Enable | `E8 04 00` |
| Disable | `E8 04 01` |
| Device notification | `E9 04 state` |

`cinema_setting()` reads the state. `set_cinema_enabled()` requires advertised support, writes the setting and waits for matching readback. `parse_cinema_value()` accepts only the exact length, inquiry and known values. These packets carry a setting, not PCM audio, HRTF coefficients or an upmix algorithm. Sending this packet to a different model cannot supply missing processing code.

This code is implementation evidence, not an independent recording of a new hardware exchange. The app's animated Cinema artwork is presentation, not an audio processor.

## Can other headphones have an equivalent effect?

Yes, as an independent desktop audio effect. A binaural renderer produces a two-channel signal, so ordinary stereo headphones can reproduce it; a vendor-specific spatial-audio feature is not a prerequisite. Microsoft explicitly supports platform spatial sound on any pair of headphones. That does not mean a platform renderer automatically upmixes all stereo content in the same way as Sony Cinema.

- [Microsoft Spatial Sound](https://learn.microsoft.com/en-us/windows/win32/coreaudio/spatial-sound)
- [Valve Steam Audio](https://valvesoftware.github.io/steam-audio/): HRTF-based rendering and environmental acoustics are available building blocks. They do not supply Sony's proprietary stereo analysis/upmix algorithm.

Two feasible approaches have different scope:

1. Use an existing OS spatial renderer where available and appropriately fed by the source application. This avoids implementing another renderer, but source formats, app integration and user-installed providers determine the result. It is not a Sony Cinema port.
2. Build an optional desktop effect: route source audio into a processing path, derive a virtual scene from stereo or use available multichannel positions, apply HRTF rendering plus controlled room reflections, then output binaural stereo. Stereo scene extraction is a separate algorithm and cannot recover original Atmos objects lost in a stereo downmix.

For system-wide Netflix/VLC/browser support, this requires actual audio routing and effect integration. Playback metadata used by Automatic Listening can select a profile but does not provide the audio samples. On Windows an APO or virtual-device route is a candidate requiring an integration prototype; it is not achievable solely by adding a Tauri control. macOS and Linux need their own integration work.

- [Microsoft APO architecture](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-processing-object-architecture)
- [Microsoft loopback recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording): ordinary loopback captures a copy of output rather than replacing the original stream. Protected content can be excluded, and exclusive-mode streams cannot use this loopback path. A capture-and-replay prototype cannot be advertised as universal Netflix/streaming support.

## Proposed product behavior

Keep **Sony Cinema** as the existing confirmed native device control. A separate **Desktop Theatre** feature could serve models without it, including suitable Sony and OnePlus outputs, after audio-route and listening validation. This would be a new engine, not the recovered Sony algorithm.

Use one spatial renderer at a time: native Sony Cinema, another device's spatializer, an OS renderer, or our desktop renderer. Avoid silently stacking effects. Do not promise cross-vendor head tracking without a verified sensor-data interface.

A desktop engine would consume CPU/memory while audio is being processed. An implementation should bypass/suspend it when unused and measure latency and resource use before promising a budget. For multichannel content, preserve available channels rather than discard them before rendering. Stereo content requires an explicit stereo virtualization/upmix design.

No runtime code was changed for this investigation. The processing location is established by Sony's developer explanation and corroborated by the traced control path. Exact Sony algorithm portability has not been established.
