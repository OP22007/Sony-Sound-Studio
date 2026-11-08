import { useEffect, useState } from 'react';
import { Check, ChevronDown, Info, Pencil, Plus, RotateCcw, Trash2, X } from 'lucide-react';
import type { Device } from './bridge';
import { isOnePlusDevice } from './oneplus';
import OnePlusEqualizer from './OnePlusEqualizer';
import EqGraph from './EqGraph';
import type { PreviewSettings } from './preview';
import { presetNameError, sameBands, sonyPresets, toDeviceSteps, xm6Frequencies } from './eq-presets';
import type { DevicePreset, SavedPreset } from './eq-presets';
import './equalizer.css';

interface Props {
  preview: boolean; available: boolean; busy: boolean; settings: PreviewSettings; device: Device;
  onChange: (key: string, value: unknown) => void;
  presets: SavedPreset[]; devicePresets: DevicePreset[];
  onReplace: (presets: SavedPreset[]) => boolean;
  onReplaceDevice: (presets: DevicePreset[]) => boolean;
}

export default function Equalizer({ preview, available, busy, settings, device, onChange, presets, devicePresets, onReplace, onReplaceDevice }: Props) {
  const [preferred, setPreferred] = useState<string | null>(preview ? settings.preset : null);
  const [editing, setEditing] = useState<string | null>(null);
  const [form, setForm] = useState<{ kind: 'create' | 'rename'; original?: string } | null>(null);
  const [name, setName] = useState('');
  const [validation, setValidation] = useState('');
  const [draft, setDraft] = useState<number[] | null>(null);
  useEffect(() => {
    if (!busy && draft && Array.isArray(device.settings.equalizer) && sameBands(draft, device.settings.equalizer)) setDraft(null);
  }, [busy, draft, device.settings.equalizer]);
  const onePlus = isOnePlusDevice(device, preview);
  if (onePlus) return <OnePlusEqualizer device={device} busy={busy} onChange={onChange}/>;
  const realBands = device.settings.equalizer;
  const usable = available && (preview || (Array.isArray(realBands) && realBands.length === 10 && realBands.every(v => Number.isInteger(v) && v >= 0 && v <= 12)));
  const values: number[] = preview ? settings.equalizer : usable ? draft ?? realBands as number[] : [];
  const unapplied = !preview && usable && !!draft && !sameBands(draft, realBands as number[]);
  const mine = preview ? presets : devicePresets.filter(p => p.model === device.model && p.firmware === device.firmware);
  const factory = device.model === 'WH-1000XM6' ? sonyPresets.map(p => ({ ...p, bands: preview ? [...p.bands] : toDeviceSteps(p.bands) })) : [];
  const choices = [...factory.map(p => ({ ...p, id: `sony:${p.name}` })), ...mine.map(p => ({ ...p, id: `local:${p.name}` }))];
  const preferredPreset = choices.find(p => (p.id === preferred || p.name === preferred) && sameBands(p.bands, values));
  const active = preferred === 'Custom' ? undefined : preferredPreset ?? choices.find(p => sameBands(p.bands, values));
  const selected = active?.id ?? 'Custom';
  const target = mine.find(p => p.name === editing);
  const dirty = !!target && !sameBands(target.bands, values);
  const processingSuspended = !preview && ['music','cinema','game'].includes(String(device.settings.listeningMode));
  const disabled = !usable || busy || processingSuspended;
  // Old illustrative profiles can contain ±10; keep them editable without clipping.
  const range = preview ? values.some(value => Math.abs(value) > 6) ? 10 : 6 : 12;
  const minimum = preview ? -range : 0;
  const maximum = preview ? range : 12;

  function apply(value: string) {
    if (value === 'Custom' && !disabled) {
      setPreferred('Custom'); setEditing(null);
      if (preview) onChange('preset', 'Custom');
      return;
    }
    const preset = choices.find(p => p.id === value);
    if (!preset || disabled) return;
    setDraft(null);
    setPreferred(value);
    setEditing(value.startsWith('local:') ? preset.name : null);
    onChange('equalizer', [...preset.bands]);
    if (preview) onChange('preset', value);
  }
  function persist(next: SavedPreset[]): boolean {
    const ok = preview ? onReplace(next) : onReplaceDevice([
      ...devicePresets.filter(p => p.model !== device.model || p.firmware !== device.firmware),
      ...next.map(p => ({ ...p, model: device.model, firmware: device.firmware ?? '' })),
    ]);
    if (!ok) setValidation('The preset could not be saved. Check available local storage and try again.');
    return ok;
  }
  function openForm(kind: 'create' | 'rename', original?: string) {
    setForm({ kind, original }); setName(original ?? ''); setValidation('');
  }
  function remove(preset: SavedPreset) {
    if (!persist(mine.filter(p => p.name !== preset.name))) return;
    if (preferred === `local:${preset.name}` || preferred === preset.name) setPreferred(null);
    if (editing === preset.name) setEditing(null);
    if (form?.original === preset.name) setForm(null);
    if (preview && [preset.name, `local:${preset.name}`].includes(settings.preset)) onChange('preset', 'Custom');
  }
  const stateLabel = preview ? 'LOCAL PREVIEW' : !usable ? 'AWAITING READBACK' : busy ? 'CONFIRMING WITH DEVICE' : unapplied ? 'UNAPPLIED DRAFT' : 'CONFIRMED ON DEVICE';
  return <section className="sound-panel eq-panel eq-workspace-panel reveal" aria-label="Equalizer workspace">
    <div className="eq-workspace-heading"><div><span className="eq-kicker">GRAPHIC EQUALIZER <span>／</span> {preview ? 'STUDIO PREVIEW' : device.model}</span><h2>Shape the detail.</h2><p>Ten independent bands. Every point maps to a precise, device-supported step.</p></div><div className={`eq-device-status${unapplied || busy ? ' is-pending' : ''}`}><i aria-hidden="true"/><span>{stateLabel}</span></div></div>
    <div className="eq-editor-toolbar">
      <div className="eq-profile-picker"><span className="eq-control-label">ACTIVE CURVE</span><div className="preset-select"><select aria-label={preview ? 'Equalizer preset' : 'Device equalizer profile'} value={selected} disabled={disabled} onChange={e => apply(e.target.value)}>
        <option value="Custom">Manual</option>
        {factory.length > 0 && <optgroup label="Sony presets">{factory.map(p => <option key={p.name} value={`sony:${p.name}`}>{p.name}</option>)}</optgroup>}
        {mine.length > 0 && <optgroup label="My presets">{mine.map(p => <option key={p.name} value={`local:${p.name}`}>{p.name}</option>)}</optgroup>}
      </select><ChevronDown size={14}/></div></div>
      <div className="eq-actions"><button className="eq-reset-button" aria-label="Reset equalizer" title="Reset equalizer" disabled={disabled || !factory.length} onClick={() => apply('sony:Off')}><RotateCcw size={15}/><span>Reset</span></button><button className="button small eq-save-button" disabled={disabled || mine.length >= 24 || (!preview && !device.firmware)} onClick={() => openForm('create')}><Plus size={15}/>Save preset</button></div>
    </div>
    <div className={`eq-editor-surface ${!usable ? 'unavailable-eq' : ''}`}>
      <div className="eq-plot-heading"><div><span className="eq-control-label">RESPONSE CURVE</span><span className="eq-plot-help">Drag a point · Arrow keys adjust by one step</span></div><div className="eq-range-mark"><span>{preview ? `±${range} dB` : '0—12 steps'}</span><small>{preview ? 'PREVIEW RANGE' : 'DEVICE RANGE'}</small></div></div>
      <div className="equalizer eq-plot-scroll"><div className="eq-scroll"><EqGraph values={values} labels={xm6Frequencies.map(frequency => preview ? frequency : `${frequency} device equalizer step`)} min={minimum} max={maximum} step={1} disabled={disabled} description={usable ? preview ? 'Illustrative equalizer response in decibels' : 'Equalizer device step profile' : 'Equalizer awaiting device data'} formatValue={value => preview ? `${value} decibels` : `${value} device steps`} onChange={(index, value) => { if (!editing && active?.id.startsWith('local:')) setEditing(active.name); const next = [...values]; next[index] = value; if (preview) { onChange('equalizer', next); onChange('preset', 'Custom'); } else { setDraft(next); setPreferred('Custom'); } }}/>
        <div className="band-inputs">{xm6Frequencies.map((frequency, i) => <label key={frequency} className="band"><output>{usable ? preview ? `${values[i] > 0 ? '+' : ''}${values[i]}` : values[i] : '—'}</output><span>{frequency.replace(' Hz', '').replace(' kHz', 'k')}</span></label>)}</div>
        <div className="eq-frequency-legend"><span><b>LOW</b>Weight & warmth</span><span><b>MID</b>Voice & presence</span><span><b>HIGH</b>Air & detail</span></div>
      </div></div>
      <div className="eq-graph-footer"><span>{preview ? 'Move freely; changes stay in this preview.' : unapplied ? 'This curve is local until you apply it.' : 'Device values update only after a confirmed reply.'}</span><span>{preview ? '−' + range + ' dB' : '0'} <i/><span>{preview ? '+' + range + ' dB' : '12 steps'}</span></span></div>
    </div>
    <div className="eq-confirmation-line"><span>{preview ? 'Studio preview · changes stay local' : unapplied ? 'Draft · waiting for explicit apply' : 'Device values shown after confirmed readback'}</span><span>{preview ? `±${range} dB` : '0–12 steps'}</span></div>
    {unapplied && <div className="eq-draft-actions" role="status"><span>{busy ? 'Waiting for headphone confirmation…' : 'Your curve is ready. Apply it to hear the change.'}</span><div><button className="text-button" disabled={disabled} onClick={() => { setDraft(null); setPreferred(null); }}>Discard changes</button><button className="button small" disabled={disabled} onClick={() => onChange('equalizer', [...values])}>Apply equalizer <Check size={14}/></button></div></div>}
    {!usable && <p className="unavailable"><Info size={14}/>Equalizer controls are unavailable because this connected device has not confirmed the equalizer capability and current band steps.</p>}
    {factory.length > 0 ? <div className="eq-preset-section"><div className="eq-library-heading eq-preset-heading"><div><span className="eq-control-label">CURVE LIBRARY</span><p>Start with a voicing, then shape it to your taste.</p></div><span>{factory.length} SONY CURVES</span></div><div className="sony-eq-library" role="group" aria-label="Sony equalizer presets">{factory.map(p => <button className="sony-eq-preset" key={p.name} disabled={disabled} aria-pressed={selected === `sony:${p.name}`} onClick={() => apply(`sony:${p.name}`)}><svg viewBox="0 0 120 60" aria-hidden="true"><line x1="4" y1="30" x2="116" y2="30"/><polyline points={p.bands.map((v, i) => `${6 + i * 12},${52 - (v - (preview ? -range : 0)) * 40 / (preview ? range * 2 : 12)}`).join(' ')}/></svg><span>{p.name}</span><small>{p.description}</small></button>)}</div><p className="eq-library-note">{preview ? 'Changes stay in this browser preview.' : 'Applying a curve sends one custom equalizer update; confirmation comes from the headphones.'}</p></div> : <div className="eq-preset-section eq-model-note"><div className="eq-library-heading eq-preset-heading"><div><span className="eq-control-label">CURVE LIBRARY</span><p>Custom sound, saved on this computer.</p></div></div><p className="eq-library-note">Sony factory curves for this model are not validated yet. Custom studio profiles remain available in preview.</p></div>}
    {target && dirty && <div className="eq-unsaved"><span>Unsaved changes to “{target.name}”</span><button className="text-button" disabled={disabled} onClick={() => { if (persist(mine.map(p => p.name === target.name ? { ...p, bands: [...values] } : p))) { setPreferred(`local:${target.name}`); if (preview) onChange('preset', `local:${target.name}`); } }}><Check size={13}/>Save changes</button></div>}
    {mine.length > 0 && <div className="eq-my-presets"><div className="eq-library-heading"><span className="eyebrow">MY PRESETS</span><span>{mine.length} / 24</span></div>{mine.map(p => <div className="eq-saved-row" key={p.name}><button className="text-button eq-saved-name" disabled={disabled} onClick={() => apply(`local:${p.name}`)} aria-label={`Apply “${p.name}”`} aria-pressed={selected === `local:${p.name}`}>{p.name}{selected === `local:${p.name}` && <Check size={13}/>}</button><button className="icon-button" disabled={busy} onClick={() => openForm('rename', p.name)} aria-label={`Rename “${p.name}”`} title="Rename preset"><Pencil size={13}/></button><button className="icon-button" disabled={busy} onClick={() => remove(p)} aria-label={`Delete “${p.name}”`} title="Delete preset"><Trash2 size={13}/></button></div>)}</div>}
    {form && <form className="save-preset" aria-label={form.kind === 'rename' ? 'Rename preset' : 'Save preset'} onSubmit={e => {
      e.preventDefault();
      if (busy || (form.kind === 'create' && !usable)) return;
      const clean = name.trim();
      const error = presetNameError(clean, mine, form.original);
      if (error) { setValidation(error); return; }
      if (form.kind === 'create' && mine.length >= 24) { setValidation('You can save up to 24 presets. Remove one before creating another.'); return; }
      const original = mine.find(p => p.name === form.original);
      if (form.kind === 'rename' && !original) { setValidation('This preset is no longer available.'); return; }
      const next = form.kind === 'rename' ? mine.map(p => p.name === form.original ? { ...p, name: clean } : p) : [...mine, { name: clean, bands: [...values] }];
      if (!persist(next)) return;
      if (form.kind === 'create' || preferred === `local:${form.original}` || selected === `local:${form.original}`) { setPreferred(`local:${clean}`); if (preview) onChange('preset', `local:${clean}`); }
      if (form.kind === 'create' || editing === form.original) setEditing(clean);
      setForm(null); setValidation('');
    }}><label htmlFor="preset-name">{form.kind === 'rename' ? 'New preset name' : 'Preset name'}</label><div><input id="preset-name" autoFocus maxLength={40} value={name} onChange={e => { setName(e.target.value); setValidation(''); }} placeholder="e.g. Late night"/><button className="button small" type="submit" disabled={busy}>{form.kind === 'rename' ? 'Rename preset' : 'Save preset'}</button><button className="icon-button" type="button" onClick={() => { setForm(null); setValidation(''); }} aria-label="Cancel preset editing"><X size={16}/></button></div>{validation && <p role="alert">{validation}</p>}</form>}
    {!form && validation && <p className="preference-error" role="alert">{validation}</p>}
  </section>;
}
