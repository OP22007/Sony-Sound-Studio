# MDR V2 multipoint: paired-device management

Static, read-only analysis of Sony Sound Connect 13.3.0 (`base.apk` SHA-256
`58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`), decompiled with `scripts/jadx-dump`.
The APK was never executed. **Nothing here has been exercised on a headphone.** A change is shown as done
only after the headphone's own list reports it.

## Where it lives

This is message **table 2**, carried in frames with data type `0x0e` (`DataType.DATA_MDR_NO2`). It is
separate from the "Multipoint" on/off general setting (a table 1 general setting, see
[v2-system-settings.md](v2-system-settings.md)). Support is advertised twice:

- The protocol-info reply `[01 00 version(4) table1 table2]` (`in0.k`): byte 7 is `EnableDisable`
  (`0` = enable) and says the headphone answers table 2. The desktop only asks table 2 questions of a
  headphone that says so, and with a 1.5 second deadline.
- The table 2 support-function request `[06 00]` on `0x0e` returns `[07 00 count (function, version)*]`.
  Pairing-device management is function `0x30` (`PAIRING_DEVICE_MANAGEMENT_CLASSIC_BT`, inquiry type 0)
  or `0x32` / `0x33` (with Bluetooth class of device, inquiry type 2).

## Commands (`PERI_*`, codes `0x30` to `0x3d`)

| Step | Bytes | Source |
| --- | --- | --- |
| GET_CAPABILITY | `[30, type]` -> `[31, type, a, b, fileTransfer]`; `b` is the **connection limit** (the Sony app refuses to start pairing once the number of connected devices reaches it: `MULTIPOINT_MAXIMUM_DEVICES_ARE_CONNECTED`, `MultipointDeviceSettingsViewModel`, `r30.b`); `a` is logged only | `oo0.b`, `oo0.p` |
| GET_PARAM | `[36, type]` | `oo0.c` |
| RET_PARAM, type 0 | `[37, 00, count, (address[17 ASCII], status, nameLen, name)*, playbackSlot]` | `oo0.s` |
| RET_PARAM, type 2 | `[37, 02, count, (address[17], status, classOfDevice[3], nameLen, name)*, playbackSlot]` | `oo0.t`, `po0.b` |
| NTFY_PARAM | `[39, type, ...]`, same layout as RET_PARAM | `oo0.h` |
| SET_EXTENDED_PARAM | `[3c, type, action, address[17 ASCII]]`, action `0` disconnect, `1` connect, `2` unpair | `oo0.x`, `oo0.y`, `ConnectivityActionType` |

`status` above zero means connected and is the device's **connection slot** (1, 2, ...). The last byte is the
**slot number that holds playback right**, not a position in the list: the Sony app marks the device whose status
equals it (`aVar.f() == kVarM.d()` in `n1`). Reading it as a list index marks the wrong device. Names are 1 to 128 bytes. Class of device lets the screen show a laptop, phone or tablet.

## What the desktop does

- Shows every paired device with its connected state, and which one plays audio.
- Connect and disconnect send one `SET_EXTENDED_PARAM`, then poll the list for up to 20 seconds. The change
  is confirmed only when the list shows the requested state. If it never does, the list is refreshed and an
  error says so; no second request or rollback is sent.
- A device already in the requested state, an address that is not in the list, or a malformed address is
  refused before anything is sent.
- A changed list announced by the headphone (another phone connecting) updates the screen live. Replies
  that do not match the advertised layout are ignored.
- **At the connection limit, connecting a device first disconnects another.** The one dropped is a connected
  device that is not the one playing audio (the device in the headphone's playback slot), is not **this computer** and is
  not the target. This computer is recognised by its own Bluetooth radio address (Windows
  `BluetoothGetRadioInfo`), so the app never cuts its own link. Where that address is unavailable, other
  computers are left alone too. The disconnection must be confirmed by the list before the connect request is
  sent; if it is not confirmed the new device is never connected. If nothing can be dropped safely the change is
  refused with an explanation. With an unknown limit nothing is ever dropped.
- **Unpairing is not offered.** It is destructive and cannot be undone from here.

## Not covered

Source switching (`SOURCE_SWITCH_CONTROL`, inquiry type 1), the music hand-over setting (type 3), the limits
in the capability reply, and unpairing. How many devices may be connected at once is the headphone's
decision; the desktop does not enforce a count.
