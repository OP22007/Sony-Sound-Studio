import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

const registry = JSON.parse(readFileSync(new URL('../../docs/compatibility/devices.json', import.meta.url), 'utf8'));

test('directory filters combine model aliases, brand, platform, status and build', async ({ page }) => {
  await page.goto('/devices/');
  await expect(page.locator('.device-card:visible')).toHaveCount(registry.devices.length);
  await page.getByRole('combobox', { name: 'Brand', exact: true }).selectOption('Sony');
  await page.getByRole('combobox', { name: 'Feature status', exact: true }).selectOption('confirmed');
  await expect(page.locator('.device-card:visible')).toHaveCount(1);
  await expect(page.locator('.device-card:visible h2')).toHaveText('WH-1000XM6');
  await page.getByRole('combobox', { name: 'Platform', exact: true }).selectOption('macos');
  await expect(page.locator('.empty-state')).toBeVisible();
  await page.getByRole('button', { name: 'Reset filters' }).click();
  await expect(page.locator('.device-card:visible')).toHaveCount(registry.devices.length);
  await page.getByLabel('Find a model').fill('064414');
  await expect(page.locator('.device-card:visible h2')).toHaveText('OnePlus Nord Buds 3 Pro');
  await page.getByLabel('Find a model').fill('Laplace');
  await expect(page.locator('.device-card:visible h2')).toHaveText('OnePlus Flow Buds');
  await page.getByLabel('Find a model').fill('');
  await page.getByRole('combobox', { name: 'Availability', exact: true }).selectOption('development');
  await expect(page.locator('.device-card:visible')).toHaveCount(30);
  await page.getByRole('combobox', { name: 'Brand', exact: true }).selectOption('Sony');
  await expect(page.locator('#result-count')).toHaveText('0 models');
});

test('the full Sony phone-app catalog is separate from desktop support', async ({ page }) => {
  expect(registry.devices.filter((m: any) => m.brand === 'Sony')).toHaveLength(62);
  await page.goto('/devices/');
  await page.getByRole('combobox', { name: 'Brand', exact: true }).selectOption('Sony');
  await expect(page.locator('.device-card:visible')).toHaveCount(62);
  await page.getByRole('combobox', { name: 'Category', exact: true }).selectOption('speaker');
  await expect(page.locator('.device-card:visible')).toHaveCount(13);
  await page.getByRole('combobox', { name: 'Category', exact: true }).selectOption('gaming');
  await expect(page.locator('.device-card:visible')).toHaveCount(2);
  await page.getByRole('combobox', { name: 'Category', exact: true }).selectOption('');
  await page.getByRole('combobox', { name: 'Availability', exact: true }).selectOption('catalog');
  await expect(page.locator('.device-card:visible')).toHaveCount(59);
  await page.goto('/devices/wh-1000xm4/');
  await expect(page.locator('.scope-note')).toContainText('Desktop support is unknown');
  await expect(page.locator('tbody .status-confirmed')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Official source' })).toHaveAttribute('href', 'https://www.sony.com/electronics/support/articles/00346846');
});

test('every registry model has a static page, complete feature matrix and correct artwork', async ({ page, request }) => {
  const ids = registry.devices.flatMap((model: any) => model.productIds);
  expect(new Set(ids).size).toBe(32);
  for (const model of registry.devices) {
    const response = await request.get(`/devices/${model.id}/`);
    expect(response.status(), model.name).toBe(200);
    const html = await response.text();
    expect(html).toContain(`<h1>${model.name}</h1>`);
    expect(html).toContain(`devices/${model.id}/`);
    expect((html.match(/<th scope="row">/g) || []).length).toBe(9);
    if (model.artwork) {
      expect(html).toContain(`../../images/${model.artwork}`);
      expect((await request.get(`/images/${model.artwork}`)).status()).toBe(200);
    } else expect(html).toContain('Exact-model artwork not available');
  }
  await page.goto('/devices/wh-1000xm6/');
  await expect(page.locator('h1')).toHaveText('WH-1000XM6');
  await expect(page.locator('tr').filter({ has: page.getByRole('rowheader', { name: 'Noise control & transparency', exact: true }) })).toContainText('Confirmed working');
  await expect(page.locator('.test-context')).toContainText('3.1.5');
  await expect(page.locator('.test-context')).toContainText('Not recorded');
  await page.getByRole('link', { name: 'Report your test results' }).click({ trial: true });
  const report = new URL((await page.getByRole('link', { name: 'Report your test results' }).getAttribute('href'))!);
  expect(report.searchParams.get('body')).toContain('Model: WH-1000XM6');
});

test('Nord report preserves unresolved readback and development-only availability', async ({ page }) => {
  await page.goto('/devices/oneplus-nord-buds-3-pro/');
  const noise = page.locator('tr').filter({ has: page.getByRole('rowheader', { name: 'Noise control & transparency', exact: true }) });
  await expect(noise).toContainText('Known issue');
  await expect(noise).toContainText('successful post-fix retest is not recorded');
  await expect(page.locator('.build-label')).toHaveText('Development source only');
  await expect(page.locator('.scope-note')).toContainText('Not included in the 0.1.3 release baseline');
  await expect(page.locator('#evidence-nord-observation')).toContainText('owner report');
});

for (const width of [320, 390, 1440]) {
  test(`directory and model pages fit ${width}px with readable table scrolling`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    for (const path of ['/devices/', '/devices/wh-1000xm6/']) {
      await page.goto(path);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await expect(page.locator('h1')).toBeVisible();
      if (path.endsWith('xm6/')) await expect(page.getByRole('region', { name: 'WH-1000XM6 feature compatibility' })).toHaveAttribute('tabindex', '0');
    }
  });
}

test('directory and detailed compatibility remain available without JavaScript', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 900 } });
  const page = await context.newPage();
  await page.goto('/devices/');
  await expect(page.locator('.device-card')).toHaveCount(registry.devices.length);
  await expect(page.locator('.filters')).toBeHidden();
  await page.locator('.device-card').filter({ hasText: 'WH-1000XM6' }).getByRole('link').click();
  await expect(page.locator('h1')).toHaveText('WH-1000XM6');
  await expect(page.locator('tbody tr')).toHaveCount(9);
  await context.close();
});

test('product artwork fits its panel instead of overflowing and being clipped', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  for (const width of [390, 1440, 1722]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/devices/');
    for (const image of await page.locator('.card-art img').all()) {
      const bounds = await image.evaluate(img => {
        const panel = img.parentElement!.getBoundingClientRect();
        const rect = img.getBoundingClientRect();
        return { fits: rect.top >= panel.top - 1 && rect.bottom <= panel.bottom + 1 && rect.left >= panel.left - 1 && rect.right <= panel.right + 1 };
      });
      expect(bounds.fits).toBe(true);
      await expect(image).toHaveCSS('object-fit', 'contain');
    }
    await page.goto('/devices/wh-1000xm6/');
    const dimensions = await page.locator('.model-art img').evaluate(img => ({ image: img.getBoundingClientRect().height, panel: img.parentElement!.getBoundingClientRect().height }));
    expect(dimensions.image).toBeLessThanOrEqual(dimensions.panel);
  }
});
