import { expect, test, type Page } from '@playwright/test';

async function enterPreview(page: Page) {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Exit preview', exact: true })).toBeVisible();
  await expect(page.getByText('changes stay on this computer', { exact: false })).toBeVisible();
}

async function openEqualizer(page: Page) {
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
}

test('first launch does not invent a connected headphone and browser discovery explains native requirement', async ({ page }) => {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByText('No headphones yet', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Exit preview', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Find headphones', exact: true }).last().click();
  await expect(page.getByRole('alert')).toContainText(/desktop app/i);
  await expect(page.getByText('No headphones yet', { exact: true })).toBeVisible();
  await page.screenshot({ path: 'tests/artifacts/first-launch.png', fullPage: true });
});

test('preview is explicit and exits back to the actual empty device state', async ({ page }) => {
  await enterPreview(page);
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Select WH-1000XM5', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Exit preview', exact: true }).click();
  await expect(page.getByText('No headphones yet', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveCount(0);
});

test('Equalizer has its own tab; listening and noise controls share the Sound studio deck responsively', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await enterPreview(page);
  const soundTab = page.getByRole('button', { name: 'Sound studio', exact: true });
  const equalizerTab = page.getByRole('button', { name: 'Equalizer', exact: true });
  const soundView = async () => {
    await soundTab.click();
    const listening = page.locator('.listening-studio');
    const noise = page.locator('.noise-panel');
    await expect(listening).toBeVisible();
    await expect(noise).toBeVisible();
    await expect(page.getByLabel('Equalizer preset', { exact: true })).toHaveCount(0);
    await expect(page.getByRole('switch', { name: 'Sound enhancement', exact: true })).toHaveCount(0);
    return { listening, noise };
  };
  const assertSideBySide = async () => {
    const { listening, noise } = await soundView();
    const a = await listening.boundingBox();
    const b = await noise.boundingBox();
    expect(a).not.toBeNull();
    expect(b).not.toBeNull();
    expect(Math.min(a!.y + a!.height, b!.y + b!.height)).toBeGreaterThan(Math.max(a!.y, b!.y));
    expect(a!.x + a!.width <= b!.x || b!.x + b!.width <= a!.x).toBe(true);
  };

  await assertSideBySide();
  await page.screenshot({ path: 'tests/artifacts/equalizer-layout-desktop-dark.png', fullPage: true });
  await page.getByRole('button', { name: 'Light theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await assertSideBySide();
  await page.screenshot({ path: 'tests/artifacts/equalizer-layout-desktop-light.png', fullPage: true });

  await page.setViewportSize({ width: 390, height: 900 });
  await page.getByRole('button', { name: 'Dark theme', exact: true }).click();
  const stack = await soundView();
  const listeningBox = await stack.listening.boundingBox();
  const noiseBox = await stack.noise.boundingBox();
  expect(listeningBox).not.toBeNull();
  expect(noiseBox).not.toBeNull();
  expect(noiseBox!.y >= listeningBox!.y + listeningBox!.height || listeningBox!.y >= noiseBox!.y + noiseBox!.height).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: 'tests/artifacts/equalizer-layout-mobile-dark.png', fullPage: true });
  await page.getByRole('button', { name: 'Light theme', exact: true }).click();
  await soundView();
  await page.screenshot({ path: 'tests/artifacts/equalizer-layout-mobile-light.png', fullPage: true });

  await equalizerTab.click();
  await expect(page.getByRole('region', { name: 'Equalizer workspace', exact: true })).toBeVisible();
  await expect(page.getByRole('switch', { name: 'Sound enhancement', exact: true })).toBeVisible();
  const preset = page.getByLabel('Equalizer preset', { exact: true });
  await preset.selectOption({ label: 'Heavy' });
  await expect(page.getByRole('slider', { name: '31 Hz', exact: true })).toHaveValue('4');
  await soundTab.click();
  await expect(page.getByLabel('Equalizer preset', { exact: true })).toHaveCount(0);
  await equalizerTab.click();
  await expect(preset).toHaveValue('sony:Heavy');
});

test('noise and EQ preview changes are independent for each model and keyboard operable', async ({ page }) => {
  await enterPreview(page);
  const ambient = page.getByRole('button', { name: 'Ambient sound', exact: true });
  await ambient.click();
  await expect(ambient).toHaveAttribute('aria-pressed', 'true');
  await openEqualizer(page);
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Off' });
  const bassBand = page.getByRole('slider', { name: '63 Hz', exact: true });
  await expect(bassBand).toHaveValue('0');
  await bassBand.focus();
  await page.keyboard.press('ArrowRight');
  await expect(bassBand).toHaveValue('1');
  await expect(page.getByLabel('Equalizer preset', { exact: true })).toHaveValue('Custom');
  await page.getByRole('button', { name: 'Select WH-1000XM5', exact: true }).click();
  await page.getByRole('button', { name: 'Sound studio', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Noise cancelling', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Select WH-1000XM6', exact: true }).click();
  await expect(ambient).toHaveAttribute('aria-pressed', 'true');
  await openEqualizer(page);
  await expect(bassBand).toHaveValue('1');
  await page.getByRole('button', { name: 'Reset equalizer', exact: true }).click();
  await expect(bassBand).toHaveValue('0');
  await expect(page.getByLabel('Equalizer preset', { exact: true })).toHaveValue('sony:Off');
  await page.screenshot({ path: 'tests/artifacts/sound-studio-dark.png', fullPage: true });
});

