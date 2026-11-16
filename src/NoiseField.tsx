import { useId } from 'react';
import './noise-field.css';

// An illustration of the confirmed control state, never a microphone or ANC measurement.
export default function NoiseField() {
  const id = useId().replaceAll(':', '');
  const lanes = [0, 1, 2, 3, 4];
  const incoming = (i: number) => {
    const y = 56 + i * 38;
    return `M -12 ${y} C 55 ${y - 20} 112 ${y + 23} ${185 - Math.abs(i - 2) * 6} ${78 + i * 25}`;
  };
  const through = (i: number) => {
    const y = 48 + i * 42;
    return `M -15 ${y} C 82 ${y - 32} 127 ${y + 42} 213 ${92 + i * 17} S 330 ${110 + i * 14} 393 ${92 + i * 17} S 516 ${y - 28} 615 ${y}`;
  };
  return <div className="noise-field" aria-hidden="true">
    <svg viewBox="0 0 600 280" fill="none">
      <defs>
        <linearGradient id={`${id}-incoming`}><stop stopColor="var(--noise-outside)" stopOpacity="0"/><stop offset=".4" stopColor="var(--noise-outside)" stopOpacity=".4"/><stop offset="1" stopColor="var(--noise-boundary)" stopOpacity=".9"/></linearGradient>
        <linearGradient id={`${id}-open`}><stop stopColor="var(--noise-open)" stopOpacity="0"/><stop offset=".23" stopColor="var(--noise-open)" stopOpacity=".65"/><stop offset=".5" stopColor="var(--noise-open)" stopOpacity=".08"/><stop offset=".77" stopColor="var(--noise-open)" stopOpacity=".65"/><stop offset="1" stopColor="var(--noise-open)" stopOpacity="0"/></linearGradient>
        <radialGradient id={`${id}-glass`} cx=".35" cy=".25" r=".8"><stop stopColor="var(--noise-boundary)" stopOpacity=".02"/><stop offset=".72" stopColor="var(--noise-boundary)" stopOpacity=".01"/><stop offset="1" stopColor="var(--noise-boundary)" stopOpacity=".12"/></radialGradient>
        <linearGradient id={`${id}-rim`} x1=".1" y1="0" x2=".9" y2="1"><stop stopColor="var(--noise-boundary)" stopOpacity=".7"/><stop offset=".45" stopColor="var(--noise-boundary)" stopOpacity=".08"/><stop offset=".75" stopColor="var(--noise-boundary)" stopOpacity=".45"/><stop offset="1" stopColor="var(--noise-boundary)" stopOpacity=".1"/></linearGradient>
        <linearGradient id={`${id}-sheet`}><stop stopColor="var(--noise-open)" stopOpacity="0"/><stop offset=".72" stopColor="var(--noise-open)" stopOpacity=".035"/><stop offset=".95" stopColor="var(--noise-open)" stopOpacity=".18"/><stop offset="1" stopColor="var(--noise-open)" stopOpacity=".03"/></linearGradient>
        <linearGradient id={`${id}-outside`}><stop stopColor="var(--noise-outside)" stopOpacity="0"/><stop offset=".8" stopColor="var(--noise-outside)" stopOpacity=".055"/><stop offset="1" stopColor="var(--noise-outside)" stopOpacity=".17"/></linearGradient>
      </defs>
      <g className="noise-treatment noise-cancel">
        <ellipse className="noise-lens" cx="300" cy="138" rx="119" ry="116" fill={`url(#${id}-glass)`} stroke={`url(#${id}-rim)`}/>
        <path className="noise-lens-bevel" fill={`url(#${id}-rim)`} fillRule="evenodd" d="M 300 22 a 119 116 0 1 0 0 232 a 119 116 0 1 0 0 -232 M 300 27 a 114 111 0 1 0 0 222 a 114 111 0 1 0 0 -222"/>
        <path className="noise-shell-seam" d="M 294 22 C 219 36 192 92 192 143 S 230 235 294 254 M 306 22 C 381 36 408 92 408 143 S 370 235 306 254" stroke={`url(#${id}-rim)`}/>
        <ellipse className="noise-ground" cx="300" cy="249" rx="125" ry="15" stroke={`url(#${id}-rim)`}/>
        {[false, true].map(mirror => <g key={String(mirror)} transform={mirror ? 'translate(600 0) scale(-1 1)' : undefined}>
          {[0, 1, 2].map(i => <path key={`volume-${i}`} className="noise-absorb-sheet" style={{ '--sheet': i } as React.CSSProperties}
            d={`M ${64 + i * 38} 32 Q ${110 + i * 30} 139 ${64 + i * 38} 244 Q ${139 + i * 15} 139 ${64 + i * 38} 32 Z`} fill={`url(#${id}-outside)`}/>) }
          {lanes.map(i => <g key={i} style={{ '--lane': i } as React.CSSProperties}>
            <path className="noise-trace" d={incoming(i)} stroke={`url(#${id}-incoming)`}/>
            <path className="noise-arrival" d={incoming(i)} pathLength="100" stroke={`url(#${id}-incoming)`}/>
            <path className="noise-counter" d={`M 213 ${84 + i * 22} Q 204 ${89 + i * 23} ${185 - Math.abs(i - 2) * 6} ${78 + i * 25}`} pathLength="100"/>
            <circle className="noise-intercept" cx={185 - Math.abs(i - 2) * 6} cy={78 + i * 25} r="2"/>
          </g>)}
        </g>)}
      </g>
      <g className="noise-treatment noise-ambient">
        <path className="noise-open-edge" d="M 202 41 C 163 78 157 185 201 233 M 398 41 C 437 78 443 185 399 233" stroke={`url(#${id}-rim)`}/>
        {[false, true].map(mirror => <g key={String(mirror)} transform={mirror ? 'translate(600 0) scale(-1 1)' : undefined}>
          {[0, 1, 2].map(i => <path key={i} className="noise-open-sheet" style={{ '--sheet': i } as React.CSSProperties}
            d={`M ${65 + i * 55} 28 C ${127 + i * 42} 65 ${137 + i * 39} 205 ${65 + i * 55} 246 C ${157 + i * 29} 219 ${158 + i * 33} 64 ${65 + i * 55} 28 Z`} fill={`url(#${id}-sheet)`}/>) }
        </g>)}
        {lanes.map(i => <g key={i} style={{ '--lane': i } as React.CSSProperties}>
          <path className="noise-through-trace" d={through(i)} stroke={`url(#${id}-open)`}/>
          <path className="noise-through" d={through(i)} pathLength="100" stroke={`url(#${id}-open)`}/>
        </g>)}
        <path className="noise-floor-open" d="M 70 228 Q 300 298 530 228 M 110 234 Q 300 282 490 234" stroke={`url(#${id}-open)`}/>
      </g>
      <g className="noise-treatment noise-passive">
        {[0, 1, 2].map(i => <path key={i} d={`M -10 ${89 + i * 45} C 110 ${64 + i * 45} 172 ${120 + i * 36} 300 ${108 + i * 30} S 490 ${71 + i * 45} 610 ${93 + i * 45}`} stroke={`url(#${id}-open)`}/>) }
      </g>
    </svg>
  </div>;
}
