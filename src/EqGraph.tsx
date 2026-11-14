import { useEffect, useId, useRef } from 'react';
import type { PointerEvent, KeyboardEvent } from 'react';

interface Props {
  values: number[];
  labels: string[];
  min: number;
  max: number;
  step: number;
  /** Raw value that means 0 dB, for devices whose steps are offset from decibels. */
  zero?: number;
  disabled: boolean;
  description: string;
  formatValue: (value: number) => string;
  onChange: (index: number, value: number) => void;
}

const WIDTH = 720;
const TOP = 40;
const BOTTOM = 140;
/** How far past a step boundary the pointer must travel before the point moves. */
const DETENT = 0.65;

interface Drag { index: number; start: number; offset: number; target: number; value: number; released: boolean; frame: number | null }

function tick() {
  // A short haptic pulse where the platform has one (phones and tablets); silently ignored elsewhere.
  try { if (typeof navigator !== 'undefined' && 'vibrate' in navigator) navigator.vibrate(4); } catch { /* optional */ }
}

export default function EqGraph({ values, labels, min, max, step, zero, disabled, description, formatValue, onChange }: Props) {
  const gradient = useId().replaceAll(':', '');
  const drags = useRef(new Map<number, Drag>());
  const latest = useRef({ values, onChange, min, max, step });
  latest.current = { values, onChange, min, max, step };
  useEffect(() => () => { drags.current.forEach(drag => { if (drag.frame !== null) cancelAnimationFrame(drag.frame); }); drags.current.clear(); }, []);
  const x = (index: number) => WIDTH * (index + 0.5) / values.length;
  const y = (value: number) => BOTTOM - (value - min) * (BOTTOM - TOP) / (max - min);
  const curve = values.map((value, index) => index === 0 ? `M ${x(index)} ${y(value)}` : `C ${(x(index - 1) + x(index)) / 2} ${y(values[index - 1])}, ${(x(index - 1) + x(index)) / 2} ${y(value)}, ${x(index)} ${y(value)}`).join(' ');

  function pointerValue(event: PointerEvent<SVGGElement>) {
    const svg = event.currentTarget.ownerSVGElement;
    const matrix = svg?.getScreenCTM();
    if (!svg || !matrix) return undefined;
    const graphY = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse()).y;
    return max - (graphY - TOP) * (max - min) / (BOTTOM - TOP);
  }

  /** Moves the point one step per frame toward the pointer, so a quick drag never jumps several steps at once. */
  function advance(pointerId: number) {
    const drag = drags.current.get(pointerId);
    if (!drag) return;
    drag.frame = null;
    const { onChange: emit, min: lo, max: hi, step: unit } = latest.current;
    const distance = (drag.target - drag.value) / unit;
    // Held points need to pass a detent; once released they settle on the nearest step.
    const threshold = drag.released ? 0.5 : DETENT;
    if (Math.abs(distance) >= threshold) {
      const next = Math.min(hi, Math.max(lo, Number((drag.value + Math.sign(distance) * unit).toFixed(6))));
      if (next !== drag.value) { drag.value = next; emit(drag.index, next); tick(); }
      if (Math.abs((drag.target - drag.value) / unit) >= threshold && next !== (distance > 0 ? hi : lo)) {
        drag.frame = requestAnimationFrame(() => advance(pointerId));
        return;
      }
    }
    if (drag.released) drags.current.delete(pointerId);
  }

  function schedule(pointerId: number) {
    const drag = drags.current.get(pointerId);
    if (drag && drag.frame === null) drag.frame = requestAnimationFrame(() => advance(pointerId));
  }

  function handleKey(index: number, event: KeyboardEvent<SVGGElement>) {
    if (disabled) return;
    const value = values[index];
    let next: number | undefined;
    if (event.key === 'ArrowUp' || event.key === 'ArrowRight') next = value + step;
    if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') next = value - step;
    if (event.key === 'Home') next = min;
    if (event.key === 'End') next = max;
    if (next !== undefined) { event.preventDefault(); onChange(index, quantize(next)); }
  }

  function quantize(value: number) {
    const bounded = Math.min(max, Math.max(min, value));
    return Math.min(max, Math.max(min, Number((min + Math.round((bounded - min) / step) * step).toFixed(6))));
  }

  return <svg className="eq-curve" viewBox="0 0 720 175" role="group" aria-label={description}>
    <defs><linearGradient id={gradient} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="var(--accent)" stopOpacity=".18"/><stop offset="100%" stopColor="var(--accent)" stopOpacity="0"/></linearGradient></defs>
    {(zero === undefined ? [max, (min + max) / 2, min] : [max, zero, min]).filter((value, index, all) => value >= min && value <= max && all.indexOf(value) === index).map(value => {
      // Devices whose raw steps are offset from decibels name the zero step, so the axis reads in dB.
      const shown = value - (zero ?? 0);
      return <g key={value}>
        <line x1="26" y1={y(value)} x2="698" y2={y(value)} className={shown === 0 ? 'eq-zero' : 'eq-grid'}/>
        <text x="2" y={y(value) + 4}>{shown > 0 ? `+${shown}` : shown}</text>
      </g>;
    })}
    {values.length > 0 && <path d={`${curve} L ${x(values.length - 1)} ${BOTTOM + 10} L ${x(0)} ${BOTTOM + 10} Z`} fill={`url(#${gradient})`} aria-hidden="true"/>}
    <path className="frequency-path" d={curve} fill="none"/>
    {values.map((value, index) => <g key={`${labels[index]}-${index}`} className={`eq-graph-handle${disabled ? ' is-disabled' : ''}`} role="slider" tabIndex={disabled ? -1 : 0}
      aria-label={labels[index]} aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} aria-valuetext={formatValue(value)} aria-disabled={disabled} aria-orientation="vertical"
      onKeyDown={event => handleKey(index, event)}
      onPointerDown={event => {
        if (disabled || event.button !== 0) return;
        event.preventDefault(); event.currentTarget.focus();
        const at = pointerValue(event);
        // Grabbing a point never moves it: remember where on the point the pointer landed.
        drags.current.set(event.pointerId, { index, start: value, offset: at === undefined ? 0 : at - value, target: value, value, released: false, frame: null });
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={event => {
        const drag = drags.current.get(event.pointerId);
        if (!drag || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
        const at = pointerValue(event);
        if (at === undefined) return;
        drag.target = Math.min(max, Math.max(min, at - drag.offset));
        schedule(event.pointerId);
      }}
      onPointerUp={event => {
        const drag = drags.current.get(event.pointerId);
        if (drag) { drag.released = true; schedule(event.pointerId); }
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      onPointerCancel={event => {
        const drag = drags.current.get(event.pointerId);
        if (drag) { if (drag.frame !== null) cancelAnimationFrame(drag.frame); drags.current.delete(event.pointerId); onChange(drag.index, drag.start); }
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
      }}>
      <circle cx={x(index)} cy={y(value)} r="19" className="eq-point-hit" stroke="transparent" strokeWidth="16" vectorEffect="non-scaling-stroke"/>
      <circle cx={x(index)} cy={y(value)} r="9" className="eq-point-halo"/>
      <circle cx={x(index)} cy={y(value)} r="3.5" className="eq-point"/>
    </g>)}
  </svg>;
}
