import { useState } from 'react';
import { Check, ChevronDown, Info, Pencil, Plus, RotateCcw, SlidersHorizontal, Trash2, X } from 'lucide-react';
import type { Device } from './bridge';
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
  const realBands = device.settings.equalizer;
  const usable = available && (preview || (Array.isArray(realBands) && realBands.length === 10 && realBands.every(v => Number.isInteger(v) && v >= 0 && v <= 12)));
  const values: number[] = preview ? settings.equalizer : usable ? realBands as number[] : [];
  const mine = preview ? presets : devicePresets.filter(p => p.model === device.model && p.firmware === device.firmware);
  const factory = device.model === 'WH-1000XM6' ? sonyPresets.map(p => ({ ...p, bands: preview ? [...p.bands] : toDeviceSteps(p.bands) })) : [];
  const choices = [...factory.map(p => ({ ...p, id: `sony:${p.name}` })), ...mine.map(p => ({ ...p, id: `local:${p.name}` }))];
  const preferredPreset = choices.find(p => (p.id === preferred || p.name === preferred) && sameBands(p.bands, values));
  const active = preferred === 'Custom' ? undefined : preferredPreset ?? choices.find(p => sameBands(p.bands, values));
  const selected = active?.id ?? 'Custom';
  const target = mine.find(p => p.name === editing);
  const dirty = !!target && !sameBands(target.bands, values);
  const disabled = !usable || busy;
  // Old illustrative profiles can contain ±10; keep them editable without clipping.
  const range = preview ? values.some(value => Math.abs(value) > 6) ? 10 : 6 : 12;
  const points = values.map((v, i) => [36 + i * 72, preview ? 90 - v / range * 50 : 140 - v / 12 * 100] as [number, number]);
  const path = points.map(([x, y], i) => i ? `C${x - 36},${points[i - 1][1]} ${x - 36},${y} ${x},${y}` : `M${x},${y}`).join(' ');

  function apply(value: string) {
    if (value === 'Custom' && !disabled) {
      setPreferred('Custom'); setEditing(null);
      if (preview) onChange('preset', 'Custom');
      return;
    }
    const preset = choices.find(p => p.id === value);
    if (!preset || disabled) return;
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
  return <section className="sound-panel eq-panel reveal" aria-label="Equalizer workspace">
    <div className="section-title"><div><span className="eyebrow">FINELY TUNED TO YOU</span><h2>A signature of your own.</h2></div><SlidersHorizontal size={21} strokeWidth={1.4}/></div>
    <div className="eq-toolbar">
      <div className="preset-select"><span>Equalizer</span><select aria-label={preview ? 'Equalizer preset' : 'Device equalizer profile'} value={selected} disabled={disabled} onChange={e => apply(e.target.value)}>
        <option value="Custom">Manual</option>
        {factory.length > 0 && <optgroup label="Sony presets">{factory.map(p => <option key={p.name} value={`sony:${p.name}`}>{p.name}</option>)}</optgroup>}
        {mine.length > 0 && <optgroup label="My presets">{mine.map(p => <option key={p.name} value={`local:${p.name}`}>{p.name}</option>)}</optgroup>}
      </select><ChevronDown size={12}/></div>
      <div className="eq-actions"><button className="icon-button" aria-label="Reset equalizer" title="Reset equalizer" disabled={disabled || !factory.length} onClick={() => apply('sony:Off')}><RotateCcw size={15}/></button><button className="text-button" disabled={disabled || mine.length >= 24 || (!preview && !device.firmware)} onClick={() => openForm('create')}>Save preset <Plus size={13}/></button></div>
    </div>
    <div className={`equalizer ${!usable ? 'unavailable-eq' : ''}`}><div className="eq-scroll"><svg className="eq-curve" viewBox="0 0 720 175" aria-label={usable ? preview ? 'Illustrative equalizer response in decibels' : 'Equalizer device step profile' : 'Equalizer awaiting device data'} role="img"><defs><linearGradient id="eq-fill" x1="0" y1="0" x2="0" y2="1"><stop stopColor="var(--accent)" stopOpacity=".21"/><stop offset="1" stopColor="var(--accent)" stopOpacity="0"/></linearGradient></defs>{[40, 90, 140].map((y, i) => <g key={y}><line x1="26" y1={y} x2="698" y2={y} className={i === 1 ? 'eq-zero' : 'eq-grid'}/><text x="2" y={y + 4}>{preview ? [`+${range}`, '0', `−${range}`][i] : ['12', '6', '0'][i]}</text></g>)}{points.length > 1 && <><path d={`${path} L684,165 L36,165 Z`} fill="url(#eq-fill)"/><path className="frequency-path" d={path}/>{points.map(([x, y], i) => <g key={i}><circle cx={x} cy={y} r="7" className="eq-point-halo"/><circle cx={x} cy={y} r="3" className="eq-point"/></g>)}</>}</svg>
      <div className="band-inputs">{xm6Frequencies.map((frequency, i) => <label key={frequency} className="band"><output>{usable ? preview ? `${values[i] > 0 ? '+' : ''}${values[i]}` : values[i] : '—'}</output><input aria-label={preview ? frequency : `${frequency} device equalizer step`} aria-valuetext={usable ? preview ? `${values[i]} decibels` : `${values[i]} device steps` : 'Unavailable'} type="range" min={preview ? -range : 0} max={preview ? range : 12} step="1" value={usable ? values[i] : 0} disabled={disabled} onChange={e => { if (!editing && active?.id.startsWith('local:')) setEditing(active.name); const next = [...values]; next[i] = Number(e.target.value); onChange('equalizer', next); if (preview) onChange('preset', 'Custom'); }}/><span>{frequency.replace(' Hz', '').replace(' kHz', 'k')}</span></label>)}</div>
    </div></div>
    {!usable && <p className="unavailable"><Info size={14}/>Equalizer controls are unavailable because this connected device has not confirmed the equalizer capability and current band steps.</p>}
    <div className="eq-footnote"><span>{preview ? 'STUDIO PREVIEW · LOCAL ONLY' : 'LIVE DEVICE EQUALIZER'}</span><span>{preview ? `±${range} dB` : '0–12 DEVICE STEPS'}</span></div>
    {factory.length > 0 ? <div className="sony-eq-library" role="group" aria-label="Sony equalizer presets">{factory.map(p => <button className="sony-eq-preset" key={p.name} disabled={disabled} aria-pressed={selected === `sony:${p.name}`} onClick={() => apply(`sony:${p.name}`)}><svg viewBox="0 0 100 30" aria-hidden="true"><polyline points={p.bands.map((v, i) => `${i * 11 + 1},${preview ? 15 - v * 2 : 27 - v * 2}`).join(' ')}/></svg><span>{p.name}</span><small>{p.description}</small></button>)}</div> : <p className="eq-library-note">Sony factory curves for this model are not validated yet. Custom studio profiles remain available in preview.</p>}
    {factory.length > 0 && <p className="eq-library-note">{preview ? 'XM6 curves from Sound Connect. Edit any curve and save your own version.' : 'Sony curves are applied through the confirmed custom-EQ path. Names are saved on this computer; phone Custom 1/2 slots are not imported.'}</p>}
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
