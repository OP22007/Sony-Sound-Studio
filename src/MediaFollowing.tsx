import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Check, ChevronDown, ChevronUp, Copy, Info, Radio, RotateCcw, Sparkles } from 'lucide-react';
import { nativeAvailable, nativeCommand } from './bridge';
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
  deviceConnected: boolean;
  cinemaSupported: boolean;
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

const categoryLabel = { movie: 'MOVIE', music: 'MUSIC', game: 'GAME', other: 'STANDARD' } as const;

export default function MediaFollowing({ deviceConnected, cinemaSupported }: Props) {
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

  return <section className="media-following sound-panel reveal" aria-label="Automatic listening mode" aria-busy={loading || busy}>
    <div className="mf-heading">
      <div className="mf-heading-copy">
        <span className="eyebrow"><span className="eyebrow-line"/> AUTOMATIC LISTENING</span>
        <h2>Follow what’s playing</h2>
        <p>Opt in to match your headphones to games, music and movies.</p>
      </div>
      <div className={`mf-state${enabled && !snapshot?.manualOverride ? ' is-on' : ''}`}><span className="status-dot"/>{status}</div>
    </div>

    {!nativeAvailable && <div className="mf-notice"><Info size={15}/><span>Automatic listening is available in the Windows desktop app.</span></div>}
    {snapshot?.status === 'unsupported' && nativeAvailable && <div className="mf-notice"><Info size={15}/><span>Automatic listening currently requires Windows.</span></div>}

    <div className="mf-toggle-row">
      <div><strong>Follow media</strong><small>Only changes settings your headphones confirm: listening mode, spatial audio and Game mode. This preference is saved on this computer.</small></div>
      <button className={`mf-switch${enabled ? ' checked' : ''}`} type="button" role="switch" aria-checked={enabled} aria-label="Follow media" disabled={toggleDisabled} onClick={() => void runCommand('set_media_automation', !enabled)}>
        <span/>
      </button>
    </div>

    {snapshot?.manualOverride && enabled && <div className="mf-override">
      <div><strong>Your manual mode is being respected</strong><small>Automatic switching will stay paused until you resume it.</small></div>
      <button className="mf-button" type="button" disabled={busy || unsupported} onClick={() => void runCommand('resume_media_automation')}><RotateCcw size={14}/> Resume</button>
    </div>}

    {enabled && snapshot && <div className="mf-live">
      <div className="mf-live-icon"><Radio size={17}/></div>
      <div className="mf-live-copy">
        <div className="mf-live-top"><span>{snapshot.browserConnected ? 'Browser companion reached desktop' : deviceConnected ? 'Headphones connected' : 'Headphones disconnected'}</span>{snapshot.desiredMode && <b>{categoryLabel[snapshot.desiredMode]}</b>}</div>
        {snapshot.source && <small className="mf-source">{snapshot.source}</small>}
        {!!snapshot.plannedSettings?.length && <small className="mf-planned">{snapshot.plannedSettings.join(' · ')}</small>}
        <small className="mf-reason">{snapshot.reason}</small>
      </div>
    </div>}

    {enabled && !cinemaSupported && <div className="mf-cinema-note"><Info size={14}/><span>Cinema mode is not confirmed for these headphones, so movie playback may not switch.</span></div>}
    {error && <div className="mf-error" role="alert"><Info size={14}/><span>{error}</span>{!snapshot && nativeAvailable && <button className="mf-retry" type="button" disabled={busy} onClick={() => void retryStatus()}>Retry</button>}<button type="button" onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}

    {enabled && snapshot?.browserReceiverError && <div className="mf-error" role="alert"><Info size={14}/><span>{snapshot.browserReceiverError}</span></div>}
    <div className="mf-rules">
      <div><Sparkles size={14}/><span><strong>Games</strong><small>Game mode or Game listening mode</small></span></div>
      <div><Sparkles size={14}/><span><strong>Music apps and sites</strong><small>Music mode, spatial audio off</small></span></div>
      <div><Sparkles size={14}/><span><strong>Streaming movies and shows</strong><small>Cinema, or spatial audio on</small></span></div>
      <div><Sparkles size={14}/><span><strong>Anything else</strong><small>Standard</small></span></div>
      <p>Only settings your headphones confirm are changed. A game wins over background music. VLC uses the media type it reports. Atmos is not identified automatically yet.</p>
    </div>

    {enabled && nativeAvailable && <VlcConnection/>}
    {enabled && snapshot?.extensionToken && <div className="mf-pairing">
      <button className="mf-details-toggle" type="button" aria-expanded={detailsOpen} onClick={() => setDetailsOpen(open => !open)}>
        <span>Browser companion setup</span>{detailsOpen ? <ChevronUp size={15}/> : <ChevronDown size={15}/>}
      </button>
      {detailsOpen && <div className="mf-details">
        <p>Install the extension in Chrome, Edge, Brave, or Firefox, then pair it with this local receiver. Playback activity is detected only while the extension is installed and Follow media is on.</p>
        <div className="mf-endpoint"><span>Receiver</span><code>http://127.0.0.1:47653/media</code></div>
        <div className="mf-install-actions"><button className="mf-button" type="button" disabled={busy} onClick={() => void prepareCompanion('chromium')}>Prepare Chrome / Edge extension</button><button className="mf-button" type="button" disabled={busy} onClick={() => void prepareCompanion('firefox')}>Prepare Firefox extension</button></div>
        <p>Chrome, Edge and Brave: open Extensions, enable Developer mode, choose Load unpacked and select the prepared folder. If already installed, reload the extension after preparing the updated files. Firefox: use Load Temporary Add-on in about:debugging and select manifest.json. Paste the pairing key into the extension popup and wait for “Connected”.</p>
        {companionPath && <div className="mf-endpoint"><span>Extension folder</span><code>{companionPath}</code></div>}
        <div className="mf-token"><span>Pairing key</span><code>{snapshot.extensionToken}</code><button className="mf-copy" type="button" onClick={() => void copyToken()} aria-label="Copy pairing key">{copied ? <Check size={14}/> : <Copy size={14}/>} {copied ? 'Copied' : 'Copy'}</button></div>
        <small className="mf-privacy">The receiver accepts local playback categories and site names; it does not collect titles or audio.</small>
      </div>}
    </div>}
  </section>;
}
