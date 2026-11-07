# Website verification

Verified locally on 6 November 2026:

- `npm test`: 15 Playwright Chromium tests passed. Coverage includes scroll transforms and pause/system-motion transitions, a single black Collexion hero, five viewport widths (320–1440px), keyboard interactions, navigation, model artwork, EQ curves, release links, reduced motion, unavailable WebGL, working WebGL initialization and motion remounting, and JavaScript-disabled content.
- `npm run build`: TypeScript checking and Vite production build passed.
- Final screenshot diagnostics: one mounted sound-field canvas, no horizontal overflow and no duplicate element IDs.
- Desktop and mobile compositions inspected in the in-app browser.
- `git diff --check`: passed for the website changes.

The headphone images are authentic vendor product photography. The interactive 3D geometry is an independently authored acoustic sculpture, not a headphone mesh. Website mode and EQ interactions are visual demonstrations only.

Automated graphics checks used Chromium software rendering; they do not establish frame-rate or energy use on physical phones. Real-device GPU testing remains outstanding.

Published to https://op22007.github.io/Sony-Sound-Studio/ using the prebuilt `gh-pages` branch. GitHub's Pages deployment succeeded. The source is in PR #6; protected-main CI must pass before merging. `node scripts/capture-live.mjs` checks the public page and saves screenshots under the ignored test-results directory.

## Device directory — 2026-11-07

- Shared registry validates 33 named models (three Sony profiles; 30 OnePlus names / 32 product IDs), all nine feature groups on Windows/macOS/Linux, evidence references and scoped hardware confirmations.
- Existing website suite: 21 passed. Device-directory suite: 7 passed after correcting local directory-index routing and the filter test locator.
- Directory tests cover combined filters, aliases/product IDs, all 33 static pages and exact-model image URLs, unresolved Nord readback, 320/390/1440px layouts and no-JavaScript navigation.
- Production build and generated GitHub-table consistency check passed. Production sitemap contains 35 URLs.
- Production-preview screenshots reviewed at desktop and mobile widths; browser filtering returned three Sony models with no page errors.
- Published directory, XM6 page and Nord Buds 3 Pro page each returned HTTP 200 from GitHub Pages.
- No new device hardware validation was performed. Unknown test dates/builds remain unknown; protocol research is not promoted to hardware confirmation. Source changes remain on the existing website pull request; prebuilt Pages publishing does not merge protected main.

## Artwork containment correction — 2026-11-07

Fixed intrinsic image sizing overflowing the fixed-height directory and model artwork panels. Added a regression check at 390, 1440 and 1722px; it passes. Production build passes. Reviewed fresh desktop/mobile screenshots showing the complete XM6 and XM5 silhouettes.

