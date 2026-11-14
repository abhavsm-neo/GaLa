import type { Torrent } from './api';

const GB = 1024 ** 3;
const MB = 1024 ** 2;

export const bytes = (value: number) =>
  value >= GB ? `${(value / GB).toFixed(2)} GB` : value >= MB || !value ? `${(value / MB).toFixed(1)} MB` : `${Math.max(1, Math.round(value / 1024))} KB`;

export const rate = (value: number) => `${(value / MB).toFixed(1)} MB/s`;

export const percent = (t: Torrent) => Math.floor(t.progress * 100);

export function eta(t: Torrent) {
  if (!t.down_rate || !t.total_bytes) return '-';
  const minutes = Math.ceil((t.total_bytes - t.done_bytes) / t.down_rate / 60);
  return minutes > 59 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`;
}

export const isFinished = (t: Torrent) => t.state === 'seeding' || t.state === 'finished';

export function stateLabel(t: Torrent) {
  if (t.holding) return 'Choose files';
  if (t.error) return 'Error';
  if (t.paused) return 'Paused';
  if (t.queued) return 'Queued';
  if (t.state === 'checking') return 'Verifying';
  if (t.state === 'metadata') return 'Finding game info';
  if (t.state === 'writing' || (t.total_bytes > 0 && t.done_bytes >= t.total_bytes && !isFinished(t))) return 'Saving to disk';
  if (t.state === 'seeding') return 'Seeding';
  if (t.state === 'finished') return 'Finished';
  return 'Downloading';
}

export function tint(id: string | number) {
  const hue = typeof id === 'number' ? (id * 67) % 360 : parseInt(id.slice(0, 4), 16) % 360;
  return `hsl(${hue} 45% 42%)`;
}

export function playtime(seconds: number) {
  if (!seconds) return 'Not played';
  if (seconds < 3600) return `${Math.max(1, Math.round(seconds / 60))} min`;
  return `${(seconds / 3600).toFixed(1)} h`;
}

export function ago(when: number) {
  const minutes = Math.max(0, Math.floor((Date.now() / 1000 - when) / 60));
  if (minutes < 60) return minutes <= 1 ? 'Just now' : `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return days === 1 ? 'Yesterday' : `${days} days ago`;
  return new Date(when * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

export function lastPlayed(when: number | null) {
  if (!when) return 'Never';
  const days = Math.floor((new Date().setHours(24, 0, 0, 0) - when * 1000) / 86400000);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  if (days < 7) return `${days} days ago`;
  return new Date(when * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
