import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { animate, createScope, stagger } from 'animejs';
import { Activity, History as HistoryIcon, ArrowDownToLine, ArrowRight, AudioLines, Bluetooth, Check, ChevronRight, CircleHelp, CircleOff, Cpu, Headphones as HeadphoneIcon, Info, Layers3, LoaderCircle, Moon, Plus, Radio, RefreshCw, Settings2, ShieldCheck, SlidersHorizontal, SlidersVertical, Sparkles, Sun, Volume2, Waves, X } from 'lucide-react';
import { browserState, defaultAppPreferences, isAppPreferences, nativeAvailable, nativeCommand, subscribeState } from './bridge';
import type { AppPreferences, AppState, AppTheme, Device, DeviceControlDescriptor, DeviceControlOption, SettingPrimitive } from './bridge';
import { defaultSettings, previewDevices, validPreview } from './preview';
import type { PreviewSettings } from './preview';
import { loadLocal, saveLocal } from './storage';
import Headphones from './Headphones';
import SoundVisualization from './SoundVisualization';
import DeviceSummary from './DeviceSummary';
import ListeningModes from './ListeningModes';
import OnePlusListeningModes from './OnePlusListeningModes';
import Equalizer from './Equalizer';
import BassControl from './BassControl';
import type { SavedPreset, DevicePreset } from './eq-presets';
import MediaFollowing from './MediaFollowing';
import HistoryPage from './HistoryPage';
import AppUpdates from './AppUpdates';
import './sound-studio.css';
import './studio-layout.css';
import { batteryReadout, selectNativeSnapshot } from './native-state';
import { isOnePlusDevice } from './oneplus';
import { isOnePlusEqRequest } from './oneplus-eq';
import type { OnePlusEqRequest } from './oneplus-eq';

type Page = 'sound' | 'equalizer' | 'automatic' | 'history' | 'device' | 'coverage' | 'settings';
type PendingDeviceSetting = { deviceId: string; key: string; value: SettingPrimitive | number[] | OnePlusEqRequest };
const isEqualizerBands = (value: unknown): value is number[] => Array.isArray(value) && value.length === 10 && value.every(band => typeof band === 'number' && Number.isInteger(band) && band >= 0 && band <= 12);
type PreferenceKey = keyof AppPreferences;
const navigation: { id: Page; label: string; icon: typeof AudioLines }[] = [
  { id: 'sound', label: 'Sound studio', icon: AudioLines },
  { id: 'equalizer', label: 'Equalizer', icon: SlidersVertical },
  { id: 'automatic', label: 'Automatic listening', icon: Radio },
  { id: 'history', label: 'Listening history', icon: HistoryIcon },
  { id: 'device', label: 'Device settings', icon: SlidersHorizontal },
  { id: 'coverage', label: 'Feature coverage', icon: Layers3 },
  { id: 'settings', label: 'App settings', icon: Settings2 },
];
const themeValid = (v: unknown): v is AppTheme => v === 'dark' || v === 'light' || v === 'system';
const boolValid = (v: unknown): v is boolean => typeof v === 'boolean';
const appPreferencesValid = isAppPreferences;
function loadBrowserPreferences(): AppPreferences {
  const saved = loadLocal('app-preferences', defaultAppPreferences, appPreferencesValid);
  if (saved !== defaultAppPreferences) return saved;
  return {
    ...defaultAppPreferences,
    theme: loadLocal('theme', defaultAppPreferences.theme, themeValid),
    reduceMotion: loadLocal('reduced-motion', defaultAppPreferences.reduceMotion, boolValid),
  };
}
const isSettingPrimitive = (value: unknown): value is SettingPrimitive => typeof value === 'string' || typeof value === 'boolean' || (typeof value === 'number' && Number.isFinite(value));
function isDeviceControlDescriptor(value: unknown, device: Device): value is DeviceControlDescriptor {
  if (!value || typeof value !== 'object') return false;
  const descriptor = value as Partial<DeviceControlDescriptor>;
  if (typeof descriptor.key !== 'string' || !descriptor.key || descriptor.key.length > 80
    || typeof descriptor.label !== 'string' || typeof descriptor.description !== 'string'
    || !device.capabilities.includes(descriptor.key) || !Object.hasOwn(device.settings, descriptor.key)
    || (descriptor.key === 'noise' && device.settings.noiseConfirmation === 'unconfirmed')) return false;
  const current = device.settings[descriptor.key];
  if (descriptor.kind === 'toggle') return typeof current === 'boolean';
  if (descriptor.kind === 'select') {
    return isSettingPrimitive(current) && Array.isArray(descriptor.options)
      && descriptor.options.some(option => option && isSettingPrimitive(option.value) && option.value === current && typeof option.label === 'string');
  }
  if (descriptor.kind === 'range') {
    return typeof current === 'number' && Number.isFinite(current)
      && typeof descriptor.min === 'number' && Number.isFinite(descriptor.min)
      && typeof descriptor.max === 'number' && Number.isFinite(descriptor.max)
      && typeof descriptor.step === 'number' && Number.isFinite(descriptor.step) && descriptor.step > 0
      && descriptor.min <= current && current <= descriptor.max && descriptor.min <= descriptor.max;
  }
  return false;
}
const noiseModeLabel = (mode: unknown) => mode === 'cancelling' ? 'Noise cancelling' : mode === 'ambient' ? 'Ambient sound' : mode === 'off' ? 'Off' : 'Unknown';
const presetValid = (v: unknown): v is SavedPreset[] => Array.isArray(v) && v.length <= 24 && v.every(p => p && typeof p.name === 'string' && p.name.length <= 40 && Array.isArray(p.bands) && p.bands.length === 10 && p.bands.every((n: unknown) => typeof n === 'number' && n >= -10 && n <= 10));
const devicePresetValid = (v: unknown): v is DevicePreset[] => Array.isArray(v) && v.length <= 24 && v.every(p => p && typeof p.name === 'string' && p.name.trim().length > 0 && p.name.length <= 40 && p.model === 'WH-1000XM6' && p.firmware === '3.1.5' && Array.isArray(p.bands) && p.bands.length === 10 && p.bands.every((n: unknown) => typeof n === 'number' && Number.isInteger(n) && n >= 0 && n <= 12));

function useOperatingSystemPreferences() {
  const [systemReduced, setSystemReduced] = useState(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
  const [systemDark, setSystemDark] = useState(() => matchMedia('(prefers-color-scheme: dark)').matches);
  useEffect(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)');
    const color = matchMedia('(prefers-color-scheme: dark)');
    const onMotion = () => setSystemReduced(motion.matches);
    const onColor = () => setSystemDark(color.matches);
    motion.addEventListener('change', onMotion);
    color.addEventListener('change', onColor);
    return () => { motion.removeEventListener('change', onMotion); color.removeEventListener('change', onColor); };
  }, []);
  return { systemReduced, systemDark };
}

