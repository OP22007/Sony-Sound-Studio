import { expect, test, type Page } from '@playwright/test';

async function installActivityFixture(page: Page, active: string | null) {
  await page.addInitScript(({ active }) => {
    const handlers = new Map<string, number>();
    const callbacks = new Map<number, (event: unknown) => void>();
    let callback = 0;
    const requests: string[] = [];
    const devices = [
      { id: 'sony-1', name: 'WH-1000XM5', connection: 'disconnected' },
      { id: 'sony-2', name: 'WH-1000XM6', connection: 'connected' },
      { id: 'oneplus-3', name: 'OnePlus Nord Buds 3 Pro', connection: 'connected' },
    ].map(device => ({ ...device, model: device.name, capabilities: [], settings: {}, battery: null, firmware: null }));
    let state = { revision: 1, platform: 'Windows fixture', transportStatus: 'Synthetic activity; no Bluetooth I/O', devices, diagnostics: [], activeOutputDeviceId: active, audioOutputObserved: !!active };
    Object.assign(window, {
      isTauri: true,
      __activityRequests: requests,
      __activityEmit: (revision: number, activeOutputDeviceId: string | null) => {
        state = { ...state, revision, activeOutputDeviceId };
        callbacks.get(handlers.get('app-state') ?? -1)?.({ event: 'app-state', id: 1, payload: structuredClone(state) });
      },
      __TAURI_INTERNALS__: {
        transformCallback: (fn: (event: unknown) => void) => { callbacks.set(++callback, fn); return callback; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: async (command: string, args?: { event?: string; handler?: number }) => {
          requests.push(command);
          if (command === 'plugin:event|listen') { handlers.set(args?.event ?? '', args?.handler ?? 0); return 1; }
          if (command.startsWith('plugin:event|')) return 1;
          if (command === 'get_app_state') return structuredClone(state);
          if (command === 'get_app_preferences') return { theme: 'dark', reduceMotion: true, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
          throw new Error(`Unexpected activity fixture command: ${command}`);
        },
      },
    });
  }, { active });
}

test('initial selection follows the actual audio output rather than the first paired device', async ({ page }) => {
  await installActivityFixture(page, 'sony-2');
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hero-copy h2')).toContainText('1000XM6');
  await expect(page.locator('.rail-device.selected')).toContainText('System audio output');
  expect(await page.evaluate(() => (window as unknown as { __activityRequests: string[] }).__activityRequests.filter(command => command === 'apply_setting' || command === 'connect_device'))).toEqual([]);
});

test('manual browsing persists until the OS changes output and stale events cannot steal selection', async ({ page }) => {
  await installActivityFixture(page, 'sony-2');
  await page.goto('/');
  const xm5 = page.getByRole('button', { name: 'Select WH-1000XM5', exact: true });
  const nord = page.getByRole('button', { name: 'Select OnePlus Nord Buds 3 Pro', exact: true });
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await xm5.click();
  await page.evaluate(() => (window as unknown as { __activityEmit: (revision: number, id: string) => void }).__activityEmit(2, 'sony-2'));
  await expect(xm5).toHaveAttribute('aria-pressed', 'true');
  await page.evaluate(() => (window as unknown as { __activityEmit: (revision: number, id: string) => void }).__activityEmit(3, 'oneplus-3'));
  await expect(nord).toHaveAttribute('aria-pressed', 'true');
  await page.evaluate(() => (window as unknown as { __activityEmit: (revision: number, id: string) => void }).__activityEmit(2, 'sony-2'));
  await expect(nord).toHaveAttribute('aria-pressed', 'true');
});

test('without an OS output match the studio initially prefers a live control session', async ({ page }) => {
  await installActivityFixture(page, null);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: 'Select WH-1000XM5', exact: true })).toHaveAttribute('aria-pressed', 'false');
});
