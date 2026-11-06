import { chromium } from '@playwright/test';

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 }, reducedMotion: 'reduce' });
  const response = await page.goto('https://op22007.github.io/Sony-Sound-Studio/', { waitUntil: 'networkidle' });
  // Offscreen lazy images need not finish before a hero screenshot.
  await page.evaluate(async () => { await document.fonts.ready; await document.querySelector('.hero-headphone').decode(); });
  await page.screenshot({ path: 'test-results/published-hero.png' });
  await page.locator('.scroll-overture').scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/published-scroll.png' });
  console.log(JSON.stringify({ status: response.status(), url: page.url(), hero: await page.locator('.hero-headphone').getAttribute('src'), overflow: await page.evaluate(() => document.documentElement.scrollWidth > innerWidth) }));
} finally { await browser.close(); }
