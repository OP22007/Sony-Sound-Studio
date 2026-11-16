import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { animate, stagger } from 'animejs';
import { Gamepad2, Info, LockKeyhole, Trash2 } from 'lucide-react';
import { nativeAvailable, nativeCommand } from './bridge';
import {
  categoryLabel, durationParts, formatDuration, historyRanges, modeLabel, noiseLabel, sessionHeadline, sessionSubline, spokenDuration,
} from './history-format';
import type { HistoryBucket, HistoryRange, HistorySettings, HistorySummary } from './history-format';
import './HistoryPage.css';

const POLL_MS = 30000;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Tween a number so the readout settles instead of jumping; instant when motion is reduced. */
function useTween(target: number, reduced: boolean): number {
  const [shown, setShown] = useState(reduced ? target : 0);
  const current = useRef(reduced ? target : 0);
  useEffect(() => {
    if (reduced) { current.current = target; setShown(target); return; }
    const state = { value: current.current };
    const run = animate(state, {
      value: target, duration: 700, ease: 'outQuart',
      onUpdate: () => { current.current = state.value; setShown(Math.round(state.value)); },
      onComplete: () => { current.current = target; setShown(target); },
    });
    return () => { run.pause(); };
  }, [target, reduced]);
  return shown;
}

function Meters({ label, buckets, labelFor }: { label: string; buckets: HistoryBucket[]; labelFor: (key: string) => string }) {
  const sum = buckets.reduce((total, bucket) => total + bucket.ms, 0);
  const top = buckets[0]?.ms ?? 1;
  return <ul className="hs-rows" aria-label={label}>
    {buckets.map((bucket, index) => <li className="hs-row" key={bucket.key}>
      <span className="hs-row-label">{labelFor(bucket.key)}</span>
      <span className="hs-meter" aria-hidden="true"><span className={`hs-meter-fill${index === 0 ? ' is-lead' : ''}`} style={{ width: `${Math.max(2, (bucket.ms / top) * 100)}%` }}/></span>
      <span className="hs-row-value">{formatDuration(bucket.ms)}<small>{Math.round((bucket.ms / Math.max(sum, 1)) * 100)}%</small></span>
    </li>)}
  </ul>;
}

