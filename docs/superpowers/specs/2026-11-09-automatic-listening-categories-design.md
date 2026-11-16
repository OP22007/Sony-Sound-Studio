# Automatic listening by content category

Date: 2026-11-09. Status: approved in conversation; implemented on `feature/automatic-listening-categories`.

## Goal

Follow media chooses settings from what is playing and what the connected device supports, instead of only Cinema or Standard on Sony. Sony upmix-series models (for example 1000X COLLEXION) switch between Standard, Cinema, Game and Music. OnePlus earbuds use spatial audio and Game mode where the model advertises them.

## Content categories

The automation produces one category: `movie`, `music`, `game` or `other`, plus a source label.

- `movie`: unchanged rules (Netflix app, VLC video, movie-site list).
- `music`: desktop music sessions (Spotify, Apple Music, Tidal and any non-video session) and extension sites classified `music` (YouTube Music, Apple Music web, Tidal web, Spotify web, SoundCloud, Deezer, Amazon Music).
- `game`: new Windows detector (read-only). A game is a foreground app that Windows reports as running a full-screen Direct3D application (`SHQueryUserNotificationState`), or a foreground process whose executable matches a built-in list of known games and launchers. Browsers, media players and presentation apps are excluded from the full-screen rule.
- `other`: any other playback.
- Priority: game > movie/music from media sessions. Game wins over background music. When the game ends, the category returns to the playing media.
- The existing 3-second stability window, multi-tab ambiguity rule and VLC conflict rule are unchanged.

## Per-device mapping

`listening_policy::desired_settings(device, category)` is a pure function over the device's confirmed `controlDescriptors` and settings. It returns the settings to apply, in order.

| Category | Sony upmix series | OnePlus (only where advertised) |
| --- | --- | --- |
| movie | listeningMode `cinema` | spatialAudio fixed (1), gameMode off |
| music | listeningMode `music` | spatialAudio off (0), gameMode off |
| game | listeningMode `game` | gameMode on; gameSoundProfile 3 if offered |
| other | listeningMode `standard` | spatialAudio off, gameMode off |

- A Sony mode the device does not offer falls back to `standard`, or nothing if that is also unavailable.
- A OnePlus setting that is not advertised, or whose option is unavailable, is skipped. Head tracking is never selected automatically.
- Settings are applied one at a time through `apply_setting_internal` with the automation epoch; each counts only after device readback. The first failure stops the rest and is reported. No rollback command is sent.
- A user change to `listeningMode`, `spatialAudio` or `gameMode` creates the manual hold, as listening mode does today.
- Failed or unconfirmed requests keep the 30-second retry delay.

## UI and docs

- The Automatic listening tab shows the detected category and source, and the settings it will apply to the connected device.
- `docs/media-following.md` lists the new rules, game detection and limits.

## Testing and limits

- Rust unit tests: category priority, Sony and OnePlus mapping on fixtures, fallbacks, skipped unadvertised settings.
- Playwright: status text for each category.
- The Windows game detector sits behind a trait so tests use a fake. It needs validation with real games; borderless-windowed games that Windows does not report as full-screen D3D are detected only through the executable list.
