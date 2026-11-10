import type { Device } from './bridge';

// Mirrors src-tauri/src/listening_policy.rs: which settings Follow media can apply to a device
// for each kind of playback. Only settings the device has confirmed (capability + descriptor
// option) count, so the page never implies support a model does not have.

export type AutomationCategory = 'game' | 'music' | 'movie' | 'other';
export type SupportLevel = 'full' | 'partial' | 'none';

export interface CategorySupport {
  level: SupportLevel;
  /** Settings that will be applied, in the same wording as the live "planned" list. */
  applies: string[];
  /** Settings the headphones do not offer for this kind of playback. */
  missing: string[];
}

type Wanted = { key: string; value: string | number | boolean; label: string; optional?: boolean };

const WANTED: Record<AutomationCategory, Wanted[]> = {
  game: [
    { key: 'listeningMode', value: 'game', label: 'Listening mode: game' },
    { key: 'spatialAudio', value: 0, label: 'Spatial audio: off' },
    { key: 'gameMode', value: true, label: 'Game mode: on' },
    { key: 'gameSoundProfile', value: 3, label: 'Game sound profile', optional: true },
  ],
  music: [
    { key: 'listeningMode', value: 'music', label: 'Listening mode: music' },
    { key: 'spatialAudio', value: 0, label: 'Spatial audio: off' },
    { key: 'gameMode', value: false, label: 'Game mode: off' },
  ],
  movie: [
    { key: 'listeningMode', value: 'cinema', label: 'Listening mode: cinema' },
    { key: 'spatialAudio', value: 1, label: 'Spatial audio: fixed' },
    { key: 'gameMode', value: false, label: 'Game mode: off' },
  ],
  other: [
    { key: 'listeningMode', value: 'standard', label: 'Listening mode: standard' },
    { key: 'spatialAudio', value: 0, label: 'Spatial audio: off' },
    { key: 'gameMode', value: false, label: 'Game mode: off' },
  ],
};

function descriptorFor(device: Device, key: string): { kind?: string; options?: { value: unknown; enabled?: boolean }[] } | undefined {
  const descriptors = device.settings.controlDescriptors;
  if (!Array.isArray(descriptors)) return undefined;
  return descriptors.find(d => d && typeof d === 'object' && (d as { key?: unknown }).key === key);
}

function offers(device: Device, key: string, value: unknown): boolean {
  if (!device.capabilities.includes(key)) return false;
  const descriptor = descriptorFor(device, key);
  if (!descriptor) return false;
  if (Array.isArray(descriptor.options)) return descriptor.options.some(option => option.value === value && option.enabled !== false);
  return descriptor.kind === 'toggle' && typeof value === 'boolean';
}

export function automationSupport(device: Device | undefined): Record<AutomationCategory, CategorySupport> | null {
  if (!device || device.connection !== 'connected') return null;
  const result = {} as Record<AutomationCategory, CategorySupport>;
  for (const category of Object.keys(WANTED) as AutomationCategory[]) {
    const applies: string[] = [];
    const missing: string[] = [];
    for (const wanted of WANTED[category]) {
      // A setting the model never has (for example spatial audio on a Sony headset) is not "missing".
      if (!device.capabilities.includes(wanted.key)) continue;
      if (offers(device, wanted.key, wanted.value)) applies.push(wanted.label);
      else if (!wanted.optional) missing.push(wanted.label);
    }
    const level: SupportLevel = applies.length === 0 ? 'none' : missing.length ? 'partial' : 'full';
    result[category] = { level, applies, missing };
  }
  return result;
}
