import { expect, test } from '@playwright/test';

async function preview(page: import('@playwright/test').Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
}

test('XM6 factory presets match the supplied Sony curves and leave preview isolated', async ({ page }) => {
  await preview(page);
  const frequencies = ['31 Hz', '63 Hz', '125 Hz', '250 Hz', '500 Hz', '1 kHz', '2 kHz', '4 kHz', '8 kHz', '16 kHz'];
  const curves = {
    Off: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    Heavy: [4, 4, -1, -1, 0, 0, 0, 0, 0, 0],
    Clear: [0, 0, 0, 0, 0, 0, 0, 2, 2, 3],
    Hard: [4, 4, -1, -1, 0, 0, 0, 2, 2, 3],
    Soft: [-3, -2, -1, 1, 1, 1, 0, 0, -2, -3],
    Game: [-6, -2, 2, 3, 1, -1, 1, 3, 0, -2],
  };
  const group = page.getByRole('group', { name: 'Sony equalizer presets' });
  for (const [name, bands] of Object.entries(curves)) {
    await group.getByRole('button', { name: new RegExp(`^${name} `) }).click();
    for (const [index, frequency] of frequencies.entries()) {
      await expect(page.getByRole('slider', { name: frequency, exact: true })).toHaveAttribute('aria-valuenow', String(bands[index]));
    }
  }
  await page.getByRole('button', { name: 'Select WH-1000XM5', exact: true }).click();
  await expect(group).toHaveCount(0);
  await expect(page.getByText('Sony factory curves for this model are not validated yet.', { exact: false })).toBeVisible();
});

test('custom preset can be updated, renamed, reloaded and deleted without changing a factory preset', async ({ page }) => {
  await preview(page);
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Heavy' });
  await page.getByRole('button', { name: 'Save preset', exact: true }).click();
  await page.getByLabel('Preset name', { exact: true }).fill('Night bass');
  await page.getByRole('form', { name: 'Save preset', exact: true }).getByRole('button', { name: 'Save preset', exact: true }).click();
  const bass = page.getByRole('slider', { name: '31 Hz', exact: true });
  await bass.focus();
  await bass.press('ArrowRight');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await page.getByRole('button', { name: 'Rename “Night bass”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Late night');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  await expect(bass).toHaveAttribute('aria-valuenow', '5');
  await page.reload();
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await page.getByRole('button', { name: 'Apply “Late night”', exact: true }).click();
  await expect(bass).toHaveAttribute('aria-valuenow', '5');
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Heavy' });
  await expect(bass).toHaveAttribute('aria-valuenow', '4');
  await page.getByRole('button', { name: 'Delete “Late night”', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply “Late night”', exact: true })).toHaveCount(0);
});

test('legacy preset names remain distinct from factory presets and Manual keeps the current curve', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('sound-connect:presets', JSON.stringify([{ name: 'Heavy', bands: [1, 2, 0, 0, 0, 0, 0, 0, 0, 0] }]));
  });
  await preview(page);
  const selector = page.getByLabel('Equalizer preset', { exact: true });
  const low = page.getByRole('slider', { name: '31 Hz', exact: true });
  await selector.selectOption('local:Heavy');
  await expect(low).toHaveAttribute('aria-valuenow', '1');
  await selector.selectOption('sony:Heavy');
  await expect(low).toHaveAttribute('aria-valuenow', '4');
  await selector.selectOption('Custom');
  await expect(selector).toHaveValue('Custom');
  await expect(low).toHaveAttribute('aria-valuenow', '4');
  await page.getByRole('button', { name: 'Rename “Heavy”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Old favorite');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  await page.getByRole('button', { name: 'Apply “Old favorite”', exact: true }).click();
  await expect(low).toHaveAttribute('aria-valuenow', '1');
});

test('reserved names and storage failures leave the saved preset library intact', async ({ page }) => {
  await preview(page);
  await page.getByRole('button', { name: 'Save preset', exact: true }).click();
  const form = page.getByRole('form', { name: 'Save preset', exact: true });
  await page.getByLabel('Preset name', { exact: true }).fill(' heavy ');
  await form.getByRole('button', { name: 'Save preset', exact: true }).click();
  await expect(form.getByRole('alert')).toContainText('unique');
  await page.getByLabel('Preset name', { exact: true }).fill('My profile');
  await page.evaluate(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function(key: string, value: string) {
      if (key === 'sound-connect:presets') throw new DOMException('Storage full', 'QuotaExceededError');
      return original.call(this, key, value);
    };
  });
  await form.getByRole('button', { name: 'Save preset', exact: true }).click();
  await expect(form.getByRole('alert')).toContainText('could not be saved');
  await expect(page.getByRole('button', { name: 'Apply “My profile”', exact: true })).toHaveCount(0);
});

