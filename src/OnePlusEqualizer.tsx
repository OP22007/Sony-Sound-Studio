import { useEffect, useState } from 'react';
import { ChevronDown, Info, Plus } from 'lucide-react';
import type { Device, DeviceControlDescriptor } from './bridge';
import { frequencyLabel, isOnePlusEqProfile, onePlusProfiles } from './oneplus-eq';
import type { OnePlusEqProfile } from './oneplus-eq';
import EqGraph from './EqGraph';

interface Props { device: Device; busy: boolean; onChange: (key: string, value: unknown) => void }

export default function OnePlusEqualizer({ device, busy, onChange }: Props) {
  const [editingId, setEditingId] = useState<number | null>(null);
  const [creating, setCreating] = useState(false);
  const [submitted, setSubmitted] = useState<{ profile: OnePlusEqProfile; previousIds: number[] } | null>(null);
  const raw: unknown = device.settings.controlDescriptors;
  const descriptor = Array.isArray(raw) ? raw.find((item: unknown): item is DeviceControlDescriptor => {
    if (!item || typeof item !== 'object') return false;
    const d = item as Partial<DeviceControlDescriptor>;
    return d.key === 'eqPreset' && d.kind === 'select' && device.capabilities.includes('eqPreset')
      && Array.isArray(d.options) && d.options.length > 0
      && d.options.every(option => option && typeof option.label === 'string' && typeof option.value === 'number');
  }) : undefined;
  const current = descriptor?.options?.find(option => option.value === device.settings.eqPreset);
  const profiles = onePlusProfiles(device.settings.oneplusEqualizerProfiles);
  const rawTemplate = device.settings.oneplusEqTemplate;
  const template = isOnePlusEqProfile(rawTemplate) ? rawTemplate : null;
  const active = profiles.find(profile => profile.selected === 1);
  const editing = profiles.find(profile => profile.eqId === editingId) ?? active ?? profiles[0];
  const connected = device.connection === 'connected';
  const writable = connected && device.capabilities.includes('oneplusEqProfile') && device.settings.oneplusEqProfile === true;
  useEffect(() => {
    if (!submitted) return;
    const saved = profiles.find(profile => !submitted.previousIds.includes(profile.eqId)
      && profile.name === submitted.profile.name && profile.selected === 1
      && JSON.stringify(profile.frequencies) === JSON.stringify(submitted.profile.frequencies)
      && JSON.stringify(profile.dbValues) === JSON.stringify(submitted.profile.dbValues));
    if (saved) { setEditingId(saved.eqId); setCreating(false); setSubmitted(null); }
  }, [profiles, submitted]);

  return <section className="sound-panel eq-panel eq-workspace-panel reveal" aria-label="Equalizer workspace">
    <div className="eq-workspace-heading"><div><span className="eq-kicker">GRAPHIC EQUALIZER <span>／</span> ONEPLUS</span><h2>Shape the detail.</h2><p>Earbud-reported profiles, frequencies and tuning range.</p></div><div className={`eq-device-status${busy ? ' is-pending' : ''}`}><i aria-hidden="true"/><span>{busy ? 'CONFIRMING WITH DEVICE' : writable ? 'DEVICE PROFILE DATA' : connected ? 'READ ONLY' : 'DISCONNECTED'}</span></div></div>
    {descriptor && current ? <div className="eq-editor-toolbar"><div className="eq-profile-picker"><span className="eq-control-label">SOUND PROFILE</span><div className="preset-select"><select id="oneplus-eq-preset" aria-label="Sound profile" value={String(current.value)} disabled={busy || !connected} onChange={event => {
      const option = descriptor.options?.find(item => String(item.value) === event.target.value);
      if (option && option.enabled !== false) onChange('eqPreset', option.value);
    }}>{descriptor.options?.map(option => <option key={String(option.value)} value={String(option.value)} disabled={option.enabled === false}>{option.label}</option>)}</select><ChevronDown size={14} aria-hidden="true"/></div></div></div>
      : <p className="unavailable"><Info size={14}/>{typeof device.settings.oneplusEqStatus === 'string' ? device.settings.oneplusEqStatus : 'No confirmed factory sound-profile reply yet. Reconnect to read this model’s equalizer capabilities.'}</p>}
    {(profiles.length > 0 || template) && <div className="eq-my-presets eq-preset-section">
      <div className="eq-library-heading eq-preset-heading"><div><span className="eq-control-label">SAVED PROFILES</span><p>Custom curves stored on your earbuds.</p></div><span>{profiles.length} {profiles.length === 1 ? 'PROFILE' : 'PROFILES'}</span></div>
      <div className="oneplus-profile-list" role="group" aria-label="Custom equalizer profiles">{profiles.map(profile => <button className="text-button eq-profile-choice" key={profile.eqId} aria-pressed={!creating && editing?.eqId === profile.eqId} disabled={busy} onClick={() => { setEditingId(profile.eqId); setCreating(false); }}><span>{profile.name || `Profile ${profile.eqId}`}</span>{profile.selected === 1 && <small>ACTIVE</small>}</button>)}{template && <button className="text-button eq-profile-choice eq-profile-create" disabled={busy || !writable || profiles.length >= 32} aria-pressed={creating} onClick={() => { setCreating(true); setSubmitted(null); }}><Plus size={15}/>New profile</button>}</div>
      {creating && template ? <ProfileEditor key="new-profile" profile={{ ...template, name: 'My profile' }} creating busy={busy} writable={writable} onApply={profile => { setSubmitted({ profile, previousIds: profiles.map(item => item.eqId) }); onChange('oneplusEqProfile', { action: 1, profile }); }}/>
        : editing && <ProfileEditor key={JSON.stringify(editing)} profile={editing} busy={busy} writable={writable} onApply={profile => onChange('oneplusEqProfile', { action: 2, profile })}/>}
    </div>}
    {!profiles.length && !template && <p className="eq-library-note">Custom bands appear when the earbuds return their saved profiles, frequencies and tuning range.</p>}
  </section>;
}

