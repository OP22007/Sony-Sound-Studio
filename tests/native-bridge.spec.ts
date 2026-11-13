import { expect, test, type Page } from '@playwright/test';

test('COLLEXION exposes five advertised modes, waits for confirmation and suspends EQ and DSEE in upmix',async({page})=>{
  const modes=[{value:'standard',label:'Standard'},{value:'music',label:'Music'},{value:'cinema',label:'360 Upmix for Cinema'},{value:'game',label:'360 Upmix for Game'},{value:'backgroundMusic',label:'Background Music'}];
  await installNativeFixture(page,true,{
    model:'WH-1000XX',firmware:'test-firmware',deviceCapabilities:['noise','listeningMode','bgmRoomSize','equalizer','dsee'],
    applySettings:{listeningMode:'music'},
    deviceSettings:{listeningMode:'standard',bgmRoomSize:2,equalizer:Array(10).fill(6),dsee:'auto',controlDescriptors:[
      {key:'listeningMode',label:'Listening mode',description:'Device modes',kind:'select',options:modes},
      {key:'bgmRoomSize',label:'Background Music distance',description:'Device rooms',kind:'select',options:[{value:0,label:'My room'},{value:1,label:'Living room'},{value:2,label:'Cafe'}]},
      {key:'dsee',label:'DSEE Ultimate',description:'Device DSEE',kind:'select',options:[{value:'off',label:'Off'},{value:'auto',label:'Auto'}]},
    ]},
  });
  await page.goto('/');
  await expect(page.getByRole('group',{name:'Sound modes'}).getByRole('button')).toHaveCount(5);
  await expect(page.locator('.sound-visual img')).toHaveAttribute('src','/headphones/sony-1000x-collexion-black.png');
  const music=page.getByRole('button',{name:'Music The space of a live studio',exact:true});
  await music.click();await expect(music).toHaveAttribute('aria-pressed','false');
  await page.evaluate(()=>(window as unknown as {__finishApply:()=>void}).__finishApply());
  await expect(music).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('.sound-visual')).toHaveAttribute('data-scene','music');
  await page.getByRole('button',{name:'Equalizer',exact:true}).click();
  await expect(page.getByRole('slider').first()).toBeDisabled();
  await expect(page.getByLabel('DSEE mode',{exact:true})).toBeDisabled();
  await expect(page.getByRole('heading',{name:'DSEE Ultimate',exact:true})).toBeVisible();
  await page.evaluate(()=>(window as unknown as {__emitState:(s:Record<string,unknown>)=>void}).__emitState({listeningMode:'standard'}));
  await expect(page.getByRole('slider').first()).toBeEnabled();await expect(page.getByLabel('DSEE mode',{exact:true})).toBeEnabled();
});

