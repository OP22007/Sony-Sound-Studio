import { expect, test, type Page } from '@playwright/test';

type Snapshot = {
  currentVersion: string; status: string; version: string | null; notes: string | null;
  downloaded: number; total: number | null; error: string | null;
};

async function installUpdaterFixture(page: Page, options: {
  initial?: Partial<Snapshot>;
  check?: Partial<Snapshot>;
  checkError?: string;
  deferGet?: boolean;
  native?: boolean;
} = {}) {
  await page.addInitScript((fixture: typeof options) => {
    const callbacks = new Map<number, (event: unknown) => void>();
    const handlers = new Map<string, number>();
    let callbackId = 0;
    const calls: string[] = [];
    let update: Snapshot = {
      currentVersion: '0.1.1', status: 'idle', version: null, notes: null,
      downloaded: 0, total: null, error: null, ...fixture.initial,
    };
    const defaults = { theme: 'dark', reduceMotion: false, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
    const state = { revision: 1, platform: 'Updater fixture', transportStatus: 'Fixture', diagnostics: [], devices: [] };
    Object.assign(window, {
      isTauri: fixture.native !== false,
      __updateCalls: calls,
      __emitUpdate: (next: Partial<Snapshot>) => {
        update = { ...update, ...next };
        const handler = handlers.get('app-update');
        if (handler) callbacks.get(handler)?.({ event: 'app-update', id: 1, payload: { ...update } });
      },
      __resolveUpdateGet: () => {
        const resolve = (window as unknown as { __resolvePendingGet?: (value: Snapshot) => void }).__resolvePendingGet;
        resolve?.({ ...update, status: 'idle', version: null, downloaded: 0, total: null });
      },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { const id = ++callbackId; callbacks.set(id, callback); return id; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: (command: string, args?: { event?: string; handler?: number }) => {
          calls.push(command);
          if (command === 'plugin:event|listen') { handlers.set(args?.event ?? '', args?.handler ?? 0); return Promise.resolve(1); }
          if (command.startsWith('plugin:event|')) return Promise.resolve(1);
          if (command === 'get_app_state') return Promise.resolve(state);
          if (command === 'get_app_preferences') return Promise.resolve(defaults);
          if (command === 'get_app_update') {
            if (fixture.deferGet) return new Promise(resolve => Object.assign(window, { __resolvePendingGet: resolve }));
            return Promise.resolve({ ...update });
          }
          if (command === 'check_app_update') {
            if (fixture.checkError) return Promise.reject(new Error(fixture.checkError));
            update = { ...update, ...fixture.check, status: fixture.check?.status ?? 'upToDate' };
            return Promise.resolve({ ...update });
          }
          if (command === 'install_app_update') return new Promise(() => {});
          return Promise.reject(new Error(`Unexpected fixture command: ${command}`));
        },
      },
    });
  }, options);
}

async function openUpdater(page: Page) {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  return page.locator('.app-updates');
}

test('updater subscribes before loading snapshot and ignores a stale snapshot after a newer event', async ({ page }) => {
  await installUpdaterFixture(page, { deferGet: true });
  const card = await openUpdater(page);
  await expect.poll(() => page.evaluate(() => (window as unknown as { __updateCalls: string[] }).__updateCalls)).toContain('get_app_update');
  await page.evaluate(() => (window as unknown as { __emitUpdate: (next: Partial<Snapshot>) => void }).__emitUpdate({ status: 'available', version: '0.1.2', notes: 'A newer release.' }));
  await expect(card).toContainText('Version 0.1.2 is ready to install.');
  await page.evaluate(() => (window as unknown as { __resolveUpdateGet: () => void }).__resolveUpdateGet());
  await expect(card).toContainText('Version 0.1.2 is ready to install.');

  const calls = await page.evaluate(() => (window as unknown as { __updateCalls: string[] }).__updateCalls);
  expect(calls.indexOf('plugin:event|listen')).toBeLessThan(calls.indexOf('get_app_update'));
  expect(calls).not.toContain('check_app_update');
  await expect(page.locator('.about-note')).toContainText('0.1.1');
});

test('successful update check reports that the app is current', async ({ page }) => {
  await installUpdaterFixture(page, { check: { status: 'upToDate' } });
  const card = await openUpdater(page);
  await card.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(card).toContainText('You’re running the latest version.');
  await expect(page.locator('.about-note')).toContainText('0.1.1');
  const calls = await page.evaluate(() => (window as unknown as { __updateCalls: string[] }).__updateCalls);
  expect(calls).toContain('check_app_update');
});

test('available update installs and keeps download/install progress visible while IPC is pending', async ({ page }) => {
  await installUpdaterFixture(page, {
    check: { status: 'available', version: '0.1.2', notes: 'Reliability improvements.' },
  });
  const card = await openUpdater(page);
  await card.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(card).toContainText('Version 0.1.2 is ready to install.');
  await expect(card).toContainText('Reliability improvements.');
  await card.getByRole('button', { name: 'Install & restart', exact: true }).click();
  await expect(card).toContainText('Installing update… the app will restart shortly.');
  await page.evaluate(() => (window as unknown as { __emitUpdate: (next: Partial<Snapshot>) => void }).__emitUpdate({ status: 'downloading', downloaded: 512, total: null }));
  await expect(card).toContainText('Downloading update…');
  await expect(card.locator('progress')).not.toHaveAttribute('value', /.+/);
  await page.evaluate(() => (window as unknown as { __emitUpdate: (next: Partial<Snapshot>) => void }).__emitUpdate({ status: 'downloading', downloaded: 512, total: 1024 }));
  await expect(card).toContainText('Downloading update… 50%');
  await page.evaluate(() => (window as unknown as { __emitUpdate: (next: Partial<Snapshot>) => void }).__emitUpdate({ status: 'installing' }));
  await expect(card).toContainText('Installing update… the app will restart shortly.');
});

test('failed update check shows the native error and offers retry', async ({ page }) => {
  await installUpdaterFixture(page, { checkError: 'Release server is unavailable.' });
  const card = await openUpdater(page);
  await card.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(card.getByRole('alert')).toContainText('Release server is unavailable.');
  await expect(card.getByRole('button', { name: 'Try again', exact: true })).toBeEnabled();
});

test('unsupported development and Linux package builds show the native reason and disable checking', async ({ page }) => {
  for (const reason of [
    'In-app installation is available in packaged release builds. Development builds must be updated from source.',
    'In-app updates require the AppImage edition on Linux. Update DEB/RPM installations using a new package from Releases.',
  ]) {
    await installUpdaterFixture(page, { initial: { status: 'unsupported', error: reason } });
    const card = await openUpdater(page);
    await expect(card.getByRole('status')).toContainText(reason);
    await expect(card.getByRole('button', { name: 'Unavailable', exact: true })).toBeDisabled();
    await expect(page.evaluate(() => (window as unknown as { __updateCalls: string[] }).__updateCalls)).resolves.not.toContain('check_app_update');
    await page.reload({ waitUntil: 'domcontentloaded' });
  }
});

test('browser studio preview explains that app updates are unavailable', async ({ page }) => {
  await installUpdaterFixture(page, { native: false });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Explore studio preview', exact: true }).click();
  await page.getByRole('button', { name: 'App settings', exact: true }).click();
  const card = page.locator('.app-updates');
  await expect(card.getByRole('status')).toContainText('App updates are unavailable in browser preview.');
  await expect(card.getByRole('button', { name: 'Unavailable', exact: true })).toBeDisabled();
});
