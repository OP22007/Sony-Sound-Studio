import { useCallback, useEffect, useId, useRef } from 'react';
import { animate, createScope, createTimeline, cubicBezier } from 'animejs';
import Headphones from './Headphones';
import { buildSoundSurface } from './sound-scene';

interface Props {
  model: string;
  noise: string;
  listening: string;
  room: unknown;
  bands: number[];
  preview: boolean;
  reducedMotion: boolean;
  pending: boolean;
}
interface Motion { pause: () => unknown; resume: () => unknown }
const settle = cubicBezier(.22, 1, .36, 1);
const breathe = cubicBezier(.37, 0, .63, 1);
const sceneName = (value: string) => value === 'cinema' ? 'Cinema' : value === 'backgroundMusic' ? 'Background Music' : 'Standard';
const noiseName = (value: string) => value === 'cancelling' ? 'Noise cancelling' : value === 'ambient' ? 'Ambient sound' : value === 'off' ? 'Noise control off' : 'Headphone studio';
const roomSpan = (room: unknown) => room === 2 ? 1.2 : room === 1 ? 1 : .82;

export default function SoundVisualization({ model, noise, listening, room, bands, preview, reducedMotion, pending }: Props) {
  const stage = useRef<HTMLDivElement>(null);
  const surface = useRef<SVGSVGElement>(null);
  const motions = useRef(new Set<Motion>());
  const visible = useRef(true);
  const gradient = useId().replaceAll(':', '');
  const profile = buildSoundSurface(bands, preview);
  const initialPaths = useRef(profile.paths);
  const previousLevels = useRef(profile.levels);
  const previousScene = useRef(listening);
  const previousRoom = useRef(roomSpan(room));
  const bandSignature = `${preview}/${bands.join(',')}`;
  const scene = listening === 'backgroundMusic' || listening === 'cinema' ? listening : 'standard';

  const register = useCallback((animations: Motion[]) => {
    animations.forEach(animation => {
      motions.current.add(animation);
      if (!visible.current || document.hidden) animation.pause();
    });
    return () => animations.forEach(animation => motions.current.delete(animation));
  }, []);

  useEffect(() => {
    const element = stage.current;
    if (!element) return;
    const sync = () => {
      const paused = document.hidden || !visible.current || reducedMotion;
      element.dataset.motion = reducedMotion ? 'reduced' : paused ? 'paused' : 'running';
      motions.current.forEach(animation => paused ? animation.pause() : animation.resume());
    };
    const observer = new IntersectionObserver(([entry]) => { visible.current = entry.isIntersecting; sync(); });
    observer.observe(element);
    document.addEventListener('visibilitychange', sync);
    sync();
    return () => { observer.disconnect(); document.removeEventListener('visibilitychange', sync); };
  }, [reducedMotion]);

  useEffect(() => {
    if (!stage.current || reducedMotion) return;
    let release = () => {};
    const scope = createScope({ root: stage.current }).add(() => {
      const entrance = createTimeline({ defaults: { ease: settle } })
        .add('.scene-entrance', { opacity: [0, 1], y: [18, 0], scale: [.97, 1], duration: 850 }, 0)
        .add('.scene-surface-entrance', { opacity: [0, 1], y: [8, 0], duration: 650 }, 180);
      const floating = animate('.visual-drift', { y: [-2, 3], duration: 5100, alternate: true, loop: true, ease: breathe });
      const shadow = animate('.scene-ground-idle', { scaleX: [.93, 1], opacity: [.5, .65], duration: 5100, alternate: true, loop: true, ease: breathe });
      release = register([entrance, floating, shadow]);
    });
    return () => { release(); scope.revert(); };
  }, [model, reducedMotion, register]);

  useEffect(() => {
    const oldScene = previousScene.current;
    previousScene.current = scene;
    if (!stage.current || reducedMotion) return;
    let release = () => {};
    const scope = createScope({ root: stage.current }).add(() => {
      const cinema = scene === 'cinema';
      const background = scene === 'backgroundMusic';
      const timeline = createTimeline({ defaults: { ease: settle } });
      if (oldScene !== scene) {
        const outgoing = stage.current?.querySelector(`[data-environment="${oldScene}"]`);
        if (outgoing) timeline.add(outgoing, { opacity: [1, 0], duration: 280 }, 0);
      }
      timeline
        .add(`[data-environment="${scene}"]`, {
          opacity: [0, 1], x: [cinema ? -12 : background ? 5 : 0, 0],
          scaleX: [cinema ? .88 : .98, 1], duration: cinema ? 900 : 700,
        }, 0)
        .add('.visual-reaction', { y: [3, cinema ? -2 : 0], scale: [1, cinema ? 1.025 : background ? .985 : 1], duration: 480 }, 0)
        .add('.scene-mode-label', { opacity: [.2, 1], y: [4, 0], duration: 300 }, 80);
      release = register([timeline]);
    });
    return () => { release(); scope.revert(); };
  }, [scene, reducedMotion, register]);

  useEffect(() => {
    const before = previousRoom.current;
    const target = roomSpan(room);
    previousRoom.current = target;
    if (!stage.current || reducedMotion) return;
    let release = () => {};
    const scope = createScope({ root: stage.current }).add(() => {
      const roomTransition = animate('.scene-room-depth', { scaleX: [before, target], duration: 700, ease: settle });
      release = register([roomTransition]);
    });
    return () => { release(); scope.revert(); };
  }, [room, reducedMotion, register]);

  useEffect(() => {
    if (!stage.current || reducedMotion) return;
    let release = () => {};
    const scope = createScope({ root: stage.current }).add(() => {
      const open = noise === 'ambient';
      const quiet = noise === 'cancelling';
      const curtainOpacity = quiet ? .8 : open ? .12 : .38;
      const timeline = createTimeline({ defaults: { ease: settle } })
        .add('.scene-quiet-left', { x: [open ? 0 : -22, open ? -26 : 0], opacity: [.3, curtainOpacity], duration: 650 }, 0)
        .add('.scene-quiet-right', { x: [open ? 0 : 22, open ? 26 : 0], opacity: [.3, curtainOpacity], duration: 650 }, 0)
        .add('.scene-periphery', { scaleX: [open ? .9 : 1.1, open ? 1.08 : 1], opacity: [open ? .05 : .4, open ? .8 : quiet ? .04 : .18], duration: 850 }, 50)
        .add('.scene-noise-label', { opacity: [.25, 1], duration: 260 }, 80);
      const animations: Motion[] = [timeline];
      if (open) animations.push(animate('.scene-air-pane', { x: [-4, 4], duration: 11000, alternate: true, loop: true, ease: breathe }));
      release = register(animations);
    });
    return () => { release(); scope.revert(); };
  }, [noise, reducedMotion, register]);

  useEffect(() => {
    const element = stage.current;
    const nodes = surface.current?.querySelectorAll<SVGPathElement>('[data-surface]');
    if (!element || !nodes) return;
    const previous = previousLevels.current;
    previousLevels.current = profile.levels;
    const delta = profile.levels.map((value, index) => value - previous[index]);
    const strongest = delta.reduce((selected, value, index) => Math.abs(value) > Math.abs(delta[selected]) ? index : selected, 0);
    const magnitude = Math.min(1, Math.abs(delta[strongest]) * 5);
    if (magnitude > .001) element.dataset.eq = ['bass', 'mids', 'treble'][strongest];
    if (reducedMotion) {
      nodes.forEach((node, index) => node.setAttribute('d', profile.paths[index]));
      return;
    }
    const morphs = [...nodes].map((node, index) => animate(node, { d: profile.paths[index], duration: 480, ease: settle }));
    let release = register(morphs);
    let scope: ReturnType<typeof createScope> | undefined;
    if (magnitude > .001) {
      scope = createScope({ root: element }).add(() => {
        const timeline = createTimeline({ defaults: { ease: settle } })
          .add('.scene-eq-highlight', { opacity: [0, .16 + magnitude * .18, 0], scaleX: [.8, 1.08, 1], duration: 680 }, 0)
          .add('.surface-front', { strokeDashoffset: [strongest * -180, strongest * -180 - 150], opacity: [0, .75, 0], duration: 750 }, 0);
        if (strongest === 0) timeline.add('.scene-ground-eq', { scaleX: [1, 1 + magnitude * .1, 1], duration: 480 }, 0);
        const releaseMorphs = release;
        const releaseTimeline = register([timeline]);
        release = () => { releaseMorphs(); releaseTimeline(); };
      });
    }
    return () => { release(); morphs.forEach(animation => animation.cancel()); scope?.revert(); };
    // The signature covers all bands used to construct the normalized surface.
  }, [bandSignature, reducedMotion, register]);

  useEffect(() => {
    if (!pending || reducedMotion || !stage.current) return;
    let release = () => {};
    const scope = createScope({ root: stage.current }).add(() => {
      const waiting = animate('.scene-pending-line', { scaleX: [.25, 1], opacity: [.25, .7], duration: 950, alternate: true, loop: true, ease: breathe });
      release = register([waiting]);
    });
    return () => { release(); scope.revert(); };
  }, [pending, reducedMotion, register]);

  return <div ref={stage} className="sound-visual" data-scene={scene} data-noise={noise} data-listening={listening}
    data-room={String(room ?? 0)} data-eq-available={bands.length === 10}
    data-choreography={`${noise}/${listening}/${room ?? 0}`} data-motion={reducedMotion ? 'reduced' : undefined} data-pending={pending}>
    <div className="scene-environments" aria-hidden="true">
      <div className="scene-environment scene-standard" data-environment="standard"><i className="scene-studio-light"/><i className="scene-studio-seam"/></div>
      <div className="scene-environment scene-background" data-environment="backgroundMusic"><div className="scene-room-depth">
        <i className="scene-back-wall"/><i className="scene-room-wing scene-room-left"/><i className="scene-room-wing scene-room-right"/>
        <i className="scene-room-floor"/><i className="scene-room-window"/><i className="scene-room-spill"/>
      </div></div>
      <div className="scene-environment scene-cinema" data-environment="cinema"><i className="scene-screen"/>
        <i className="scene-cinema-wing scene-cinema-left"/><i className="scene-cinema-wing scene-cinema-right"/><i className="scene-cinema-floor"/>
      </div>
      <div className="scene-periphery"><i className="scene-air-pane scene-air-left"/><i className="scene-air-pane scene-air-right"/></div>
      <div className="scene-quiet-left"/><div className="scene-quiet-right"/><div className="scene-eq-highlight"/>
      <div className="scene-ground-idle"><div className="scene-ground-eq"/></div>
    </div>
    <div className={`visual-product-frame${model.includes('CH720') ? ' visual-product-wide' : ''}`}>
      <div className="scene-entrance"><div className="visual-reaction"><div className="visual-drift"><Headphones model={model}/></div></div></div>
    </div>
    <div className="scene-surface-wrap" aria-label="Equalizer settings surface">
      <div className="scene-surface-entrance">
      <svg ref={surface} className="visual-profile" viewBox="0 0 600 125" aria-hidden="true">
        <defs>
          <linearGradient id={`${gradient}-fill`} x1="0" y1="0" x2="0" y2="1"><stop stopColor="var(--scene-spectrum)" stopOpacity=".14"/><stop offset="1" stopColor="var(--scene-spectrum)" stopOpacity="0"/></linearGradient>
          <linearGradient id={`${gradient}-thread`}><stop stopColor="var(--scene-spectrum)" stopOpacity="0"/><stop offset=".35" stopColor="var(--scene-spectrum)"/><stop offset=".65" stopColor="var(--scene-spectrum)"/><stop offset="1" stopColor="var(--scene-spectrum)" stopOpacity="0"/></linearGradient>
        </defs>
        {initialPaths.current.map((path, index) => <path key={index} data-surface=""
          className={index === 0 ? 'surface-fill' : index === initialPaths.current.length - 1 ? 'surface-front' : 'surface-thread'}
          d={path} fill={index === 0 ? `url(#${gradient}-fill)` : 'none'}
          stroke={index === 0 ? 'none' : `url(#${gradient}-thread)`}
          strokeDasharray={index === initialPaths.current.length - 1 ? '100 600' : undefined}/>)}
      </svg>
      <div className="scene-frequency-labels"><span>BASS</span><span>MIDS</span><span>TREBLE</span></div>
      </div>
    </div>
    <div className="visual-caption"><span className="scene-mode-label">{sceneName(scene)}</span><span className="scene-caption-dot">·</span><span className="scene-noise-label">{pending ? 'WAITING FOR HEADPHONE' : noiseName(noise)}</span></div>
    <span className="scene-pending-line" aria-hidden="true"/>
  </div>;
}
