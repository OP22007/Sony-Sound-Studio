import { useId, useRef } from 'react';
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

export default function EqGraph({ values, labels, min, max, step, zero, disabled, description, formatValue, onChange }: Props) {
  const gradient = useId().replaceAll(':', '');
  const dragStarts = useRef(new Map<number, { index: number; value: number }>());
  const x = (index: number) => WIDTH * (index + 0.5) / values.length;
  const y = (value: number) => BOTTOM - (value - min) * (BOTTOM - TOP) / (max - min);
  const curve = values.map((value, index) => index === 0 ? `M ${x(index)} ${y(value)}` : `C ${(x(index - 1) + x(index)) / 2} ${y(values[index - 1])}, ${(x(index - 1) + x(index)) / 2} ${y(value)}, ${x(index)} ${y(value)}`).join(' ');

  function setFromPointer(index: number, event: PointerEvent<SVGGElement>) {
    const svg = event.currentTarget.ownerSVGElement;
    if (!svg || disabled) return;
    const matrix = svg.getScreenCTM();
    if (!matrix) return;
    const graphY = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse()).y;
    const valueAtPointer = max - (graphY - TOP) * (max - min) / (BOTTOM - TOP);
    onChange(index, quantize(valueAtPointer));
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
      onPointerDown={event => { if (disabled || event.button !== 0) return; event.preventDefault(); event.currentTarget.focus(); dragStarts.current.set(event.pointerId, { index, value }); event.currentTarget.setPointerCapture(event.pointerId); setFromPointer(index, event); }}
      onPointerMove={event => { if (event.currentTarget.hasPointerCapture(event.pointerId)) setFromPointer(index, event); }}
      onPointerUp={event => { dragStarts.current.delete(event.pointerId); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }}
      onPointerCancel={event => { const start = dragStarts.current.get(event.pointerId); if (start) onChange(start.index, start.value); dragStarts.current.delete(event.pointerId); if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId); }}>
      <circle cx={x(index)} cy={y(value)} r="19" className="eq-point-hit" stroke="transparent" strokeWidth="16" vectorEffect="non-scaling-stroke"/>
      <circle cx={x(index)} cy={y(value)} r="9" className="eq-point-halo"/>
      <circle cx={x(index)} cy={y(value)} r="3.5" className="eq-point"/>
    </g>)}
  </svg>;
}
