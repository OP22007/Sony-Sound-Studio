// Builds the site for Vercel with canonical URLs, the sitemap and the generated device pages
// pointing at the deployment's production domain instead of GitHub Pages.
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const production = process.env.VERCEL_PROJECT_PRODUCTION_URL;
const siteUrl = process.env.SITE_URL || (production ? `https://${production}/` : undefined);
if (!siteUrl) console.warn('No SITE_URL or VERCEL_PROJECT_PRODUCTION_URL: canonical URLs keep the GitHub Pages address.');
else console.log(`Building for ${siteUrl}`);

const env = { ...process.env, ...(siteUrl ? { SITE_URL: siteUrl } : {}) };
// Regenerate the device pages for this domain (the committed ones, and the GitHub table check in
// `npm run build`, are tied to the GitHub Pages address), then type-check and build.
for (const command of [['node', 'scripts/generate-devices.mjs'], ['npx', 'tsc', '--noEmit'], ['npx', 'vite', 'build']]) {
  const result = spawnSync(command[0], command.slice(1), { stdio: 'inherit', shell: process.platform === 'win32', env });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

// Vercel serves these two scripts itself once Web Analytics and Speed Insights are enabled for the project,
// so they are added only to the Vercel build (GitHub Pages and local runs would get 404s for them).
const snippet = '<script defer src="/_vercel/insights/script.js"></script><script defer src="/_vercel/speed-insights/script.js"></script>';
const pages = [];
const walk = dir => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path);
    else if (entry.name.endsWith('.html')) pages.push(path);
  }
};
walk('dist');
let tagged = 0;
for (const page of pages) {
  const html = readFileSync(page, 'utf8');
  if (html.includes('/_vercel/insights/') || !html.includes('</body>')) continue;
  writeFileSync(page, html.replace('</body>', `${snippet}</body>`));
  tagged++;
}
console.log(`Added Vercel Analytics and Speed Insights to ${tagged} of ${pages.length} pages`);
