# Desktop refinement and connection behavior

## Confirmed intent

The user approved refining the desktop studio, retaining authentic product artwork,
with usable controls and refined motion. They additionally requested automatic
connection when supported headphones connect to the laptop, selection of the active
headphone, graph-based EQ editing, removal of unexplained automatic-listening holds,
and persistent VLC integration after restarting VLC.

## Assumptions and limits

One local user with a few paired devices. Existing React/Vite/Tauri/Anime.js components
remain; this pass does not introduce a WebGL runtime or pretend a photograph is a
rotatable 3D model. Visual quality takes priority over an arbitrary animation budget,
while hidden scenes still suspend their animation. No microphone capture, playback
audio capture, telemetry, or added cloud service is required. Native readback remains
authoritative. Physical validation and resource profiling are separate from synthetic
tests; no new minimum-memory or frame-rate measurement is claimed.

Windows exposes the default multimedia render endpoint through Core Audio. Linux and
macOS expose connected-device state in this implementation, but do not yet resolve
the active audio output. macOS control-service writes remain subject to the existing
disabled-adapter restriction. VLC preference setup is Windows-only and opt-in.

## Decision log

| Decision | Alternative | Reason |
| --- | --- | --- |
| Refine the existing graphite/amber studio | Replace the entire visual language | Preserve the previously approved product direction and familiar controls. |
| Give authentic product artwork a larger, safe frame | Invent a replacement mesh | Product accuracy and a clear composition without an unverified asset. |
| Make graph points the EQ controls | Retain small duplicate range bars | Direct manipulation, keyboard slider semantics, and fewer focus targets. |
| Stage Sony graph edits and apply once | Send every pointer-move value | Avoid interrupting a drag with native busy state or flooding Bluetooth writes. |
| Observe native Bluetooth/output edges every four seconds | Reconnect only the last saved device | Detect newly connected headphones without keeping a webview alive. |
| Match only unique, connected identities/names | Select the first paired entry | Pairing is not a live connection, and duplicate names must not select the wrong device. |
| Follow output changes, retain manual browsing between changes | Reset UI selection on every snapshot | Keep the active device discoverable without fighting deliberate navigation. |
| Hold automation only for explicit, changed, queued listening-mode choices | Infer intent from readback differences | Transitions and stale replies do not establish a user's manual choice. |
| Make manual holds session-only | Restore a permanent pause at startup | Avoid surprising pauses from an earlier session. |
| Save VLC HTTP startup through an explicit action | Start it through the View menu each run | VLC's View menu action lasts only for the current session. |

## Implementation

The device observer only enumerates the paired catalogue on connection/output edges.
It schedules bounded connection retries, respects the auto-reconnect preference and
an explicit Disconnect, and rearms after a real Bluetooth reconnect. Public snapshots
carry opaque app device IDs, not Bluetooth addresses. Active-output changes are
published separately from control-connection state and from device capabilities.

Sony and OnePlus share accessible draggable graph handles with pointer capture,
touch handling, cancellation and keyboard arrows/Home/End. The curve remains a
visualization of band settings rather than a measured acoustic frequency response.
Native Sony edits remain local until Apply; failure retains the draft and Discard
restores the confirmed curve. Preset selection continues its existing confirmed-write
workflow. OnePlus retains model-specific layouts and its existing explicit apply path.

The VLC setup command requires saved credentials and a closed VLC process. It preserves
other preferences/interfaces, creates a backup, and atomically saves loopback HTTP
startup. It never starts VLC, records a password in diagnostics, or changes a headphone
mode. Fixture tests use temporary configuration files; development did not modify
the installed VLC preferences.

## Validation

Windows native tests: 132 passed, five hardware-dependent tests intentionally ignored.
Browser suite: 67 passed, covering pointer/keyboard EQ, pending/rejected writes, device
selection, stale snapshots, VLC startup opt-in, responsive layouts, themes, reduced
motion and animation suspension. Desktop Sound Studio and EQ screenshots were visually
reviewed; artwork framing was checked across supported preview models and widths.

Physical Bluetooth transitions, Core Audio matching and persistent VLC startup still
need a user test on the rebuilt executable. macOS/Linux compilation and hardware were
not validated in this pass. No GitHub release is implied by a local build.

