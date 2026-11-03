import { Clapperboard, Coffee, Headphones } from 'lucide-react';
import type { DeviceControlDescriptor, SettingPrimitive } from './bridge';

interface Props {
  descriptor?: DeviceControlDescriptor;
  roomDescriptor?: DeviceControlDescriptor;
  mode: string;
  room: unknown;
  preview: boolean;
  busy: boolean;
  pending: boolean;
  onChange: (key: string, value: unknown) => void;
}

const details: Record<string, { icon: typeof Headphones; description: string }> = {
  standard: { icon: Headphones, description: 'Your everyday sound' },
  backgroundMusic: { icon: Coffee, description: 'A little atmosphere' },
  cinema: { icon: Clapperboard, description: 'A wider soundstage' },
};

export default function ListeningModes({ descriptor, roomDescriptor, mode, room, preview, busy, pending, onChange }: Props) {
  const options = preview ? [{ value: 'standard', label: 'Standard' }, { value: 'backgroundMusic', label: 'Background Music' }, { value: 'cinema', label: 'Cinema' }] : descriptor?.options ?? [];
  const rooms = preview ? [{ value: 0, label: 'My room' }, { value: 1, label: 'Living room' }, { value: 2, label: 'Cafe' }] : roomDescriptor?.options ?? [];
  return <section className="listening-studio reveal" aria-label="Listening mode">
    <div className="listening-heading"><span className="eyebrow">LISTEN YOUR WAY</span><h2>Sound mode</h2><p>{pending ? 'Waiting for headphone confirmation…' : preview ? 'Explore the listening experience.' : options.length ? 'Listening modes reported by your headphones.' : 'Connect a supported headphone to confirm its listening modes.'}</p></div>
    <div className="listening-options" role="group" aria-label="Sound modes">{options.map(option => {
      const detail = details[String(option.value)];
      const Icon = detail?.icon ?? Headphones;
      return <button key={String(option.value)} className={`listening-choice${mode === option.value ? ' selected' : ''}`} aria-pressed={mode === option.value} disabled={busy} onClick={() => onChange('listeningMode', option.value)}><Icon size={22} strokeWidth={1.4}/><span><strong>{option.label}</strong><small>{detail?.description ?? 'Headphone listening mode'}</small></span><i/></button>;
    })}{!options.length && <span className="status-label">Listening modes not confirmed</span>}</div>
    {mode === 'backgroundMusic' && rooms.length > 0 && <div className="listening-environment"><span>YOUR SPACE</span><div role="group" aria-label="Background Music environment">{rooms.map(option => <button key={String(option.value)} aria-pressed={room === option.value} disabled={busy} className={room === option.value ? 'selected' : ''} onClick={() => onChange('bgmRoomSize', option.value as SettingPrimitive)}>{option.label}</button>)}</div></div>}
  </section>;
}
