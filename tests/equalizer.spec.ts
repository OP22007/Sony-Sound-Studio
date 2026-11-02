import { expect, test } from '@playwright/test';

async function preview(page: import('@playwright/test').Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
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
      await expect(page.getByRole('slider', { name: frequency, exact: true })).toHaveValue(String(bands[index]));
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
  await bass.fill('5');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await page.getByRole('button', { name: 'Rename “Night bass”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Late night');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  await expect(bass).toHaveValue('5');
  await page.reload();
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'Apply “Late night”', exact: true }).click();
  await expect(bass).toHaveValue('5');
  await page.getByLabel('Equalizer preset', { exact: true }).selectOption({ label: 'Heavy' });
  await expect(bass).toHaveValue('4');
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
  await expect(low).toHaveValue('1');
  await selector.selectOption('sony:Heavy');
  await expect(low).toHaveValue('4');
  await selector.selectOption('Custom');
  await expect(selector).toHaveValue('Custom');
  await expect(low).toHaveValue('4');
  await page.getByRole('button', { name: 'Rename “Heavy”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Old favorite');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  await page.getByRole('button', { name: 'Apply “Old favorite”', exact: true }).click();
  await expect(low).toHaveValue('1');
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
