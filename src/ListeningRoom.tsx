import { useEffect, useRef, useState } from 'react';
import type { RoomRenderer } from './listening-room-renderer';

interface Props { scene: string; room: unknown; reducedMotion: boolean }

export default function ListeningRoom(props: Props) {
  const host = useRef<HTMLDivElement>(null);
  const renderer = useRef<RoomRenderer | null>(null);
  const latest = useRef(props);
  latest.current = props;
  const [status, setStatus] = useState('loading');

  const fallBack = () => {
    renderer.current?.dispose();
    renderer.current = null;
    setStatus('fallback');
  };

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    let disposed = false;
    void import('./listening-room-renderer').then(({ createRoomRenderer }) => {
      if (disposed) return;
      renderer.current = createRoomRenderer(element, () => { if (!disposed) fallBack(); });
      renderer.current.setMode(latest.current.scene, latest.current.room, latest.current.reducedMotion);
      setStatus('ready');
    }).catch(() => { if (!disposed) fallBack(); });
    return () => { disposed = true; renderer.current?.dispose(); renderer.current = null; };
  }, []);

  useEffect(() => {
    try { renderer.current?.setMode(props.scene, props.room, props.reducedMotion); }
    catch { fallBack(); }
  }, [props.scene, props.room, props.reducedMotion]);

  return <div ref={host} className="scene-3d-room" data-renderer={status} data-space={props.scene === 'backgroundMusic' ? ['personal-room', 'living-room', 'cafe'][Number(props.room)] ?? 'personal-room' : props.scene}>
    {status === 'loading' && <span className="space-loading" role="status">Preparing space…</span>}
  </div>;
}
