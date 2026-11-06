// Captures only the browser's explicit illustrative preview; no native app or device connection.
import { chromium } from '@playwright/test';
import { fileURLToPath } from 'node:url';
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  await page.goto('http://127.0.0.1:1420/', { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Exit preview', exact: true }).waitFor();
  await page.evaluate(async () => { await document.fonts.ready; await Promise.all([...document.images].map(img => img.decode().catch(() => {}))); });
  await page.screenshot({ path: fileURLToPath(new URL('../public/images/studio-preview.png', import.meta.url)) });
  console.log('Saved actual app preview screenshot (1440 × 960).');
} finally { await browser.close(); }
