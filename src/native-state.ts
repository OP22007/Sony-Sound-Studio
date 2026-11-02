import type { AppState, Device } from './bridge';

/** Events and command responses can arrive out of order across the IPC boundary. */
export function selectNativeSnapshot(current: AppState, incoming: AppState): AppState {
  if (!Number.isSafeInteger(incoming.revision) || incoming.revision < 0 || incoming.revision < current.revision) return current;
  return incoming;
}

export function batteryReadout(device: Device): string {
  if (device.battery === null) return 'Not read from device';
  const charging = device.settings.batteryCharging;
  const labels: Record<string, string> = {
    notCharging: 'not charging', charging: 'charging', unknown: 'charging state unknown', charged: 'fully charged',
    '0': 'not charging', '1': 'charging', '2': 'charging state unknown', '3': 'fully charged',
  };
  const label = typeof charging === 'string' || typeof charging === 'number' ? labels[String(charging)] : undefined;
  return `${device.battery}%${label ? ` · ${label}` : ''}`;
}
