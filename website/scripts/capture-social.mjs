import { chromium } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  page.on('console', message => { if (message.type() === 'warning' || message.type() === 'error') console.log(message.text()); });
  await page.goto('http://127.0.0.1:4321/', { waitUntil: 'networkidle' });
  await page.evaluate(async () => { await document.fonts.ready; await Promise.all([...document.images].map(image => image.decode().catch(() => {}))); });
  await page.screenshot({ path: fileURLToPath(new URL('../test-results/hero-desktop.png', import.meta.url)) });
  await page.setViewportSize({ width: 1200, height: 760 });
  await page.screenshot({ path: fileURLToPath(new URL('../public/images/social.png', import.meta.url)), clip: { x: 0, y: 0, width: 1200, height: 630 } });
  const diagnostics = await page.evaluate(() => ({
    scene: document.querySelector('#scene-status')?.textContent,
    canvasCount: document.querySelectorAll('#sound-stage canvas').length,
    horizontalOverflow: document.documentElement.scrollWidth > innerWidth,
    duplicateIds: [...document.querySelectorAll('[id]')].map(node => node.id).filter((id, i, ids) => ids.indexOf(id) !== i),
  }));
  console.log(JSON.stringify(diagnostics));
} finally { await browser.close(); }
