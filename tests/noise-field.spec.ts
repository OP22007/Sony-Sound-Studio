import { expect, test } from '@playwright/test';

test('noise treatments stay distinct with reduced motion and leave mode controls usable', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  const field = page.locator('.noise-field');
  for (const [label, active, opacity] of [['Noise cancelling', 'noise-cancel', '1'], ['Ambient sound', 'noise-ambient', '1'], ['Off', 'noise-passive', '0.5']]) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await expect(field.locator(`.${active}`)).toHaveCSS('opacity', opacity);
    for (const other of ['noise-cancel', 'noise-ambient', 'noise-passive'].filter(name => name !== active)) {
      await expect(field.locator(`.${other}`)).toHaveCSS('opacity', '0');
    }
    expect(await field.evaluate(element => element.getAnimations({ subtree: true }).length)).toBe(0);
  }
  await page.getByRole('button', { name: /Cinema A wider/ }).click();
  await expect(field.locator('.noise-passive')).toHaveCSS('opacity', '0.5');
});

test('noise flow runs only for its active mode and pauses offscreen and for reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  const visual = page.locator('.sound-visual');
  const arrival = page.locator('.noise-arrival').first();
  const through = page.locator('.noise-through').first();
  await expect(visual).toHaveAttribute('data-motion', 'running');
  await expect(arrival).toHaveCSS('animation-play-state', 'running');
  await expect(through).toHaveCSS('animation-play-state', 'paused');
  const offset = await arrival.evaluate(element => getComputedStyle(element).strokeDashoffset);
  await expect.poll(() => arrival.evaluate(element => getComputedStyle(element).strokeDashoffset)).not.toBe(offset);
  await page.getByRole('button', { name: 'Ambient sound', exact: true }).click();
  await expect(arrival).toHaveCSS('animation-play-state', 'paused');
  await expect(through).toHaveCSS('animation-play-state', 'running');
  await page.setViewportSize({ width: 1440, height: 240 });
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await expect(visual).toHaveAttribute('data-motion', 'paused');
  await expect(through).toHaveCSS('animation-play-state', 'paused');
  const paused = await through.evaluate(element => getComputedStyle(element).strokeDashoffset);
  await page.waitForTimeout(120);
  expect(await through.evaluate(element => getComputedStyle(element).strokeDashoffset)).toBe(paused);
  await visual.scrollIntoViewIfNeeded();
  await expect(through).toHaveCSS('animation-play-state', 'running');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(page.locator('.noise-field')).toBeVisible();
  await expect(through).toHaveCSS('animation-name', 'none');
});
