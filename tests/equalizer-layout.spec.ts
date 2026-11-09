import { expect, test } from '@playwright/test';

for (const width of [1440, 960, 390]) test(`equalizer curve aligns with all ten reachable bands at ${width}px`, async ({ page }) => {
  await page.setViewportSize({ width, height: 960 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const sliders = page.locator('.equalizer-workspace .eq-graph-handle');
  await expect(sliders).toHaveCount(10);
  const positions = await page.locator('.equalizer-workspace').evaluate(panel => {
    const curve = [...panel.querySelectorAll('.eq-point')].map(node => node.getBoundingClientRect());
    return [...panel.querySelectorAll('.band')].map((node, i) => {
      const box = node.getBoundingClientRect();
      return { center: box.x + box.width / 2, point: curve[i].x + curve[i].width / 2, left: box.left, right: box.right };
    });
  });
  for (const band of positions) {
    expect(band.left).toBeGreaterThanOrEqual(0);
    expect(band.right).toBeLessThanOrEqual(width);
    expect(Math.abs(band.center - band.point)).toBeLessThan(2);
  }
  await page.locator('.eq-curve').scrollIntoViewIfNeeded();
  await expect(page.getByRole('slider', { name: '16 kHz', exact: true })).toBeInViewport();
  await page.getByRole('slider', { name: '16 kHz', exact: true }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('slider', { name: '16 kHz', exact: true })).toHaveAttribute('aria-valuenow', '1');
  await page.screenshot({ path: `tests/artifacts/equalizer-page-${width}-dark.png`, fullPage: true });
  await page.getByRole('button', { name: 'Light theme', exact: true }).click();
  await page.screenshot({ path: `tests/artifacts/equalizer-page-${width}-light.png`, fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

test('empty equalizer workspace offers discovery and explicit preview', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Find your headphones first.', exact: true })).toBeVisible();
  await expect(page.locator('.eq-panel')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Find headphones', exact: true }).last()).toBeEnabled();
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider', { name: '31 Hz', exact: true })).toBeEnabled();
});