test('appearance persists and preview never becomes an automatic real connection after reload', async ({ page }) => {
  await enterPreview(page);
  await page.getByRole('button', { name: 'Light theme', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Dark theme', exact: true })).toBeVisible();
  await page.screenshot({ path: 'tests/artifacts/sound-studio-light.png', fullPage: true });
  await page.reload({ waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('button', { name: 'Dark theme', exact: true })).toBeVisible();
  await expect(page.getByText('No headphones yet', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Exit preview', exact: true })).toHaveCount(0);
});

test('all workspace sections are reachable and disclose feature limitations', async ({ page }) => {
  await enterPreview(page);
  await page.getByRole('button', { name: 'Automatic listening', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Automatic listening.', exact: true })).toBeVisible();
  await expect(page.getByRole('switch', { name: 'Follow media', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'In tune with your day.', exact: true })).toBeVisible();
  await expect(page.getByText('Preview only', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Feature coverage', exact: true }).click();
  await expect(page.getByText(/does not yet claim full Sony Sound Connect parity/)).toBeVisible();
  await page.getByRole('button', { name: 'Services & intelligence', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Services & intelligence', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Privacy & diagnostics', exact: true })).toBeVisible();
  const motion = page.getByRole('switch', { name: 'Reduce motion', exact: true });
  await expect(motion).toHaveAttribute('aria-checked', 'false');
  await expect(motion).toBeEnabled();
  await expect(page.getByText('Your system requests reduced motion; the studio is respecting it.')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-reduced-motion', 'true');
  await motion.click();
  await expect(motion).toHaveAttribute('aria-checked', 'true');
  await motion.click();
  await expect(motion).toHaveAttribute('aria-checked', 'false');
  await expect(page.locator('html')).toHaveAttribute('data-reduced-motion', 'true');
});

for (const width of [800, 390]) test(`narrow ${width}px viewport keeps primary navigation and controls reachable`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  await enterPreview(page);
  await expect(page.getByRole('button', { name: 'Noise cancelling', exact: true })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 1);
  expect(overflow).toBe(false);
  if (width === 390) {
    const copy = await page.locator('.hero-copy').boundingBox();
    const art = await page.locator('.sound-visual').boundingBox();
    expect(copy).not.toBeNull();
    expect(art).not.toBeNull();
    expect(art!.y).toBeGreaterThanOrEqual(copy!.y + copy!.height - 1);
  }
  await page.getByRole('button', { name: 'Feature coverage', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Know what’s connected.', exact: true })).toBeVisible();
  await page.screenshot({ path: `tests/artifacts/studio-narrow-${width}.png`, fullPage: true });
});

test('preview interactions produce no unhandled page errors', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await enterPreview(page);
  await openEqualizer(page);
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Clear' });
  await page.getByRole('button', { name: 'Select WH-CH720N', exact: true }).click();
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  await page.getByRole('button', { name: 'Exit preview', exact: true }).click();
  expect(errors).toEqual([]);
});

test('custom EQ preset saves locally, survives reload and can be deleted', async ({ page }) => {
  await enterPreview(page);
  await openEqualizer(page);
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Heavy' });
  await page.getByRole('button', { name: 'Save preset', exact: true }).click();
  await page.getByLabel('Preset name', { exact: true }).fill('Evening session');
  await page.locator('form').getByRole('button', { name: 'Save preset', exact: true }).click();
  await expect(page.getByLabel('Equalizer preset', { exact: true })).toHaveValue('local:Evening session');
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await openEqualizer(page);
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Evening session' });
  await expect(page.getByRole('slider', { name: '63 Hz', exact: true })).toHaveValue('4');
  await page.getByRole('button', { name: 'Delete “Evening session”', exact: true }).click();
  await expect(page.getByLabel('Equalizer preset', { exact: true }).locator('option', { hasText: 'Evening session' })).toHaveCount(0);
});

test('each model loads its own original product render with real image dimensions', async ({ page }) => {
  await enterPreview(page);
  const sources = new Set<string>();
  for (const model of ['WH-1000XM6', 'WH-1000XM5', 'WH-CH720N']) {
    await page.getByRole('button', { name: `Select ${model}`, exact: true }).click();
    const image = page.getByRole('img', { name: `${model} official Sony product render`, exact: true }).first();
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate((element: HTMLImageElement) => element.complete && element.naturalWidth > 100 && element.naturalHeight > 100)).toBe(true);
    sources.add(await image.getAttribute('src') ?? '');
  }
  expect(sources.size).toBe(3);
  await page.getByRole('button', { name: 'Select WH-1000XM6', exact: true }).click();
  await page.screenshot({ path: 'tests/artifacts/original-product-render.png', fullPage: true });
});