Windows optimized executable rebuilt successfully with `npm run tauri -- build --no-bundle`
after the user exited the running app. Output: `src-tauri/target/release/sound-connect-desktop.exe`.
The executable was not launched automatically and installed VLC preferences were not changed.


## UI refinement within the original layout — 8 November 2026

User feedback rejected moving navigation into a horizontal header. The original
left sidebar, device list, workspace heading and two-column sound/noise control
card are restored. Refinement remains inside this arrangement: more readable
navigation/control labels, a larger authentic product stage with deliberate
framing, restrained neutral surfaces and amber selection, concise current-mode
readouts and a clearer EQ editor/preset library. All device logic is preserved.

The base stylesheet now loads before component styles so intentional component
layouts are no longer overridden. Scanline texture was removed and preview state
is labelled separately from live device state. Below 900px the original responsive
navigation wraps; the device list stays in normal flow to avoid covering nav buttons.

Reviewed screenshots: `tests/artifacts/sidebar-refined-studio.png` and
`tests/artifacts/sidebar-refined-eq.png`. Original manufacturer artwork and existing
mode-specific choreography remain; no WebGL runtime was added. Hardware behavior
was not tested in this visual pass.

Refined-sidebar verification: TypeScript and production frontend build passed; all
67 browser regression tests passed, including the 800px navigation case, 390px
EQ reachability, confirmed native writes and reduced-motion/artwork framing checks.


## Compact device status and grounded artwork

The sidebar arrangement stays intact. The desktop hero shrinks from 402px to
322px (including borders), approximately 20%. A dedicated battery glyph and
percentage use only reported device values; unknown battery remains a dash.
Connection/control status, listening mode and ANC are grouped for scanning,
with Disconnect rendered as a secondary action. Codec is explicitly Not reported
because no current native source supplies a negotiated audio codec.

The official product image remains the visual source. A small opposing shadow
animation, soft contact shadow and subtle earcup spill light give it depth without
inventing unseen product geometry or adding a WebGL dependency. L/R indicators
represent output channels, not an inferred physical earcup orientation.

On Windows only, get_output_levels reads Core Audio IAudioMeterInformation peak
metadata from the default multimedia render endpoint; it does not capture or
record audio. The command requires a connected selected device matching the
observed system output, rejects ambiguous model names, rechecks state after the
read, and compares the current endpoint name. Mono is mirrored; unsupported
multichannel layouts, missing interfaces and invalid samples return unavailable.
Frontend sampling is sequential at 250ms and stops when hidden, offscreen,
unmounted or reduced motion is enabled. No synthetic playback levels run in
preview. Actual output peaks drive at most 0.7px translation and 0.25% scale;
idle float and confirmed-mode transitions remain separate.

The meter is not implemented on macOS/Linux in this pass. Native hardware audio
metering requires a user test; browser fixtures validate channel response and
reduced-motion cleanup. Visual review covers dark/light, 1440px, 960px and 390px.
Native unit tests: 133 passed, 5 hardware tests ignored.

Compact-card Windows release build completed successfully. Frontend production build and native tests passed; browser regressions and corrected targeted cases passed. The executable was not launched automatically. Physical peak response remains for user validation.

Connection action correction: Connect/Disconnect now align to the start of the device details, with adjacent helper text rather than being pushed to the stage boundary. Shared scrollbars use theme-aware surfaces, rounded thumbs and no arrow buttons in Chromium/WebView2; conflicting per-panel scrollbar rules were removed. Unconfirmed listening state no longer labels the neutral environment Standard. TypeScript, production frontend build and 17 relevant browser tests passed; desktop/960px disconnected layouts were visually reviewed.

## Mode-specific 3D listening rooms — 8 November 2026

The compact hero now places the original manufacturer headphone image in front
of an actual Three.js interior. Cinema has a projection screen, folded burgundy
curtains, 24 instanced velvet seats and aisle lights. Background Music uses three
separate interiors: a desk/bookshelf personal room, sofa/reading-light living room,
and a cafe with timber flooring, bistro seating, pendants and espresso counter.
Music and Game have distinct stage and acoustic-panel environments where supported.
These are visual environments; mode availability and device confirmation remain
controlled by the existing native capability gates. The headphone itself remains
authentic product artwork, not a newly invented 3D mesh.

