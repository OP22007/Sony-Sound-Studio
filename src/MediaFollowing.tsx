import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { AudioLines, Check, ChevronDown, ChevronUp, Clapperboard, Copy, Gamepad2, Globe, Headphones, Info, Minus, Music2, RotateCcw, X } from 'lucide-react';
import { nativeAvailable, nativeCommand } from './bridge';
import type { Device } from './bridge';
import { automationSupport } from './automation-support';
import './MediaFollowing.css';
import VlcConnection from './VlcConnection';

interface MediaAutomationSnapshot {
  enabled: boolean;
  status: string;
  source: string | null;
  desiredMode: 'movie' | 'music' | 'game' | 'other' | null;
  plannedSettings?: string[];
  reason: string;
  manualOverride: boolean;
  extensionToken: string;
  browserConnected: boolean;
  browserReceiverError?: string | null;
}

interface Props {
  device?: Device;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function statusLabel(snapshot: MediaAutomationSnapshot): string {
  if (snapshot.status === 'device-unavailable') return 'Listening mode unavailable';
  if (snapshot.status === 'unsupported') return 'Unavailable on this system';
  if (!snapshot.enabled || snapshot.status === 'disabled') return 'Off';
  if (snapshot.manualOverride || snapshot.status === 'manual') return 'Paused for your manual choice';
  if (snapshot.status === 'following') return 'Following media';
  if (snapshot.status === 'error') return 'Could not confirm mode';
  if (snapshot.status === 'retrying') return 'Waiting to retry';
  return 'Waiting for playback';
}

const categories = [
  { key: 'game', label: 'Game', icon: Gamepad2, trigger: 'Full-screen games and known game apps', sets: 'Game mode, Game listening mode' },
  { key: 'music', label: 'Music', icon: Music2, trigger: 'Spotify, Apple Music, Tidal, YouTube Music, VLC audio', sets: 'Music mode, spatial audio off' },
  { key: 'movie', label: 'Movie', icon: Clapperboard, trigger: 'Netflix, Prime Video, Disney+ and other streaming sites, VLC video', sets: 'Cinema, spatial audio on' },
  { key: 'other', label: 'Standard', icon: AudioLines, trigger: 'Anything else that is playing', sets: 'Standard, spatial audio off' },
] as const;
const supportLabel = { full: 'Supported', partial: 'Partly supported', none: 'Not supported' } as const;

function supportSummary(name: string, support: NonNullable<ReturnType<typeof automationSupport>>): string {
  const names = { game: 'Game', music: 'Music', movie: 'Movie', other: 'Standard' } as const;
  const keys = Object.keys(names) as (keyof typeof names)[];
  const full = keys.filter(key => support[key].level === 'full').map(key => names[key]);
  const partial = keys.filter(key => support[key].level === 'partial').map(key => names[key]);
  const none = keys.filter(key => support[key].level === 'none').map(key => names[key]);
  const parts = [full.length ? `${name} fully supports ${full.join(', ')}.` : `${name} does not fully support any playback type.`];
  if (partial.length) parts.push(`${partial.join(', ')} ${partial.length > 1 ? 'are' : 'is'} partly supported.`);
  if (none.length) parts.push(`${none.join(', ')} will leave your headphones unchanged.`);
  return parts.join(' ');
}

const categoryLabel = { movie: 'Movie', music: 'Music', game: 'Game', other: 'Standard' } as const;

export default function MediaFollowing({ device }: Props) {
  const deviceConnected = device?.connection === 'connected';
  const support = automationSupport(device);
  const [snapshot, setSnapshot] = useState<MediaAutomationSnapshot | null>(null);
  const [loading, setLoading] = useState(nativeAvailable);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [companionPath, setCompanionPath] = useState<string | null>(null);

  useEffect(() => {
    if (!nativeAvailable) {
      setLoading(false);
      return;
    }

    let disposed = false;
    let eventReceived = false;
    let unlisten: (() => void) | undefined;
    void listen<MediaAutomationSnapshot>('media-automation', event => {
      if (disposed) return;
      eventReceived = true;
      setSnapshot(event.payload);
      setError(null);
      setLoading(false);
    }).then(stop => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(reason => {
      if (!disposed) setError(`Live status is unavailable: ${errorMessage(reason)}`);
    });

    void nativeCommand<MediaAutomationSnapshot>('get_media_automation')
      .then(result => {
        if (!disposed && !eventReceived) setSnapshot(result);
      })
      .catch(reason => {
        if (!disposed) setError(errorMessage(reason));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const unsupported = !nativeAvailable || snapshot?.status === 'unsupported';
  const runCommand = async (command: 'set_media_automation' | 'resume_media_automation', enabled?: boolean) => {
    if (busy || !nativeAvailable) return;
    setBusy(true);
    setError(null);
    try {
      const next = await nativeCommand<MediaAutomationSnapshot>(command, command === 'set_media_automation' ? { enabled } : undefined);
      setSnapshot(next);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const retryStatus = async () => {
    if (!nativeAvailable || busy) return;
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await nativeCommand<MediaAutomationSnapshot>('get_media_automation'));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const copyToken = async () => {
    if (!snapshot?.extensionToken) return;
    try {
      await navigator.clipboard.writeText(snapshot.extensionToken);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch (reason) {
      setError(`Could not copy the pairing key: ${errorMessage(reason)}`);
    }
  };

  const prepareCompanion = async (browser: 'chromium' | 'firefox') => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setCompanionPath(await nativeCommand<string>('prepare_media_companion', { browser }));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally { setBusy(false); }
  };

  const enabled = snapshot?.enabled ?? false;
  const cannotEnable = !nativeAvailable || unsupported || loading || busy;
  const toggleDisabled = busy || loading || (enabled ? !nativeAvailable : cannotEnable);
  const status = snapshot ? statusLabel(snapshot) : loading ? 'Loading status' : nativeAvailable ? 'Status unavailable' : 'Desktop app required';

  const manual = !!snapshot?.manualOverride && enabled;
  const active = enabled && !manual ? snapshot?.desiredMode ?? null : null;
  const planned = snapshot?.plannedSettings ?? [];
  const confirmed = snapshot?.status === 'following';
  const readout = !enabled ? 'Off' : manual ? 'Paused' : active ? categoryLabel[active] : 'Listening';
  const readoutNote = !enabled
    ? 'Turn on Follow media and your headphones adjust themselves as you play games, music and movies.'
    : manual ? 'You changed a setting by hand. Automatic switching waits until you resume it.'
    : active ? snapshot?.reason : 'Start playing something. Pauses keep the current settings.';

  return <section className="media-following sound-panel reveal" aria-label="Automatic listening mode" aria-busy={loading || busy}>
    <header className="mf-heading">
      <div className="mf-heading-copy">
        <span className="eyebrow"><span className="eyebrow-line"/> AUTOMATIC LISTENING</span>
        <h2>Follow what’s playing</h2>
        <p>Opt in to match your headphones to games, music and movies.</p>
      </div>
      <div className="mf-master">
        <div><strong>Follow media</strong><small>{enabled ? 'On' : 'Off'} · saved on this computer</small></div>
        <button className={`mf-switch${enabled ? ' checked' : ''}`} type="button" role="switch" aria-checked={enabled} aria-label="Follow media" disabled={toggleDisabled} onClick={() => void runCommand('set_media_automation', !enabled)}>
          <span/>
        </button>
      </div>
    </header>

    {!nativeAvailable && <div className="mf-notice"><Info size={15}/><span>Automatic listening is available in the Windows desktop app.</span></div>}
    {snapshot?.status === 'unsupported' && nativeAvailable && <div className="mf-notice"><Info size={15}/><span>Automatic listening currently requires Windows.</span></div>}
    {manual && <div className="mf-override">
      <div><strong>Your manual mode is being respected</strong><small>Automatic switching stays paused until you resume it.</small></div>
      <button className="mf-button primary" type="button" disabled={busy || unsupported} onClick={() => void runCommand('resume_media_automation')}><RotateCcw size={14}/> Resume</button>
    </div>}
    {error && <div className="mf-error" role="alert"><Info size={14}/><span>{error}</span>{!snapshot && nativeAvailable && <button className="mf-retry" type="button" disabled={busy} onClick={() => void retryStatus()}>Retry</button>}<button type="button" onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}
    {enabled && snapshot?.browserReceiverError && <div className="mf-error" role="alert"><Info size={14}/><span>{snapshot.browserReceiverError}</span></div>}

    <div className={`mf-stage${active ? ' is-active' : ''}${enabled ? '' : ' is-off'}`} aria-live="polite">
      <div className="mf-stage-main">
        <span className="mf-stage-label">{enabled ? 'Now detecting' : 'Follow media'}</span>
        <strong className="mf-readout">{readout}</strong>
        {active && snapshot?.source && <span className="mf-source">{snapshot.source}</span>}
        <p className="mf-stage-note">{readoutNote}</p>
      </div>
      <div className="mf-stage-side">
        <div className="mf-state-row"><span className={`mf-pill${enabled && !manual ? ' is-on' : ''}`}><span className="status-dot"/>{status}</span></div>
        <span className="mf-stage-label">{confirmed ? 'Confirmed on your headphones' : 'Will apply to your headphones'}</span>
        {planned.length > 0
          ? <ul className="mf-planned">{planned.map(item => <li key={item}>{confirmed ? <Check size={14}/> : <span className="mf-bullet"/>}{item}</li>)}</ul>
          : <p className="mf-planned-empty">{enabled ? 'Nothing to apply yet.' : 'Settings appear here once playback is detected.'}</p>}
        <div className="mf-chips">
          <span className={`mf-chip${deviceConnected ? ' ok' : ''}`}><Headphones size={13}/>{deviceConnected ? 'Headphones connected' : 'Headphones disconnected'}</span>
          <span className={`mf-chip${snapshot?.browserConnected ? ' ok' : ''}`}><Globe size={13}/>{snapshot?.browserConnected ? 'Browser companion reached desktop' : 'Browser companion not connected'}</span>
        </div>
      </div>
    </div>

    <div className="mf-section-head">
      <h3>What your headphones will do</h3>
      <p>{support ? supportSummary(device?.name ?? 'Your headphones', support) : 'Connect your headphones to see which of these they support. Only settings they confirm are ever changed. A game wins over background music.'}</p>
    </div>
    <ul className="mf-map">
      {categories.map(({ key, label, icon: Icon, trigger, sets }) => {
        const entry = support?.[key];
        const level = entry?.level;
        return <li key={key} className={`${active === key ? 'is-current' : ''}${level ? ` is-${level}` : ''}`.trim() || undefined} aria-current={active === key ? 'true' : undefined}>
          <div className="mf-map-top">
            <span className="mf-map-icon"><Icon size={18}/></span>
            {level && <span className={`mf-support is-${level}`}>{level === 'full' ? <Check size={12}/> : level === 'partial' ? <Minus size={12}/> : <X size={12}/>}{supportLabel[level]}</span>}
          </div>
          <strong>{label}</strong>
          <small>{trigger}</small>
          <span className="mf-map-sets">
            {entry ? (entry.applies.length ? entry.applies.join(' · ') : 'Your headphones stay as they are') : sets}
            {entry && entry.missing.length > 0 && <em className="mf-map-missing">Not offered: {entry.missing.join(' · ')}</em>}
          </span>
        </li>;
      })}
    </ul>

    {enabled && <div className="mf-section-head"><h3>Connect your players</h3><p>Windows apps like Spotify are detected on their own. Browsers and VLC need a quick one-time setup.</p></div>}
    {enabled && snapshot?.extensionToken && <div className="mf-pairing">
      <button className="mf-details-toggle" type="button" aria-label="Browser companion setup" aria-describedby="mf-browser-status" aria-expanded={detailsOpen} onClick={() => setDetailsOpen(open => !open)}>
        <span className="mf-toggle-title"><Globe size={16}/>Browser companion setup</span>
        <span className="mf-toggle-meta"><span id="mf-browser-status" className={`mf-pill small${snapshot.browserConnected ? ' is-on' : ''}`}><span className="status-dot"/>{snapshot.browserConnected ? 'Connected' : 'Not connected'}</span>{detailsOpen ? <ChevronUp size={15}/> : <ChevronDown size={15}/>}</span>
      </button>
      {detailsOpen && <div className="mf-details">
        <p className="mf-lede">Needed to tell movies from music on websites. The extension reports only the site name and a playback category, never titles or audio.</p>
        <ol className="mf-steps">
          <li><span className="mf-step-title">Prepare the extension</span>
            <div className="mf-install-actions"><button className="mf-button" type="button" disabled={busy} onClick={() => void prepareCompanion('chromium')}>Prepare Chrome / Edge extension</button><button className="mf-button" type="button" disabled={busy} onClick={() => void prepareCompanion('firefox')}>Prepare Firefox extension</button></div>
            {companionPath && <div className="mf-endpoint"><span>Extension folder</span><code>{companionPath}</code></div>}</li>
          <li><span className="mf-step-title">Load it in your browser</span>
            <p>Chrome, Edge and Brave: open Extensions, turn on Developer mode, choose Load unpacked and select the prepared folder. If it is already installed, reload it. Firefox: use Load Temporary Add-on in about:debugging and select manifest.json.</p></li>
          <li><span className="mf-step-title">Pair it</span>
            <p>Paste this key into the extension popup and wait for “Connected”.</p>
            <div className="mf-token"><code>{snapshot.extensionToken}</code><button className="mf-copy" type="button" onClick={() => void copyToken()} aria-label="Copy pairing key">{copied ? <Check size={14}/> : <Copy size={14}/>} {copied ? 'Copied' : 'Copy'}</button></div>
            <div className="mf-endpoint"><span>Receiver</span><code>http://127.0.0.1:47653/media</code></div></li>
        </ol>
        <small className="mf-privacy">Playback is detected only while the extension is installed and Follow media is on. Atmos is not identified automatically.</small>
      </div>}
    </div>}
    {enabled && nativeAvailable && <VlcConnection/>}
  </section>;
}
