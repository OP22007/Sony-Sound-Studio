# Desktop design and validation

Status: **approved for implementation within recorded validation gates**, 2026-10-31. The user explicitly requested implementation after architecture review. Sequential skeptic, constraint, user-advocate and arbiter review approved the architecture; accepted conditions and task ownership are recorded in [implementation registry](../implementation-registry.md). This is not certification of full Sony parity or hardware support.

## Confirmed intent and decisions

| Decision | Rationale / alternatives |
| --- | --- |
| Tauri 2, Rust, React + Vite + TypeScript | Accepted native shell and frontend; user selected React over the earlier Svelte proposal |
| Windows, macOS and Linux | User has hardware for later testing on all three |
| XM6, XM5, CH720N first | These are available devices; support must be proven per firmware |
| Desktop-only operation | Phone is a research instrument, never a product runtime dependency |
| Public open source, local-first | Online features opt-in; no default telemetry |
| Expressive audio-studio interface | Accepted device rail, large device presentation, interactive EQ, charcoal/amber and light themes |
| Visual quality takes priority | User removed UI/animation memory budgets; optimize idle waste and background resources without restricting visual ambition |
| Independent implementation | Reference project can be wrong; use original-artifact evidence and captures rather than copying code |

## Proposed architecture: section 1 for review

Rust owns each device session: Bluetooth transport, streaming frame decoder, request queue, ACK/retry state, capability negotiation and authoritative settings. Platform adapters implement Windows, macOS and Linux transports behind one interface. The protocol layer does not depend on React or OS UI APIs. Transport availability, particularly service access and coexistence with audio, must be demonstrated on each OS before promising parity.

React renders typed snapshots and subscribes to state changes through narrowly scoped Tauri commands/events. A control shows a pending state until the device confirms it; failures restore the last confirmed value and offer a useful retry. Unsolicited notifications update the same Rust state. Disconnect invalidates pending requests; reconnect renegotiates capabilities and rereads settings. One serialized command stream per device prevents overlapping operations from corrupting sequence or response matching.

The tray and Rust sessions remain active after the main window closes. The proposed lifecycle destroys the main webview and recreates it on reopen, subject to measured behavior on each platform. Animations pause when hidden and honor reduced motion. Local preferences and optional history have separate retention settings; diagnostics omit device addresses and private payloads by default. Debug capture export is explicit and reviewable.

Disposition: accepted for implementation following the user's implementation request and peer review.

## Remaining design sections

These need incremental review before implementation: capability/state schemas; visual layout and interaction prototype; error/reconnect flows; local storage and service authentication; platform-specific transport; update isolation and recovery; accessibility; release packaging; test and resource budgets based on measurements.

Assumptions to validate: one user per installation; a small number of simultaneous devices; no server required for core controls; independent models may expose different bands and control schemas. Exact XM5/CH720N firmware and the installed phone app version remain to be recorded.

## Evidence collection plan

1. Record device model, exact firmware, phone OS, installed Sound Connect version and active audio connection mode. The uploaded 13.3.0 artifact may differ from the installed app.
2. Obtain Bluetooth HCI snoop logs using Android developer facilities. Follow [Android's verification/debugging documentation](https://source.android.com/docs/core/connect/bluetooth/verifying_debugging); extraction permissions and bugreport contents vary by device. Do not assume direct access to the system log directory.
3. Start with XM6 3.1.5. Capture baseline connection, service selection and capability discovery. Keep a timestamped action log.
4. Change one setting at a time: noise cancellation to ambient to off where offered; one ambient step; one EQ band or preset; DSEE. Read back after each action and restore original values. Keep unrelated apps and automatic scenes from changing settings during this baseline.
5. Identify request, ACK, matched response and unsolicited notification separately. Record table/version, payload schema, state transitions, ordering, timeouts and interactions with multipoint/LE Audio.
6. Repeat on XM5 and CH720N, recording exact firmware. Compare capability replies rather than projecting XM6 behavior onto other models.
7. Validate independently implemented read-only sessions first on Windows, then a reversible setting round-trip. Repeat on macOS/Linux with their adapters and audio coexistence.

Captures may contain identifiers and unrelated traffic. Keep originals private, minimize sessions, and publish only reviewed redacted fixtures. Firmware updates and reset/recovery experiments require a separately understood and validated update flow.

## Implementation sequence after design validation

| Stage | Deliverable | Exit evidence |
| --- | --- | --- |
| Protocol foundation | Independently authored encoder/decoder, bounded buffering, async session state | Golden vectors, escaping/checksum cases, fragmented/coalesced reads, invalid length/checksum, timeout/cancellation and replay fixtures |
| Native transport | Adapter per OS; pairing/permission diagnostics | Actual Sony service connection and read-only identity/capability discovery on each host |
| First complete control | XM6 noise or EQ based on captured schema | UI action → Rust → device → confirmed readback; reconnect and mobile-originated change verified |
| Visual shell | Device presentation, navigation, EQ interaction, themes and coordinated Anime.js motion | Reviewed running UI; keyboard/screen-reader checks; reduced-motion behavior |
| Model expansion | XM5 and CH720N negotiated controls | Model-specific tests; unsupported functions absent or explained; no false success |
| Advanced parity | Automation, listening data, services and spatial features | Each dependency resolved and end-to-end behavior demonstrated |
| Firmware updates | Isolated update workflow | Authenticity, compatibility, progress, interruption and recovery verified |
| Release | Installers and documented support matrix | Install/upgrade/uninstall checks across hosts; full acceptance and resource measurements |

Measure the entire process tree for cold launch, steady UI, active animation, idle tray and repeated close/reopen cycles. Record CPU, private memory/resident memory, startup latency, interaction frame times and leaks on named hardware. No invented memory target or claim of “minimum possible” before measurements.

The invoked [brainstorming skill](C:/Users/ompom/.codex/skills/brainstorming/SKILL.md) requires design validation before implementation and a high-risk design review through its multi-agent-brainstorming handoff. Complete those gates after the remaining design sections have been reviewed. This requirement does not block evidence documentation or ongoing capture research.
