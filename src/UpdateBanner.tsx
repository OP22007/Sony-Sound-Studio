import { useEffect, useRef, useState } from 'react';
import { ArrowDownToLine, LoaderCircle, Sparkles, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { nativeAvailable, nativeCommand } from './bridge';
import type { AppUpdateSnapshot } from './bridge';
import { loadLocal } from './storage';
import './AppUpdates.css';

const FIRST_CHECK_MS = 8000;
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
const isBoolean = (value: unknown): value is boolean => typeof value === 'boolean';

/** Automatic checks are on unless the user switched them off in App settings. */
export const autoUpdateEnabled = () => loadLocal('auto-update-check', true, isBoolean);

/** Tells people a newer version exists and lets them install it with one click. */
export default function UpdateBanner() {
  const [snapshot, setSnapshot] = useState<AppUpdateSnapshot | null>(null);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  const [installRequested, setInstallRequested] = useState(false);
  const busy = useRef(false);

  useEffect(() => {
    if (!nativeAvailable) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<AppUpdateSnapshot>('app-update', event => { if (!disposed) setSnapshot(event.payload); }).then(stop => {
      if (disposed) stop(); else unlisten = stop;
      return nativeCommand<AppUpdateSnapshot>('get_app_update').then(next => { if (!disposed) setSnapshot(previous => previous ?? next); });
    }).catch(() => { /* The banner is optional; settings still offer a manual check. */ });

    // Look for a new version shortly after start and then every few hours while the window is open.
    const check = () => {
      if (disposed || busy.current || !autoUpdateEnabled()) return;
      busy.current = true;
      nativeCommand<AppUpdateSnapshot>('check_app_update')
        .then(next => { if (!disposed) setSnapshot(next); })
        .catch(() => { /* A failed background check is silent. */ })
        .finally(() => { busy.current = false; });
    };
    const delay = (window as unknown as { __updateCheckDelayMs?: number }).__updateCheckDelayMs ?? FIRST_CHECK_MS;
    const first = window.setTimeout(check, delay);
    const repeat = window.setInterval(check, CHECK_EVERY_MS);
    return () => { disposed = true; unlisten?.(); window.clearTimeout(first); window.clearInterval(repeat); };
  }, []);

  async function install() {
    if (installRequested) return;
    setInstallRequested(true);
    try {
      setSnapshot(await nativeCommand<AppUpdateSnapshot>('install_app_update'));
    } catch (error) {
      setSnapshot(previous => previous ? { ...previous, status: 'error', error: String(error) } : previous);
    } finally {
      setInstallRequested(false);
    }
  }

  if (!snapshot) return null;
  const { status } = snapshot;
  const working = status === 'downloading' || status === 'installing' || installRequested;
  const offered = status === 'available' && snapshot.version !== null && snapshot.version !== dismissedVersion;
  const failedInstall = status === 'error' && snapshot.version !== null;
  if (!offered && !working && !failedInstall) return null;

  const progress = snapshot.total && snapshot.total > 0 ? Math.min(100, Math.round(snapshot.downloaded / snapshot.total * 100)) : null;
  const message = status === 'installing' ? 'Installing the update… Sound Connect will restart shortly.'
    : status === 'downloading' ? progress === null ? 'Downloading the update…' : `Downloading the update… ${progress}%`
      : failedInstall ? snapshot.error ?? 'The update could not be installed.'
        : installRequested ? 'Starting the update…'
          : `Sound Connect ${snapshot.version} is ready. You are on ${snapshot.currentVersion}.`;
  const title = failedInstall ? 'The update did not finish' : working ? 'Updating' : 'New update available';

  return <section className={`update-banner${failedInstall ? ' is-error' : ''}`} aria-label="App update" role={failedInstall ? 'alert' : 'status'}>
    <span className="update-banner-icon" aria-hidden="true">{working ? <LoaderCircle size={18} className="spinner"/> : <Sparkles size={18}/>}</span>
    <span className="update-banner-copy"><strong>{title}</strong><span>{message}</span>
      {status === 'downloading' && <progress className="update-banner-progress" max={snapshot.total ?? undefined} value={snapshot.total === null ? undefined : snapshot.downloaded} aria-label="Update download progress"/>}
      {offered && snapshot.notes && <small className="update-banner-notes">{snapshot.notes}</small>}
    </span>
    {!working && <span className="update-banner-actions">
      <button type="button" className="button small" onClick={() => void install()}><ArrowDownToLine size={14}/>{failedInstall ? 'Try again' : 'Update now'}</button>
      {offered && <button type="button" className="icon-button" aria-label="Remind me later" title="Remind me later" onClick={() => setDismissedVersion(snapshot.version)}><X size={15}/></button>}
    </span>}
  </section>;
}