async function installNativeFixture(page: Page, noiseSupported: boolean, options: {
  firmware?: string;
  model?: string;
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
  historySummary?: Record<string, unknown>;
  historyError?: string;
  updateAvailable?: boolean;
  updateCheckDelayMs?: number;
} = {}) {
  await page.addInitScript(({ noiseSupported, options }) => {
    const calls: string[] = [];
    const requests: Array<{ command: string; args?: Record<string, unknown> }> = [];
    const callbacks = new Map<number, (event: unknown) => void>();
    let nextCallback = 0;
    const eventHandlers = new Map<string, number[]>();
    const emit = (name: string, id: number, payload: unknown) => (eventHandlers.get(name) ?? []).forEach(handler => callbacks.get(handler)?.({ event: name, id, payload }));
    const preferenceStorageKey = '__fixtureNativePreferences';
    const defaultPreferences = { theme: 'dark', reduceMotion: false, closeToTray: true, launchAtLogin: false, autoReconnect: true, startMinimized: false };
    let appPreferences = JSON.parse(localStorage.getItem(preferenceStorageKey) ?? 'null') ?? { ...defaultPreferences, ...options.appPreferences };
    let media = { enabled: false, status: 'disabled', source: null, desiredMode: null, reason: 'Follow media is off.', manualOverride: false, extensionToken: 'a'.repeat(64), browserConnected: false, ...options.mediaState };
    let updateState: Record<string, unknown> = { currentVersion: '0.1.6', status: 'idle', version: null, notes: null, downloaded: 0, total: null, error: null };
    (window as unknown as { __updateCheckDelayMs: number }).__updateCheckDelayMs = options.updateCheckDelayMs ?? 8000;
    let historySettings = { enabled: true, details: !!options.historySummary?.details, exact: !!options.historySummary?.exact };
    let historyCleared = false;
    const emptyHistory = { totalMs: 0, firstMs: null, days: [], modes: [], noise: [], devices: [], knownDevices: [], categories: [], sources: [], titles: [], urls: [], recent: [], skipped: 0, writeError: null, contentSupported: true };
    let vlc = { enabled: false, passwordConfigured: false, status: 'VLC connection is off.', playing: false, kind: null as string | null };
    const state = {
      revision: 1,
      platform: 'Native IPC test fixture',
      transportStatus: 'Controlled integration fixture; no physical Bluetooth access',
      diagnostics: [],
      devices: [{
        id: 'test-device', name: options.model ?? 'WH-1000XM6', model: options.model ?? 'WH-1000XM6', connection: 'connected',
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
      __fixtureRequests: requests,
      __emitMedia: (change: Record<string, unknown>) => {
        media = { ...media, ...change };
        emit('media-automation', 2, { ...media });
      },
      __emitState: (settings: Record<string, unknown>) => {
        Object.assign(state.devices[0].settings, settings);
        state.revision += 1;
        emit('app-state', 1, snapshot());
      },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { const id = ++nextCallback; callbacks.set(id, callback); return id; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: (command: string, args?: { range?: string; details?: boolean; exact?: boolean; password?: string; enabled?: boolean; event?: string; handler?: number; preferences?: unknown; setting?: string; value?: unknown; deviceId?: string }) => {
          calls.push(command);
          requests.push({ command, args: args as Record<string, unknown> | undefined });
          if (command === 'plugin:event|listen') { eventHandlers.set(args?.event ?? '', [...(eventHandlers.get(args?.event ?? '') ?? []), args?.handler ?? 0]); return Promise.resolve(1); }
          if (command.startsWith('plugin:event|')) return Promise.resolve(1);
          if (command === 'get_app_state') return Promise.resolve(snapshot());
          if (command === 'get_app_update') return Promise.resolve({ ...updateState });
          if (command === 'check_app_update') {
            updateState = options.updateAvailable
              ? { ...updateState, status: 'available', version: '0.9.9', notes: 'Faster connections, a Multipoint tab and a new equalizer.' }
              : { ...updateState, status: 'upToDate' };
            emit('app-update', 3, { ...updateState });
            return Promise.resolve({ ...updateState });
          }
          if (command === 'install_app_update') {
            updateState = { ...updateState, status: 'installing' };
            emit('app-update', 3, { ...updateState });
            return Promise.resolve({ ...updateState });
          }
          if (command === 'get_media_automation') return Promise.resolve({ ...media });
          if (command === 'get_history') {
            if (options.historyError) return Promise.reject(new Error(options.historyError));
            const base = historyCleared || !options.historySummary ? emptyHistory : options.historySummary;
            return Promise.resolve({ ...base, range: args?.range, ...historySettings });
          }
          if (command === 'get_history_settings') return Promise.resolve({ ...historySettings });
          if (command === 'set_history_settings') { historySettings = { enabled: !!args?.enabled, details: !!args?.details, exact: !!args?.exact }; return Promise.resolve({ ...historySettings }); }
          if (command === 'clear_history') { historyCleared = true; return Promise.resolve(null); }
          if (command === 'get_vlc_connection') return Promise.resolve({ ...vlc });
          if (command === 'test_vlc_connection') return Promise.resolve({ ...vlc, playing: true, kind: 'movie', status: 'VLC connection test succeeded; playback detected.' });
          if (command === 'configure_vlc_connection') {
            vlc = { ...vlc, enabled: !!args?.enabled, passwordConfigured: vlc.passwordConfigured || !!args?.password, status: 'Waiting for a local VLC status check.' };
            return Promise.resolve({ ...vlc });
          }
          if (command === 'configure_vlc_startup') return Promise.resolve({ ...vlc, startupConfigured: true, status: 'VLC startup saved. Open VLC normally.' });
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
  await expect(bass).toBeEnabled();
  await expect(bass).toHaveAttribute('aria-valuenow', '7');
  await page.getByRole('button', { name: 'Apply equalizer', exact: true }).click();
  await expect(bass).toBeDisabled();
  await expect(bass).toHaveAttribute('aria-valuenow', '7');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(bass).toHaveAttribute('aria-valuenow', '7');
});

test('OnePlus Nord Buds 3 Pro exposes generic noise control without Sony controls or presets', async ({ page }) => {
  await installNativeFixture(page, true, {
    model: 'OnePlus Nord Buds 3 Pro',
    deviceCapabilities: ['noiseProfile'],
    deviceSettings: { noiseProfile: 2, controlDescriptors: [{ key: 'noiseProfile', label: 'Noise profile', description: 'Supported OnePlus noise modes', kind: 'select', options: [{ value: 0, label: 'Off' }, { value: 1, label: 'Noise cancelling' }, { value: 2, label: 'Transparency' }, { value: 4, label: 'Max' }, { value: 5, label: 'Moderate' }, { value: 6, label: 'Mild' }, { value: 7, label: 'Smart' }] }], protocolFormat: 'heymelody-opo-v1', protocolStatus: 'HeyMelody OPO response confirmed' },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Device settings' }).click();
  await expect(page.getByRole('heading', { name: 'OnePlus Nord Buds 3 Pro' })).toBeVisible();
  await expect(page.getByText('heymelody-opo-v1', { exact: true })).toBeVisible();
  await expect(page.getByText(/Sony|MDR|DSEE|Listening mode \(Standard/)).toHaveCount(0);
  await page.getByRole('button', { name: 'Feature coverage' }).click();
  await expect(page.getByRole('heading', { name: 'OnePlus control coverage' })).toBeVisible();
  await expect(page.getByText(/XM6|Sound Connect 13\.3\.0|ten-band/)).toHaveCount(0);
  await expect(page.getByText('Noise profiles', { exact: true })).toBeVisible();
  await expect(page.getByText('Equalizer profiles', { exact: true })).toBeVisible();
  await expect(page.getByText('Battery', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Equalizer' }).click();
  await expect(page.getByRole('group', { name: 'Sony equalizer presets' })).toHaveCount(0);
  await expect(page.getByText(/No confirmed factory sound-profile reply yet/)).toBeVisible();
  await page.getByRole('button', { name: 'Sound studio' }).click();
  await page.getByLabel('Noise profile').selectOption('1');
  const calls = await page.evaluate(() => (window as Window & { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls).toContain('apply_setting');
});

test('OnePlus artwork loads for the exact model without substituting a nearby model name', async ({ page }) => {
  for (const model of ['OnePlus Nord Buds 3 Pro', 'OnePlus Buds Pro 3', 'OnePlus Buds Pro 33']) {
    const variant = await page.context().newPage();
    await installNativeFixture(variant, false, { model, deviceSettings: { protocolFormat: 'heymelody-opo-v1' } });
    await variant.goto('/');
    if (model.endsWith('33')) {
      await expect(variant.getByText('OnePlus product artwork unavailable').first()).toBeVisible();
      await expect(variant.locator('.product-render')).toHaveCount(0);
    } else {
      const art = variant.locator('img.product-render').first();
      await expect(art).toBeVisible();
      await expect(art).toHaveAttribute('alt', new RegExp(model));
      await expect.poll(() => art.evaluate((element: HTMLImageElement) => element.complete && element.naturalWidth > 100 && element.naturalHeight > 100)).toBe(true);
    }
    await variant.close();
  }
});

for (const bass of [
  { model: 'OnePlus Nord Buds 3 Pro', key: 'bassWave', label: 'BassWave', min: -5, max: 5 },
  { model: 'WH-1000XM5', key: 'clearBass', label: 'CLEAR BASS', min: -10, max: 10 },
]) test(`${bass.label} stages the slider and applies the model-specific key only after explicit apply`, async ({ page }) => {
  await installNativeFixture(page, false, {
    model: bass.model, deviceCapabilities: [bass.key],
    deviceSettings: { [bass.key]: 0, controlDescriptors: [{ key: bass.key, label: bass.label, description: 'Confirmed bass range', kind: 'range', min: bass.min, max: bass.max, step: 1 }] },
    applySettings: { [bass.key]: 1 },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const slider = page.getByRole('slider', { name: `${bass.label} level`, exact: true });
  await expect(slider).toHaveAttribute('min', String(bass.min));
  await expect(slider).toHaveAttribute('max', String(bass.max));
  await slider.focus(); await slider.press('ArrowRight');
  await expect(page.getByText('Unsaved · device level 0', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __fixtureRequests: { command: string }[] }).__fixtureRequests.filter(item => item.command === 'apply_setting'))).toHaveLength(0);
  await page.getByRole('button', { name: 'Apply bass', exact: true }).click();
  await expect(slider).toBeDisabled();
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: { command: string; args: unknown }[] }).__fixtureRequests);
  expect(requests.find(item => item.command === 'apply_setting')?.args).toMatchObject({ setting: bass.key, value: 1 });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(slider).toHaveValue('1');
  await expect(page.getByText('Confirmed device level +1', { exact: true })).toBeVisible();
});

test('BassWave enablement keeps its saved level and waits for switch confirmation', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro', deviceCapabilities: ['bassWave', 'bassWaveEnabled'],
    deviceSettings: { bassWave: 2, bassWaveEnabled: false, controlDescriptors: [
      { key: 'bassWave', label: 'BassWave', description: '', kind: 'range', min: -5, max: 5, step: 1 },
      { key: 'bassWaveEnabled', label: 'BassWave enabled', description: '', kind: 'toggle' },
    ] }, applySettings: { bassWaveEnabled: true },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const slider = page.getByRole('slider', { name: 'BassWave level', exact: true });
  await expect(slider).toBeDisabled(); await expect(slider).toHaveValue('2');
  const toggle = page.getByRole('switch', { name: 'Enable BassWave', exact: true });
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-checked', 'false');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(toggle).toHaveAttribute('aria-checked', 'true');
  await expect(slider).toBeEnabled(); await expect(slider).toHaveValue('2');
});

test('a rejected bass change retains the confirmed device level and an editable draft', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro', deviceCapabilities: ['bassWave'],
    deviceSettings: { bassWave: 0, controlDescriptors: [{ key: 'bassWave', label: 'BassWave', description: '', kind: 'range', min: -5, max: 5, step: 1 }] },
    applySettingError: 'BassWave readback did not match.',
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const slider = page.getByRole('slider', { name: 'BassWave level', exact: true });
  await slider.focus(); await slider.press('ArrowRight');
  await page.getByRole('button', { name: 'Apply bass', exact: true }).click();
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByText('BassWave readback did not match.', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Unsaved · device level 0', { exact: true })).toBeVisible();
  await expect(slider).toBeEnabled();
  await page.getByRole('button', { name: 'Discard', exact: true }).click();
  await expect(slider).toHaveValue('0');
  await expect(page.getByText('Confirmed device level 0', { exact: true })).toBeVisible();
});

test('bass sliders require a valid vendor-specific descriptor and current value', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro', deviceCapabilities: ['bassWave', 'clearBass'],
    deviceSettings: { bassWave: 20, clearBass: 0, controlDescriptors: [
      { key: 'bassWave', label: 'BassWave', description: '', kind: 'range', min: -5, max: 5, step: 1 },
      { key: 'clearBass', label: 'CLEAR BASS', description: '', kind: 'range', min: -10, max: 10, step: 1 },
    ] },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider', { name: /BassWave level|CLEAR BASS level/ })).toHaveCount(0);
});

test('OnePlus equalizer uses only a confirmed eqPreset descriptor', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro',
    deviceCapabilities: ['eqPreset'],
    deviceSettings: {
      protocolFormat: 'heymelody-opo-v1', eqPreset: 3,
      controlDescriptors: [{ key: 'eqPreset', label: 'Sound profile', description: 'Available profiles', kind: 'select', options: [{ value: 3, label: 'Balanced' }, { value: 4, label: 'Pure Voice' }, { value: 5, label: 'Bass Plus' }] }],
    },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await page.getByLabel('Sound profile').selectOption('5');
  const calls = await page.evaluate(() => (window as Window & { __fixtureCalls: string[] }).__fixtureCalls);
  expect(calls).toContain('apply_setting');
});

test('an observed custom OnePlus mode keeps factory profiles available without making the unknown mode writable', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro',
    deviceCapabilities: ['eqPreset'],
    deviceSettings: {
      protocolFormat: 'heymelody-opo-v1', eqPreset: 128,
      controlDescriptors: [{ key: 'eqPreset', label: 'Equalizer preset', description: 'Model profiles', kind: 'select', options: [
        { value: 3, label: 'Balanced' }, { value: 4, label: 'Pure Voice' }, { value: 5, label: 'Bass Plus' },
        { value: 128, label: 'Current device profile (128)', enabled: false },
      ] }],
    },
    applySettings: { eqPreset: 3 },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const profile = page.getByLabel('Sound profile');
  await expect(profile).toHaveValue('128');
  await expect(profile.getByRole('option', { name: 'Current device profile (128)' })).toBeDisabled();
  await profile.selectOption('3');
  await expect(profile).toBeDisabled();
  await expect(profile).toHaveValue('128');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(profile).toHaveValue('3');
});

test('OnePlus custom EQ uses the reported band layout and sends edits and names only on explicit apply', async ({ page }) => {
  const profile = { selected: 1, minValue: -6, maxValue: 6, eqId: 128, name: 'My listening', frequencies: [100, 200, 400, 800, 1600, 3200], dbValues: [0, 1, 0, -1, 0, 2] };
  const confirmed = { ...profile, name: 'Evening', dbValues: [1, 1, 0, -1, 0, 2] };
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro', deviceCapabilities: ['oneplusEqProfile'],
    deviceSettings: { protocolFormat: 'heymelody-opo-v1', oneplusEqProfile: true, oneplusEqualizerProfiles: [profile] },
    applySettings: { oneplusEqualizerProfiles: [confirmed] },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider')).toHaveCount(6);
  await expect(page.getByRole('heading', { name: 'DSEE Extreme', exact: true })).toHaveCount(0);
  const aligned = await page.locator('.oneplus-profile-editor').evaluate(element => {
    const points = [...element.querySelectorAll('.eq-point')];
    const sliders = [...element.querySelectorAll('.band')];
    return points.every((point, index) => {
      const p = point.getBoundingClientRect(); const s = sliders[index].getBoundingClientRect();
      return Math.abs(p.x + p.width / 2 - s.x - s.width / 2) < 3;
    });
  });
  expect(aligned).toBe(true);
  const bass = page.getByRole('slider', { name: '100 Hz custom equalizer', exact: true });
  await bass.focus(); await bass.press('ArrowRight');
  await page.getByLabel('Profile name', { exact: true }).fill('Evening');
  expect(await page.evaluate(() => (window as unknown as { __fixtureRequests: { command: string }[] }).__fixtureRequests.filter(item => item.command === 'apply_setting'))).toHaveLength(0);
  await page.getByRole('button', { name: 'Save & apply profile' }).click();
  await expect(bass).toBeDisabled();
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: { command: string; args: unknown }[] }).__fixtureRequests);
  expect(requests.find(item => item.command === 'apply_setting')?.args).toMatchObject({ setting: 'oneplusEqProfile', value: { action: 2, profile: confirmed } });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(page.getByLabel('Profile name', { exact: true })).toHaveValue('Evening');
  await expect(page.getByRole('button', { name: 'Apply profile', exact: true })).toBeEnabled();
  await page.screenshot({ path: 'tests/artifacts/oneplus-custom-equalizer.png', fullPage: true });
});

test('OnePlus creates a profile from its model template and waits for the device-assigned ID', async ({ page }) => {
  const template = { selected: 1, minValue: -6, maxValue: 6, eqId: 0, name: '', frequencies: [62, 250, 1000, 4000, 8000, 16000], dbValues: [0, 0, 0, 0, 0, 0] };
  const confirmed = { ...template, eqId: 129, name: 'Late night' };
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro', deviceCapabilities: ['oneplusEqProfile'],
    deviceSettings: { protocolFormat: 'heymelody-opo-v1', oneplusEqProfile: true, oneplusEqualizerProfiles: [], oneplusEqTemplate: template },
    applySettings: { oneplusEqualizerProfiles: [confirmed] },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await page.getByRole('button', { name: 'New profile', exact: true }).click();
  await expect(page.getByRole('slider')).toHaveCount(6);
  await page.getByLabel('Profile name', { exact: true }).fill('Late night');
  await page.getByRole('button', { name: 'Create & apply profile' }).click();
  await expect(page.getByText('NEW PROFILE · NOT YET SAVED')).toBeVisible();
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: { command: string; args: unknown }[] }).__fixtureRequests);
  expect(requests.find(item => item.command === 'apply_setting')?.args).toMatchObject({ setting: 'oneplusEqProfile', value: { action: 1, profile: { ...template, name: 'Late night' } } });
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(page.getByText('NEW PROFILE · NOT YET SAVED')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Late night' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply profile', exact: true })).toBeEnabled();
});

