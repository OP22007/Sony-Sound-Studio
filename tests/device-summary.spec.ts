import { expect, test, type Page } from '@playwright/test';

async function installMeterFixture(page: Page, options: { battery?: number | null; charging?: string; readAt?: number } = {}) {
  await page.addInitScript((options) => {
    let callback = 0;
    const requests: string[] = [];
    Object.assign(window, {
      isTauri: true,
      __meterRequests: requests,
      __meterLevels: { left: .3, right: .6 },
      __TAURI_INTERNALS__: {
        transformCallback: () => ++callback,
        unregisterCallback: () => {},
        invoke: async (command: string) => {
          requests.push(command);
          if (command.startsWith('plugin:event|')) return 1;
          if (command === 'get_app_preferences') return { theme: 'dark', reduceMotion: false, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
          if (command === 'get_output_levels') return (window as unknown as { __meterLevels: unknown }).__meterLevels;
          if (command === 'get_app_state') return {
            revision: 1, platform: 'windows', transportStatus: 'Synthetic peak-meter fixture', diagnostics: [], activeOutputDeviceId: 'xm6', audioOutputObserved: true,
            devices: [{ id: 'xm6', name: 'WH-1000XM6', model: 'WH-1000XM6', connection: 'connected', battery: options.battery === undefined ? 84 : options.battery, firmware: 'test', capabilities: ['noise'], settings: { batteryCharging: options.charging, batteryReadAtMs: options.readAt, noise: 'cancelling', noiseConfirmation: 'confirmed', listeningMode: 'standard' } }],
          };
          throw new Error(`Unexpected command: ${command}`);
        },
      },
    });
  }, options);
}

test('compact device summary shows reported battery, confirmed modes and honest codec status', async ({ page }) => {
  await installMeterFixture(page);
  await page.goto('/');
  const hero = page.locator('.device-hero');
  await expect(hero).toContainText('84%');
  await expect(hero).toContainText('Active audio output');
  await expect(hero).toContainText('Bluetooth control');
  await expect(hero.locator('.device-codec')).toHaveText('Codec Not reported');
  await expect(hero.locator('.hero-session-summary')).toContainText('Standard');
  await expect(hero.locator('.hero-session-summary')).toContainText('Noise cancelling');
  await expect(hero.getByRole('button', { name: 'Disconnect', exact: true })).toHaveClass(/device-disconnect/);
  expect((await hero.boundingBox())!.height).toBeLessThanOrEqual(330);
  expect(await page.evaluate(() => (window as unknown as { __meterRequests: string[] }).__meterRequests.filter(c => c === 'get_output_levels'))).toEqual([]);
});

test('output peaks drive subtle channel response and stop when reduced motion is enabled', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await installMeterFixture(page);
  await page.goto('/');
  const stage = page.locator('.sound-visual');
  await expect(stage).toHaveAttribute('data-audio', 'live');
  await expect.poll(() => stage.evaluate(e => e.style.getPropertyValue('--audio-right'))).toBe('0.6');
  await page.evaluate(() => { (window as unknown as { __meterLevels: unknown }).__meterLevels = null; });
  await expect(stage).toHaveAttribute('data-audio', 'unavailable');
  await expect.poll(() => stage.evaluate(e => e.style.getPropertyValue('--audio-energy'))).toBe('0');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(stage).toHaveAttribute('data-motion', 'reduced');
  const count = await page.evaluate(() => (window as unknown as { __meterRequests: string[] }).__meterRequests.length);
  await page.waitForTimeout(650);
  expect(await page.evaluate(() => (window as unknown as { __meterRequests: string[] }).__meterRequests.length)).toBe(count);
});


test('low battery warning uses the strict 20 percent threshold and shows actual reply time', async ({ page }) => {
  const readAt = Date.now() - 120_000;
  await installMeterFixture(page, { battery: 19, charging: 'notCharging', readAt });
  await page.goto('/');
  const battery = page.locator('.device-battery');
  await expect(battery).toContainText('19%');
  await expect(battery.locator('.battery-warning')).toBeVisible();
  await expect(battery).toContainText('Not charging');
  await expect(battery.locator('time')).toHaveAttribute('datetime', new Date(readAt).toISOString());
  expect((await page.locator('.device-hero').boundingBox())!.height).toBeLessThanOrEqual(330);
});

test('charging is reported independently of low battery and unknown update time stays honest', async ({ page }) => {
  await installMeterFixture(page, { battery: 12, charging: 'charging' });
  await page.goto('/');
  const battery = page.locator('.device-battery');
  await expect(battery).toContainText('Charging');
  await expect(battery).toHaveClass(/is-charging/);
  await expect(battery.locator('.battery-warning')).toBeVisible();
  await expect(battery.locator('.battery-freshness')).toHaveText('Update time unavailable');
  await expect(battery.locator('time')).toHaveCount(0);
});

test('20 percent is not low and full-charge readback is displayed', async ({ page }) => {
  await installMeterFixture(page, { battery: 20, charging: 'charged' });
  await page.goto('/');
  await expect(page.locator('.battery-warning')).toHaveCount(0);
  await expect(page.locator('.device-battery')).toContainText('Fully charged');
});


test('absent battery does not invent charging or update time', async ({ page }) => {
  await installMeterFixture(page, { battery: null, charging: 'charging', readAt: Date.now() });
  await page.goto('/');
  const battery = page.locator('.device-battery');
  await expect(battery).toContainText('Not reported');
  await expect(battery).not.toHaveClass(/is-charging/);
  await expect(battery.locator('.battery-freshness')).toHaveCount(0);
  await expect(battery.locator('.battery-warning')).toHaveCount(0);
});