export default function App() {
  const [state, setState] = useState<AppState>(browserState);
  const latestNativeSnapshot = useRef<AppState>(browserState);
  const applyNativeSnapshot = useCallback((incoming: AppState) => {
    const accepted = selectNativeSnapshot(latestNativeSnapshot.current, incoming);
    if (accepted !== incoming) return false;
    latestNativeSnapshot.current = accepted;
    setState(accepted);
    return true;
  }, []);
  const [preview, setPreview] = useState(false);
  const [selected, setSelected] = useState('');
  const lastActiveOutput = useRef<string | null>(null);
  const [page, setPage] = useState<Page>('sound');
  const [preferences, setPreferences] = useState<AppPreferences>(() => nativeAvailable ? { ...defaultAppPreferences } : loadBrowserPreferences());
  const [preferencesLoaded, setPreferencesLoaded] = useState(!nativeAvailable);
  const [preferencesLoading, setPreferencesLoading] = useState(nativeAvailable);
  const [preferencesError, setPreferencesError] = useState<string | null>(null);
  const [pendingPreference, setPendingPreference] = useState<PreferenceKey | null>(null);
  const [pendingDeviceSetting, setPendingDeviceSetting] = useState<PendingDeviceSetting | null>(null);
  const [deviceSettingError, setDeviceSettingError] = useState<{ deviceId: string; message: string } | null>(null);
  const [previewState, setPreviewState] = useState<Record<string, PreviewSettings>>(() => loadLocal('preview-settings', {}, validPreview));
  const [presets, setPresets] = useState<SavedPreset[]>(() => loadLocal('presets', [], presetValid));
  const [devicePresets, setDevicePresets] = useState<DevicePreset[]>(() => loadLocal('device-presets', [], devicePresetValid));
  const [busy, setBusy] = useState<string | null>(nativeAvailable ? 'Loading desktop state' : null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  const noiseNoticeDeviceId = useRef<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<string[]>([]);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [exportReady, setExportReady] = useState(false);
  const [appVersion, setAppVersion] = useState('—');
  const updateAppVersion = useCallback((version: string) => setAppVersion(version), []);
  const root = useRef<HTMLDivElement>(null);
  const osPreferences = useOperatingSystemPreferences();
  const effectiveTheme = preferences.theme === 'system' ? osPreferences.systemDark ? 'dark' : 'light' : preferences.theme;
  const reducedMotion = preferences.reduceMotion || osPreferences.systemReduced;
  const devices = preview ? previewDevices : state.devices;
  const device = devices.find(d => d.id === selected)
    ?? (!preview ? devices.find(d => d.id === state.activeOutputDeviceId) ?? devices.find(d => d.connection === 'connected') : undefined)
    ?? devices[0];
  useEffect(() => {
    if (preview) { lastActiveOutput.current = null; return; }
    const active = state.activeOutputDeviceId ?? null;
    if (active !== lastActiveOutput.current) {
      lastActiveOutput.current = active;
      if (active && state.devices.some(item => item.id === active)) {
        setSelected(active);
        noiseNoticeDeviceId.current = null;
        setNotice('');
      }
    }
  }, [preview, state.activeOutputDeviceId, state.devices]);
  const current = preview && device ? previewState[device.id] ?? defaultSettings : defaultSettings;
  const dseeDescriptor = !preview && device && Array.isArray(device.settings.controlDescriptors)
    ? device.settings.controlDescriptors
      .filter((value): value is DeviceControlDescriptor => isDeviceControlDescriptor(value, device))
      .find(descriptor => descriptor.key === 'dsee' && descriptor.kind === 'select')
    : undefined;
  const dseePending = !!device && pendingDeviceSetting?.deviceId === device.id && pendingDeviceSetting.key === 'dsee';
  const dseeError = !!device && deviceSettingError?.deviceId === device.id ? deviceSettingError.message : null;
  const heroNoiseMode = preview
    ? current.noise
    : device?.settings.noiseConfirmation === 'confirmed' && ['cancelling', 'ambient', 'off'].includes(String(device.settings.noise))
      ? String(device.settings.noise)
      : 'unknown';

  const studioDescriptors = !preview && device && Array.isArray(device.settings.controlDescriptors)
    ? device.settings.controlDescriptors.filter((value): value is DeviceControlDescriptor => isDeviceControlDescriptor(value, device))
    : [];
  const onePlus = isOnePlusDevice(device, preview);
  const listeningDescriptor = studioDescriptors.find(descriptor => descriptor.key === 'listeningMode');
  const roomDescriptor = studioDescriptors.find(descriptor => descriptor.key === 'bgmRoomSize');
  const spatialDescriptor = onePlus ? studioDescriptors.find(descriptor => descriptor.key === 'spatialAudio' && descriptor.kind === 'select'
    && typeof device?.settings.spatialAudio === 'number' && [0, 1, 2].includes(device.settings.spatialAudio)
    && descriptor.options?.some(option => typeof option.value === 'number' && option.value === device.settings.spatialAudio && [0, 1, 2].includes(option.value))) : undefined;
  const gameDescriptor = onePlus ? studioDescriptors.find(descriptor => descriptor.key === 'gameMode' && descriptor.kind === 'toggle') : undefined;
  const gameSoundDescriptor = onePlus ? studioDescriptors.find(descriptor => descriptor.key === 'gameSoundProfile' && descriptor.kind === 'select' && descriptor.options?.some(option => typeof option.value === 'number' && Number.isFinite(option.value))) : undefined;
  const spatialValue = spatialDescriptor && typeof device?.settings.spatialAudio === 'number' && spatialDescriptor.options?.some(option => option.value === device.settings.spatialAudio && typeof option.value === 'number' && [0, 1, 2].includes(option.value)) ? device.settings.spatialAudio : undefined;
  const gameValue = gameDescriptor && typeof device?.settings.gameMode === 'boolean' ? device.settings.gameMode : undefined;
  const gameSoundValue = gameSoundDescriptor && typeof device?.settings.gameSoundProfile === 'number' && gameSoundDescriptor.options?.some(option => option.value === device.settings.gameSoundProfile) ? device.settings.gameSoundProfile : undefined;
  const upmixActive = !preview && !onePlus && ['music', 'cinema', 'game'].includes(String(device?.settings.listeningMode));
  const listeningMode = preview ? current.listeningMode ?? 'standard'
    : onePlus ? gameValue === true && (spatialValue === undefined || spatialValue === 0) ? 'game'
      : spatialValue === 1 ? 'spatialAudioFixed' : spatialValue === 2 ? 'spatialAudioHeadTracked' : spatialValue === 0 ? 'standard' : 'unknown'
    : typeof device?.settings.listeningMode === 'string' ? device.settings.listeningMode : 'unknown';
  const listeningLabel = onePlus ? listeningMode === 'spatialAudioFixed' ? 'Spatial audio'
    : listeningMode === 'spatialAudioHeadTracked' ? 'Head-tracked spatial audio'
      : listeningMode === 'standard' ? 'Standard' : listeningMode === 'game' ? 'Game mode' : 'Not confirmed'
    : ({ standard: 'Standard', cinema: 'Cinema', backgroundMusic: 'Background Music', music: 'Music', game: 'Game' } as Record<string, string>)[listeningMode] ?? 'Not confirmed';
  const visualizationBands = preview ? current.equalizer : isEqualizerBands(device?.settings.equalizer) ? device.settings.equalizer : [];
  const lastStudioBands = useRef<Record<string, number[]>>({});
  const visualKey = `${preview}:${device?.id ?? ''}`;
  const visualBandSignature = visualizationBands.join(',');
  useEffect(() => {
    if (page === 'sound' && device) lastStudioBands.current[visualKey] = [...visualizationBands];
  }, [page, visualKey, visualBandSignature]);

  const loadAppPreferences = useCallback(async () => {
    if (!nativeAvailable) return;
    setPreferencesLoading(true);
    setPreferencesError(null);
    try {
      const saved = await nativeCommand<AppPreferences>('get_app_preferences');
      if (!isAppPreferences(saved)) throw new Error('The desktop returned invalid saved preferences.');
      setPreferences(saved);
      setPreferencesLoaded(true);
    } catch (e) {
      setPreferencesError(e instanceof Error ? e.message : String(e));
    } finally {
      setPreferencesLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!device || preview || !notice.includes('requested. Device state could not be verified.')) return;
    if (noiseNoticeDeviceId.current !== device.id || device.settings.noiseConfirmation !== 'confirmed') return;
    noiseNoticeDeviceId.current = null;
    setNotice(`${noiseModeLabel(device.settings.noise)} confirmed by your headphones.`);
  }, [device, notice, preview]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    if (nativeAvailable) {
      nativeCommand<AppState>('get_app_state').then(result => { if (!disposed) applyNativeSnapshot(result); })
        .catch(e => { if (!disposed) setError(String(e)); }).finally(() => { if (!disposed) setBusy(null); });
      void loadAppPreferences();
      subscribeState(result => { if (!disposed) applyNativeSnapshot(result); }).then(fn => { if (disposed) fn(); else unsubscribe = fn; }).catch(e => { if (!disposed) setError(`State updates unavailable: ${String(e)}`); });
    }
    return () => { disposed = true; unsubscribe?.(); };
  }, [applyNativeSnapshot, loadAppPreferences]);

  useEffect(() => { document.documentElement.dataset.theme = effectiveTheme; }, [effectiveTheme]);
  useEffect(() => { saveLocal('preview-settings', previewState); }, [previewState]);
  useEffect(() => {
    document.documentElement.dataset.reducedMotion = String(reducedMotion);
    if (reducedMotion || document.hidden) return;
    const scope = createScope({ root }).add(() => {
      animate('.reveal', { opacity: [0, 1], translateY: [14, 0], delay: stagger(65), duration: 560, ease: 'outQuint' });
    });
    return () => scope.revert();
  }, [page, preview, selected, reducedMotion]);

  async function runNative(command: string, args?: Record<string, unknown>) {
    if (busy) return;
    setBusy(command); setError(null); setNotice('');
    try {
      const result = await nativeCommand<AppState>(command, args);
      const accepted = applyNativeSnapshot(result);
      if (accepted && command === 'apply_setting' && typeof args?.setting === 'string') setDeviceSettingError(null);
      if (accepted && command === 'discover_devices') setNotice(result.devices.length ? `Found ${result.devices.length} paired ${result.devices.length === 1 ? 'device' : 'devices'}. Select a headphone to connect.` : 'No compatible headphones found. Pair your headphones in your computer’s Bluetooth settings, then try again.');
      if (accepted && command === 'apply_setting' && args?.setting === 'noise') {
        const updated = result.devices.find(item => item.id === args.deviceId);
        if (updated?.settings.noiseConfirmation === 'unconfirmed') {
          noiseNoticeDeviceId.current = updated.id;
          setNotice(`${noiseModeLabel(args.value)} requested. Device state could not be verified.`);
        } else if (updated?.settings.noiseConfirmation === 'confirmed') {
          noiseNoticeDeviceId.current = null;
          setNotice(`${noiseModeLabel(updated.settings.noise)} confirmed by your headphones.`);
        }
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      if (command === 'apply_setting' && typeof args?.deviceId === 'string') setDeviceSettingError({ deviceId: args.deviceId, message });
    }
    finally { setBusy(null); }
  }

  async function updatePreference<K extends PreferenceKey>(key: K, value: AppPreferences[K]) {
    if (preferencesLoading || pendingPreference) return;
    const next = { ...preferences, [key]: value };
    setPreferencesError(null);
    if (!nativeAvailable) {
      if (!saveLocal('app-preferences', next)) {
        setPreferencesError('This browser could not save the preference on this computer.');
        return;
      }
      setPreferences(next);
      setPreferencesLoaded(true);
      return;
    }
    if (!preferencesLoaded) return;
    setPendingPreference(key);
    try {
      const saved = await nativeCommand<AppPreferences>('update_app_preferences', { preferences: next });
      if (!isAppPreferences(saved)) throw new Error('The desktop returned invalid saved preferences.');
      setPreferences(saved);
    } catch (e) {
      setPreferencesError(e instanceof Error ? e.message : String(e));
    } finally {
      setPendingPreference(null);
    }
  }

  function enterPreview() { setPreview(true); setSelected(previewDevices[0].id); setPage('sound'); setError(null); setNotice(''); }
  function exitPreview() { setPreview(false); setSelected(state.devices[0]?.id ?? ''); setPage('sound'); setNotice(''); }
  function updatePreview(patch: Partial<PreviewSettings>) {
    if (!preview || !device) return;
    setPreviewState(previous => ({ ...previous, [device.id]: { ...(previous[device.id] ?? defaultSettings), ...patch } }));
  }
  const can = (feature: string) => preview || !!(device?.connection === 'connected' && device.capabilities.includes(feature) && Object.hasOwn(device.settings, feature));
  function setting(key: string, value: unknown) {
    if (!device) return;
    if (preview) updatePreview({ [key]: value });
    else if (!busy && can(key) && (isSettingPrimitive(value) || (key === 'equalizer' && isEqualizerBands(value)) || (key === 'oneplusEqProfile' && isOnePlusDevice(device) && isOnePlusEqRequest(value)))) {
      const deviceId = device.id;
      setDeviceSettingError(null);
      setPendingDeviceSetting({ deviceId, key, value });
      void runNative('apply_setting', { deviceId, setting: key, value }).finally(() => {
        setPendingDeviceSetting(current => current?.deviceId === deviceId && current.key === key ? null : current);
      });
    }
  }
  async function loadDiagnostics() {
    setShowDiagnostics(true); setExportReady(false);
    setDiagnosticsLoading(true);
    try {
      if (!nativeAvailable) { setDiagnostics(['Browser session: native Bluetooth is unavailable.', 'No device addresses, audio or listening history collected.', 'Use the desktop build for transport diagnostics.']); return; }
      setDiagnostics(await nativeCommand<string[]>('get_diagnostics'));
    } catch (e) { setError(String(e)); }
    finally { setDiagnosticsLoading(false); }
  }
  async function probeNoiseSchema() {
    if (!device || preview || device.connection !== 'connected' || busy) return;
    await runNative('apply_setting', { deviceId: device.id, setting: 'probe-noise-schema', value: null });
    await loadDiagnostics();
  }
  function exportDiagnostics() {
    const blob = new Blob([['Sound Connect — diagnostic export', `Platform: ${state.platform}`, ...diagnostics].join('\n')], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a'); link.href = url; link.download = 'sound-connect-diagnostics.txt'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000); setExportReady(false);
  }

  return <div ref={root} className={`app ${reducedMotion ? 'reduce-motion' : ''}`}>
    <aside className="sidebar" aria-label="Studio controls">
      <a className="brand" href="#studio" onClick={e => { e.preventDefault(); setPage('sound'); }} aria-label="Sound Connect home"><span className="brand-mark"><AudioLines size={23} strokeWidth={1.8}/></span><span>sound<span className="brand-light">connect</span><small>DESKTOP STUDIO</small></span></a>
      <div className="sidebar-caption">WORKSPACE</div>
      <nav aria-label="Main navigation">{navigation.map(({ id, label, icon: Icon }) => <button key={id} className={`nav-item ${page === id ? 'active' : ''}`} onClick={() => setPage(id)} aria-current={page === id ? 'page' : undefined}><Icon size={18}/><span>{label}</span>{page === id && <span className="nav-dot"/>}</button>)}</nav>
      <div className="device-rail">
        <div className="rail-heading"><span>{preview ? 'PREVIEW DEVICES' : 'YOUR DEVICES'}</span><button className="icon-button" aria-label="Find headphones" title="Find headphones" disabled={!!busy || preview} onClick={() => void runNative('discover_devices')}><Plus size={16}/></button></div>
        {devices.length ? devices.map(d => <button className={`rail-device ${device?.id === d.id ? 'selected' : ''}`} key={d.id} aria-label={`Select ${d.name}`} aria-pressed={device?.id === d.id} onClick={() => { setSelected(d.id); noiseNoticeDeviceId.current = null; setNotice(''); }}><span className="rail-headphone"><HeadphoneIcon size={23} strokeWidth={1.3}/></span><span><strong>{d.name}</strong><small><span className={`status-dot ${preview || d.connection === 'connected' ? 'online' : ''}`}/>{preview ? 'Interactive preview' : state.activeOutputDeviceId === d.id ? 'System audio output' : d.connection}</small></span>{device?.id === d.id && <ChevronRight size={13}/>}</button>) : <div className="rail-empty"><HeadphoneIcon size={21}/><p>No headphones yet</p><span>Pair a device to get started.</span></div>}
      </div>
      <div className="sidebar-bottom"><div className="local-note"><ShieldCheck size={16}/><span>Local by design<small>No account. No telemetry.</small></span></div><div className="sidebar-meta"><span>Independent desktop app</span><span>0.1</span></div></div>
    </aside>

    <main id="studio" className="workspace">
      <header className="topbar"><div className="breadcrumb">Your workspace <ChevronRight size={12}/><span>{navigation.find(n => n.id === page)?.label}</span></div><div className="top-actions"><span className={`environment ${preview ? 'preview' : ''}`}><span className="status-dot"/>{preview ? 'Studio preview' : nativeAvailable ? state.platform : 'Browser preview'}</span><button className="icon-button theme-button" disabled={!preferencesLoaded || preferencesLoading || !!pendingPreference} onClick={() => void updatePreference('theme', effectiveTheme === 'dark' ? 'light' : 'dark')} aria-label={effectiveTheme === 'dark' ? 'Light theme' : 'Dark theme'} title={effectiveTheme === 'dark' ? 'Light theme' : 'Dark theme'}>{effectiveTheme === 'dark' ? <Sun size={18}/> : <Moon size={18}/>}</button></div></header>
      {preview && <div className="preview-strip" role="status"><span><Sparkles size={14}/><strong>Preview mode</strong><span className="preview-long"> · changes stay on this computer</span></span><button onClick={exitPreview}>Exit preview <ArrowRight size={14}/></button></div>}
      {error && <div className="feedback error" role="alert"><Info size={17}/><span>{error}</span><button className="icon-button" aria-label="Dismiss error" onClick={() => setError(null)}><X size={16}/></button></div>}
      {notice && <div className="feedback" role="status">{notice.includes('requested. Device state could not be verified.') ? <Info size={16}/> : <Check size={16}/>}<span>{notice}</span><button className="icon-button" aria-label="Dismiss notification" onClick={() => setNotice('')}><X size={16}/></button></div>}
      {busy && <div className="operation-status" role="status"><LoaderCircle size={15} className="spinner"/> {busy === 'discover_devices' ? 'Finding your headphones…' : busy === 'connect_device' ? 'Opening the control connection…' : busy === 'apply_setting' ? 'Waiting for device confirmation…' : 'Updating desktop state…'}</div>}

      {page === 'sound' && (!device ? <Welcome busy={!!busy} native={nativeAvailable} transportStatus={state.transportStatus} onDiscover={() => void runNative('discover_devices')} onPreview={enterPreview}/> : <>
        <div className="page-heading reveal"><div><div className="eyebrow">YOUR PERSONAL AUDIO STUDIO</div><h1>Your listening space.</h1></div><span className="heading-index">01 <span>/ SOUND</span></span></div>
        <section className="device-hero reveal"><DeviceSummary device={device} preview={preview} activeOutput={state.activeOutputDeviceId === device.id} listening={listeningLabel} noise={heroNoiseMode === 'unknown' ? 'Not confirmed' : noiseModeLabel(heroNoiseMode)} pending={pendingDeviceSetting?.deviceId === device.id} busy={!!busy} onConnection={() => void runNative(device.connection === 'connected' ? 'disconnect_device' : 'connect_device', { deviceId: device.id })}/>
        <SoundVisualization key={device.id} deviceId={device.id} audioReactive={!preview && state.platform.toLowerCase() === 'windows' && device.connection === 'connected' && state.activeOutputDeviceId === device.id} pending={pendingDeviceSetting?.deviceId === device.id} model={device.model} noise={heroNoiseMode} listening={listeningMode} room={preview ? current.bgmRoomSize ?? 0 : device.settings.bgmRoomSize} bands={visualizationBands} initialBands={lastStudioBands.current[visualKey]} preview={preview} reducedMotion={reducedMotion}/><div className="hero-corner"><span>{preview ? 'INTERACTIVE PREVIEW' : 'LIVE DEVICE STATE'}</span><ArrowRight size={17}/></div></section>
        <div className="studio-controls">
          {onePlus ? <OnePlusListeningModes spatialDescriptor={spatialDescriptor} gameDescriptor={gameDescriptor} gameSoundDescriptor={gameSoundDescriptor} spatialValue={spatialValue} gameValue={gameValue} gameSoundValue={gameSoundValue} busy={!!busy} pending={pendingDeviceSetting?.deviceId === device.id && ['spatialAudio', 'gameMode', 'gameSoundProfile'].includes(pendingDeviceSetting.key)} onChange={setting}/>
            : <ListeningModes descriptor={listeningDescriptor} roomDescriptor={roomDescriptor} mode={listeningMode} room={preview ? current.bgmRoomSize ?? 0 : device.settings.bgmRoomSize} preview={preview} busy={!!busy} pending={pendingDeviceSetting?.deviceId === device.id && ['listeningMode', 'bgmRoomSize'].includes(pendingDeviceSetting.key)} onChange={setting}/>}
          <NoisePanel preview={preview} available={can(isOnePlusDevice(device, preview) ? 'noiseProfile' : 'noise')} busy={!!busy} settings={current} device={device} onChange={setting}/>
        </div>
      </>)}
      {page === 'equalizer' && <>
        <div className="page-heading reveal"><div><div className="eyebrow">THE FINER FREQUENCIES</div><h1>Equalizer.</h1></div>{device && <span className="equalizer-device"><HeadphoneIcon size={17}/>{device.name}</span>}</div>
        {!device ? <section className="settings-surface reveal"><SlidersHorizontal size={30}/><h2>Find your headphones first.</h2><p className="body-copy">Connect a paired headphone to adjust its equalizer, or explore the studio preview.</p><button className="button" disabled={!!busy} onClick={() => void runNative('discover_devices')}>Find headphones <ArrowRight size={16}/></button><button className="text-button" onClick={enterPreview}>Explore studio preview <ArrowRight size={14}/></button></section> : <div className="equalizer-workspace">
{upmixActive && <p className="body-copy" role="status">EQ and DSEE are suspended in Music, Cinema and Game. Choose Standard on the main page to adjust them.</p>}
<Equalizer key={`${preview}:${device.id}:${device.firmware}`} preview={preview} available={can(isOnePlusDevice(device, preview) ? 'eqPreset' : 'equalizer')} busy={!!busy} settings={current} device={device} onChange={setting} presets={presets} devicePresets={devicePresets} onReplace={next => { if (!saveLocal('presets', next)) return false; setPresets(next); setNotice('Preset library saved to this computer.'); return true; }} onReplaceDevice={next => { if (!saveLocal('device-presets', next)) return false; setDevicePresets(next); setNotice('Device preset library saved to this computer.'); return true; }}/>
          {!preview && <BassControl device={device} busy={!!busy} onChange={setting}/>}
          {!isOnePlusDevice(device, preview) && <section className="enhancement-strip reveal"><span className="enhancement-icon"><Sparkles size={21}/></span><div><h3>{/1000XX|COLLEXION/i.test(device.model) ? 'DSEE Ultimate' : device.model.includes('CH720') ? 'DSEE' : 'DSEE Extreme'}</h3><p>{preview ? 'Explore the sound enhancement control.' : dseeDescriptor ? 'DSEE mode confirmed by your headphone.' : 'DSEE mode requires a verified device capability.'}</p></div>{preview ? <Toggle label="Sound enhancement" checked={current.dsee} disabled={!!busy} onChange={value => updatePreview({ dsee: value })}/> : dseeDescriptor && typeof device.settings.dsee === 'string' ? <div className="dsee-control"><select className="device-control-select" aria-label="DSEE mode" value={String(dseeDescriptor.options?.findIndex(option => option.value === device.settings.dsee) ?? -1)} disabled={!!busy || upmixActive} onChange={event => { const option = dseeDescriptor.options?.[Number(event.target.value)]; if (option) setting('dsee', option.value); }}>{dseeDescriptor.options?.map((option, index) => <option key={`${String(option.value)}-${index}`} value={index}>{option.label}</option>)}</select><small role={dseePending ? 'status' : undefined}>{dseePending ? 'Waiting for headphone confirmation…' : 'Confirmed by headphone'}</small>{dseeError && <small className="dsee-error" role="alert">{dseeError}</small>}</div> : <span className="status-label">Not confirmed</span>}{preview && <span className="subtle-tag">PREVIEW</span>}</section>}
        </div>}
      </>}
      {page === 'automatic' && <><div className="page-heading reveal"><div><div className="eyebrow">FOLLOW YOUR MEDIA</div><h1>Automatic listening.</h1></div><Radio size={28} strokeWidth={1}/></div>{preview ? <Unavailable text="Exit studio preview to configure automatic listening for your headphones."/> : <MediaFollowing device={device}/>}</>}
      {page === 'history' && <><div className="page-heading reveal"><div><div className="eyebrow">YOUR LISTENING LOG</div><h1>Listening history.</h1></div><HistoryIcon size={28} strokeWidth={1}/></div><HistoryPage reducedMotion={reducedMotion} deviceName={device?.name}/></>}
      {page === 'device' && <DevicePage device={device} preview={preview} current={current} busy={!!busy} pending={pendingDeviceSetting} error={deviceSettingError?.deviceId === device?.id ? deviceSettingError.message : null} onChange={setting} onDiscover={() => void runNative('discover_devices')}/>}
      {page === 'coverage' && <CoveragePage device={device} preview={preview}/>}
      {page === 'settings' && <><div className="page-heading reveal"><div><div className="eyebrow">A QUIETER KIND OF SOFTWARE</div><h1>Make yourself at home.</h1></div><Settings2 size={28} strokeWidth={1}/></div><section className="settings-surface reveal" aria-busy={preferencesLoading || !!pendingPreference}><div className="section-title"><h2>Appearance & motion</h2><span>YOUR WORKSPACE</span></div>{preferencesLoading && <p className="preference-status" role="status">Loading saved preferences…</p>}{preferencesError && <div className="preference-error" role="alert"><Info size={15}/><span>{preferencesError}</span>{nativeAvailable && <button className="text-button" disabled={preferencesLoading} onClick={() => void loadAppPreferences()}>Retry</button>}</div>}{preferencesLoaded ? <><SettingRow title="Color scheme" description="Choose a fixed palette or follow your operating system."><select className="preference-select" aria-label="Color scheme" value={preferences.theme} disabled={preferencesLoading || !!pendingPreference} onChange={e => void updatePreference('theme', e.target.value as AppTheme)}><option value="dark">Dark</option><option value="light">Light</option><option value="system">System</option></select></SettingRow><SettingRow title="Reduce motion" description={osPreferences.systemReduced ? 'Your system requests reduced motion; the studio is respecting it.' : 'Simplify the studio’s transitions and entrance motion.'}><Toggle label="Reduce motion" checked={preferences.reduceMotion} disabled={preferencesLoading || !!pendingPreference} onChange={value => void updatePreference('reduceMotion', value)}/></SettingRow></> : <Unavailable text="Saved desktop preferences are unavailable until the native settings store responds."/>}</section><section className="settings-surface reveal" aria-busy={preferencesLoading || !!pendingPreference}><div className="section-title"><h2>Desktop behavior</h2><Cpu size={19}/></div>{preferencesLoaded ? <><SettingRow title="Close to tray" description="Keep connected headphones available after closing the studio window."><Toggle label="Close to tray" checked={preferences.closeToTray} disabled={preferencesLoading || !!pendingPreference} onChange={value => void updatePreference('closeToTray', value)}/></SettingRow><SettingRow title="Launch at login" description="Start the desktop companion when you sign in to this computer."><Toggle label="Launch at login" checked={preferences.launchAtLogin} disabled={preferencesLoading || !!pendingPreference} onChange={value => void updatePreference('launchAtLogin', value)}/></SettingRow><SettingRow title="Reconnect automatically" description="Connect supported headphones when Bluetooth connects, and retry interrupted control connections."><Toggle label="Reconnect automatically" checked={preferences.autoReconnect} disabled={preferencesLoading || !!pendingPreference} onChange={value => void updatePreference('autoReconnect', value)}/></SettingRow><SettingRow title="Start minimized" description="Start in the tray instead of showing the studio window."><Toggle label="Start minimized" checked={preferences.startMinimized} disabled={preferencesLoading || !!pendingPreference} onChange={value => void updatePreference('startMinimized', value)}/></SettingRow></> : <Unavailable text="System startup and window behavior controls require the native preferences store."/>}</section><AppUpdates onVersionChange={updateAppVersion}/><section className="settings-surface reveal"><div className="section-title"><h2>Privacy & diagnostics</h2><ShieldCheck size={18}/></div><p className="body-copy">Your device preferences and listening history stay on this computer. No account or analytics, and nothing is uploaded. You choose what history records and can clear it on the Listening history page. Bluetooth control runs in the desktop app.</p><SettingRow title="NC/ASM protocol probe" description="Read-only probe of Sony’s NC/ASM capability and status replies. Raw response bytes stay in local diagnostics for review."><button className="text-button" disabled={!device || preview || device.connection !== 'connected' || !!busy} onClick={() => void probeNoiseSchema()}>Read NC/ASM responses <ArrowRight size={15}/></button></SettingRow><SettingRow title="Transport diagnostics" description="Inspect bounded local messages before choosing to export."><button className="text-button" disabled={nativeAvailable && diagnosticsLoading} onClick={() => void loadDiagnostics()}>{diagnosticsLoading ? 'Inspecting…' : 'Inspect'} <ArrowRight size={15}/></button></SettingRow>{showDiagnostics && <div className="diagnostics"><div className="diagnostic-header"><span>SESSION DIAGNOSTICS</span><button className="text-button" disabled={diagnosticsLoading} onClick={() => void loadDiagnostics()} aria-label="Refresh diagnostics"><RefreshCw size={14}/>Refresh</button></div><pre>{diagnostics.length ? diagnostics.join('\n') : 'No diagnostic messages recorded.'}</pre>{exportReady ? <div className="export-confirm"><p>This text export includes the messages displayed above and your platform name. Review them for private information before saving.</p><button className="button small" onClick={exportDiagnostics}>Save text file <ArrowDownToLine size={14}/></button><button className="text-button" onClick={() => setExportReady(false)}>Cancel</button></div> : <button className="text-button" onClick={() => setExportReady(true)}>Review export <ArrowDownToLine size={14}/></button>}</div>}</section><section className="about-note reveal"><AudioLines size={25}/><div><h3>Sound Connect Desktop <span>{appVersion}</span></h3><p>An independent open-source project. Not affiliated with or endorsed by Sony. Sony and its product names belong to their respective owners.</p></div></section></>}
      <footer className="workspace-footer"><span><span className="tiny-square"/> LISTEN ON YOUR OWN TERMS</span><span>{preview ? 'Preview · no headphone commands sent' : 'Local-first · independent desktop studio'}</span></footer>
    </main>
  </div>;
}

function Welcome({ busy, native, transportStatus, onDiscover, onPreview }: { busy: boolean; native: boolean; transportStatus: string; onDiscover: () => void; onPreview: () => void }) {
  return <div className="welcome"><div className="welcome-heading reveal"><div className="eyebrow"><span className="eyebrow-line"/> YOUR SOUND, ON DESKTOP</div><h1>Your sound.<br/><em>Within reach.</em></h1><p>A dedicated space for your headphones.<br/>Less friction between you and the music.</p><div className="welcome-actions"><button className="button" disabled={busy} onClick={onDiscover}><Bluetooth size={17}/>{busy ? 'Finding headphones…' : 'Find headphones'}<ArrowRight size={17}/></button><button className="text-button" onClick={onPreview}>Explore studio preview <ChevronRight size={15}/></button></div><div className="welcome-status"><span className="status-dot"/>{native ? 'Pair your headphones in system Bluetooth settings first.' : 'Bluetooth control is available in the desktop app.'}</div></div><div className="welcome-art reveal"><div className="welcome-ring ring-a"/><div className="welcome-ring ring-b"/><div className="welcome-art-word" aria-hidden="true">FEEL<br/>EVERY<br/>NOTE.</div><Headphones/><span className="art-caption"><span>PERSONAL SOUND</span><span>REIMAGINED FOR DESKTOP ↗</span></span></div><section className="onboarding reveal" aria-label="Getting connected"><div className="onboarding-step"><span>01</span><div><h2>Pair once.</h2><p>Connect through your computer’s Bluetooth settings.</p></div></div><div className="onboarding-step"><span>02</span><div><h2>Find your headphones.</h2><p>Discover a real device and open its control connection.</p></div></div><div className="onboarding-step"><span>03</span><div><h2>Make it yours.</h2><p>Available controls follow your device’s verified capabilities.</p></div></div></section><div className="connection-note reveal"><Info size={16}/><p>{native ? transportStatus : 'The studio preview is interactive and isolated. Its sample settings do not change connected headphones.'}</p></div></div>;
}

function NoisePanel({ preview, available, busy, settings, device, onChange }: { preview: boolean; available: boolean; busy: boolean; settings: PreviewSettings; device: Device; onChange: (key: string, value: unknown) => void }) {
  const mode = preview ? settings.noise : typeof device.settings.noise === 'string' ? device.settings.noise : '';
  const onePlus = isOnePlusDevice(device, preview);
  const descriptors: unknown = device.settings.controlDescriptors;
  const profile = Array.isArray(descriptors) ? descriptors.find((item: unknown): item is DeviceControlDescriptor => {
    if (!item || typeof item !== 'object') return false;
    const candidate = item as Partial<DeviceControlDescriptor>;
    return candidate.key === 'noiseProfile' && candidate.kind === 'select' && device.capabilities.includes('noiseProfile')
      && Array.isArray(candidate.options) && candidate.options.length > 0
      && candidate.options.every(option => option && typeof option.label === 'string' && ['string', 'number', 'boolean'].includes(typeof option.value));
  }) : undefined;
  const unconfirmed = !preview && device.settings.noiseConfirmation === 'unconfirmed';
  return <section className="sound-panel noise-panel reveal">
    <div className="section-title"><div><span className="eyebrow">YOUR SURROUNDINGS</span><h2>Noise control</h2><p>Choose how much of the outside you hear.</p></div><Waves size={23} strokeWidth={1.3}/></div>
    {onePlus ? profile ? <div className="preset-select"><label htmlFor="oneplus-noise-profile">Noise profile</label><select id="oneplus-noise-profile" aria-label="Noise profile" value={String(device.settings.noiseProfile)} disabled={busy} onChange={event => {
      const option = profile.options?.find(item => String(item.value) === event.target.value);
      if (option) onChange('noiseProfile', option.value);
    }}>{profile.options?.map(option => <option key={String(option.value)} value={String(option.value)}>{option.label}</option>)}</select></div> : <Unavailable text="Noise profiles are unavailable because this headphone has not confirmed its supported modes and current selection."/> : <div className="segmented" role="group" aria-label="Noise control">{[{ id: 'cancelling', label: 'Noise cancelling', icon: AudioLines }, { id: 'ambient', label: 'Ambient sound', icon: Waves }, { id: 'off', label: 'Off', icon: CircleOff }].map(({ id, label, icon: Icon }) => <button key={id} aria-pressed={!unconfirmed && mode === id} disabled={!available || busy} className={!unconfirmed && mode === id ? 'selected' : ''} onClick={() => onChange('noise', id)}><Icon size={19}/><span>{label}</span></button>)}</div>}
    {!onePlus && (!available ? <Unavailable text="Noise control is unavailable because this connected device has not confirmed its current mode."/> : unconfirmed ? <div className="mode-note" role="status"><Info size={16}/><p>{typeof device.settings.noiseRequested === 'string' ? `${noiseModeLabel(device.settings.noiseRequested)} requested. Device state could not be verified.` : 'Device state could not be verified.'}</p></div> : mode === 'ambient' && !preview ? <><Unavailable text='Ambient mode is confirmed. Its level slider stays off until this headphone reports its supported range.'/><div className="mode-note"><span className="signal-mark"><span/><span/><span/></span><p>Hear your surroundings naturally.</p></div></> : mode === 'ambient' ? <><div className="ambient-control"><label htmlFor="ambient">Ambient sound level <output>{settings.ambient}</output></label><input id="ambient" aria-label="Ambient sound level" type="range" min="1" max="20" value={settings.ambient} disabled={busy} onChange={e => onChange('ambient', Number(e.target.value))}/><div><span>Stay in your space</span><span>Let more in</span></div></div></> : <div className="mode-note"><span className="signal-mark"><span/><span/><span/></span><p>{mode === 'cancelling' ? 'Find a little quiet. Focus on what moves you.' : 'Hear your surroundings naturally.'}</p></div>)}
  </section>;
}


function Toggle({ checked, onChange, label, disabled = false }: { checked: boolean; onChange: (checked: boolean) => void; label: string; disabled?: boolean }) {
  return <button className="toggle" type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)}><span/></button>;
}
function SettingRow({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return <div className="setting-row"><div><h3>{title}</h3><p>{description}</p></div>{children}</div>;
}
function Unavailable({ text }: { text: string }) { return <p className="unavailable"><Info size={14}/>{text}</p>; }

function powerOffOptionLabel(value: unknown) {
  if (typeof value !== 'number') return 'a current power-off value';
  return ({ 0: '5 minutes', 1: '30 minutes', 2: '60 minutes', 3: '180 minutes', 4: '15 minutes', 16: 'when removed from ears', 17: 'disabled' } as Record<number, string>)[value] ?? `power-off option ${value}`;
}

function DevicePage({ device, preview, current, busy, pending, error, onChange, onDiscover }: { device?: Device; preview: boolean; current: PreviewSettings; busy: boolean; pending: PendingDeviceSetting | null; error: string | null; onChange: (key: string, value: unknown) => void; onDiscover: () => void }) {
  const previewDescriptors: DeviceControlDescriptor[] = [
    { key: 'speak', label: 'Speak-to-Chat', description: 'A conversation starts. Your listening takes a pause.', kind: 'toggle' },
    { key: 'wear', label: 'Pause when headphones are removed', description: 'Keep your place when you step away.', kind: 'toggle' },
    { key: 'touch', label: 'Touch sensor control panel', description: 'Control playback with gestures on the earcup.', kind: 'toggle' },
    { key: 'dsee', label: 'Sound enhancement', description: 'Explore the illustrative sound-enhancement control.', kind: 'toggle' },
  ];
  const rawDescriptors = preview ? previewDescriptors : device?.settings.controlDescriptors;
  const descriptors = device && preview
    ? previewDescriptors.filter(descriptor => typeof current[descriptor.key as keyof PreviewSettings] === 'boolean')
    : device && Array.isArray(rawDescriptors)
      ? rawDescriptors.filter((value): value is DeviceControlDescriptor => isDeviceControlDescriptor(value, device))
        .filter((descriptor, index, all) => all.findIndex(candidate => candidate.key === descriptor.key) === index)
      : [];
  const descriptorKeys = new Set(descriptors.map(descriptor => descriptor.key));
  // Focus on voice is part of Ambient sound mode; the headphone ignores it in other modes.
  const needsAmbient = (key: string) => !preview && key === 'focusOnVoice' && device?.settings.noise !== 'ambient';
  const onePlus = isOnePlusDevice(device, preview);
  const genericKnownControls = [
    { key: 'noise', label: 'Noise control mode', reason: 'The device has not confirmed its current mode.' },
    { key: 'listeningMode', label: 'Listening mode', reason: 'The headphone has not confirmed a complete listening-mode menu.' },
    { key: 'bgmRoomSize', label: 'Background Music distance', reason: 'The headphone has not advertised or confirmed a Background Music environment setting.' },
    { key: 'ambient', label: 'Ambient sound level', reason: 'The headphone has not supplied supported minimum, maximum, and step values.' },
    { key: 'focusOnVoice', label: 'Focus on voice', reason: 'The device has not advertised a verified voice-focus setting.' },
    { key: 'speakToChat', label: 'Speak-to-Chat', reason: 'The device has not advertised a verified Speak-to-Chat control.' },
    { key: 'wear', label: 'Pause when headphones are removed', reason: 'The desktop has not validated this device’s wear-detection setting.' },
    { key: 'touch', label: 'Touch sensor control panel', reason: 'The desktop has not validated this device’s touch-control setting.' },
    { key: 'dsee', label: 'DSEE upscaling', reason: 'The device has not advertised a verified DSEE upscaling setting.' },
    { key: 'multipoint', label: 'Multipoint connections', reason: 'The desktop has not validated this device’s multipoint controls.' },
    { key: 'autoPowerOff', label: 'Automatic power off', reason: 'The headphone has not returned a validated power-off capability list and current value.' },
    { key: 'guidance', label: 'Voice guidance & language', reason: 'The desktop has not validated this device’s guidance controls.' },
    { key: 'notificationTone', label: 'Notification tones', reason: 'The desktop has not validated this device’s notification-tone setting.' },
    { key: 'soundQuality', label: 'Bluetooth sound quality', reason: 'The desktop has not validated this device’s sound-quality setting.' },
  ];
  const knownControls = genericKnownControls.filter(control => !descriptorKeys.has(control.key) && !onePlus);
  const settingError = error;
  return <><div className="page-heading reveal"><div><div className="eyebrow">THE DETAILS MAKE THE DIFFERENCE</div><h1>In tune with your day.</h1></div><span className="heading-index">02 <span>/ DEVICE</span></span></div>{!device ? <section className="settings-surface reveal"><HeadphoneIcon size={36}/><h2>A headphone belongs here.</h2><p className="body-copy">Find a paired device to inspect its identity and settings.</p><button className="button" onClick={onDiscover} disabled={busy}>Find headphones <ArrowRight size={16}/></button></section> : <><section className="device-details reveal"><div className="details-art"><Headphones compact model={device.model}/></div><div><div className="eyebrow">{preview ? 'ILLUSTRATIVE DEVICE' : 'YOUR HEADPHONES'}</div><h2>{device.name}</h2><dl><div><dt>Control channel</dt><dd>{preview ? 'Preview only' : device.connection}</dd></div><div><dt>Firmware</dt><dd>{device.firmware ?? 'Not read from device'}</dd></div><div><dt>Battery</dt><dd>{batteryReadout(device)}</dd></div></dl></div></section><ProtocolMetadata device={device} preview={preview}/><section className="settings-surface reveal"><div className="section-title"><h2>Confirmed device controls</h2>{preview && <span>PREVIEW SETTINGS</span>}</div>{settingError && <div className="preference-error" role="alert"><Info size={15}/><span>{settingError}</span></div>}{descriptors.length ? descriptors.map(descriptor => <DeviceControlRow key={descriptor.key} descriptor={needsAmbient(descriptor.key) ? { ...descriptor, description: 'Available while Noise control is set to Ambient sound.' } : descriptor} value={preview ? current[descriptor.key as keyof PreviewSettings] as SettingPrimitive : device.settings[descriptor.key] as SettingPrimitive} pending={pending?.deviceId === device.id && pending.key === descriptor.key} disabled={busy || needsAmbient(descriptor.key)} onChange={onChange}/>) : <p className="body-copy">No device controls have confirmed capability and current-value data yet.</p>}{!preview && knownControls.length > 0 && <div className="unverified-controls"><div className="section-title"><h3>Not confirmed for this device</h3><span>{knownControls.length} CONTROLS</span></div>{knownControls.map(control => {
        const connected = device.connection === 'connected';
        const advertised = device.capabilities.includes(control.key);
        const hasValue = Object.hasOwn(device.settings, control.key);
        const modePending = control.key === 'noise' && device.settings.noiseConfirmation === 'unconfirmed';
        const rangeUnknown = control.key === 'ambient' && hasValue;
        const confirmedReadOnly = advertised && hasValue && !modePending && !rangeUnknown;
        const supportReplyConfirmed = device.settings.supportReplyConfirmed === true;
        const functionBacked = ['wear', 'touch', 'multipoint', 'guidance', 'notificationTone', 'soundQuality', 'autoPowerOff'].includes(control.key);
        const notAdvertised = supportReplyConfirmed && functionBacked && !advertised;
        const advertisedSchemaOnly = advertised && functionBacked && !hasValue;
        const unsupportedCommandGroup = advertisedSchemaOnly && ['guidance', 'notificationTone', 'soundQuality'].includes(control.key);
        const reason = !connected ? 'Connect this headphone to refresh its supported controls and current values.'
          : modePending ? 'A mode response was observed, but the requested final state has not been confirmed.'
          : rangeUnknown ? 'A device level was read, but its supported minimum, maximum, and step are not established.'
          : confirmedReadOnly && control.key === 'autoPowerOff' ? `The headphone reports automatic power off ${powerOffOptionLabel(device.settings.autoPowerOff)}, but this variant has no validated editable path yet.`
          : confirmedReadOnly ? 'The device returned a current value, but no validated editable control descriptor is available.'
          : control.key === 'listeningMode' && advertised ? 'The headphone advertises a listening-mode feature, but the desktop has not confirmed a complete mode map and current state.'
          : notAdvertised ? `This headphone did not list ${control.label.toLowerCase()} among the functions it supports, so there is nothing to change.`
          : unsupportedCommandGroup ? 'The headphone lists this feature, but it uses a command group this desktop does not control yet.'
          : advertisedSchemaOnly ? 'The headphone lists this setting, but its current value could not be read. Reconnect to try again.'
          : control.reason;
        const status = !connected ? 'Not connected' : modePending ? 'Unconfirmed' : rangeUnknown ? 'Range unknown' : confirmedReadOnly ? 'Read only' : notAdvertised ? 'Not advertised' : advertisedSchemaOnly || (control.key === 'listeningMode' && advertised) ? 'Advertised, readback unavailable' : 'Not validated';
        return <SettingRow key={control.key} title={control.label} description={reason}><span className="status-label">{status}</span></SettingRow>;
      })}</div>}</section><section className="settings-surface reveal"><div className="section-title"><h2>Connections & maintenance</h2><Bluetooth size={19}/></div><SettingRow title="Firmware updates" description="Updates remain unavailable until package authenticity, compatibility and safe recovery are verified."><span className="status-label">Not available</span></SettingRow><p className="unavailable"><Info size={14}/>No reset or firmware transfer command is exposed by this build.</p></section></>}</>;
}

function DeviceControlRow({ descriptor, value, pending, disabled, onChange }: { descriptor: DeviceControlDescriptor; value: SettingPrimitive; pending: boolean; disabled: boolean; onChange: (key: string, value: unknown) => void }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => { if (!pending) setDraft(value); }, [pending, value]);
  const options = descriptor.options ?? [];
  const commitRangeDraft = () => {
    if (descriptor.kind === 'range' && typeof draft === 'number' && draft !== value && !pending && !disabled) onChange(descriptor.key, draft);
  };
  const control = descriptor.kind === 'toggle' && typeof value === 'boolean'
    ? <Toggle label={descriptor.label} checked={value} disabled={disabled} onChange={next => onChange(descriptor.key, next)}/>
    : descriptor.kind === 'select'
      ? <select className="device-control-select" aria-label={descriptor.label} value={String(options.findIndex(option => option.value === value))} disabled={disabled} onChange={event => { const option: DeviceControlOption | undefined = options[Number(event.target.value)]; if (option) onChange(descriptor.key, option.value); }}>{options.map((option, index) => <option key={`${index}-${String(option.value)}`} value={index}>{option.label}</option>)}</select>
      : descriptor.kind === 'range' && typeof value === 'number'
        ? <div className="device-range"><label htmlFor={`device-setting-${descriptor.key}`}>{descriptor.label}<output>{typeof draft === 'number' ? draft : value}</output></label><input id={`device-setting-${descriptor.key}`} aria-label={descriptor.label} type="range" min={descriptor.min} max={descriptor.max} step={descriptor.step} value={typeof draft === 'number' ? draft : value} disabled={disabled} onChange={event => setDraft(Number(event.target.value))} onPointerUp={commitRangeDraft} onPointerCancel={commitRangeDraft} onKeyUp={event => { if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)) commitRangeDraft(); }} onBlur={commitRangeDraft}/></div>
        : <span className="status-label">Invalid control</span>;
  return <SettingRow title={descriptor.label} description={descriptor.description}><div className="device-control-action">{control}<small role={pending ? 'status' : undefined}>{pending ? `Device confirmed: ${value}. Waiting for headphone confirmation…` : draft !== value ? 'Draft change · release to send' : 'Confirmed by device'}</small></div></SettingRow>;
}

