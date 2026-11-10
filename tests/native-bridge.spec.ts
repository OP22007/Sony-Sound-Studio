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
} = {}) {
  await page.addInitScript(({ noiseSupported, options }) => {
    const calls: string[] = [];
    const requests: Array<{ command: string; args?: Record<string, unknown> }> = [];
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
          requests.push({ command, args: args as Record<string, unknown> | undefined });
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