test('malformed OnePlus custom curves do not expose editable sliders', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Buds Pro 3', deviceCapabilities: ['oneplusEqProfile'],
    deviceSettings: { protocolFormat: 'heymelody-opo-v1', oneplusEqProfile: true, oneplusEqualizerProfiles: [
      { selected: 1, minValue: -6, maxValue: 6, eqId: 128, name: 'Bad curve', frequencies: [100, 100], dbValues: [0, 20] },
    ] },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider')).toHaveCount(0);
  await expect(page.getByText(/Custom bands appear when/)).toBeVisible();
});

test('OnePlus model descriptors keep Buds Pro 3 indices independent from Nord Buds profiles', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Buds Pro 3',
    deviceCapabilities: ['eqPreset'],
    deviceSettings: {
      protocolFormat: 'heymelody-opo-v1', eqPreset: 12,
      controlDescriptors: [{ key: 'eqPreset', label: 'Buds Pro 3 sound profile', description: 'This model’s profiles', kind: 'select', options: [{ value: 10, label: 'Studio' }, { value: 12, label: 'Warm' }] }],
    },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const profile = page.getByLabel('Sound profile');
  await expect(profile).toHaveValue('12');
  await expect(profile.locator('option')).toHaveText(['Studio', 'Warm']);
  await profile.selectOption('10');
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests);
  expect(requests.find(request => request.command === 'apply_setting')?.args).toMatchObject({ setting: 'eqPreset', value: 10 });
});

