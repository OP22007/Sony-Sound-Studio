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


## Full Sony catalog — 2026-11-07

Expanded the registry to all 62 entries in Sony USA Sound Connect article 00346846 (September 2026 list): 47 headphones/earphones, two gaming models and 13 speakers. Together with 30 OnePlus catalog names, the directory contains 92 devices and the sitemap contains 94 URLs. Catalog-only Sony models retain Unknown Windows/Linux support and Not implemented macOS controls, using shared registry defaults. Added category and catalog-only filters. All 30 browser tests and the production build passed. Phone-app catalog inclusion does not establish desktop implementation or hardware validation.


## Exact-model artwork expansion — 2026-11-07

Added 76 visually reviewed, exact-model original image files. The directory now has artwork for all 62 Sony models and 29 of 30 OnePlus entries (91/92 total). OnePlus Flow Buds retains an explicit unavailable placeholder because its catalog identity could not be matched to authentic manufacturer artwork.

Manufacturer product pages, Japanese model-image catalogs and press assets supply 74 images. MDR-XB950B1 uses its exact-model John Lewis product listing; Buds 3V uses launch artwork reproduced by IT Home with its watermark retained. White Buds Pro 2R and the China light edition share identical manufacturer image bytes, explicitly assigned to those variants by the regional pages. Rejected color swatches, text layers and loading placeholders during visual review.

The provenance manifest records source page, original image URL, SHA-256, dimensions and review date for every new file. Generation checks hashes, minimum dimensions and model associations. Model pages link to their artwork source separately from device-feature evidence. Existing compatibility statuses are unchanged.

Production build, generated-table consistency and all 30 browser tests passed, including all 92 model pages/image URLs and mobile/desktop image containment. Production-preview directory screenshots and contact sheets were reviewed. Source changes stay on the website branch; publishing prebuilt Pages does not merge protected main.

## Owner compatibility admin — 2026-11-07

Added the separate `admin/` Vercel application with GitHub OAuth state/PKCE and a server-side account-ID check for OP22007 (140067318). Encrypted HttpOnly sessions, CSRF/origin checks, bounded feature edits and non-forced branch updates protect private routes and writes. No admin HTML or backend is included in the GitHub Pages build. Extracted the static compatibility renderer into a shared pure module so admin publication and website builds use the same output.

Owner reviews record per-model, per-platform, per-feature statuses and test evidence. Confirmation requires a complete hardware test context. GitHub save and Pages publication are explicit separate operations; original tests and shared profile defaults are preserved. Deployment requests are not presented as completed deployments.

19 Node tests passed for authentication denial, cookie tampering/expiry, OAuth state/PKCE, CSRF, confirmation requirements, markup escaping, atomic source commits, stale saves/publications and failed deployment requests. All 34 browser tests passed (four admin workflow/layout tests plus the existing 30 website checks); production build and generated-table consistency passed. Desktop/mobile editor screenshots were reviewed. OAuth/GitHub operations are mocked in automated tests. Live Vercel/OAuth validation remains pending the owner's project URL, OAuth app registration and environment secrets; no claim of completed online deployment is made.
