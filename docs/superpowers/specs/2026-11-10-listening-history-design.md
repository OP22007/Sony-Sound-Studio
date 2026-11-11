# Listening history — design

Status: design approved, not implemented. Date: 2026-11-10.

## Understanding summary

- **What:** a local listening-history feature with a new **History** tab: totals for today, 7 days, 30 days and all time, time per mode, time per device, time per content type, and a recent-sessions list.
- **Why:** the user wants to see how they use their headphones: time per listening mode and what they play.
- **Who:** users of Sony and experimental OnePlus devices on every platform; detail recording is Windows-only because it depends on Follow media.
- **Basic recording (on by default):** connected time per device and per listening mode (Standard, Cinema, Music, Game, Ambient, Noise cancelling). Works on every platform.
- **Detail recording (separate opt-in, off by default):** content category (game, music, movie, other) plus app, site and track names. Needs Follow media, so Windows only.
- **Privacy:** everything stays on this computer. Nothing is uploaded, and the History tab says so.
- **Non-goals:** cloud sync, sharing, social features, recommendations, export, and audio-level "was sound playing" detection when Follow media is off.

## Assumptions

1. History is stored in a local file in the app data folder.
2. Data is kept until the user clears it. There is no automatic retention limit.
3. "Listening time" means connected time in a mode. The UI labels it that way and does not claim audio was playing unless Follow media supplies that.
4. Recording overhead is one small write about every minute.
5. An unclean exit loses at most about one minute.
6. Track and site names come from the existing Windows media sessions and browser companion, with no new permissions.
7. Rust logic gets unit tests and the tab gets Playwright tests. Nothing is hardware-verified.

## Architecture

New backend module `src-tauri/src/history.rs`:

1. **Recorder** holds one open segment per connected device: `{start, device_id, device_name, mode, category?, app?, site?, track?}`.
   - Inputs: device connect, disconnect and mode changes from the device session in `lib.rs`; category, source, app, site and track from `media_automation.rs` (its `Choice` and media snapshot).
   - A segment closes and a new one opens when the device, mode, category or source changes. A track change opens a new segment only while detail recording is on.
   - A heartbeat checkpoints the open segment about once a minute. Disconnect, app exit and system suspend close it.
2. **Store** is an append-only `history.jsonl` in the app data folder. One line per closed segment: `{v:1, start, end, device, mode, category?, app?, site?, track?}`.
   - Detail fields are written only while the detail toggle is on.
   - A heartbeat rewrites only the last line, so the file does not grow per checkpoint.
   - The reader skips malformed lines and reports how many.
   - The file rotates monthly if it passes a size threshold. "All" reads the rotated files too.
3. **Aggregator** is a Tauri command `history_summary(range)` returning totals by day (local midnight), mode, device and category, top apps and sites, and the 20 most recent segments. The frontend does no maths on raw lines.

Settings in `preferences.rs`: `historyEnabled` (default true) and `historyDetails` (default false). Turning `historyEnabled` off stops recording and keeps existing data.

`history_clear` deletes the history files and resets open segments.

## History tab

Top to bottom:

1. Range switch: Today, 7 days, 30 days, All.
2. Total time for the range with a small daily bar chart.
3. By mode: a horizontal bar per mode with time and share.
4. By device.
5. By content (only when detail data exists), with top apps and sites. A note says this is Windows-only and needs "Record details".
6. Recent sessions: time, device, mode and, with details on, app or track.
7. Settings block: Record listening history; Record apps, sites and tracks (applies going forward only); a line that history stays on this computer and is never uploaded; Clear history with a confirmation, because it cannot be undone.

The tab refreshes when opened, when the range changes, and about every 30 seconds while visible. Totals are labelled "connected time in each mode". With Follow media on, content numbers are playback time.

Empty state: "Nothing recorded yet. History starts when a headphone connects." On macOS and Linux the content section explains why it is unavailable.

It reuses the existing tab layout and the Follow media card styles, with no new design system.

## Edge cases and errors

