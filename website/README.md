# Sound Studio product website

A separate static product site for the desktop app. Vite, TypeScript, Three.js and Anime.js; no server, database, accounts, analytics or native device access. The desktop app's build and dependencies are unchanged.

## Local development

```sh
cd website
npm ci
npm run dev
```

Open http://127.0.0.1:4321/. The desktop app continues to use port 1420.

```sh
npm run build
npx playwright install chromium
npm test
npm run preview
```

Production output is `website/dist`. Relative asset paths support both a GitHub project subdirectory and a Vercel root deployment. The HTML contains the actual headings, copy, compatibility notes, structured data and download links before JavaScript runs.

## GitHub Pages

1. In repository Settings → Pages choose **GitHub Actions** as the source.
2. Run **Product website** from the Actions tab using **Run workflow** on the branch containing this site.
3. The workflow builds, tests and deploys the static output to Pages. Pull requests affecting the website run checks without deployment.

The default canonical URL is `https://op22007.github.io/Sony-Sound-Studio/`. Change it if the repository name or domain changes. Creating the site locally does not publish it or change repository visibility.

## Vercel

Import the repository and set **Root Directory** to `website`. Build command: `npm run build`; output directory: `dist`; install command: `npm ci`. No Next.js or server function is required for the 3D scene or animations.

Set the environment variable `SITE_URL` to your final HTTPS domain, such as `https://your-domain.example/`, then rebuild. It supplies the canonical URL, social-image URL, robots sitemap location and sitemap entry. Update it when changing domains. Preview deployments should be configured not to compete with the production canonical domain.

## Content and asset maintenance

- Platform links intentionally lead to the verified project release page, not guessed installer filenames. Visitors choose the appropriate release asset there.
- Compatibility reflects the repository's current README. OnePlus/BassWave are source-build features; macOS currently has discovery but no control connection. Update these statements when a release changes that status.
- Hero and model images are the original vendor product renders already used by the app. Provenance and ownership are recorded in `public/CREDITS.txt` and the root `public/headphones/SOURCES.md`; they are not covered by the code's MIT license.
- The WebGL acoustic sculpture is independently authored procedural geometry. It is not an official Sony headphone mesh. Product photography remains visible while it loads, if it fails, or when JavaScript is unavailable.
- Manrope is self-hosted from `@fontsource-variable/manrope`, distributed under the SIL Open Font License in that package. No font-provider request is made at runtime.
- `public/images/studio-preview.png` is a genuine screenshot of the existing app's explicitly selected browser preview, not a staged connected-device claim. To refresh, run the desktop app's browser dev server on port1420, then `node scripts/capture-studio.mjs` from this directory.
- The tuning curves and sound-field modes on the website are explicitly visual demonstrations. They do not produce sound or write settings to a device.

## Motion and rendering

One Anime.js entrance sequence, intersection-triggered content reveals, interpolated EQ curves and a mode-reactive WebGL scene. Three.js is loaded separately from the essential page. The scene caps pixel density, pauses outside the viewport/in a background document, respects reduced motion, provides a manual pause and disposes resources. The page uses native scrolling. Phone GPU/frame-time performance still requires real-device testing.

Scrolling choreographs a separate product layer, counter-moving oversized typography, a rotating acoustic dial, the app preview unfolding in perspective, EQ depth and model parallax. Heading words enter in staggered Anime.js sequences. One event-scheduled animation frame batches layout reads and style writes; there is no permanent scroll render loop. System reduced motion and the footer pause button disable all scroll transforms.
