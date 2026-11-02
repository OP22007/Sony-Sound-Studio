# Implementation task registry

Owner: root orchestrator. Start: 2026-10-31. All agents share this workspace; file ownership below prevents conflicting edits. No reference-project source may be copied. APK and decompiled sources remain private.

Repository: `https://github.com/OP22007/Sony-Sound-Studio.git`. Local `main` is based on the existing `origin/main` initial commit; its MIT license is preserved. No agent may push or commit independently while integration is in progress.

## Quality gates

Completion requires actual files, appropriate tests, production builds, intended-file review, secret scan and explicit hardware limitations. No simulated action may be presented as a real headphone change. Browser preview must be clearly identified. UI quality has priority over arbitrary memory limits; idle CPU/webview lifecycle still matter.

## Assignments

Current task — complete Device settings and App settings (2026-11-01):

| ID | Owner | Scope | State |
| --- | --- | --- | --- |
| S1 | nc_anc_protocol | Device protocol research/implementation; discovery.rs, device_controls.rs, device schema evidence | In progress; preserve verified NC19 behavior |
| S2 | setter_enum_values | Read-only original-app research for Speak-to-Chat, wear pause, touch, guidance and notification controls | In progress; feed exact schemas to S1 |
| S3 | eq_frontend_audit | Settings UI, typed preferences bridge, dynamic device descriptors, frontend verification | In progress |
| S4 | app_preferences_luna | Native preference storage, OS autostart dependency, persistence tests | Implementation complete; root integrates |
| S5 | root | Native IPC, startup/tray/reconnect integration, cross-layer verification and user hardware handoff | In progress |

Desktop UI interaction remains user-operated. Native preferences use a private atomic JSON store; autostart must reflect the real OS registration. Device controls require advertised function support, a known wire schema and matched device readback. Unknown controls must never report simulated success. No firmware or reset workflow is enabled without independent package/recovery validation.

NC readback follow-up (2026-11-01): `nc_anc_protocol` owns `discovery.rs` (final-state validation and notification investigation); `eq_frontend_audit` owns `App.tsx` and native bridge regression tests (unconfirmed state presentation); `setter_enum_values` investigates original status/capability schemas read-only. Root integrates native state metadata, transport copy, and validation. User reports ANC remains on after removal of automatic rollback, while raw inquiry 0x17 GET still returns `67 17 00 00 00 00 00`. This is not verified final Off: the recorded status field is UNDER_CHANGING. Runtime state confirmation remains unresolved.

Expanded live capture found a distinct nine-byte NC update after a cancelling request: `69 19 01 01 00 00 01 00 00`. Inquiry 0x17 GETs remained all zero. Original inquiry 0x19 schema is being checked before interpreting the new frame. Native suite passed 30 tests (4 hardware tests skipped); frontend build and 4 native bridge fixture tests passed. These fixture results do not establish live NC confirmation.

Inquiry 0x19 follow-up: original-app parser/serializer and enum evidence now establish the nine-byte layout. User logs confirm Off (`69 19 01 00 00 00 01 00 00`), cancelling (`69 19 01 01 00 00 01 00 00`), ambient (`69 19 01 01 01 00 01 00 00`), and matching parameter GET on 0x19; user hears each requested mode. XM6 3.1.5 routing now uses 0x19 after advertised-function and strict readback checks. SET preserves ambient level, secondary toggle and adaptive sensitivity. Passive notifications update the selected mode; pending notices resolve on final state. Root verification: 33 Rust tests passed, 4 hardware tests skipped; frontend build and 5 native bridge fixtures passed; debug executable rebuilt and launched for user-operated final validation. No desktop UI automation is being used during this validation.

Final user-operated verification (2026-11-01): user answered **“Everything matches now”** when asked whether all three NC modes show the correct selection/confirmation and whether the app follows the physical NC/AMB button. The rebuilt Windows app's NC read/write and physical-button synchronization are therefore hardware-verified on this user's WH-1000XM6 firmware 3.1.5. This result does not validate other headphone models, firmware versions, or platforms.