Three.js is pinned and loaded in a separate lazy chunk. One renderer/context is
reused across modes. Pixel ratio is capped at 1.5 and internal width at 1000px.
Repeated theater seating uses instancing; materials and basic geometry are shared
within each scene. Local procedural geometry avoids external asset downloads.
Contact shadows use a tiny generated texture; no real-time shadow maps or
postprocessing passes run. Pointer camera parallax settles and stops requesting
frames; hidden/offscreen rendering pauses and reduced motion suppresses parallax.
Scene resources are disposed when replaced or unmounted. WebGL failure or context
loss releases the canvas and restores the existing CSS environment and controls.

Validation: TypeScript/production build passed; all 71 browser tests passed,
including context reuse across rooms/theme/resizing and WebGL/context-loss fallback.
Cinema, cafe, living room and personal room were visually reviewed; cafe was also
reviewed in light theme and at 390px. Production renderer chunk is about 135kB gzip,
loaded separately from the main interface. Native hardware controls were not
changed or exercised during this visual task.

Windows optimized executable rebuilt successfully with the 3D rooms using npm run tauri -- build --no-bundle. The app was not launched automatically. Physical device testing remains separate from this visual change.

## Noise-control sound field refinement

The product stage now has a separate decorative SVG acoustic field, composited
above the room and behind the authentic product art. Noise cancelling closes a
translucent bevelled lens: warm incoming wavefronts terminate at its boundary,
with short opposing cool traces and soft absorption sheets. Ambient opens that
boundary and uses longer sage wave paths and curved translucent sheets flowing
through the listening space. Off removes the active boundary and moving sheets,
leaving only three subdued neutral traces. Confirmed native noise state continues
to select the effect; pending requests and unconfirmed state do not invent a result.
This is an illustration, not a noise measurement or claim of acoustic performance.

Mode changes crossfade and reshape the field. Stream paths and sheet movement
pause when their mode is inactive, the stage is offscreen or the document is hidden.
Reduced motion retains distinct static treatments without animation. No new GPU
context, dependencies, audio capture or hardware commands were introduced. Existing
room rendering and original compact/sidebar layout are preserved. Dark, light and
390px views were visually reviewed. Production frontend build and 10 relevant
browser tests passed, including mode differentiation, offscreen suspension and
reduced-motion behavior.

The Windows optimized executable was rebuilt successfully with the noise-field update. The app was not launched automatically.

## Battery and visual hierarchy refinement

The battery readout uses a smaller percentage and refined fill icon, with a charging
bolt only for reported charging state. Charging, fully charged, not charging and
unknown states remain distinct. A visible warning appears strictly below 20%,
including while charging. Unknown battery never displays a charging claim.

Native battery replies now stamp batteryReadAtMs on initial Sony/OnePlus readback
and valid OnePlus battery notifications, including replies whose percentage is
unchanged. Unrelated state updates do not refresh this timestamp; disconnect clears
it with other device settings. The UI shows Last read with a local time/full-date
tooltip. Missing timestamps are explicitly unavailable; it does not imply a live
stream or introduce battery polling.

The acoustic enclosure rim/seam and floor ellipse are quieter; standard-room
vertical slats and ground shadows are less intense. The original headphone art
keeps a shorter contact shadow and restrained rim/earcup lighting. Wave motion
continues to honor visibility and reduced-motion controls. Sound mode cards now
have a capped width, tighter spacing, smaller icon treatments and muted mode-specific
selection tones within the original two-column control deck and sidebar layout.

Visual review includes 1440px, 960px and 390px, plus actual-low-battery UI fixtures.
These fixture percentages are synthetic test data, not a hardware reading. Native
unit tests: 133 passed, 5 hardware tests ignored. Charging and receipt timestamps
still require validation on physical devices; no hardware settings were changed.

Verification completed: all 23 relevant browser tests passed on rerun; the first concurrent run had two timing failures. Production frontend and Windows optimized executable builds passed. The rebuilt app was not launched automatically.