test('grabbing a point does not move it and a fast drag travels one step at a time', async ({ page }) => {
  await preview(page);
  const graph = page.getByRole('slider', { name: '31 Hz', exact: true });
  const svg = page.locator('.eq-curve');
  const box = await svg.boundingBox();
  const point = await graph.boundingBox();
  if (!box || !point) throw new Error('Equalizer graph is not visible');
  const before = await graph.getAttribute('aria-valuenow');
  const cx = point.x + point.width / 2;
  // Press a few pixels off the point's centre: the value must not change on grab.
  await page.mouse.move(cx, point.y + point.height / 2 + 6);
  await page.mouse.down();
  await expect(graph).toHaveAttribute('aria-valuenow', before ?? '');
  // Jerk the pointer to the top of the graph in one event; record every value on the way.
  await page.evaluate(() => {
    const node = document.querySelector('[aria-label="31 Hz"]') as Element;
    (window as unknown as { seen: string[] }).seen = [];
    new MutationObserver(() => (window as unknown as { seen: string[] }).seen.push(node.getAttribute('aria-valuenow') ?? '')).observe(node, { attributes: true, attributeFilter: ['aria-valuenow'] });
  });
  await page.mouse.move(cx, box.y + 2, { steps: 1 });
  await page.mouse.up();
  await expect(graph).toHaveAttribute('aria-valuenow', '6');
  const seen = await page.evaluate(() => (window as unknown as { seen: string[] }).seen.map(Number));
  const steps = seen.slice(1).map((value, index) => Math.abs(value - seen[index]));
  // One jump from 0 to 6 would be a single update. React may merge two frames into one render,
  // so allow a pair of steps between observed values, but require a gradual climb.
  expect(seen.length).toBeGreaterThanOrEqual(3);
  expect(Math.max(...steps)).toBeLessThanOrEqual(2);
});

test('graph handles drag with the pointer and expose bounded keyboard sliders', async ({ page }) => {
  await preview(page);
  const graph = page.getByRole('slider', { name: '31 Hz', exact: true });
  const svg = page.locator('.eq-curve');
  const box = await svg.boundingBox();
  if (!box) throw new Error('Equalizer graph is not visible');
  const point = await graph.boundingBox();
  if (!point) throw new Error('Bass graph handle is not visible');
  await page.mouse.move(point.x + point.width / 2, point.y + point.height / 2);
  await page.mouse.down();
  const targetY = box.y + (140 - (3 + 6) * 100 / 12) / 175 * box.height;
  await page.mouse.move(point.x + point.width / 2, targetY, { steps: 4 });
  await page.mouse.up();
  await expect(graph).toHaveAttribute('aria-valuenow', '3');
  await expect(graph).toHaveAttribute('aria-valuetext', '3 decibels');
  await graph.focus();
  await page.keyboard.press('End');
  await expect(graph).toHaveAttribute('aria-valuenow', '6');
  await page.keyboard.press('ArrowUp');
  await expect(graph).toHaveAttribute('aria-valuenow', '6');
  await page.keyboard.press('Home');
  await expect(graph).toHaveAttribute('aria-valuenow', '-6');
});