| ID | Agent | State | Owned scope | Verification | Checkpoint |
| --- | --- | --- | --- | --- | --- |
| R1 | design_review | Complete: APPROVED | Read-only sequential skeptic, constraints, user advocate reviews | All objections accepted; arbiter approved bounded implementation | Before implementation |
| B1 | design_review (earlier agent) | Safe boundary; implementation saved | `src-tauri/**` except new explicitly delegated mac/status files | Cargo tests 22 pass; refresh build required after revision | Safe checkpoint 16:42 IST |
| F1 | frontend (earlier agent) | Complete | `src/**`, frontend configs/package, index.html, public assets | Typecheck/build passed; integration suite 13/13 | Complete 16:42 IST |
| Q1 | validation (earlier agent) | Complete | `tests/**`, scripts, README, independent integration review | Browser 13/13; REA closed; no running tasks | Complete 16:42 IST |
| L1 | protocol_research_luna | In progress | `docs/research/v2-control-schema-map.json/.md` only | Every evidence ID resolves privately; schema must keep unknown fields explicit | 30 minutes |
| L2 | release_audit_luna | Complete: read-only review | No file ownership | Reported macOS write timeout risk, stale Windows text; permissions/privacy otherwise sound | Complete 16:43 IST |
| L3 | macos_rfcomm_luna | Complete: fail-closed Mac connect | `src-tauri/src/transport/macos.rs` only | Isolated Apple target cargo check passed; Mac hardware untested | Complete 16:43 IST |
| L4 | platform_status_copy_luna | Complete | Only Windows diagnostic/status wording | cargo fmt; cargo test 22 pass, four hardware-dependent ignored | Complete 16:43 IST |
| L5 | bundle_configs_luna | Complete | Tauri platform config overlays only | Official Tauri docs + CLI loaded config; JSON parsed; all icon paths exist | Complete 16:44 IST |
| L6 | playwright_artifacts_luna | Complete | Playwright artifact paths | MS Edge Playwright 13/13; JSON, screenshots and traces stored only under ignored `tests/artifacts/` | Complete 16:48 IST |

Orchestrator owns this registry and integration decisions. An agent must request ownership before changing another agent's scope. No duplicate assignment starts while its owner remains active. Thirty-minute checkpoints are measured during this active run; stale assignments are queried or reassigned.

## Accepted design updates

- User explicitly requested implementation on 2026-10-31 following the architecture review prompt. Proceed without another permission loop.
- Rust is authoritative for real device state. The frontend uses typed commands and pending/confirmed/error states.
- Do not issue undocumented writes, firmware updates or reset operations. Implement researched transport and read-only discovery first; expose unresolved features with precise status while completing independent implementations where evidence permits.
- Device controls in an explicit preview mode may simulate state locally, with persistent visible labeling. Actual connected mode must never fall back to simulated success.
- User visual correction: use the authentic original Sony product renders for XM6, XM5 and CH720N rather than an invented headphone illustration. Frontend owns sourcing, correct model mapping and provenance; code license must not imply Sony artwork is MIT-licensed.

## Review and verification log

