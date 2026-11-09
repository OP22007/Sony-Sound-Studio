import { useState } from 'react';
import { Waves } from 'lucide-react';
import type { Device, DeviceControlDescriptor } from './bridge';
import { isOnePlusDevice } from './oneplus';
import './bass-control.css';

interface Props { device: Device; busy: boolean; onChange: (key: string, value: unknown) => void }

export default function BassControl({ device, busy, onChange }: Props) {
  const key = isOnePlusDevice(device) ? 'bassWave' : 'clearBass';
  const raw = device.settings.controlDescriptors;
  const current = device.settings[key];
  if (!Array.isArray(raw) || !device.capabilities.includes(key) || typeof current !== 'number' || !Number.isSafeInteger(current)) return null;
  const descriptor = raw.find((item: unknown): item is DeviceControlDescriptor => {
    if (!item || typeof item !== 'object') return false;
    const d = item as Partial<DeviceControlDescriptor>;
    return d.key === key && d.kind === 'range' && typeof d.label === 'string'
      && typeof d.min === 'number' && Number.isSafeInteger(d.min)
      && typeof d.max === 'number' && Number.isSafeInteger(d.max) && d.max > d.min
      && typeof d.step === 'number' && Number.isSafeInteger(d.step) && d.step > 0
      && current >= d.min && current <= d.max && (current - d.min) % d.step === 0;
  });
  if (!descriptor) return null;
  const toggle = raw.find((item: unknown): item is DeviceControlDescriptor => !!item && typeof item === 'object'
    && (item as DeviceControlDescriptor).key === 'bassWaveEnabled'
    && (item as DeviceControlDescriptor).kind === 'toggle');
  const enabled = toggle && device.capabilities.includes('bassWaveEnabled') && typeof device.settings.bassWaveEnabled === 'boolean'
    ? device.settings.bassWaveEnabled : undefined;
  return <BassSlider key={`${device.id}:${current}:${descriptor.min}:${descriptor.max}:${descriptor.step}:${enabled}`} device={device} descriptor={descriptor} current={current} enabled={enabled} busy={busy} onChange={onChange}/>;
}

function BassSlider({ device, descriptor, current, enabled, busy, onChange }: Props & { descriptor: DeviceControlDescriptor; current: number; enabled?: boolean }) {
  const [draft, setDraft] = useState(current);
  const min = descriptor.min!; const max = descriptor.max!; const step = descriptor.step!;
  const disabled = busy || device.connection !== 'connected';
  const dirty = draft !== current;
  const display = (value: number) => value > 0 ? `+${value}` : String(value);
  const progress = (draft - min) / (max - min) * 100;
  return <section className="sound-panel bass-panel reveal" aria-label={`${descriptor.label} control`}>
    <div className="section-title"><div><span className="eyebrow">LOW-END CHARACTER</span><h2>{descriptor.label}</h2></div><Waves size={22} strokeWidth={1.4}/></div>
    <p className="bass-description">{descriptor.key === 'bassWave' ? 'Shape the weight of your bass with the earbuds’ BassWave control.' : 'Fine-tune the bass independently while keeping your equalizer bands intact.'}</p>
    {typeof enabled === 'boolean' && <div className="bass-toggle-row"><span>BassWave</span><button className={`toggle${enabled ? ' on' : ''}`} role="switch" aria-label="Enable BassWave" aria-checked={enabled} disabled={disabled} onClick={() => onChange('bassWaveEnabled', !enabled)}><span/></button></div>}
    <div className="bass-level"><label htmlFor={`bass-${descriptor.key}`}>Bass level</label><output htmlFor={`bass-${descriptor.key}`}>{display(draft)}<small>LEVEL</small></output></div>
    <input className="bass-slider" id={`bass-${descriptor.key}`} type="range" aria-label={`${descriptor.label} level`} aria-valuetext={`Level ${display(draft)}`} min={min} max={max} step={step} value={draft} disabled={disabled || enabled === false} style={{ background: `linear-gradient(to right, var(--accent) ${progress}%, var(--line-strong) ${progress}%)` }} onChange={event => setDraft(Number(event.target.value))}/>
    <div className="bass-scale"><span>{display(min)}</span>{min < 0 && max > 0 && <span style={{ left: `${-min / (max - min) * 100}%` }}>0</span>}<span>{display(max)}</span></div>
    <div className="bass-footer"><span role="status">{busy ? 'Waiting for device confirmation…' : enabled === false ? 'BassWave is off · saved level retained' : dirty ? `Unsaved · device level ${display(current)}` : `Confirmed device level ${display(current)}`}</span><div><button className="text-button" disabled={disabled || !dirty} onClick={() => setDraft(current)}>Discard</button><button className="button small" disabled={disabled || enabled === false || !dirty} onClick={() => onChange(descriptor.key, draft)}>Apply bass</button></div></div>
  </section>;
}
