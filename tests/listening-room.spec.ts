import { expect, test } from '@playwright/test';

test.use({ launchOptions: { args: ['--enable-unsafe-swiftshader'] } });

test('one 3D context follows listening spaces and survives theme and size changes', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  const room = page.locator('.scene-3d-room');
  await expect(room).toHaveAttribute('data-renderer', 'ready');
  const canvas = await room.locator('canvas').elementHandle();
  await page.getByRole('button', { name: /Cinema A wider/ }).click();
  await expect(room).toHaveAttribute('data-space', 'cinema');
  await page.getByRole('button', { name: /Background Music A little/ }).click();
  for (const [label, space] of [['Cafe', 'cafe'], ['Living room', 'living-room'], ['My room', 'personal-room']]) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await expect(room).toHaveAttribute('data-space', space);
    expect(await canvas!.evaluate(element => element.isConnected)).toBe(true);
    await expect(room.locator('canvas')).toHaveCount(1);
  }
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await page.setViewportSize({ width: 390, height: 1000 });
  await expect(room).toHaveAttribute('data-renderer', 'ready');
  await expect(room.locator('canvas')).toBeVisible();
  expect(errors).toEqual([]);
  // Context loss must restore the CSS environment and release the GPU canvas.
  await canvas!.evaluate(element => element.dispatchEvent(new Event('webglcontextlost', { cancelable: true })));
  await expect(room).toHaveAttribute('data-renderer', 'fallback');
  await expect(room.locator('canvas')).toHaveCount(0);
  await expect(page.locator('.scene-background')).toHaveCSS('visibility', 'visible');
});

test('unavailable WebGL keeps the original artwork and mode controls usable', async ({ page }) => {
  await page.addInitScript(() => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (type: string, ...args: unknown[]) {
      if (type.startsWith('webgl') || type === 'experimental-webgl') return null;
      return Reflect.apply(getContext, this, [type, ...args]);
    } as typeof getContext;
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await expect(page.locator('.scene-3d-room')).toHaveAttribute('data-renderer', 'fallback');
  await page.getByRole('button', { name: /Cinema A wider/ }).click();
  await expect(page.locator('.scene-cinema')).toHaveCSS('visibility', 'visible');
  await expect(page.locator('.sound-visual img')).toBeVisible();
  await expect(page.locator('.sound-visual')).toHaveAttribute('data-scene', 'cinema');
});
