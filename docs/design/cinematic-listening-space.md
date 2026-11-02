# Cinematic listening space

Approved direction: option 1, explicitly selected for implementation by the user on 2026-11-01.

## Intent

- Present the original Sony artwork as a believable product portrait with a safe frame.
- Express listening modes through the space, light, and depth around the headphone.
- Give Standard, Background Music, and Cinema visibly distinct environments.
- Let ANC quiet the surrounding field and Ambient open it.
- Show EQ as a sculpted settings surface: broad bass contours, central midrange definition, and fine treble detail.
- Keep device capability gates, pending states, and confirmed readback authoritative.

## Assumptions and constraints

One local user and a few paired devices; no network or audio capture is needed for this visualization. Continue using React, Vite, Tauri and the installed Anime.js v4. Visual quality takes priority over an arbitrary memory cap, while idle work and hidden-window resource use remain bounded. Reduced motion presents the complete final composition without animation. Headphone controls stay usable during transitions. Scene code remains separate from Bluetooth and device-setting code for maintenance.

## Decision log

| Decision | Alternatives | Reason |
| --- | --- | --- |
| Cinematic environment around the official Sony render | True 3D showroom; instrument-focused UI | User selected option 1. Accurate Sony meshes are not available; official artwork preserves product identity. |
| Environment movement carries mode changes | Repeated product tilts, rings, bars | The user rejected the prior effects. Architecture, light and depth create a coherent listening space. |
| Product remains physically steady | Rotating a PNG to imitate new viewpoints | Original artwork cannot provide unseen product angles. Gentle levitation and a coordinated shadow are appropriate. |
| EQ deforms a deterministic mesh | Random waveform, spectrum animation | The visualization describes EQ settings and does not imply live audio analysis. |
| Separate entrance, listening, noise, EQ and pending timelines | One shared timeline for every change | Independent controls can change without restarting unrelated motion. Cleanup restores styles; animations suspend outside view. |

## Composition and motion

Standard has a balanced, softly lit studio. Background Music introduces a warm three-plane room; My Room, Living Room and Cafe widen the room progressively. Cinema reveals a broad panoramic light field and deeper side wings. Layers crossfade and translate on Anime.js timelines; product movement remains small and contained.

Noise-control overlays are independent of listening mode. ANC contracts and dims the peripheral field; Ambient opens the side light and reveals depth. Off returns a neutral perimeter. EQ changes morph a perspective mesh and briefly illuminate the relevant bass, midrange or treble area; bass also influences the grounded shadow. Pending requests receive immediate restrained feedback, with the actual scene following confirmed device state.

## Verification

Check the real rendered scene across all three official product images and desktop/narrow widths. Exercise all mode/environment combinations, EQ groups, rapid changes, native pending/failed confirmations, reduced motion, hidden/offscreen suspension, and light/dark palettes. Browser fixtures validate UI wiring; they do not establish new headphone-protocol support. Subjective motion quality requires visual review, not only passing tests.

Implementation evidence, 2026-11-01: production frontend build passed; all 25 browser checks passed. Product bounds were checked across XM6, XM5 and CH720N at 1920, 1440, 1024, 760 and 390 px. Additional checks covered native confirmation, mode/room differences without motion, finite EQ mesh geometry, active timelines, offscreen suspension/resume and reduced motion. Standard, Cafe and Cinema screenshots were visually reviewed. A local browser-preview recording is saved at `tests/artifacts/cinematic-listening-preview.webm`; `scripts/capture-scene-preview.mjs` reproduces it. Frame-time profiling and fresh physical headphone validation were not performed for this visual change.
