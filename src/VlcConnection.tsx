import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ChevronDown, ChevronUp, Info, LockKeyhole, Plug, Radio, RefreshCw, Unplug } from 'lucide-react';
import { nativeCommand } from './bridge';
import './VlcConnection.css';

interface VlcConnectionSnapshot {
  enabled: boolean;
  passwordConfigured: boolean;
  status: string;
  playing: boolean;
  kind: 'movie' | 'music' | 'other' | null;
  startupConfigured?: boolean;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export default function VlcConnection() {
  const [snapshot, setSnapshot] = useState<VlcConnectionSnapshot | null>(null);
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);

  useEffect(() => {
    let disposed = false;
    let eventReceived = false;
    let unlisten: (() => void) | undefined;

    void listen<VlcConnectionSnapshot>('vlc-connection', event => {
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

    void nativeCommand<VlcConnectionSnapshot>('get_vlc_connection')
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

  const configure = async (enabled: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await nativeCommand<VlcConnectionSnapshot>('configure_vlc_connection', {
        enabled,
        ...(enabled && password.length > 0 ? { password } : {}),
      });
      setSnapshot(result);
      setPassword('');
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const testConnection = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await nativeCommand<VlcConnectionSnapshot>(
        'test_vlc_connection',
        password.length > 0 ? { password } : undefined,
      );
      setSnapshot(result);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  const configureStartup = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await nativeCommand<VlcConnectionSnapshot>('configure_vlc_startup'));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally { setBusy(false); }
  };

  const connected = snapshot?.enabled ?? false;
  const statusText = loading ? 'Checking connection…' : snapshot?.status ?? 'Status unavailable';
  const kindText = snapshot?.kind ? snapshot.kind[0].toUpperCase() + snapshot.kind.slice(1) : null;

  return <section className="vlc-connection" aria-label="VLC connection" aria-busy={loading || busy}>
    <div className="vlc-heading">
      <div>
        <span className="eyebrow"><span className="eyebrow-line"/> OPTIONAL PLAYER</span>
        <h3>VLC connection</h3>
        <p>Standard Windows playback detection may not see VLC. Connect its local web interface to follow VLC playback.</p>
      </div>
      <span className={`vlc-status${connected ? ' is-connected' : ''}`}><span className="status-dot"/>{connected ? 'Monitoring enabled' : 'Monitoring off'}</span>
    </div>

    <div className="vlc-current" role="status" aria-live="polite">
      <div className="vlc-current-icon">{snapshot?.playing ? <Radio size={16}/> : <Info size={16}/>}</div>
      <div className="vlc-current-copy">
        <strong>{snapshot?.playing ? `VLC playback detected${kindText ? ` · ${kindText}` : ''}` : statusText}</strong>
        {snapshot?.playing && <small>{statusText}</small>}
      </div>
      {snapshot?.playing && <span className="vlc-playing"><span/>PLAYING</span>}
    </div>

    <label className="vlc-password-label" htmlFor="vlc-http-password"><LockKeyhole size={14}/><span>Lua HTTP password</span>{snapshot?.passwordConfigured && <small>Password saved</small>}</label>
    <input
      className="vlc-password"
      id="vlc-http-password"
      type="password"
      autoComplete="new-password"
      value={password}
      onChange={event => setPassword(event.target.value)}
      placeholder={snapshot?.passwordConfigured ? 'Leave blank to keep saved password' : 'Enter the password set in VLC'}
      aria-label="VLC Lua HTTP password"
      disabled={busy}
    />
    <small className="vlc-password-note">Saved credentials stay on this computer. The input is cleared after saving; passwords are never saved in browser storage.</small>

    <div className="vlc-actions">
      <button className="vlc-action primary" type="button" disabled={busy || loading} onClick={() => void configure(true)}>
        <Plug size={14}/>{busy ? 'Working…' : connected ? 'Save and reconnect' : 'Connect and save'}
      </button>
      {connected && <button className="vlc-action" type="button" disabled={busy || loading} onClick={() => void configure(false)}><Unplug size={14}/>Disconnect</button>}
      <button className="vlc-action test" type="button" disabled={busy} onClick={() => void testConnection()}><RefreshCw size={13} className={busy ? 'vlc-spinning' : ''}/>Test connection</button>
    </div>
    <p className="vlc-test-note">Testing reads VLC’s status. It does not enable Follow media or change your headphone mode.</p>

    <div className="vlc-startup">
      <div><strong>{snapshot?.startupConfigured ? 'Ready for VLC restarts' : 'Keep VLC connected after restart'}</strong><p>{snapshot?.startupConfigured ? 'The Web interface is saved in VLC preferences. Open VLC normally; monitoring reconnects when it becomes available.' : 'Save your password above, close VLC, then enable startup once. This saves the local Web interface in VLC preferences and keeps a backup.'}</p></div>
      <button className="vlc-action" type="button" disabled={busy || loading || !connected || !snapshot?.passwordConfigured} onClick={() => void configureStartup()}>{snapshot?.startupConfigured ? 'Reconfigure VLC startup' : 'Enable VLC startup'}</button>
    </div>

    {error && <div className="vlc-error" role="alert"><Info size={14}/><span>{error}</span><button type="button" onClick={() => setError(null)} aria-label="Dismiss error">×</button></div>}

    <div className="vlc-setup">
      <button className="vlc-setup-toggle" type="button" aria-expanded={setupOpen} onClick={() => setSetupOpen(open => !open)}>
        <span>One-time VLC setup</span>{setupOpen ? <ChevronUp size={15}/> : <ChevronDown size={15}/>}
      </button>
      {setupOpen && <div className="vlc-setup-details">
        <ol>
          <li>In VLC, open <strong>Tools → Preferences</strong>.</li>
          <li>Choose <strong>Show settings: All</strong>, then open <strong>Interface → Main interfaces</strong> and enable <strong>Web</strong>.</li>
          <li>Open <strong>Interface → Main interfaces → Lua</strong> and set a dedicated Lua HTTP password.</li>
          <li>Keep the HTTP host at <code>127.0.0.1</code> and port at <code>8080</code> in the advanced HTTP settings or startup options.</li>
        </ol>
        <p>Save the preferences and restart VLC. <strong>View → Add Interface → Web</strong> only starts it for the current session. The Enable VLC startup button above saves it for future launches.</p>
      </div>}
    </div>
  </section>;
}
