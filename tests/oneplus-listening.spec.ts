import { expect, test, type Page } from '@playwright/test';

type Fixture = { descriptors?: Record<string, unknown>[]; settings?: Record<string, unknown>; capabilities?: string[] };

async function installOnePlusFixture(page: Page, fixture: Fixture) {
  await page.addInitScript((input: Fixture) => {
    const callbacks = new Map<number, (event: unknown) => void>();
    let callbackId = 0;
    let state: Record<string, any> = {
      revision: 1, platform: 'Fixture', transportStatus: 'Controlled browser fixture', diagnostics: [],
      devices: [{ id: 'oneplus-fixture', name: 'OnePlus Buds Pro 3', model: 'OnePlus Buds Pro 3', connection: 'connected', battery: null, firmware: 'fixture',
        capabilities: input.capabilities ?? ['spatialAudio'], settings: { spatialAudio: 0, ...(input.settings ?? {}), controlDescriptors: input.descriptors } }],
    };
    let appPreferences = { theme: 'dark', reduceMotion: false, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
    const snapshot = () => JSON.parse(JSON.stringify(state));
    Object.assign(window, {
      isTauri: true,
      __fixtureRequests: [] as Array<{ command: string; args?: Record<string, unknown> }>,
      __finishApply: null as null | (() => void),
      __rejectApply: null as null | (() => void),
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { const id = ++callbackId; callbacks.set(id, callback); return id; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: (command: string, args?: Record<string, unknown> & { event?: string; handler?: number; setting?: string; value?: unknown; preferences?: Record<string, unknown> }) => {
          (window as any).__fixtureRequests.push({ command, args });
          if (command === 'plugin:event|listen') return Promise.resolve(1);
          if (command.startsWith('plugin:event|')) return Promise.resolve(1);
          if (command === 'get_app_state') return Promise.resolve(snapshot());
          if (command === 'get_app_preferences') return Promise.resolve({ ...appPreferences });
          if (command === 'update_app_preferences') {
            appPreferences = { ...appPreferences, ...(args?.preferences ?? {}) };
            return Promise.resolve({ ...appPreferences });
          }
          if (command === 'get_app_update') return Promise.resolve({ currentVersion: '0.1.0', status: 'idle', version: null, notes: null, downloaded: 0, total: null, error: null });
          if (command === 'get_media_automation') return Promise.resolve({ enabled: false, status: 'disabled', source: null, desiredMode: null, reason: 'off', manualOverride: false, extensionToken: null, browserConnected: false });
          if (command === 'get_vlc_connection') return Promise.resolve({ enabled: false, passwordConfigured: false, status: 'off', playing: false, kind: null });
          if (command === 'get_output_levels') return Promise.resolve(null);
          if (command === 'apply_setting') return new Promise((resolve, reject) => {
            (window as any).__finishApply = () => {
              state.devices[0].settings[args?.setting ?? ''] = args?.value;
              state.revision += 1;
              resolve(snapshot());
            };
            (window as any).__rejectApply = () => reject(new Error('Device did not confirm the profile.'));
          });
          return Promise.resolve(null);
        },
      },
    });
  }, fixture);
}

test('OnePlus numeric spatial audio request keeps the confirmed Off value until native confirmation', async ({ page }) => {
  await installOnePlusFixture(page, {
    capabilities: ['spatialAudio', 'gameMode', 'gameSoundProfile', 'eqPreset'],
    settings: { gameMode: false, gameSoundProfile: 0, eqPreset: 0 },
    descriptors: [
      { key: 'spatialAudio', label: 'Spatial audio', description: 'Confirmed modes', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 1, label: 'Fixed spatial audio' }, { value: 2, label: 'Head-tracked spatial audio' }] },
      { key: 'gameMode', label: 'Game mode', description: 'Reduce latency', kind: 'toggle' },
      { key: 'gameSoundProfile', label: 'Game sound profile', description: 'Negotiated profiles', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 3, label: 'Game sound profile 3' }] },
      { key: 'eqPreset', label: 'Sound profile', description: 'OnePlus EQ', kind: 'select', options: [{ value: 0, label: 'Balanced' }] },
    ],
  });
  await page.goto('/');
  await page.setViewportSize({ width: 1440, height: 960 });
  const fixed = page.getByRole('button', { name: 'Fixed spatial audio Spatial effect fixed in place', exact: true });
  const off = page.getByRole('button', { name: 'Off Standard listening', exact: true });
  await expect(page.locator('.oneplus-spatial-options .listening-choice')).toHaveCount(3);
  await expect(off).toHaveAttribute('aria-pressed', 'true');
  await page.screenshot({ path: 'tests/artifacts/oneplus-listening-dark.png' });
  await page.getByRole('button', { name: 'Light theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: 'tests/artifacts/oneplus-listening-light.png' });
  await page.getByRole('button', { name: 'Dark theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await fixed.click();
  await expect(off).toHaveAttribute('aria-pressed', 'true');
  await expect(fixed).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.hero-session-summary')).toContainText('Standard');
  await expect(page.getByText('Waiting for headphone confirmation…').first()).toBeVisible();
  const request = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests.find(item => item.command === 'apply_setting'));
  expect(request?.args).toMatchObject({ setting: 'spatialAudio', value: 1, deviceId: 'oneplus-fixture' });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(fixed).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hero-session-summary')).toContainText('Spatial audio');
  await expect(page.locator('.visual-caption')).toContainText('Spatial audio');
  await expect(page.locator('.sound-visual')).toHaveAttribute('data-scene', 'music');
  const headTracked = page.getByRole('button', { name: 'Head-tracked spatial audio Spatial effect follows head movement', exact: true });
  await headTracked.click();
  await expect(fixed).toHaveAttribute('aria-pressed', 'true');
  const headTrackedRequest = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests.filter(item => item.command === 'apply_setting').at(-1));
  expect(headTrackedRequest?.args).toMatchObject({ setting: 'spatialAudio', value: 2 });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(headTracked).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hero-session-summary')).toContainText('Head-tracked spatial audio');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Sound profile', exact: true })).toBeEnabled();
  await expect(page.getByLabel('DSEE mode', { exact: true })).toHaveCount(0);
  await expect(page.getByText(/EQ and DSEE are suspended/)).toHaveCount(0);
  await page.getByRole('button', { name: 'Sound studio', exact: true }).click();

  const profile = page.getByRole('combobox', { name: 'Game sound profile', exact: true });
  await expect(profile).toHaveValue('0');
  await profile.selectOption('3');
  await expect(profile).toHaveValue('0');
  const profileRequest = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests.filter(item => item.command === 'apply_setting').at(-1));
  expect(profileRequest?.args).toMatchObject({ setting: 'gameSoundProfile', value: 3, deviceId: 'oneplus-fixture' });
  await expect(page.getByText('Waiting for headphone confirmation…').first()).toBeVisible();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(profile).toHaveValue('3');

  const game = page.getByRole('switch', { name: 'Game mode · low latency', exact: true });
  await expect(game).toHaveAttribute('aria-checked', 'false');
  await game.click();
  await expect(game).toHaveAttribute('aria-checked', 'false');
  const gameRequest = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests.filter(item => item.command === 'apply_setting').at(-1));
  expect(gameRequest?.args).toMatchObject({ setting: 'gameMode', value: true });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(game).toHaveAttribute('aria-checked', 'true');
  await off.click();
  await expect(off).toHaveAttribute('aria-pressed', 'false');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(off).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hero-session-summary')).toContainText('Game mode');
});

