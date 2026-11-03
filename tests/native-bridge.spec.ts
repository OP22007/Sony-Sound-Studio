import { expect, test, type Page } from '@playwright/test';

async function installNativeFixture(page: Page, noiseSupported: boolean, options: {
  firmware?: string;
  noiseSettings?: Record<string, unknown>;
  applySettings?: Record<string, unknown>;
  deviceSettings?: Record<string, unknown>;
  deviceCapabilities?: string[];
  appPreferences?: Record<string, unknown>;
  preferenceGetError?: string;
  preferenceUpdateError?: string;
  applySettingError?: string;
  mediaState?: Record<string, unknown>;
  mediaSetError?: string;
} = {}) {
  await page.addInitScript(({ noiseSupported, options }) => {
    const calls: string[] = [];
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextCallback = 0;
    const eventHandlers = new Map<string, number>();
    const preferenceStorageKey = '__fixtureNativePreferences';
    const defaultPreferences = { theme: 'dark', reduceMotion: false, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
    let appPreferences = JSON.parse(localStorage.getItem(preferenceStorageKey) ?? 'null') ?? { ...defaultPreferences, ...options.appPreferences };
    let media = { enabled: false, status: 'disabled', source: null, desiredMode: null, reason: 'Follow media is off.', manualOverride: false, extensionToken: 'a'.repeat(64), browserConnected: false, ...options.mediaState };
    let vlc = { enabled: false, passwordConfigured: false, status: 'VLC connection is off.', playing: false, kind: null as string | null };
    const state = {
      revision: 1,
      platform: 'Native IPC test fixture',
      transportStatus: 'Controlled integration fixture; no physical Bluetooth access',
      diagnostics: [],
      devices: [{
        id: 'test-device', name: 'WH-1000XM6', model: 'WH-1000XM6', connection: 'connected',
        battery: null, firmware: options.firmware ?? null,
        capabilities: options.deviceCapabilities ?? (noiseSupported ? ['noise'] : []),
        settings: { ...(noiseSupported ? { noise: 'cancelling', ...options.noiseSettings } : {}), ...options.deviceSettings },
      }],
    };
    const snapshot = () => ({
      ...state,
      diagnostics: [...state.diagnostics],
      devices: state.devices.map(device => ({ ...device, capabilities: [...device.capabilities], settings: { ...device.settings } })),
    });
    Object.assign(window, {
      isTauri: true,
      __fixtureCalls: calls,
      __emitMedia: (change: Record<string, unknown>) => {
        media = { ...media, ...change };
        callbacks.get(eventHandlers.get('media-automation') ?? 0)?.({ event: 'media-automation', id: 2, payload: { ...media } });
      },
      __emitState: (settings: Record<string, unknown>) => {
        Object.assign(state.devices[0].settings, settings);
        state.revision += 1;
        callbacks.get(eventHandlers.get('app-state') ?? 0)?.({ event: 'app-state', id: 1, payload: snapshot() });
      },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { const id = ++nextCallback; callbacks.set(id, callback); return id; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: (command: string, args?: { password?: string; enabled?: boolean; event?: string; handler?: number; preferences?: unknown; setting?: string; value?: unknown; deviceId?: string }) => {
          calls.push(command);
          if (command === 'plugin:event|listen') { eventHandlers.set(args?.event ?? '', args?.handler ?? 0); return Promise.resolve(1); }
          if (command.startsWith('plugin:event|')) return Promise.resolve(1);
          if (command === 'get_app_state') return Promise.resolve(snapshot());
          if (command === 'get_app_update') return Promise.resolve({ currentVersion: '0.1.2', status: 'idle', version: null, notes: null, downloaded: 0, total: null, error: null });
          if (command === 'get_media_automation') return Promise.resolve({ ...media });
          if (command === 'get_vlc_connection') return Promise.resolve({ ...vlc });
          if (command === 'test_vlc_connection') return Promise.resolve({ ...vlc, playing: true, kind: 'movie', status: 'VLC connection test succeeded; playback detected.' });
          if (command === 'configure_vlc_connection') {
            vlc = { ...vlc, enabled: !!args?.enabled, passwordConfigured: vlc.passwordConfigured || !!args?.password, status: 'Waiting for a local VLC status check.' };
            return Promise.resolve({ ...vlc });
          }
          if (command === 'prepare_media_companion') return Promise.resolve('C:/Test/Browser companion/chromium');
          if (command === 'set_media_automation') {
            if (options.mediaSetError) return Promise.reject(new Error(options.mediaSetError));
            media = { ...media, enabled: !!args?.enabled, status: args?.enabled ? 'waiting' : 'disabled', manualOverride: false };
            return Promise.resolve({ ...media });
          }
          if (command === 'resume_media_automation') {
            media = { ...media, manualOverride: false, status: 'waiting' };
            return Promise.resolve({ ...media });
          }
          if (command === 'get_app_preferences') return options.preferenceGetError ? Promise.reject(new Error(options.preferenceGetError)) : Promise.resolve({ ...appPreferences });
          if (command === 'update_app_preferences') {
            if (options.preferenceUpdateError) return Promise.reject(new Error(options.preferenceUpdateError));
            appPreferences = { ...(args?.preferences as Record<string, unknown>) };
            localStorage.setItem(preferenceStorageKey, JSON.stringify(appPreferences));
            return Promise.resolve({ ...appPreferences });
          }
          if (command === 'apply_setting') return new Promise((resolve, reject) => {
            Object.assign(window, {
              __rejectApply: () => reject(new Error(options.applySettingError ?? 'Device response timed out. Last confirmed setting retained.')),
              __finishApply: () => {
                if (options.applySettings) Object.assign(state.devices[0].settings, options.applySettings);
                else if (args?.setting) Object.assign(state.devices[0].settings, { [args.setting]: args.value });
                state.revision += 1;
                resolve(snapshot());
              },
            });
          });
          return Promise.reject(new Error(`Unexpected fixture command: ${command}`));
        },
      },
    });
  }, { noiseSupported, options });
}

test('Sound Studio applies negotiated listening modes and EQ arrays only after device confirmation', async ({ page }) => {
  const descriptors = [
    { key: 'listeningMode', label: 'Listening mode', description: 'Device modes', kind: 'select', options: [{ value: 'standard', label: 'Standard' }, { value: 'backgroundMusic', label: 'Background Music' }] },
    { key: 'bgmRoomSize', label: 'Background Music distance', description: 'Device rooms', kind: 'select', options: [{ value: 0, label: 'My room' }, { value: 2, label: 'Cafe' }] },
  ];
  await installNativeFixture(page, true, {
    deviceCapabilities: ['noise', 'equalizer', 'listeningMode', 'bgmRoomSize'],
    noiseSettings: { noiseConfirmation: 'confirmed' },
    deviceSettings: { equalizer: [6, 6, 6, 6, 6, 6, 6, 6, 6, 6], listeningMode: 'standard', bgmRoomSize: 0, controlDescriptors: descriptors },
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: /Cinema A wider soundstage/ })).toHaveCount(0);
  const bgm = page.getByRole('button', { name: /Background Music A little atmosphere/ });
  await bgm.click();
  await expect(bgm).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.visual-caption')).toContainText('WAITING FOR HEADPHONE');
  await expect(page.locator('.sound-visual')).toHaveAttribute('data-listening', 'standard');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(bgm).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Cafe', exact: true }).click();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(page.locator('.sound-visual')).toHaveAttribute('data-room', '2');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const bass = page.getByRole('slider', { name: '31 Hz device equalizer step', exact: true });
  await bass.focus();
  await bass.press('ArrowRight');
  await expect(bass).toBeDisabled();
  await expect(bass).toHaveValue('6');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(bass).toHaveValue('7');
});

