import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
});

test('scroll choreography moves the single black hero and becomes static when paused', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.goto('/');
  await expect(page.locator('.hero-visual img')).toHaveCount(1);
  await expect(page.locator('.hero-headphone')).toHaveAttribute('src', /collexion-black/);
  await expect(page.locator('html')).toHaveAttribute('data-scroll-motion', 'on');
  await page.evaluate(() => window.scrollTo({ top: 400, behavior: 'instant' }));
  await expect.poll(() => page.locator('.hero').evaluate(el => el.style.getPropertyValue('--hero-turn'))).not.toBe('0deg');
  await expect(page.locator('.scroll-product')).not.toHaveCSS('transform', 'none');
  await page.locator('#motion-toggle').click();
  await expect(page.locator('html')).toHaveAttribute('data-scroll-motion', 'off');
  await expect(page.locator('.scroll-product')).toHaveCSS('transform', 'none');
  await expect(page.locator('.studio-window')).toHaveCSS('transform', 'none');
  await page.getByRole('button', { name: 'Resume motion', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-scroll-motion', 'on');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(page.locator('html')).toHaveAttribute('data-scroll-motion', 'off');
});

for (const width of [320, 390, 768, 1024, 1440]) {
  test(`layout and original imagery remain usable at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 960 });
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto('/');
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Good headphones.Great company.');
    await page.evaluate(() => document.fonts.ready);
    const image = page.locator('.hero-headphone');
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate(img => (img as HTMLImageElement).naturalWidth)).toBe(1200);
    await page.locator('.studio-screenshot').scrollIntoViewIfNeeded();
    await expect.poll(() => page.locator('.studio-screenshot').evaluate(img => (img as HTMLImageElement).naturalWidth)).toBe(1440);
    await page.locator('#download').scrollIntoViewIfNeeded();
    await expect(page.locator('.platform-card')).toHaveCount(3);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    await page.screenshot({ path: `test-results/site-${width}.png`, fullPage: true });
    expect(errors).toEqual([]);
  });
}

test('mode choices update the explanation and remain visual-only', async ({ page }) => {
  await page.goto('/');
  const cinema = page.getByRole('button', { name: 'Cinema', exact: true });
  await cinema.focus();
  await page.keyboard.press('Enter');
  await expect(cinema).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#mode-caption')).toContainText('Native Cinema requires supported headphones');
  await page.getByRole('button', { name: 'Ambient', exact: true }).click();
  await expect(cinema).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('#mode-caption')).toContainText('unchanged');
});

test('tuning presets and keyboard bass adjustments change the illustrative curve', async ({ page }) => {
  await page.goto('/');
  const baseline = await page.locator('#curve').getAttribute('d');
  await page.getByRole('button', { name: 'Warm', exact: true }).click();
  const warm = await page.locator('#curve').getAttribute('d');
  expect(warm).not.toBe(baseline);
  const bass = page.getByRole('slider', { name: 'Visual demo bass level' });
  await bass.focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('#bass-value')).toHaveText('+1');
  expect(await page.locator('#curve').getAttribute('d')).not.toBe(warm);
  await expect(page.getByText('This web demo does not play audio or change your headphones.', { exact: false })).toBeVisible();
});

test('model explorer changes exact artwork and discloses unverified coverage', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'WH–1000XM5', exact: false }).click();
  await expect(page.locator('#model-name')).toHaveText('WH–1000XM5');
  await expect(page.locator('#model-status')).toContainText('VALIDATION NEEDED');
  await expect(page.locator('#model-image')).toHaveAttribute('src', /wh-1000xm5-black.png$/);
  await page.getByRole('button', { name: 'OnePlus', exact: false }).click();
  await expect(page.locator('#model-name')).toHaveText('Nord Buds 3 Pro');
  await expect(page.locator('#model-image')).toHaveAttribute('src', /oneplus-nord-buds-3-pro-mint.png$/);
  await expect(page.locator('#model-caveat')).toContainText('Not included in release 0.1.3');
  await expect(page.getByRole('button', { name: 'WH–1000XM6', exact: false })).toBeHidden();
  await page.getByRole('button', { name: 'Sony', exact: true }).click();
  await expect(page.locator('#model-name')).toHaveText('WH–1000XM6');
});

test('mobile navigation supports keyboard opening, link navigation and Escape', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  const menu = page.locator('.menu-toggle');
  await menu.focus();
  await page.keyboard.press('Enter');
  await expect(menu).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Escape');
  await expect(menu).toHaveAttribute('aria-expanded', 'false');
  await expect(menu).toBeFocused();
  await menu.click();
  await page.getByRole('navigation').getByRole('link', { name: 'Compatibility' }).click();
  await expect(page).toHaveURL(/#compatibility$/);
  await expect(menu).toHaveAttribute('aria-expanded', 'false');
});

test('downloads link to real releases and disclose platform limitations', async ({ page }) => {
  await page.goto('/');
  for (const link of await page.locator('.platform-card').all()) await expect(link).toHaveAttribute('href', 'https://github.com/OP22007/Sony-Sound-Studio/releases/latest');
  await expect(page.locator('.platform-card').filter({ hasText: 'macOS' })).toContainText('Headphone control is not enabled');
  await page.locator('summary').filter({ hasText: 'Is this the official Sony app?' }).click();
  await expect(page.locator('details[open]')).toContainText('unaffiliated');
});

test('reduced motion and the explicit pause control keep the page usable', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-motion', 'paused');
  await expect(page.getByRole('button', { name: 'Motion reduced by system', exact: true })).toBeDisabled();
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await expect(page.locator('html')).toHaveAttribute('data-motion', 'running');
  await page.getByRole('button', { name: 'Pause motion', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-motion', 'paused');
});

test('unavailable WebGL retains original product art and working mode buttons', async ({ page }) => {
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (type: string, ...args: unknown[]) {
      if (type.startsWith('webgl')) return null;
      return original.call(this, type as '2d', ...args as []) as never;
    } as typeof original;
  });
  await page.goto('/');
  await expect(page.locator('#scene-status')).toHaveText('PRODUCT PHOTOGRAPHY / STATIC VIEW');
  await expect(page.locator('.hero-headphone')).toBeVisible();
  await page.getByRole('button', { name: 'Cinema', exact: true }).click();
  await expect(page.locator('#mode-caption')).toContainText('Cinema');
});

test('the normal 3D path mounts successfully and survives mode changes and motion remounts', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.goto('/');
  await expect(page.locator('#sound-stage canvas')).toHaveCount(1);
  await expect(page.locator('.hero-visual')).toHaveClass(/scene-ready/);
  await page.getByRole('button', { name: 'Cinema', exact: true }).click();
  await expect(page.locator('#scene-status')).toContainText('3D SOUND FIELD');
  await page.getByRole('button', { name: 'Pause motion', exact: true }).click();
  await expect(page.locator('#sound-stage canvas')).toHaveCount(1);
  await expect(page.locator('#scene-status')).toContainText('STILL');
  await page.getByRole('button', { name: 'Resume motion', exact: true }).click();
  await expect(page.locator('#sound-stage canvas')).toHaveCount(1);
  await expect(page.locator('#scene-status')).toContainText('MOVE TO EXPLORE');
});

test('JavaScript-disabled mobile visitors can read the site and use download navigation', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  await page.goto('http://127.0.0.1:4321/');
  await expect(page.getByRole('navigation').getByRole('link', { name: 'Get the preview' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await page.getByRole('link', { name: 'Meet your new studio', exact: true }).click();
  await expect(page).toHaveURL(/#download$/);
  await expect(page.locator('.platform-card')).toHaveCount(3);
  await context.close();
});
