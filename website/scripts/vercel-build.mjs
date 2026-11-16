// Builds the site for Vercel with canonical URLs, the sitemap and the generated device pages
// pointing at the deployment's production domain instead of GitHub Pages.
import { spawnSync } from 'node:child_process';

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