test('original OnePlus Buds and Bullets without descriptors never show Sony EQ or ANC controls', async ({ page }) => {
  for (const model of ['OnePlus Buds', 'OnePlus Bullets Wireless Z2']) {
    const variantPage = await page.context().newPage();
    await installNativeFixture(variantPage, false, { model, deviceCapabilities: [], deviceSettings: {} });
    await variantPage.goto('/');
    await variantPage.getByRole('button', { name: 'Device settings', exact: true }).click();
    await expect(variantPage.getByText('Not confirmed for this device', { exact: true })).toHaveCount(0);
    await variantPage.getByRole('button', { name: 'Equalizer', exact: true }).click();
    await expect(variantPage.getByText(/No confirmed factory sound-profile reply yet/)).toBeVisible();
    await expect(variantPage.getByRole('group', { name: 'Sony equalizer presets' })).toHaveCount(0);
    await variantPage.getByRole('button', { name: 'Feature coverage' }).click();
    await expect(variantPage.getByRole('heading', { name: 'OnePlus control coverage' })).toBeVisible();
    await expect(variantPage.getByText(/No equalizer profile control has been confirmed for this device/)).toBeVisible();
    await expect(variantPage.getByText(/Sony factory|XM6|ten-band/)).toHaveCount(0);
    await variantPage.getByRole('button', { name: 'Sound studio' }).click();
    await expect(variantPage.getByRole('group', { name: 'Noise control' })).toHaveCount(0);
    await variantPage.close();
  }
});

test('OnePlus wear-detection and game-mode descriptors show confirmed toggles and send their setting keys', async ({ page }) => {
  await installNativeFixture(page, false, {
    model: 'OnePlus Nord Buds 3 Pro',
    deviceCapabilities: ['wearDetection', 'gameMode'],
    deviceSettings: {
      protocolFormat: 'heymelody-opo-v1', wearDetection: true, gameMode: false,
      controlDescriptors: [
        { key: 'wearDetection', label: 'Wear detection', description: 'Pause when removed.', kind: 'toggle' },
        { key: 'gameMode', label: 'Game mode', description: 'Reduce audio latency.', kind: 'toggle' },
      ],
    },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();
  const wear = page.getByRole('switch', { name: 'Wear detection' });
  const game = page.getByRole('switch', { name: 'Game mode' });
  await expect(wear).toHaveAttribute('aria-checked', 'true');
  await expect(game).toHaveAttribute('aria-checked', 'false');
  await wear.click();
  await expect(wear).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(wear).toHaveAttribute('aria-checked', 'false');
  await expect(wear).toBeEnabled();
  await game.click();
  await expect(game).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(game).toHaveAttribute('aria-checked', 'true');
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests);
  expect(requests.filter(request => request.command === 'apply_setting').map(request => [request.args?.setting, request.args?.value])).toEqual([
    ['wearDetection', false], ['gameMode', true],
  ]);
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
  await expect(low).toHaveAttribute('aria-valuenow', '6');
  await expect(selector).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveAttribute('aria-valuenow', '10');
  await expect(page.getByRole('slider', { name: '125 Hz device equalizer step', exact: true })).toHaveAttribute('aria-valuenow', '5');
  await page.getByRole('button', { name: 'Save preset', exact: true }).click();
  await page.getByLabel('Preset name', { exact: true }).fill('My bass');
  await page.getByRole('form', { name: 'Save preset', exact: true }).getByRole('button', { name: 'Save preset', exact: true }).click();
  const callsBefore = await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls.filter(x => x === 'apply_setting').length);
  await page.getByRole('button', { name: 'Rename “My bass”', exact: true }).click();
  await page.getByLabel('New preset name').fill('Daily listening');
  await page.getByRole('button', { name: 'Rename preset', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls.filter(x => x === 'apply_setting').length)).toBe(callsBefore);
  await low.focus();
  await low.press('ArrowRight');
  await expect(low).toHaveAttribute('aria-valuenow', '11');
  await page.getByRole('button', { name: 'Apply equalizer', exact: true }).click();
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveAttribute('aria-valuenow', '11');
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('sound-connect:device-presets') ?? '[]'));
  expect(saved).toEqual([{ name: 'Daily listening', model: 'WH-1000XM6', firmware: '3.1.5', bands: [11, 10, 5, 5, 6, 6, 6, 6, 6, 6] }]);
  await page.reload();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply “Daily listening”', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Apply “Daily listening”', exact: true }).click();
  await expect(low).toHaveAttribute('aria-valuenow', '6');
  await page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());
  await expect(low).toHaveAttribute('aria-valuenow', '11');
});

test('rejected Sony preset leaves the previous confirmed EQ selection and bands intact', async ({ page }) => {
  await installNativeFixture(page, true, { deviceCapabilities: ['noise', 'equalizer'], deviceSettings: { equalizer: [6, 6, 6, 6, 6, 6, 6, 6, 6, 6] } });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const selector = page.getByLabel('Device equalizer profile', { exact: true });
  await selector.selectOption({ label: 'Game' });
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(selector).toHaveValue('sony:Off');
  await expect(page.getByRole('slider', { name: '31 Hz device equalizer step', exact: true })).toHaveAttribute('aria-valuenow', '6');
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
  await page.evaluate(() => (window as unknown as { __emitMedia: (change: Record<string, unknown>) => void }).__emitMedia({ status: 'following', source: 'www.netflix.com', desiredMode: 'movie', plannedSettings: ['Listening mode: cinema'], reason: 'Cinema confirmed by headphones.', browserConnected: true }));
  await expect(panel).toContainText('www.netflix.com');
  await expect(panel.locator('.mf-readout')).toHaveText('Movie');
  await expect(panel).toContainText('Listening mode: cinema');
  await expect(panel).toContainText('Cinema confirmed by headphones.');
  await page.evaluate(() => (window as unknown as { __emitMedia: (change: Record<string, unknown>) => void }).__emitMedia({ source: 'cs2.exe', desiredMode: 'game', plannedSettings: ['Game mode: on'], reason: 'Game detected · confirmed by headphones.' }));
  await expect(panel.locator('.mf-readout')).toHaveText('Game');
  await expect(panel).toContainText('Game mode: on');
  const card = (name: string) => panel.locator('.mf-map li').filter({ has: page.locator('strong', { hasText: new RegExp(`^${name}$`) }) });
  await expect(card('Movie')).toContainText('Supported');
  await expect(card('Movie')).toContainText('Listening mode: cinema');
  await expect(card('Standard')).toContainText('Supported');
  await expect(card('Game')).toContainText('Not supported');
  await expect(card('Game')).toContainText('Your headphones stay as they are');
  await expect(card('Music')).toContainText('Not supported');
  await expect(panel).toContainText('fully supports Movie, Standard');
  await expect(panel).toContainText('Game, Music will leave your headphones unchanged');
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

test('Sony graph dragging stages the complete curve, sends one apply and retains a rejected draft', async ({ page }) => {
  await installNativeFixture(page, false, { firmware: '3.1.5', deviceCapabilities: ['equalizer'], deviceSettings: { equalizer: Array(10).fill(6) } });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  const point = page.getByRole('slider', { name: '31 Hz device equalizer step', exact: true });
  const box = await point.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await page.mouse.down();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2 - 38, { steps: 8 });
  await page.mouse.up();
  const edited = Number(await point.getAttribute('aria-valuenow'));
  expect(edited).toBeGreaterThan(6);
  await expect(point).toBeEnabled();
  expect(await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls.filter(command => command === 'apply_setting'))).toHaveLength(0);
  await page.getByRole('button', { name: 'Apply equalizer', exact: true }).click();
  await expect(point).toBeDisabled();
  const requests = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args: Record<string, unknown> }> }).__fixtureRequests.filter(request => request.command === 'apply_setting'));
  expect(requests).toHaveLength(1);
  expect(requests[0].args.value).toEqual([edited, 6, 6, 6, 6, 6, 6, 6, 6, 6]);
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByRole('alert')).toContainText('timed out');
  await expect(point).toBeEnabled();
  await expect(point).toHaveAttribute('aria-valuenow', String(edited));
  await page.getByRole('button', { name: 'Discard changes', exact: true }).click();
  await expect(point).toHaveAttribute('aria-valuenow', '6');
});

