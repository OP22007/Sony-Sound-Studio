# Follow media

Follow media is an optional Windows feature in the **Automatic listening** navigation tab.
It uses the existing device command queue and matched device readback. It starts off.
Connect one headphone with a validated Cinema option, then enable **Follow media**.
Expand **Browser companion setup** to prepare the bundled Chrome/Edge/Brave or
Firefox extension, open its folder, and copy the pairing key. The installed desktop
app includes these extension sources; a separate repository checkout is not needed.

## Rules

| Playback | Requested listening mode |
| --- | --- |
| Netflix app reporting Windows playback | Cinema |
| VLC reporting Video through its local status connection or Windows media sessions | Cinema |
| VLC reporting Music or an unknown playback type | Standard |
| Apple Music, Tidal and other desktop music sessions | Standard |
| Movie/show websites identified by the paired extension | Cinema |
| YouTube Music, Apple Music web, Tidal web and ordinary browser media | Standard |

Recognized movie/show hosts: Netflix, Prime Video, Disney+, Hulu, Max/HBO Max,
Apple TV, Peacock, Paramount+, JioHotstar and Hotstar. Subdomains are supported;
lookalike domain names do not match. Prime Video playback hosted on an Amazon
shopping domain is not classified as Cinema. Unknown sites use Standard.
This is a site rule, so previews and ads on a supported streaming site share its mode.

The extension is necessary for browser sites; Windows media sessions alone do not
reliably provide the site. Browsers without the extension do not trigger a Cinema
guess based on video metadata. Install and pair the companion using
[the extension instructions](../extensions/media-mode/README.md).

## Switching and manual control

- Playback must remain in the same category for three seconds before a switch.
- Brief pauses and stopped playback keep the current mode; they do not force a reset.
- When multiple browser tabs are playing, switching waits until only one remains.
- When multiple headphones are connected, automatic commands target the matched active
  Windows audio output. If output identity cannot be determined, exactly one connected
  headphone is required. A known speaker/non-headphone output never falls back to another headphone.
- A user-initiated listening-mode change pauses automation for the current app session.
- Discovery, connection checks, equalizer/noise-control changes, and readbacks do not
  create a manual hold. Commands rejected before entering the device queue do not
  create one either. An explicitly queued listening-mode choice pauses automation
  even if its device confirmation subsequently fails, preserving the user's intent.
- Use **Resume Follow media** to release the manual hold.
- Manual holds end when the app exits. Enabling Follow media or resuming also clears
  any hold immediately.
- Disabling automation leaves the current headphone mode unchanged.
- Queued automatic requests are invalidated before device I/O. A command already
  sent to the headphone may still finish; no speculative rollback command is sent.
- Failed or unconfirmed requests are reported and have a 30-second retry delay.

## Limits

Windows players must publish a system media session. VLC builds/settings that do
not publish one need the optional **VLC connection** below Follow media. The installed
VLC 3.0.24 was running while Windows reported zero VLC media sessions during the
2026-11-02 diagnostic. Its local Web interface was also not enabled.

To connect VLC, open its Tools → Preferences, select Show settings: All, enable
Interface → Main interfaces → Web, and set a dedicated password under Main interfaces
→ Lua. Bind HTTP to `127.0.0.1`, port `8080`. Save, then restart VLC when convenient;
View → Add Interface → Web can start the interface in the current session after
configuring the password. Enter that password in the app's VLC connection panel,
test it, then choose **Connect and save**. Testing alone does not enable monitoring.

To have the interface start each time VLC opens, close VLC and choose **Enable VLC
startup** in the app. This Windows-only opt-in action updates VLC's per-user `vlcrc` settings to
enable the HTTP interface on loopback at port 8080, using the saved dedicated password.
It preserves other preferences and interface modules and writes a uniquely named
backup beside the file. VLC must be closed so it cannot overwrite the update when it
exits. Start VLC again after setup. The password is stored in VLC's own preferences as
well as Sound Connect's local connection settings.

The connector requests only `GET /requests/status.json` with no VLC commands.
It polls every two seconds only when Follow media and the VLC connection are enabled.
The VLC response can contain media metadata; only playback state and stream type are
used, and titles/file paths are discarded. The dedicated password is saved locally
in `vlc-connection.json`, never in browser storage or diagnostics. Unknown VLC playback
types use Standard; file names and window titles are not inspected.

Browser companion 1.0.1 verifies pairing with the desktop before displaying Connected
and attaches to existing accessible tabs. Existing installations must reload the
extension after preparing the updated files. The HTTP receiver runs separately from
headphone commands, so a pending Bluetooth readback cannot block browser pairing.

**Atmos/spatial track detection is not implemented.** Windows' general media-session
metadata does not expose an Atmos flag. Apple Music and Tidal therefore use Standard,
including tracks whose source format cannot be verified. You can manually select
Cinema; that choice pauses automation. System spatial audio being enabled is not
treated as proof that the current track is Atmos.

The current integration is Windows-only. The controls report unavailable on macOS
and Linux; existing device controls are unaffected.

## Privacy and resource use

The desktop observes playback state, playback type and a transient player identifier.
The extension sends only its opaque tab identifier, hostname, playback state and
coarse category. Titles, full URLs, artwork, audio and listening history are not
collected or stored. The private pairing key and enabled preference are saved locally
in `media-automation.json`; pairing keys are not diagnostic-log entries.

Browser reports go to `127.0.0.1:47653`, with a 256-bit pairing key. The receiver binds
only while Follow media is enabled. Reports expire after 16 seconds. Playback events
wake the native observer, with a 30-second reconciliation check; the browser heartbeat
runs every five seconds only during playback. Disabling releases the media observer
and local receiver. Automation runs in Rust, including when the studio window is closed
to the tray.

## Validation

Implementation checks on Windows, 2026-11-02:

- 59 Rust unit/integration tests passed; five explicit OS/hardware smoke tests are
  excluded from the default run.
- The Windows media observer smoke test was run separately and passed. It initialized
  the actual OS media API and read a paused current session without accessing Bluetooth.
- Eight extension VM tests passed, including payload privacy, host classification,
  ordered play/pause requests, playback-only heartbeats, authenticated pairing,
  attachment to existing tabs, and duplicate-injection protection.
- A real isolated Edge runtime verified the pairing request and injection into an
  already-open tab. No existing browser profile was used.
- All 28 browser tests passed. Integration covers native opt-in, readback status, manual hold/resume,
  failed preference saves, extension preparation, and a 390-pixel layout.
- The production frontend and Windows executable compile successfully.

The VLC connection also has parser tests for actual stream-type layouts, audio versus
video, paused/stopped states, authentication failures, chunked HTTP, response limits,
and exclusion of media tags from video classification. A UI integration test confirms
that testing the connection does not enable monitoring or send headphone commands.

These checks do not establish real Netflix/VLC/Apple Music/Tidal playback-to-headphone
switching, DRM-site compatibility, Firefox runtime compatibility, or Atmos detection.
Those need separate playback/hardware validation. No headphone setting commands were
sent while implementing and checking this feature.