test('Sony EQ curves use confirmed device steps; custom profiles rename and update without extra writes', async ({ page }) => {
  await installNativeFixture(page, true, {
    firmware: '3.1.5', deviceCapabilities: ['noise', 'equalizer'],
    deviceSettings: { equalizer: [6, 6, 6, 6, 6, 6, 6, 6, 6, 6] },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const selector = page.getByLabel('Device equalizer profile', { exact: true });
  const low = page.getByRole('slider', { name: '31 Hz device equalizer step', exact: true });
  await selector.selectOption({ label: 'Heavy' });
  await expect(low).toHaveValue('6');
  await expect(selector).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveValue('10');
  await expect(page.getByRole('slider', { name: '125 Hz device equalizer step', exact: true })).toHaveValue('5');
  await page.getByRole('button', { name: 'Save preset', exact: true }).click();
  await page.getByLabel('Preset name', { exact: true }).fill('My bass');
  await page.getByRole('form', { name: 'Save preset', exact: true }).getByRole('button', { name: 'Save preset', exact: true }).click();
  const callsBefore = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls.filter(x => x === 'apply_setting').length);
  await page.getByRole('button', { name: 'Rename “My bass”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Daily listening');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls.filter(x => x === 'apply_setting').length)).toBe(callsBefore);
  await low.fill('11');
  await expect(low).toHaveValue('10');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveValue('11');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('sound-connect:device-presets') ?? '[]'));
  expect(saved).toEqual([{ name: 'Daily listening', model: 'WH-1000XM6', firmware: '3.1.5', bands: [11, 10, 5, 5, 6, 6, 6, 6, 6, 6] }]);
  await page.reload();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply “Daily listening”', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Apply “Daily listening”', exact: true }).click();
  await expect(low).toHaveValue('6');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveValue('11');
});