test('VLC startup configuration is an explicit action after credentials are saved', async ({ page }) => {
  await installNativeFixture(page, false, { mediaState: { enabled: true, status: 'waiting' } });
  await page.goto('/');
  await page.getByRole('button', { name: 'Automatic listening', exact: true }).click();
  const vlc = page.getByRole('region', { name: 'VLC connection', exact: true });
  const startup = vlc.getByRole('button', { name: 'Enable VLC startup', exact: true });
  await expect(startup).toBeDisabled();
  await vlc.getByLabel('VLC Lua HTTP password', { exact: true }).fill('fixture-private-credential');
  await vlc.getByRole('button', { name: 'Connect and save', exact: true }).click();
  await expect(startup).toBeEnabled();
  expect(await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls)).not.toContain('configure_vlc_startup');
  await startup.click();
  await expect(vlc).toContainText('Ready for VLC restarts');
  const startupRequests = await page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests.filter(request => request.command === 'configure_vlc_startup'));
  expect(startupRequests).toEqual([{ command: 'configure_vlc_startup', args: {} }]);
  expect(await page.evaluate(() => (window as unknown as { __fixtureCalls: string[] }).__fixtureCalls)).not.toContain('apply_setting');
});

test('native controls stay unavailable without negotiated capabilities', async ({ page }) => {
  await installNativeFixture(page, false);
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByRole('button', { name: 'Select WH-1000XM6', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Noise cancelling', exact: true })).toBeDisabled();
  await expect(page.getByText('Noise control is unavailable because this connected device has not confirmed its current mode.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByRole('slider')).toHaveCount(0);
  await expect(page.getByText('Equalizer controls are unavailable because this connected device has not confirmed the equalizer capability and current band steps.', { exact: true })).toBeVisible();
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
  await expect(page.locator('.device-hero .device-kicker')).toHaveText('Disconnected');
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
    deviceSettings: { controlDescriptors: descriptors, soundQuality: 'quality', noise: 'ambient', focusOnVoice: false, ambient: 4 },
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

test('Focus on voice is disabled with an explanation unless Noise control is Ambient sound', async ({ page }) => {
  await installNativeFixture(page, false, {
    deviceCapabilities: ['focusOnVoice'],
    deviceSettings: { controlDescriptors: [{ key: 'focusOnVoice', label: 'Focus on voice', description: 'Emphasize speech in ambient mode.', kind: 'toggle' }], noise: 'cancelling', focusOnVoice: false },
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'Device settings', exact: true }).click();
  await expect(page.getByRole('switch', { name: 'Focus on voice' })).toBeDisabled();
  await expect(page.getByText('Available while Noise control is set to Ambient sound.')).toBeVisible();
});

const HOUR = 3_600_000;
function historyFixture(overrides: Record<string, unknown> = {}) {
  const now = Date.now();
  const days = [0, 1, 2, 3, 4, 5, 6].map(index => ({ startMs: now - (6 - index) * 24 * HOUR, ms: [0, 2, 1, 0, 3, 1, 2][index] * HOUR }));
  const start = now - 3 * HOUR;
  return {
    totalMs: 9 * HOUR, firstMs: now - 6 * 24 * HOUR, days,
    modes: [{ key: 'music', ms: 5 * HOUR }, { key: 'cinema', ms: 3 * HOUR }, { key: 'standard', ms: HOUR }],
    noise: [{ key: 'cancelling', ms: 6 * HOUR }, { key: 'ambient', ms: 3 * HOUR }],
    devices: [{ key: 'WH-1000XM6', ms: 9 * HOUR }], knownDevices: ['OnePlus Buds 3 Pro', 'WH-1000XM6'],
    categories: [{ key: 'music', ms: 5 * HOUR }, { key: 'movie', ms: 3 * HOUR }],
    sources: [{ key: 'Spotify', ms: 5 * HOUR }, { key: 'netflix.com', ms: 3 * HOUR }],
    titles: [], urls: [],
    recent: [{ start, end: start + 90 * 60_000, device: 'WH-1000XM6', mode: 'cinema', noise: 'cancelling', category: 'movie', source: 'netflix.com' }],
    skipped: 0, writeError: null, contentSupported: true, ...overrides,
  };
}
const requestsOf = (page: Page) => page.evaluate(() => (window as unknown as { __fixtureRequests: Array<{ command: string; args?: Record<string, unknown> }> }).__fixtureRequests);

test('Listening history shows totals, breakdowns and recent sessions, and reloads when the range changes', async ({ page }) => {
  await installNativeFixture(page, true, { historySummary: historyFixture({ details: true }) });
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Listening history.' })).toBeVisible();
  await expect(page.locator('.hs-readout')).toHaveAttribute('data-total-ms', String(9 * HOUR));
  await expect(page.locator('.hs-readout')).toHaveText('9h0min');
  await expect(page.getByText('Stays on this computer. Nothing is uploaded.')).toBeVisible();
  await expect(page.getByRole('list', { name: 'Time by listening mode' }).getByRole('listitem')).toHaveCount(3);
  await expect(page.getByRole('list', { name: 'Time by listening mode' }).getByRole('listitem').first()).toContainText('Music');
  await expect(page.getByRole('list', { name: 'Time by noise control' })).toContainText('Noise cancelling');
  // History follows the headphone in use. Other devices appear only after choosing "All devices".
  await expect(page.getByLabel('Show history for')).toHaveValue('WH-1000XM6');
  await expect(page.getByRole('heading', { name: 'By device' })).toHaveCount(0);
  await expect(page.locator('.hs-stage-label')).toContainText('WH-1000XM6');
  expect((await requestsOf(page)).filter(request => request.command === 'get_history').at(-1)?.args).toMatchObject({ device: 'WH-1000XM6' });
  await page.getByLabel('Show history for').selectOption({ label: 'All devices' });
  await expect(page.getByRole('list', { name: 'Time by device' })).toContainText('WH-1000XM6');
  expect((await requestsOf(page)).filter(request => request.command === 'get_history').at(-1)?.args).toMatchObject({ device: null });
  await page.getByLabel('Show history for').selectOption('WH-1000XM6');
  await expect(page.getByRole('list', { name: 'Time by app or site' })).toContainText('Spotify');
  await expect(page.getByRole('group', { name: 'Daily listening time' }).getByRole('listitem')).toHaveCount(7);
  await expect(page.locator('.hs-session').first()).toContainText('Cinema · Noise cancelling');
  await expect(page.locator('.hs-session').first()).toContainText('netflix.com');
  await expect(page.getByRole('button', { name: '7 days' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Today' }).click();
  await expect(page.getByRole('button', { name: 'Today' })).toHaveAttribute('aria-pressed', 'true');
  const historyCalls = (await requestsOf(page)).filter(request => request.command === 'get_history');
  expect(historyCalls.at(-1)?.args).toMatchObject({ range: 'today', tzOffsetMinutes: expect.any(Number) });
});

test('Listening history explains an empty log and unavailable details instead of showing blanks', async ({ page }) => {
  await installNativeFixture(page, true, {});
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  await expect(page.getByText('Nothing recorded yet. History starts when a headphone connects.')).toBeVisible();
  await expect(page.locator('.hs-readout')).toHaveText('0min');
  await expect(page.getByRole('heading', { name: 'By listening mode' })).toHaveCount(0);

  const windowsOnly = await page.context().newPage();
  await installNativeFixture(windowsOnly, true, { historySummary: historyFixture({ contentSupported: false, categories: [], sources: [] }) });
  await windowsOnly.goto('/');
  await windowsOnly.getByRole('button', { name: 'Listening history', exact: true }).click();
  await expect(windowsOnly.getByText('Playback details are recorded on Windows only.')).toBeVisible();
  await windowsOnly.close();
});

test('Listening history keeps details off by default, saves the toggle, and clears only after confirmation', async ({ page }) => {
  await installNativeFixture(page, true, { historySummary: historyFixture() });
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  const basic = page.getByRole('switch', { name: 'Record listening history' });
  const details = page.getByRole('switch', { name: 'Record apps and sites' });
  await expect(basic).toHaveAttribute('aria-checked', 'true');
  await expect(details).toHaveAttribute('aria-checked', 'false');
  await expect(page.getByText('Off. Turn on “Record apps and sites” below to see what you listen to.')).toBeVisible();
  await details.click();
  await expect(details).toHaveAttribute('aria-checked', 'true');
  expect((await requestsOf(page)).filter(request => request.command === 'set_history_settings').at(-1)?.args).toEqual({ enabled: true, details: true, exact: false });
  await basic.click();
  await expect(basic).toHaveAttribute('aria-checked', 'false');
  await expect(details).toBeDisabled();

  await page.getByRole('button', { name: 'Clear history…' }).click();
  await expect(page.getByRole('alert').filter({ hasText: 'cannot be undone' })).toBeVisible();
  await page.getByRole('button', { name: 'Keep it' }).click();
  expect((await requestsOf(page)).some(request => request.command === 'clear_history')).toBe(false);
  await page.getByRole('button', { name: 'Clear history…' }).click();
  await page.getByRole('button', { name: 'Delete history' }).click();
  await expect(page.getByText('Nothing recorded yet. History starts when a headphone connects.')).toBeVisible();
  expect((await requestsOf(page)).some(request => request.command === 'clear_history')).toBe(true);
});

test('Listening history reports read and save failures and fits a 390px screen', async ({ page }) => {
  await installNativeFixture(page, true, { historySummary: historyFixture({ writeError: "History couldn't be saved. Check free disk space and folder permissions.", skipped: 2 }) });
  await page.setViewportSize({ width: 390, height: 800 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  await expect(page.getByText("History couldn't be saved.")).toBeVisible();
  await expect(page.getByText('2 damaged entries were skipped.')).toBeVisible();
  await expect(page.locator('.hs-readout')).toHaveText('9h0min');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  const broken = await page.context().newPage();
  await installNativeFixture(broken, true, { historyError: 'History is unavailable.' });
  await broken.goto('/');
  await broken.getByRole('button', { name: 'Listening history', exact: true }).click();
  await expect(broken.getByRole('alert').filter({ hasText: 'History is unavailable.' })).toBeVisible();
  await broken.close();
});


test('Listening history shows exact titles and URLs only after the third switch, which depends on the second', async ({ page }) => {
  const url = 'https://www.netflix.com/watch/81?trackId=7&q=stranger';
  const now = Date.now();
  await installNativeFixture(page, true, { historySummary: historyFixture({
    details: true, exact: false,
    titles: [{ key: 'Radiohead \u2014 Karma Police', ms: 3 * HOUR }, { key: 'Elden Ring', ms: 2 * HOUR }],
    urls: [{ key: url, ms: 3 * HOUR }],
    recent: [
      { start: now - 2 * HOUR, end: now - HOUR, device: 'WH-1000XM6', mode: 'music', category: 'music', source: 'Spotify', title: 'Karma Police', artist: 'Radiohead' },
      { start: now - 4 * HOUR, end: now - 3 * HOUR, device: 'WH-1000XM6', mode: 'cinema', category: 'movie', source: 'netflix.com', title: 'Stranger Things', url },
    ],
  }) });
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  const exact = page.getByRole('switch', { name: 'Record exact titles and URLs' });
  await expect(exact).toHaveAttribute('aria-checked', 'false');
  await expect(exact).toBeEnabled();
  await expect(page.getByText('Top songs, videos and games')).toHaveCount(0);
  await expect(page.getByRole('list', { name: 'Time by page address' })).toHaveCount(0);

  await exact.click();
  await expect(exact).toHaveAttribute('aria-checked', 'true');
  expect((await requestsOf(page)).filter(request => request.command === 'set_history_settings').at(-1)?.args).toEqual({ enabled: true, details: true, exact: true });
  await expect(page.getByRole('list', { name: 'Time by title' })).toContainText('Radiohead \u2014 Karma Police');
  await expect(page.getByRole('list', { name: 'Time by title' })).toContainText('Elden Ring');
  await expect(page.getByRole('list', { name: 'Time by page address' })).toContainText(url);
  await expect(page.locator('.hs-session').first().locator('strong')).toHaveText('Radiohead \u2014 Karma Police');
  await expect(page.locator('.hs-session').first()).toContainText('Music \u00b7 WH-1000XM6 \u00b7 Music \u00b7 Spotify');
  await expect(page.locator('.hs-session-url')).toHaveText(url);

  await page.getByRole('switch', { name: 'Record apps and sites' }).click();
  await expect(exact).toBeDisabled();
});

test('Listening history offers other devices only on request and never shows them by default', async ({ page }) => {
  await installNativeFixture(page, true, { historySummary: historyFixture() });
  await page.goto('/');
  await page.getByRole('button', { name: 'Listening history', exact: true }).click();
  const scope = page.getByLabel('Show history for');
  await expect(scope.locator('option')).toHaveText(['WH-1000XM6 (in use)', 'OnePlus Buds 3 Pro', 'All devices']);
  await expect(scope).toHaveValue('WH-1000XM6');
  const calls = (await requestsOf(page)).filter(request => request.command === 'get_history');
  expect(calls.every(request => request.args?.device === 'WH-1000XM6')).toBe(true);
  await scope.selectOption('OnePlus Buds 3 Pro');
  await expect(page.locator('.hs-stage-label')).toContainText('OnePlus Buds 3 Pro');
  expect((await requestsOf(page)).filter(request => request.command === 'get_history').at(-1)?.args).toMatchObject({ device: 'OnePlus Buds 3 Pro' });
  await expect(page.getByRole('heading', { name: 'By device' })).toHaveCount(0);
});

const ch720nLayout = {
  inquiry: 0, bandCount: 6, levelSteps: 21, customizable: true,
  labels: ['Clear Bass', '400 Hz', '1 kHz', '2.5 kHz', '6.3 kHz', '16 kHz'], clearBassIndex: 0,
  presets: [{ id: 0, label: 'Off' }, { id: 22, label: 'Bass' }, { id: 160, label: 'Custom' }, { id: 112, label: 'Warm' }],
};
const finishApply = (page: Page) => page.evaluate(() => (window as unknown as { __finishApply: () => void }).__finishApply());

test('A headphone that describes its own equalizer gets its own bands, range and presets', async ({ page }) => {
  await installNativeFixture(page, true, {
    model: 'WH-CH720N', firmware: '1.0.0', deviceCapabilities: ['noise', 'equalizer'],
    deviceSettings: { equalizer: [10, 10, 10, 10, 10, 10], equalizerPresetCode: 0, eqLayout: ch720nLayout },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByText('CONFIRMED ON DEVICE')).toBeVisible();
  await expect(page.getByText('6 independent bands, as reported by your headphone.')).toBeVisible();
  const sliders = page.getByRole('slider');
  await expect(sliders).toHaveCount(6);
  for (const label of ch720nLayout.labels) {
    await expect(page.getByRole('slider', { name: `${label} device equalizer step`, exact: true })).toHaveAttribute('aria-valuemax', '20');
  }
  const bass = page.getByRole('slider', { name: 'Clear Bass device equalizer step', exact: true });
  await expect(bass).toHaveAttribute('aria-valuetext', '0 dB');
  await expect(page.getByText('±10 dB').first()).toBeVisible();
  // No XM6 factory curves are applied to a headphone that did not offer them.
  await expect(page.getByRole('group', { name: 'Sony equalizer presets' })).toHaveCount(0);
  const selector = page.getByLabel('Device equalizer profile', { exact: true });
  await expect(selector.locator('optgroup[label="Headphone presets"] option')).toHaveText(['Off', 'Bass', 'Warm']);
  await expect(selector).toHaveValue('dev:0');

  // Editing a band sends the six device steps; nothing is shown as changed until confirmed.
  await bass.focus();
  await bass.press('ArrowRight');
  await expect(bass).toHaveAttribute('aria-valuenow', '11');
  await page.getByRole('button', { name: 'Apply equalizer', exact: true }).click();
  await expect(bass).toBeDisabled();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'equalizer', value: [11, 10, 10, 10, 10, 10] });
  await finishApply(page);
  await expect(bass).toHaveAttribute('aria-valuenow', '11');
  await expect(bass).toHaveAttribute('aria-valuetext', '+1 dB');

  // Choosing a headphone preset sends only the preset id.
  await page.getByRole('button', { name: 'Bass', exact: true }).click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'sonyEqPreset', value: 22 });
});

test('A headphone with preset-only equalizer offers presets and no band editing', async ({ page }) => {
  await installNativeFixture(page, true, {
    model: 'WH-CH510', firmware: '1.0.0', deviceCapabilities: ['equalizer'],
    deviceSettings: { equalizer: [], equalizerPresetCode: 22, eqLayout: { ...ch720nLayout, inquiry: 2, customizable: false, bandCount: 0, labels: [], clearBassIndex: null, presets: [{ id: 0, label: 'Off' }, { id: 22, label: 'Bass' }] } },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByText('This headphone offers equalizer presets.')).toBeVisible();
  await expect(page.getByRole('slider')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Save preset' })).toBeDisabled();
  await expect(page.getByLabel('Device equalizer profile', { exact: true })).toHaveValue('dev:22');
  await expect(page.getByRole('button', { name: 'Bass', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Off', exact: true }).click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'sonyEqPreset', value: 0 });
});

test('A headphone that described no equalizer says so and keeps the controls disabled', async ({ page }) => {
  await installNativeFixture(page, true, { model: 'WH-CH720N', firmware: '1.0.0', deviceCapabilities: ['noise'] });
  await page.goto('/');
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(page.getByText('did not describe an equalizer that Sound Connect Desktop can control')).toBeVisible();
  await expect(page.getByText('has not confirmed the equalizer capability and current band steps')).toBeVisible();
});


const mpDevices = [
  { address: 'AA:BB:CC:00:11:22', name: 'Pixel 9', classOfDevice: 0x5a020c },
  { address: 'AA:BB:CC:00:11:33', name: 'Work laptop', classOfDevice: 0x02010c },
  { address: 'AA:BB:CC:00:11:44', name: 'iPad', classOfDevice: 0x5a020c },
];
const mpState = (connected: string[], playing = '', extra: { maxConnections?: number; thisComputer?: string } = {}) => JSON.stringify({
  devices: mpDevices.map(device => ({ ...device, connected: connected.includes(device.name), status: connected.includes(device.name) ? 1 : 0, playback: device.name === playing, thisComputer: device.name === extra.thisComputer })),
  ...(extra.maxConnections ? { maxConnections: extra.maxConnections } : {}),
});
const multipointFixture = (connected: string[], extra: Record<string, unknown> = {}) => ({
  model: 'WH-1000XM5', firmware: '1.0.0', deviceCapabilities: ['multipointDevices'],
  deviceSettings: { multipointDevices: mpState(connected, connected[0]) }, ...extra,
});

test('Multipoint lists every paired device with its state and lets you pick which are connected', async ({ page }) => {
  await installNativeFixture(page, false, multipointFixture(['Pixel 9'], { applySettings: { multipointDevices: mpState(['Pixel 9', 'Work laptop'], 'Pixel 9') } }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Multipoint connections' });
  await expect(panel.getByRole('listitem')).toHaveCount(3);
  await expect(panel.getByRole('status')).toHaveText('1 of 3 connected');
  await expect(panel.getByText('Connected · playing audio')).toBeVisible();
  await expect(panel.getByText('AA:BB:CC:00:11:33')).toBeVisible();
  const laptop = panel.getByRole('switch', { name: 'Connection to Work laptop' });
  await expect(panel.getByRole('switch', { name: 'Connection to Pixel 9' })).toHaveAttribute('aria-checked', 'true');
  await expect(laptop).toHaveAttribute('aria-checked', 'false');
  // The generic "not validated" row is replaced by the real control.
  await expect(page.getByText('The desktop has not validated this device’s multipoint controls.')).toHaveCount(0);
  await expect(panel.getByRole('heading', { name: 'Paired devices' })).toBeVisible();

  await laptop.click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'multipointDevice', value: { address: 'AA:BB:CC:00:11:33', connect: true } });
  await expect(panel.getByText('Waiting for the headphone…')).toBeVisible();
  await expect(laptop).toBeDisabled();
  await expect(laptop).toHaveAttribute('aria-checked', 'false');
  await finishApply(page);
  await expect(laptop).toHaveAttribute('aria-checked', 'true');
  await expect(panel.getByRole('status')).toHaveText('2 of 3 connected');
});

test('Multipoint disconnects a device, refreshes on request and keeps the old state when the headphone does not confirm', async ({ page }) => {
  await installNativeFixture(page, false, multipointFixture(['Pixel 9', 'iPad'], { applySettingError: 'The headphone did not report that device as disconnected within 20 seconds.' }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Multipoint connections' });
  const ipad = panel.getByRole('switch', { name: 'Connection to iPad' });
  await expect(ipad).toHaveAttribute('aria-checked', 'true');
  await ipad.click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'multipointDevice', value: { address: 'AA:BB:CC:00:11:44', connect: false } });
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByRole('alert').filter({ hasText: 'The headphone did not report that device as disconnected within 20 seconds.' }).first()).toBeVisible();
  await expect(ipad).toHaveAttribute('aria-checked', 'true');
  await expect(panel.getByRole('status')).toHaveText('2 of 3 connected');

  await panel.getByRole('button', { name: 'Refresh' }).click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'multipointRefresh' });
});

test('Multipoint is not shown for a headphone that did not advertise paired-device management', async ({ page }) => {
  await installNativeFixture(page, false, { model: 'WH-1000XM5', deviceCapabilities: [] });
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Multipoint connections' })).toHaveCount(0);
  await expect(page.getByText('did not report a paired-device list')).toBeVisible();
});

test('Multipoint rows stay readable at 390px and a malformed list is ignored', async ({ page }) => {
  await installNativeFixture(page, false, multipointFixture(['Pixel 9']));
  await page.setViewportSize({ width: 390, height: 900 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Multipoint connections' });
  await expect(panel.getByRole('listitem')).toHaveCount(3);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  const broken = await page.context().newPage();
  await installNativeFixture(broken, false, { model: 'WH-1000XM5', deviceCapabilities: ['multipointDevices'], deviceSettings: { multipointDevices: '{"devices":[{"address":"nope"}]}' } });
  await broken.goto('/');
  await broken.getByRole('button', { name: 'Multipoint', exact: true }).click();
  await expect(broken.getByRole('region', { name: 'Multipoint connections' })).toHaveCount(0);
  await broken.close();
});


test('Multipoint is its own tab, not part of Device settings', async ({ page }) => {
  await installNativeFixture(page, false, multipointFixture(['Pixel 9']));
  await page.goto('/');
  await page.getByRole('button', { name: 'Device settings' }).click();
  await expect(page.getByRole('region', { name: 'Multipoint connections' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Multipoint.' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Multipoint connections' })).toBeVisible();
});

test('Multipoint says what connecting will do when the headphone is full, and never offers to drop the active device or this computer', async ({ page }) => {
  // Two connections allowed: the phone and this laptop. The laptop (this computer) plays audio.
  await installNativeFixture(page, false, multipointFixture([], {
    deviceSettings: { multipointDevices: mpState(['Pixel 9', 'Work laptop'], 'Work laptop', { maxConnections: 2, thisComputer: 'Work laptop' }) },
    applySettings: { multipointDevices: mpState(['Work laptop', 'iPad'], 'Work laptop', { maxConnections: 2, thisComputer: 'Work laptop' }) },
  }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Multipoint connections' });
  await expect(panel.getByText('2 of 3 connected · up to 2 at once')).toBeVisible();
  await expect(panel.getByText('This computer', { exact: true })).toBeVisible();
  await expect(panel.getByText('Connecting will disconnect Pixel 9 (not playing audio).')).toBeVisible();
  const ipad = panel.getByRole('switch', { name: 'Connection to iPad' });
  await ipad.click();
  expect((await requestsOf(page)).filter(request => request.command === 'apply_setting').at(-1)?.args)
    .toMatchObject({ setting: 'multipointDevice', value: { address: 'AA:BB:CC:00:11:44', connect: true } });
  await expect(panel.getByText('Disconnecting Pixel 9, then connecting…')).toBeVisible();
  await finishApply(page);
  await expect(ipad).toHaveAttribute('aria-checked', 'true');
  await expect(panel.getByRole('switch', { name: 'Connection to Pixel 9' })).toHaveAttribute('aria-checked', 'false');
  await expect(panel.getByRole('switch', { name: 'Connection to Work laptop' })).toHaveAttribute('aria-checked', 'true');

  // Phone plays audio and the laptop is this computer: nothing can be dropped, so connecting is not offered.
  const full = await page.context().newPage();
  await installNativeFixture(full, false, multipointFixture([], {
    deviceSettings: { multipointDevices: mpState(['Pixel 9', 'Work laptop'], 'Pixel 9', { maxConnections: 2, thisComputer: 'Work laptop' }) },
  }));
  await full.goto('/');
  await full.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const fullPanel = full.getByRole('region', { name: 'Multipoint connections' });
  await expect(fullPanel.getByText('Full: every connected device is playing audio or is this computer.')).toBeVisible();
  await expect(fullPanel.getByRole('switch', { name: 'Connection to iPad' })).toBeDisabled();
  await full.close();
});

test('A control-link reset during a multipoint change is shown as a reconnecting notice, not as a failed connection', async ({ page }) => {
  const message = 'The headphone briefly reset its control link while connecting iPad. It does that when its connections change, so the change most likely went through. Sound Connect is reconnecting, and the list updates once it has.';
  await installNativeFixture(page, false, multipointFixture(['Pixel 9'], { applySettingError: message }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Multipoint', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Multipoint connections' });
  await panel.getByRole('switch', { name: 'Connection to iPad' }).click();
  await page.evaluate(() => (window as unknown as { __rejectApply: () => void }).__rejectApply());
  await expect(page.getByRole('status').filter({ hasText: 'briefly reset its control link' })).toBeVisible();
  await expect(page.getByRole('alert').filter({ hasText: 'briefly reset its control link' })).toHaveCount(0);
  await expect(page.getByText('did not connect')).toHaveCount(0);
  await expect(panel.getByRole('switch', { name: 'Connection to iPad' })).toBeEnabled();
});

test('A new version found in the background is announced with a one-click update', async ({ page }) => {
  await installNativeFixture(page, true, { updateAvailable: true, updateCheckDelayMs: 50 });
  await page.goto('/');
  const banner = page.getByRole('status', { name: 'App update' });
  await expect(banner.getByText('New update available')).toBeVisible();
  await expect(banner).toContainText('Sound Connect 0.9.9 is ready. You are on 0.1.6.');
  await expect(banner).toContainText('Faster connections, a Multipoint tab and a new equalizer.');
  // It is shown on every page, not only in App settings.
  await page.getByRole('button', { name: 'Equalizer', exact: true }).click();
  await expect(banner).toBeVisible();
  await banner.getByRole('button', { name: 'Update now' }).click();
  expect((await requestsOf(page)).some(request => request.command === 'install_app_update')).toBe(true);
  await expect(banner).toContainText('Installing the update… Sound Connect will restart shortly.');
  await expect(banner.getByRole('button', { name: 'Update now' })).toHaveCount(0);
});

test('The update banner can be put off, and is absent when the app is up to date', async ({ page }) => {
  await installNativeFixture(page, true, { updateAvailable: true, updateCheckDelayMs: 50 });
  await page.goto('/');
  const banner = page.getByRole('status', { name: 'App update' });
  await expect(banner).toBeVisible();
  await banner.getByRole('button', { name: 'Remind me later' }).click();
  await expect(banner).toHaveCount(0);

  const current = await page.context().newPage();
  await installNativeFixture(current, true, { updateAvailable: false, updateCheckDelayMs: 50 });
  await current.goto('/');
  await expect.poll(async () => (await requestsOf(current)).some(request => request.command === 'check_app_update')).toBe(true);
  await expect(current.getByRole('status', { name: 'App update' })).toHaveCount(0);
  await current.close();
});

test('Automatic update checks can be switched off in App settings and stay off', async ({ page }) => {
  await installNativeFixture(page, true, { updateAvailable: true, updateCheckDelayMs: 50 });
  await page.addInitScript(() => localStorage.setItem('sound-connect:auto-update-check', 'false'));
  await page.goto('/');
  await page.waitForTimeout(600);
  expect((await requestsOf(page)).some(request => request.command === 'check_app_update')).toBe(false);
  await expect(page.getByRole('status', { name: 'App update' })).toHaveCount(0);
  await page.getByRole('button', { name: 'App settings' }).click();
  const toggle = page.getByLabel('Check for updates automatically');
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect(page.getByRole('status', { name: 'App update' })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('sound-connect:auto-update-check'))).toBe('true');
});