function dayLabel(startMs: number, compact: boolean): string {
  const date = new Date(startMs + 60 * 60 * 1000);
  return compact
    ? date.toLocaleDateString(undefined, { weekday: 'short' })
    : date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

const ALL_DEVICES = '__all__';

export default function HistoryPage({ reducedMotion, deviceName }: { reducedMotion: boolean; deviceName?: string }) {
  const [range, setRange] = useState<HistoryRange>('7d');
  // History is for the headphone in use. Other devices appear only when the user asks for them.
  const [scope, setScope] = useState<string>(deviceName ?? ALL_DEVICES);
  const [summary, setSummary] = useState<HistorySummary | null>(null);
  const [settings, setSettings] = useState<HistorySettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const root = useRef<HTMLElement>(null);
  const requested = useRef<string>('');

  const load = useCallback(async (next: HistoryRange, device: string) => {
    if (!nativeAvailable) return;
    const key = `${next}|${device}`;
    requested.current = key;
    try {
      const result = await nativeCommand<HistorySummary>('get_history', { range: next, tzOffsetMinutes: new Date().getTimezoneOffset(), device: device === ALL_DEVICES ? null : device });
      if (requested.current === key) { setSummary(result); setError(null); }
    } catch (reason) {
      if (requested.current === key) setError(errorMessage(reason));
    }
  }, []);

  useEffect(() => {
    if (!nativeAvailable) return;
    void load(range, scope);
    const timer = window.setInterval(() => { if (!document.hidden) void load(range, scope); }, POLL_MS);
    return () => window.clearInterval(timer);
  }, [range, scope, load]);

  useEffect(() => {
    if (!nativeAvailable) return;
    nativeCommand<HistorySettings>('get_history_settings').then(setSettings).catch(reason => setError(errorMessage(reason)));
  }, []);

  const total = useTween(summary?.totalMs ?? 0, reducedMotion);

  // One entrance per range: bars rise, meters fill. Polling refreshes do not replay it.
  const motionKey = summary ? `${summary.range}|${scope}` : '';
  useLayoutEffect(() => {
    if (!motionKey || reducedMotion || !root.current) return;
    const bars = root.current.querySelectorAll('.hs-bar-fill');
    const meters = root.current.querySelectorAll('.hs-meter-fill');
    const rows = root.current.querySelectorAll('.hs-row, .hs-session');
    const runs = [
      animate(bars, { scaleY: [0, 1], delay: stagger(16), duration: 640, ease: 'outExpo' }),
      animate(meters, { scaleX: [0, 1], delay: stagger(55, { start: 160 }), duration: 760, ease: 'outExpo' }),
      animate(rows, { opacity: [0, 1], translateY: [8, 0], delay: stagger(30, { start: 120 }), duration: 460, ease: 'outQuint' }),
    ];
    return () => runs.forEach(run => run.pause());
  }, [motionKey, reducedMotion]);

  async function save(next: HistorySettings) {
    if (busy) return;
    setBusy(true);
    try {
      setSettings(await nativeCommand<HistorySettings>('set_history_settings', { enabled: next.enabled, details: next.details, exact: next.exact }));
      setError(null);
      void load(range, scope);
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  }

  async function clear() {
    if (busy) return;
    setBusy(true);
    try {
      await nativeCommand<null>('clear_history');
      setConfirmClear(false);
      setError(null);
      await load(range, scope);
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  }

  if (!nativeAvailable) return <p className="unavailable"><Info size={14}/>History is recorded by the desktop app. Open it to see your listening time.</p>;

  const phrase = historyRanges.find(item => item.id === range)?.phrase ?? '';
  const pickable = [...new Set([...(deviceName ? [deviceName] : []), ...(summary?.knownDevices ?? [])])];
  const scoped = scope !== ALL_DEVICES;
  const empty = !!summary && summary.totalMs === 0 && summary.firstMs === null;
  const { hours, minutes } = durationParts(total);
  const maxDay = Math.max(1, ...(summary?.days.map(day => day.ms) ?? [1]));
  const compact = (summary?.days.length ?? 0) <= 7;
  const days = summary?.days ?? [];

  return <section className="sound-panel history reveal" ref={root} aria-label="Listening history">
    <div className="hs-top">
      <div role="group" aria-label="History range" className="hs-ranges">
        {historyRanges.map(item => <button key={item.id} type="button" className="hs-range" aria-pressed={range === item.id} onClick={() => setRange(item.id)}>{item.label}</button>)}
      </div>
      <label className="hs-scope"><span>Show</span><select value={scope} onChange={event => setScope(event.target.value)} aria-label="Show history for">
        {pickable.map(name => <option key={name} value={name}>{name}{name === deviceName ? ' (in use)' : ''}</option>)}
        <option value={ALL_DEVICES}>All devices</option>
      </select></label>
      <p className="hs-private"><LockKeyhole size={14}/>Stays on this computer. Nothing is uploaded.</p>
    </div>

    {error && <p className="hs-error" role="alert"><Info size={14}/>{error}</p>}
    {summary?.writeError && <p className="hs-error" role="status"><Info size={14}/>{summary.writeError}</p>}

    <div className={`hs-stage${empty ? ' is-empty' : ''}`}>
      <div className="hs-stage-main">
        <span className="hs-stage-label">Connected time · {scoped ? `${scope} · ` : 'all devices · '}{phrase}</span>
        <strong className="hs-readout" data-total-ms={summary?.totalMs ?? 0} aria-label={spokenDuration(summary?.totalMs ?? 0)}>
          {hours > 0 && <><span>{hours}</span><em>h</em></>}<span>{minutes}</span><em>min</em>
        </strong>
        <p className="hs-stage-note">{empty
          ? 'Nothing recorded yet. History starts when a headphone connects.'
          : 'Time your headphones were connected, by listening mode. It does not mean audio was playing.'}</p>
        {summary?.firstMs ? <small className="hs-since">Recording since {new Date(summary.firstMs).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}</small> : null}
      </div>
      <div className="hs-chart" role="group" aria-label="Daily listening time">
        {maxDay > 1 && <span className="hs-scale" aria-hidden="true">Busiest day {formatDuration(maxDay)}</span>}
        <ol className="hs-bars">
          {days.map((day, index) => {
            const today = index === days.length - 1;
            return <li key={day.startMs} className={`hs-bar${today ? ' is-today' : ''}`} title={`${dayLabel(day.startMs, false)}: ${formatDuration(day.ms)}`} aria-label={`${dayLabel(day.startMs, false)}, ${spokenDuration(day.ms)}`}>
              <span className="hs-bar-track"><span className="hs-bar-fill" style={{ height: day.ms === 0 ? '2px' : `${Math.max(3, (day.ms / maxDay) * 100)}%` }}/></span>
              {compact && <span className="hs-bar-label" aria-hidden="true">{today ? 'Today' : dayLabel(day.startMs, true)}</span>}
            </li>;
          })}
        </ol>
        {!compact && days.length > 0 && <div className="hs-axis" aria-hidden="true"><span>{dayLabel(days[0].startMs, false)}</span><span>Today</span></div>}
      </div>
    </div>

    {summary && !empty && <div className="hs-grid">
      <section className="hs-card" aria-labelledby="hs-modes"><h3 id="hs-modes">By listening mode</h3>{summary.modes.length ? <Meters label="Time by listening mode" buckets={summary.modes} labelFor={modeLabel}/> : <p className="hs-none">No time in this range.</p>}</section>
      {!scoped && <section className="hs-card" aria-labelledby="hs-devices"><h3 id="hs-devices">By device</h3>{summary.devices.length ? <Meters label="Time by device" buckets={summary.devices} labelFor={key => key}/> : <p className="hs-none">No time in this range.</p>}</section>}
      {summary.noise.length > 0 && <section className="hs-card" aria-labelledby="hs-noise"><h3 id="hs-noise">By noise control</h3><Meters label="Time by noise control" buckets={summary.noise} labelFor={noiseLabel}/></section>}
      <section className="hs-card" aria-labelledby="hs-content"><h3 id="hs-content"><Gamepad2 size={15}/>By content</h3>
        {!summary.contentSupported ? <p className="hs-none">Playback details are recorded on Windows only.</p>
          : !summary.details ? <p className="hs-none">Off. Turn on “Record apps and sites” below to see what you listen to.</p>
          : summary.categories.length === 0 ? <p className="hs-none">Nothing yet. Details appear when Follow media detects playback, so turn Follow media on.</p>
          : <><Meters label="Time by content type" buckets={summary.categories} labelFor={categoryLabel}/>{summary.sources.length > 0 && <><h4>Top apps and sites</h4><Meters label="Time by app or site" buckets={summary.sources.slice(0, 5)} labelFor={key => key}/></>}
            {summary.exact && summary.titles.length > 0 && <><h4>Top songs, videos and games</h4><Meters label="Time by title" buckets={summary.titles.slice(0, 5)} labelFor={key => key}/></>}
            {summary.exact && summary.urls.length > 0 && <><h4>Top pages</h4><ul className="hs-pages" aria-label="Time by page address">{summary.urls.slice(0, 5).map(page => <li key={page.key}><span className="hs-page-url" title={page.key}>{page.key}</span><span>{formatDuration(page.ms)}</span></li>)}</ul></>}</>}
      </section>
    </div>}

    {summary && summary.recent.length > 0 && <section className="hs-card hs-recent" aria-labelledby="hs-recent"><h3 id="hs-recent">Recent sessions</h3>
      <ul>{summary.recent.map(segment => <li className="hs-session" key={`${segment.device}-${segment.start}`}>
        <time dateTime={new Date(segment.start).toISOString()}>{new Date(segment.start).toLocaleString(undefined, { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}</time>
        <span className="hs-session-main"><strong>{sessionHeadline(segment)}</strong><small>{sessionSubline(segment)}</small>{segment.url && <small className="hs-session-url">{segment.url}</small>}</span>
        <span className="hs-session-time">{formatDuration(segment.end - segment.start)}</span>
      </li>)}</ul>
    </section>}

    {summary && summary.skipped > 0 && <p className="hs-none">{summary.skipped} damaged {summary.skipped === 1 ? 'entry was' : 'entries were'} skipped.</p>}

    <section className="hs-card hs-settings" aria-labelledby="hs-settings">
      <h3 id="hs-settings">Recording</h3>
      <div className="hs-setting">
        <span><strong id="hs-enabled-label">Record listening history</strong><small>Connected time per headphone and listening mode. Turning it off keeps what is already saved.</small></span>
        <button type="button" role="switch" className="hs-switch" aria-checked={settings?.enabled ?? false} aria-labelledby="hs-enabled-label" disabled={!settings || busy} onClick={() => settings && void save({ ...settings, enabled: !settings.enabled })}><span/></button>
      </div>
      <div className="hs-setting">
        <span><strong id="hs-details-label">Record apps and sites</strong><small>Also saves the playback type (game, music, movie) and the app or site name from Follow media. Titles and addresses need the next switch. Applies from now on only.</small></span>
        <button type="button" role="switch" className="hs-switch" aria-checked={settings?.details ?? false} aria-labelledby="hs-details-label" disabled={!settings || busy || !settings.enabled} onClick={() => settings && void save({ ...settings, details: !settings.details })}><span/></button>
      </div>
      <div className="hs-setting">
        <span><strong id="hs-exact-label">Record exact titles and URLs</strong><small>Also saves the full address of the page you are on (including anything after the ?), song and video titles with the artist, and the game's window title. Never from private windows. Windows only; addresses need the Browser companion 1.0.3. Applies from now on only.</small></span>
        <button type="button" role="switch" className="hs-switch" aria-checked={settings?.exact ?? false} aria-labelledby="hs-exact-label" disabled={!settings || busy || !settings.enabled || !settings.details} onClick={() => settings && void save({ ...settings, exact: !settings.exact })}><span/></button>
      </div>
      <div className="hs-clear">
        {confirmClear
          ? <><p role="alert">Delete all saved listening history? This cannot be undone.</p><div><button type="button" className="hs-danger" disabled={busy} onClick={() => void clear()}>Delete history</button><button type="button" className="hs-quiet" disabled={busy} onClick={() => setConfirmClear(false)}>Keep it</button></div></>
          : <button type="button" className="hs-quiet" onClick={() => setConfirmClear(true)}><Trash2 size={14}/>Clear history…</button>}
      </div>
    </section>
  </section>;
}