test('rejected Sony preset leaves the previous confirmed EQ selection and bands intact', async ({ page }) => {
  await installNativeFixture(page, true, { deviceCapabilities: ['noise', 'equalizer'], deviceSettings: { equalizer: [6, 6, 6, 6, 6, 6, 6, 6, 6, 6] } });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const selector = page.getByLabel('Device equalizer profile', { exact: true });
  await selector.selectOption({ label: 'Game' });
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(selector).toHaveValue('sony:Off');
  await expect(page.getByRole('slider', { name: '31 Hz device equalizer step', exact: true })).toHaveValue('6');
  await expect(page.getByRole('alert')).toContainText('timed out');
});

test('Follow media uses native opt-in, displays readback status, and releases a manual hold', async ({ page }) => {
  await installNativeFixture(page, true, {
    deviceCapabilities: ['noise', 'listeningMode'],
    deviceSettings: { listeningMode: 'standard', controlDescriptors: [{ key: 'listeningMode', label: 'Listening mode', description: 'Device modes', kind: 'select', options: [{ value: 'standard', label: 'Standard' }, { value: 'cinema', label: 'Cinema' }] }] },
  });
  await page.goto('/');
  const panel = page.getByRole('region', { name: 'Automatic listening mode' });
  await expect(panel).toHaveCount(0);
  await page.getByRole('button', { name: 'Automatic listening', exact: true }).click();
  const toggle = panel.getByRole('switch', { name: 'Follow media', exact: true });
  await expect(toggle).toHaveAttribute('aria-checked', 'false');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-checked', 'true');
  await page.evaluate(() => (window as unknown as { __emitMedia: (change: Record<string, unknown>) => void }).__emitMedia({ status: 'following', source: 'www.netflix.com', desiredMode: 'cinema', reason: 'Cinema confirmed by headphones.', browserConnected: true }));
  await expect(panel).toContainText('www.netflix.com');
  await expect(panel).toContainText('Cinema confirmed by headphones.');
  await page.evaluate(() => (window as unknown as { __emitMedia: (change: Record<string, unknown>) => void }).__emitMedia({ status: 'manual', manualOverride: true }));
  await expect(panel).toContainText('Your manual mode is being respected');
  await panel.getByRole('button', { name: 'Resume', exact: true }).click();
  await expect(panel.getByRole('button', { name: 'Resume', exact: true })).toHaveCount(0);
  await panel.getByRole('button', { name: 'Browser companion setup', exact: true }).click();
  await expect(panel.getByText('a'.repeat(64), { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: 'Prepare Chrome / Edge extension', exact: true }).click();
  await expect(panel.getByText('C:/Test/Browser companion/chromium', { exact: true })).toBeVisible();
  await panel.screenshot({ path: 'tests/artifacts/media-following.png' });
  await page.setViewportSize({ width: 390, height: 900 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await panel.screenshot({ path: 'tests/artifacts/media-following-narrow.png' });
  const calls = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls).toContain('set_media_automation');
  expect(calls).toContain('resume_media_automation');
  expect(calls).not.toContain('apply_setting');
});

test('Follow media reports failed saves and remains switchable off without Cinema support', async ({ page }) => {
  await installNativeFixture(page, false, { mediaState: { enabled: true, status: 'device-unavailable' }, mediaSetError: 'Could not save media preferences.' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Automatic listening', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Automatic listening mode' });
  const toggle = panel.getByRole('switch', { name: 'Follow media', exact: true });
  await expect(toggle).toBeEnabled();
  await toggle.click();
  await expect(panel.getByRole('alert')).toContainText('Could not save media preferences.');
  await expect(toggle).toHaveAttribute('aria-checked', 'true');
});

test('VLC status tests remain read-only and opt-in credentials are cleared after save', async ({ page }) => {
  await installNativeFixture(page, false, { mediaState: { enabled: true, status: 'waiting' } });
  await page.goto('/');
  await page.getByRole('button', { name: 'Automatic listening', exact: true }).click();
  const vlc = page.getByRole('region', { name: 'VLC connection', exact: true });
  await expect(vlc).toContainText('Monitoring off');
  const password = vlc.getByLabel('VLC Lua HTTP password', { exact: true });
  await password.fill('fixture-local-password');
  await vlc.getByRole('button', { name: 'Test connection', exact: true }).click();
  await expect(vlc).toContainText('VLC connection test succeeded');
  await expect(vlc).toContainText('Monitoring off');
  let calls = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls).not.toContain('configure_vlc_connection');
  expect(calls).not.toContain('apply_setting');
  await vlc.getByRole('button', { name: 'Connect and save', exact: true }).click();
  await expect(vlc).toContainText('Monitoring enabled');
  await expect(password).toHaveValue('');
  await vlc.getByRole('button', { name: 'Disconnect', exact: true }).click();
  await expect(vlc).toContainText('Monitoring off');
  calls = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls.filter(command => command === 'configure_vlc_connection')).toHaveLength(2);
  expect(calls).not.toContain('apply_setting');
});

