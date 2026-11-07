# Compatibility evidence

Registry review: 2026-11-07. This is a review date, not a hardware test date. Original test dates and exact app builds were not preserved for the initial owner reports. No claim is expanded to another platform, firmware or model.

## Implementation

Sony release baseline: 0.1.3. Windows has native control paths. Linux has BlueZ/RFCOMM transport but lacks recorded model-specific hardware validation. macOS control connections are disabled; paired-device discovery and UI previews are separate from control support. Firmware transfer and factory reset are not implemented.

Sources: [platform notes](../../README.md), [0.1.3 release notes](../releases/v0.1.3.md). OnePlus work belongs to the development source and is not included in this release baseline.

## XM6 noise control

Owner testing on WH-1000XM6 firmware 3.1.5 on Windows confirmed all three NC/ASM modes, displayed confirmation and physical NC/AMB button tracking. See the [recorded schema and owner confirmation](../research/v2-ncasm-19-schema.md). This does not confirm ambient-level range, other device settings or other models.

## XM6 EQ

Project records report one successful EQ-band write and matching readback on the Windows XM6 baseline. [Preset research](../research/eq-presets.md) distinguishes screenshot-derived curves applied through custom EQ from native preset IDs. Full hardware/audio equivalence for every preset is not recorded. Registry status: Partially working.

## OnePlus catalog

The locally supplied HeyMelody 117.6.6 catalog contains 32 OnePlus product IDs representing 30 names, plus regional/bundled aliases. Its mappings informed the experimental adapter's noise-profile and EQ routes. Catalog presence is static evidence, not proof of retail availability or successful desktop operation. No APK, decompiled proprietary code or private capture is published here.

Some models have no established noise/EQ/bass mapping. Those cells remain Unknown; lack of a mapping does not establish that the hardware lacks a feature. Sony Cinema and OnePlus spatial controls, firmware transfer and reset are not implemented in this adapter. Custom profile operations and capability-gated settings still need model-specific hardware validation.

## Nord observation

Owner-supplied Windows diagnostics for OnePlus Nord Buds 3 Pro reported product/capability replies and a physical change from transparency to noise cancelling, alongside an unsupported/ambiguous noise-profile error. The resulting parser fix was built, but a successful post-fix owner retest is not recorded. Connection is Reported working; noise control remains a Known issue. Firmware and exact tested app version are unknown.

## Updating evidence

Attach model, firmware, OS version, app release/commit, test date, feature, requested value and observed result. Prefer matching device readback plus physical observation where applicable. Remove Bluetooth addresses, serial numbers, browser pairing keys and personal media metadata before submitting. Maintainers review reports; the website never automatically promotes an issue report to confirmed compatibility.
