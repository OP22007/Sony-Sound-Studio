export interface HistoryBucket { key: string; ms: number }
export interface HistorySegment {
  start: number; end: number; device: string; model?: string;
  mode?: string; noise?: string; category?: string; source?: string;
  title?: string; artist?: string; url?: string;
}
export interface HistorySummary {
  range: HistoryRange; enabled: boolean; details: boolean; exact: boolean; totalMs: number; firstMs: number | null;
  days: { startMs: number; ms: number }[];
  modes: HistoryBucket[]; noise: HistoryBucket[]; devices: HistoryBucket[]; knownDevices: string[];
  categories: HistoryBucket[]; sources: HistoryBucket[]; titles: HistoryBucket[]; urls: HistoryBucket[];
  recent: HistorySegment[]; skipped: number; writeError: string | null; contentSupported: boolean;
}
export interface HistorySettings { enabled: boolean; details: boolean; exact: boolean }
export type HistoryRange = 'today' | '7d' | '30d' | 'all';

export const historyRanges: { id: HistoryRange; label: string; phrase: string }[] = [
  { id: 'today', label: 'Today', phrase: 'today' },
  { id: '7d', label: '7 days', phrase: 'the last 7 days' },
  { id: '30d', label: '30 days', phrase: 'the last 30 days' },
  { id: 'all', label: 'All time', phrase: 'all time' },
];

export function durationParts(ms: number): { hours: number; minutes: number } {
  const total = Math.max(0, Math.round(ms / 60000));
  return { hours: Math.floor(total / 60), minutes: total % 60 };
}

export function formatDuration(ms: number): string {
  if (ms > 0 && ms < 60000) return '<1 min';
  const { hours, minutes } = durationParts(ms);
  if (hours === 0) return `${minutes} min`;
  return minutes === 0 ? `${hours} h` : `${hours} h ${minutes} min`;
}

export function spokenDuration(ms: number): string {
  const { hours, minutes } = durationParts(ms);
  const parts = [hours ? `${hours} ${hours === 1 ? 'hour' : 'hours'}` : '', minutes || !hours ? `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}` : ''];
  return parts.filter(Boolean).join(' ');
}

const modeNames: Record<string, string> = {
  standard: 'Standard', music: 'Music', cinema: 'Cinema', game: 'Game', backgroundMusic: 'Background Music',
  spatial: 'Spatial audio', unknown: 'Not reported',
};
const noiseNames: Record<string, string> = { off: 'Off', cancelling: 'Noise cancelling', ambient: 'Ambient sound' };
const categoryNames: Record<string, string> = { movie: 'Movie', music: 'Music', game: 'Game', other: 'Other' };

function titleCase(key: string): string {
  const spaced = key.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/[-_]+/g, ' ');
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}
export const modeLabel = (key: string) => modeNames[key] ?? titleCase(key);
export const noiseLabel = (key: string) => noiseNames[key] ?? titleCase(key);
export const categoryLabel = (key: string) => categoryNames[key] ?? titleCase(key);

export function sessionTitle(segment: HistorySegment): string {
  const parts = [segment.mode ? modeLabel(segment.mode) : 'Mode not reported'];
  if (segment.noise && segment.noise !== 'off') parts.push(noiseLabel(segment.noise));
  return parts.join(' · ');
}

export function sessionDetail(segment: HistorySegment): string {
  const detail = [segment.category ? categoryLabel(segment.category) : '', segment.source ?? ''].filter(Boolean);
  return detail.join(' · ');
}

export function nowPlaying(segment: HistorySegment): string {
  if (!segment.title) return '';
  return segment.artist ? `${segment.artist} — ${segment.title}` : segment.title;
}

/** Main line of a session: what was playing when known, otherwise the listening mode. */
export function sessionHeadline(segment: HistorySegment): string {
  return nowPlaying(segment) || sessionTitle(segment);
}

/** Second line: the mode when the headline is a title, then device and where it played. */
export function sessionSubline(segment: HistorySegment): string {
  const parts = nowPlaying(segment) ? [sessionTitle(segment), segment.device] : [segment.device];
  return [...parts, sessionDetail(segment)].filter(Boolean).join(' · ');
}
