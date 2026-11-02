export interface SoundSurface {
  paths: string[];
  levels: number[];
}

const clamp = (value: number) => Math.max(0, Math.min(1, value));
const curve = (points: number[][]) => points.map(([x, y], index) => {
  const n = (value: number) => value.toFixed(2);
  if (!index) return `M${n(x)},${n(y)}`;
  const previous = points[index - 1];
  const middle = (previous[0] + x) / 2;
  return `C${n(middle)},${n(previous[1])} ${n(middle)},${n(y)} ${n(x)},${n(y)}`;
}).join(' ');

/** A deterministic settings surface, never a representation of live audio. */
export function buildSoundSurface(bands: number[], preview: boolean): SoundSurface {
  const values = Array.from({ length: 10 }, (_, index) => {
    const value = bands[index];
    return Number.isFinite(value) ? clamp(preview ? (value + 10) / 20 : value / 12) : .5;
  });
  const average = (start: number, end: number) => values.slice(start, end).reduce((sum, value) => sum + value, 0) / (end - start);
  const levels = [average(0, 3), average(3, 7), average(7, 10)];
  const sample = (position: number) => {
    const index = Math.min(8, Math.floor(position * 9));
    const t = position * 9 - index;
    const eased = t * t * (3 - 2 * t);
    return values[index] * (1 - eased) + values[index + 1] * eased - .5;
  };
  const rows = Array.from({ length: 9 }, (_, row) => Array.from({ length: 29 }, (_, column) => {
    const position = column / 28;
    const width = 390 + row * 21;
    const detail = position > .65 ? Math.sin(position * 62 + row * .18) * (levels[2] - .5) * 3 : 0;
    return [300 + (position - .5) * width, 34 + row * 4.6 - sample(position) * (30 + row * 4) + detail];
  }));
  const contours = rows.map(curve);
  const ribs = [0, 4, 8, 12, 16, 20, 24, 28].map(column => curve(rows.map(row => row[column])));
  const front = contours[8];
  const back = curve([...rows[0]].reverse()).replace(/^M/, 'L');
  return { paths: [`${front} ${back} Z`, ...contours, ...ribs, front], levels };
}
