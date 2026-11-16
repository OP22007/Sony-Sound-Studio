import { AudioLines, Gamepad2, Headphones } from 'lucide-react';
import type { DeviceControlDescriptor } from './bridge';

interface Props {
  spatialDescriptor?: DeviceControlDescriptor;
  gameDescriptor?: DeviceControlDescriptor;
  gameSoundDescriptor?: DeviceControlDescriptor;
  spatialValue: unknown;
  gameValue: unknown;
  gameSoundValue: unknown;
  busy: boolean;
  pending: boolean;
  onChange: (key: string, value: unknown) => void;
}

export default function OnePlusListeningModes({ spatialDescriptor, gameDescriptor, gameSoundDescriptor, spatialValue, gameValue, gameSoundValue, busy, pending, onChange }: Props) {
  const options = spatialDescriptor?.kind === 'select'
    ? (spatialDescriptor.options ?? []).filter(option => [0, 1, 2].includes(option.value as number) && typeof option.value === 'number')
    : [];
  const current = typeof spatialValue === 'number' && options.some(option => option.value === spatialValue) ? spatialValue : null;
  const gameConfirmed = gameDescriptor?.kind === 'toggle' && typeof gameValue === 'boolean';
  const profileOptions = gameSoundDescriptor?.kind === 'select' ? (gameSoundDescriptor.options ?? []).filter(option => typeof option.value === 'number') : [];
  const profileCurrent = typeof gameSoundValue === 'number' && profileOptions.some(option => option.value === gameSoundValue) ? String(gameSoundValue) : '';

  return <section className="listening-studio reveal oneplus-listening" aria-label="OnePlus listening controls">
    <div className="listening-heading"><span className="eyebrow">LISTEN YOUR WAY</span><h2>Listening controls</h2>
      <p>{pending ? 'Waiting for headphone confirmation…' : options.length || gameConfirmed || profileOptions.length ? 'Options reported by your headphone.' : 'Listening modes have not been confirmed by this headphone.'}</p>
    </div>
    <div className="oneplus-listening-controls">
      <div className="oneplus-spatial-group" role="group" aria-label="Spatial audio">
        <span className="oneplus-control-label"><AudioLines size={15}/> Spatial audio</span>
        {options.length > 0 ? <div className="oneplus-spatial-options">{options.map(option => {
          const Icon = option.value === 0 ? Headphones : AudioLines;
          return <button key={String(option.value)} type="button" data-mode={`spatial-${option.value}`} className={`listening-choice${current === option.value ? ' selected' : ''}`} aria-pressed={current === option.value} disabled={busy || pending || current === null || option.enabled === false} onClick={() => onChange('spatialAudio', option.value)}>
            <Icon size={18} strokeWidth={1.5}/><span><strong>{option.label}</strong><small>{option.value === 0 ? 'Standard listening' : option.value === 1 ? 'Spatial effect fixed in place' : 'Spatial effect follows head movement'}</small></span><i/>
          </button>;
        })}</div> : <span className="status-label">Not confirmed</span>}
      </div>
      <div className="oneplus-game-control">
        <div><span className="oneplus-control-label"><Gamepad2 size={15}/> Game mode</span><small>Low-latency control</small></div>
        {gameConfirmed ? <button type="button" role="switch" aria-label="Game mode · low latency" aria-checked={gameValue} className="toggle" disabled={busy || pending} onClick={() => onChange('gameMode', !gameValue)}><span/></button> : <span className="status-label">Not confirmed</span>}
      </div>
      <div className="oneplus-profile-control">
        <label htmlFor="oneplus-game-sound-profile">Game sound profile</label>
        {profileOptions.length > 0 && profileCurrent !== '' ? <div className="oneplus-profile-select-wrap">
          <select id="oneplus-game-sound-profile" className="device-control-select" aria-label="Game sound profile" value={profileCurrent} disabled={busy || pending} onChange={event => {
            const option = profileOptions.find(item => String(item.value) === event.target.value);
            if (option && option.enabled !== false) onChange('gameSoundProfile', option.value);
          }}>
            {profileOptions.map((option, index) => <option key={`${String(option.value)}-${index}`} value={String(option.value)} disabled={option.enabled === false}>{option.label}</option>)}
          </select>
          <small>{pending ? 'Waiting for headphone confirmation…' : 'Confirmed by headphone'}</small>
        </div> : <span className="status-label">Not confirmed</span>}
      </div>
      {pending && <small className="oneplus-listening-pending" role="status">Waiting for headphone confirmation…</small>}
    </div>
  </section>;
}
