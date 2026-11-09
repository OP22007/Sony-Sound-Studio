# COLLEXION listening modes

Implemented for capability-advertising Sony devices, including WH-1000XX
(1000X THE COLLEXION). Implementation and synthetic replay tests do not establish
physical hardware validation. A COLLEXION hardware test is still outstanding.

## Evidence

- [Sony mode overview](https://www.sony.com/electronics/support/articles/00399613)
- [Sony listening-mode guide](https://helpguide.sony.net/mdr/2998/v1/en/contents/H035_Switching_the_sound_according_to_the_content_being_played_or_the_usage_situation.html)
- Sony Sound Connect Android 13.3.0 base APK SHA-256:
  `58837826f5f145cd0ad9689a298d2bfe9702f4d51d6d4e67d41440175ec0892c`.
- Local research export: `.research/evidence/collexion-series.json` (ignored,
  not distributed). Classes/methods: `fn0.d0` capability decoder,
  `fn0.n0.b.b` state validator, `fn0.n0.h` state getter,
  `fn0.h.g` notification getter, `fn0.h1.b.h` setter,
  `vv.a.N` capability mapping, `f60.b.b` mode write mediator.

Sony documents Standard, Music, 360 Upmix for Cinema, 360 Upmix for Game,
and Background Music. Background Music has My Room, Living Room and Cafe.
Music/Cinema/Game suspend EQ, DSEE Ultimate and Android head tracking.
Sony also documents restrictions during calls and wired playback; this change
does not decode a separate call/cable availability status.

## Android protocol evidence

Support function `ec` advertises UPMIX_SERIES; audio inquiry is `0a`.
Inner payloads, before existing transport framing:

| Operation | Bytes |
| --- | --- |
| Capability GET | `e0 0a` |
| Capability reply | `e1 0a count itemIds... exclusiveCount exclusiveIds...` |
| State GET | `e6 0a` |
| State reply | `e7 0a itemId` |
| State SET | `e8 0a itemId` |
| State notification | `e9 0a itemId` |

Wire item IDs differ from UI order: NONE/Standard `00`, Cinema `01`,
Game `02`, Music `03`. Counted capability lists must have exact lengths,
known unique mode IDs and no trailing bytes. Standard disables upmix even
when the list only contains the nonzero modes.

Background Music retains its separate inquiry `03` or `09`, selected from
advertised capabilities. Its state includes enabled/disabled and room index
`00`–`02`; room selection is preserved across listening-mode changes.

## Desktop behavior

Discovery requires capability and state replies before exposing the new
series control. Device names alone do not enable features. Only advertised
mode choices are offered, with Background Music added after its own readback.

Entering Background Music first disables upmix; entering an upmix mode first
disables Background Music. Both states are reread before confirming the final
selection. ACK alone never confirms a setting. A mismatch or disconnect does
not trigger an automatic restoration command. Notifications track supported
hardware-button changes and reject unknown or incoherent transitional states.

The frontend shows distinct Music and Game scenes and the exact model artwork.
EQ and DSEE controls are suspended during Music/Cinema/Game; native write
handlers independently check the current upmix state. This change does not
guess a COLLEXION equalizer packet layout.

## Validation and hardware checklist

Codec tests cover enum IDs, malformed states and counted capability payloads.
Stateful native replays cover all five modes, ordering, preserved room,
restricted capabilities, notifications, ACK-only rejection, no restoration,
and EQ/DSEE guards. Browser fixtures cover pending versus confirmed selection,
five choices, artwork and EQ/DSEE suspension; they are simulated devices.

Before reporting hardware-confirmed support, test all five modes, all three
Background Music rooms, physical-button changes, idle stability, reconnect,
power-cycle state, EQ/DSEE restrictions, and call/wired-playback behavior on a
real COLLEXION. Record firmware, platform and actual audible results.
