import { ArrowRight, Bluetooth, Headphones, Waves, Zap } from 'lucide-react';
import type { Device } from './bridge';
import { batteryReadout } from './native-state';

interface Props {
  device: Device;
  preview: boolean;
  activeOutput: boolean;
  listening: string;
  noise: string;
  pending: boolean;
  busy: boolean;
  onConnection: () => void;
}

export default function DeviceSummary({ device, preview, activeOutput, listening, noise, pending, busy, onConnection }: Props) {
  const connected = device.connection === 'connected';
  const battery = !preview && typeof device.battery === 'number' && Number.isFinite(device.battery)
    && device.battery >= 0 && device.battery <= 100 ? Math.round(device.battery) : null;
  const status = preview ? 'Studio preview' : activeOutput && connected ? 'Active audio output' : connected ? 'Control connected' : device.connection === 'connecting' ? 'Connecting' : 'Disconnected';
  const charging = device.settings.batteryCharging;
  const isCharging = charging === 'charging' || charging === 1;
  const chargeStatus = isCharging ? 'Charging' : charging === 'charged' || charging === 3 ? 'Fully charged'
    : charging === 'notCharging' || charging === 0 ? 'Not charging' : 'Charge status unknown';
  const low = battery !== null && battery < 20;
  const readAt = device.settings.batteryReadAtMs;
  const readDate = !preview && battery !== null && typeof readAt === 'number' && Number.isFinite(readAt)
    && readAt > 0 && readAt <= Date.now() + 60_000 ? new Date(readAt) : null;
  return <div className="hero-copy">
    <div className="device-kicker"><span className={`status-dot ${preview || connected ? 'online' : ''}`}/>{status}</div>
    <div className="device-identity">
      <h2>{device.name.replace('WH-', '')}</h2>
      <div className={`device-battery${low ? ' is-low' : ''}${isCharging && battery !== null ? ' is-charging' : ''}`} title={preview ? 'Battery is unavailable in preview' : batteryReadout(device)}>
        <span className="battery-glyph" aria-hidden="true"><i style={{ width: `${battery ?? 0}%` }}/>{isCharging && battery !== null && <Zap size={11} strokeWidth={2.5}/>}</span>
        <strong>{battery === null ? '—' : `${battery}%`}</strong>
        <small className="battery-charge-status">{battery === null ? 'Not reported' : chargeStatus}</small>
        {low && <small className="battery-warning">Low battery · below 20%</small>}
        {battery !== null && <small className="battery-freshness">{readDate ? <time dateTime={readDate.toISOString()} title={`Battery reply received ${readDate.toLocaleString()}`}>Last read {readDate.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}</time> : 'Update time unavailable'}</small>}
      </div>
    </div>
    <div className="device-link-details"><span><Bluetooth size={14}/>{preview ? 'Bluetooth preview' : connected ? 'Bluetooth control' : 'Bluetooth'}</span><span className="device-codec" title="The current audio codec is not reported by this app.">Codec <b>Not reported</b></span></div>
    <dl className="hero-session-summary">
      <div><dt><Headphones size={13}/>Listening mode</dt><dd>{listening}</dd></div>
      <div><dt><Waves size={13}/>Noise control</dt><dd>{noise}</dd></div>
    </dl>
    <div className="device-summary-footer">
      {!preview && <button className={connected ? 'text-button device-disconnect' : 'button small'} disabled={busy} onClick={onConnection}>{connected ? 'Disconnect' : 'Connect headphone'}{!connected && <ArrowRight size={14}/>}</button>}
      <span className={pending ? 'is-pending' : ''}>{pending ? 'Confirming change…' : preview ? 'Interactive preview · local changes' : connected ? 'Device readback' : 'Connect to read device settings'}</span>
    </div>
  </div>;
}
