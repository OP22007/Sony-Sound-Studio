# Website verification

Verified locally on 6 November 2026:

- `npm test`: 15 Playwright Chromium tests passed. Coverage includes scroll transforms and pause/system-motion transitions, a single black Collexion hero, five viewport widths (320–1440px), keyboard interactions, navigation, model artwork, EQ curves, release links, reduced motion, unavailable WebGL, working WebGL initialization and motion remounting, and JavaScript-disabled content.
- `npm run build`: TypeScript checking and Vite production build passed.
- Final screenshot diagnostics: one mounted sound-field canvas, no horizontal overflow and no duplicate element IDs.
- Desktop and mobile compositions inspected in the in-app browser.
- `git diff --check`: passed for the website changes.

The headphone images are authentic vendor product photography. The interactive 3D geometry is an independently authored acoustic sculpture, not a headphone mesh. Website mode and EQ interactions are visual demonstrations only.

Automated graphics checks used Chromium software rendering; they do not establish frame-rate or energy use on physical phones. Real-device GPU testing and a live Pages/Vercel deployment remain unverified. No deployment was performed.
