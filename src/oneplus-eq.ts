export interface OnePlusEqProfile {
  selected: number;
  minValue: number;
  maxValue: number;
  eqId: number;
  name: string;
  frequencies: number[];
  dbValues: number[];
}

export interface OnePlusEqRequest { action: 1 | 2; profile: OnePlusEqProfile }

const integer = (value: unknown, min: number, max: number): value is number =>
  typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;

export function isOnePlusEqProfile(value: unknown): value is OnePlusEqProfile {
  if (!value || typeof value !== 'object') return false;
  const p = value as Partial<OnePlusEqProfile>;
  return integer(p.selected, 0, 1) && integer(p.eqId, 0, 255)
    && integer(p.minValue, -128, 127) && integer(p.maxValue, -128, 127) && p.minValue < p.maxValue
    && typeof p.name === 'string' && new TextEncoder().encode(p.name).length <= 255
    && Array.isArray(p.frequencies) && p.frequencies.length > 0 && p.frequencies.length <= 16
    && p.frequencies.every(frequency => integer(frequency, 1, 65535))
    && new Set(p.frequencies).size === p.frequencies.length
    && Array.isArray(p.dbValues) && p.dbValues.length === p.frequencies.length
    && p.dbValues.every(db => integer(db, p.minValue!, p.maxValue!));
}

export function onePlusProfiles(value: unknown): OnePlusEqProfile[] {
  if (!Array.isArray(value) || value.length > 32 || !value.every(isOnePlusEqProfile)) return [];
  if (new Set(value.map(profile => profile.eqId)).size !== value.length) return [];
  return value;
}

export function isOnePlusEqRequest(value: unknown): value is OnePlusEqRequest {
  if (!value || typeof value !== 'object') return false;
  const request = value as Partial<OnePlusEqRequest>;
  return (request.action === 1 || request.action === 2) && isOnePlusEqProfile(request.profile);
}

export function frequencyLabel(hz: number): string {
  return hz >= 1000 ? `${Number((hz / 1000).toFixed(2))} kHz` : `${hz} Hz`;
}
