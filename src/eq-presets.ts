/** XM6 ten-band curves transcribed from the user's Sound Connect screenshots.
 * Frequencies and signed gains are separate from the wire's unsigned 0..12 steps.
 * Applying a curve uses the existing verified custom-EQ command, not a factory slot SET.
 */
export const xm6Frequencies = ['31 Hz', '63 Hz', '125 Hz', '250 Hz', '500 Hz', '1 kHz', '2 kHz', '4 kHz', '8 kHz', '16 kHz'];
export const sonyPresets = [
  { name: 'Off', description: 'A flat starting point.', bands: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
  { name: 'Heavy', description: 'More weight in the low end.', bands: [4, 4, -1, -1, 0, 0, 0, 0, 0, 0] },
  { name: 'Clear', description: 'Lift and detail in the treble.', bands: [0, 0, 0, 0, 0, 0, 0, 2, 2, 3] },
  { name: 'Hard', description: 'Emphasized bass and treble.', bands: [4, 4, -1, -1, 0, 0, 0, 2, 2, 3] },
  { name: 'Soft', description: 'Gentler bass and treble.', bands: [-3, -2, -1, 1, 1, 1, 0, 0, -2, -3] },
  { name: 'Game', description: 'Focus on footsteps and impact.', bands: [-6, -2, 2, 3, 1, -1, 1, 3, 0, -2] },
] as const;

export interface SavedPreset { name: string; bands: number[] }
export interface DevicePreset extends SavedPreset { model: string; firmware: string }
export const sameBands = (left: readonly number[], right: readonly number[]) => left.length === right.length && left.every((value, index) => value === right[index]);
export const toDeviceSteps = (bands: readonly number[]) => bands.map(value => value + 6);
export function presetNameError(name: string, presets: SavedPreset[], except?: string): string | null {
  const clean = name.trim();
  if (!clean || clean.length > 40 || /[\u0000-\u001f\u007f]/.test(clean)) return 'Use a name of 1–40 characters without control characters.';
  const lower = clean.toLocaleLowerCase();
  if (['custom', 'manual', 'device', ...sonyPresets.map(p => p.name.toLowerCase())].includes(lower)
    || presets.some(p => p.name !== except && p.name.toLocaleLowerCase() === lower)) return 'Choose a unique preset name.';
  return null;
}