- Skeptic: accepted all five objections. Unknown payloads/capabilities remain unavailable; ACK never confirms setting; session generations reject stale results; bounded decoder handles fragmentation/resync/corruption; no firmware/reset writes; platform and preview readiness are distinct.
- Constraint Guardian: approved with conditions, all accepted. Native work off UI thread with cancellation/timeouts; bounded frames/queues/logs; validated narrow IPC and least permissions; local preferences/no telemetry; redacted opt-in diagnostics; OS readiness separately reported; proprietary research excluded; no unmeasured resource claims.
- User Advocate: approved with conditions, all accepted. Real empty state first; opt-in isolated preview; no fabricated readouts; pending/error versus confirmed state; actionable unsupported messages; keyboard/focus/reduced motion; close and quit distinct; sensitive exports explicit and invalidated update/reset disabled.
- Integrator/Arbiter: APPROVED for implementation within these gates. This does not certify complete Sony parity or hardware support.
- Initial verification: frontend typecheck/production build passed. Latest native suite passed 22 tests (four hardware dependent ignored); run again with final packaging build.
- Real hardware evidence: Windows native paired-device enumeration returned one Sony device, WH-1000XM6. User confirmed it is powered on/paired, firmware 3.1.5. This validates discovery only; settings support is not implied.
- Research progressed beyond the initial report: validation established exact protocol-info request/response layouts from additional private REA records; backend is implementing and testing the read-only handshake. Public evidence documentation must be updated with final results.
- Real hardware evidence: Windows RFCOMM socket opened successfully to the paired XM6 in 2.93 seconds and closed cleanly, with no payload sent in that smoke test.
- Initial optimized Windows executable and 2.29 MiB NSIS installer compiled; both predate the final revision/packaging updates, so rebuild before delivery.
- Protocol correction resolved before request: service UUID generation and MDR message table are independent axes. All adapters no longer infer message type from UUID index. Original V2 serializer establishes type `0x0c` for protocol-info GET on the exposed service.
- XM6 3.1.5 hardware handshake passed: matched V2 protocol response (`rawVersion=50343986`) and transport ACK observed separately; reply ACK emitted according to independently verified sequence inversion. This number is protocol metadata, not firmware. Session closed; no sound settings changed.
- Frontend independent suite passed 9/9 cases after fixing Vite watching Rust build output. Cases cover isolated preview, first launch, model state, keyboard EQ, saved presets, theme, navigation, motion, narrow layout and official artwork loads. Final integrated checks remain pending.
- Official original Sony model renders replaced the invented drawing. Exact page/download provenance and hashes are recorded in `public/headphones/SOURCES.md`; Sony artwork rights are distinct from the MIT source license.
- macOS RFCOMM control is disabled before any connection attempt until a bounded write path is proven. Paired-device discovery remains. Apple-target adapter-only check passed; full app build and hardware remain untested on macOS.
- Linux BlueZ/RFCOMM source passed an isolated Linux x86_64 adapter check; hardware and packaged app remain untested on Linux. Its runtime needs `bluetoothctl` and `sdptool`.
- User confirmed the native app closed and explicitly requested the XM6 readback test. Backend used one native hardware session, queried model/firmware and battery after exact schemas were established, then closed cleanly.
- Validation established the battery source layout for V2 GET_STATUS request `[0x22,0x00]` and response `[0x23,0x00,level,charging]`, length 4, level 0–100, charging enum 0–3. This readback, model and firmware were hardware-tested on XM6 3.1.5; the user's time-specific battery value is not stored in the repo.
- UI follow-up identified and corrected control labels from 8px to 12px minimum (frequency labels 11px); a 390px visual check found hero overlap, corrected with a separate mobile art region. Final browser suite 13/13 includes the 390px layout and native IPC fixtures; fixtures do not claim hardware behavior.
- Final validation handoff: Playwright 13/13 passed, including out-of-order snapshot guard; latest frontend production build passed. macOS/Linux adapter-only checks passed; native hardware untested on those OSes.
- Luna model is active for all newly spawned agents at the user's request. Earlier agents were asked to stop at safe boundaries; backend reported no live socket/app/build, validation closed REA and finished its suite, frontend saved all changes. No commit/push has occurred.
- Platform overlays now target Windows NSIS, macOS app/DMG and Linux DEB/RPM/AppImage. Tauri CLI loaded the config and JSON validation passed; native packaging on Mac/Linux remains unverified.
- Latest Windows NSIS rebuilt after frontend revision merge, protocol readback and status fixes; output 2.29 MiB. `npm run build`, `cargo fmt --check`, `cargo test --lib` (22 passed/4 hardware ignored), Playwright (13/13) passed.
- Latest native work implements capability-gated XM6 NC/ASM and EQ controls. NC/ASM GET is confirmed; the first mode SET did not change state, and the previous mode was restored and read back. A follow-up bounded poll/restore path is in code. Ambient level stays disabled without a device range. EQ live read/write still needs a successful RFCOMM session and readback. Current reconnect timed out with Windows code 10060.
- Local Windows resource smoke: active app tree measured 201.6 MiB private/445.7 MiB working set (app plus six WebView2 children) in one 6.33-second post-interaction sample; after window destroy, tray-only tree measured 5.5 MiB private/31.3 MiB working set and 0 stable-process CPU over 6.28 seconds. See `docs/research/resource-measurements.md`; do not generalize one run into a minimum-memory guarantee.
- The close-to-tray phase removed all WebView2 children and left the Rust tray process alive. The tray reopen action was not independently verified in this run; app remains running in the tray for manual open/quit.

## Shared IPC contract (locked for initial implementation)

Tauri commands use snake_case names and camelCase argument keys. All payload fields below serialize camelCase. Rust owns real state; browser preview state stays solely in React and is visibly labeled.

```ts
type ConnectionState = 'disconnected' | 'connecting' | 'connected';
type Device = {
  id: string; name: string; model: string; connection: ConnectionState;
  battery: number | null; firmware: string | null;
  capabilities: string[]; settings: Record<string, unknown>;
};
type AppState = {
  platform: string; transportStatus: string; devices: Device[];
  diagnostics: string[];
};
```

- `get_app_state()` → AppState
- `discover_devices()` → AppState; enumerates actual paired/discoverable devices, never invented data
- `connect_device({ deviceId: string })` → AppState
- `disconnect_device({ deviceId: string })` → AppState
- `apply_setting({ deviceId: string, setting: string, value: unknown })` → AppState or descriptive error; unsupported/unknown operations always reject
- `get_diagnostics()` → string[]; bounded and redacted
- Optional `app-state` event emits the full AppState; frontend must function using command results even without events.

Additional commands require coordination. Preview UI implements settings locally and never calls native apply_setting. Device labels cannot substitute for negotiated capabilities. Real controls require the corresponding capability string and confirmed settings data.