test('native controls stay unavailable without negotiated capabilities', async ({ page }) => {
  await installNativeFixture(page, false);
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Noise cancelling', exact: true })).toBeDisabled();
  await expect(page.getByText('Noise control is unavailable because this connected device has not confirmed its current mode.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider', { name: '63 Hz device equalizer step', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Exit preview', exact: true })).toHaveCount(0);
  const calls = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls).not.toContain('apply_setting');
});

test('native app preferences persist through IPC and reload from the desktop store', async ({ page }) => {
  await installNativeFixture(page, false);
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  const closeToTray = page.getByRole('switch', { name: 'Close to tray', exact: true });
  await expect(closeToTray).toHaveAttribute('aria-checked', 'true');
  await closeToTray.click();
  await expect(closeToTray).toHaveAttribute('aria-checked', 'false');
  await expect.poll(() => page.evaluate(() => localStorage.getItem('__fixtureNativePreferences'))).toContain('"closeToTray":false');
  await expect(page.evaluate(() => localStorage.getItem('sound-connect:app-preferences'))).resolves.toBeNull();

  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  await expect(page.getByRole('switch', { name: 'Close to tray', exact: true })).toHaveAttribute('aria-checked', 'false');
});

test('native app preference IPC failure leaves the last confirmed value in place', async ({ page }) => {
  await installNativeFixture(page, false, { preferenceUpdateError: 'The system rejected the preference update.' });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  const launchAtLogin = page.getByRole('switch', { name: 'Launch at login', exact: true });
  await expect(launchAtLogin).toHaveAttribute('aria-checked', 'false');
  await launchAtLogin.click();
  await expect(page.getByRole('alert')).toContainText('The system rejected the preference update.');
  await expect(launchAtLogin).toHaveAttribute('aria-checked', 'false');
});

test('native preference load failure does not present defaults as saved values and offers retry', async ({ page }) => {
  await installNativeFixture(page, false, { preferenceGetError: 'The native preferences store is unavailable.' });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('The native preferences store is unavailable.');
  await expect(page.getByText('Saved desktop preferences are unavailable until the native settings store responds.', { exact: true })).toBeVisible();
  await expect(page.getByRole('switch', { name: 'Close to tray', exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Retry', exact: true })).toBeEnabled();
});

test('newer disconnection event wins over delayed connected command response', async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextId = 0;
    const eventHandlers = new Map<string, number>();
    const device = { id: 'ordered-device', name: 'WH-1000XM6', model: 'WH-1000XM6', connection: 'disconnected', battery: null, firmware: null, capabilities: [], settings: {} };
    const snapshot = (revision: number, connection: string) => ({ revision, platform: 'Native IPC ordering fixture', transportStatus: connection, devices: [{ ...device, connection }], diagnostics: [] });
    Object.assign(window, {
      isTauri: true,
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { const id = ++nextId; callbacks.set(id, callback); return id; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: (command: string, args?: { event?: string; handler?: number }) => {
          if (command === 'plugin:event|listen') { eventHandlers.set(args?.event ?? '', args?.handler ?? 0); return Promise.resolve(1); }
          if (command.startsWith('plugin:event|')) return Promise.resolve();
          if (command === 'get_app_state') return Promise.resolve(snapshot(1, 'disconnected'));
          if (command === 'connect_device') return new Promise(resolve => {
            Object.assign(window, { __finishOutOfOrder: () => {
              callbacks.get(eventHandlers.get('app-state') ?? 0)?.({ event: 'app-state', id: 1, payload: snapshot(3, 'disconnected') });
              resolve(snapshot(2, 'connected'));
            } });
          });
          return Promise.reject(new Error(`Unexpected fixture command: ${command}`));
        },
      },
    });
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Connect headphone', exact: true }).click();
  await page.evaluate(() => (window as unknown as { __finishOutOfOrder: () => void }).__finishOutOfOrder());
  await expect(page.getByRole('button', { name: 'Connect headphone', exact: true })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Disconnect', exact: true })).toHaveCount(0);
  await expect(page.getByText('Control channel: disconnected', { exact: true })).toBeVisible();
});

test('pending native request does not change confirmed setting and failure is visible', async ({ page }) => {
  await installNativeFixture(page, true);
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  const confirmed = page.getByRole('button', { name: 'Noise cancelling', exact: true });
  const requested = page.getByRole('button', { name: 'Ambient sound', exact: true });
  await expect(confirmed).toHaveAttribute('aria-pressed', 'true');
  await requested.click();
  await expect(page.getByText('Waiting for device confirmation…', { exact: false })).toBeVisible();
  await expect(confirmed).toHaveAttribute('aria-pressed', 'true');
  await expect(requested).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByRole('alert')).toContainText('Last confirmed setting retained.');
  await expect(confirmed).toHaveAttribute('aria-pressed', 'true');
  await expect(requested).toHaveAttribute('aria-pressed', 'false');
  await expect(requested).toBeEnabled();
});

test('unconfirmed noise readback leaves every mode unselected and does not claim Off after a request', async ({ page }) => {
  await installNativeFixture(page, true, {
    noiseSettings: { noise: 'off', noiseConfirmation: 'unconfirmed' },
    applySettings: { noise: 'off', noiseConfirmation: 'unconfirmed', noiseRequested: 'cancelling' },
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });

  const cancelling = page.getByRole('button', { name: 'Noise cancelling', exact: true });
  const ambient = page.getByRole('button', { name: 'Ambient sound', exact: true });
  const off = page.getByRole('button', { name: 'Off', exact: true });
  await expect(page.locator('.noise-panel [role="status"]')).toContainText('Device state could not be verified.');
  await expect(page.locator('.noise-panel [role="status"]')).not.toContainText('Unknown requested');
  for (const mode of [cancelling, ambient, off]) {
    await expect(mode).toBeEnabled();
    await expect(mode).toHaveAttribute('aria-pressed', 'false');
  }

  await cancelling.click();
  await expect(cancelling).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(page.locator('.feedback[role="status"]')).toContainText('Noise cancelling requested. Device state could not be verified.');
  await expect(page.locator('.noise-panel [role="status"]')).toContainText('Noise cancelling requested. Device state could not be verified.');
  for (const mode of [cancelling, ambient, off]) {
    await expect(mode).toHaveAttribute('aria-pressed', 'false');
  }
  await expect(page.getByText('Off confirmed by your headphones.', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Off requested. Device state could not be verified.', { exact: true })).toHaveCount(0);
});

test('late native noise confirmation updates the pending notice for the selected device', async ({ page }) => {
  await installNativeFixture(page, true, {
    noiseSettings: { noise: 'off', noiseConfirmation: 'unconfirmed' },
    applySettings: { noise: 'off', noiseConfirmation: 'unconfirmed', noiseRequested: 'cancelling' },
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });

  const cancelling = page.getByRole('button', { name: 'Noise cancelling', exact: true });
  const off = page.getByRole('button', { name: 'Off', exact: true });
  const notice = page.locator('.feedback[role="status"]');
  await cancelling.click();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(notice).toContainText('Noise cancelling requested. Device state could not be verified.');
  await expect(notice.locator('svg.lucide-info')).toBeVisible();
  await expect(cancelling).toHaveAttribute('aria-pressed', 'false');
  await expect(off).toHaveAttribute('aria-pressed', 'false');

  await page.evaluate(() => (window as unknown as { __emitState: (settings: Record<string, unknown>) => void }).__emitState({
    noise: 'cancelling', noiseConfirmation: 'confirmed',
  }));
  await expect(notice).toContainText('Noise cancelling confirmed by your headphones.');
  await expect(notice.locator('svg.lucide-check')).toBeVisible();
  await expect(notice.locator('svg.lucide-info')).toHaveCount(0);
  await expect(cancelling).toHaveAttribute('aria-pressed', 'true');
  await expect(off).toHaveAttribute('aria-pressed', 'false');
});

test('descriptor-driven select, toggle, and range settings wait for device confirmation', async ({ page }) => {
  const descriptors = [
    { key: 'soundQuality', label: 'Bluetooth sound quality', description: 'Choose the device-reported sound-quality mode.', kind: 'select', options: [{ value: 'quality', label: 'Prioritize quality' }, { value: 'stable', label: 'Prioritize stable connection' }] },
    { key: 'focusOnVoice', label: 'Focus on voice', description: 'Emphasize speech in ambient mode.', kind: 'toggle' },
    { key: 'ambient', label: 'Ambient sound level', description: 'Device-reported ambient range.', kind: 'range', min: 2, max: 10, step: 2 },
  ];
  await installNativeFixture(page, false, {
    deviceCapabilities: ['soundQuality', 'focusOnVoice', 'ambient'],
    deviceSettings: { controlDescriptors: descriptors, soundQuality: 'quality', focusOnVoice: false, ambient: 4 },
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();

  const quality = page.getByRole('combobox', { name: 'Bluetooth sound quality' });
  await expect(quality).toHaveValue('0');
  await quality.selectOption('1');
  await expect(quality).toBeDisabled();
  await expect(page.locator('.device-control-action small[role="status"]').first()).toContainText('Waiting for headphone confirmation…');
  await expect(quality).toHaveValue('0');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(quality).toHaveValue('1');

  const focus = page.getByRole('switch', { name: 'Focus on voice' });
  await expect(focus).toHaveAttribute('aria-checked', 'false');
  await focus.click();
  await expect(focus).toBeDisabled();
  await expect(focus).toHaveAttribute('aria-checked', 'false');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(focus).toHaveAttribute('aria-checked', 'true');

  const ambient = page.getByRole('slider', { name: 'Ambient sound level' });
  await expect(ambient).toHaveAttribute('min', '2');
  await expect(ambient).toHaveAttribute('max', '10');
  await expect(ambient).toHaveAttribute('step', '2');
  await ambient.focus();
  await ambient.press('ArrowRight');
  await expect(ambient).toBeDisabled();
  await expect(ambient).toHaveValue('6');
  await expect(page.getByText('Device confirmed: 4. Waiting for headphone confirmation…', { exact: true })).toBeVisible();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(ambient).toHaveValue('6');
});

test('descriptor-driven setting failure keeps confirmed value and exposes the device error', async ({ page }) => {
  const descriptors = [{ key: 'soundQuality', label: 'Bluetooth sound quality', description: 'Choose the device-reported sound-quality mode.', kind: 'select', options: [{ value: 'quality', label: 'Prioritize quality' }, { value: 'stable', label: 'Prioritize stable connection' }] }];
  await installNativeFixture(page, false, {
    deviceCapabilities: ['soundQuality'],
    deviceSettings: { controlDescriptors: descriptors, soundQuality: 'quality' },
    applySettingError: 'The headphone did not confirm the selected mode.',
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();
  const quality = page.getByRole('combobox', { name: 'Bluetooth sound quality' });
  await quality.selectOption('1');
  await expect(quality).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.locator('.feedback.error[role="alert"]')).toContainText('The headphone did not confirm the selected mode.');
  await expect(quality).toHaveValue('0');
  await expect(quality).toBeEnabled();
});