- **Clock and sleep:** duration comes from a monotonic clock, and the wall-clock start is stored for display. Suspend closes the segment and resume opens a new one. Segments are capped at 12 hours.
- **Midnight:** a segment crossing local midnight is split by the aggregator, and the stored line stays whole.
- **Multiple devices:** each connected device has its own open segment.
- **Blips:** segments under 5 seconds are dropped.
- **Detail toggle:** read when a segment opens or changes, so it affects new segments only. Existing detail data stays until Clear history.
- **Write failure:** logged and shown once as "History couldn't be saved" on the tab. It never interrupts device control or Follow media.
- **Corrupt file:** bad lines are skipped and counted ("N entries skipped"). The file is never deleted automatically.

## Testing

- Rust unit tests: segment open, close and merge rules; midnight splitting; the 5-second and 12-hour rules; aggregation by mode, device and category; malformed-line tolerance; detail fields omitted when the toggle is off; clear; heartbeat rewriting only the last line; rotation.
- Playwright tests with a fixture summary: the History tab, range switching, the empty state, the Windows-only content note, the toggles and the Clear confirmation.
- Not verified: real headphones and real playback sessions.

## Decision log

| Decision | Choice | Alternatives considered | Why |
|---|---|---|---|
| Scope | Basic plus opt-in details | Basic only; everything on by default | Privacy-first, chosen by the user |
| Detail default | Off, separate toggle | On by default; everything opt-in | Track and site names are the sensitive part |
| Toggle semantics | Going forward only; Clear history deletes | Turning off also deletes detail data | Chosen by the user |
| Day boundary | Local midnight | UTC; user setting | Chosen by the user |
| Display | New History tab | Device-page summary; export | Chosen by the user |
| Storage | Append-only JSONL | SQLite (`rusqlite`); frontend localStorage | No new native dependency on a three-OS CI build; records with the window closed; plain format can migrate later |
| Listening time meaning | Connected time in a mode | Audio-level playback detection | App cannot detect audio without Follow media; labelled honestly |
| Day splitting | Aggregator splits at read time | Split when writing | Keeps stored data simple and timezone-independent |

## Implementation notes (differences from the design above)

- **Sampling, not hooks.** A backend thread samples connected devices and the Follow media snapshot every 5 seconds, so history needs no changes to device or media code. Durations are accurate to about 5 seconds.
- **Open segments** are checkpointed to `history-open.json` every minute and recovered into `history.jsonl` at the next start, instead of rewriting the last line of the log. The behaviour is the same: at most about a minute is lost on a crash.
- **Exact titles and URLs are a third, separate switch** ("Record exact titles and URLs", off by default, effective only while history and "Record apps and sites" are also on). Decided with the user: the full page address including the query, the song or video title with the artist, and the game's window title. It was added after the first build; "Record apps and sites" alone still saves only the playback type and the app or site name.
  - **Collection is gated at the source.** The desktop reads Windows media titles, game window titles and browser page details only while the switch is effectively on, and clears them when it is turned off. The browser companion (1.0.3) always sends the address and title of a *playing* tab on the local loopback connection; the desktop drops them on receipt unless the switch is on.
  - **Private windows are never recorded.** The companion sends an address only when the browser confirms the tab is not incognito, and sends nothing extra if it cannot check. Stop reports carry no address.
  - **Browser tabs** use the title the player publishes to Windows (the song) when there is one, otherwise the page title. Games use the foreground window title.
  - Addresses can contain tokens or search text. They are stored in plain text in `history.jsonl` on this computer, the same as the rest of the history, and Clear history deletes them.
- **Noise control is recorded as its own dimension** next to the listening mode, because Sony and OnePlus report them separately. The tab has a "By noise control" card.
- **OnePlus mode** is derived: Game mode, then spatial audio, otherwise not reported.
- **Settings** live in `history-settings.json`, separate from app preferences, with their own commands.
- **Rotation** is not built yet. At roughly 1 KB per listening hour it is not needed until the file reaches many megabytes.
- **Chart range:** the daily chart always spans at least 7 days so "Today" has context; totals still follow the selected range. "All time" charts the last 90 days.
- **Day boundaries** use the viewer's UTC offset at read time (`Date.getTimezoneOffset`), so a range that crosses a daylight-saving change can be off by an hour at its edge.

## Risks

- Aggregation scans the whole file. At roughly 1 KB per listening hour this stays fast for years, and rotation is the mitigation if it does not.
- Detail data is sensitive even locally. The tab states it stays on this computer, and Clear history deletes it.
- Time totals are an estimate of connected time, not proof of listening. The wording must not overclaim.