function ProtocolMetadata({ device, preview }: { device: Device; preview: boolean }) {
  if (preview) return null;
  const rawVersion = device.settings.protocolVersionRaw;
  const protocolFormat = device.settings.protocolFormat;
  const status = device.settings.protocolStatus;
  const service = device.settings.serviceUuid;
  return <section className="settings-surface protocol-metadata reveal">
    <div className="section-title"><h2>Device control connection</h2><Radio size={19}/></div>
    <p className="body-copy">This is the headphone’s settings channel. Audio pairing and playback are managed by your operating system.</p>
    <dl>
      <div><dt>Protocol response</dt><dd>{typeof status === 'string' ? status : 'No protocol reply recorded for this session.'}</dd></div>
      <div><dt>Protocol format</dt><dd>{typeof protocolFormat === 'string' || typeof protocolFormat === 'number' ? String(protocolFormat) : 'Not confirmed'}</dd></div>
      <div><dt>Raw protocol version</dt><dd>{typeof rawVersion === 'number' ? `0x${rawVersion.toString(16).padStart(4, '0')}` : 'Not read'}</dd></div>
      <div><dt>Control service UUID</dt><dd className="service-uuid">{typeof service === 'string' ? service : 'No control service connected'}</dd></div>
      <div><dt>Negotiated feature controls</dt><dd>{device.capabilities.length ? device.capabilities.join(', ') : 'No feature controls validated yet'}</dd></div>
    </dl>
  </section>;
}

