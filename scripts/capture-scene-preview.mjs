import { chromium } from '@playwright/test';

// Records only the isolated browser preview. No native bridge or Bluetooth access.
const browser = await chromium.launch({ channel: 'msedge', headless: true });
const context = await browser.newContext({
  viewport: { width: 1440, height: 1200 },
  reducedMotion: 'no-preference',
  recordVideo: { dir: 'tests/artifacts/scene-video', size: { width: 1440, height: 1200 } },
});
const page = await context.newPage();
try {
  await page.goto('http://127.0.0.1:1420/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.waitForTimeout(1400);
  for (const label of ['Ambient sound', 'Off', 'Noise cancelling']) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await page.waitForTimeout(1200);
  }
  await page.getByRole('button', { name: /Background Music A little atmosphere/ }).click();
  await page.waitForTimeout(1400);
  for (const label of ['Living room', 'Cafe']) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await page.waitForTimeout(1100);
  }
  await page.getByRole('button', { name: /Cinema A wider soundstage/ }).click();
  await page.waitForTimeout(1400);
  await page.getByRole('slider', { name: '63 Hz', exact: true }).fill('10');
  await page.waitForTimeout(1000);
  await page.getByRole('slider', { name: '1 kHz', exact: true }).fill('8');
  await page.waitForTimeout(1000);
  await page.getByRole('slider', { name: '16 kHz', exact: true }).fill('10');
  await page.waitForTimeout(1500);
  const video = page.video();
  await page.close();
  await video.saveAs('tests/artifacts/cinematic-listening-preview.webm');
  console.log('tests/artifacts/cinematic-listening-preview.webm — isolated scene/mode/EQ motion preview');
} finally {
  await context.close();
  await browser.close();
}
