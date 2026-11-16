import { useEffect, useState } from 'react';
import { ArrowDownToLine, Check, CircleHelp, LoaderCircle, RefreshCw, RotateCw, Sparkles } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { nativeAvailable, nativeCommand } from './bridge';
import type { AppUpdateSnapshot } from './bridge';
import { saveLocal } from './storage';
import { autoUpdateEnabled } from './UpdateBanner';
import './AppUpdates.css';

const browserSnapshot: AppUpdateSnapshot = {
  currentVersion: '—', status: 'unsupported', version: null, notes: null,
  downloaded: 0, total: null, error: 'App updates are unavailable in browser preview.',
};

export default function AppUpdates({ onVersionChange }: { onVersionChange: (version: string) => void }) {
  const [snapshot, setSnapshot] = useState<AppUpdateSnapshot>(() => nativeAvailable
    ? { ...browserSnapshot, status: 'idle', error: null }
    : browserSnapshot);
  const [pendingAction, setPendingAction] = useState<'check' | 'install' | null>(null);
  const [auto, setAuto] = useState(() => autoUpdateEnabled());

  useEffect(() => {
    if (!nativeAvailable) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let eventSeen = false;
    const apply = (next: AppUpdateSnapshot) => {
      if (disposed) return;
      setSnapshot(next);
      if (next.currentVersion) onVersionChange(next.currentVersion);
    };
    void listen<AppUpdateSnapshot>('app-update', event => { eventSeen = true; apply(event.payload); }).then(stop => {
      if (disposed) stop(); else unlisten = stop;
      return nativeCommand<AppUpdateSnapshot>('get_app_update').then(next => {
        if (!eventSeen) apply(next);
      });
    }).catch(error => {
      if (!disposed) setSnapshot(previous => ({ ...previous, status: 'error', error: String(error) }));
    });
    return () => { disposed = true; unlisten?.(); };
  }, [onVersionChange]);

  async function run(action: 'check' | 'install') {
    if (!nativeAvailable || pendingAction) return;
    setPendingAction(action);
    try {
      const next = await nativeCommand<AppUpdateSnapshot>(action === 'check' ? 'check_app_update' : 'install_app_update');
      setSnapshot(next);
      if (next.currentVersion) onVersionChange(next.currentVersion);
    } catch (error) {
      setSnapshot(previous => ({ ...previous, status: 'error', error: String(error) }));
    } finally {
      setPendingAction(null);
    }
  }

  const checking = snapshot.status === 'checking' || pendingAction === 'check';
  const installing = snapshot.status === 'installing' || pendingAction === 'install';
  const downloading = snapshot.status === 'downloading';
  const progress = snapshot.total && snapshot.total > 0
    ? Math.min(100, Math.round(snapshot.downloaded / snapshot.total * 100)) : null;
  const statusText = checking ? 'Checking for updates…'
    : downloading ? progress === null ? 'Downloading update…' : `Downloading update… ${progress}%`
      : installing ? 'Installing update… the app will restart shortly.'
        : snapshot.status === 'available' ? `Version ${snapshot.version} is ready to install.`
          : snapshot.status === 'upToDate' ? 'You’re running the latest version.'
              : snapshot.status === 'unsupported' ? snapshot.error ?? 'Updates are unavailable for this build.'
              : snapshot.status === 'error' ? snapshot.error ?? 'The update check failed.'
                : 'Check for a newer version of Sound Connect.';

  return <section className="settings-surface reveal app-updates" aria-busy={checking || downloading || installing}>
    <div className="section-title"><h2>App updates</h2><Sparkles size={18}/></div>
    <div className="app-update-content">
      <div className="app-update-copy">
        <span className="app-update-version">CURRENT VERSION <strong>{snapshot.currentVersion}</strong></span>
        <p className={`app-update-status ${snapshot.status === 'error' ? 'is-error' : ''}`} role={snapshot.status === 'error' ? 'alert' : 'status'}>
          {checking || downloading || installing ? <LoaderCircle size={15} className="spinner"/> : snapshot.status === 'upToDate' ? <Check size={15}/> : snapshot.status === 'unsupported' ? <CircleHelp size={15}/> : null}
          {statusText}
        </p>
        {snapshot.notes && snapshot.status === 'available' && <p className="app-update-notes">{snapshot.notes}</p>}
        <label className="app-update-auto"><input type="checkbox" checked={auto} disabled={!nativeAvailable} onChange={event => { setAuto(event.target.checked); saveLocal('auto-update-check', event.target.checked); if (event.target.checked) void run('check'); }}/>Check for updates automatically</label>
        {downloading && <progress className="app-update-progress" max={snapshot.total ?? undefined} value={snapshot.total === null ? undefined : snapshot.downloaded} aria-label="Update download progress"/>}
      </div>
      <div className="app-update-actions">
        {snapshot.status === 'available' ? <button className="button small" disabled={!nativeAvailable || !!pendingAction} onClick={() => void run('install')}><ArrowDownToLine size={14}/>Install & restart</button>
          : <button className="text-button" disabled={!nativeAvailable || checking || downloading || installing || snapshot.status === 'unsupported'} onClick={() => void run('check')}>
            {checking ? <LoaderCircle size={14} className="spinner"/> : snapshot.status === 'upToDate' || snapshot.status === 'error' ? <RefreshCw size={14}/> : <RotateCw size={14}/>}
            {checking ? 'Checking…' : snapshot.status === 'upToDate' ? 'Check again' : snapshot.status === 'unsupported' ? 'Unavailable' : snapshot.status === 'error' ? 'Try again' : 'Check for updates'}
          </button>}
      </div>
    </div>
  </section>;
}