function CoveragePage({ device, preview }: { device?: Device; preview: boolean }) {
  const [filter, setFilter] = useState<'all' | 'device' | 'services'>('all');
  const sonyFeatures: { group: 'device' | 'services'; name: string; desc: string; status?: string; icon: typeof Waves }[] = [
    { group: 'device', name: 'Noise cancellation & ambient sound', desc: 'On XM6 3.1.5, noise mode uses inquiry 0x19. A mode is confirmed only after the device reports the requested final state; missing or conflicting reports stay unconfirmed. Ambient level appears only when the device supplies its range.', status: 'Final device state required', icon: Waves },
    { group: 'device', name: 'Equalizer', desc: 'XM6 3.1.5 uses a ten-band profile with device steps. Writes wait for matching readback; on mismatch the previous profile is restored only when a second read confirms it.', status: 'Ten-band readback · confirmed writes', icon: SlidersHorizontal },
    { group: 'device', name: 'Multipoint & everyday controls', desc: 'Documented features vary by model. Each control requires a supported-function response and round-trip test.', icon: Bluetooth },
    { group: 'device', name: 'Firmware & voice guidance', desc: 'Original app includes transfer paths. Signing, compatibility and safe recovery need separate validation.', icon: ShieldCheck },
    { group: 'services', name: 'Adaptive Sound Control', desc: 'Phone motion and location inputs do not directly map to desktop. A phone-free desktop equivalent needs validation.', icon: Activity },
    { group: 'services', name: '360 Reality Audio & spatial features', desc: 'Calibration, service eligibility, audio playback and host capabilities must work together end to end.', icon: Radio },
    { group: 'services', name: 'Safe listening & listening history', desc: 'Measurement provenance, units and headphone data access remain unresolved. No sample listening metrics are shown.', icon: Volume2 },
    { group: 'services', name: 'Quick Access, accounts & backup', desc: 'Service authentication and supported desktop integrations remain unresolved. Core operation needs no account.', icon: Layers3 },
  ];
  const onePlus = isOnePlusDevice(device, preview);
  const model = device?.model;
  const controlKeys = new Set(Array.isArray(device?.settings.controlDescriptors)
    ? device.settings.controlDescriptors.filter((item): item is DeviceControlDescriptor => !!item && typeof item === 'object' && typeof item.key === 'string').map(item => item.key)
    : []);
  const confirmedControl = (key: string) => !!device?.capabilities.includes(key) && controlKeys.has(key);
  const noiseAvailable = confirmedControl('noiseProfile');
  const equalizerAvailable = confirmedControl('eqPreset');
  const batteryAvailable = typeof device?.battery === 'number';
  const onePlusFeatures: typeof sonyFeatures = [
    { group: 'device', name: 'Noise profiles', desc: noiseAvailable ? 'The connected model advertises device-defined noise profiles. This app uses the reported labels and values.' : 'No noise profile control has been confirmed for this device. Non-ANC models may not provide one.', status: noiseAvailable ? 'Descriptor available · hardware validation pending' : 'Not confirmed for this device', icon: Waves },
    { group: 'device', name: 'Equalizer profiles', desc: equalizerAvailable ? 'The connected model advertises device-defined equalizer profiles. This app uses the reported labels and values; it does not assume a shared band layout.' : 'No equalizer profile control has been confirmed for this device.', status: equalizerAvailable ? 'Descriptor available · hardware validation pending' : 'Not confirmed for this device', icon: SlidersHorizontal },
    { group: 'device', name: 'Battery', desc: batteryAvailable ? 'A battery value is currently reported by the connected device. Earbud and case reporting may vary by model.' : 'No battery value is currently reported by the connected device.', status: batteryAvailable ? 'Readout reported' : 'Not reported', icon: Activity },
    { group: 'device', name: 'Wear detection', desc: confirmedControl('wearDetection') ? 'The device advertises a wear-detection setting descriptor and changes use native confirmation.' : 'Wear detection has not been advertised by this device.', status: confirmedControl('wearDetection') ? 'Descriptor available · hardware validation pending' : 'Not confirmed for this device', icon: Bluetooth },
    { group: 'device', name: 'Game mode', desc: confirmedControl('gameMode') ? 'The device advertises a game-mode setting descriptor and changes use native confirmation.' : 'Game mode has not been advertised by this device.', status: confirmedControl('gameMode') ? 'Descriptor available · hardware validation pending' : 'Not confirmed for this device', icon: Activity },
    { group: 'device', name: 'Spatial audio', desc: confirmedControl('spatialAudio') ? 'The negotiated protocol descriptor reports spatial-audio modes and numeric values. Physical-device confirmation is still pending.' : 'Spatial-audio modes have not been confirmed for this device.', status: confirmedControl('spatialAudio') ? 'Protocol confirmed · hardware validation pending' : 'Not confirmed for this device', icon: Radio },
    { group: 'device', name: 'Custom equalizer & multipoint', desc: 'Custom EQ data and multipoint control have no validated OnePlus write path in this build.', status: 'Not implemented', icon: SlidersHorizontal },
    { group: 'device', name: 'Firmware updates & voice guidance', desc: 'Firmware transfer, recovery, and voice-guidance controls need protocol research and safe device validation.', status: 'Not implemented', icon: ShieldCheck },
    { group: 'services', name: 'Phone app services', desc: 'Phone-dependent services, account features, and cloud backup are outside the current desktop control path.', status: 'Not implemented', icon: Layers3 },
  ];
  const features = onePlus ? onePlusFeatures : sonyFeatures;
  const intro = onePlus
    ? { title: 'OnePlus control coverage', body: `This experimental integration uses the HeyMelody OPO protocol path for ${model ?? 'this model'}. Each device is represented using only its reported capabilities and descriptors; physical-device validation is still in progress.`, evidence: 'Protocol-format integration · physical-device validation pending' }
    : { title: 'A complete view of the work ahead.', body: 'Official features, original-app research and hardware validation are different milestones. This build does not yet claim full Sony Sound Connect parity.', evidence: `${model ?? 'XM6 · XM5 · CH720N'} ${preview ? '· interactive preview is illustrative' : '· XM6 read paths tested; writes require confirmed readback'}` };
  return <><div className="page-heading reveal"><div><div className="eyebrow">EVIDENCE BEFORE PROMISES</div><h1>Know what’s connected.</h1></div><span className="heading-index">03 <span>/ DEVICE</span></span></div><section className="coverage-intro reveal"><div className="coverage-symbol"><Layers3 size={36} strokeWidth={1}/></div><div><h2>{intro.title}</h2><p>{intro.body}</p><span>{intro.evidence}</span></div></section><div className="coverage-filters reveal" role="group" aria-label="Feature category">{([{ id: 'all', label: 'All features' }, { id: 'device', label: 'Device controls' }, { id: 'services', label: 'Services & intelligence' }] as const).map(f => <button key={f.id} aria-pressed={filter === f.id} className={filter === f.id ? 'selected' : ''} onClick={() => setFilter(f.id)}>{f.label}</button>)}</div><div className="coverage-list">{features.filter(f => filter === 'all' || f.group === filter).map(({ name, desc, icon: Icon, status }, i) => <section className="coverage-row reveal" key={name}><span className="coverage-number">0{i + 1}</span><Icon size={22} strokeWidth={1.3}/><div><h2>{name}</h2><p>{desc}</p></div><span className="status-label">{status ?? 'Awaiting validation'}</span></section>)}</div><div className="research-note reveal"><CircleHelp size={20}/><div><h3>{onePlus ? 'Grounded in protocol research.' : 'Grounded in the original app.'}</h3><p>{onePlus ? 'OnePlus support here is based on HeyMelody OPO protocol inspection and per-device capability descriptors. Physical hardware validation remains outstanding; Sound Connect app research is not used for this model family.' : 'Research inspected Android Sound Connect 13.3.0 with REA. The project’s evidence report records artifact hashes, observed framing and command identifiers. No phone is required for this desktop product’s operation.'}</p></div></div></>;
}
