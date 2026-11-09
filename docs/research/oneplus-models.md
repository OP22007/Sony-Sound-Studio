# Experimental OnePlus model catalog

This source build recognizes all **32 OnePlus product IDs**, representing **30 model names and seven aliases**, from the supplied HeyMelody 117.6.6 compatibility configuration. Every entry is **hardware-unvalidated**. Inclusion in the phone app's configuration does not prove a product's retail availability, desktop compatibility or feature parity.

Names select a discovery path; the actual product-ID reply selects the configuration. Regional product IDs with the same name remain separate entries. The desktop uses Classic Bluetooth RFCOMM on Windows and Linux; macOS headphone control connections remain disabled.

| Model name in the supplied catalog | Product IDs | Additional bundled names | NC schema | EQ schema | Settings service |
| --- | --- | --- | --- | --- | --- |
| OnePlus Flow Buds | `067414` | OnePlus Nord Buds Laplace 4 | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Pro 4 | `067814` | OnePlus Buds Bohr | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Pro 3 | `064014` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Clip | `06E410` | OnePlus Open Nobel | — | Mapped | `0000079A…` |
| OnePlus Buds Pro 2 | `062014` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Pro 2R | `063414` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Pro 2（轻享版） | `063014` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Pro | `060C14` | — | Mapped | Mapped | `00001107…` |
| OnePlus Buds 4 | `065414` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds 3 | `063C14` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Ace 3 | `066414` | OnePlus Nord Buds Einstein 1 | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Ace 2 | `064C14` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Buds Z2 | `061014` | — | Mapped | Mapped | `00001107…` |
| OnePlus Buds Z | `060814`, `068814` | — | — | — | `00001107…` |
| OnePlus Buds Ace | `062814` | — | Mapped | Mapped | `00001107…` |
| OnePlus Nord Buds 4 Pro | `066814` | OnePlus Nord Buds Einstein 2 | Mapped | Mapped | `0000079A…` |
| OnePlus Nord Buds 4 | `067014` | OnePlus Nord Buds ohm EX | Mapped | Mapped | `0000079A…` |
| OnePlus Nord Buds 3 Pro | `064414` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Nord Buds 3 | `065014` | — | Mapped | Mapped | `0000079A…` |
| OnePlus Nord Buds 2 | `062414` | — | Mapped | Mapped | `00001107…` |
| OnePlus Buds 3V | `066054` | OnePlus Nord Curie DO | — | Mapped | `0000079A…` |
| OnePlus Buds V | `064814` | — | — | Mapped | `00001107…` |
| OnePlus Buds N | `061814` | — | — | Mapped | `00001107…` |
| OnePlus Nord Buds | `061414` | — | — | Mapped | `00001107…` |
| OnePlus Nord Buds 3r | `065854` | — | — | Mapped | `0000079A…` |
| OnePlus Nord Buds 2r | `062C14` | — | — | Mapped | `00001107…` |
| OnePlus Nord Buds CE | `061C14` | — | — | Mapped | `0000079A…` |
| OnePlus Buds | `060414`, `068414` | — | — | — | `00001107…` |
| OnePlus Bullets Wireless Z3 | `051014` | — | — | Mapped | `00001107…` |
| OnePlus BulletsWireless Z2 ANC | `050C14` | — | Mapped | Mapped | `00001107…` |

## What is enabled

- Firmware and battery reads when advertised and successfully parsed.
- Model-specific ANC/Transparency profiles, intersected with the device's live supported mask. The adapter never substitutes the Nord Buds 3 Pro index map for older models.
- Model-specific built-in EQ presets after a successful current-mode reply and an advertised EQ command. An unlisted current mode is shown as a disabled read-only choice, so it does not hide the model’s known presets.
- Custom EQ profiles use live frequency, range and saved-profile data. Existing curves can be selected, edited and renamed; creation uses the model’s original-app layout and requires a newly returned profile. Every write requires matching readback. No custom profile deletion is exposed.
- BassWave level and enablement on the 18 configured product IDs, subject to the live numeric and feature-switch capabilities. Slider limits come from the device; level writes preserve the live range and require fresh matching readback. [Protocol evidence](bass-controls.md).
- Wear detection and legacy or GameSound game-mode switches where configured, advertised and returned in live feature-status entries.
- Advertised battery and noise-control notifications. Each settings write requires a matching state readback; acceptance alone is not confirmation.

A dash means the supplied configuration has no mapped schema for that control family. It does not assert that the product lacks every related feature. New or unknown protocol shapes leave the associated control unavailable.

## Compatibility rules

The supplied configuration enables custom EQ for 25 product IDs. Five entries specify ten bands (31, 62, 125, 250, 500, 1000, 2000, 4000, 8000 and 16000 Hz); the remaining configured models use the original app’s six-band default (62, 250, 1000, 4000, 8000 and 16000 Hz) for creating profiles. Existing profiles always retain their actual returned frequencies and range. Configuration alone never enables a write: live command support and a successful profile-list reply are also required.

Version-dependent EQ entries from `equalizerModeCompat` and `equalizerModeByVersion` are stored separately. Because firmware-version ordering has not been independently established, such an entry is included only when the connected device currently reports that preset index. These include Hans Zimmer Soundscape Tuning on Buds Pro 2 variants and Clear & crisp on Buds Clip. The app does not unlock firmware-gated presets by guessing how to compare version strings.

Newer configured noise modes whose semantic category is not established can still use an exact labeled profile descriptor when supported; they must not be interpreted using another model's Off/ANC/Transparency indices. Unknown semantics do not create common noise-mode controls.

OnePlus-style headphone names outside this catalog can appear in discovery. If their product ID is unknown, the app reports that result and enables no settings. Sony control packets are never used as a fallback. Arbitrarily renamed Bluetooth devices without a recognizable model name may require restoring their original name for discovery.

Original Bullets models absent from this APK, future hardware, custom EQ deletion, gesture editing, spatial controls, multipoint management, firmware transfer and reset are not claimed as implemented. This catalog is broader experimental coverage for testing, not a claim that every OnePlus feature or every product ever made is supported.

## Testing and feedback

Pair with the operating system, open this source build and choose **Find headphones**. Connect one device, check identity/firmware/battery, then test each advertised NC profile and EQ preset individually. Test wear detection and game mode only if their controls appear. Record whether physical NC changes update the app.

Please report model name, firmware, operating system, app version, the exact control tested, audible/physical result and app confirmation or error. Include transport diagnostics with personal data removed. The existing 0.1.3 release predates this integration.

[Protocol evidence, artifact hashes and limitations](oneplus-heymelody.md).