function ProfileEditor({ profile, busy, writable, creating = false, onApply }: { profile: OnePlusEqProfile; busy: boolean; writable: boolean; creating?: boolean; onApply: (profile: OnePlusEqProfile) => void }) {
  const [name, setName] = useState(profile.name);
  const [bands, setBands] = useState([...profile.dbValues]);
  const clean = name.trim();
  const validName = clean.length > 0 && new TextEncoder().encode(clean).length <= 255;
  const dirty = clean !== profile.name || bands.some((band, index) => band !== profile.dbValues[index]);
  const disabled = busy || !writable;
  const updateBand = (index: number, value: number) => setBands(previous => previous.map((band, i) => i === index ? value : band));
  return <div className="oneplus-profile-editor eq-oneplus-editor-card">
    <div className="eq-profile-editor-heading"><div><span className="eq-control-label">{creating ? 'NEW PROFILE' : profile.selected === 1 ? 'ACTIVE PROFILE' : 'PROFILE TUNING'}</span><p>{creating ? 'Tune the curve, then save it to your earbuds.' : 'Changes stay here until you apply the profile.'}</p></div><span className="eq-range-mark"><b>{profile.minValue} to +{profile.maxValue} dB</b><small>DEVICE-REPORTED RANGE</small></span></div>
    <div className="oneplus-profile-name"><label htmlFor="oneplus-profile-name">Profile name</label><input id="oneplus-profile-name" value={name} maxLength={255} disabled={disabled} onChange={event => setName(event.target.value)}/></div>
    {!validName && <p className="preference-error" role="alert">Use a name of 1–255 UTF-8 bytes.</p>}
    <div className="equalizer eq-oneplus-plot"><div className="eq-scroll" style={bands.length > 10 ? { minWidth: bands.length * 48 } : undefined}>
      <EqGraph values={bands} labels={profile.frequencies.map(frequency => `${frequencyLabel(frequency)} custom equalizer`)} min={profile.minValue} max={profile.maxValue} step={1} disabled={disabled} description="Custom equalizer response in decibels" formatValue={value => `${value} decibels`} onChange={updateBand}/>
      <div className="band-inputs" style={{ gridTemplateColumns: `repeat(${bands.length}, minmax(46px, 1fr))` }}>{profile.frequencies.map((frequency, index) => <label className="band" key={frequency}><output>{bands[index] > 0 ? '+' : ''}{bands[index]} dB</output><span>{frequencyLabel(frequency)}</span></label>)}</div>
      <p className="eq-oneplus-plot-help">Drag any point or focus it and use the arrow keys. Edits remain local until you apply this profile.</p>
    </div></div>
    <div className="eq-footnote"><span>{creating ? 'NEW PROFILE · NOT YET SAVED' : profile.selected === 1 ? 'ACTIVE ON EARBUDS' : 'SAVED ON EARBUDS'}{dirty && !creating ? ' · UNSAVED CHANGES' : ''}</span><span>{profile.minValue} TO +{profile.maxValue} dB</span></div>
    <div className="eq-actions oneplus-apply"><button className="text-button" disabled={disabled || !dirty} onClick={() => { setName(profile.name); setBands([...profile.dbValues]); }}>Discard changes</button><button className="button small" disabled={disabled || !validName} onClick={() => onApply({ ...profile, selected: 1, name: clean, dbValues: [...bands] })}>{busy ? 'Confirming…' : creating ? 'Create & apply profile' : dirty ? 'Save & apply profile' : 'Apply profile'}</button></div>
    {!writable && <p className="eq-library-note">The saved curve is read-only until this device confirms its custom-EQ command.</p>}
  </div>;
}
