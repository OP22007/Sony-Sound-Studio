import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type ConnectionState = 'disconnected' | 'connecting' | 'connected';
export type AppTheme = 'dark' | 'light' | 'system';
export interface AppPreferences {
  theme: AppTheme;
  reduceMotion: boolean;
  closeToTray: boolean;
  launchAtLogin: boolean;
  autoReconnect: boolean;
  startMinimized: boolean;
  lowBatteryAlerts: boolean;
}
export const defaultAppPreferences: AppPreferences = {
  theme: 'dark', reduceMotion: false, closeToTray: true,
  launchAtLogin: false, autoReconnect: true, startMinimized: false, lowBatteryAlerts: true,
};
export type SettingPrimitive = string | number | boolean;
export interface DeviceControlOption { value: SettingPrimitive; label: string; enabled?: boolean }
export interface DeviceControlDescriptor {
  key: string;
  label: string;
  description: string;
  kind: 'toggle' | 'select' | 'range';
  options?: DeviceControlOption[];
  min?: number;
  max?: number;
  step?: number;
}
export interface Device {
  id: string; name: string; model: string; connection: ConnectionState;
  battery: number | null; firmware: string | null;
  capabilities: string[]; settings: Record<string, unknown>;
}

export function isAppPreferences(value: unknown): value is AppPreferences {
  if (!value || typeof value !== 'object') return false;
  const p = value as Record<string, unknown>;
  return (p.theme === 'dark' || p.theme === 'light' || p.theme === 'system')
    && typeof p.reduceMotion === 'boolean'
    && typeof p.closeToTray === 'boolean'
    && typeof p.launchAtLogin === 'boolean'
    && typeof p.autoReconnect === 'boolean'
    && typeof p.startMinimized === 'boolean'
    // Older native builds do not send this switch; it defaults to on.
    && (p.lowBatteryAlerts === undefined || typeof p.lowBatteryAlerts === 'boolean');
}
export interface AppState {
  revision: number; platform: string; transportStatus: string; devices: Device[]; diagnostics: string[];
  activeOutputDeviceId?: string | null;
  audioOutputObserved?: boolean;
}
export interface AppUpdateSnapshot {
  currentVersion: string;
  status: 'idle' | 'checking' | 'available' | 'upToDate' | 'downloading' | 'installing' | 'error' | 'unsupported';
  version: string | null;
  notes: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
}
export const nativeAvailable = isTauri();
export const browserState: AppState = {
  revision: 0, platform: 'Browser', transportStatus: 'Open the desktop app to discover your headphones.',
  devices: [], diagnostics: [],
};

export function nativeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!nativeAvailable) return Promise.reject(new Error('Bluetooth is available in the desktop app. This browser can explore the studio preview.'));
  return invoke<T>(command, args);
}

export function subscribeState(onState: (state: AppState) => void): Promise<() => void> {
  if (!nativeAvailable) return Promise.resolve(() => {});
  return listen<AppState>('app-state', event => onState(event.payload));
}
