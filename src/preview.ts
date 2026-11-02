import type { Device } from './bridge';

export const frequencies = ['63 Hz', '125 Hz', '250 Hz', '500 Hz', '1 kHz', '2 kHz', '4 kHz', '8 kHz', '12 kHz', '16 kHz'];
export const stockPresets: Record<string, number[]> = {
  Balanced: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  Warm: [3, 3, 2, 1, 0, -1, -1, 0, 1, 1],
  Vocal: [-2, -1, 0, 1, 3, 4, 3, 1, 0, -1],
  Immersive: [4, 3, 1, 0, -1, 0, 2, 3, 3, 2],
};

export interface PreviewSettings {
  noise: 'cancelling' | 'ambient' | 'off'; ambient: number; equalizer: number[];
  preset: string; bass: number; dsee: boolean; speak: boolean; wear: boolean; touch: boolean;
  listeningMode?: 'standard' | 'backgroundMusic' | 'cinema'; bgmRoomSize?: number;
}
export const defaultSettings: PreviewSettings = {
  noise: 'cancelling', ambient: 10, equalizer: [...stockPresets.Immersive], preset: 'Immersive',
  bass: 2, dsee: true, speak: false, wear: true, touch: true, listeningMode: 'standard', bgmRoomSize: 0,
};
export const previewDevices: Device[] = [
  { id: 'preview-xm6', name: 'WH-1000XM6', model: 'WH-1000XM6', connection: 'connected', battery: null, firmware: null, capabilities: [], settings: {} },
  { id: 'preview-xm5', name: 'WH-1000XM5', model: 'WH-1000XM5', connection: 'connected', battery: null, firmware: null, capabilities: [], settings: {} },
  { id: 'preview-ch720n', name: 'WH-CH720N', model: 'WH-CH720N', connection: 'connected', battery: null, firmware: null, capabilities: [], settings: {} },
];

export function validPreview(value: unknown): value is Record<string, PreviewSettings> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  return Object.entries(value).every(([id, settings]) => {
    if (!previewDevices.some(device => device.id === id) || !settings || typeof settings !== 'object') return false;
    const s = settings as PreviewSettings;
    return ['cancelling', 'ambient', 'off'].includes(s.noise)
      && (s.listeningMode === undefined || ['standard', 'backgroundMusic', 'cinema'].includes(s.listeningMode))
      && (s.bgmRoomSize === undefined || [0, 1, 2].includes(s.bgmRoomSize))
      && Array.isArray(s.equalizer) && s.equalizer.length === 10 && s.equalizer.every(v => typeof v === 'number' && v >= -10 && v <= 10)
      && Number.isInteger(s.ambient) && s.ambient >= 1 && s.ambient <= 20
      && Number.isInteger(s.bass) && s.bass >= -10 && s.bass <= 10
      && typeof s.preset === 'string' && ['dsee', 'speak', 'wear', 'touch'].every(k => typeof s[k as keyof PreviewSettings] === 'boolean');
  });
}
