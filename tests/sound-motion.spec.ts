import { expect, test } from '@playwright/test';
import { buildSoundSurface } from '../src/sound-scene';

test('EQ settings produce a finite surface with consistent geometry in native and preview scales', () => {
  const neutral = buildSoundSurface(Array(10).fill(6), false);
  expect(buildSoundSurface(Array(10).fill(0), true).paths).toEqual(neutral.paths);
  expect(buildSoundSurface([], false).paths).toEqual(neutral.paths);
  for (const indices of [[0, 1, 2], [3, 4, 5, 6], [7, 8, 9]]) {
    const bands = Array(10).fill(6);
    indices.forEach(index => { bands[index] = 12; });
    const surface = buildSoundSurface(bands, false);
    expect(surface.paths.length).toBe(neutral.paths.length);
    expect(surface.paths).not.toEqual(neutral.paths);
    expect(surface.paths.join(' ')).not.toMatch(/NaN|Infinity/);
  }
});

test('cinematic listening environments, room depth, and noise perimeter remain distinct without motion', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  const visual = page.locator('.sound-visual');
  await expect(visual).toHaveAttribute('data-scene', 'standard');
  await expect(page.locator('.scene-standard')).toHaveCSS('opacity', '1');
  await page.locator('.device-hero').screenshot({ path: 'tests/artifacts/scene-standard.png' });
  await page.getByRole('button', { name: /Background Music A little atmosphere/ }).click();
  await expect(page.locator('.scene-background')).toHaveCSS('opacity', '1');
  await expect(page.locator('.scene-standard')).toHaveCSS('opacity', '0');
  const near = await page.locator('.scene-room-depth').boundingBox();
  await page.getByRole('button', { name: 'Cafe', exact: true }).click();
  const far = await page.locator('.scene-room-depth').boundingBox();
  expect(far!.width).toBeGreaterThan(near!.width * 1.3);
  await page.locator('.device-hero').screenshot({ path: 'tests/artifacts/scene-background-cafe.png' });
  await page.getByRole('button', { name: 'Ambient sound', exact: true }).click();
  await expect(page.locator('.scene-periphery')).toHaveCSS('opacity', '0.8');
  await page.getByRole('button', { name: 'Noise cancelling', exact: true }).click();
  await expect(page.locator('.scene-periphery')).toHaveCSS('opacity', '0.04');
  await page.getByRole('button', { name: /Cinema A wider soundstage/ }).click();
  await expect(page.locator('.scene-cinema')).toHaveCSS('opacity', '1');
  await expect(page.locator('.scene-background')).toHaveCSS('opacity', '0');
  await page.locator('.device-hero').screenshot({ path: 'tests/artifacts/scene-cinema.png' });
});

test('official product art fits the hero for all preview models and window widths', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  for (const width of [1920, 1440, 1024, 760, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    for (const model of ['WH-1000XM6', 'WH-1000XM5', 'WH-CH720N']) {
      await page.getByRole('button', { name: `Select ${model}`, exact: true }).click();
      const image = page.locator('.sound-visual img');
      await expect(image).toBeVisible();
      await image.evaluate((element: HTMLImageElement) => element.decode());
      const hero = await page.locator('.device-hero').boundingBox();
      const art = await image.boundingBox();
      expect(art!.y, `${width}/${model} top`).toBeGreaterThan(hero!.y + 5);
      expect(art!.y + art!.height, `${width}/${model} bottom`).toBeLessThan(hero!.y + hero!.height - 5);
      expect(art!.x, `${width}/${model} left`).toBeGreaterThan(hero!.x);
      expect(art!.x + art!.width, `${width}/${model} right`).toBeLessThan(hero!.x + hero!.width);
    }
  }
});

test('noise, listening modes, environments and EQ produce distinct running motion; reduced motion stops it', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  const visual = page.locator('.sound-visual');
  const choreography = () => visual.getAttribute('data-choreography');
  await expect.poll(choreography).toBe('cancelling/standard/0');
  await page.getByRole('button', { name: 'Ambient sound', exact: true }).click();
  await expect.poll(choreography).toBe('ambient/standard/0');
  await page.getByRole('button', { name: 'Off', exact: true }).click();
  await expect.poll(choreography).toBe('off/standard/0');
  await page.getByRole('button', { name: /Background Music A little atmosphere/ }).click();
  await expect(visual).toHaveAttribute('data-listening', 'backgroundMusic');
  await page.getByRole('button', { name: 'Cafe', exact: true }).click();
  await expect(visual).toHaveAttribute('data-room', '2');
  await page.getByRole('button', { name: /Cinema A wider soundstage/ }).click();
  await expect(visual).toHaveAttribute('data-listening', 'cinema');
  await expect.poll(choreography).toBe('off/cinema/2');
  const before = await page.locator('.visual-profile .surface-front').getAttribute('d');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await page.getByRole('slider', { name: '63 Hz', exact: true }).focus();
  await page.keyboard.press('End');
  await page.getByRole('button', { name: 'Sound studio', exact: true }).click();
  await expect(visual).toHaveAttribute('data-eq', 'bass');
  await expect.poll(() => page.locator('.visual-profile .surface-front').getAttribute('d')).not.toBe(before);
  const treble = await page.locator('.visual-profile .surface-front').getAttribute('d');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await page.getByRole('slider', { name: '16 kHz', exact: true }).focus();
  await page.keyboard.press('End');
  await page.getByRole('button', { name: 'Sound studio', exact: true }).click();
  await expect(visual).toHaveAttribute('data-eq', 'treble');
  await expect.poll(() => page.locator('.visual-profile .surface-front').getAttribute('d')).not.toBe(treble);
  const pose = await page.locator('.visual-drift').getAttribute('style');
  await expect.poll(() => page.locator('.visual-drift').getAttribute('style')).not.toBe(pose);
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: 'tests/artifacts/sound-motion-cinema.png', fullPage: true });
  await page.setViewportSize({ width: 1440, height: 240 });
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await expect(visual).toHaveAttribute('data-motion', 'paused');
  const paused = await page.locator('.visual-drift').getAttribute('style');
  await page.waitForTimeout(150);
  expect(await page.locator('.visual-drift').getAttribute('style')).toBe(paused);
  await visual.scrollIntoViewIfNeeded();
  await expect(visual).toHaveAttribute('data-motion', 'running');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(visual).toHaveAttribute('data-motion', 'reduced');
  const still = await page.locator('.visual-drift').getAttribute('style');
  await page.waitForTimeout(150);
  expect(await page.locator('.visual-drift').getAttribute('style')).toBe(still);
});
