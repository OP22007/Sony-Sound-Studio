import type { Device } from './bridge';

export function isOnePlusModel(model: unknown): boolean {
  return typeof model === 'string' && /^oneplus(?:\s|$)/i.test(model.trim());
}

export function isOnePlusDevice(device?: Pick<Device, 'model' | 'settings'>, preview = false): boolean {
  return !preview && !!device && (device.settings.protocolFormat === 'heymelody-opo-v1' || isOnePlusModel(device.model));
}