test('OnePlus card hides unsupported spatial values and reports missing descriptors honestly', async ({ page }) => {
  await installOnePlusFixture(page, {
    capabilities: ['spatialAudio', 'gameSoundProfile'],
    settings: { gameSoundProfile: 0 },
    descriptors: [
      { key: 'spatialAudio', label: 'Spatial audio', description: 'Fixture modes', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 1, label: 'Fixed', enabled: false }, { value: 9, label: 'Unsupported mode' }] },
      { key: 'gameSoundProfile', label: 'Game sound profile', description: 'Fixture profiles', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 3, label: 'Unavailable profile', enabled: false }] },
    ],
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Off Standard listening', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /Unsupported mode/ })).toHaveCount(0);
  await expect(page.locator('.oneplus-spatial-options button[data-mode="spatial-1"]')).toBeDisabled();
  const disabledProfile = page.getByRole('combobox', { name: 'Game sound profile', exact: true });
  await expect(disabledProfile).toHaveValue('0');
  await expect(disabledProfile.locator('option[value="3"]')).toBeDisabled();
  await expect(page.getByRole('switch', { name: /Game mode/ })).toHaveCount(0);

  const missingPage = await page.context().newPage();
  await installOnePlusFixture(missingPage, { capabilities: ['spatialAudio'], settings: { spatialAudio: 0 } });
  await missingPage.goto('/');
  await expect(missingPage.getByRole('group', { name: 'Spatial audio' }).getByText('Not confirmed', { exact: true })).toBeVisible();
  await expect(missingPage.locator('.hero-session-summary')).toContainText('Not confirmed');
  await expect(missingPage.locator('.oneplus-spatial-options')).toHaveCount(0);

  const invalidCurrentPage = await page.context().newPage();
  await installOnePlusFixture(invalidCurrentPage, {
    capabilities: ['spatialAudio'], settings: { spatialAudio: 9 },
    descriptors: [{ key: 'spatialAudio', label: 'Spatial audio', description: 'Fixture modes', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 1, label: 'Fixed' }, { value: 9, label: 'Unverified current mode' }] }],
  });
  await invalidCurrentPage.goto('/');
  await expect(invalidCurrentPage.getByRole('group', { name: 'Spatial audio' }).getByText('Not confirmed', { exact: true })).toBeVisible();
  await expect(invalidCurrentPage.locator('.oneplus-spatial-options')).toHaveCount(0);
});

test('rejected OnePlus GameSound request keeps the last confirmed profile selected', async ({ page }) => {
  await installOnePlusFixture(page, {
    capabilities: ['gameSoundProfile'],
    settings: { gameSoundProfile: 0 },
    descriptors: [{ key: 'gameSoundProfile', label: 'Game sound profile', description: 'Negotiated profiles', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 3, label: 'Game sound profile 3' }] }],
  });
  await page.goto('/');
  const profile = page.getByRole('combobox', { name: 'Game sound profile', exact: true });
  await expect(profile).toHaveValue('0');
  await profile.selectOption('3');
  await expect(profile).toHaveValue('0');
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByRole('alert').last()).toContainText('Device did not confirm the profile.');
  await expect(profile).toHaveValue('0');
  await expect(profile).toBeEnabled();
});
