import { useMemo } from 'react';
import { Bluetooth, Headphones, Laptop, LoaderCircle, RefreshCw, Smartphone, Tablet } from 'lucide-react';
import type { Device } from './bridge';
import './MultipointPanel.css';

export interface PairedDevice {
  address: string; name: string; connected: boolean; status: number; playback: boolean; classOfDevice?: number; thisComputer?: boolean;
}
export interface MultipointList { devices: PairedDevice[]; maxConnections?: number }
const ADDRESS = /^[0-9a-f]{2}([:-][0-9a-f]{2}){5}$/i;

/** The headphone's paired-device list, as the desktop stored it. Anything malformed is ignored. */
export function readMultipoint(device: Device): MultipointList | null {
  const raw = device.settings.multipointDevices;
  if (typeof raw !== 'string') return null;
  try {
    const parsed = JSON.parse(raw) as { devices?: unknown; maxConnections?: unknown };
    if (!Array.isArray(parsed.devices)) return null;
    const devices = parsed.devices.filter((entry): entry is PairedDevice => !!entry && typeof entry === 'object'
      && typeof (entry as PairedDevice).address === 'string' && ADDRESS.test((entry as PairedDevice).address)
      && typeof (entry as PairedDevice).name === 'string' && typeof (entry as PairedDevice).connected === 'boolean');
    if (devices.length !== parsed.devices.length) return null;
    const max = typeof parsed.maxConnections === 'number' && Number.isInteger(parsed.maxConnections) && parsed.maxConnections > 0 ? parsed.maxConnections : undefined;
    return { devices, maxConnections: max };
  } catch { return null; }
}

export function isMultipointRequest(value: unknown): value is { address: string; connect: boolean } {
  return !!value && typeof value === 'object' && typeof (value as { address: unknown }).address === 'string'
    && ADDRESS.test((value as { address: string }).address) && typeof (value as { connect: unknown }).connect === 'boolean';
}

const sameAddress = (a: string, b: string) => a.replace(/[^0-9a-f]/gi, '').toLowerCase() === b.replace(/[^0-9a-f]/gi, '').toLowerCase();
const isComputer = (device: PairedDevice) => device.classOfDevice !== undefined && ((device.classOfDevice >> 8) & 0x1f) === 1;

export type ConnectPlan = { kind: 'connect' } | { kind: 'swap'; victim: PairedDevice } | { kind: 'blocked' };

/** Mirrors the desktop's rule: at the connection limit, the connected device that is not playing audio,
 *  not this computer and not the target is disconnected first. Other computers are left alone while
 *  this computer cannot be identified. */
export function planConnect(list: MultipointList, target: PairedDevice): ConnectPlan {
  const { devices, maxConnections } = list;
  if (!maxConnections || devices.filter(entry => entry.connected).length < maxConnections) return { kind: 'connect' };
  const knowsSelf = devices.some(entry => entry.thisComputer);
  const victim = devices.find(entry => entry.connected && !entry.playback && !entry.thisComputer && !sameAddress(entry.address, target.address) && (knowsSelf || !isComputer(entry)));
  return victim ? { kind: 'swap', victim } : { kind: 'blocked' };
}

function iconFor(device: PairedDevice) {
  // Bluetooth class of device: major class in bits 8-12 (1 computer, 2 phone, 4 audio/video).
  const major = device.classOfDevice === undefined ? -1 : (device.classOfDevice >> 8) & 0x1f;
  if (major === 1) return Laptop;
  if (major === 2) return Smartphone;
  if (major === 4) return Headphones;
  return /ipad|tablet|tab\b/i.test(device.name) ? Tablet : Bluetooth;
}

interface Props {
  device: Device;
  busy: boolean;
  pendingAddress: string | null;
  refreshing: boolean;
  onChange: (key: string, value: unknown) => void;
}

export default function MultipointPanel({ device, busy, pendingAddress, refreshing, onChange }: Props) {
  const list = useMemo(() => readMultipoint(device), [device]);
  const live = device.connection === 'connected';
  if (!list) return null;
  const { devices, maxConnections } = list;
  const connectedCount = devices.filter(entry => entry.connected).length;
  return <section className="settings-surface multipoint reveal" aria-label="Multipoint connections">
    <div className="section-title"><h2>Paired devices</h2><button type="button" className="text-button mp-refresh" disabled={!live || busy} onClick={() => onChange('multipointRefresh', true)}>{refreshing ? <LoaderCircle size={14} className="mp-spin"/> : <RefreshCw size={14}/>}Refresh</button></div>
    <p className="body-copy mp-lead">Switch a device on to connect it to this headphone, or off to disconnect it. {maxConnections ? `This headphone keeps up to ${maxConnections} connected at once. If it is full, connecting another device first disconnects one that is not playing audio, never this computer.` : 'How many can be connected at once is decided by the headphone.'} A device only counts as connected after the headphone reports it.</p>
    <p className="mp-count" role="status">{devices.length === 0 ? 'No paired devices reported.' : `${connectedCount} of ${devices.length} connected${maxConnections ? ` · up to ${maxConnections} at once` : ''}`}</p>
    {devices.length > 0 && <ul className="mp-list" aria-label="Paired devices">
      {devices.map(entry => {
        const Icon = iconFor(entry);
        const pending = pendingAddress?.toLowerCase() === entry.address.toLowerCase();
        const plan = !entry.connected ? planConnect(list, entry) : null;
        const hint = pending && plan?.kind === 'swap' ? `Disconnecting ${plan.victim.name}, then connecting…`
          : plan?.kind === 'swap' ? `Connecting will disconnect ${plan.victim.name} (not playing audio).`
          : plan?.kind === 'blocked' ? `Full: every connected device is playing audio or is this computer.` : '';
        return <li key={entry.address} className={`mp-device${entry.connected ? ' is-connected' : ''}${pending ? ' is-pending' : ''}`}>
          <span className="mp-icon" aria-hidden="true"><Icon size={20}/></span>
          <span className="mp-main"><strong>{entry.name}{entry.thisComputer && <em className="mp-tag">This computer</em>}</strong><small>{entry.address.toUpperCase()}</small>{hint && <small className={`mp-hint${plan?.kind === 'blocked' ? ' is-blocked' : ''}`}>{hint}</small>}</span>
          <span className={`mp-state${entry.playback ? ' is-playing' : ''}`}>{pending ? 'Waiting for the headphone…' : entry.playback ? 'Connected · playing audio' : entry.connected ? 'Connected' : 'Not connected'}</span>
          <button type="button" role="switch" className="mp-switch" aria-checked={entry.connected} aria-label={`Connection to ${entry.name}`}
            disabled={!live || busy || plan?.kind === 'blocked'} onClick={() => onChange('multipointDevice', { address: entry.address, connect: !entry.connected })}><span/></button>
        </li>;
      })}
    </ul>}
    {!live && <p className="unavailable">Connect this headphone to change which devices are connected.</p>}
  </section>;
}
